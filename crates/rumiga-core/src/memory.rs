// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Amiga memory subsystem.
//!
//! Implements the Amiga memory map with chip RAM, slow RAM, fast RAM,
//! Kickstart ROM, custom chip registers, and CIA registers.

use alloc::vec;
use alloc::vec::Vec;
use m68000::memory_access::MemoryAccess;

use crate::cia::CiaPair;

/// Custom chip register address range.
const CUSTOM_BASE: u32 = 0x00DF_F000;
const CUSTOM_END: u32 = 0x00E0_0000;

/// CIA-A base address (odd bytes).
const CIA_A_BASE: u32 = 0x00BF_E001;
/// CIA-B base address (even bytes).
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

/// Number of word registers in the custom chip address space ($DFF000–$DFF1FF).
const CUSTOM_REG_COUNT: usize = 256;

/// The Amiga memory subsystem implementing the m68000 `MemoryAccess` trait.
pub struct AmigaMemory {
    config: MemoryConfig,
    chip_ram: Vec<u8>,
    slow_ram: Vec<u8>,
    fast_ram: Vec<u8>,
    rom: Vec<u8>,
    /// When true, ROM is overlaid at address 0 (after reset, before first write to CIA).
    pub overlay: bool,
    /// Shadow copy of custom chip registers (256 words at offsets $000–$1FE).
    pub custom_regs: [u16; CUSTOM_REG_COUNT],
    /// Log of register writes this scanline: (offset, value) pairs.
    reg_write_log: Vec<(u16, u16)>,
    /// CIA-A PRA shadow (bit 0 = OVL).
    pub cia_a_pra: u8,
    /// CIA pair (A and B) — lives here so `MemoryAccess` can read/write registers.
    pub cia: CiaPair,
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
            custom_regs: [0; CUSTOM_REG_COUNT],
            reg_write_log: Vec::new(),
            cia_a_pra: 0,
            cia: CiaPair::new(),
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

    /// Drain the register write log, returning an iterator over (offset, value) pairs.
    pub fn drain_reg_writes(&mut self) -> alloc::vec::Drain<'_, (u16, u16)> {
        self.reg_write_log.drain(..)
    }

    /// Read a custom chip register by word offset (0x000–0x1FE).
    #[must_use]
    pub const fn read_custom_reg(&self, offset: u16) -> u16 {
        let idx = (offset / 2) as usize;
        if idx < CUSTOM_REG_COUNT {
            self.custom_regs[idx]
        } else {
            0
        }
    }

    /// Write a custom chip register by word offset (0x000–0x1FE).
    pub fn write_custom_reg(&mut self, offset: u16, value: u16) {
        let idx = (offset / 2) as usize;
        if idx < CUSTOM_REG_COUNT {
            self.custom_regs[idx] = value;
            self.reg_write_log.push((offset, value));
        }
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
    #[allow(clippy::unnecessary_wraps)]
    fn read_byte(&mut self, addr: u32) -> Option<u8> {
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
            // CIA-A at odd addresses ($BFE001), register select via A8-A11
            if addr & 1 != 0 && addr >= CIA_A_BASE {
                let reg = ((addr >> 8) & 0xF) as u8;
                if reg == 0 {
                    // PRA: mix written value (output bits) with hardware input bits
                    // Bits 0-1: output (OVL, LED)
                    // Bits 2-5: input from disk/joystick hardware
                    //   Bit 2: DSKPROT (1=not protected)
                    //   Bit 3: DSKTRACK0 (0=at track 0)
                    //   Bit 4: DSKCHANGE (0=disk removed/changed)
                    //   Bit 5: DSKRDY (0=ready)
                    // Bits 6-7: joystick fire buttons (active low)
                    let output_bits = self.cia_a_pra & self.cia.cia_a.ddra;
                    // Check motor state from CIA-B PRB bit 7 (0=motor on)
                    // Check if any drive is selected (CIA-B PRB bits 3-6, active low)
                    // Drive select check (for future use with disk inserted) = self.cia.cia_b.prb & 0x78 != 0x78;
                    // RDY: 0=ready, 1=not ready.
                    // WinUAE: with no disk, drive_diskready() returns false → RDY=1 always.
                    // RDY only goes 0 when a disk is inserted and motor has spun up.
                    let rdy: u8 = 0x20; // Always not-ready when no disk
                    // DSKCHANGE=0 (no disk), DSKPROT=1, TRACK0=1 (not at track 0)
                    let input_bits: u8 = 0x04 | 0x08 | rdy | 0xC0;
                    return Some(output_bits | (input_bits & !self.cia.cia_a.ddra));
                }
                return Some(self.cia.cia_a.read(reg));
            }
            // CIA-B at even addresses ($BFD000), register select via A8-A11
            if addr & 1 == 0 {
                let reg = ((addr >> 8) & 0xF) as u8;
                return Some(self.cia.cia_b.read(reg));
            }
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
            let offset = (addr - CUSTOM_BASE) & 0x1FE;
            let word = self.custom_regs[(offset / 2) as usize];
            // Even address = high byte, odd address = low byte
            return if addr & 1 == 0 {
                Some((word >> 8) as u8)
            } else {
                Some((word & 0xFF) as u8)
            };
        }

        // ROM: 0xF80000/0xFC0000–0x1000000
        if (self.rom_base()..ROM_END).contains(&addr) {
            let offset = addr - self.rom_base();
            if offset < self.config.rom_size {
                return Some(self.rom[offset as usize]);
            }
        }

        // Unmapped — return open bus (0xFF) like real hardware
        Some(0xFF)
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
            // CIA-A at odd addresses ($BFE001), register select via A8-A11
            if addr & 1 != 0 && addr >= CIA_A_BASE {
                let reg = ((addr >> 8) & 0xF) as u8;
                self.cia.cia_a.write(reg, value);
                if reg == 0 {
                    self.cia_a_pra = value;
                    // CIA-A PRA bit 0: 0 disables overlay (chip RAM at $0)
                    self.overlay = value & 1 != 0;
                }
            } else if addr & 1 == 0 {
                // CIA-B at even addresses ($BFD000), register select via A8-A11
                let reg = ((addr >> 8) & 0xF) as u8;
                self.cia.cia_b.write(reg, value);
            }
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

        // Custom chip registers (word-only writes; accumulate byte pairs)
        if (CUSTOM_BASE..CUSTOM_END).contains(&addr) {
            let offset = ((addr - CUSTOM_BASE) & 0x1FE) as u16;
            let idx = (offset / 2) as usize;
            if idx < CUSTOM_REG_COUNT {
                if addr & 1 == 0 {
                    // High byte write — store, wait for low byte
                    self.custom_regs[idx] =
                        (self.custom_regs[idx] & 0x00FF) | (u16::from(value) << 8);
                } else {
                    // Low byte write — complete the word and log it
                    self.custom_regs[idx] = (self.custom_regs[idx] & 0xFF00) | u16::from(value);
                    self.reg_write_log.push((offset, self.custom_regs[idx]));
                }
            }
            return true;
        }

        // ROM — writes ignored
        if (self.rom_base()..ROM_END).contains(&addr) {
            return true;
        }

        // Unmapped — ignore writes like real hardware
        true
    }
}

