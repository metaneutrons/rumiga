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

#[cfg(all(feature = "std", feature = "no_std"))]
compile_error!("features `std` and `no_std` are mutually exclusive");

#[cfg(not(any(feature = "std", feature = "no_std")))]
compile_error!("select exactly one runtime feature: `std` or `no_std`");

extern crate alloc;

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
