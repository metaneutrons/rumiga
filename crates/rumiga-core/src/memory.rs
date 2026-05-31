// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Amiga memory subsystem.
//!
//! Implements the Amiga memory map with chip RAM, slow RAM, fast RAM,
//! Kickstart ROM, custom chip registers, and CIA registers.

#![allow(
    clippy::cast_lossless,
    clippy::if_not_else,
    clippy::map_unwrap_or,
    clippy::option_if_let_else,
    clippy::redundant_locals,
    clippy::too_many_lines
)]

use std::cell::{Cell, RefCell};

use m68k::AddressBus;

use crate::a2065::A2065;
use crate::blitter::BlitterState;
use crate::cia::CiaPair;
use crate::custom;
use crate::ide::AtaController;
use crate::network::MacAddress;

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

/// PCMCIA and Gayle address spaces.
const PCMCIA_COMMON_START: u32 = 0x0060_0000;
const PCMCIA_COMMON_END: u32 = 0x00A0_0000;
const PCMCIA_ATTR_START: u32 = 0x00A0_0000;
const PCMCIA_ATTR_END: u32 = 0x00A8_0000;
const GAYLE_LOW_START: u32 = 0x00D8_0000;
const GAYLE_LOW_END: u32 = 0x00DD_0000;
const GAYLE_HIGH_START: u32 = 0x00DD_0000;
const GAYLE_HIGH_END: u32 = 0x00DF_0000;
const GAYLE_IDE_START: u32 = 0x00DA_0000;
const GAYLE_IDE_END: u32 = 0x00DA_4000;
const GAYLE_CS_ADDR: u32 = 0x00DA_8000;
const GAYLE_IRQ_ADDR: u32 = 0x00DA_9000;
const GAYLE_INT_ADDR: u32 = 0x00DA_A000;
const GAYLE_CFG_ADDR: u32 = 0x00DA_B000;
const GAYLE_IRQ_IDE: u8 = 0x80;
const IDE_DATA_REG: usize = 0x00;
const IDE_STATUS_REG: usize = 0x07;
const IDE_SECONDARY_REG: usize = 0x0400;
const IDE_SECONDARY_LAST_REG: usize = IDE_SECONDARY_REG + 5;
const IDE_DEVCON_REG: usize = 0x0406;
const IDE_DRVADDR_REG: usize = 0x0407;

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
    /// CPU type (M68000, M68020, etc).
    pub cpu_type: m68k::CpuType,
    /// Whether CIA accesses use Gayle/Fat Gary single-CIA chip-select decoding.
    pub gayle_cia_decode: bool,
}

impl MemoryConfig {
    /// Amiga 500 default: 512KB chip, 512KB slow, no fast, 256KB ROM.
    #[must_use]
    pub const fn a500() -> Self {
        Self {
            chip_ram_size: 512 * 1024,
            slow_ram_size: 0,
            fast_ram_size: 0,
            rom_size: 256 * 1024,
            cpu_type: m68k::CpuType::M68000,
            gayle_cia_decode: false,
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
            cpu_type: m68k::CpuType::M68000,
            gayle_cia_decode: false,
        }
    }

    /// Amiga 600: 1MB chip, Gayle IDE/CIA decode, 512KB ROM.
    #[must_use]
    pub const fn a600() -> Self {
        Self {
            chip_ram_size: 1024 * 1024,
            slow_ram_size: 0,
            fast_ram_size: 0,
            rom_size: 512 * 1024,
            cpu_type: m68k::CpuType::M68000,
            gayle_cia_decode: true,
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
            cpu_type: m68k::CpuType::M68020,
            gayle_cia_decode: true,
        }
    }
}

/// Number of word registers in the custom chip address space ($DFF000–$DFF1FF).
const CUSTOM_REG_COUNT: usize = 256;

