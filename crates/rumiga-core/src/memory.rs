// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Amiga memory subsystem.
//!
//! Implements the Amiga memory map with chip RAM, slow RAM, fast RAM,
//! Kickstart ROM, custom chip registers, and CIA registers.

use alloc::vec;
use alloc::vec::Vec;
use m68000::memory_access::MemoryAccess;

/// Custom chip register address range.
const CUSTOM_BASE: u32 = 0x00DF_F000;
const CUSTOM_END: u32 = 0x00E0_0000;

/// CIA-B base address.
const CIA_B_BASE: u32 = 0x00BF_D000;
/// CIA address space end.
const CIA_END: u32 = 0x00C0_0000;

/// Kickstart ROM base (256KB mapped at 0xFC0000 for 512KB ROM, or 0xF80000).
const ROM_BASE_256K: u32 = 0x00FC_0000;
const ROM_BASE_512K: u32 = 0x00F8_0000;
const ROM_END: u32 = 0x0100_0000;

/// Memory configuration for a specific Amiga model.
#[derive(Clone, Debug)]
pub struct MemoryConfig {
    /// Chip RAM size in bytes (512KB, 1MB, or 2MB).
    pub chip_ram_size: u32,
    /// Slow RAM size in bytes (0 or 512KB at 0xC00000).
    pub slow_ram_size: u32,
    /// Fast RAM size in bytes (0–8MB at 0x200000).
    pub fast_ram_size: u32,
    /// ROM size in bytes (256KB or 512KB).
    pub rom_size: u32,
}

impl MemoryConfig {
    /// Amiga 500 default: 512KB chip, 512KB slow, no fast, 256KB ROM.
    #[must_use]
    pub const fn a500() -> Self {
        Self {
            chip_ram_size: 512 * 1024,
            slow_ram_size: 512 * 1024,
            fast_ram_size: 0,
            rom_size: 256 * 1024,
        }
    }

    /// Amiga 500+: 1MB chip, no slow, no fast, 512KB ROM.
    #[must_use]
    pub const fn a500_plus() -> Self {
        Self {
            chip_ram_size: 1024 * 1024,
            slow_ram_size: 0,
            fast_ram_size: 0,
            rom_size: 512 * 1024,
        }
    }

    /// Amiga 1200: 2MB chip, no slow, no fast, 512KB ROM.
    #[must_use]
    pub const fn a1200() -> Self {
        Self {
            chip_ram_size: 2 * 1024 * 1024,
            slow_ram_size: 0,
            fast_ram_size: 0,
            rom_size: 512 * 1024,
        }
    }
}

/// The Amiga memory subsystem implementing the m68000 `MemoryAccess` trait.
pub struct AmigaMemory {
    config: MemoryConfig,
    chip_ram: Vec<u8>,
    slow_ram: Vec<u8>,
    fast_ram: Vec<u8>,
    rom: Vec<u8>,
    /// When true, ROM is overlaid at address 0 (after reset, before first write to CIA).
    pub overlay: bool,
}

impl AmigaMemory {
    /// Create a new memory subsystem with the given configuration.
    ///
    /// ROM data must be loaded separately via [`Self::load_rom`].
    #[must_use]
    pub fn new(config: MemoryConfig) -> Self {
        let chip_ram = vec![0u8; config.chip_ram_size as usize];
        let slow_ram = vec![0u8; config.slow_ram_size as usize];
        let fast_ram = vec![0u8; config.fast_ram_size as usize];
        let rom = vec![0xFFu8; config.rom_size as usize];
        Self {
            config,
            chip_ram,
            slow_ram,
            fast_ram,
            rom,
            overlay: true,
        }
    }

    /// Load ROM data.
    ///
    /// # Panics
    /// Panics if `data` length doesn't match configured ROM size.
    pub fn load_rom(&mut self, data: &[u8]) {
        assert_eq!(
            data.len(),
            self.config.rom_size as usize,
            "ROM data size mismatch: expected {}, got {}",
            self.config.rom_size,
            data.len()
        );
        self.rom.copy_from_slice(data);
    }

