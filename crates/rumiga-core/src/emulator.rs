// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Main emulation loop tying CPU and chipset together.

use r68k_emu::cpu::ConfiguredCore;
pub use r68k_emu::cpu::ProcessingState;
use r68k_emu::interrupts::InterruptController;

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

/// Interrupt controller that bridges r68k to the Amiga chipset interrupt system.
///
/// The actual interrupt level is computed from `CustomChipState` and injected
/// each scanline via [`AmigaInterruptController::set_level`].
pub struct AmigaInterruptController {
    level: u8,
}

impl AmigaInterruptController {
    const fn new() -> Self {
        Self { level: 0 }
    }

    /// Update the pending interrupt level from chipset state.
    pub fn set_level(&mut self, level: u8) {
        self.level = level;
    }
}

impl InterruptController for AmigaInterruptController {
    fn reset_external_devices(&mut self) {
        self.level = 0;
    }

    fn highest_priority(&self) -> u8 {
        self.level
    }

    fn acknowledge_interrupt(&mut self, priority: u8) -> Option<u8> {
        // Autovector: vector = 24 + priority level
        // Don't clear level here — the Amiga holds the interrupt line asserted
        // until software clears INTREQ. The CPU's SR IPL mask prevents re-entry.
        Some(24 + priority)
    }
}

/// The configured r68k CPU type used by the emulator.
pub type AmigaCpu = ConfiguredCore<AmigaInterruptController, AmigaMemory>;

/// Cycle threshold after which CIA timers are force-started if still stopped (~frame 160).
///
/// On Kickstart 1.3, timer.device should start the CIA timers during `InitCode`.
/// Due to an unresolved emulation issue in the cia.resource `AddICRVector` path,
/// the timers may remain stopped. This threshold triggers a one-time workaround
/// that starts them, enabling timer-based boot timeouts.
const FORCE_CIA_TIMER_THRESHOLD: u64 = 22_000_000;

/// Main emulator state combining CPU and all chipset subsystems.
pub struct Emulator {
    /// r68k CPU core (owns memory as its `AddressBus`).
    pub cpu: AmigaCpu,
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

        let memory = AmigaMemory::new(config);
        let int_ctrl = AmigaInterruptController::new();
        let mut cpu = ConfiguredCore::new_with(0, int_ctrl, memory);
        cpu.reset();

