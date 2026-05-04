// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Main emulation loop tying CPU and chipset together.

use alloc::vec;
use alloc::vec::Vec;
use m68000::M68000;
use m68000::cpu_details::Mc68000;

use crate::audio::AudioState;
use crate::blitter::BlitterState;
use crate::chipset::CustomChipState;
use crate::copper::{CopperAction, CopperState};
use crate::custom;
use crate::events::{EventScheduler, EventType, SCANLINES_PAL};
use crate::floppy::FloppyController;
use crate::memory::{AmigaMemory, MemoryConfig};
use crate::playfield::{self, PlayfieldState};
use crate::sprites::SpriteEngine;

/// CPU cycles per scanline (227 color clocks × 2).
const CYCLES_PER_LINE: usize = 227 * 2;

/// Display width in pixels.
const DISPLAY_WIDTH: usize = playfield::DISPLAY_WIDTH as usize;

/// Display height in pixels (as u16 for beam comparison).
///
/// Derived from [`playfield::DISPLAY_HEIGHT`]; compile-time assertion guarantees no truncation.
#[allow(clippy::cast_possible_truncation)]
const VISIBLE_LINES: u16 = {
    assert!(playfield::DISPLAY_HEIGHT <= u16::MAX as u32);
    playfield::DISPLAY_HEIGHT as u16
};

/// Framebuffer size in pixels.
const FRAMEBUFFER_SIZE: usize = DISPLAY_WIDTH * playfield::DISPLAY_HEIGHT as usize;

/// Maximum queued key events per frame.
const MAX_KEY_EVENTS: usize = 16;

/// Main emulator state combining CPU and all chipset subsystems.
pub struct Emulator {
    /// Motorola 68000 CPU.
    pub cpu: M68000<Mc68000>,
    /// Amiga memory subsystem.
    pub memory: AmigaMemory,
    /// Custom chip register state.
    pub chipset: CustomChipState,
    /// Cycle-accurate event scheduler.
    pub events: EventScheduler,
    /// Copper coprocessor.
    pub copper: CopperState,
    /// Bitplane/playfield renderer.
    pub playfield: PlayfieldState,
    /// Blitter DMA engine.
    pub blitter: BlitterState,
    /// Floppy disk controller.
    pub floppy: FloppyController,
    /// Audio subsystem.
    pub audio: AudioState,
    /// Sprite engine.
    pub sprites: SpriteEngine,
    /// RGB565 framebuffer (320×256).
    pub framebuffer: Vec<u16>,
    /// Whether a complete frame has been rendered.
    pub frame_ready: bool,
    /// Total CPU cycles executed since start.
    pub total_cycles: u64,
    /// Pending keyboard events (keycode, pressed).
    key_events: Vec<(u8, bool)>,
    /// Mouse delta X accumulator.
    mouse_dx: i16,
    /// Mouse delta Y accumulator.
    mouse_dy: i16,
    /// Mouse button state (left pressed).
    mouse_left: bool,
    /// Mouse button state (right pressed).
    mouse_right: bool,
    /// Disk DMA pointer (DSKPT register).
    pub dskpt: u32,
}

impl Emulator {
    /// Create a new emulator with the given memory configuration.
    ///
    /// Schedules the initial `HSync` event.
    #[must_use]
    pub fn new(config: MemoryConfig) -> Self {
        let mut events = EventScheduler::new();
        events.schedule(EventType::HSync, 227);

        Self {
            cpu: M68000::<Mc68000>::new(),
            memory: AmigaMemory::new(config),
            chipset: CustomChipState::new(),
            events,
            copper: CopperState::new(),
            playfield: PlayfieldState::new(),
            blitter: BlitterState::new(),
            floppy: FloppyController::new(),
            audio: AudioState::new(),
            sprites: SpriteEngine::new(),
            framebuffer: vec![0; FRAMEBUFFER_SIZE],
            frame_ready: false,
            total_cycles: 0,
            key_events: Vec::new(),
            mouse_dx: 0,
            mouse_dy: 0,
            mouse_left: false,
            mouse_right: false,
            dskpt: 0,
        }
    }

