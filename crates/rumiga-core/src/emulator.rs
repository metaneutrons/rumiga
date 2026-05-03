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
        }
    }

    /// Load Kickstart ROM data into memory.
    pub fn load_rom(&mut self, data: &[u8]) {
        self.memory.load_rom(data);
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
        }

        // CIA tick
        self.cia.cia_a.tick();
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
