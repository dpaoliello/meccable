//! **meccamote** — Drive a Meccanoid's wheels with a Wiimote Nunchuck joystick.
//!
//! Usage: `meccamote <MECCANOID_MAC> [OPTIONS]`
//!
//! Pair the Wiimote by pressing **1+2**, with the Nunchuck plugged in.
//! Hold the **Z button** (dead-man switch) while steering with the joystick.
//! Press **Home** or **Ctrl+C** to stop and exit.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use meccable::{Meccanoid, WheelDirection};
use wiimote_rs::extensions::WiimoteExtension;
use wiimote_rs::input::{ButtonData, InputReport};
use wiimote_rs::output::{DataReporingMode, OutputReport};
use wiimote_rs::prelude::*;

// ── CLI ─────────────────────────────────────────────────────────────────────

/// Drive a Meccanoid robot with a Wiimote Nunchuck joystick.
#[derive(Parser)]
#[command(name = "meccamote")]
struct Cli {
    /// Bluetooth MAC address of the Meccanoid (e.g. "c4:be:84:d4:68:1b")
    address: String,

    /// Joystick dead-zone radius around centre (0–127)
    #[arg(long, default_value_t = 15)]
    dead_zone: u8,

    /// Maximum wheel speed cap (0–255)
    #[arg(long, default_value_t = 255)]
    max_speed: u8,

    /// Milliseconds between wheel-command updates
    #[arg(long, default_value_t = 50)]
    update_rate: u64,
}

// ── Nunchuck state ──────────────────────────────────────────────────────────

/// Snapshot of nunchuck controller input.
#[derive(Clone, Copy, Debug)]
struct NunchuckState {
    /// Joystick X axis (0–255, ~128 = centre)
    joy_x: u8,
    /// Joystick Y axis (0–255, ~128 = centre)
    joy_y: u8,
    /// Z button (trigger on underside) is currently pressed
    z_button: bool,
    /// C button is currently pressed
    c_button: bool,
    /// Home button on the Wiimote itself
    home_button: bool,
}

impl Default for NunchuckState {
    fn default() -> Self {
        Self {
            joy_x: 128,
            joy_y: 128,
            z_button: false,
            c_button: false,
            home_button: false,
        }
    }
}

/// Parse 6 raw nunchuck extension bytes into joystick + button state.
///
/// Byte layout (unencrypted mode, per WiiBrew):
///   [0] Joystick X
///   [1] Joystick Y
///   [2] Accelerometer X high bits
///   [3] Accelerometer Y high bits
///   [4] Accelerometer Z high bits
///   [5] Accel low bits | Z (bit 0, active-low) | C (bit 1, active-low)
fn parse_nunchuck_bytes(bytes: &[u8]) -> Option<NunchuckState> {
    if bytes.len() < 6 {
        return None;
    }
    Some(NunchuckState {
        joy_x: bytes[0],
        joy_y: bytes[1],
        z_button: (bytes[5] & 0x01) == 0,
        c_button: (bytes[5] & 0x02) == 0,
        home_button: false, // set separately from Wiimote buttons
    })
}

// ── Joystick → wheel mapping ────────────────────────────────────────────────

/// Computed differential-drive wheel command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WheelCommand {
    left_dir: WheelDirection,
    left_speed: u8,
    right_dir: WheelDirection,
    right_speed: u8,
}

impl WheelCommand {
    const STOP: Self = Self {
        left_dir: WheelDirection::Forward,
        left_speed: 0,
        right_dir: WheelDirection::Forward,
        right_speed: 0,
    };
}

