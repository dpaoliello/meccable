# Nunchuck-to-Meccanoid Wheel-Drive CLI — Plan

## Goal

A new binary (`nunchuck-drive`) that continuously reads the nunchuck joystick from a paired Wiimote and sends differential wheel commands to a Meccanoid robot in real time.

---

## Architecture

```
Wiimote+Nunchuck ──(Bluetooth HID)──► nunchuck-drive ──(BLE)──► Meccanoid
                                       ┌─────────────┐
                                       │ Wiimote read │ (sync, wiimote-rs)
                                       │      ↓       │
                                       │  Joystick    │
                                       │  mapping     │
                                       │      ↓       │
                                       │ Meccanoid    │ (async, btleplug)
                                       │  write       │
                                       └─────────────┘
```

---

## Steps

### 1. Add dependencies

In `Cargo.toml`, add:

- **`wiimote-rs`** — Wiimote Bluetooth HID input (recommended by the research doc; MIT, Windows-native, has extension framework).
- **`clap`** — CLI argument parsing (MAC address for the Meccanoid, optional dead-zone/speed tuning flags).

### 2. Create the new binary

Add a new file at `src/bin/nunchuck_drive.rs`. Using `src/bin/` keeps the existing `main.rs` REPL binary intact and lets Cargo auto-discover the new binary.

### 3. Implement Wiimote/nunchuck reading

Using `wiimote-rs`:

1. Create a `WiimoteManager`, listen for new devices (user presses 1+2 to pair).
2. Once a Wiimote is found, set the data report mode to one that includes extension bytes (e.g., report mode `0x32`: Core Buttons + 8 Extension Bytes).
3. On each input report, extract the 6-byte nunchuck extension payload and parse:
   - **Byte 0** → Joystick X (0–255, ~128 center)
   - **Byte 1** → Joystick Y (0–255, ~128 center)
   - **Bit 0 of byte 5** → Z button (active low)
   - **Bit 1 of byte 5** → C button (active low)

If `wiimote-rs`'s `extensions` module already exposes parsed nunchuck data, use that directly. If not, parse the raw extension bytes manually — the format is trivial and well-documented on [WiiBrew](https://wiibrew.org/wiki/Wiimote/Extension_Controllers/Nunchuck).

### 4. Joystick-to-differential-drive mapping

Convert the nunchuck's (X, Y) into `(left_speed, left_dir, right_speed, right_dir)` using standard arcade-drive mixing:

```
Y axis → forward/backward thrust  (Y > center = forward, Y < center = backward)
X axis → turning                   (X > center = turn right, X < center = turn left)

left_power  = clamp(thrust + turn, -255, 255)
right_power = clamp(thrust - turn, -255, 255)

direction = if power >= 0 { Forward } else { Backward }
speed     = |power| as u8
```

Apply a **dead zone** (~10–15 units around center 128) so the robot doesn't creep when the stick is released. Make the dead zone configurable via a CLI flag (e.g., `--dead-zone 15`).

### 5. Main loop

```
1. Connect to Meccanoid (async, via meccable::Meccanoid::connect)
2. Start Wiimote manager & wait for a Wiimote to pair (sync, in a spawned thread)
3. Loop:
   a. Read nunchuck input (blocking read in dedicated thread, send via crossbeam/mpsc channel)
   b. Map joystick to wheel commands
   c. Send move_wheels() to Meccanoid
   d. Sleep ~50ms (20 Hz update rate — fast enough for responsive control,
      slow enough to not flood BLE)
   e. If Z button pressed → stop wheels (emergency stop / dead-man switch)
   f. If C button pressed → toggle eye color (fun feedback)
   g. If Wiimote Home button pressed → disconnect and exit
4. On exit: stop wheels, disconnect Meccanoid cleanly
```

Because `wiimote-rs` is synchronous and the Meccanoid API is async (tokio), bridge them with a dedicated reader thread that sends joystick state over a channel to the async main loop.

### 6. CLI interface

```
nunchuck-drive <MECCANOID_MAC>

Options:
  --dead-zone <u8>       Joystick dead zone radius (default: 15)
  --max-speed <u8>       Cap wheel speed (default: 255)
  --update-rate <u64>    Milliseconds between wheel updates (default: 50)
```

### 7. Error handling & graceful shutdown

- Catch Ctrl+C (`tokio::signal`) to stop wheels before exiting.
- If the Wiimote disconnects, stop wheels and print a message (keep running so user can re-pair).
- If the Meccanoid BLE connection drops, attempt reconnection with back-off.

### 8. Testing approach

- **Unit-test the joystick mapping function** — pure function, easy to test with table-driven cases (center → stop, full-forward → both wheels forward max, full-right → left forward / right backward, etc.).
- **Manual integration testing** — verify with actual hardware.

---

## File changes summary

| File | Change |
|---|---|
| `Cargo.toml` | Add `wiimote-rs` and `clap` dependencies |
| `src/bin/nunchuck_drive.rs` | New binary — all nunchuck-drive logic |
| `src/lib.rs` | No change (already re-exports `meccanoid::*`) |
| `src/meccanoid.rs` | No change needed (public API is sufficient) |

---

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| `wiimote-rs` nunchuck parsing incomplete | Parse raw extension bytes manually (6-byte format is trivial) |
| `wiimote-rs` extension init handshake not automated | Send the extension init register writes via raw output reports |
| Sync/async bridge overhead | Dedicated thread + channel is standard and lightweight |
| BLE command rate too high causes dropped packets | Throttle to 20 Hz; configurable via `--update-rate` |
| Joystick center drift | Dead-zone parameter; can also read nunchuck calibration data from registers `0x20–0x2F` |