    /// Returns a reference to the chip RAM slice.
    #[must_use]
    pub fn chip_ram(&self) -> &[u8] {
        &self.chip_ram
    }

    /// Returns a mutable reference to the chip RAM slice for DMA access.
    pub fn chip_ram_mut(&mut self) -> &mut [u8] {
        &mut self.chip_ram
    }

    /// ROM base address based on ROM size.
    const fn rom_base(&self) -> u32 {
        if self.config.rom_size == 512 * 1024 {
            ROM_BASE_512K
        } else {
            ROM_BASE_256K
        }
    }

    /// Read a byte from the memory map.
    fn read_byte(&self, addr: u32) -> Option<u8> {
        let addr = addr & 0x00FF_FFFF; // 24-bit address bus

        // Overlay: ROM mapped at 0x000000 after reset
        if self.overlay && addr < self.config.rom_size {
            return Some(self.rom[addr as usize]);
        }

        // Chip RAM: 0x000000–chip_ram_size (with mirroring)
        if addr < self.config.chip_ram_size {
            return Some(self.chip_ram[addr as usize]);
        }

        // Chip RAM mirror (wraps within chip_ram_size)
        if addr < 0x0020_0000 {
            let mirrored = addr % self.config.chip_ram_size;
            return Some(self.chip_ram[mirrored as usize]);
        }

        // Fast RAM: 0x200000–0xA00000
        if self.config.fast_ram_size > 0 && (0x0020_0000..0x00A0_0000).contains(&addr) {
            let offset = addr - 0x0020_0000;
            if offset < self.config.fast_ram_size {
                return Some(self.fast_ram[offset as usize]);
            }
        }

        // CIA space: 0xBF0000–0xC00000
        if (CIA_B_BASE..CIA_END).contains(&addr) {
            // TODO: dispatch to CIA emulation
            return Some(0xFF);
        }

        // Slow RAM: 0xC00000–0xC80000
        if self.config.slow_ram_size > 0 && (0x00C0_0000..0x00C8_0000).contains(&addr) {
            let offset = addr - 0x00C0_0000;
            if offset < self.config.slow_ram_size {
                return Some(self.slow_ram[offset as usize]);
            }
        }

        // Custom chip registers: 0xDFF000–0xE00000
        if (CUSTOM_BASE..CUSTOM_END).contains(&addr) {
            // TODO: dispatch to custom chip register read
            return Some(0x00);
        }

        // ROM: 0xF80000/0xFC0000–0x1000000
        if (self.rom_base()..ROM_END).contains(&addr) {
            let offset = addr - self.rom_base();
            if offset < self.config.rom_size {
                return Some(self.rom[offset as usize]);
            }
        }

        // Unmapped — bus error
        None
    }

    /// Write a byte to the memory map.
    fn write_byte(&mut self, addr: u32, value: u8) -> bool {
        let addr = addr & 0x00FF_FFFF;

        // Chip RAM
        if addr < self.config.chip_ram_size {
            self.chip_ram[addr as usize] = value;
            return true;
        }

        // Chip RAM mirror
        if addr < 0x0020_0000 {
            let mirrored = addr % self.config.chip_ram_size;
            self.chip_ram[mirrored as usize] = value;
            return true;
        }

        // Fast RAM
        if self.config.fast_ram_size > 0 && (0x0020_0000..0x00A0_0000).contains(&addr) {
            let offset = addr - 0x0020_0000;
            if offset < self.config.fast_ram_size {
                self.fast_ram[offset as usize] = value;
                return true;
            }
        }

        // CIA space
        if (CIA_B_BASE..CIA_END).contains(&addr) {
            // TODO: dispatch to CIA emulation
            return true;
        }

        // Slow RAM
        if self.config.slow_ram_size > 0 && (0x00C0_0000..0x00C8_0000).contains(&addr) {
            let offset = addr - 0x00C0_0000;
            if offset < self.config.slow_ram_size {
                self.slow_ram[offset as usize] = value;
                return true;
            }
        }

        // Custom chip registers
        if (CUSTOM_BASE..CUSTOM_END).contains(&addr) {
            // TODO: dispatch to custom chip register write
            return true;
        }

        // ROM — writes ignored
        if (self.rom_base()..ROM_END).contains(&addr) {
            return true;
        }

        // Unmapped
        false
    }
}