/// The Amiga memory subsystem implementing m68k's `AddressBus` trait.
#[allow(clippy::struct_excessive_bools)]
pub struct AmigaMemory {
    config: MemoryConfig,
    /// Amiga chip RAM buffer.
    pub chip_ram: Vec<u8>,
    /// Slow RAM (512KB at $C00000, directly accessible for workarounds).
    pub slow_ram: Vec<u8>,
    fast_ram: Vec<u8>,
    rom: Vec<u8>,
    /// Legacy diagnostic for ROM mutation; ROM loading must leave Kickstart bytes intact.
    pub rom_drive_step_patch_applied: bool,
    /// When true, ROM is overlaid at address 0 (after reset, before first write to CIA).
    pub overlay: bool,
    /// Shadow copy of custom chip registers (256 words at offsets $000–$1FE).
    pub custom_regs: [u16; CUSTOM_REG_COUNT],
    /// Log of register writes this scanline: (offset, value) pairs.
    reg_write_log: Vec<(u16, u16)>,
    /// CIA-A PRA shadow (bit 0 = OVL).
    pub cia_a_pra: u8,
    /// Set when CIA-B PRB is written (disk controller needs to process it).
    pub cia_b_prb_dirty: bool,
    /// Disk status bits for CIA-A PRA (bits 2-5), updated by emulator from floppy state.
    pub disk_status: u8,
    /// Left mouse button state.
    pub mouse_left: bool,
    /// CIA pair (A and B) — lives here so `AddressBus` can read/write registers.
    pub cia: RefCell<CiaPair>,
    /// DSKBYTR shadow register for read-clearing behavior.
    pub dskbytr: Cell<u16>,
    /// Gayle INTREQ shadow register.
    pub gayle_irq: u8,
    /// Gayle INTENA shadow register.
    pub gayle_intena: u8,
    /// Gayle CONFIG shadow register.
    pub gayle_config: u8,
    /// Gayle Status shadow register.
    pub gayle_status: u8,
    /// Gayle ID read sequence counter.
    pub gayle_id_cnt: Cell<u8>,
    /// Gayle IDE controller.
    pub ide: RefCell<AtaController>,
    /// Optional A2065-compatible Zorro II Ethernet card.
    pub a2065: RefCell<A2065>,
    /// Active blitter execution thread.
    pub blit_thread: Option<std::thread::JoinHandle<(Vec<u8>, BlitterState)>>,
    /// Set when the blitter thread finishes and RAM is restored.
    pub blitter_completed: bool,
    /// Final blitter register state returned by the completed blit thread.
    pub completed_blitter: Option<BlitterState>,
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
        let mut custom_regs = [0; CUSTOM_REG_COUNT];
        custom_regs[(custom::BEAMCON0 / 2) as usize] = custom::BEAMCON0_PAL;

