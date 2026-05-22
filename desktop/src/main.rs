// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Rumiga desktop binary — development and debugging target.

use std::fs;
use std::process;

use minifb::Key;
use rumiga_core::emulator::Emulator;
use rumiga_core::memory::MemoryConfig;
use rumiga_platform::VideoOutput;
use rumiga_platform_desktop::DesktopVideo;

const WIDTH: usize = 320;
const HEIGHT: usize = 256;
const ROM_SIZE_256K: usize = 256 * 1024;
const ROM_SIZE_512K: usize = 512 * 1024;

/// Amiga ESC keycode.
const AMIGA_KEY_ESC: u8 = 0x45;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MachineModel {
    A500,
    A500Plus,
    A600,
    A1200,
}

impl MachineModel {
    const fn config(self) -> MemoryConfig {
        match self {
            Self::A500 => MemoryConfig::a500(),
            Self::A500Plus | Self::A600 => MemoryConfig::a500_plus(),
            Self::A1200 => MemoryConfig::a1200(),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::A500 => "a500",
            Self::A500Plus => "a500-plus",
            Self::A600 => "a600",
            Self::A1200 => "a1200",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "a500" => Some(Self::A500),
            "a500+" | "a500-plus" | "a500plus" => Some(Self::A500Plus),
            "a600" => Some(Self::A600),
            "a1200" => Some(Self::A1200),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct LaunchArgs {
    model: Option<MachineModel>,
    rom_path: String,
    adf_paths: Vec<String>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let launch_args = parse_args(&args).unwrap_or_else(|e| {
        eprintln!("{e}");
        print_usage();
        process::exit(1);
    });

    let rom_path = &launch_args.rom_path;
    let rom_data = fs::read(rom_path).unwrap_or_else(|e| {
        eprintln!("Failed to read ROM file '{rom_path}': {e}");
        process::exit(1);
    });

    let model = select_model(&launch_args, rom_data.len()).unwrap_or_else(|e| {
        eprintln!("{e}");
        process::exit(1);
    });
    eprintln!("Starting Rumiga with {} profile", model.name());

    let mut emulator = Emulator::new(model.config());
    emulator.load_rom(&rom_data);

    // Load ADF disk images into DF0-DF3 in argument order.
    for (drive, adf_path) in launch_args.adf_paths.iter().enumerate() {
        let adf_data = fs::read(adf_path).unwrap_or_else(|e| {
            eprintln!("Failed to read ADF file '{adf_path}': {e}");
            process::exit(1);
        });
        eprintln!("Inserted {adf_path} as DF{drive}");
        emulator.insert_floppy(drive, adf_data);
    }

    let mut video = DesktopVideo::new("Rumiga", WIDTH, HEIGHT, 2).unwrap_or_else(|| {
        eprintln!("Failed to create video window");
        process::exit(1);
    });

    let window_handle = video.window_handle();

    #[allow(clippy::cast_possible_truncation)]
    let (w, h) = (WIDTH as u32, HEIGHT as u32);

    while video.is_open() {
        // Check ESC to quit
        if window_handle.borrow().is_key_down(Key::Escape) {
            break;
        }

        // Pass key events to emulator
        {
            let win = window_handle.borrow();
            for key in win.get_keys_pressed(minifb::KeyRepeat::No) {
                if let Some(keycode) = map_key_to_amiga(key) {
                    emulator.key_event(keycode, true);
                }
            }
            for key in win.get_keys_released() {
                if let Some(keycode) = map_key_to_amiga(key) {
                    emulator.key_event(keycode, false);
                }
            }
        }

        emulator.run_frame();
        video.present_frame(emulator.framebuffer(), w, h);
        emulator.clear_frame_ready();
    }
}

fn parse_args(args: &[String]) -> Result<LaunchArgs, String> {
    let mut model = None;
    let mut positional = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--model" | "-m" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--model requires a value".to_owned());
                };
                model = Some(
                    MachineModel::parse(value)
                        .ok_or_else(|| format!("Unsupported machine model '{value}'"))?,
                );
                index += 2;
            }
            "--help" | "-h" => return Err(String::new()),
            value if value.starts_with('-') => return Err(format!("Unknown option '{value}'")),
            value => {
                positional.push(value.to_owned());
                index += 1;
            }
        }
    }

    let Some(rom_path) = positional.first() else {
        return Err("Missing Kickstart ROM path".to_owned());
    };
    if positional.len() > 5 {
        return Err("Too many disk images; Rumiga supports DF0 through DF3".to_owned());
    }

    Ok(LaunchArgs {
        model,
        rom_path: rom_path.clone(),
        adf_paths: positional.iter().skip(1).cloned().collect(),
    })
}