        Self {
            cpu,
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
    ///
    /// Re-resets the CPU so it picks up the correct reset vectors from the new ROM.
    pub fn load_rom(&mut self, data: &[u8]) {
        self.cpu.mem.load_rom(data);
        self.cpu.reset();
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
        self.cpu.execute1();
    }

    /// Execute one scanline worth of emulation.
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn run_scanline(&mut self) {
        // Sync readable registers into memory so CPU reads correct values
        self.sync_readable_regs();

        // Execute CPU instructions for this scanline
        let mut cycles_used: usize = 0;
        while cycles_used < CYCLES_PER_LINE {
            // Sync interrupt registers so CPU reads see current state
            self.cpu.mem.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq & 0x7FFF;
            self.cpu.mem.custom_regs[(custom::INTENAR / 2) as usize] = self.chipset.intena & 0x7FFF;

            // Update interrupt level for r68k's interrupt controller
            let pending = self.chipset.intreq & self.chipset.intena & 0x3FFF;
            if pending != 0 && (self.chipset.intena & custom::INT_SETCLR) != 0 {
                self.cpu.int_ctrl.set_level(self.chipset.interrupt_level());
            } else {
                self.cpu.int_ctrl.set_level(0);
            }

            let c = self.cpu.execute1();
            if c.0 <= 0 || self.cpu.processing_state == ProcessingState::Stopped {
                cycles_used = CYCLES_PER_LINE;
                break;
            }
            cycles_used += c.0.unsigned_abs() as usize;
            // Update HPOS based on cycles consumed (2 CPU cycles = 1 color clock)
            self.chipset.hpos = u16::try_from((cycles_used / 2).min(226)).unwrap_or(226);
            // Sync beam position so CPU reads of VHPOSR see advancing hpos
            self.cpu.mem.custom_regs[(custom::VHPOSR / 2) as usize] =
                (self.chipset.vpos << 8) | (self.chipset.hpos & 0xFF);
            // Dispatch register writes immediately
            let writes: Vec<(u16, u16)> = self.cpu.mem.drain_reg_writes().collect();
            for (offset, value) in writes {
                self.dispatch_register_write(offset, value);
            }
            // Handle CIA-B PRB writes (disk drive selection/motor/step)
            if self.cpu.mem.cia_b_prb_dirty {
                self.cpu.mem.cia_b_prb_dirty = false;
                let prb = self.cpu.mem.cia.borrow().cia_b.prb;
                self.floppy.disk_select(prb);
                let mut st: u8 = 0x3C; // default: all high
                if self.floppy.at_track0() {
                    st &= !0x10;
                }
                if !self.floppy.has_disk() {
                    st &= !0x04; // DSKCHANGE=0 (no disk)
                }
                // DSKRDY (bit 5):
                // - No drive selected: HIGH (not ready)
                // - Drive selected, motor on, disk present: LOW (ready)
                // - Drive selected, motor on, no disk: HIGH (not ready)
                // - Drive selected, motor off: shows drive ID (LOW for std DD)
                if self.floppy.any_drive_selected() {
                    if self.floppy.motor_on() {
                        if self.floppy.has_disk() {
                            st &= !0x20; // Ready
                        }
                    } else {
                        // Motor off: drive ID bit (0 for standard DD)
                        if self.floppy.drive_id_bit() == 0 {
                            st &= !0x20;
                        }
                    }
                }
                self.cpu.mem.disk_status = st;
            }
            // Run a disk DMA cycle per instruction
            if self.chipset.dmaen(crate::custom::DMA_DISK) {
                let chip_ram = self.cpu.mem.chip_ram_mut();
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
            let chip_ram = self.cpu.mem.chip_ram();
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
            let regs = &self.cpu.mem.custom_regs;
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
            let chip_ram = self.cpu.mem.chip_ram();
            self.playfield
                .render_scanline(vpos, chip_ram, &mut line_buffer);
            let offset = usize::from(vpos) * DISPLAY_WIDTH;
            self.framebuffer[offset..offset + DISPLAY_WIDTH].copy_from_slice(&line_buffer);
        }

        // CIA E-clock: ~45 ticks per scanline (709379 Hz / 15625 Hz)
        for _ in 0..45 {
            let mut cia = self.cpu.mem.cia.borrow_mut();
            if cia.cia_a.tick() {
                self.chipset.intreq |= custom::INT_PORTS;
            }
            if cia.cia_b.tick() {
                self.chipset.intreq |= custom::INT_EXTER;
            }
        }
        // Also fire INT_EXTER if CIA-B has any masked interrupt pending (e.g. FLAG)
        {
            let cia = self.cpu.mem.cia.borrow();
            if cia.cia_b.icr_ir {
                self.chipset.intreq |= custom::INT_EXTER;
            }
        }
        self.cpu.mem.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;

        // CIA-B TOD clocked by HSync (every scanline)
        self.cpu.mem.cia.borrow_mut().cia_b.tick_tod();

        // Disk index pulse: only fires when a disk is present and spinning.
        // Without a disk, no index hole exists so no pulse is generated.
        // This is critical: without index pulses, trackdisk.device times out
        // and the boot code shows the "insert disk" hand.
        // Disk index pulse: fires once per revolution when motor is spinning.
        // Use raw CIA-B PRB bit 7 (0=motor on) since floppy.motor_on() may not
        // reflect the state during init (disk_select hasn't processed it yet).
        if self.cpu.mem.cia.borrow().cia_b.prb & 0x80 == 0 && self.chipset.vpos == 0 {
            // Fire index pulse once per revolution (~300ms real, once per frame here)
            let mut cia = self.cpu.mem.cia.borrow_mut();
            cia.cia_b.icr_data |= 0x10; // FLAG bit
            if cia.cia_b.icr_mask & 0x10 != 0 {
                self.chipset.intreq |= custom::INT_EXTER;
                self.cpu.mem.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
            }
        }

        // Process pending key events into CIA-A serial data register
        if let Some((keycode, pressed)) = self.key_events.first().copied() {
            // Amiga keyboard protocol: bit 7 = 0 for press, 1 for release
            let code = if pressed { keycode } else { keycode | 0x80 };
            self.cpu.mem.cia.borrow_mut().cia_a.sdr = code;
            self.key_events.remove(0);
        }

        // Floppy DMA: run ~32 word cycles per scanline
        if self.chipset.dmaen(crate::custom::DMA_DISK) {
            let chip_ram = self.cpu.mem.chip_ram_mut();
            for _ in 0..32 {
                self.floppy.disk_dma_cycle(chip_ram);
            }
            // Deliver pending disk interrupts
            if self.floppy.pending_sync_irq {
                self.floppy.pending_sync_irq = false;
                self.chipset.intreq |= 0x1000; // DSKSYNC
                self.cpu.mem.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
            }
            if self.floppy.pending_blk_irq {
                self.floppy.pending_blk_irq = false;
                self.chipset.intreq |= custom::INT_DSKBLK;
                self.cpu.mem.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
            }
        }

        // VBlank handling
        if vpos == 0 {
            {
                self.chipset.intreq |= custom::INT_VERTB;
                self.cpu.mem.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
            }
            self.copper.restart_vertical_blank();
            self.frame_ready = true;
            // CIA-A TOD clocked by VSync (once per frame)
            self.cpu.mem.cia.borrow_mut().cia_a.tick_tod();
            // Reset mouse deltas at frame boundary
            self.mouse_dx = 0;
            self.mouse_dy = 0;

            // Workaround: force-start CIA timers if timer.device failed to start them.
            if self.total_cycles > FORCE_CIA_TIMER_THRESHOLD
                && self.cpu.mem.cia.borrow().cia_b.cra & 0x01 == 0
            {
                let mut cia = self.cpu.mem.cia.borrow_mut();
                cia.cia_b.cra |= 0x01;
                cia.cia_b.icr_mask |= 0x01;
                if cia.cia_a.cra & 0x01 == 0 {
                    cia.cia_a.cra |= 0x01;
                    cia.cia_a.icr_mask |= 0x01;
                }
            }

            // TODO: Fix InitStruct offset bug that causes trackdisk's signal
            // bits to be misaligned (device port signals bit 9, task waits bit 10).
        }

        // Sync INTREQR/INTENAR so the CPU reads correct values in interrupt handlers
        self.cpu.mem.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
        self.cpu.mem.custom_regs[(custom::INTENAR / 2) as usize] = self.chipset.intena;

        // Update interrupt level for next scanline — r68k handles delivery internally
        let pending = self.chipset.intreq & self.chipset.intena & 0x3FFF;
        if pending != 0 && (self.chipset.intena & custom::INT_SETCLR) != 0 {
            self.cpu.int_ctrl.set_level(self.chipset.interrupt_level());
        } else {
            self.cpu.int_ctrl.set_level(0);
        }
    }

    /// Sync live chipset state into the custom register shadow so CPU reads are correct.
    fn sync_readable_regs(&mut self) {
        use crate::custom;
        let regs = &mut self.cpu.mem.custom_regs;
        // VPOSR: bit 15=LOF, bits 14-8=Agnus ID, bits 0-2=vpos high
        // OCS Agnus (A500): ID=$00, only bit 0 of vpos high visible
        regs[(custom::VPOSR / 2) as usize] = 0x8000 | ((self.chipset.vpos >> 8) & 1);
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
                self.cpu.mem.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
                self.cpu.mem.custom_regs[(custom::INTENAR / 2) as usize] = self.chipset.intena;
            }
            custom::COP1LCH => {
                self.copper.cop1lc = (self.copper.cop1lc & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom::COP1LCL => {
                self.copper.cop1lc = (self.copper.cop1lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                self.copper.cop1lc &= (self.cpu.mem.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::COP2LCH => {
                self.copper.cop2lc = (self.copper.cop2lc & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom::COP2LCL => {
                self.copper.cop2lc = (self.copper.cop2lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                self.copper.cop2lc &= (self.cpu.mem.chip_ram().len() as u32).wrapping_sub(1);
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
                let chip_ram = self.cpu.mem.chip_ram_mut();
                self.blitter.execute_blit(chip_ram);
                // Fire blitter-done interrupt
                self.chipset.intreq |= custom::INT_BLIT;
                self.cpu.mem.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
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

        // r68k resets in new_with, but we loaded ROM after construction.
        // Re-reset to pick up the new vectors.
        emu.cpu.reset();

        // After reset, PC should be at the reset vector address
        let pc = emu.cpu.pc;
        assert!(
            pc >= 0x00FC_0008,
            "PC should point to reset vector address, got {pc:#010X}"
        );
    }
}