        Self {
            config,
            chip_ram,
            slow_ram,
            fast_ram,
            rom,
            rom_drive_step_patch_applied: false,
            overlay: true,
            custom_regs,
            reg_write_log: Vec::new(),
            cia_a_pra: 0,
            cia_b_prb_dirty: false,
            disk_status: 0x3C, // Default: all status bits high (no drive selected state)
            mouse_left: false,
            cia: RefCell::new(CiaPair::new()),
            dskbytr: Cell::new(0),
            gayle_irq: 0,
            gayle_intena: 0,
            gayle_config: 0,
            gayle_status: 0,
            gayle_id_cnt: Cell::new(0),
            ide: RefCell::new(AtaController::new()),
            a2065: RefCell::new(A2065::new_disabled()),
            blit_thread: None,
            blitter_completed: false,
            completed_blitter: None,
        }
    }

    /// Enable an A2065-compatible Ethernet card in the Zorro II autoconfig chain.
    pub fn enable_a2065(&self, mac_address: MacAddress) {
        self.a2065.borrow_mut().enable(mac_address);
    }

    /// Remove the emulated A2065-compatible Ethernet card from the memory map.
    pub fn disable_a2065(&self) {
        self.a2065.borrow_mut().disable();
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
        self.rom_drive_step_patch_applied = false;
    }

    /// Wait for the active background blit thread to complete and restore chip RAM.
    pub fn sync_blitter(&mut self) {
        if let Some(handle) = self.blit_thread.take() {
            if let Ok((chip_ram, blitter)) = handle.join() {
                self.chip_ram = chip_ram;
                self.completed_blitter = Some(blitter);
                self.blitter_completed = true;
            }
        }
    }

    /// Check if the active background blit thread is finished, and restore chip RAM if so.
    pub fn sync_blitter_lazy(&mut self) {
        let is_finished = if let Some(ref handle) = self.blit_thread {
            handle.is_finished()
        } else {
            false
        };
        if is_finished {
            self.sync_blitter();
        }
    }

    fn gayle_ide_register(addr: u32) -> Option<usize> {
        if !(GAYLE_IDE_START..GAYLE_IDE_END).contains(&addr) {
            return None;
        }

        // FS-UAE/WinUAE-compatible Gayle alias decode. Bits A5 and A13 do not
        // select the ATA register; bit A12 selects the control block.
        let offset = addr & 0xFFFF;
        Some(((offset & !0x2020_u32) >> 2) as usize)
    }

    const fn cia_register(addr: u32) -> u8 {
        ((addr >> 8) & 0x0F) as u8
    }

    const fn cia_chip_select(addr: u32) -> u8 {
        ((addr >> 12) & 0x03) as u8
    }

    const fn cia_write_selects_a(&self, addr: u32) -> bool {
        let cs = Self::cia_chip_select(addr);
        if self.config.gayle_cia_decode {
            cs == 2
        } else {
            cs & 1 == 0
        }
    }

    const fn cia_write_selects_b(&self, addr: u32) -> bool {
        let cs = Self::cia_chip_select(addr);
        if self.config.gayle_cia_decode {
            cs == 1
        } else {
            cs & 2 == 0
        }
    }

    fn read_cia_a_register(&self, reg: u8) -> u8 {
        if reg == 0 {
            // PRA: mix output bits with hardware input bits.
            // Bits 0-1: output (OVL, LED)
            // Bits 2-5: disk status (from emulator's floppy controller)
            // Bits 6-7: joystick fire buttons (active low = 1 when not pressed)
            let cia = self.cia.borrow();
            let output_bits = self.cia_a_pra & cia.cia_a.ddra;
            let mut input_bits: u8 = self.disk_status & 0x3C;
            if !self.mouse_left {
                input_bits |= 0x40; // Bit 6 high when not pressed
            }
            input_bits |= 0x80; // Bit 7 high (joystick fire released)
            output_bits | (input_bits & !cia.cia_a.ddra)
        } else {
            self.cia.borrow_mut().cia_a.read(reg)
        }
    }

    fn read_cia_b_register(&self, reg: u8) -> u8 {
        self.cia.borrow_mut().cia_b.read(reg)
    }

    fn write_cia_a_register(&mut self, reg: u8, value: u8) {
        self.cia.borrow_mut().cia_a.write(reg, value);
        if reg == 0 {
            self.cia_a_pra = value;
            // CIA-A PRA bit 0: 0 disables overlay (chip RAM at $0).
            self.overlay = value & 1 != 0;
        }
    }

    fn write_cia_b_register(&mut self, reg: u8, value: u8) {
        self.cia.borrow_mut().cia_b.write(reg, value);
        if reg == 1 {
            self.cia_b_prb_dirty = true;

            // Enable CIA-B FLAG mask for DSKCHANGE detection.
            // Don't fire FLAG here - it fires naturally when DSKCHANGE
            // transitions, which happens during disk I/O attempts.
            let mut cia = self.cia.borrow_mut();
            if cia.cia_b.icr_mask & 0x10 == 0 {
                cia.cia_b.icr_mask |= 0x10;
            }
        }
    }

    fn read_cia_word(&self, addr: u32) -> Option<u16> {
        if !(CIA_B_BASE..CIA_END).contains(&addr) {
            return None;
        }

        let reg = Self::cia_register(addr);
        let cs = Self::cia_chip_select(addr);
        let mut value = match cs {
            0 if !self.config.gayle_cia_decode => {
                (u16::from(self.read_cia_b_register(reg)) << 8)
                    | u16::from(self.read_cia_a_register(reg))
            }
            1 => (u16::from(self.read_cia_b_register(reg)) << 8) | 0x00FF,
            2 => 0xFF00 | u16::from(self.read_cia_a_register(reg)),
            _ => 0xFFFF,
        };

        if addr & 1 != 0 {
            value = value.rotate_left(8);
        }
        Some(value)
    }

    fn write_cia_word(&mut self, addr: u32, value: u16) -> bool {
        if !(CIA_B_BASE..CIA_END).contains(&addr) {
            return false;
        }

        let reg = Self::cia_register(addr);
        let value = if addr & 1 != 0 {
            value.rotate_left(8)
        } else {
            value
        };
        let [high, low] = value.to_be_bytes();

        if self.cia_write_selects_b(addr) {
            self.write_cia_b_register(reg, high);
        }
        if self.cia_write_selects_a(addr) {
            self.write_cia_a_register(reg, low);
        }
        true
    }

    fn read_gayle_status(&self) -> u8 {
        let ide = self.ide.borrow();
        let pending_ide_irq = if ide.pending_irq && (ide.devcon & 0x02) == 0 {
            GAYLE_IRQ_IDE
        } else {
            0
        };
        self.gayle_status | (self.gayle_irq & GAYLE_IRQ_IDE) | pending_ide_irq
    }

    fn read_gayle_ide_register(&mut self, reg: usize) -> u8 {
        match reg {
            IDE_DATA_REG..=IDE_STATUS_REG => {
                let mut ide = self.ide.borrow_mut();
                let value = ide.read_register(reg, false);
                if reg == IDE_STATUS_REG {
                    ide.pending_irq = false;
                    drop(ide);
                    self.gayle_irq &= !GAYLE_IRQ_IDE;
                }
                value
            }
            IDE_SECONDARY_REG..=IDE_SECONDARY_LAST_REG => 0xFF,
            IDE_DEVCON_REG => self.ide.borrow_mut().read_register(IDE_STATUS_REG, true),
            IDE_DRVADDR_REG => self.ide.borrow().read_drive_address(),
            _ => 0xFF,
        }
    }

    fn write_gayle_ide_register(&self, reg: usize, value: u8) {
        match reg {
            IDE_DATA_REG..=IDE_STATUS_REG => {
                self.ide.borrow_mut().write_register(reg, false, value);
            }
            IDE_DEVCON_REG => {
                self.ide
                    .borrow_mut()
                    .write_register(IDE_STATUS_REG, true, value);
            }
            _ => {}
        }
    }

    /// Returns a reference to the chip RAM slice.
    #[must_use]
    pub fn chip_ram(&self) -> &[u8] {
        &self.chip_ram
    }

    /// Returns a reference to the ROM data.
    #[must_use]
    pub fn rom_data(&self) -> &[u8] {
        &self.rom
    }

    /// Returns a mutable reference to the chip RAM slice for DMA access.
    pub fn chip_ram_mut(&mut self) -> &mut [u8] {
        self.sync_blitter();
        &mut self.chip_ram
    }

    /// Drain the register write log, returning an iterator over (offset, value) pairs.
    pub fn drain_reg_writes(&mut self) -> std::vec::Drain<'_, (u16, u16)> {
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
    fn read_byte_internal(&mut self, addr: u32) -> u8 {
        let addr = addr; // 32-bit logical address bus

        // Overlay: ROM mapped at 0x000000 after reset
        if self.overlay && addr < self.config.rom_size {
            return self.rom[addr as usize];
        }

        // Chip RAM: 0x000000–chip_ram_size (with mirroring)
        if addr < self.config.chip_ram_size {
            return self.chip_ram[addr as usize];
        }

        // Chip RAM mirror (wraps within chip_ram_size)
        if addr < 0x0020_0000 {
            let mirrored = addr % self.config.chip_ram_size;
            return self.chip_ram[mirrored as usize];
        }

        // Fast RAM: 0x200000–0xA00000
        if self.config.fast_ram_size > 0 && (0x0020_0000..0x00A0_0000).contains(&addr) {
            let offset = addr - 0x0020_0000;
            if offset < self.config.fast_ram_size {
                return self.fast_ram[offset as usize];
            }
        }

        // PCMCIA common: 0x00600000–0x009FFFFF
        if (PCMCIA_COMMON_START..PCMCIA_COMMON_END).contains(&addr) {
            return 0xFF;
        }

        // PCMCIA attribute: 0x00A00000–0x00A7FFFF
        if (PCMCIA_ATTR_START..PCMCIA_ATTR_END).contains(&addr) {
            return 0xFF;
        }

        // CIA space: 0xBF0000–0xC00000
        if (CIA_B_BASE..CIA_END).contains(&addr) {
            // CIA-A at odd addresses ($BFE001), register select via A8-A11
            if addr & 1 != 0 && addr >= CIA_A_BASE {
                return self.read_cia_a_register(Self::cia_register(addr));
            }
            // CIA-B at even addresses ($BFD000), register select via A8-A11
            if addr & 1 == 0 {
                return self.read_cia_b_register(Self::cia_register(addr));
            }
            return 0xFF;
        }

        // Slow RAM: 0xC00000–0xC80000
        if self.config.slow_ram_size > 0 && (0x00C0_0000..0x00C8_0000).contains(&addr) {
            let offset = addr - 0x00C0_0000;
            if offset < self.config.slow_ram_size {
                return self.slow_ram[offset as usize];
            }
        }

        // Gayle Low Space: 0x00D80000–0x00DCFFFF
        if (GAYLE_LOW_START..GAYLE_LOW_END).contains(&addr) {
            match addr {
                GAYLE_CS_ADDR => return self.read_gayle_status(),
                GAYLE_IRQ_ADDR => return self.gayle_irq,
                GAYLE_INT_ADDR => return self.gayle_intena,
                GAYLE_CFG_ADDR => return self.gayle_config & 0x0F,
                _ => {
                    if let Some(reg) = Self::gayle_ide_register(addr) {
                        return self.read_gayle_ide_register(reg);
                    }
                    return 0x00;
                }
            }
        }

        // Gayle High Space: 0x00DD0000–0x00DEFFFF
        if (GAYLE_HIGH_START..GAYLE_HIGH_END).contains(&addr) {
            let offset = addr & 0xFFFF;
            if offset == 0x1000 {
                let cnt = self.gayle_id_cnt.get();
                let val = if cnt == 0 || cnt == 1 || cnt == 3 || cnt == 7 {
                    0x80
                } else {
                    0x00
                };
                self.gayle_id_cnt.set(cnt.wrapping_add(1));
                return val;
            }
            return 0x00;
        }

        // Custom chip registers: 0xDFF000–0xE00000
        if (CUSTOM_BASE..CUSTOM_END).contains(&addr) {
            let offset = (addr - CUSTOM_BASE) & 0x1FE;
            let mut word = self.custom_regs[(offset / 2) as usize];
            if offset == 0x01A {
                word = self.dskbytr.get();
                // Clear bit 15 (BYTERDY) on read
                self.dskbytr.set(word & !0x8000);
            }
            // Even address = high byte, odd address = low byte
            return if addr & 1 == 0 {
                (word >> 8) as u8
            } else {
                (word & 0xFF) as u8
            };
        }

        // ROM mirror at $E00000-$E7FFFF (ECS Agnus ksmirror_e0)
        if (0x00E0_0000..0x00E8_0000).contains(&addr) {
            let offset = (addr - 0x00E0_0000) % self.config.rom_size;
            return self.rom[offset as usize];
        }

        if let Some(value) = self.a2065.borrow().read_byte(addr) {
            return value;
        }

        // ROM: 0xF80000/0xFC0000–0x1000000
        if (self.rom_base()..ROM_END).contains(&addr) {
            let offset = addr - self.rom_base();
            if offset < self.config.rom_size {
                return self.rom[offset as usize];
            }
        }

        // Unmapped — return open bus (0xFF) like real hardware
        0xFF
    }

    /// Write a byte to the memory map.
    fn write_byte_internal(&mut self, addr: u32, value: u8) {
        let addr = addr; // 32-bit logical address bus

        // Chip RAM
        if addr < self.config.chip_ram_size {
            self.chip_ram[addr as usize] = value;
            return;
        }

        // Chip RAM mirror
        if addr < 0x0020_0000 {
            let mirrored = addr % self.config.chip_ram_size;
            self.chip_ram[mirrored as usize] = value;
            return;
        }

        // Fast RAM
        if self.config.fast_ram_size > 0 && (0x0020_0000..0x00A0_0000).contains(&addr) {
            let offset = addr - 0x0020_0000;
            if offset < self.config.fast_ram_size {
                self.fast_ram[offset as usize] = value;
                return;
            }
        }

        // PCMCIA common
        if (PCMCIA_COMMON_START..PCMCIA_COMMON_END).contains(&addr) {
            return;
        }

        // PCMCIA attribute
        if (PCMCIA_ATTR_START..PCMCIA_ATTR_END).contains(&addr) {
            return;
        }

        // CIA space
        if (CIA_B_BASE..CIA_END).contains(&addr) {
            let reg = Self::cia_register(addr);
            if self.config.gayle_cia_decode {
                if self.cia_write_selects_b(addr) {
                    self.write_cia_b_register(reg, value);
                }
                if self.cia_write_selects_a(addr) {
                    self.write_cia_a_register(reg, value);
                }
            } else {
                // CIA-A at odd addresses ($BFE001), CIA-B at even addresses ($BFD000).
                if addr & 1 != 0 && addr >= CIA_A_BASE {
                    self.write_cia_a_register(reg, value);
                } else if addr & 1 == 0 {
                    self.write_cia_b_register(reg, value);
                }
            }
            return;
        }

        // Slow RAM
        if self.config.slow_ram_size > 0 && (0x00C0_0000..0x00C8_0000).contains(&addr) {
            let offset = addr - 0x00C0_0000;
            if offset < self.config.slow_ram_size {
                self.slow_ram[offset as usize] = value;
                return;
            }
        }

        // Gayle Low Space: 0x00D80000–0x00DCFFFF
        if (GAYLE_LOW_START..GAYLE_LOW_END).contains(&addr) {
            match addr {
                GAYLE_CS_ADDR => {
                    self.gayle_status = value;
                }
                GAYLE_IRQ_ADDR => {
                    self.gayle_irq = (self.gayle_irq & value) | (value & 0x03);
                }
                GAYLE_INT_ADDR => {
                    self.gayle_intena = value;
                }
                GAYLE_CFG_ADDR => {
                    self.gayle_config = value;
                }
                _ => {
                    if let Some(reg) = Self::gayle_ide_register(addr) {
                        self.write_gayle_ide_register(reg, value);
                    }
                }
            }
            return;
        }

        // Gayle High Space: 0x00DD0000–0x00DEFFFF
        if (GAYLE_HIGH_START..GAYLE_HIGH_END).contains(&addr) {
            self.gayle_id_cnt.set(0);
            return;
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
        }

        let _ = self.a2065.borrow_mut().write_byte(addr, value);

        // ROM writes and unmapped — ignored
    }
}

