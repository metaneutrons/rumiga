// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! CPU trace sink contract.
//!
//! These tests run under both core runtime profiles and never create a file.
//! The expected records are golden values captured from the file-writing
//! implementation that preceded the injected sink, so they pin the record
//! layout byte for byte.

use std::fmt;
use std::sync::{Arc, Mutex};

use rumiga_core::TraceSink;
use rumiga_core::emulator::Emulator;
use rumiga_core::memory::MemoryConfig;

/// Address of the synthetic instruction sequence in chip RAM.
const PROGRAM_BASE: u32 = 0x1000;

/// `NOP` opcode, chosen because it neither reads nor writes emulated state.
const NOP: [u8; 2] = [0x4E, 0x71];

/// Records captured from the core, shared with the test after injection.
#[derive(Clone, Default)]
struct RecordingSink {
    records: Arc<Mutex<Vec<String>>>,
    flushes: Arc<Mutex<usize>>,
}

impl RecordingSink {
    fn records(&self) -> Vec<String> {
        self.records.lock().unwrap().clone()
    }

    fn flushes(&self) -> usize {
        *self.flushes.lock().unwrap()
    }
}

impl TraceSink for RecordingSink {
    fn write_record(&mut self, record: fmt::Arguments<'_>) {
        self.records.lock().unwrap().push(record.to_string());
    }

    fn flush(&mut self) {
        *self.flushes.lock().unwrap() += 1;
    }
}

/// Build an emulator executing `count` `NOP` instructions from [`PROGRAM_BASE`].
///
/// The overlay is disabled so the program is fetched from chip RAM rather than
/// from an absent Kickstart image, which keeps the fixture deterministic
/// without a ROM asset.
fn nop_emulator(count: usize) -> Emulator {
    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.memory.overlay = false;
    for index in 0..count {
        let base = PROGRAM_BASE as usize + index * 2;
        emu.memory.chip_ram[base] = NOP[0];
        emu.memory.chip_ram[base + 1] = NOP[1];
    }
    emu.cpu.pc = PROGRAM_BASE;
    emu
}

/// Expected record for the `NOP` at `pc`, captured from the previous
/// file-writing implementation.
fn expected_nop_record(pc: u32) -> String {
    format!(
        "PC: {pc:08X} | OP: 4E71 (NOP                 ) | D0: 00000000 D1: 00000000 D2: 00000000 D3: 00000000 | A0: 00000000 A1: 00000000 A2: 00000000 A7: FFFFFFFF | SR: 2704"
    )
}

#[test]
fn trace_records_are_byte_compatible() {
    let sink = RecordingSink::default();
    let mut emu = nop_emulator(4);
    emu.set_trace_sink(Box::new(sink.clone()), None);

    for _ in 0..4 {
        emu.step_instruction();
    }
    emu.flush_trace();

    let records = sink.records();
    let expected: Vec<String> = (0..4)
        .map(|index| expected_nop_record(PROGRAM_BASE + index * 2))
        .collect();
    assert_eq!(records, expected);
    assert_eq!(records[0].len(), 165, "record width must not drift");
    assert_eq!(emu.trace_count(), 4);
    assert_eq!(sink.flushes(), 1, "flush must be explicit, not drop-driven");
}

#[test]
fn trace_limit_bounds_recorded_instructions() {
    let sink = RecordingSink::default();
    let mut emu = nop_emulator(8);
    emu.set_trace_sink(Box::new(sink.clone()), Some(4));

    for _ in 0..8 {
        emu.step_instruction();
    }

    assert_eq!(sink.records().len(), 4);
    assert_eq!(emu.trace_count(), 4);
}

#[test]
fn attaching_a_sink_resets_the_recorded_count() {
    let first = RecordingSink::default();
    let mut emu = nop_emulator(8);
    emu.set_trace_sink(Box::new(first.clone()), None);
    for _ in 0..4 {
        emu.step_instruction();
    }
    assert_eq!(emu.trace_count(), 4);

    let second = RecordingSink::default();
    emu.set_trace_sink(Box::new(second.clone()), None);
    assert_eq!(emu.trace_count(), 0);
    emu.step_instruction();

    assert_eq!(first.records().len(), 4);
    assert_eq!(second.records().len(), 1);
    assert_eq!(emu.trace_count(), 1);
}

#[test]
fn clearing_the_sink_flushes_and_stops_recording() {
    let sink = RecordingSink::default();
    let mut emu = nop_emulator(8);
    emu.set_trace_sink(Box::new(sink.clone()), None);
    emu.step_instruction();
    emu.clear_trace_sink();

    let recorded = sink.records().len();
    for _ in 0..4 {
        emu.step_instruction();
    }

    assert_eq!(recorded, 1);
    assert_eq!(sink.records().len(), 1);
    assert_eq!(sink.flushes(), 1);
}

#[test]
fn emulator_without_a_sink_records_nothing() {
    let mut emu = nop_emulator(4);
    for _ in 0..4 {
        emu.step_instruction();
    }
    assert_eq!(emu.trace_count(), 0);
}

/// The core stays movable across threads; a sink must not remove that property.
#[test]
fn emulator_remains_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Emulator>();
}

/// Kickstart image directory used by the opportunistic real-ROM coverage.
fn rom_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("HOME")).join("Documents/retro/amiga_winuae/rom")
}

#[test]
fn traces_a_real_kickstart_instruction_stream() {
    let rom_file = rom_dir().join("kick.a1200.40.068.rom");
    if !rom_file.exists() {
        eprintln!("SKIP: ROM not found at {}", rom_file.display());
        return;
    }

    let sink = RecordingSink::default();
    let mut emu = Emulator::new(MemoryConfig::a1200());
    emu.load_rom(&std::fs::read(&rom_file).unwrap());
    emu.set_trace_sink(Box::new(sink.clone()), Some(50));

    for _ in 0..100 {
        emu.step_instruction();
    }
    emu.flush_trace();

    let records = sink.records();
    assert_eq!(
        records.len(),
        50,
        "trace limit must bound a real ROM stream"
    );
    for record in &records {
        assert_eq!(record.len(), 165, "record width must not drift: {record}");
        assert!(record.starts_with("PC: "), "unexpected record: {record}");
        assert!(record.contains(" | SR: "), "unexpected record: {record}");
    }
}