    /// Load Kickstart ROM data into memory.
    pub fn load_rom(&mut self, data: &[u8]) {
        self.memory.load_rom(data);
    }

    /// Insert an ADF disk image into the specified floppy drive (0–3).
    pub fn insert_floppy(&mut self, drive: usize, data: Vec<u8>) {
        self.floppy.insert_disk(drive, data);
    }

    /// Queue a keyboard event for CIA handling.
    pub fn key_event(&mut self, keycode: u8, pressed: bool) {
        if self.key_events.len() < MAX_KEY_EVENTS {
            self.key_events.push((keycode, pressed));
        }
    }

    /// Accumulate mouse movement deltas.
    pub fn mouse_move(&mut self, dx: i16, dy: i16) {
        self.mouse_dx = self.mouse_dx.saturating_add(dx);
        self.mouse_dy = self.mouse_dy.saturating_add(dy);
    }

    /// Set mouse button state.
    pub fn mouse_button(&mut self, left: bool, right: bool) {
        self.mouse_left = left;
        self.mouse_right = right;
    }

    /// Run one full PAL frame (312 scanlines).
    pub fn run_frame(&mut self) {
        self.frame_ready = false;
        for _ in 0..SCANLINES_PAL {
            self.run_scanline();
        }
    }

    /// Execute one scanline worth of emulation.
    #[allow(clippy::too_many_lines)]
    pub fn run_scanline(&mut self) {
        // Sync readable registers into memory so CPU reads correct values
        self.sync_readable_regs();

        // Execute CPU instructions for this scanline
        let mut cycles_used: usize = 0;
        while cycles_used < CYCLES_PER_LINE {
            let c = self.cpu.interpreter(&mut self.memory);
            if c == 0 {
                // CPU is in STOP state (waiting for interrupt) — consume remaining cycles
                cycles_used = CYCLES_PER_LINE;
                break;
            }
            cycles_used += c;
        }
        self.total_cycles += cycles_used as u64;

        // Dispatch CPU register writes to subsystems
        let writes: Vec<(u16, u16)> = self.memory.drain_reg_writes().collect();
        for (offset, value) in writes {
            self.dispatch_register_write(offset, value);
        }

        // Advance chipset beam by one full scanline
        self.chipset.hpos = 0;
        if self.chipset.vpos >= 311 {
            self.chipset.vpos = 0;
        } else {
            self.chipset.vpos += 1;
        }
        let vpos = self.chipset.vpos;

        // Run copper for this scanline
        if self.copper.enabled {
            let chip_ram = self.memory.chip_ram();
            let mut copper_writes = Vec::new();
            for h in 0u16..227 {
                if let Some(action) = self.copper.cycle(chip_ram, vpos, h) {
                    match action {
                        CopperAction::WriteRegister { offset, value } => {
                            copper_writes.push((offset, value));
                        }
                    }
                }
            }
            for (offset, value) in copper_writes {
                self.dispatch_register_write(offset, value);
            }
        }

        // Sync copper palette changes to playfield color array
        self.playfield.color = self.chipset.color;

        // CIA E-clock: ~45 ticks per scanline (709379 Hz / 15625 Hz)
        for _ in 0..45 {
            if self.memory.cia.cia_a.tick() {
                self.chipset.intreq |= custom::INT_PORTS;
            }
            if self.memory.cia.cia_b.tick() {
                self.chipset.intreq |= custom::INT_EXTER;
            }
        }
        self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;

        // CIA-B TOD clocked by HSync (every scanline)
        self.memory.cia.cia_b.tick_tod();

        // Disk index pulse: only fires when a disk is present and spinning.
        // Without a disk, no index hole exists so no pulse is generated.
        // This is critical: without index pulses, trackdisk.device times out
        // and the boot code shows the "insert disk" hand.
        let motor_on = self.memory.cia.cia_b.prb & 0x80 == 0;
        let drive_selected = self.memory.cia.cia_b.prb & 0x78 != 0x78;
        let has_disk = self.floppy.drives[0].data.is_some(); // TODO: check selected drive
        if motor_on && drive_selected && has_disk && self.chipset.vpos == 0 {
            // Fire index pulse once per frame (~20ms, faster than real but sufficient)
            self.memory.cia.cia_b.icr_data |= 0x10; // FLAG bit
            if self.memory.cia.cia_b.icr_mask & 0x10 != 0 {
                self.chipset.intreq |= custom::INT_EXTER;
                self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
            }
            // Also fire DSKSYNC (bit 12) to unblock trackdisk waiting for sync word
            if self.chipset.dmaen(custom::DMA_DISK) {
                self.chipset.intreq |= 0x1000; // DSKSYNC
                self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
            }
        }

        // Process pending key events into CIA-A serial data register
        if let Some((keycode, pressed)) = self.key_events.first().copied() {
            // Amiga keyboard protocol: bit 7 = 0 for press, 1 for release
            let code = if pressed { keycode } else { keycode | 0x80 };
            self.memory.cia.cia_a.sdr = code;
            self.key_events.remove(0);
        }

        // Floppy DMA: when disk DMA is active and drive has data, transfer
        if self.floppy.dma_active && self.chipset.dmaen(crate::custom::DMA_DISK) {
            let dma_ptr = self.dskpt;
            let chip_ram = self.memory.chip_ram_mut();
            self.floppy.read_track_to_ram(chip_ram, dma_ptr);
            self.floppy.dma_active = false;
            self.floppy.dma_done = true;
        }

        // Render this scanline if in visible area
        if vpos < VISIBLE_LINES {
            let mut line_buffer = [0u16; DISPLAY_WIDTH];
            let chip_ram = self.memory.chip_ram();
            self.playfield
                .render_scanline(vpos, chip_ram, &mut line_buffer);
            let offset = usize::from(vpos) * DISPLAY_WIDTH;
            self.framebuffer[offset..offset + DISPLAY_WIDTH].copy_from_slice(&line_buffer);
        }

        // VBlank handling
        if vpos == 0 {
            // Only fire VBlank interrupt after system is initialized.
            // During InitCode, VBlank interrupts disrupt the sequential module
            // initialization order (graphics.library must complete before trackdisk).
            // Fire VBlank only after the initial delay loop completes (~2.4M cycles)
            // but this must be BEFORE InitCode processes resident modules.
            // Actually: VBlank should ALWAYS fire (it's hardware). The real fix is
            // that the VBlank handler must not disrupt InitCode's sequential processing.
            // For now, always fire VBlank - the real bug is elsewhere.
            {
                self.chipset.intreq |= custom::INT_VERTB;
                self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
            }
            self.copper.restart_vertical_blank();
            self.frame_ready = true;
            // CIA-A TOD clocked by VSync (once per frame)
            self.memory.cia.cia_a.tick_tod();
            // Reset mouse deltas at frame boundary
            self.mouse_dx = 0;
            self.mouse_dy = 0;
        }

        // Sync INTREQR/INTENAR so the CPU reads correct values in interrupt handlers
        self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
        self.memory.custom_regs[(custom::INTENAR / 2) as usize] = self.chipset.intena;

        // Deliver pending interrupts to CPU.
        // Only assert when there are enabled pending interrupts.
        // The m68000 crate handles priority masking internally via SR.interrupt_mask.
        // The CPU wakes from STOP when an interrupt is asserted (even if masked).
        let pending = self.chipset.intreq & self.chipset.intena & 0x3FFF;
        if pending != 0 && (self.chipset.intena & custom::INT_SETCLR) != 0 {
            let level = self.chipset.interrupt_level();
            if level > 0 {
                use m68000::exception::{Exception, Vector};
                let vector = match level {
                    1 => Vector::Level1Interrupt,
                    2 => Vector::Level2Interrupt,
                    3 => Vector::Level3Interrupt,
                    4 => Vector::Level4Interrupt,
                    5 => Vector::Level5Interrupt,
                    6 => Vector::Level6Interrupt,
                    _ => Vector::Level7Interrupt,
                };
                // The BTreeSet in m68000 deduplicates — safe to call every scanline.
                // The CPU only processes it if level > SR.interrupt_mask.
                self.cpu.exception(Exception::from(vector));
            }
        }
    }