/// Convert a nunchuck joystick position to differential-drive wheel commands.
///
/// Uses **arcade-drive** mixing:
///   - Y axis → forward / backward thrust
///   - X axis → turning (added to left wheel, subtracted from right wheel)
fn map_joystick(state: &NunchuckState, dead_zone: u8, max_speed: u8) -> WheelCommand {
    let raw_x = state.joy_x as i16 - 128;
    let raw_y = state.joy_y as i16 - 128;
    let dz = dead_zone as i16;

    // Apply dead zone
    let thrust = if raw_y.abs() <= dz { 0i16 } else { raw_y };
    let turn = if raw_x.abs() <= dz { 0i16 } else { raw_x };

    if thrust == 0 && turn == 0 {
        return WheelCommand::STOP;
    }

    let scale = max_speed as f32 / 128.0;
    let max = max_speed as f32;

    let left_f = ((thrust + turn) as f32 * scale).clamp(-max, max);
    let right_f = ((thrust - turn) as f32 * scale).clamp(-max, max);

    fn split(power: f32) -> (WheelDirection, u8) {
        if power >= 0.0 {
            (WheelDirection::Forward, power as u8)
        } else {
            (WheelDirection::Backward, (-power) as u8)
        }
    }

    let (left_dir, left_speed) = split(left_f);
    let (right_dir, right_speed) = split(right_f);

    WheelCommand {
        left_dir,
        left_speed,
        right_dir,
        right_speed,
    }
}

// ── Wiimote reader thread ───────────────────────────────────────────────────

/// Data report mode 0x32: Core Buttons (2 bytes) + 8 Extension bytes.
/// Nunchuck data occupies the first 6 extension bytes (data[2..8]).
const REPORT_MODE_BUTTONS_EXT8: u8 = 0x32;

/// Runs in a dedicated OS thread (wiimote-rs is synchronous).
///
/// Reads Wiimote+Nunchuck input and publishes the latest [`NunchuckState`]
/// through the shared `Mutex`. Sets `should_exit` when the Wiimote Home
/// button is pressed.
fn wiimote_reader(state: Arc<Mutex<NunchuckState>>, should_exit: Arc<AtomicBool>) {
    let manager = WiimoteManager::get_instance();
    let new_devices = {
        let manager = manager.lock().unwrap();
        manager.new_devices_receiver()
    };

    println!("Press 1+2 on your Wiimote to connect...");

    // Each iteration handles one Wiimote session. When it disconnects we loop
    // back and wait for the next device (supports reconnection).
    for device in new_devices.iter() {
        if should_exit.load(Ordering::Relaxed) {
            break;
        }

        println!("Wiimote found!");

        // ── Detect nunchuck extension ───────────────────────────────────
        {
            let dev = device.lock().unwrap();

            // The manager may have detected the extension at connect time.
            let has_nunchuck = match dev.extension() {
                Some(WiimoteExtension::Nunchuck) => {
                    println!("Nunchuck detected.");
                    true
                }
                Some(other) => {
                    eprintln!("Extension detected but not a Nunchuck: {other:?}");
                    false
                }
                None => {
                    // Try manual detection (init registers + read identifier).
                    match WiimoteExtension::detect(&dev) {
                        Ok(Some(WiimoteExtension::Nunchuck)) => {
                            println!("Nunchuck detected (manual init).");
                            true
                        }
                        Ok(other) => {
                            eprintln!(
                                "No Nunchuck found (got {other:?}). \
                                 Is the Nunchuck plugged in?"
                            );
                            false
                        }
                        Err(e) => {
                            eprintln!("Extension detection failed: {e:?}");
                            false
                        }
                    }
                }
            };

            if !has_nunchuck {
                eprintln!("Skipping this Wiimote — plug in a Nunchuck and press 1+2 again.");
                continue;
            }
        }

        // ── Set data report mode ────────────────────────────────────────
        {
            let dev = device.lock().unwrap();
            let mode = OutputReport::DataReportingMode(DataReporingMode {
                continuous: true,
                mode: REPORT_MODE_BUTTONS_EXT8,
            });
            if let Err(e) = dev.write(&mode) {
                eprintln!("Failed to set report mode: {e:?}");
                continue;
            }
        }

        println!("Reading nunchuck input. Hold Z to drive, Home to exit.");

        // ── Read loop ───────────────────────────────────────────────────
        loop {
            if should_exit.load(Ordering::Relaxed) {
                return;
            }

            let report = {
                let dev = device.lock().unwrap();
                dev.read_timeout(200) // 200 ms timeout so we can check should_exit
            };

            match report {
                Ok(InputReport::DataReport(_, data)) => {
                    let buttons = data.buttons();

                    // Home → signal exit
                    if buttons.contains(ButtonData::HOME) {
                        println!("Home button pressed — exiting.");
                        should_exit.store(true, Ordering::Relaxed);
                        *state.lock().unwrap() = NunchuckState::default();
                        return;
                    }

                    // Parse the 6 nunchuck bytes (extension bytes start at
                    // index 2 for report mode 0x32).
                    if let Some(mut nunchuck) = parse_nunchuck_bytes(&data.data[2..8]) {
                        nunchuck.home_button = buttons.contains(ButtonData::HOME);
                        *state.lock().unwrap() = nunchuck;
                    }
                }
                Ok(_) => { /* status, acknowledge, memory read — ignore */ }
                Err(WiimoteError::Disconnected) => {
                    eprintln!("Wiimote disconnected. Waiting for reconnection...");
                    *state.lock().unwrap() = NunchuckState::default();
                    break; // back to the outer loop to wait for a new device
                }
                Err(_) => {
                    // Timeout or transient parse error — keep reading.
                }
            }
        }
    }
}

