//! Simple Rust library to control a Meccanoid robot via Bluetooth LE

use anyhow::{Context, Result, anyhow};
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use std::time::Duration;
use strum::EnumString;
use tokio::time;
use uuid::Uuid;

/// Servo motors.
#[repr(u8)]
#[derive(EnumString, PartialEq, Eq, Clone, Copy)]
#[strum(ascii_case_insensitive)]
pub enum Servo {
    #[strum(serialize = "re")]
    RightElbow = 0,
    #[strum(serialize = "rsp")]
    RightShoulderPitch = 1,
    #[strum(serialize = "rsr")]
    RightShoulderRoll = 2,
    #[strum(serialize = "lsr")]
    LeftShoulderRoll = 3,
    #[strum(serialize = "lsp")]
    LeftShoulderPitch = 4,
    #[strum(serialize = "le")]
    LeftElbow = 5,
    #[strum(serialize = "ny")]
    NeckYaw = 6,
    #[strum(serialize = "nr")]
    NeckRoll = 7,
}

/// Colors for servo lights.
#[repr(u8)]
#[derive(EnumString, PartialEq, Eq, Clone, Copy)]
#[strum(ascii_case_insensitive)]
pub enum ServoColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

/// Individual chest lights.
#[repr(u8)]
#[derive(EnumString, PartialEq, Eq, Clone, Copy)]
#[strum(ascii_case_insensitive)]
pub enum ChestLight {
    Blue,
    Red,
    Green,
    Yellow,
}

/// Wheel movement directions.
#[repr(u8)]
#[derive(EnumString, Debug, PartialEq, Eq, Clone, Copy)]
#[strum(ascii_case_insensitive)]
pub enum WheelDirection {
    #[strum(serialize = "f")]
    Forward = 0x01,
    #[strum(serialize = "b")]
    Backward = 0x02,
}

#[repr(u8)]
#[derive(PartialEq, Eq, Clone, Copy)]
enum Command {
    MoveServos = 0x08,
    ServoLights = 0x0c,
    ChestLights = 0x1c,
    ImAwake = 0x19,
    MoveWheels = 0x0d,
    EyeLights = 0x11,
}

// The characteristic UUID to use when sending commands
// You may need to discover this from your device
// TODO: How to detect this?
const CHARACTERISTIC_UUID: Uuid = Uuid::from_u128(0x0000ffe9_0000_1000_8000_00805f9b34fb);

/// Controls a Meccanoid robot via Bluetooth LE.
pub struct Meccanoid {
    device: Peripheral,

    // Stateful arrays that are remembered and mutated
    servos: [u8; 17],
    servo_lights: [u8; 17],
    chest_lights: [u8; 17],
}

