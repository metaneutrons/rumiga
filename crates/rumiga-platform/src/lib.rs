// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Platform abstraction traits for the Rumiga Amiga emulator.
//!
//! Defines the interfaces that platform backends must implement for video
//! output, audio output, input handling, storage, and networking.

#![no_std]

extern crate alloc;