// ── Async main ──────────────────────────────────────────────────────────────

async fn run(cli: Cli) -> Result<()> {
    println!("Nunchuck Drive — Meccanoid Wheel Controller");
    println!("Connecting to Meccanoid at {}...", cli.address);

    let mut mec = Meccanoid::connect(&cli.address)
        .await
        .context("Connecting to Meccanoid")?;
    println!("Meccanoid connected!");

    // Shared state between the wiimote reader thread and the async main loop.
    let nunchuck_state = Arc::new(Mutex::new(NunchuckState::default()));
    let should_exit = Arc::new(AtomicBool::new(false));

    // Ctrl+C handler — triggers the same graceful shutdown as Home.
    let exit_for_ctrlc = should_exit.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        println!("\nCtrl+C received — shutting down.");
        exit_for_ctrlc.store(true, Ordering::Relaxed);
    });

    // Spawn wiimote reader in a dedicated OS thread (sync ↔ async bridge).
    let state_for_reader = nunchuck_state.clone();
    let exit_for_reader = should_exit.clone();
    std::thread::Builder::new()
        .name("wiimote-reader".into())
        .spawn(move || wiimote_reader(state_for_reader, exit_for_reader))
        .context("Spawning wiimote reader thread")?;

    let update_interval = Duration::from_millis(cli.update_rate);
    let resend_cycles: u32 = 10; // re-send unchanged commands every N cycles
    let mut prev_command = WheelCommand::STOP;
    let mut cycles_since_send: u32 = 0;
    let mut prev_c_pressed = false;
    let mut eye_toggle = false;

    println!("Waiting for Wiimote... Press 1+2 to pair.");
    println!("Hold Z to drive. Press Home or Ctrl+C to exit.\n");

    // ── Control loop ────────────────────────────────────────────────────
    loop {
        if should_exit.load(Ordering::Relaxed) {
            break;
        }

        let state = *nunchuck_state.lock().unwrap();

        // Z button = dead-man switch: must hold to drive.
        let command = if state.z_button {
            map_joystick(&state, cli.dead_zone, cli.max_speed)
        } else {
            WheelCommand::STOP
        };

        // C button rising edge → toggle eye colour (visual feedback).
        if state.c_button && !prev_c_pressed {
            eye_toggle = !eye_toggle;
            let (r, g, b) = if eye_toggle { (7, 0, 0) } else { (0, 0, 7) };
            if let Err(e) = mec.eye_lights(r, g, b).await {
                eprintln!("Eye lights error: {e}");
            }
        }
        prev_c_pressed = state.c_button;

        // Send wheel command on change or periodically for reliability.
        if command != prev_command || cycles_since_send >= resend_cycles {
            if let Err(e) = mec
                .move_wheels(
                    command.right_dir,
                    command.right_speed,
                    command.left_dir,
                    command.left_speed,
                )
                .await
            {
                eprintln!("Wheel command error: {e}");
            }
            prev_command = command;
            cycles_since_send = 0;
        } else {
            cycles_since_send += 1;
        }

        tokio::time::sleep(update_interval).await;
    }

    // ── Graceful shutdown ───────────────────────────────────────────────
    println!("Stopping wheels...");
    mec.move_wheels(WheelDirection::Forward, 0, WheelDirection::Forward, 0)
        .await
        .context("Stopping wheels")?;

    println!("Disconnecting from Meccanoid...");
    mec.disconnect().await.context("Disconnecting")?;
    println!("Done!");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        eprintln!("Error: {e:?}");
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a NunchuckState with the given joystick position.
    fn joy(x: u8, y: u8) -> NunchuckState {
        NunchuckState {
            joy_x: x,
            joy_y: y,
            z_button: true,
            c_button: false,
            home_button: false,
        }
    }

    #[test]
    fn centre_maps_to_stop() {
        let cmd = map_joystick(&joy(128, 128), 15, 255);
        assert_eq!(cmd, WheelCommand::STOP);
    }

    #[test]
    fn inside_dead_zone_maps_to_stop() {
        // Slightly off-centre but within dead zone
        let cmd = map_joystick(&joy(130, 125), 15, 255);
        assert_eq!(cmd, WheelCommand::STOP);
    }

    #[test]
    fn full_forward() {
        let cmd = map_joystick(&joy(128, 255), 15, 255);
        assert_eq!(cmd.left_dir, WheelDirection::Forward);
        assert_eq!(cmd.right_dir, WheelDirection::Forward);
        assert!(cmd.left_speed > 200);
        assert!(cmd.right_speed > 200);
        // Both wheels should be roughly equal (straight line)
        assert_eq!(cmd.left_speed, cmd.right_speed);
    }

    #[test]
    fn full_backward() {
        let cmd = map_joystick(&joy(128, 0), 15, 255);
        assert_eq!(cmd.left_dir, WheelDirection::Backward);
        assert_eq!(cmd.right_dir, WheelDirection::Backward);
        assert!(cmd.left_speed > 200);
        assert!(cmd.right_speed > 200);
    }

    #[test]
    fn full_right_turns_in_place() {
        let cmd = map_joystick(&joy(255, 128), 15, 255);
        // Turning right: left wheel forward, right wheel backward
        assert_eq!(cmd.left_dir, WheelDirection::Forward);
        assert_eq!(cmd.right_dir, WheelDirection::Backward);
        assert!(cmd.left_speed > 200);
        assert!(cmd.right_speed > 200);
    }

    #[test]
    fn full_left_turns_in_place() {
        let cmd = map_joystick(&joy(0, 128), 15, 255);
        // Turning left: left wheel backward, right wheel forward
        assert_eq!(cmd.left_dir, WheelDirection::Backward);
        assert_eq!(cmd.right_dir, WheelDirection::Forward);
        assert!(cmd.left_speed > 200);
        assert!(cmd.right_speed > 200);
    }

    #[test]
    fn max_speed_caps_output() {
        let cmd = map_joystick(&joy(128, 255), 15, 100);
        assert!(cmd.left_speed <= 100);
        assert!(cmd.right_speed <= 100);
    }

    #[test]
    fn nunchuck_bytes_centre() {
        // Typical centre values: X=128, Y=128, accel don't matter,
        // byte 5 = 0xFF (both buttons released, active-low)
        let bytes = [128, 128, 0, 0, 0, 0xFF];
        let state = parse_nunchuck_bytes(&bytes).unwrap();
        assert_eq!(state.joy_x, 128);
        assert_eq!(state.joy_y, 128);
        assert!(!state.z_button);
        assert!(!state.c_button);
    }

    #[test]
    fn nunchuck_bytes_z_pressed() {
        // Z bit 0 active-low: 0xFE means Z pressed, C released
        let bytes = [128, 128, 0, 0, 0, 0xFE];
        let state = parse_nunchuck_bytes(&bytes).unwrap();
        assert!(state.z_button);
        assert!(!state.c_button);
    }

    #[test]
    fn nunchuck_bytes_both_buttons_pressed() {
        // Both active-low bits cleared: 0xFC
        let bytes = [128, 128, 0, 0, 0, 0xFC];
        let state = parse_nunchuck_bytes(&bytes).unwrap();
        assert!(state.z_button);
        assert!(state.c_button);
    }

    #[test]
    fn nunchuck_bytes_too_short_returns_none() {
        let bytes = [128, 128, 0];
        assert!(parse_nunchuck_bytes(&bytes).is_none());
    }
}