async fn find_device_by_address(central: &Adapter, address: &str) -> Result<Option<Peripheral>> {
    let peripherals = central
        .peripherals()
        .await
        .with_context(|| "Getting list of Bluetooth devices")?;
    for p in peripherals {
        if let Ok(Some(props)) = p.properties().await
            && props.address.to_string().to_lowercase() == address.to_lowercase()
        {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

impl Meccanoid {
    /// Connect to the Meccanoid at the given Bluetooth address
    ///
    /// You can find the address by scanning for BLE devices.
    /// Example address: "c4:be:84:d4:68:1b"
    pub async fn connect(address: &str) -> Result<Self> {
        // Get the Bluetooth adapter
        let manager = Manager::new()
            .await
            .with_context(|| "Getting Bluetooth manager")?;
        let adapters = manager
            .adapters()
            .await
            .with_context(|| "Getting list of Bluetooth adapters")?;
        let central = adapters
            .into_iter()
            .nth(0)
            .ok_or(anyhow!("No Bluetooth adapter found"))?;

        // Start scanning for devices
        central.start_scan(ScanFilter::default()).await?;
        time::sleep(Duration::from_secs(2)).await;

        // Find the device by address
        let device = find_device_by_address(&central, address)
            .await
            .with_context(|| "Finding Meccanoid by address")?
            .ok_or_else(|| anyhow!("Meccanoid device not found"))?;

        // Connect to the device
        device
            .connect()
            .await
            .with_context(|| "Establishing Bluetooth connection")?;

        // Discover services and characteristics
        device
            .discover_services()
            .await
            .with_context(|| "Querying meccanoid for services")?;

        let mut this = Self {
            device,

            servos: [
                0x7f, 0x80, 0x00, 0xff, 0x80, 0x7f, 0x7f, 0x7f, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
                0x01, 0x01, 0x01,
            ],

            servo_lights: [
                0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04,
                0x04, 0x04, 0x00,
            ],

            chest_lights: [
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00,
            ],
        };

        // Send a wheel move of zero to make the meccanoid see us
        this.move_wheels(WheelDirection::Forward, 0, WheelDirection::Forward, 0)
            .await?;

        // Put arms into their start state
        this.send(Command::MoveServos, &this.servos).await?;

        // Set the lights to blue
        this.eye_lights(0, 0, 7).await?;

        Ok(this)
    }

    /// Disconnect from the Meccanoid
    pub async fn disconnect(self) -> Result<()> {
        self.device
            .disconnect()
            .await
            .with_context(|| "Disconnecting from Meccanoid")
    }

    /// Set a servo value
    pub async fn servo(&mut self, servo: Servo, mut value: u8) -> Result<()> {
        // These guys are reversed, handle that for the user
        if (servo == Servo::RightElbow) && value != 0x80 {
            value = 0xff - value;
        }

        self.servos[servo as usize] = value;

        self.send(Command::MoveServos, &self.servos)
            .await
            .with_context(|| "Sending servo command")?;

        Ok(())
    }

    /// Set the servo light to a given colour
    pub async fn servo_light(&mut self, servo: Servo, color: ServoColor) -> Result<()> {
        self.servo_lights[servo as usize] = color as u8;

        self.send(Command::ServoLights, &self.servo_lights)
            .await
            .with_context(|| "Sending servo light command")?;

        Ok(())
    }

    /// Set the on/off state of a chest light
    pub async fn chest_light(&mut self, light: ChestLight, on: bool) -> Result<()> {
        let value = if on { 0x01 } else { 0x00 };

        self.chest_lights[light as usize] = value;

        self.send(Command::ChestLights, &self.chest_lights)
            .await
            .with_context(|| "Sending chest light command")?;

        Ok(())
    }

    /// Move the wheels
    pub async fn move_wheels(
        &mut self,
        right_dir: WheelDirection,
        right_speed: u8,
        left_dir: WheelDirection,
        left_speed: u8,
    ) -> Result<()> {
        // Send the command
        let command = [
            left_dir as u8,
            right_dir as u8,
            left_speed,
            right_speed,
            0xff,
            0xff,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];

        self.send(Command::MoveWheels, &command)
            .await
            .with_context(|| "Sending wheel movement command")?;

        Ok(())
    }

    /// Set the eye lights to a specific colour
    ///
    /// RGB values between 0 and 7.
    pub async fn eye_lights(&mut self, r: u8, g: u8, b: u8) -> Result<()> {
        let r = r.min(7);
        let g = g.min(7);
        let b = b.min(7);

        let command = [
            0x00,
            0x00,
            (g << 3) | r,
            b,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];

        self.send(Command::EyeLights, &command)
            .await
            .with_context(|| "Sending eye light command")?;

        Ok(())
    }

    /// Makes Meccanoid say "I'm awake" and waggle its arms.
    pub async fn wake_up(&mut self) -> Result<()> {
        self.send(Command::ImAwake, &[0x1d; 17])
            .await
            .with_context(|| "Sending wake up command")?;
        Ok(())
    }

    /// Send a command to the unit
    async fn send(&self, command: Command, values: &[u8]) -> Result<()> {
        assert!(values.len() <= 17, "Values array must be 17 bytes long");

        let mut payload = [0; 20];
        payload[0] = command as u8;
        payload[1..18].copy_from_slice(values);

        // Calculate checksum
        let checksum: u16 = payload[..18].iter().map(|&v| v as u16).sum();

        // Build payload with checksum
        payload[18] = ((checksum >> 8) & 0xff) as u8;
        payload[19] = (checksum & 0xff) as u8;

        // Find the characteristic
        let chars = self.device.characteristics();
        let char = chars
            .iter()
            .find(|c| c.uuid == CHARACTERISTIC_UUID)
            .ok_or(anyhow!("Characteristic not found"))?;

        // Write the command
        self.device
            .write(char, &payload, WriteType::WithoutResponse)
            .await
            .with_context(|| "Writing command to meccanoid")?;

        Ok(())
    }
}