impl MemoryAccess for AmigaMemory {
    fn get_byte(&mut self, addr: u32) -> Option<u8> {
        self.read_byte(addr)
    }

    fn get_word(&mut self, addr: u32) -> Option<u16> {
        let hi = self.read_byte(addr)?;
        let lo = self.read_byte(addr.wrapping_add(1))?;
        Some((u16::from(hi) << 8) | u16::from(lo))
    }

    fn set_byte(&mut self, addr: u32, value: u8) -> Option<()> {
        if self.write_byte(addr, value) {
            Some(())
        } else {
            None
        }
    }

    fn set_word(&mut self, addr: u32, value: u16) -> Option<()> {
        let hi = (value >> 8) as u8;
        let lo = (value & 0xFF) as u8;
        let _ = self.write_byte(addr, hi);
        let _ = self.write_byte(addr.wrapping_add(1), lo);
        Some(())
    }

    fn reset_instruction(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_ram_read_write() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        mem.overlay = false;
        let _ = mem.set_byte(0x0000, 0x42);
        assert_eq!(mem.get_byte(0x0000), Some(0x42));
        let _ = mem.set_byte(0x7_FFFF, 0xAB);
        assert_eq!(mem.get_byte(0x7_FFFF), Some(0xAB));
    }

    #[test]
    fn chip_ram_mirror() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        mem.overlay = false;
        let _ = mem.set_byte(0x0100, 0x55);
        // 512KB chip RAM mirrors: 0x80100 should mirror to 0x0100
        assert_eq!(mem.get_byte(0x8_0100), Some(0x55));
    }

    #[test]
    fn rom_read_only() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        let rom = vec![0xAA; 256 * 1024];
        mem.load_rom(&rom);
        assert_eq!(mem.get_byte(0xFC_0000), Some(0xAA));
        // Write to ROM should be ignored
        let _ = mem.set_byte(0xFC_0000, 0x55);
        assert_eq!(mem.get_byte(0xFC_0000), Some(0xAA));
    }

    #[test]
    fn overlay_maps_rom_at_zero() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        let mut rom = vec![0x00; 256 * 1024];
        rom[0] = 0x00;
        rom[1] = 0x10;
        rom[2] = 0x00;
        rom[3] = 0x00;
        mem.load_rom(&rom);
        // With overlay, address 0 reads from ROM
        assert_eq!(mem.get_word(0x0000), Some(0x0010));
        // Disable overlay
        mem.overlay = false;
        // Now address 0 reads from chip RAM (which is zeroed)
        assert_eq!(mem.get_word(0x0000), Some(0x0000));
    }

    #[test]
    fn word_access() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        mem.overlay = false;
        let _ = mem.set_word(0x1000, 0xDEAD);
        assert_eq!(mem.get_word(0x1000), Some(0xDEAD));
        assert_eq!(mem.get_byte(0x1000), Some(0xDE));
        assert_eq!(mem.get_byte(0x1001), Some(0xAD));
    }

    #[test]
    fn slow_ram_access() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        let _ = mem.set_byte(0xC0_0000, 0x77);
        assert_eq!(mem.get_byte(0xC0_0000), Some(0x77));
    }

    #[test]
    fn unmapped_returns_none() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        mem.overlay = false;
        // Address in unmapped region (no fast RAM configured, 0x200000+)
        assert_eq!(mem.get_byte(0x20_0000), None);
    }
}
