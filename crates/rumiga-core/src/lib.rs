// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Amiga emulation core.
//!
//! This crate implements the Amiga custom chipset, memory subsystem, and
//! timing engine.

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
pub mod playfield;
pub mod sprites;
