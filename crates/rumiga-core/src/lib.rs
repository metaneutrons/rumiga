// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Amiga emulation core.
//!
//! This crate implements the Amiga custom chipset, memory subsystem, and
//! timing engine.
//!
//! The default `std` feature retains background blitter execution. Embedded
//! consumers select the mutually exclusive `no_std` profile with
//! `--no-default-features --features no_std`; that profile requires an
//! allocator, excludes host-only services, and forwards `no_std` to `m68k`. The
//! canonical portable gate compiles that complete stock-core graph as an
//! optimized bare-metal RISC-V release.
//!
//! CPU tracing is available in both profiles. The core formats records and
//! writes them to an injected [`TraceSink`]; file creation and buffering belong
//! to the platform adapter that supplies the sink.

#![cfg_attr(not(feature = "std"), no_std)]
// Primitive types with `core` or `alloc` equivalents must remain portable even
// when their callers select the desktop runtime profile.
#![deny(clippy::std_instead_of_alloc, clippy::std_instead_of_core)]
// Emulated time must never come from the host clock, in either profile.
#![deny(clippy::disallowed_types)]
// Guest values must be converted with an explicit byte order; see `clippy.toml`.
#![deny(clippy::disallowed_methods)]

#[cfg(all(feature = "std", feature = "no_std"))]
compile_error!("features `std` and `no_std` are mutually exclusive");

#[cfg(not(any(feature = "std", feature = "no_std")))]
compile_error!("select exactly one runtime feature: `std` or `no_std`");

extern crate alloc;

/// A guest address is 32 bits wide, so `usize` must hold one without truncation.
///
/// The product target is a 32-bit RISC-V core, where this holds exactly. It is asserted
/// rather than assumed because the failure mode on a narrower target is silent: every
/// `guest_address as usize` would truncate, the host tests would still pass, and the
/// device would read the wrong memory. A build for such a target fails here instead.
const _: () = assert!(
    core::mem::size_of::<usize>() >= core::mem::size_of::<u32>(),
    "rumiga-core requires usize to be at least 32 bits: guest addresses are u32"
);

/// The largest chip RAM this core models is 2 MiB, so a RAM length fits in `u32`.
///
/// Several sites narrow a slice length to `u32` to mask a guest pointer against the RAM
/// size. That is sound only while the length fits, which this states.
const _: () = assert!(
    2 * 1024 * 1024 <= u32::MAX as usize,
    "chip RAM length must fit in u32 for guest pointer masking"
);

pub mod a2065;
pub mod audio;
pub mod blitter;
pub mod chipset;
pub mod cia;
pub mod copper;
pub mod custom;
pub mod digest;
pub mod emulator;
pub mod events;
pub mod floppy;
pub mod ide;
pub mod memory;
pub mod network;
pub mod playfield;
pub mod replay;
pub mod sprites;
pub mod video;

/// Diagnostic record transport contract implemented by platform adapters.
pub use rumiga_platform::TraceSink;
