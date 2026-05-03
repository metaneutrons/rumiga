// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Amiga emulation core.
//!
//! This crate implements the Amiga custom chipset, memory subsystem, and
//! timing engine. It is `no_std`-compatible and depends only on `core` and
//! `alloc`.

#![no_std]

extern crate alloc;

pub mod memory;
