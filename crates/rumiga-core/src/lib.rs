// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Amiga emulation core.
//!
//! This crate implements the Amiga custom chipset, memory subsystem, and
//! timing engine.
//!
//! The default `std` feature retains host filesystem tracing and background
//! blitter execution. Embedded consumers select the mutually exclusive
//! `no_std` profile with `--no-default-features --features no_std`; that profile
//! requires an allocator, excludes host-only services, and forwards `no_std` to
//! `m68k`. The canonical portable gate compiles that complete stock-core graph
//! as an optimized bare-metal RISC-V release.

#![cfg_attr(not(feature = "std"), no_std)]

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
pub mod emulator;
pub mod events;
pub mod floppy;
pub mod ide;
pub mod memory;
pub mod network;
pub mod playfield;
pub mod sprites;
