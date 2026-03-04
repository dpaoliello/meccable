# Wiimote & Nunchuck Input in Rust — Research

## Context

The **meccable** project controls a Meccanoid robot over Bluetooth using `btleplug`.
This document researches how to read input from a **Nintendo Wii Remote (Wiimote)** and its
**Nunchuck** extension using open-source Rust crates, with the goal of using them as a controller
for the robot.

---

## Background: Wiimote Communication

The Wiimote communicates over **Bluetooth HID**. The host pairs with the Wiimote via standard
Bluetooth discovery (pressing 1+2 on the remote), after which it appears as an HID device.
Data is exchanged through HID **input and output reports**.

The **Nunchuck** (and other extensions like the Classic Controller) plugs into the Wiimote's
extension port and its data is multiplexed into the Wiimote's reports. The host reads Nunchuck
data (joystick X/Y, accelerometer, C/Z buttons) by requesting a report mode that includes
extension bytes and then parsing those bytes.

Key data available:
| Source    | Data                                                           |
|-----------|----------------------------------------------------------------|
| Wiimote   | Buttons (A, B, 1, 2, +, -, Home, D-pad), accelerometer, IR camera |
| Nunchuck  | Joystick (X, Y), accelerometer, C button, Z button            |

---

## Option 1: `wiimote-rs` (crates.io)