impl AmigaMemory {
    /// Copy the state of another `AmigaMemory` instance.
    pub fn copy_from(&mut self, other: &Self) {
        self.chip_ram.copy_from_slice(&other.chip_ram);
        self.slow_ram.copy_from_slice(&other.slow_ram);
        self.fast_ram.copy_from_slice(&other.fast_ram);
        self.rom.copy_from_slice(&other.rom);
        self.rom_drive_step_patch_applied = other.rom_drive_step_patch_applied;
        self.overlay = other.overlay;
        self.custom_regs = other.custom_regs;
        self.cia_a_pra = other.cia_a_pra;
        self.cia_b_prb_dirty = other.cia_b_prb_dirty;
        self.disk_status = other.disk_status;
        *self.cia.borrow_mut() = other.cia.borrow().clone();
        self.dskbytr.set(other.dskbytr.get());
        self.gayle_irq = other.gayle_irq;
        self.gayle_intena = other.gayle_intena;
        self.gayle_config = other.gayle_config;
        self.gayle_status = other.gayle_status;
        self.gayle_id_cnt.set(other.gayle_id_cnt.get());
        *self.ide.borrow_mut() = other.ide.borrow().clone();
        *self.a2065.borrow_mut() = other.a2065.borrow().clone();
    }
}

