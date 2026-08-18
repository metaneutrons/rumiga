// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Custom chip state and register read/write dispatcher.
//!
//! Implements the OCS custom chip register file with DMACON, INTENA/INTREQ
//! logic, and beam position tracking.

use crate::custom;
use crate::video::VideoStandard;

/// Number of color palette entries (OCS).
const PALETTE_SIZE: usize = 32;

/// Maximum horizontal position (color clocks per line, 0-indexed).
const HPOS_MAX: u16 = 226;

/// Custom chip register state.
#[derive(Clone, Debug)]
pub struct CustomChipState {
    /// DMA control register (active value, without set/clear bit).
    pub dmacon: u16,
    /// Interrupt enable register (active value, without set/clear bit).
    pub intena: u16,
    /// Interrupt request register (active value, without set/clear bit).
    pub intreq: u16,
    /// Vertical beam position (0–311 PAL, 0–261 NTSC).
    pub vpos: u16,
    /// Horizontal beam position (0–226 color clocks).
    pub hpos: u16,
    /// Color palette registers.
    pub color: [u16; PALETTE_SIZE],
    /// Video standard that fixes the beam geometry.
    video_standard: VideoStandard,
}

impl Default for CustomChipState {
    fn default() -> Self {
        Self::new(VideoStandard::Pal)
    }
}

impl CustomChipState {
    /// Create a new chip state with all registers zeroed.
    ///
    /// The video standard fixes where the beam wraps and what `VPOSR` and
    /// `BEAMCON0` report, so it belongs to the chip state rather than to the
    /// caller of each read.
    #[must_use]
    pub const fn new(video_standard: VideoStandard) -> Self {
        Self {
            dmacon: 0,
            intena: 0,
            intreq: 0,
            vpos: 0,
            hpos: 0,
            color: [0; PALETTE_SIZE],
            video_standard,
        }
    }

    /// Video standard that fixes the beam geometry.
    #[must_use]
    pub const fn video_standard(&self) -> VideoStandard {
        self.video_standard
    }

    /// Value the chipset reports in `VPOSR`.
    ///
    /// Bit 15 is `LOF`, bits 14–8 carry the Agnus identification, of which bit 12
    /// reports NTSC, and the low bits carry the high bits of the beam position.
    /// On OCS Agnus only bit 0 of the beam high bits is visible, and the Agnus
    /// revision itself is not yet modelled.
    ///
    /// One implementation serves both the register shadow the guest reads and the
    /// direct register read, so the two cannot disagree.
    #[must_use]
    pub const fn vposr(&self) -> u16 {
        0x8000 | self.video_standard.vposr_standard_bits() | ((self.vpos >> 8) & 1)
    }

    /// Check if a DMA channel is enabled (master enable + channel bit).
    #[must_use]
    pub const fn dmaen(&self, channel_mask: u16) -> bool {
        (self.dmacon & channel_mask) != 0 && (self.dmacon & custom::DMA_MASTER) != 0
    }

    /// Compute the highest pending interrupt level (1–6), or 0 if none.
    ///
    /// Amiga interrupt priority mapping:
    /// - Level 1: TBE, DSKBLK, SOFT
    /// - Level 2: PORTS (CIA-A)
    /// - Level 3: COPER, VERTB, BLIT
    /// - Level 4: AUD0, AUD1, AUD2, AUD3
    /// - Level 5: RBF, DSKSYN
    /// - Level 6: EXTER (CIA-B)
    #[must_use]
    pub const fn interrupt_level(&self) -> u8 {
        let pending = self.intreq & self.intena;
        if pending == 0 {
            return 0;
        }
        if pending & custom::INT_EXTER != 0 {
            6
        } else if pending & (custom::INT_RBF | custom::INT_DSKSYN) != 0 {
            5
        } else if pending
            & (custom::INT_AUD0 | custom::INT_AUD1 | custom::INT_AUD2 | custom::INT_AUD3)
            != 0
        {
            4
        } else if pending & (custom::INT_COPER | custom::INT_VERTB | custom::INT_BLIT) != 0 {
            3
        } else if pending & custom::INT_PORTS != 0 {
            2
        } else {
            1
        }
    }

    /// Advance beam position by one color clock. Returns true on vertical wrap.
    pub fn advance_beam(&mut self) -> bool {
        if self.hpos >= HPOS_MAX {
            self.hpos = 0;
            if self.vpos >= self.video_standard.last_line() {
                self.vpos = 0;
                return true;
            }
            self.vpos += 1;
        } else {
            self.hpos += 1;
        }
        false
    }

