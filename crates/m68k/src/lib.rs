//! # m68k
//!
//! A safe Rust M68000 family CPU emulator.
//!
//! Supports: M68000, M68010, M68EC020, M68020, M68EC030, M68030, M68EC040, M68LC040, M68040, SCC68070

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(all(feature = "std", feature = "no_std"))]
compile_error!("features `std` and `no_std` are mutually exclusive");

#[cfg(not(any(feature = "std", feature = "no_std")))]
compile_error!("select exactly one runtime feature: `std` or `no_std`");

#[cfg(all(feature = "fpu", not(feature = "std")))]
compile_error!("feature `fpu` requires the `std` runtime profile");

extern crate alloc;

pub mod core;
pub mod dasm;
#[cfg(all(feature = "fpu", feature = "std"))]
pub mod fpu;
pub mod mmu;

// Re-export commonly used types from core
pub use core::cpu::CpuCore;
pub use core::memory::AddressBus;
pub use core::types::{CpuType, HleHandler, NoOpHleHandler, Size, StepResult};