| | |
|---|---|
| **Crate** | [`wiimote-rs`](https://crates.io/crates/wiimote-rs) |
| **Version** | 0.2.0 (May 2025) |
| **Repository** | <https://github.com/cesmec/wiimote-rs> |
| **License** | MIT |
| **Downloads** | ~4,200 all-time |
| **Platforms** | **Windows** ✅, **Linux** ✅, macOS ✗ |

### Summary

A pure-Rust library for communicating with Wii remotes over Bluetooth. This is the most
actively maintained and feature-complete Wiimote crate. It handles Bluetooth discovery,
pairing, and bidirectional HID report communication.

### Features

- Connect Wii remotes over Bluetooth (press 1+2 to pair)
- Send output reports (LEDs, rumble, data report mode)
- Receive input reports (buttons, accelerometer, data reports)
- Read accelerometer calibration and convert raw values
- Read Motion Plus calibration and convert raw values
- Read Balance Board calibration and convert raw values
- Has an `extensions` module (suggesting extension/nunchuck support is in scope)

### Nunchuck Support

The crate has an `extensions` module in its API (`wiimote_rs::extensions`). The Wiimote's
HID protocol exposes extension data (including Nunchuck) through specific report modes.
The crate already supports setting data report modes and reading raw report data, so even if
Nunchuck parsing isn't fully built in yet, the raw extension bytes are accessible and can be
parsed manually.

### Platform Notes

- **Windows**: No additional setup required — ideal since this project targets Windows.
- **Linux**: Requires `libudev-dev`, `libbluetooth-dev`, `clang`.
- **macOS**: Not supported.

### Pros

- Best maintained dedicated Wiimote crate for desktop platforms
- Works on Windows out of the box
- MIT license (compatible with this project's MIT license)
- Has extension framework suggesting Nunchuck support is planned/partially implemented
- Manages Bluetooth discovery and pairing internally

### Cons

- Small user base (6 GitHub stars, 2 contributors)
- Extension/Nunchuck parsing may require manual work or contributing upstream
- No async API (uses synchronous blocking reads)

### Example

```rust
use wiimote_rs::prelude::*;
use wiimote_rs::input::InputReport;

fn main() -> WiimoteResult<()> {
    let manager = WiimoteManager::get_instance();
    let new_devices = {
        let manager = manager.lock().unwrap();
        manager.new_devices_receiver()
    };

    new_devices.iter().try_for_each(|device| -> WiimoteResult<()> {
        let input_report = device.lock().unwrap().read()?;
        match input_report {
            InputReport::DataReport(_, data) => {
                let buttons = data.buttons();
                // Parse extension bytes for nunchuck data...
            }
            _ => {}
        }
        Ok(())
    })
}
```

---

## Option 2: `wiimote` (codeberg/metamuffin)

| | |
|---|---|
| **Crate** | [`wiimote`](https://crates.io/crates/wiimote) |
| **Version** | 0.1.0 (Aug 2023) |
| **Repository** | <https://codeberg.org/metamuffin/wiimote-rs> |
| **License** | AGPL-3.0 |
| **Downloads** | ~1,500 all-time |
| **Platforms** | Cross-platform (via `hidapi`) |

### Summary

A Wiimote library/driver that uses the `hidapi` crate as its backend. It provides button,
speaker, rumble, player LED, IR camera, and accelerometer support.

### Nunchuck Support

**Not yet implemented.** The README explicitly lists Nunchuk and MotionPlus under
"Not yet implemented features." This is a significant gap for our use case.

### Pros

- Uses `hidapi` which is cross-platform (Windows, Linux, macOS)
- Supports buttons, speaker, rumble, LEDs, IR camera, accelerometer

### Cons

- **AGPL-3.0 license** — incompatible with this project's MIT license without relicensing
- **No Nunchuck support** — the primary feature we need is missing
- Not actively maintained (last commit Aug 2023)
- Very small user base (0 stars, 1 contributor)

---

## Option 3: `xwiimote` / `xwiimote-sys`

| | |
|---|---|
| **Crate** | [`xwiimote`](https://crates.io/crates/xwiimote) / [`xwiimote-sys`](https://crates.io/crates/xwiimote-sys) |
| **Version** | 0.2.5 / 0.1.6 |
| **Repository** | <https://github.com/hsanzg/xwiimote-rs> |
| **License** | MIT |
| **Downloads** | ~10,800 / ~8,900 all-time |
| **Platforms** | **Linux only** ✗ (requires `libudev`, `libxwiimote`) |

### Summary

Idiomatic Rust bindings to the [`xwiimote`](https://github.com/dvdhrm/xwiimote) Linux
user-space library. The xwiimote kernel driver is the standard Linux driver for Wiimotes
and supports all extensions including the Nunchuck.

### Nunchuck Support

**Yes** — the underlying xwiimote library fully supports Nunchuck, Classic Controller,
Motion Plus, and other extensions. The Rust bindings expose this.

### Pros

- Full extension support (Nunchuck, Classic Controller, Motion Plus, Balance Board)
- Builds on the battle-tested Linux xwiimote kernel driver
- MIT license
- Includes a demo app (`wiinote`) showcasing features

### Cons

- **Linux only** — not usable on Windows, which is this project's current platform
- Requires system packages: `libudev >= 183`, `libxwiimote >= 2-2`
- Small user base (2 GitHub stars, 1 contributor)
- No updates in ~3 years

---

## Option 4: `cwiid` / `libcwiid-sys`

| | |
|---|---|
| **Crate** | [`cwiid`](https://crates.io/crates/cwiid) / [`libcwiid-sys`](https://crates.io/crates/libcwiid-sys) |
| **Version** | 0.1.15 / 0.1.18 |
| **Repository** | <https://github.com/RedIODev/Cwiid-rs-api> |
| **License** | GPL-3.0 |
| **Downloads** | ~19,000 / ~22,400 all-time |
| **Platforms** | **Linux only** (based on the C `cwiid` library) |

### Summary

Rust bindings around the classic [cwiid](https://github.com/abstrakraft/cwiid) C library,
which is a long-established Linux Wiimote library. The safe API wraps the raw FFI bindings.

### Nunchuck Support

The README states "only the Wiimote itself is supported but accessories are planned."
**Nunchuck not currently supported** in the Rust bindings.

### Pros

- Based on a well-known C library with a long history
- Relatively higher download count among Wiimote crates

### Cons

- **GPL-3.0 license** — incompatible with MIT without relicensing
- **Linux only**
- **No Nunchuck support** in the Rust bindings
- Self-described "untested alpha release"
- No updates in ~4 years

---

## Option 5: `riimote`

| | |
|---|---|
| **Crate** | [`riimote`](https://crates.io/crates/riimote) |
| **Version** | 0.1.0 |
| **Repository** | <https://github.com/EthanYidong/riimote> |
| **License** | (not specified) |
| **Downloads** | ~1,600 all-time |
| **Platforms** | **Linux only** (uses `bluez-async` / D-Bus) |

### Summary

A Rust Wiimote library that communicates via BlueZ D-Bus on Linux. Requires custom
udev rules and D-Bus permissions configuration.

### Nunchuck Support

Unknown / unlikely — the crate appears abandoned with minimal documentation.

### Pros

- Pure Rust, no C bindings

### Cons

- **Linux only** (BlueZ D-Bus)
- Abandoned (last commit 5 years ago)
- Requires manual system configuration (udev rules, D-Bus permissions)
- Minimal documentation, no license file
- Only 3 GitHub stars

---

## Option 6: `wii-ext` (embedded-hal)

| | |
|---|---|
| **Crate** | [`wii-ext`](https://crates.io/crates/wii-ext) |
| **Version** | 0.4.0 |
| **Repository** | <https://github.com/9names/wii-ext-rs> |
| **License** | MIT / Apache-2.0 |
| **Downloads** | ~5,100 all-time |
| **Platforms** | **Embedded only** (uses `embedded-hal` I2C traits) |

### Summary

A platform-agnostic driver for Wiimote extension controllers (Nunchuk, Classic, Classic Pro,
NES Classic, SNES Classic) using `embedded-hal` and `embedded-hal-async` I2C traits. This
is designed for **microcontrollers** (e.g., RP2040) that connect to extension controllers
directly via the I2C port — **not** via Bluetooth to the Wiimote.

### Nunchuck Support

**Yes** — Nunchuck is a primary supported device. Reads joystick, accelerometer, and buttons.

### Pros

- Excellent Nunchuck and Classic Controller support
- Both blocking and async APIs
- Dual MIT/Apache-2.0 license
- Active maintenance (CI updated recently)

### Cons

- **Not applicable for our use case** — requires direct I2C wiring to the extension
  controller, not Bluetooth communication with a Wiimote
- Only usable on embedded platforms with `embedded-hal` I2C implementations
- Would work if we wired a Nunchuck directly to a microcontroller, but not for reading
  from a Wiimote+Nunchuck combo over Bluetooth

---

## Option 7: Generic Gamepad via `gilrs`

| | |
|---|---|
| **Crate** | [`gilrs`](https://crates.io/crates/gilrs) |
| **Version** | 0.11.1 (Dec 2025) |
| **Repository** | <https://gitlab.com/gilrs-project/gilrs> |
| **License** | MIT / Apache-2.0 |
| **Downloads** | ~5,300,000 all-time |
| **Platforms** | Windows ✅, Linux ✅, macOS ✅, Wasm ✅ |

### Summary

GilRs is a mature, widely-used game input library that provides a **unified gamepad
abstraction** across platforms. It supports SDL2 controller mappings, hotplugging, force
feedback, and power info. On Windows it uses Windows Gaming Input (WGI) or XInput; on Linux
it uses evdev.

### Wiimote / Nunchuck Support

**Indirect.** If the Wiimote is paired with the OS and recognized as a standard HID gamepad
(which the Linux `hid-wiimote` kernel driver does), gilrs can read it as a generic gamepad.
On Windows, the Wiimote would need to appear as a standard game controller through a
third-party driver (e.g., WiinUSoft, HID Wiimote driver).

The Nunchuck's joystick and buttons would only be accessible if the OS-level driver exposes
them as gamepad axes/buttons. The xwiimote Linux driver does expose Nunchuck data, so on
Linux this would work through gilrs. On Windows, this depends on the third-party driver.

### Pros

- Extremely mature and widely used (5.3M downloads, used by Bevy engine)
- Cross-platform with no external C dependencies on Windows
- Unified API regardless of controller type
- SDL2 mapping support for button/axis remapping
- Hotplugging, force feedback

### Cons

- **No native Wiimote protocol knowledge** — depends entirely on OS-level drivers
- **Windows requires a third-party Wiimote driver** to expose the Wiimote as an HID gamepad
- Nunchuck data availability depends on driver quality
- Loses access to Wiimote-specific features (IR camera, speaker, raw accelerometer calibration)
- Abstraction layer means you can't send raw output reports (LEDs, rumble must go through
  the driver)

---

## Option 8: Raw HID via `hidapi`

| | |
|---|---|
| **Crate** | [`hidapi`](https://crates.io/crates/hidapi) |
| **Version** | 2.6.5 (Feb 2026) |
| **Repository** | <https://github.com/ruabmbua/hidapi-rs> |
| **License** | MIT |
| **Downloads** | ~5,750,000 all-time |
| **Platforms** | Windows ✅, Linux ✅, macOS ✅ |

### Summary

Rust bindings to the [`hidapi`](https://github.com/libusb/hidapi) C library, providing
cross-platform access to USB and Bluetooth HID devices. This is a **generic HID layer** —
you would implement the Wiimote protocol yourself on top of it.

### Wiimote / Nunchuck Support

**Manual implementation required.** You would:
1. Enumerate HID devices and find the Wiimote by vendor/product ID (`0x057E` / `0x0306`)
2. Open the device
3. Send output reports to set the data report mode (e.g., `0x32` for buttons + 8 extension bytes)
4. Read input reports and parse the Wiimote button bits and Nunchuck extension bytes yourself

The Wiimote protocol is [well-documented on WiiBrew](https://wiibrew.org/wiki/Wiimote),
making a manual implementation feasible.

### Pros

- Extremely mature (5.7M downloads), actively maintained
- True cross-platform: Windows, Linux, macOS
- Full control over the HID protocol — can implement all Wiimote/Nunchuck features
- MIT license
- `windows-native` feature flag avoids the need for the C `hidapi` library on Windows

### Cons

- **Requires implementing the entire Wiimote protocol** — significant effort
- Must handle: extension initialization, encryption handshake, calibration reads,
  report mode negotiation, data parsing
- No higher-level abstractions; purely byte-level HID read/write

---

## Comparison Matrix

| Crate | Nunchuck | Windows | License | Active | Effort |
|---|---|---|---|---|---|
| **`wiimote-rs`** | Partial (framework exists) | ✅ | MIT | ✅ (2025) | Low–Medium |
| `wiimote` | ✗ | ✅ (via hidapi) | AGPL-3.0 | ✗ | N/A |
| `xwiimote` | ✅ | ✗ (Linux only) | MIT | ✗ | Low |
| `cwiid` | ✗ | ✗ (Linux only) | GPL-3.0 | ✗ | N/A |
| `riimote` | ✗ | ✗ (Linux only) | Unclear | ✗ | N/A |
| `wii-ext` | ✅ | N/A (embedded) | MIT/Apache | ✅ | N/A |
| **`gilrs`** | Via OS driver | ✅ (needs 3rd-party driver) | MIT/Apache | ✅ (2025) | Low (if driver available) |
| **`hidapi`** | Manual | ✅ | MIT | ✅ (2026) | High |

---

## Recommendations

### Best Option: `wiimote-rs`

For this project, **`wiimote-rs`** is the strongest candidate:

1. **Windows support** out of the box — matching the project's current platform.
2. **MIT license** — compatible with this project.
3. **Actively maintained** — most recent release was May 2025.
4. **Extension framework** — the `extensions` module suggests Nunchuck support is in scope.
   Even if not fully implemented, the infrastructure for reading extension data is present
   and the raw bytes can be parsed.
5. **Bluetooth management built in** — handles discovery and pairing, just like `btleplug`
   does for the Meccanoid.

The main risk is that Nunchuck parsing may need to be implemented or contributed upstream.
However, the Nunchuck data format is simple and well-documented (6 bytes: joystick X, Y,
accelerometer X/Y/Z high bits, and button + accelerometer low bits).

### Fallback: `hidapi` (DIY approach)

If `wiimote-rs` proves insufficient, building a custom implementation on **`hidapi`** is the
most flexible fallback:

- Full control over every aspect of the protocol
- Same cross-platform reach
- Can be tailored to exactly the subset of features needed (buttons + nunchuck joystick)
- The [WiiBrew documentation](https://wiibrew.org/wiki/Wiimote) is comprehensive enough to
  implement this

### Not Recommended

- **`wiimote`** (codeberg) — AGPL license, no Nunchuck support.
- **`cwiid`** — GPL license, Linux only, no Nunchuck in Rust bindings, unmaintained.
- **`riimote`** — Linux only, abandoned, no documentation.
- **`xwiimote`** — Excellent on Linux but Linux-only; not viable for Windows.
- **`wii-ext`** — Embedded-only; requires direct I2C, not Bluetooth.
- **`gilrs`** — Viable as a quick hack if a Windows Wiimote HID driver is installed, but
  doesn't provide Wiimote-specific features and adds a driver dependency.

---

## References

- [WiiBrew Wiimote Documentation](https://wiibrew.org/wiki/Wiimote) — comprehensive
  protocol reference
- [WiiBrew Extension Controllers](https://wiibrew.org/wiki/Wiimote/Extension_Controllers/Nunchuck) — Nunchuck data format
- [wiimote-rs source (extensions module)](https://github.com/cesmec/wiimote-rs/tree/main/src)
- [hidapi crate docs](https://docs.rs/hidapi/latest/hidapi/)
- [gilrs crate docs](https://docs.rs/gilrs/latest/gilrs/)