impl AddressBus for AmigaMemory {
    fn read_byte(&mut self, addr: u32) -> u8 {
        if addr < self.config.chip_ram_size
            || addr < 0x0020_0000
            || (CUSTOM_BASE..CUSTOM_END).contains(&addr)
        {
            self.sync_blitter();
        } else {
            self.sync_blitter_lazy();
        }
        self.read_byte_internal(addr)
    }

    fn read_word(&mut self, addr: u32) -> u16 {
        let masked = addr;
        if masked < self.config.chip_ram_size
            || masked < 0x0020_0000
            || (CUSTOM_BASE..CUSTOM_END).contains(&masked)
        {
            self.sync_blitter();
        } else {
            self.sync_blitter_lazy();
        }
        // IDE Data Register read: all Gayle data-port aliases map to register 0.
        if Self::gayle_ide_register(masked) == Some(IDE_DATA_REG) {
            return self.ide.borrow_mut().read_data_word();
        }
        if let Some(value) = self.read_cia_word(masked) {
            return value;
        }
        if let Some(value) = self.a2065.borrow().read_word(masked) {
            return value;
        }
        // Custom chip registers: atomic word read
        if (CUSTOM_BASE..CUSTOM_END).contains(&masked) {
            let offset = ((masked - CUSTOM_BASE) & 0x1FE) as u16;
            let idx = (offset / 2) as usize;
            return if idx < CUSTOM_REG_COUNT {
                if offset == 0x01A {
                    let word = self.dskbytr.get();
                    // Clear bit 15 (BYTERDY) on read
                    self.dskbytr.set(word & !0x8000);
                    word
                } else {
                    self.custom_regs[idx]
                }
            } else {
                0
            };
        }
        let hi = self.read_byte_internal(addr);
        let lo = self.read_byte_internal(addr.wrapping_add(1));
        (u16::from(hi) << 8) | u16::from(lo)
    }

