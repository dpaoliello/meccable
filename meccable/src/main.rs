//! Simple Rust library to control a Meccanoid robot via Bluetooth LE

use anyhow::{Context, Result};
use std::str::FromStr;
use std::time::Duration;
use tokio::time;

use meccable::{ChestLight, Meccanoid, Servo, ServoColor, WheelDirection};

fn get_from_user<T: FromStr>(kind: &str, stdin: &std::io::Stdin, buffer: &mut String) -> Result<T> {
    loop {
        println!("Enter {kind}");
        buffer.clear();
        stdin
            .read_line(buffer)
            .with_context(|| format!("Reading {kind} from stdin"))?;
        if let Ok(item) = T::from_str(buffer.trim()) {
            return Ok(item);
        } else {
            println!("Invalid {kind}, try again");
        }
    }
}

async fn inner_main() -> Result<()> {
    println!("Meccanoid Bluetooth Controller");

    // Connect to the Meccanoid.
    // You can find it by scanning for BLE devices
    let address = std::env::args()
        .nth(1)
        .expect("Please provide the MAC address of your Meccanoid");

    println!("Connecting to Meccanoid at {address}...");

    // Create a new controller
    let mut mec = Meccanoid::connect(&address)
        .await
        .with_context(|| "Connecting to meccanoid")?;
    println!("Connected successfully!");

    let stdin = std::io::stdin();
    let mut buffer = String::new();
    loop {
        println!("Enter action");
        buffer.clear();
        stdin.read_line(&mut buffer)?;

        match buffer.trim() {
            "quit" => break,

            "awake" => {
                mec.wake_up().await?;
            }

            "servomove" | "sm" => {
                let servo = get_from_user::<Servo>("servo", &stdin, &mut buffer)?;
                let value = get_from_user::<u8>("value (0-255)", &stdin, &mut buffer)?;
                mec.servo(servo, value).await?;
            }

            "servocolor" | "sc" => {
                let servo = get_from_user::<Servo>("servo", &stdin, &mut buffer)?;
                let color = get_from_user::<ServoColor>("color", &stdin, &mut buffer)?;
                mec.servo_light(servo, color).await?;
            }

            "eyecolor" | "ec" => {
                let r = get_from_user::<u8>("red (0-7)", &stdin, &mut buffer)?;
                let g = get_from_user::<u8>("green (0-7)", &stdin, &mut buffer)?;
                let b = get_from_user::<u8>("blue (0-7)", &stdin, &mut buffer)?;
                mec.eye_lights(r, g, b).await?;
            }

            "chestlight" | "cl" => {
                let light = get_from_user::<ChestLight>("light", &stdin, &mut buffer)?;
                let on = get_from_user::<bool>("on", &stdin, &mut buffer)?;
                mec.chest_light(light, on).await?;
            }

            "move" => {
                let right_dir =
                    get_from_user::<WheelDirection>("right wheel direction", &stdin, &mut buffer)?;
                let right_speed =
                    get_from_user::<u8>("right wheel speed (0-255)", &stdin, &mut buffer)?;
                let left_dir =
                    get_from_user::<WheelDirection>("left wheel direction", &stdin, &mut buffer)?;
                let left_speed =
                    get_from_user::<u8>("left wheel speed (0-255)", &stdin, &mut buffer)?;
                let move_time = get_from_user::<u64>("move time (ms)", &stdin, &mut buffer)?;
                mec.move_wheels(right_dir, right_speed, left_dir, left_speed)
                    .await?;
                time::sleep(Duration::from_millis(move_time)).await;
                mec.move_wheels(WheelDirection::Forward, 0, WheelDirection::Forward, 0)
                    .await?;
            }

            _ => println!("Unknown action"),
        }
    }

    // Disconnect
    println!("Disconnecting...");
    mec.disconnect().await?;
    println!("Done!");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(e) = inner_main().await {
        eprintln!("Error: {e:?}");
    }
    Ok(())
}