    /// Read a custom chip register by offset (0x000–0x1FE).
    #[must_use]
    pub fn read_register(&self, offset: u16) -> u16 {
        match offset {
            custom::DMACONR => self.dmacon,
            custom::VPOSR => self.vposr(),
            custom::VHPOSR => (self.vpos << 8) | (self.hpos & 0xFF),
            custom::BEAMCON0 => self.video_standard.beamcon0(),
            custom::INTENAR => self.intena,
            custom::INTREQR => self.intreq,
            o if (custom::COLOR00..=custom::COLOR31).contains(&o) => {
                let idx = ((o - custom::COLOR00) / 2) as usize;
                self.color[idx]
            }
            _ => 0,
        }
    }

    /// Write a custom chip register by offset (0x000–0x1FE).
    pub fn write_register(&mut self, offset: u16, value: u16) {
        match offset {
            custom::DMACON => apply_set_clear(&mut self.dmacon, value),
            custom::INTENA => apply_set_clear(&mut self.intena, value),
            custom::INTREQ => apply_set_clear(&mut self.intreq, value),
            o if (custom::COLOR00..=custom::COLOR31).contains(&o) => {
                let idx = ((o - custom::COLOR00) / 2) as usize;
                self.color[idx] = value & 0x0FFF;
            }
            _ => {}
        }
    }
}

/// Apply set/clear logic: bit 15 = set (OR), bit 15 clear = clear (AND NOT).
fn apply_set_clear(reg: &mut u16, value: u16) {
    let bits = value & 0x7FFF;
    if value & 0x8000 != 0 {
        *reg |= bits;
    } else {
        *reg &= !bits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom;

    #[test]
    fn dmacon_enable_disable() {
        let mut state = CustomChipState::new(VideoStandard::Pal);
        // Set master + bitplane DMA
        state.write_register(
            custom::DMACON,
            0x8000 | custom::DMA_MASTER | custom::DMA_BITPLANE,
        );
        assert!(state.dmaen(custom::DMA_BITPLANE));
        assert!(!state.dmaen(custom::DMA_COPPER));

        // Clear master
        state.write_register(custom::DMACON, custom::DMA_MASTER);
        assert!(!state.dmaen(custom::DMA_BITPLANE));
    }

    #[test]
    fn intreq_intena_interrupt_level() {
        let mut state = CustomChipState::new(VideoStandard::Pal);
        // Enable VERTB interrupt
        state.write_register(custom::INTENA, 0x8000 | custom::INT_VERTB);
        // Request VERTB
        state.write_register(custom::INTREQ, 0x8000 | custom::INT_VERTB);
        assert_eq!(state.interrupt_level(), 3);

        // Also request EXTER
        state.write_register(custom::INTENA, 0x8000 | custom::INT_EXTER);
        state.write_register(custom::INTREQ, 0x8000 | custom::INT_EXTER);
        assert_eq!(state.interrupt_level(), 6);

        // Clear EXTER request
        state.write_register(custom::INTREQ, custom::INT_EXTER);
        assert_eq!(state.interrupt_level(), 3);
    }

    #[test]
    fn no_pending_interrupts_returns_zero() {
        let state = CustomChipState::new(VideoStandard::Pal);
        assert_eq!(state.interrupt_level(), 0);
    }

    #[test]
    fn beam_position_wraps() {
        let mut state = CustomChipState::new(VideoStandard::Pal);
        state.hpos = HPOS_MAX;
        state.vpos = 100;
        let vblank = state.advance_beam();
        assert!(!vblank);
        assert_eq!(state.hpos, 0);
        assert_eq!(state.vpos, 101);
    }

    #[test]
    fn beam_position_vertical_wrap() {
        let mut state = CustomChipState::new(VideoStandard::Pal);
        state.hpos = HPOS_MAX;
        state.vpos = VideoStandard::Pal.last_line();
        let vblank = state.advance_beam();
        assert!(vblank);
        assert_eq!(state.hpos, 0);
        assert_eq!(state.vpos, 0);
    }

    #[test]
    fn color_register_write_read() {
        let mut state = CustomChipState::new(VideoStandard::Pal);
        state.write_register(custom::COLOR00, 0x0F00);
        assert_eq!(state.read_register(custom::COLOR00), 0x0F00);
        state.write_register(custom::COLOR31, 0x0ABC);
        assert_eq!(state.read_register(custom::COLOR31), 0x0ABC);
    }

    #[test]
    fn beamcon0_reports_pal_timing() {
        let state = CustomChipState::new(VideoStandard::Pal);
        assert_eq!(state.read_register(custom::BEAMCON0), custom::BEAMCON0_PAL);
    }
}
