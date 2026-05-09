// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Sprite DMA and rendering for the Amiga OCS chipset.

use crate::playfield::amiga_to_rgb565;

/// Number of hardware sprites.
pub const NUM_SPRITES: usize = 8;

/// Width of each sprite in pixels.
pub const SPRITE_WIDTH: u32 = 16;

/// State of a single hardware sprite.
#[derive(Debug, Clone, Copy)]
pub struct SpriteState {
    /// Sprite data pointer (`SPRxPT`).
    pub pt: u32,
    /// `SPRxPOS` — vstart\[7:0\] in high byte, hstart\[8:1\] in low byte.
    pub pos: u16,
    /// `SPRxCTL` — vstop\[7:0\] in high byte; low byte contains hstart bit 0,
    /// vstop bit 8, vstart bit 8, and attach flag.
    pub ctl: u16,
    /// `SPRxDATA` — bitplane A.
    pub data_a: u16,
    /// `SPRxDATB` — bitplane B.
    pub data_b: u16,
    /// Sprite is armed (waiting for vstart match).
    pub armed: bool,
    /// Sprite is currently displaying.
    pub active: bool,
    /// DMA is enabled for this sprite.
    pub dma_enabled: bool,
}

impl SpriteState {
    /// Create a zeroed sprite state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pt: 0,
            pos: 0,
            ctl: 0,
            data_a: 0,
            data_b: 0,
            armed: false,
            active: false,
            dma_enabled: false,
        }
    }
}

impl Default for SpriteState {
    fn default() -> Self {
        Self::new()
    }
}

/// Sprite rendering engine managing all 8 hardware sprites.
#[derive(Debug, Clone)]
pub struct SpriteEngine {
    /// Per-sprite state.
    pub sprites: [SpriteState; NUM_SPRITES],
}

impl SpriteEngine {
    /// Create a new `SpriteEngine` with all sprites in their default state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sprites: [SpriteState::new(); NUM_SPRITES],
        }
    }

    /// Extract vertical start position from pos/ctl registers.
    ///
    /// Bit 8 comes from CTL bit 2, bits 7-0 from POS high byte.
    #[must_use]
    pub const fn vstart(sprite: &SpriteState) -> u16 {
        let low = sprite.pos >> 8;
        let high = if sprite.ctl & 0x04 != 0 { 1u16 } else { 0 };
        (high << 8) | low
    }

    /// Extract vertical stop position from ctl register.
    ///
    /// Bit 8 comes from CTL bit 1, bits 7-0 from CTL high byte.
    #[must_use]
    pub const fn vstop(sprite: &SpriteState) -> u16 {
        let low = sprite.ctl >> 8;
        let high = if sprite.ctl & 0x02 != 0 { 1u16 } else { 0 };
        (high << 8) | low
    }

    /// Extract horizontal start position from pos/ctl registers.
    ///
    /// Bits 8-1 from POS low byte, bit 0 from CTL bit 0.
    #[must_use]
    pub const fn hstart(sprite: &SpriteState) -> u16 {
        let high = (sprite.pos & 0xFF) << 1;
        let low = sprite.ctl & 0x01;
        high | low
    }

    /// Process the start of a scanline: activate or deactivate sprites based on `vpos`.
    pub fn begin_scanline(&mut self, vpos: u16) {
        for sprite in &mut self.sprites {
            if !sprite.dma_enabled {
                continue;
            }
            let vs = Self::vstart(sprite);
            let ve = Self::vstop(sprite);
            if vpos == vs {
                sprite.armed = true;
                sprite.active = true;
            } else if vpos == ve {
                sprite.active = false;
                sprite.armed = false;
            }
        }
    }

    /// Render a single sprite into the line buffer.
    ///
    /// `diw_hstart` is the display window horizontal start (from DIWSTRT low byte)
    /// used to convert sprite hardware coordinates to buffer pixel positions.
    ///
    /// Sprite pair N (sprites 2N, 2N+1) uses palette colors `16 + N*4` through
    /// `16 + N*4 + 3`. Color index 0 (both planes zero) is transparent and does
    /// not overwrite the buffer.
    pub fn render_into_line(
        &self,
        line_buffer: &mut [u16],
        colors: &[u16; 32],
        sprite_idx: usize,
        diw_hstart: u16,
    ) {
        let Some(sprite) = self.sprites.get(sprite_idx) else {
            return;
        };
        if !sprite.active {
            return;
        }

        let hstart = Self::hstart(sprite);
        let pair = sprite_idx / 2;
        let palette_base = 16 + pair * 4;

        for bit in 0..16u16 {
            let shift = 15 - bit;
            let a = (sprite.data_a >> shift) & 1;
            let b = (sprite.data_b >> shift) & 1;
            let idx = (b << 1) | a;
            if idx == 0 {
                continue;
            }
            let hpos = hstart + bit;
            if hpos < diw_hstart {
                continue;
            }
            let px = (hpos - diw_hstart) as usize;
            if let Some(dest) = line_buffer.get_mut(px) {
                *dest = amiga_to_rgb565(colors[palette_base + idx as usize]);
            }
        }
    }

    /// Fetch the next two words from chip RAM at the sprite pointer and advance it.
    ///
    /// If the sprite is not yet active, the words are loaded into pos/ctl.
    /// Otherwise they are loaded into `data_a`/`data_b`.
    pub fn fetch_data(&mut self, sprite_idx: usize, chip_ram: &[u8]) {
        let Some(sprite) = self.sprites.get_mut(sprite_idx) else {
            return;
        };
        let addr = sprite.pt as usize;
        let word_a = Self::read_word(chip_ram, addr);
        let word_b = Self::read_word(chip_ram, addr + 2);
        sprite.pt = sprite.pt.wrapping_add(4);

        if sprite.active {
            sprite.data_a = word_a;
            sprite.data_b = word_b;
        } else {
            sprite.pos = word_a;
            sprite.ctl = word_b;
        }
    }

    /// Read a big-endian u16 from a byte slice, returning 0 if out of bounds.
    const fn read_word(data: &[u8], addr: usize) -> u16 {
        if addr + 1 < data.len() {
            ((data[addr] as u16) << 8) | data[addr + 1] as u16
        } else {
            0
        }
    }
}

