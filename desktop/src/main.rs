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

/// Amiga ESC keycode.
const AMIGA_KEY_ESC: u8 = 0x45;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: rumiga-desktop <kickstart.rom> [adf-file]");
        process::exit(1);
    }

    let rom_path = &args[1];
    let rom_data = fs::read(rom_path).unwrap_or_else(|e| {
        eprintln!("Failed to read ROM file '{rom_path}': {e}");
        process::exit(1);
    });

    let mut emulator = Emulator::new(match rom_data.len() {
        262_144 => MemoryConfig::a500(),
        524_288 => MemoryConfig::a500_plus(),
        _ => {
            eprintln!(
                "Unsupported ROM size: {} bytes (expected 256KB or 512KB)",
                rom_data.len()
            );
            process::exit(1);
        }
    });
    emulator.load_rom(&rom_data);

    // Load ADF disk image if provided
    if let Some(adf_path) = args.get(2) {
        let adf_data = fs::read(adf_path).unwrap_or_else(|e| {
            eprintln!("Failed to read ADF file '{adf_path}': {e}");
            process::exit(1);
        });
        emulator.insert_floppy(0, adf_data);
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