fn select_model(args: &LaunchArgs, rom_size: usize) -> Result<MachineModel, String> {
    let model = args
        .model
        .unwrap_or_else(|| infer_model_from_rom(&args.rom_path, rom_size));

    if rom_size != expected_rom_size(model) {
        return Err(format!(
            "{} profile expects a {} KB ROM, got {} bytes",
            model.name(),
            expected_rom_size(model) / 1024,
            rom_size
        ));
    }

    Ok(model)
}

fn infer_model_from_rom(rom_path: &str, rom_size: usize) -> MachineModel {
    if rom_size == ROM_SIZE_256K {
        return MachineModel::A500;
    }

    let lower_path = rom_path.to_ascii_lowercase();
    if lower_path.contains("a1200") {
        MachineModel::A1200
    } else if lower_path.contains("a600") {
        MachineModel::A600
    } else {
        MachineModel::A500Plus
    }
}

const fn expected_rom_size(model: MachineModel) -> usize {
    match model {
        MachineModel::A500 => ROM_SIZE_256K,
        MachineModel::A500Plus | MachineModel::A600 | MachineModel::A1200 => ROM_SIZE_512K,
    }
}

fn print_usage() {
    eprintln!(
        "Usage: rumiga-desktop [--model a500|a500-plus|a600|a1200] <kickstart.rom> [df0.adf] [df1.adf] [df2.adf] [df3.adf]"
    );
}

/// Map a minifb key to an Amiga raw keycode.
const fn map_key_to_amiga(key: Key) -> Option<u8> {
    match key {
        Key::Escape => Some(AMIGA_KEY_ESC),
        Key::Space => Some(0x40),
        Key::Enter => Some(0x44),
        Key::Up => Some(0x4C),
        Key::Down => Some(0x4D),
        Key::Left => Some(0x4F),
        Key::Right => Some(0x4E),
        Key::Backspace => Some(0x41),
        Key::Tab => Some(0x42),
        Key::A => Some(0x20),
        Key::B => Some(0x35),
        Key::C => Some(0x33),
        Key::D => Some(0x22),
        Key::E => Some(0x12),
        Key::F => Some(0x23),
        Key::G => Some(0x24),
        Key::H => Some(0x25),
        Key::I => Some(0x17),
        Key::J => Some(0x26),
        Key::K => Some(0x27),
        Key::L => Some(0x28),
        Key::M => Some(0x37),
        Key::N => Some(0x36),
        Key::O => Some(0x18),
        Key::P => Some(0x19),
        Key::Q => Some(0x10),
        Key::R => Some(0x13),
        Key::S => Some(0x21),
        Key::T => Some(0x14),
        Key::U => Some(0x16),
        Key::V => Some(0x34),
        Key::W => Some(0x11),
        Key::X => Some(0x32),
        Key::Y => Some(0x15),
        Key::Z => Some(0x31),
        Key::Key0 => Some(0x0A),
        Key::Key1 => Some(0x01),
        Key::Key2 => Some(0x02),
        Key::Key3 => Some(0x03),
        Key::Key4 => Some(0x04),
        Key::Key5 => Some(0x05),
        Key::Key6 => Some(0x06),
        Key::Key7 => Some(0x07),
        Key::Key8 => Some(0x08),
        Key::Key9 => Some(0x09),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_accepts_model_rom_and_disk() {
        let args = vec![
            "--model".to_owned(),
            "a1200".to_owned(),
            "kick.rom".to_owned(),
            "workbench.adf".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                model: Some(MachineModel::A1200),
                rom_path: "kick.rom".to_owned(),
                adf_paths: vec!["workbench.adf".to_owned()],
            })
        );
    }

    #[test]
    fn parse_args_accepts_up_to_four_disks() {
        let args = vec![
            "kick.rom".to_owned(),
            "df0.adf".to_owned(),
            "df1.adf".to_owned(),
            "df2.adf".to_owned(),
            "df3.adf".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                model: None,
                rom_path: "kick.rom".to_owned(),
                adf_paths: vec![
                    "df0.adf".to_owned(),
                    "df1.adf".to_owned(),
                    "df2.adf".to_owned(),
                    "df3.adf".to_owned(),
                ],
            })
        );
    }

    #[test]
    fn select_model_infers_a1200_from_rom_name() {
        let args = LaunchArgs {
            model: None,
            rom_path: "kick.a1200.46.143.rom".to_owned(),
            adf_paths: Vec::new(),
        };

        assert_eq!(select_model(&args, ROM_SIZE_512K), Ok(MachineModel::A1200));
    }

    #[test]
    fn select_model_rejects_wrong_rom_size() {
        let args = LaunchArgs {
            model: Some(MachineModel::A1200),
            rom_path: "kick.a500.34.005.rom".to_owned(),
            adf_paths: Vec::new(),
        };

        assert!(select_model(&args, ROM_SIZE_256K).is_err());
    }
}
