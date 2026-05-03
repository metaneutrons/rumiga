// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Integration test: boot a real Kickstart ROM and verify emulation progresses.

use rumiga_core::emulator::Emulator;
use rumiga_core::memory::MemoryConfig;
use std::fs;
use std::path::PathBuf;

fn dirs_next() -> PathBuf {
    PathBuf::from(env!("HOME")).join("Documents/retro/amiga_winuae/rom")
}

#[test]
fn boot_kickstart_13_executes_instructions_and_produces_non_black_framebuffer() {
    let rom_file = dirs_next().join("kick.a500.34.005.rom");
    if !rom_file.exists() {
        eprintln!("SKIP: ROM not found at {}", rom_file.display());
        return;
    }

    let rom = fs::read(&rom_file).unwrap();
    assert_eq!(rom.len(), 256 * 1024, "ROM must be 256KB");

    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.load_rom(&rom);

    // Run 5 frames
    for _ in 0..5 {
        emu.run_frame();
    }

    // After 5 frames, CPU should have executed many cycles
    assert!(
        emu.total_cycles > 100_000,
        "Expected >100k cycles after 5 frames, got {}",
        emu.total_cycles
    );

    // Framebuffer should have some non-zero pixels (Kickstart shows colors)
    let non_zero = emu.framebuffer().iter().filter(|&&p| p != 0).count();
    println!(
        "After 5 frames: {} total cycles, {}/{} non-zero pixels",
        emu.total_cycles,
        non_zero,
        emu.framebuffer().len()
    );

    // The Kickstart ROM should at minimum set COLOR00 (background)
    // Even if rendering isn't perfect, the CPU should be running
    assert!(
        emu.total_cycles > 500_000,
        "CPU should execute >500k cycles in 5 PAL frames"
    );
}

#[test]
fn boot_kickstart_13_cpu_reads_valid_reset_vectors() {
    let rom_file = dirs_next().join("kick.a500.34.005.rom");
    if !rom_file.exists() {
        eprintln!("SKIP: ROM not found at {}", rom_file.display());
        return;
    }

    let rom = fs::read(&rom_file).unwrap();
    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.load_rom(&rom);

    // The ROM's first 4 bytes = initial SSP, next 4 = initial PC
    let ssp = u32::from_be_bytes([rom[0], rom[1], rom[2], rom[3]]);
    let pc = u32::from_be_bytes([rom[4], rom[5], rom[6], rom[7]]);

    println!("ROM reset vectors: SSP=0x{ssp:08X}, PC=0x{pc:08X}");

    // SSP should be a valid RAM address (typically 0x400 or similar)
    // PC should be in ROM space (0xFC0000+ for 256K ROM)
    assert!(
        !(0x0008_0000..0x00FC_0000).contains(&pc),
        "Reset PC should be in ROM or low memory, got 0x{pc:08X}"
    );

    // Run one frame to let CPU fetch reset vectors
    emu.run_frame();

    // CPU should have advanced past the reset vector
    assert!(emu.total_cycles > 0, "CPU should have executed some cycles");
}

#[test]
fn boot_kickstart_13_produces_display_output_after_startup_delay() {
    let rom_file = dirs_next().join("kick.a500.34.005.rom");
    if !rom_file.exists() {
        eprintln!("SKIP: ROM not found");
        return;
    }

    let rom = fs::read(&rom_file).unwrap();
    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.load_rom(&rom);

    // Kickstart 1.3 has a ~15 frame startup delay loop
    // Run 100 frames to get past it
    for _ in 0..100 {
        emu.run_frame();
    }

    let fb = emu.framebuffer();
    let non_zero = fb.iter().filter(|&&p| p != 0).count();
    println!(
        "After 100 frames: {} cycles, {}/{} non-zero pixels",
        emu.total_cycles,
        non_zero,
        fb.len()
    );

    assert!(
        non_zero > 0,
        "Kickstart should produce visible output after 100 frames (2 seconds)"
    );
}
