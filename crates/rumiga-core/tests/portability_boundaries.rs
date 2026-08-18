// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Portability boundaries: pointer width, byte order, and guest address conversion.
//!
//! # Why not Miri
//!
//! The plan names Miri as one option. It is the wrong instrument here. Miri finds
//! undefined behaviour, and the workspace sets `unsafe_code = "forbid"`, so there is no
//! raw pointer arithmetic that could produce any. The two failure modes that actually
//! threaten this crate on its 32-bit target are neither of them undefined:
//!
//! - a `usize` narrower than a guest address truncates silently, and
//! - a native-endian conversion reads a guest value with the host's byte order.
//!
//! Both compile, both pass every host test, and both produce a wrong machine on the
//! device. Miri reports neither. The plan's third option, property fixtures, is what
//! this file provides, alongside the compile-time assertions in `lib.rs` and the
//! `clippy.toml` ban on native-endian conversions.
//!
//! # What is not covered
//!
//! Running the test suite with a 32-bit `usize` would be the direct check. No such
//! target with a usable `std` is available on the development host, and adding an
//! `i686` leg would test a pointer width the product never runs. The portable gate
//! compiles the core for `riscv32imafc`, which catches type errors but not truncation,
//! so the pointer-width claim rests on the compile-time assertions rather than on
//! execution. That is a weaker claim and is recorded as one.

use m68k::AddressBus as _;
use rumiga_core::emulator::Emulator;
use rumiga_core::memory::MemoryConfig;

/// Guest addresses are 32 bits, so the conversion to an index must be lossless.
#[test]
fn a_guest_address_survives_the_round_trip_through_usize() {
    for address in [
        0x0000_0000_u32,
        0x0000_0001,
        0x0007_FFFF,
        0x0008_0000,
        0x001F_FFFF,
        0x00BF_FFFF,
        0x00FF_FFFF,
        0x0100_0000,
        0x7FFF_FFFF,
        0x8000_0000,
        0xFFFF_FFFF,
    ] {
        let index = address as usize;
        assert_eq!(
            u32::try_from(index),
            Ok(address),
            "address {address:#010x} did not survive conversion to usize"
        );
    }
}

/// A `u16` written through the bus reads back with the guest's byte order.
///
/// The bytes are checked individually, not only the round trip: a round trip alone
/// passes under a consistent host-endian implementation, which is exactly the bug.
#[test]
fn word_access_is_big_endian_in_memory() {
    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.memory.overlay = false;

    emu.memory.chip_ram_mut()[0x1000] = 0x12;
    emu.memory.chip_ram_mut()[0x1001] = 0x34;

    // Read through the bus the CPU uses, not a test-only helper.
    assert_eq!(
        emu.memory.read_word(0x1000),
        0x1234,
        "the high byte must come first"
    );
}

/// A `u32` written through the bus reads back with the guest's byte order.
#[test]
fn long_access_is_big_endian_in_memory() {
    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.memory.overlay = false;

    for (offset, byte) in [0x12_u8, 0x34, 0x56, 0x78].into_iter().enumerate() {
        emu.memory.chip_ram_mut()[0x2000 + offset] = byte;
    }

    assert_eq!(
        emu.memory.read_long(0x2000),
        0x1234_5678,
        "long access must compose big-endian from two words"
    );
}

/// Writing a word places its bytes in guest order, not host order.
#[test]
fn writing_a_word_places_bytes_in_guest_order() {
    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.memory.overlay = false;

    emu.memory.write_word(0x3000, 0xABCD);

    assert_eq!(emu.memory.chip_ram()[0x3000], 0xAB);
    assert_eq!(emu.memory.chip_ram()[0x3001], 0xCD);
}

/// Every modelled RAM length fits in `u32`, which guest pointer masking assumes.
#[test]
fn every_modelled_ram_length_fits_in_u32() {
    for config in [
        MemoryConfig::a500(),
        MemoryConfig::a500_plus(),
        MemoryConfig::a600(),
        MemoryConfig::a1200(),
    ] {
        let emu = Emulator::new(config);

        for (name, length) in [
            ("chip", emu.memory.chip_ram().len()),
            ("slow", emu.memory.slow_ram_bytes().len()),
            ("fast", emu.memory.fast_ram_bytes().len()),
        ] {
            assert!(
                u32::try_from(length).is_ok(),
                "{name} RAM length {length} does not fit in u32"
            );
        }
    }
}

/// The framebuffer index space fits in `usize` on any supported target.
///
/// A PAL frame is 754 by 288 pixels, which is far below a 32-bit `usize`, but the
/// assertion states the dependency rather than leaving it to arithmetic luck.
#[test]
fn the_framebuffer_index_space_fits_in_usize() {
    let emu = Emulator::new(MemoryConfig::a500());
    let pixels = emu.framebuffer().len();

    assert!(pixels > 0);
    assert!(
        u32::try_from(pixels).is_ok(),
        "framebuffer index space must fit in 32 bits"
    );
}

/// Explicit big-endian conversion is independent of the host's byte order.
///
/// This is what makes the `clippy.toml` ban worth having. The development hosts and the
/// RISC-V target are little-endian, so a native-endian conversion would disagree with the
/// guest while still round-tripping through itself; these assertions pin the conversions
/// that do not depend on the host at all.
///
/// No assertion is made about the host's own byte order. A big-endian host would be
/// equally correct, it would merely make host tests less able to distinguish a
/// native-endian mistake, and failing the build over that would be hostile for no gain.
#[test]
fn explicit_big_endian_conversion_ignores_host_byte_order() {
    assert_eq!(u16::from_be_bytes([0x12, 0x34]), 0x1234);
    assert_eq!(0x1234_u16.to_be_bytes(), [0x12, 0x34]);
    assert_eq!(u32::from_be_bytes([0x12, 0x34, 0x56, 0x78]), 0x1234_5678);
    assert_eq!(0x1234_5678_u32.to_be_bytes(), [0x12, 0x34, 0x56, 0x78]);
}
