// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Fabian Schmieder

//! Hermetic differential evidence for the active 68000 core.
//!
//! The independent `m68000` workspace crate is the comparison oracle. The
//! explicit checkpoints keep this test useful even if both implementations
//! regress in the same way.

use m68000::{M68000, cpu_details::Mc68000};
use rumiga_core::{emulator::Emulator, memory::MemoryConfig};

const ROM_BASE: u32 = 0x00FC_0000;
const INITIAL_SSP: u32 = 0x0007_FFFC;
const START_OFFSET: u32 = 8;

const PROGRAM: [u16; 5] = [
    0x7005, // MOVEQ #5,D0
    0x72FF, // MOVEQ #-1,D1
    0x5280, // ADDQ.L #1,D0
    0x4E71, // NOP
    0x60FE, // BRA.S *
];

#[derive(Debug)]
struct Checkpoint {
    pc_offset: u32,
    d0: u32,
    d1: u32,
    sr: u16,
}

const CHECKPOINTS: [Checkpoint; PROGRAM.len()] = [
    Checkpoint {
        pc_offset: 10,
        d0: 5,
        d1: 0,
        sr: 0x2700,
    },
    Checkpoint {
        pc_offset: 12,
        d0: 5,
        d1: u32::MAX,
        sr: 0x2708,
    },
    Checkpoint {
        pc_offset: 14,
        d0: 6,
        d1: u32::MAX,
        sr: 0x2700,
    },
    Checkpoint {
        pc_offset: 16,
        d0: 6,
        d1: u32::MAX,
        sr: 0x2700,
    },
    Checkpoint {
        pc_offset: 16,
        d0: 6,
        d1: u32::MAX,
        sr: 0x2700,
    },
];

fn write_long(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn build_rumiga_rom() -> Vec<u8> {
    let mut rom = vec![0; 256 * 1024];
    write_long(&mut rom, 0, INITIAL_SSP);
    write_long(&mut rom, 4, ROM_BASE + START_OFFSET);

    for (index, opcode) in PROGRAM.into_iter().enumerate() {
        let offset = START_OFFSET as usize + index * 2;
        rom[offset..offset + 2].copy_from_slice(&opcode.to_be_bytes());
    }

    rom
}

fn build_reference_memory() -> Vec<u8> {
    let mut memory = vec![0; 64];
    write_long(&mut memory, 0, INITIAL_SSP);
    write_long(&mut memory, 4, START_OFFSET);

    for (index, opcode) in PROGRAM.into_iter().enumerate() {
        let offset = START_OFFSET as usize + index * 2;
        memory[offset..offset + 2].copy_from_slice(&opcode.to_be_bytes());
    }

    memory
}

#[test]
fn synthetic_boot_trace_matches_reference_and_frozen_checkpoints() {
    let mut rumiga = Emulator::new(MemoryConfig::a500());
    rumiga.load_rom(&build_rumiga_rom());

    let mut reference = M68000::<Mc68000>::new();
    let mut reference_memory = build_reference_memory();

    assert_eq!(rumiga.cpu.pc, ROM_BASE + START_OFFSET);
    assert_eq!(rumiga.cpu.dar[15], INITIAL_SSP);

    for (step, expected) in CHECKPOINTS.iter().enumerate() {
        rumiga.step_instruction();
        let reference_cycles = reference.interpreter(reference_memory.as_mut_slice());
        assert!(reference_cycles > 0, "reference stopped at step {step}");

        let rumiga_pc = rumiga
            .cpu
            .pc
            .checked_sub(ROM_BASE)
            .expect("Rumiga PC left the synthetic ROM");
        let reference_pc = reference.regs.pc.0;
        let reference_sr = u16::from(reference.regs.sr);
        let reference_data = reference.regs.d.map(|register| register.0);
        let reference_a7 = reference.regs.a(7);

        assert_eq!(rumiga_pc, expected.pc_offset, "Rumiga PC at step {step}");
        assert_eq!(
            reference_pc, expected.pc_offset,
            "reference PC at step {step}"
        );
        assert_eq!(rumiga.cpu.dar[0], expected.d0, "Rumiga D0 at step {step}");
        assert_eq!(
            reference.regs.d[0].0, expected.d0,
            "reference D0 at step {step}"
        );
        assert_eq!(rumiga.cpu.dar[1], expected.d1, "Rumiga D1 at step {step}");
        assert_eq!(
            reference.regs.d[1].0, expected.d1,
            "reference D1 at step {step}"
        );
        assert_eq!(rumiga.cpu.get_sr(), expected.sr, "Rumiga SR at step {step}");
        assert_eq!(reference_sr, expected.sr, "reference SR at step {step}");

        assert_eq!(rumiga_pc, reference_pc, "differential PC at step {step}");
        assert_eq!(
            rumiga.cpu.dar[..8],
            reference_data,
            "differential data registers at step {step}"
        );
        assert_eq!(
            rumiga.cpu.get_sr(),
            reference_sr,
            "differential SR at step {step}"
        );
        assert_eq!(rumiga.cpu.dar[15], INITIAL_SSP, "Rumiga A7 at step {step}");
        assert_eq!(reference_a7, INITIAL_SSP, "reference A7 at step {step}");
        assert_eq!(
            rumiga.cpu.dar[15], reference_a7,
            "differential A7 at step {step}"
        );
    }
}
