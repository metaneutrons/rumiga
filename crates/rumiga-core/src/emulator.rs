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
use crate::cia::CiaPair;
use crate::copper::{CopperAction, CopperState};
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
    /// CIA pair (A and B).
    pub cia: CiaPair,
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
            cia: CiaPair::new(),
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
    pub fn run_scanline(&mut self) {
        // Execute CPU instructions for this scanline
        let mut cycles_used: usize = 0;
        while cycles_used < CYCLES_PER_LINE {
            let c = self.cpu.interpreter(&mut self.memory);
            cycles_used += c;
        }
        self.total_cycles += cycles_used as u64;

        // Advance chipset beam by one full line
        self.chipset.advance_beam();
        let vpos = self.chipset.vpos;
        let hpos = self.chipset.hpos;

        // Run copper for this scanline
        if self.copper.enabled {
            let chip_ram = self.memory.chip_ram();
            for _ in 0..227 {
                if let Some(action) = self.copper.cycle(chip_ram, vpos, hpos) {
                    match action {
                        CopperAction::WriteRegister { offset, value } => {
                            self.chipset.write_register(offset, value);
                        }
                    }
                }
            }
        }

        // Sync copper palette changes to playfield color array
        self.playfield.color = self.chipset.color;

        // Tick CIA timers
        self.cia.cia_a.tick();
        self.cia.cia_b.tick();

        // Process pending key events into CIA-A serial data register
        if let Some((keycode, pressed)) = self.key_events.first().copied() {
            // Amiga keyboard protocol: bit 7 = 0 for press, 1 for release
            let code = if pressed { keycode } else { keycode | 0x80 };
            self.cia.cia_a.sdr = code;
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
            self.copper.restart_vertical_blank();
            self.frame_ready = true;
            // Reset mouse deltas at frame boundary
            self.mouse_dx = 0;
            self.mouse_dy = 0;
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
