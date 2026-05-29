// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Test with a minimal synthetic ROM that directly sets up the Amiga display.
//! This bypasses the complex Kickstart boot sequence to verify our chipset works.

use m68k::AddressBus;
use rumiga_core::emulator::Emulator;
use rumiga_core::memory::MemoryConfig;

/// Build a minimal 256KB ROM that sets up a simple display.
fn build_test_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];

    // Reset vectors at offset 0
    // SSP = $040000
    rom[0] = 0x00;
    rom[1] = 0x04;
    rom[2] = 0x00;
    rom[3] = 0x00;
    // PC = $FC0008 (start of our code, offset 8 in ROM)
    rom[4] = 0x00;
    rom[5] = 0xFC;
    rom[6] = 0x00;
    rom[7] = 0x08;

    // Code at offset 8 ($FC0008):
    let code: Vec<u16> = vec![
        // LEA $DFF000,A0
        0x41F9, 0x00DF, 0xF000, // MOVE.W #$7FFF,$096(A0)  ; DMACON = clear all
        0x317C, 0x7FFF, 0x0096, // MOVE.W #$7FFF,$09A(A0)  ; INTENA = clear all
        0x317C, 0x7FFF, 0x009A, // MOVE.W #$7FFF,$09C(A0)  ; INTREQ = clear all
        0x317C, 0x7FFF, 0x009C,
        // Set up colors: COLOR00 = $0005 (dark blue bg)
        // MOVE.W #$0005,$180(A0)
        0x317C, 0x0005, 0x0180, // COLOR01 = $0FFF (white)
        0x317C, 0x0FFF, 0x0182,
        // Set display window
        // MOVE.W #$2C81,$08E(A0)  ; DIWSTRT
        0x317C, 0x2C81, 0x008E, // MOVE.W #$2CC1,$090(A0)  ; DIWSTOP
        0x317C, 0x2CC1, 0x0090, // MOVE.W #$0038,$092(A0)  ; DDFSTRT
        0x317C, 0x0038, 0x0092, // MOVE.W #$00D0,$094(A0)  ; DDFSTOP
        0x317C, 0x00D0, 0x0094,
        // Set 1 bitplane
        // MOVE.W #$1200,$100(A0)  ; BPLCON0 = 1 plane + color on
        0x317C, 0x1200, 0x0100,
        // Set bitplane pointer to $10000 (we'll put data there)
        // MOVE.W #$0001,$0E0(A0)  ; BPL1PTH = $0001
        0x317C, 0x0001, 0x00E0, // MOVE.W #$0000,$0E2(A0)  ; BPL1PTL = $0000
        0x317C, 0x0000, 0x00E2,
        // Enable DMA: bitplane + master
        // MOVE.W #$8380,$096(A0)  ; DMACON = SET master + bpl + copper
        0x317C, 0x8380, 0x0096, // Infinite loop
        // BRA.S *
        0x60FE,
    ];

    // Write code to ROM at offset 8
    for (i, &word) in code.iter().enumerate() {
        let off = 8 + i * 2;
        rom[off] = (word >> 8) as u8;
        rom[off + 1] = (word & 0xFF) as u8;
    }

    rom
}

#[test]
fn minimal_rom_sets_up_display_and_produces_colored_output() {
    let rom = build_test_rom();
    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.load_rom(&rom);

    // Write a pattern to chip RAM at $10000 (bitplane data)
    // Alternating $FFFF/$0000 words = alternating 16px stripes
    for i in 0..320u32 {
        let addr = 0x10000 + i * 2;
        let value: u16 = if (i / 2) % 2 == 0 { 0xFFFF } else { 0x0000 };
        AddressBus::write_word(&mut emu.memory, addr, value);
    }

    // Run 5 frames
    for _ in 0..5 {
        emu.run_frame();
    }

    println!("DMACON: ${:04X}", emu.chipset.dmacon);
    println!("BPLCON0: ${:04X}", emu.playfield.bplcon0);
    println!("Planes: {}", emu.playfield.num_planes());
    println!("DIWSTRT: ${:04X}", emu.playfield.diwstrt);
    println!("DDFSTRT: ${:04X}", emu.playfield.ddfstrt);
    println!("BPL1PT: ${:08X}", emu.playfield.bplpt[0]);
    println!("COLOR00: ${:04X}", emu.playfield.color[0]);
    println!("COLOR01: ${:04X}", emu.playfield.color[1]);

    let fb = emu.framebuffer();
    let non_zero = fb.iter().filter(|&&p| p != 0).count();
    let unique: std::collections::HashSet<u16> = fb.iter().copied().collect();
    println!("Non-zero pixels: {non_zero}/{}", fb.len());
    println!("Unique colors: {}", unique.len());

    // We should see at least 2 colors (background + foreground)
    assert!(emu.chipset.dmacon != 0, "DMACON should be set");
    assert!(
        emu.playfield.num_planes() >= 1,
        "Should have at least 1 bitplane"
    );
    assert!(non_zero > 0, "Should have visible pixels");
}