    /// Sync live chipset state into the custom register shadow so CPU reads are correct.
    fn sync_readable_regs(&mut self) {
        use crate::custom;
        let regs = &mut self.memory.custom_regs;
        // VPOSR: bit 15 = LOF (long frame), bits 0-2 = vpos high bits
        // OCS PAL: no Agnus ID bits set. NTSC would have $1000.
        regs[(custom::VPOSR / 2) as usize] = 0x8000 | ((self.chipset.vpos >> 8) & 1);
        regs[(custom::VHPOSR / 2) as usize] = (self.chipset.vpos << 8) | (self.chipset.hpos & 0xFF);
        regs[(custom::DMACONR / 2) as usize] = self.chipset.dmacon;
        regs[(custom::INTENAR / 2) as usize] = self.chipset.intena;
        regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
    }

    /// Dispatch a single custom chip register write to the appropriate subsystem.
    #[allow(clippy::cast_possible_truncation)]
    pub fn dispatch_register_write(&mut self, offset: u16, value: u16) {
        use crate::custom;
        match offset {
            custom::BPLCON0 => self.playfield.bplcon0 = value,
            custom::BPLCON1 => self.playfield.bplcon1 = value,
            custom::BPLCON2 => self.playfield.bplcon2 = value,
            custom::DIWSTRT => self.playfield.diwstrt = value,
            custom::DIWSTOP => self.playfield.diwstop = value,
            custom::DDFSTRT => self.playfield.ddfstrt = value,
            custom::DDFSTOP => self.playfield.ddfstop = value,
            custom::DMACON => {
                self.chipset.write_register(offset, value);
                self.copper.enabled = self.chipset.dmaen(custom::DMA_COPPER);
            }
            custom::INTENA | custom::INTREQ => {
                self.chipset.write_register(offset, value);
                self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
                self.memory.custom_regs[(custom::INTENAR / 2) as usize] = self.chipset.intena;
            }
            custom::COP1LCH => {
                self.copper.cop1lc = (self.copper.cop1lc & 0x0000_FFFF) | (u32::from(value) << 16);
                self.copper.cop1lc &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::COP1LCL => {
                self.copper.cop1lc = (self.copper.cop1lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                self.copper.cop1lc &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::COP2LCH => {
                self.copper.cop2lc = (self.copper.cop2lc & 0x0000_FFFF) | (u32::from(value) << 16);
                self.copper.cop2lc &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::COP2LCL => {
                self.copper.cop2lc = (self.copper.cop2lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                self.copper.cop2lc &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::COPJMP1 => self.copper.strobe_cop1(),
            custom::COPJMP2 => self.copper.strobe_cop2(),
            custom::DSKLEN => self.floppy.write_dsklen(value),
            custom::DSKPTH => {
                self.dskpt = (self.dskpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom::DSKPTL => {
                self.dskpt = (self.dskpt & 0xFFFF_0000) | u32::from(value);
            }
            o if (custom::COLOR00..=custom::COLOR31).contains(&o) => {
                self.chipset.write_register(o, value);
                let idx = ((o - custom::COLOR00) / 2) as usize;
                self.playfield.color[idx] = value & 0x0FFF;
            }
            o if (custom::BPL1PTH..=custom::BPL6PTL).contains(&o) => {
                let reg_idx = ((o - custom::BPL1PTH) / 2) as usize;
                let plane = reg_idx / 2;
                if plane < self.playfield.bplpt.len() {
                    if reg_idx & 1 == 0 {
                        self.playfield.bplpt[plane] =
                            (self.playfield.bplpt[plane] & 0x0000_FFFF) | (u32::from(value) << 16);
                    } else {
                        self.playfield.bplpt[plane] =
                            (self.playfield.bplpt[plane] & 0xFFFF_0000) | u32::from(value);
                    }
                }
            }
            o if (custom::BLTCON0..=custom::BLTSIZE).contains(&o) => {
                self.dispatch_blitter_write(o, value);
            }
            _ => {}
        }
    }

    /// Dispatch blitter register writes.
    fn dispatch_blitter_write(&mut self, offset: u16, value: u16) {
        use crate::custom;
        match offset {
            custom::BLTCON0 => self.blitter.bltcon0 = value,
            custom::BLTCON1 => self.blitter.bltcon1 = value,
            custom::BLTAFWM => self.blitter.bltafwm = value,
            custom::BLTALWM => self.blitter.bltalwm = value,
            custom::BLTCPTH => {
                self.blitter.bltcpt =
                    (self.blitter.bltcpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom::BLTCPTL => {
                self.blitter.bltcpt = (self.blitter.bltcpt & 0xFFFF_0000) | u32::from(value);
            }
            custom::BLTBPTH => {
                self.blitter.bltbpt =
                    (self.blitter.bltbpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom::BLTBPTL => {
                self.blitter.bltbpt = (self.blitter.bltbpt & 0xFFFF_0000) | u32::from(value);
            }
            custom::BLTAPTH => {
                self.blitter.bltapt =
                    (self.blitter.bltapt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom::BLTAPTL => {
                self.blitter.bltapt = (self.blitter.bltapt & 0xFFFF_0000) | u32::from(value);
            }
            custom::BLTDPTH => {
                self.blitter.bltdpt =
                    (self.blitter.bltdpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom::BLTDPTL => {
                self.blitter.bltdpt = (self.blitter.bltdpt & 0xFFFF_0000) | u32::from(value);
            }
            custom::BLTSIZE => {
                self.blitter.bltsize = value;
                self.blitter.start_blit();
                let chip_ram = self.memory.chip_ram_mut();
                self.blitter.execute_blit(chip_ram);
            }
            _ => {}
        }
    }

    /// Get the current framebuffer contents.
    #[must_use]
    pub fn framebuffer(&self) -> &[u16] {
        &self.framebuffer
    }

    /// Returns `true` if a complete frame has been rendered.
    #[must_use]
    pub const fn is_frame_ready(&self) -> bool {
        self.frame_ready
    }

    /// Clear the frame-ready flag after consuming the frame.
    pub fn clear_frame_ready(&mut self) {
        self.frame_ready = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_valid_state() {
        let emu = Emulator::new(MemoryConfig::a500());
        assert_eq!(emu.framebuffer.len(), FRAMEBUFFER_SIZE);
        assert!(!emu.frame_ready);
        assert_eq!(emu.total_cycles, 0);
        assert!(emu.events.is_pending(EventType::HSync));
    }

    #[test]
    fn load_rom_and_cpu_reads_reset_vector() {
        let mut emu = Emulator::new(MemoryConfig::a500());

        // Build a minimal ROM: SSP at 0x0000_0800, PC at 0x00FC_0008
        let mut rom = vec![0u8; 256 * 1024];
        // Initial SSP (address 0x00000000 via overlay)
        rom[0] = 0x00;
        rom[1] = 0x00;
        rom[2] = 0x08;
        rom[3] = 0x00;
        // Initial PC (address 0x00000004 via overlay)
        rom[4] = 0x00;
        rom[5] = 0xFC;
        rom[6] = 0x00;
        rom[7] = 0x08;
        // At PC=0x00FC0008 (ROM offset 8): NOP (0x4E71)
        rom[8] = 0x4E;
        rom[9] = 0x71;

        emu.load_rom(&rom);

        // Execute one instruction — the CPU should process the reset exception
        // and then execute the NOP at the reset vector PC.
        let c = emu.cpu.interpreter(&mut emu.memory);
        assert!(c > 0);

        // After reset processing, PC should be at 0x00FC0008 or past it
        let pc = emu.cpu.regs.pc.0;
        // The reset handler fetches SSP and PC, then starts executing at the
        // reset PC. After one interpreter call the PC should be at or past
        // the reset vector address.
        assert!(
            pc >= 0x00FC_0008,
            "PC should point to reset vector address, got {pc:#010X}"
        );
    }
}
