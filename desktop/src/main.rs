// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Rumiga desktop binary — development and debugging target.

use std::fs;
use std::process;

use rumiga_core::emulator::Emulator;
use rumiga_core::memory::MemoryConfig;
use rumiga_platform::VideoOutput;
use rumiga_platform_desktop::DesktopVideo;

const WIDTH: usize = 320;
const HEIGHT: usize = 256;

fn main() {
    let Some(rom_path) = std::env::args().nth(1) else {
        eprintln!("Usage: rumiga-desktop <kickstart.rom>");
        process::exit(1);
    };

    let rom_data = fs::read(&rom_path).unwrap_or_else(|e| {
        eprintln!("Failed to read ROM file '{rom_path}': {e}");
        process::exit(1);
    });

    let mut emulator = Emulator::new(MemoryConfig::a500());
    emulator.load_rom(&rom_data);

    let mut video = DesktopVideo::new("Rumiga", WIDTH, HEIGHT, 2).unwrap_or_else(|| {
        eprintln!("Failed to create video window");
        process::exit(1);
    });

    #[allow(clippy::cast_possible_truncation)]
    let (w, h) = (WIDTH as u32, HEIGHT as u32);

    while video.is_open() {
        emulator.run_frame();
        if emulator.is_frame_ready() {
            video.present_frame(emulator.framebuffer(), w, h);
            emulator.clear_frame_ready();
        }
    }
}
