// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Steady-state allocation measurement for the scanline loop.
//!
//! # Why this is its own test binary
//!
//! A global allocator is process-wide, so its counters see every thread. Cargo gives each
//! `tests/*.rs` file its own binary, and this file holds one test, so the measured window
//! has nothing else running in it. Adding a second test here would make the numbers
//! depend on test scheduling.
//!
//! # Why a third-party allocator
//!
//! The workspace sets `unsafe_code = "forbid"`, and a counting global allocator needs
//! `unsafe impl GlobalAlloc`. `forbid` cannot be relaxed per crate, so the wrapper has to
//! come from outside the workspace rather than be written here.
//!
//! # What this measures and what it does not
//!
//! It measures allocations the process performs while frames run, which is the effect that
//! matters. It cannot attribute them: a non-zero count says something allocated, not what.
//! The emulator's own capacity accessors answer the "what" and work in the `no_std` profile
//! where this test cannot run.

use std::alloc::System;

use rumiga_core::custom;
use rumiga_core::emulator::Emulator;
use rumiga_core::memory::MemoryConfig;
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

/// Frames run before measuring, so first-use buffer growth is not counted.
///
/// Growth on first use is not a steady-state allocation. Counting it would make the test
/// fail for a reason it is not about.
const WARMUP_FRAMES: u32 = 4;

/// Frames measured. Long enough that a per-scanline allocation cannot hide.
///
/// At 312 scanlines a single per-scanline allocation would show up as more than nineteen
/// thousand allocations here, so the failure is unmistakable rather than marginal.
const MEASURED_FRAMES: u32 = 64;

/// High half of the copper list address.
///
/// Kept as register halves rather than one address split by a cast, so no truncating
/// conversion needs justifying in a test whose subject is allocation, not arithmetic.
const COPPER_LIST_HIGH: u16 = 0x0002;

/// Low half of the copper list address.
const COPPER_LIST_LOW: u16 = 0x0000;

/// Where the guest program is placed and executed from.
const PROGRAM_BASE: u32 = 0x0000_1000;

/// A two-instruction guest loop that writes a custom register and branches back.
///
/// `move.w #$0FFF,$00DFF180` then `bra.s` to the start. This exists so the fixture
/// produces guest register writes on every scanline, which is the path a booting
/// Kickstart exercises constantly. A fixture without it measured a quieter loop and
/// passed while a real one-minute run allocated 978,521 times.
const GUEST_PROGRAM: [u16; 5] = [
    0x33FC, // move.w #imm,(xxx).L
    0x0FFF, // immediate
    0x00DF, // address high
    0xF180, // address low: COLOR00
    0x60F6, // bra.s back to PROGRAM_BASE
];

/// Build a machine with copper DMA running against a small copper list.
///
/// The copper matters specifically: its per-scanline path is where the emulator used to
/// allocate a fresh buffer for pending register writes. A test that left the copper
/// disabled would measure a quieter loop than the product runs.
fn machine_with_active_copper() -> Emulator {
    let mut emulator = Emulator::new(MemoryConfig::a500());
    emulator.memory.overlay = false;

    // A copper list that writes a colour register and then waits, so the copper produces
    // register writes on every scanline rather than finishing immediately.
    let list: [(u16, u16); 4] = [
        (custom::COLOR00, 0x0123),
        (0x0000, 0x0000),
        (custom::COLOR01, 0x0456),
        (0xFFFF, 0xFFFE),
    ];
    let base = (COPPER_LIST_HIGH as usize) << 16 | COPPER_LIST_LOW as usize;
    for (index, (first, second)) in list.iter().enumerate() {
        let offset = base + index * 4;
        // to_be_bytes rather than shift-and-cast: no truncating cast to justify, and the
        // byte order a copper list needs is stated rather than implied.
        emulator.memory.chip_ram[offset..offset + 2].copy_from_slice(&first.to_be_bytes());
        emulator.memory.chip_ram[offset + 2..offset + 4].copy_from_slice(&second.to_be_bytes());
    }

    assert_eq!(
        base,
        (COPPER_LIST_HIGH as usize) << 16 | COPPER_LIST_LOW as usize,
        "the register halves must address the list that was written"
    );
    emulator.dispatch_register_write(custom::COP1LCH, COPPER_LIST_HIGH);
    emulator.dispatch_register_write(custom::COP1LCL, COPPER_LIST_LOW);
    emulator.dispatch_register_write(custom::COPJMP1, 0);

    // Place and enter the guest loop so custom register writes reach the memory log.
    for (index, word) in GUEST_PROGRAM.iter().enumerate() {
        let offset = PROGRAM_BASE as usize + index * 2;
        emulator.memory.chip_ram[offset..offset + 2].copy_from_slice(&word.to_be_bytes());
    }
    emulator.cpu.pc = PROGRAM_BASE;
    // Bit 15 set means "OR these channel bits in": master enable plus the copper.
    emulator.dispatch_register_write(
        custom::DMACON,
        0x8000 | custom::DMA_MASTER | custom::DMA_COPPER,
    );

    emulator
}

#[test]
fn the_scanline_loop_does_not_allocate_in_steady_state() {
    let mut emulator = machine_with_active_copper();

    for _ in 0..WARMUP_FRAMES {
        emulator.run_frame();
    }
    // Both guards exist because a fixture that stops reaching a path would otherwise
    // keep passing while that path allocates again.
    assert!(
        emulator.copper_writes_capacity() > 0,
        "the warmup should have exercised the copper write buffer; capacity is still zero, \
         so this test is measuring a quieter loop than intended"
    );
    assert!(
        emulator.guest_reg_writes_capacity() > 0,
        "the warmup should have exercised the guest register write buffer; capacity is still \
         zero, so the path that cost 978,521 allocations per minute is not being measured"
    );

    let region = Region::new(ALLOCATOR);
    for _ in 0..MEASURED_FRAMES {
        emulator.run_frame();
    }
    let change = region.change();

    assert_eq!(
        (change.allocations, change.reallocations),
        (0, 0),
        "running {MEASURED_FRAMES} frames allocated {} times and reallocated {} times; \
         emulator buffer capacities are guest_reg_writes={} copper_writes={} \
         early_video_scanlines={} key_queue={}",
        change.allocations,
        change.reallocations,
        emulator.guest_reg_writes_capacity(),
        emulator.copper_writes_capacity(),
        emulator.early_video_scanlines_capacity(),
        emulator.key_queue_capacity(),
    );
}