    fn read_long(&mut self, addr: u32) -> u32 {
        let hi = self.read_word(addr);
        let lo = self.read_word(addr.wrapping_add(2));
        (u32::from(hi) << 16) | u32::from(lo)
    }

    fn write_byte(&mut self, addr: u32, value: u8) {
        if addr < self.config.chip_ram_size
            || addr < 0x0020_0000
            || (CUSTOM_BASE..CUSTOM_END).contains(&addr)
        {
            self.sync_blitter();
        } else {
            self.sync_blitter_lazy();
        }
        self.write_byte_internal(addr, value);
    }

    fn write_word(&mut self, addr: u32, value: u16) {
        let masked = addr;
        if masked < self.config.chip_ram_size
            || masked < 0x0020_0000
            || (CUSTOM_BASE..CUSTOM_END).contains(&masked)
        {
            self.sync_blitter();
        } else {
            self.sync_blitter_lazy();
        }
        // IDE Data Register write: all Gayle data-port aliases map to register 0.
        if Self::gayle_ide_register(masked) == Some(IDE_DATA_REG) {
            self.ide.borrow_mut().write_data_word(value);
            return;
        }
        if self.write_cia_word(masked, value) {
            return;
        }
        if self.a2065.borrow_mut().write_word(masked, value) {
            return;
        }
        // Custom chip registers: handle as atomic word write
        if (CUSTOM_BASE..CUSTOM_END).contains(&masked) {
            let offset = ((masked - CUSTOM_BASE) & 0x1FE) as u16;
            let idx = (offset / 2) as usize;
            if idx < CUSTOM_REG_COUNT {
                let val = value;
                self.custom_regs[idx] = val;
                self.reg_write_log.push((offset, val));

                // Immediately update readable shadow for set/clear registers
                let bits = val & 0x7FFF;
                match offset {
                    0x096 => {
                        // DMACON write → DMACONR readable
                        let r = &mut self.custom_regs[1usize];
                        if val & 0x8000 != 0 {
                            *r |= bits;
                        } else {
                            *r &= !bits;
                        }
                    }
                    0x09A => {
                        // INTENA write → INTENAR readable
                        let r = &mut self.custom_regs[14usize];
                        if val & 0x8000 != 0 {
                            *r |= bits;
                        } else {
                            *r &= !bits;
                        }
                    }
                    0x09C => {
                        // INTREQ write → INTREQR readable
                        let r = &mut self.custom_regs[15usize];
                        if val & 0x8000 != 0 {
                            *r |= bits;
                        } else {
                            *r &= !bits;
                        }
                    }
                    0x09E => {
                        // ADKCON write → ADKCONR readable (offset $010 = index 8)
                        let r = &mut self.custom_regs[8usize];
                        if val & 0x8000 != 0 {
                            *r |= bits;
                        } else {
                            *r &= !bits;
                        }
                    }
                    _ => {}
                }
            }
            return;
        }
        let hi = (value >> 8) as u8;
        let lo = (value & 0xFF) as u8;
        self.write_byte_internal(addr, hi);
        self.write_byte_internal(addr.wrapping_add(1), lo);
    }

