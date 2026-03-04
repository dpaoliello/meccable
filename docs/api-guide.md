# meccable — API Guide

`meccable` is a Rust library (and companion CLI tool) for controlling **Meccanoid** robots over **Bluetooth Low Energy (BLE)**. It has been tested with the Meccanoid G15KS and should also work with the Meccanoid G15 (which lacks elbow and neck servos).

The library is built on top of [`btleplug`](https://crates.io/crates/btleplug) for BLE communication and [`tokio`](https://crates.io/crates/tokio) for async I/O.

---

## Quick start

Add `meccable` to your `Cargo.toml`:

```toml
[dependencies]
meccable = { path = "../meccable" }   # or publish/use a git dependency
tokio = { version = "1", features = ["full"] }
anyhow = "1"
```

Connect and drive the robot:

```rust
use meccable::{Meccanoid, Servo, ServoColor, ChestLight, WheelDirection};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Connect using the robot's Bluetooth MAC address
    let mut mec = Meccanoid::connect("c4:be:84:d4:68:1b").await?;

    // Move a servo
    mec.servo(Servo::LeftElbow, 128).await?;

    // Change a servo LED colour
    mec.servo_light(Servo::LeftElbow, ServoColor::Cyan).await?;

    // Set eye colour (RGB, each 0–7)
    mec.eye_lights(0, 0, 7).await?;

    // Toggle a chest light
    mec.chest_light(ChestLight::Blue, true).await?;

    // Drive forward for 1 second
    mec.move_wheels(WheelDirection::Forward, 128, WheelDirection::Forward, 128).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    mec.move_wheels(WheelDirection::Forward, 0, WheelDirection::Forward, 0).await?;

    // Play the "I'm awake" animation
    mec.wake_up().await?;

    // Disconnect cleanly
    mec.disconnect().await?;
    Ok(())
}
```

---

## Public types

### `Meccanoid`

The main controller struct. It holds the BLE connection and the current state of servos and lights so that each command sends a complete, consistent frame to the robot.

| Method | Signature | Description |
|---|---|---|
| `connect` | `async fn connect(address: &str) -> Result<Self>` | Scans for BLE devices, connects to the Meccanoid at the given MAC address, discovers services, and initialises the robot (zeroes the wheels, resets servos to default positions, sets eye lights to blue). |
| `disconnect` | `async fn disconnect(self) -> Result<()>` | Cleanly disconnects from the robot. Consumes `self`. |
| `servo` | `async fn servo(&mut self, servo: Servo, value: u8) -> Result<()>` | Sets a servo to the given position (0–255). The right-elbow value is automatically inverted to account for its reversed mounting. |
| `servo_light` | `async fn servo_light(&mut self, servo: Servo, color: ServoColor) -> Result<()>` | Sets the LED embedded in a servo to the given colour. |
| `chest_light` | `async fn chest_light(&mut self, light: ChestLight, on: bool) -> Result<()>` | Turns an individual chest light on or off. |
| `eye_lights` | `async fn eye_lights(&mut self, r: u8, g: u8, b: u8) -> Result<()>` | Sets the eye colour. Each RGB channel is 0–7 (values above 7 are clamped). |
| `move_wheels` | `async fn move_wheels(&mut self, right_dir: WheelDirection, right_speed: u8, left_dir: WheelDirection, left_speed: u8) -> Result<()>` | Drives the wheels. Each wheel has an independent direction and speed (0–255). Send speed `0` to stop. |
| `wake_up` | `async fn wake_up(&mut self) -> Result<()>` | Plays the built-in "I'm awake" animation (voice line + arm waggle). |

### `Servo`

Identifies one of the robot's eight servos. Can be parsed from a case-insensitive string abbreviation via `FromStr` (powered by `strum`).

| Variant | String alias | Location |
|---|---|---|
| `RightElbow` | `re` | Right arm elbow |
| `RightShoulderPitch` | `rsp` | Right shoulder (up/down) |
| `RightShoulderRoll` | `rsr` | Right shoulder (in/out) |
| `LeftShoulderRoll` | `lsr` | Left shoulder (in/out) |
| `LeftShoulderPitch` | `lsp` | Left shoulder (up/down) |
| `LeftElbow` | `le` | Left arm elbow |
| `NeckYaw` | `ny` | Neck horizontal rotation |
| `NeckRoll` | `nr` | Neck tilt |

### `ServoColor`

Colour for the LED ring inside each servo. Parsed case-insensitively.

Variants: `Black`, `Red`, `Green`, `Yellow`, `Blue`, `Magenta`, `Cyan`, `White`.

### `ChestLight`

Identifies one of the four chest LEDs. Parsed case-insensitively.

Variants: `Blue`, `Red`, `Green`, `Yellow`.

### `WheelDirection`

Direction for a wheel motor. Parsed case-insensitively—also accepts single-letter shortcuts.

| Variant | String alias |
|---|---|
| `Forward` | `f` |
| `Backward` | `b` |

---

## CLI tool

The crate also ships a binary (`main.rs`) that provides an interactive REPL for controlling the robot from the terminal.

```
cargo run -- <MAC_ADDRESS>
```

Once connected, enter one of the following commands at the prompt:

| Command | Alias | Description |
|---|---|---|
| `servomove` | `sm` | Set a servo position (prompts for servo + value 0–255) |
| `servocolor` | `sc` | Set a servo LED colour (prompts for servo + colour) |
| `eyecolor` | `ec` | Set eye colour (prompts for R, G, B each 0–7) |
| `chestlight` | `cl` | Toggle a chest light (prompts for light + on/off) |
| `move` | — | Drive wheels (prompts for direction, speed, and duration) |
| `awake` | — | Play the "I'm awake" animation |
| `quit` | — | Disconnect and exit |

---

## Protocol notes

Every command sent to the robot is a **20-byte BLE write** (without response) to characteristic UUID `0000ffe9-0000-1000-8000-00805f9b34fb`. The frame layout is:

| Byte | Content |
|---|---|
| 0 | Command ID |
| 1–17 | Payload (17 bytes) |
| 18–19 | Big-endian checksum (sum of bytes 0–17) |

The library manages this framing internally; callers only interact with the high-level methods above.

---

## Requirements

- A Bluetooth LE adapter supported by `btleplug`.
- Rust edition **2024** (nightly — see `rust-toolchain.toml`).
- Async runtime: **Tokio** (the `full` feature set is used).