impl MemoryAccess for AmigaMemory {
    fn get_byte(&mut self, addr: u32) -> Option<u8> {
        self.read_byte(addr)
    }

    fn get_word(&mut self, addr: u32) -> Option<u16> {
        let masked = addr & 0x00FF_FFFF;
        // Custom chip registers: atomic word read
        if (CUSTOM_BASE..CUSTOM_END).contains(&masked) {
            let offset = ((masked - CUSTOM_BASE) & 0x1FE) as u16;
            let idx = (offset / 2) as usize;
            return if idx < CUSTOM_REG_COUNT {
                Some(self.custom_regs[idx])
            } else {
                Some(0)
            };
        }
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
        let masked = addr & 0x00FF_FFFF;
        // Custom chip registers: handle as atomic word write
        if (CUSTOM_BASE..CUSTOM_END).contains(&masked) {
            let offset = ((masked - CUSTOM_BASE) & 0x1FE) as u16;
            let idx = (offset / 2) as usize;
            if idx < CUSTOM_REG_COUNT {
                self.custom_regs[idx] = value;
                self.reg_write_log.push((offset, value));

                // Immediately update readable shadow for set/clear registers
                let bits = value & 0x7FFF;
                match offset {
                    0x096 => {
                        // DMACON write → DMACONR readable
                        let r = &mut self.custom_regs[1usize];
                        if value & 0x8000 != 0 {
                            *r |= bits;
                        } else {
                            *r &= !bits;
                        }
                    }
                    0x09A => {
                        // INTENA write → INTENAR readable
                        let r = &mut self.custom_regs[14usize];
                        if value & 0x8000 != 0 {
                            *r |= bits;
                        } else {
                            *r &= !bits;
                        }
                    }
                    0x09C => {
                        // INTREQ write → INTREQR readable
                        let r = &mut self.custom_regs[15usize];
                        if value & 0x8000 != 0 {
                            *r |= bits;
                        } else {
                            *r &= !bits;
                        }
                    }
                    _ => {}
                }
            }
            return Some(());
        }
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
        assert_eq!(mem.get_byte(0x20_0000), Some(0xFF));
    }
}
