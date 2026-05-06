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

/// Cycle threshold after which CIA timers are force-started if still stopped (~frame 160).
///
/// On Kickstart 1.3, timer.device should start the CIA timers during `InitCode`.
/// Due to an unresolved emulation issue in the cia.resource `AddICRVector` path,
/// the timers may remain stopped. This threshold triggers a one-time workaround
/// that starts them, enabling timer-based boot timeouts.
const FORCE_CIA_TIMER_THRESHOLD: u64 = 22_000_000;

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

    /// Execute a single CPU instruction (for debugging/tracing).
    pub fn step_instruction(&mut self) {
        self.cpu.interpreter(&mut self.memory);
    }

    /// Execute one scanline worth of emulation.
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn run_scanline(&mut self) {
        // Sync readable registers into memory so CPU reads correct values
        self.sync_readable_regs();

        // Execute CPU instructions for this scanline
        let mut cycles_used: usize = 0;
        while cycles_used < CYCLES_PER_LINE {
            let c = self.cpu.interpreter(&mut self.memory);
            if c == 0 {
                cycles_used = CYCLES_PER_LINE;
                break;
            }
            cycles_used += c;
            // Update HPOS based on cycles consumed (2 CPU cycles = 1 color clock)
            self.chipset.hpos = u16::try_from((cycles_used / 2).min(226)).unwrap_or(226);
            // Sync beam position so CPU reads of VHPOSR see advancing hpos
            self.memory.custom_regs[(custom::VHPOSR / 2) as usize] =
                (self.chipset.vpos << 8) | (self.chipset.hpos & 0xFF);
            // Dispatch register writes immediately
            let writes: Vec<(u16, u16)> = self.memory.drain_reg_writes().collect();
            for (offset, value) in writes {
                self.dispatch_register_write(offset, value);
            }
            // Handle CIA-B PRB writes (disk drive selection/motor/step)
            if self.memory.cia_b_prb_dirty {
                self.memory.cia_b_prb_dirty = false;
                let prb = self.memory.cia.cia_b.prb;
                self.floppy.disk_select(prb);
                // Update disk status for CIA-A PRA reads
                // FS-UAE DISK_status_ciaa: start with $3C, clear bits per drive state
                let mut st: u8 = 0x3C;
                if self.floppy.at_track0() {
                    st &= !0x10; // bit 4: TRACK0 asserted
                }
                if !self.floppy.has_disk() {
                    // No disk: DSKCHANGE=0 (disk removed/never inserted)
                    st &= !0x04;
                } else if self.floppy.motor_on() {
                    st &= !0x20; // bit 5: RDY asserted (ready)
                }
                self.memory.disk_status = st;
            }
            // Run a disk DMA cycle per instruction (allows DMA to progress during DoIO)
            if self.chipset.dmaen(crate::custom::DMA_DISK) {
                let chip_ram = self.memory.chip_ram_mut();
                self.floppy.disk_dma_cycle(chip_ram);
                if self.floppy.pending_sync_irq {
                    self.floppy.pending_sync_irq = false;
                    self.chipset.intreq |= 0x1000;
                }
                if self.floppy.pending_blk_irq {
                    self.floppy.pending_blk_irq = false;
                    self.chipset.intreq |= custom::INT_DSKBLK;
                }
            }
            // Deliver pending interrupts within the scanline
            // (required for graphics.library init which waits for VBlank in a tight loop)
            let pending = self.chipset.intreq & self.chipset.intena & 0x3FFF;
            if pending != 0 && (self.chipset.intena & custom::INT_SETCLR) != 0 {
                let level = self.chipset.interrupt_level();
                if level > self.cpu.regs.sr.interrupt_mask {
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
                    self.cpu.exception(Exception::from(vector));
                }
            }
        }
        self.total_cycles += cycles_used as u64;

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
                            // COPJMP1/2 must be handled immediately (affects copper PC)
                            match offset {
                                custom::COPJMP1 => self.copper.strobe_cop1(),
                                custom::COPJMP2 => self.copper.strobe_cop2(),
                                _ => copper_writes.push((offset, value)),
                            }
                        }
                    }
                }
            }
            for (offset, value) in copper_writes {
                self.dispatch_register_write(offset, value);
            }
        }

        // Render this scanline AFTER copper sets up registers for this line
        if vpos < VISIBLE_LINES {
            let mut line_buffer = [0u16; DISPLAY_WIDTH];
            // Sync playfield state from shadow registers (copper has updated them)
            let regs = &self.memory.custom_regs;
            self.playfield.bplcon0 = regs[(custom::BPLCON0 / 2) as usize];
            self.playfield.bplcon1 = regs[(0x102 / 2) as usize];
            self.playfield.bplcon2 = regs[(0x104 / 2) as usize];
            self.playfield.diwstrt = regs[(0x08E / 2) as usize];
            self.playfield.diwstop = regs[(0x090 / 2) as usize];
            self.playfield.ddfstrt = regs[(0x092 / 2) as usize];
            self.playfield.ddfstop = regs[(0x094 / 2) as usize];
            for i in 0u16..6 {
                let h = u32::from(regs[(0x0E0 / 2 + i * 2) as usize]);
                let l = u32::from(regs[(0x0E2 / 2 + i * 2) as usize]);
                self.playfield.bplpt[usize::from(i)] = (h << 16) | l;
            }
            for i in 0usize..32 {
                let c = regs[0x180 / 2 + i];
                self.playfield.color[i] = c & 0x0FFF;
            }
            let chip_ram = self.memory.chip_ram();
            self.playfield
                .render_scanline(vpos, chip_ram, &mut line_buffer);
            let offset = usize::from(vpos) * DISPLAY_WIDTH;
            self.framebuffer[offset..offset + DISPLAY_WIDTH].copy_from_slice(&line_buffer);
        }

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
        if self.floppy.motor_on() && self.floppy.has_disk() && self.chipset.vpos == 0 {
            // Fire index pulse once per revolution (~300ms real, once per frame here)
            self.memory.cia.cia_b.icr_data |= 0x10; // FLAG bit
            if self.memory.cia.cia_b.icr_mask & 0x10 != 0 {
                self.chipset.intreq |= custom::INT_EXTER;
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

        // Floppy DMA: run ~32 word cycles per scanline (one word every ~7 hpos)
        // Real hardware: one word every 2µs = ~113 words per scanline at PAL timing.
        // We run fewer to avoid over-speeding, but enough for timely completion.
        if self.chipset.dmaen(crate::custom::DMA_DISK) {
            let chip_ram = self.memory.chip_ram_mut();
            for _ in 0..32 {
                self.floppy.disk_dma_cycle(chip_ram);
            }
            // Deliver pending disk interrupts
            if self.floppy.pending_sync_irq {
                self.floppy.pending_sync_irq = false;
                self.chipset.intreq |= 0x1000; // DSKSYNC
                self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
            }
            if self.floppy.pending_blk_irq {
                self.floppy.pending_blk_irq = false;
                self.chipset.intreq |= custom::INT_DSKBLK;
                self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
            }
        }

        // VBlank handling
        if vpos == 0 {
            // Always fire VBlank (hardware signal).
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

            // Workaround: force-start CIA timers if timer.device failed to start them.
            //
            // On Kickstart 1.3, timer.device's init calls cia.resource's
            // `AddICRVector` to claim CIA timer interrupts. Due to an
            // unresolved emulation issue in the cia.resource init path, this
            // call fails and the timers are never started. Without running
            // timers, the boot process cannot time out and show the
            // "insert disk" hand.
            //
            // We detect this condition once after InitCode completes (~frame
            // 160) and start the timers with a standard latch value if they
            // are still stopped.
            if self.total_cycles > FORCE_CIA_TIMER_THRESHOLD
                && self.memory.cia.cia_b.cra & 0x01 == 0
            {
                // CIA-B Timer A: used by timer.device for ECLOCK timing.
                // Standard latch = $FFFF, continuous mode.
                self.memory.cia.cia_b.cra |= 0x01; // START
                self.memory.cia.cia_b.icr_mask |= 0x01; // Enable Timer A interrupt
                // CIA-A Timer A: used by timer.device for MICROHZ timing.
                if self.memory.cia.cia_a.cra & 0x01 == 0 {
                    self.memory.cia.cia_a.cra |= 0x01;
                    self.memory.cia.cia_a.icr_mask |= 0x01;
                }
            }
        }

        // Sync INTREQR/INTENAR so the CPU reads correct values in interrupt handlers
        self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
        self.memory.custom_regs[(custom::INTENAR / 2) as usize] = self.chipset.intena;

        // Deliver pending interrupts to CPU.
        let pending = self.chipset.intreq & self.chipset.intena & 0x3FFF;
        if pending != 0 && (self.chipset.intena & custom::INT_SETCLR) != 0 {
            let level = self.chipset.interrupt_level();
            if level > self.cpu.regs.sr.interrupt_mask {
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
                self.cpu.exception(Exception::from(vector));
            }
        }
    }

    /// Sync live chipset state into the custom register shadow so CPU reads are correct.
    fn sync_readable_regs(&mut self) {
        use crate::custom;
        let regs = &mut self.memory.custom_regs;
        // VPOSR: bit 15=LOF, bits 14-8=Agnus ID, bits 0-2=vpos high
        // ECS Agnus (A500+): ID=$20 → bits 12-8 = $20 → VPOSR has $2000
        regs[(custom::VPOSR / 2) as usize] = 0x8000 | 0x2000 | ((self.chipset.vpos >> 8) & 1);
        regs[(custom::VHPOSR / 2) as usize] = (self.chipset.vpos << 8) | (self.chipset.hpos & 0xFF);
        regs[(custom::DMACONR / 2) as usize] = self.chipset.dmacon & 0x7FFF;
        regs[(custom::INTENAR / 2) as usize] = self.chipset.intena & 0x7FFF;
        regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq & 0x7FFF;
        // SERDATR ($018): TBE (bit 13) + TSRE (bit 12) = transmit buffer empty
        regs[(0x018 / 2) as usize] = 0x3000;
        // POTGOR ($016): active-high button state (bits 8-15 = all buttons released)
        regs[(0x016 / 2) as usize] = 0xFF00;
        // JOY0DAT ($00A): no joystick movement
        regs[(0x00A / 2) as usize] = 0x0000;
        // JOY1DAT ($00C): no joystick movement
        regs[(0x00C / 2) as usize] = 0x0000;
        // DENISEID ($07C): OCS Denise returns $FFFF (register doesn't exist)
        regs[(0x07C / 2) as usize] = 0xFFFF;
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
            }
            custom::COP1LCL => {
                self.copper.cop1lc = (self.copper.cop1lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                self.copper.cop1lc &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::COP2LCH => {
                self.copper.cop2lc = (self.copper.cop2lc & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom::COP2LCL => {
                self.copper.cop2lc = (self.copper.cop2lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                self.copper.cop2lc &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::COPJMP1 => self.copper.strobe_cop1(),
            custom::COPJMP2 => self.copper.strobe_cop2(),
            custom::DSKLEN => self.floppy.write_dsklen(value),
            custom::DSKSYNC => self.floppy.write_dsksync(value),
            custom::DSKPTH => {
                self.floppy.dskpt = (self.floppy.dskpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom::DSKPTL => {
                self.floppy.dskpt = (self.floppy.dskpt & 0xFFFF_0000) | u32::from(value);
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
