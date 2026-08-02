//! Vinput Rust management GUI executable.

use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let mut check = false;
    let mut offline = false;
    let mut config = None::<PathBuf>;

    while let Some(argument) = args.next() {
        match argument.to_string_lossy().as_ref() {
            "--version" | "-V" => {
                println!("vinput-gui {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--check" => check = true,
            "--offline" => offline = true,
            "--config" => {
                let value = args.next().ok_or("--config requires a path")?;
                config = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => return Err(format!("unknown argument `{other}`").into()),
        }
    }

    if offline && !check {
        return Err("--offline requires --check".into());
    }
    if check {
        let snapshot = vinput_gui::headless_snapshot(config.as_deref(), !offline)?;
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
        return Ok(());
    }

    vinput_gui::run()?;
    Ok(())
}

fn print_help() {
    println!("Vinput Rust management GUI");
    println!();
    println!("Usage: vinput-gui [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --config <PATH>  Read an explicit config for --check");
    println!("  --check          Print a redacted GUI/config/daemon snapshot and exit");
    println!("  --offline        Skip D-Bus probing during --check");
    println!("  -V, --version    Print version");
    println!("  -h, --help       Print help");
}