    fn write_long(&mut self, addr: u32, value: u32) {
        self.write_word(addr, (value >> 16) as u16);
        self.write_word(addr.wrapping_add(2), (value & 0xFFFF) as u16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_ram_read_write() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        mem.overlay = false;
        AddressBus::write_byte(&mut mem, 0x0000, 0x42);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x0000), 0x42);
        AddressBus::write_byte(&mut mem, 0x7_FFFF, 0xAB);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x7_FFFF), 0xAB);
    }

    #[test]
    fn chip_ram_mirror() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        mem.overlay = false;
        AddressBus::write_byte(&mut mem, 0x0100, 0x55);
        // 512KB chip RAM mirrors: 0x80100 should mirror to 0x0100
        assert_eq!(AddressBus::read_byte(&mut mem, 0x8_0100), 0x55);
    }

    #[test]
    fn rom_read_only() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        let rom = vec![0xAA; 256 * 1024];
        mem.load_rom(&rom);
        assert_eq!(AddressBus::read_byte(&mut mem, 0xFC_0000), 0xAA);
        // Write to ROM should be ignored
        AddressBus::write_byte(&mut mem, 0xFC_0000, 0x55);
        assert_eq!(AddressBus::read_byte(&mut mem, 0xFC_0000), 0xAA);
    }

    #[test]
    fn load_rom_preserves_kickstart_drive_parameter_table() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        let mut rom = vec![0xFF; 256 * 1024];
        rom[0x2_9F40] = 0x0B;
        rom[0x2_9F41] = 0xB8;

        mem.load_rom(&rom);

        assert!(!mem.rom_drive_step_patch_applied);
        assert_eq!(mem.rom[0x2_9F40], 0x0B);
        assert_eq!(mem.rom[0x2_9F41], 0xB8);
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
        assert_eq!(AddressBus::read_word(&mut mem, 0x0000), 0x0010);
        // Disable overlay
        mem.overlay = false;
        // Now address 0 reads from chip RAM (which is zeroed)
        assert_eq!(AddressBus::read_word(&mut mem, 0x0000), 0x0000);
    }

    #[test]
    fn word_access() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        mem.overlay = false;
        AddressBus::write_word(&mut mem, 0x1000, 0xDEAD);
        assert_eq!(AddressBus::read_word(&mut mem, 0x1000), 0xDEAD);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x1000), 0xDE);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x1001), 0xAD);
    }

    #[test]
    fn slow_ram_access() {
        let mut cfg = MemoryConfig::a500();
        cfg.slow_ram_size = 512 * 1024;
        let mut mem = AmigaMemory::new(cfg);
        AddressBus::write_byte(&mut mem, 0xC0_0000, 0x77);
        assert_eq!(AddressBus::read_byte(&mut mem, 0xC0_0000), 0x77);
    }

    #[test]
    fn unmapped_returns_ff() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        mem.overlay = false;
        // Address in unmapped region (no fast RAM configured, 0x200000+)
        assert_eq!(AddressBus::read_byte(&mut mem, 0x20_0000), 0xFF);
    }

    #[test]
    fn beamcon0_defaults_to_pal_timing() {
        let mem = AmigaMemory::new(MemoryConfig::a500());
        assert_eq!(mem.read_custom_reg(custom::BEAMCON0), custom::BEAMCON0_PAL);
    }

    #[test]
    fn a1200_gayle_byte_write_selects_ciaa_by_page_not_a0() {
        let mut mem = AmigaMemory::new(MemoryConfig::a1200());

        AddressBus::write_byte(&mut mem, 0x00BF_EE00, 0x01);

        let cia = mem.cia.borrow();
        assert_eq!(cia.cia_a.cra & 0x01, 0x01);
        assert_eq!(cia.cia_b.cra & 0x01, 0x00);
        assert_eq!(cia.cia_a.timer_a_stats.start_writes, 1);
        assert_eq!(cia.cia_b.timer_a_stats.start_writes, 0);
    }

    #[test]
    fn a1200_gayle_byte_write_selects_ciab_by_page_not_a0() {
        let mut mem = AmigaMemory::new(MemoryConfig::a1200());

        AddressBus::write_byte(&mut mem, 0x00BF_DE01, 0x01);

        let cia = mem.cia.borrow();
        assert_eq!(cia.cia_a.cra & 0x01, 0x00);
        assert_eq!(cia.cia_b.cra & 0x01, 0x01);
        assert_eq!(cia.cia_a.timer_a_stats.start_writes, 0);
        assert_eq!(cia.cia_b.timer_a_stats.start_writes, 1);
    }

    #[test]
    fn a1200_gayle_word_write_uses_ciaa_low_byte() {
        let mut mem = AmigaMemory::new(MemoryConfig::a1200());

        AddressBus::write_word(&mut mem, 0x00BF_EE00, 0x0001);

        let cia = mem.cia.borrow();
        assert_eq!(cia.cia_a.cra & 0x01, 0x01);
        assert_eq!(cia.cia_b.cra & 0x01, 0x00);
        assert_eq!(cia.cia_a.timer_a_stats.start_writes, 1);
    }

    #[test]
    fn a1200_gayle_word_write_uses_ciab_high_byte() {
        let mut mem = AmigaMemory::new(MemoryConfig::a1200());

        AddressBus::write_word(&mut mem, 0x00BF_DE00, 0x0100);

        let cia = mem.cia.borrow();
        assert_eq!(cia.cia_a.cra & 0x01, 0x00);
        assert_eq!(cia.cia_b.cra & 0x01, 0x01);
        assert_eq!(cia.cia_b.timer_a_stats.start_writes, 1);
    }

    #[test]
    fn gayle_id_sequence() {
        let mut mem = AmigaMemory::new(MemoryConfig::a1200());
        mem.overlay = false;

        // Sequence of reads from 0x00DD1000 should return:
        // 0x80, 0x80, 0x00, 0x80, 0x00, 0x00, 0x00, 0x80, then 0x00
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DD_1000), 0x80);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DD_1000), 0x80);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DD_1000), 0x00);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DD_1000), 0x80);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DD_1000), 0x00);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DD_1000), 0x00);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DD_1000), 0x00);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DD_1000), 0x80);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DD_1000), 0x00);

        // Write to Gayle High region resets the read counter
        AddressBus::write_byte(&mut mem, 0x00DD_0000, 0x00);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DD_1000), 0x80);
    }

    #[test]
    fn gayle_registers_read_write() {
        let mut mem = AmigaMemory::new(MemoryConfig::a1200());
        mem.overlay = false;

        // Test Gayle INTENA write/read
        AddressBus::write_byte(&mut mem, 0x00DA_A000, 0x5A);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DA_A000), 0x5A);

        // Test Gayle CONFIG write/read
        AddressBus::write_byte(&mut mem, 0x00DA_B000, 0xA5);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DA_B000), 0x05);

        // Test Gayle Status write/read
        AddressBus::write_byte(&mut mem, 0x00DA_8000, 0x03);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DA_8000), 0x03);

        // Test Gayle IRQ write/read
        AddressBus::write_byte(&mut mem, 0x00DA_9000, 0x03);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DA_9000), 0x03);
    }

    #[test]
    fn gayle_ide_command_alias_matches_fs_uae_decode() {
        let mut mem = AmigaMemory::new(MemoryConfig::a1200());
        mem.overlay = false;
        mem.ide.borrow_mut().insert_disk(vec![0; 1024 * 1024]);

        AddressBus::write_byte(&mut mem, 0x00DA_201C, 0xEC);

        let ide = mem.ide.borrow();
        assert_eq!(ide.command, 0xEC);
        assert_eq!(ide.devcon, 0x00);
        assert_ne!(ide.status & crate::ide::IDE_STATUS_DRQ, 0);
    }

    #[test]
    fn gayle_ide_devcon_alias_matches_fs_uae_decode() {
        let mut mem = AmigaMemory::new(MemoryConfig::a1200());
        mem.overlay = false;
        mem.ide.borrow_mut().insert_disk(vec![0; 1024 * 1024]);

        AddressBus::write_byte(&mut mem, 0x00DA_1018, 0x02);

        let ide = mem.ide.borrow();
        assert_eq!(ide.devcon, 0x02);
        assert_eq!(ide.command, 0x00);
    }

    #[test]
    fn gayle_status_reflects_and_acknowledges_ide_irq() {
        let mut mem = AmigaMemory::new(MemoryConfig::a1200());
        mem.overlay = false;
        mem.ide.borrow_mut().insert_disk(vec![0; 1024 * 1024]);

        AddressBus::write_byte(&mut mem, 0x00DA_201C, 0xEC);
        assert_ne!(
            AddressBus::read_byte(&mut mem, 0x00DA_8000) & GAYLE_IRQ_IDE,
            0
        );

        let _ = AddressBus::read_byte(&mut mem, 0x00DA_201C);
        assert_eq!(
            AddressBus::read_byte(&mut mem, 0x00DA_8000) & GAYLE_IRQ_IDE,
            0
        );
    }

    #[test]
    fn gayle_reserved_low_space_reads_zero() {
        let mut mem = AmigaMemory::new(MemoryConfig::a1200());
        mem.overlay = false;

        assert_eq!(AddressBus::read_byte(&mut mem, 0x00D8_0000), 0x00);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DA_4000), 0x00);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DA_FFFF), 0x00);
    }

    #[test]
    fn ide_status_returns_7f() {
        let mut mem = AmigaMemory::new(MemoryConfig::a1200());
        mem.overlay = false;
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00DA_001E), 0x7F);
    }

    #[test]
    fn pcmcia_unmapped_returns_ff() {
        let mut mem = AmigaMemory::new(MemoryConfig::a1200());
        mem.overlay = false;
        assert_eq!(AddressBus::read_byte(&mut mem, 0x0060_0000), 0xFF);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x009F_FFFF), 0xFF);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00A0_0000), 0xFF);
        assert_eq!(AddressBus::read_byte(&mut mem, 0x00A7_FFFF), 0xFF);
    }
}