impl Default for SpriteEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn make_sprite(pos: u16, ctl: u16) -> SpriteState {
        SpriteState {
            pos,
            ctl,
            ..SpriteState::new()
        }
    }

    #[test]
    fn vstart_extraction() {
        // vstart = 0x2C (pos high byte), bit 8 clear
        let s = make_sprite(0x2C00, 0x0000);
        assert_eq!(SpriteEngine::vstart(&s), 0x2C);

        // vstart bit 8 set (ctl bit 2)
        let s = make_sprite(0x2C00, 0x0004);
        assert_eq!(SpriteEngine::vstart(&s), 0x12C);
    }

    #[test]
    fn vstop_extraction() {
        // vstop = 0x3C (ctl high byte), bit 8 clear
        let s = make_sprite(0x0000, 0x3C00);
        assert_eq!(SpriteEngine::vstop(&s), 0x3C);

        // vstop bit 8 set (ctl bit 1)
        let s = make_sprite(0x0000, 0x3C02);
        assert_eq!(SpriteEngine::vstop(&s), 0x13C);
    }

    #[test]
    fn hstart_extraction() {
        // hstart[8:1] = 0x40 from pos low byte, bit 0 clear
        let s = make_sprite(0x0040, 0x0000);
        assert_eq!(SpriteEngine::hstart(&s), 0x80);

        // hstart bit 0 set (ctl bit 0)
        let s = make_sprite(0x0040, 0x0001);
        assert_eq!(SpriteEngine::hstart(&s), 0x81);
    }

    #[test]
    fn render_at_correct_position() {
        let mut engine = SpriteEngine::new();
        engine.sprites[0].active = true;
        engine.sprites[0].pos = 0x0020; // hstart[8:1] = 0x20 -> hstart = 0x40
        engine.sprites[0].ctl = 0x0000;
        engine.sprites[0].data_a = 0x8000; // only leftmost pixel, plane A
        engine.sprites[0].data_b = 0x0000;

        let mut colors = [0u16; 32];
        colors[17] = 0x0F00; // pair 0, index 1 — red

        let mut buf = [0u16; 256];
        engine.render_into_line(&mut buf, &colors, 0, 0);

        assert_eq!(buf[0x40], amiga_to_rgb565(0x0F00));
        assert_eq!(buf[0x3F], 0);
        assert_eq!(buf[0x41], 0);
    }

    #[test]
    fn transparent_pixels_dont_overwrite() {
        let mut engine = SpriteEngine::new();
        engine.sprites[0].active = true;
        engine.sprites[0].pos = 0x0010; // hstart = 0x20
        engine.sprites[0].ctl = 0x0000;
        engine.sprites[0].data_a = 0x0000; // all transparent
        engine.sprites[0].data_b = 0x0000;

        let mut colors = [0u16; 32];
        colors[17] = 0xFFFF;

        let mut buf = [0xAAAA_u16; 256];
        engine.render_into_line(&mut buf, &colors, 0, 0);

        // Buffer should be unchanged
        for px in &buf {
            assert_eq!(*px, 0xAAAA);
        }
    }

    #[test]
    fn sprite_uses_correct_palette_for_pair() {
        let mut engine = SpriteEngine::new();
        // Sprite 4 is in pair 2 -> palette base = 16 + 2*4 = 24
        engine.sprites[4].active = true;
        engine.sprites[4].pos = 0x0000; // hstart = 0
        engine.sprites[4].ctl = 0x0000;
        engine.sprites[4].data_a = 0x8000; // index 1 at pixel 0
        engine.sprites[4].data_b = 0x8000; // index 3 at pixel 0

        let mut colors = [0u16; 32];
        colors[24 + 3] = 0x00F0; // pair 2, index 3 — green

        let mut buf = [0u16; 256];
        engine.render_into_line(&mut buf, &colors, 4, 0);

        assert_eq!(buf[0], amiga_to_rgb565(0x00F0));
    }

    #[test]
    fn begin_scanline_activates_and_deactivates() {
        let mut engine = SpriteEngine::new();
        engine.sprites[0].dma_enabled = true;
        engine.sprites[0].pos = 0x2C00; // vstart = 0x2C
        engine.sprites[0].ctl = 0x3C00; // vstop = 0x3C

        // Before vstart
        engine.begin_scanline(0x2B);
        assert!(!engine.sprites[0].active);

        // At vstart
        engine.begin_scanline(0x2C);
        assert!(engine.sprites[0].active);
        assert!(engine.sprites[0].armed);

        // At vstop
        engine.begin_scanline(0x3C);
        assert!(!engine.sprites[0].active);
        assert!(!engine.sprites[0].armed);
    }
}
