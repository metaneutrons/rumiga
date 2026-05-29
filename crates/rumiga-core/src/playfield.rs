// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Bitplane DMA and playfield rendering for the Amiga OCS chipset.

#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::needless_range_loop,
    clippy::precedence,
    clippy::too_many_lines,
    clippy::unreadable_literal
)]

/// First visible low-resolution hardware position in the normal OCS viewport.
///
/// `WinUAE`'s normal non-extreme overscan limits clamp the visible horizontal
/// range to roughly hpos 92..460, which gives Workbench room for side borders.
pub const DISPLAY_LEFT_HPOS: u16 = 92;

/// High-resolution pixels per normal PAL line.
///
/// Lores playfields are expanded 2x horizontally into this buffer. The 736 px
/// width matches `WinUAE`'s normal OCS visible span `(460 - 92) * 2`.
pub const DISPLAY_WIDTH: u32 = 736;

/// Maximum PAL display height in non-interlaced lines.
///
/// Matches `WinUAE`'s native PAL viewport height (`AMIGA_HEIGHT_MAX_PAL`) so
/// overscan Workbench screens are not clipped at the old 256-line boundary.
pub const DISPLAY_HEIGHT: u32 = 288;

/// lisa/AGA maximum number of bitplanes.
pub const MAX_PLANES: usize = 8;

/// Number of pixels per bitplane word.
const PIXELS_PER_WORD: u16 = 16;

/// Standard OCS low-resolution display window start.
const STANDARD_DIW_START_HPOS: u16 = 0x81;

/// Standard OCS bitplane data fetch start.
const STANDARD_DDF_START_HPOS: u16 = 0x38;

/// Display width as u16 for scanline iteration.
///
/// Derived from [`DISPLAY_WIDTH`]; compile-time assertion guarantees no truncation.
#[allow(clippy::cast_possible_truncation)]
const LINE_WIDTH: u16 = {
    assert!(DISPLAY_WIDTH <= u16::MAX as u32);
    DISPLAY_WIDTH as u16
};

/// Playfield state holding all registers and data needed for bitplane rendering.
#[derive(Debug, Clone)]
pub struct PlayfieldState {
    /// Bitplane control register 0 (plane count in bits 14-12).
    pub bplcon0: u16,
    /// Bitplane control register 1 (scroll delays).
    pub bplcon1: u16,
    /// Bitplane control register 2 (priority).
    pub bplcon2: u16,
    /// Display data fetch start.
    pub ddfstrt: u16,
    /// Display data fetch stop.
    pub ddfstop: u16,
    /// Display window start (upper-left).
    pub diwstrt: u16,
    /// Display window stop (lower-right).
    pub diwstop: u16,
    /// Display window high bits (ECS).
    pub diwhigh: u16,
    /// Bitplane control register 3 (AGA bank select / LOCT).
    pub bplcon3: u16,
    /// Bitplane control register 4 (AGA sprite bank / playfield XOR).
    pub bplcon4: u16,
    /// Fetch mode register (AGA sprite/playfield DMA width).
    pub fmode: u16,
    /// Bitplane pointers (24-bit addresses stored as u32).
    pub bplpt: [u32; MAX_PLANES],
    /// Bitplane data shift registers.
    pub bpldat: [u16; MAX_PLANES],
    /// Color palette (32 entries, 12-bit Amiga RGB).
    pub color: [u16; 32],
    /// AGA Color palette (256 entries, 24-bit RGB).
    pub color_aga: [u32; 256],
}

impl PlayfieldState {
    /// Create a new `PlayfieldState` with default (zeroed) registers.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bplcon0: 0,
            bplcon1: 0,
            bplcon2: 0,
            bplcon3: 0,
            bplcon4: 0x0011,
            fmode: 0,
            ddfstrt: 0,
            ddfstop: 0,
            diwstrt: 0,
            diwstop: 0,
            diwhigh: 0,
            bplpt: [0; MAX_PLANES],
            bpldat: [0; MAX_PLANES],
            color: [0; 32],
            color_aga: [0; 256],
        }
    }

    /// Extract the number of active bitplanes from BPLCON0.
    /// Supports up to 8 planes under AGA.
    #[must_use]
    pub const fn num_planes(&self) -> usize {
        let bplcon0 = self.bplcon0;
        if (bplcon0 & 0x0010) != 0 && (bplcon0 & 0x7000) != 0 {
            0
        } else if (bplcon0 & 0x0010) != 0 {
            8
        } else {
            ((bplcon0 >> 12) & 0x7) as usize
        }
    }

    /// Returns the display window coordinates `(hstart, hstop, vstart, vstop)`.
    ///
    /// Extracted from DIWSTRT (vstart high byte, hstart low byte) and
    /// DIWSTOP (vstop high byte, hstop low byte). If DIWHIGH is written, it decodes
    /// the extended high bits for horizontal/vertical start and stop positions.
    #[must_use]
    pub const fn display_window(&self) -> (u16, u16, u16, u16) {
        let mut hstart = self.diwstrt & 0xFF;
        let mut vstart = self.diwstrt >> 8;
        let mut hstop = self.diwstop & 0xFF;
        let mut vstop = self.diwstop >> 8;

        if self.diwhigh != 0 {
            // Vertical start high bits: bits 0-2 (ECS/AGA)
            vstart |= (self.diwhigh & 0x7) << 8;
            // Horizontal start high bit: bit 5
            hstart |= ((self.diwhigh >> 5) & 1) << 8;
            // Vertical stop high bits: bits 8-10 (ECS/AGA)
            vstop |= ((self.diwhigh >> 8) & 0x7) << 8;
            // Horizontal stop high bit: bit 13
            hstop |= ((self.diwhigh >> 13) & 1) << 8;
        } else {
            // OCS fallback behavior
            if (vstop & 0x80) == 0 {
                vstop |= 0x100;
            }
            hstop |= 0x100;
        }

        (hstart, hstop, vstart, vstop)
    }

    /// Fetch a 16-bit word from chip RAM at `bplpt[plane]` into `bpldat[plane]`,
    /// then increment the pointer by 2.
    ///
    /// If the pointer is out of bounds, `bpldat[plane]` is set to 0.
    pub fn fetch_bitplane_word(&mut self, plane: usize, chip_ram: &[u8]) {
        let addr = (self.bplpt[plane] as usize) % chip_ram.len();
        self.bpldat[plane] = if addr + 1 < chip_ram.len() {
            u16::from_be_bytes([chip_ram[addr], chip_ram[addr + 1]])
        } else {
            0
        };
        self.bplpt[plane] = self.bplpt[plane].wrapping_add(2);
    }

    /// Render one scanline of pixels into `line_buffer` as RGB565.
    ///
    /// For each pixel position, bitplane bits are combined (plane 0 = LSB) to
    /// form a palette index. Pixels outside the display window use `color[0]`.
    pub fn render_scanline(&mut self, line: u16, chip_ram: &[u8], line_buffer: &mut [u16]) {
        let (hstart, hstop, vstart, vstop) = self.display_window();
        let bg_color = if self.color_aga[0] == 0 && self.color[0] != 0 {
            let c = self.color[0];
            let r = ((c >> 8) & 0xF) as u32;
            let g = ((c >> 4) & 0xF) as u32;
            let b = (c & 0xF) as u32;
            (r | (r << 4)) << 16 | (g | (g << 4)) << 8 | (b | (b << 4))
        } else {
            self.color_aga[0]
        };
        let bg = rgb888_to_rgb565(bg_color);

        let num_planes = self.num_planes().min(MAX_PLANES);
        let hires = self.bplcon0 & 0x8000 != 0;
        let ham = (self.bplcon0 & 0x0800) != 0;
        let fetch_words = self.data_fetch_words(hires);
        let mut words_fetched = 0usize;
        let mut current_word = None;
        let line_visible = line >= vstart && line < vstop;
        let source_offset = self.horizontal_source_offset(hstart);

        let mut hold_r = ((bg_color >> 16) & 0xFF) as u8;
        let mut hold_g = ((bg_color >> 8) & 0xFF) as u8;
        let mut hold_b = (bg_color & 0xFF) as u8;

        for px in 0..LINE_WIDTH {
            let raster_hpos = DISPLAY_LEFT_HPOS + (px / 2);
            if !line_visible || num_planes == 0 || raster_hpos < hstart || raster_hpos >= hstop {
                if let Some(dest) = line_buffer.get_mut(usize::from(px)) {
                    *dest = bg;
                }
                continue;
            }

            // Fetch new words every 16 source pixels. Lores source pixels are
            // doubled in the high-resolution output buffer.
            let window_px = raster_hpos - hstart;
            let source_px = if hires {
                (window_px + source_offset) * 2 + (px & 1)
            } else {
                window_px + source_offset
            };

            let source_word = usize::from(source_px / PIXELS_PER_WORD);
            if source_word >= fetch_words {
                if let Some(dest) = line_buffer.get_mut(usize::from(px)) {
                    *dest = bg;
                }
                continue;
            }
            while current_word != Some(source_word) && words_fetched <= source_word {
                for plane in 0..num_planes {
                    self.fetch_bitplane_word(plane, chip_ram);
                }
                current_word = Some(words_fetched);
                words_fetched += 1;
            }

            // Combine bits from each plane (bit 15 = leftmost pixel).
            let bit_index = 15 - (source_px % PIXELS_PER_WORD);
            let color_index = self.color_index_at_bit(num_planes, bit_index);

            let rgb = if ham {
                if num_planes == 8 {
                    // HAM8 Mode
                    let control = (color_index >> 6) & 3;
                    let data = color_index & 0x3F;
                    match control {
                        0 => {
                            let c = self.color_aga[data as usize];
                            hold_r = ((c >> 16) & 0xFF) as u8;
                            hold_g = ((c >> 8) & 0xFF) as u8;
                            hold_b = (c & 0xFF) as u8;
                        }
                        1 => hold_b = (data << 2 | (data >> 4)) as u8,
                        2 => hold_r = (data << 2 | (data >> 4)) as u8,
                        _ => hold_g = (data << 2 | (data >> 4)) as u8,
                    }
                } else {
                    // HAM6 Mode
                    let control = (color_index >> 4) & 3;
                    let data = color_index & 0x0F;
                    match control {
                        0 => {
                            let c = if self.color_aga[data as usize] == 0
                                && self.color[data as usize] != 0
                            {
                                let c12 = self.color[data as usize];
                                let r = ((c12 >> 8) & 0xF) as u32;
                                let g = ((c12 >> 4) & 0xF) as u32;
                                let b = (c12 & 0xF) as u32;
                                (r | (r << 4)) << 16 | (g | (g << 4)) << 8 | (b | (b << 4))
                            } else {
                                self.color_aga[data as usize]
                            };
                            hold_r = ((c >> 16) & 0xFF) as u8;
                            hold_g = ((c >> 8) & 0xFF) as u8;
                            hold_b = (c & 0xFF) as u8;
                        }
                        1 => hold_b = (data << 4 | data) as u8,
                        2 => hold_r = (data << 4 | data) as u8,
                        _ => hold_g = (data << 4 | data) as u8,
                    }
                }
                ((hold_r as u16 >> 3) << 11) | ((hold_g as u16 >> 2) << 5) | (hold_b as u16 >> 3)
            } else {
                let color_val = if self.color_aga[color_index as usize] == 0
                    && self.color[color_index as usize & 0x1F] != 0
                {
                    let c12 = self.color[color_index as usize & 0x1F];
                    let r = ((c12 >> 8) & 0xF) as u32;
                    let g = ((c12 >> 4) & 0xF) as u32;
                    let b = (c12 & 0xF) as u32;
                    (r | (r << 4)) << 16 | (g | (g << 4)) << 8 | (b | (b << 4))
                } else {
                    self.color_aga[color_index as usize]
                };
                hold_r = ((color_val >> 16) & 0xFF) as u8;
                hold_g = ((color_val >> 8) & 0xFF) as u8;
                hold_b = (color_val & 0xFF) as u8;
                rgb888_to_rgb565(color_val)
            };

            if let Some(dest) = line_buffer.get_mut(usize::from(px)) {
                *dest = rgb;
            }
        }

        while line_visible && words_fetched < fetch_words {
            for plane in 0..num_planes {
                self.fetch_bitplane_word(plane, chip_ram);
            }
            words_fetched += 1;
        }
    }

    const fn horizontal_source_offset(&self, hstart: u16) -> u16 {
        if self.ddfstrt == 0 && self.ddfstop == 0 {
            return 0;
        }

        let fetch_start = self.ddfstrt & 0x00FC;
        let standard_phase = STANDARD_DIW_START_HPOS - STANDARD_DDF_START_HPOS;
        let phase = hstart.saturating_sub(fetch_start);
        phase.saturating_sub(standard_phase) / 2
    }

    fn color_index_at_bit(&self, num_planes: usize, bit_index: u16) -> u16 {
        let mut color_index: u16 = 0;
        for plane in 0..num_planes {
            color_index |= ((self.bpldat[plane] >> bit_index) & 1) << plane;
        }
        color_index
    }

    fn data_fetch_words(&self, hires: bool) -> usize {
        if self.ddfstrt == 0 && self.ddfstop == 0 {
            return if hires { 40 } else { 20 };
        }

        let start = self.ddfstrt & 0x00FC;
        let stop = self.ddfstop & 0x00FC;
        if stop < start {
            return 0;
        }

        if hires {
            usize::from(((stop - start) / 4) + 2)
        } else {
            usize::from(((stop - start) / 8) + 1)
        }
    }
}

impl Default for PlayfieldState {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert an Amiga 12-bit color (`$0RGB`, 4 bits per channel) to RGB565.
///
/// The 4-bit components are expanded to 5-6-5 bits by shifting and replicating
/// the MSB into the LSB position for better color accuracy.
#[must_use]
pub const fn amiga_to_rgb565(color12: u16) -> u16 {
    let r4 = (color12 >> 8) & 0xF;
    let g4 = (color12 >> 4) & 0xF;
    let b4 = color12 & 0xF;

    // Expand 4-bit to 5-bit: shift left 1, replicate MSB into LSB
    let r5 = (r4 << 1) | (r4 >> 3);
    // Expand 4-bit to 6-bit: shift left 2, replicate top 2 bits into bottom
    let g6 = (g4 << 2) | (g4 >> 2);
    // Expand 4-bit to 5-bit
    let b5 = (b4 << 1) | (b4 >> 3);

    (r5 << 11) | (g6 << 5) | b5
}

/// Convert a 24-bit color (8 bits per channel, stored as `$00RRGGBB` in u32) to RGB565.
#[must_use]
pub const fn rgb888_to_rgb565(rgb24: u32) -> u16 {
    let r = ((rgb24 >> 16) & 0xFF) as u16;
    let g = ((rgb24 >> 8) & 0xFF) as u16;
    let b = (rgb24 & 0xFF) as u16;

    // Convert 8-bit to 5-bit Red (r >> 3), 6-bit Green (g >> 2), 5-bit Blue (b >> 3)
    (((r >> 3) & 0x1F) << 11) | (((g >> 2) & 0x3F) << 5) | ((b >> 3) & 0x1F)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_start_px(pf: &PlayfieldState) -> usize {
        let (hstart, _, _, _) = pf.display_window();
        usize::from(hstart.saturating_sub(DISPLAY_LEFT_HPOS)) * 2
    }

    #[test]
    fn amiga_to_rgb565_black() {
        assert_eq!(amiga_to_rgb565(0x0000), 0x0000);
    }

    #[test]
    fn amiga_to_rgb565_white() {
        // $0FFF -> R=0x1F, G=0x3F, B=0x1F -> 0xFFFF
        assert_eq!(amiga_to_rgb565(0x0FFF), 0xFFFF);
    }

    #[test]
    fn amiga_to_rgb565_red() {
        // $0F00 -> R=0x1F, G=0, B=0 -> 0xF800
        assert_eq!(amiga_to_rgb565(0x0F00), 0xF800);
    }

    #[test]
    fn amiga_to_rgb565_green() {
        // $00F0 -> R=0, G=0x3F, B=0 -> 0x07E0
        assert_eq!(amiga_to_rgb565(0x00F0), 0x07E0);
    }

    #[test]
    fn amiga_to_rgb565_blue() {
        // $000F -> R=0, G=0, B=0x1F -> 0x001F
        assert_eq!(amiga_to_rgb565(0x000F), 0x001F);
    }

    #[test]
    fn num_planes_extraction() {
        let mut pf = PlayfieldState::new();
        pf.bplcon0 = 0x1000; // 1 plane (bit 12)
        assert_eq!(pf.num_planes(), 1);
        pf.bplcon0 = 0x4000; // 4 planes (bits 14)
        assert_eq!(pf.num_planes(), 4);
        pf.bplcon0 = 0x6000; // 6 planes
        assert_eq!(pf.num_planes(), 6);
    }

    #[test]
    fn single_bitplane_renders_two_colors() {
        let mut pf = PlayfieldState::new();
        pf.bplcon0 = 0x1000; // 1 plane
        // Standard PAL display window
        pf.diwstrt = 0x2C81; // vstart=0x2C, hstart=0x81
        pf.diwstop = 0x2CC1; // vstop=0x12C, hstop=0x1C1
        pf.color[0] = 0x0000; // black background
        pf.color[1] = 0x0FFF; // white foreground

        // Chip RAM: first word = 0xFF00 (8 set bits, 8 clear bits)
        let mut chip_ram = [0u8; 1024];
        chip_ram[0] = 0xFF;
        chip_ram[1] = 0x00;
        // Second word = 0x00FF
        chip_ram[2] = 0x00;
        chip_ram[3] = 0xFF;

        pf.bplpt[0] = 0;

        let mut line_buffer = [0u16; DISPLAY_WIDTH as usize];
        pf.render_scanline(0x2C, &chip_ram, &mut line_buffer);

        let white = amiga_to_rgb565(0x0FFF);
        let black = amiga_to_rgb565(0x0000);
        let start = active_start_px(&pf);

        // First 8 lores pixels are doubled into 16 high-res output pixels.
        for px in &line_buffer[start..start + 16] {
            assert_eq!(*px, white);
        }
        // Next 8 lores pixels are black, also doubled.
        for px in &line_buffer[start + 16..start + 32] {
            assert_eq!(*px, black);
        }
        // Next word begins with 8 black lores pixels.
        for px in &line_buffer[start + 32..start + 48] {
            assert_eq!(*px, black);
        }
        // Then 8 white lores pixels.
        for px in &line_buffer[start + 48..start + 64] {
            assert_eq!(*px, white);
        }
    }

    #[test]
    fn multiple_bitplanes_combine_correctly() {
        let mut pf = PlayfieldState::new();
        pf.bplcon0 = 0x2000; // 2 planes
        pf.diwstrt = 0x2C81;
        pf.diwstop = 0x2CC1;
        pf.color[0] = 0x0000; // index 0 = black
        pf.color[1] = 0x0F00; // index 1 = red (plane 0 only)
        pf.color[2] = 0x00F0; // index 2 = green (plane 1 only)
        pf.color[3] = 0x000F; // index 3 = blue (both planes)

        // Plane 0: 0xAAAA = alternating 1,0,1,0...
        // Plane 1: 0xCCCC = 1,1,0,0,1,1,0,0...
        let mut chip_ram = [0u8; 1024];
        chip_ram[0] = 0xAA;
        chip_ram[1] = 0xAA;
        chip_ram[100] = 0xCC;
        chip_ram[101] = 0xCC;

        pf.bplpt[0] = 0;
        pf.bplpt[1] = 100;

        let mut line_buffer = [0u16; DISPLAY_WIDTH as usize];
        pf.render_scanline(0x2C, &chip_ram, &mut line_buffer);

        let start = active_start_px(&pf);
        // Lores pixels are doubled in the high-res output buffer.
        // Pixel 0: plane0=1, plane1=1 -> index 3 (blue)
        assert_eq!(line_buffer[start], amiga_to_rgb565(0x000F));
        assert_eq!(line_buffer[start + 1], amiga_to_rgb565(0x000F));
        // Pixel 1: plane0=0, plane1=1 -> index 2 (green)
        assert_eq!(line_buffer[start + 2], amiga_to_rgb565(0x00F0));
        assert_eq!(line_buffer[start + 3], amiga_to_rgb565(0x00F0));
        // Pixel 2: plane0=1, plane1=0 -> index 1 (red)
        assert_eq!(line_buffer[start + 4], amiga_to_rgb565(0x0F00));
        assert_eq!(line_buffer[start + 5], amiga_to_rgb565(0x0F00));
        // Pixel 3: plane0=0, plane1=0 -> index 0 (black)
        assert_eq!(line_buffer[start + 6], amiga_to_rgb565(0x0000));
        assert_eq!(line_buffer[start + 7], amiga_to_rgb565(0x0000));
    }

    #[test]
    fn highres_renders_native_source_pixels() {
        let mut pf = PlayfieldState::new();
        pf.bplcon0 = 0x8000 | 0x1000; // high-res, 1 plane
        pf.diwstrt = 0x2C81;
        pf.diwstop = 0x2CC1;
        pf.color[0] = 0x0000;
        pf.color[1] = 0x0FFF;

        let mut chip_ram = [0u8; 128];
        chip_ram[0] = 0xAA; // even high-res pixels are set
        chip_ram[1] = 0xAA;
        chip_ram[2] = 0x55; // odd high-res pixels are set
        chip_ram[3] = 0x55;

        pf.bplpt[0] = 0;

        let mut line_buffer = [0u16; DISPLAY_WIDTH as usize];
        pf.render_scanline(0x2C, &chip_ram, &mut line_buffer);

        let white = amiga_to_rgb565(0x0FFF);
        let black = amiga_to_rgb565(0x0000);
        let start = active_start_px(&pf);
        for (i, px) in line_buffer[start..start + 16].iter().enumerate() {
            let expected = if i % 2 == 0 { white } else { black };
            assert_eq!(*px, expected, "pixel {i} mismatch");
        }
        assert_eq!(pf.bplpt[0], 80);
    }

    #[test]
    fn highres_ddf_window_controls_words_fetched() {
        let mut pf = PlayfieldState::new();
        pf.bplcon0 = 0x8000 | 0x1000;
        pf.diwstrt = 0x2C81;
        pf.diwstop = 0x2CC1;
        pf.ddfstrt = 0x0038;
        pf.ddfstop = 0x00D8;
        pf.color[0] = 0x0000;
        pf.color[1] = 0x0FFF;

        let chip_ram = [0xFFu8; 256];
        let mut line_buffer = [0u16; DISPLAY_WIDTH as usize];
        pf.render_scanline(0x2C, &chip_ram, &mut line_buffer);

        assert_eq!(pf.bplpt[0], 84);
    }

    #[test]
    fn nonstandard_ddf_to_diw_phase_skips_hidden_left_source_pixels() {
        let mut pf = PlayfieldState::new();
        pf.bplcon0 = 0x1000;
        pf.diwstrt = 0x1D95;
        pf.diwstop = 0x38AD;
        pf.ddfstrt = 0x0040;
        pf.ddfstop = 0x00D0;
        pf.color[0] = 0x0000;
        pf.color[1] = 0x0FFF;

        let mut chip_ram = [0u8; 64];
        chip_ram[0] = 0x03;
        chip_ram[1] = 0xFF;
        pf.bplpt[0] = 0;

        let mut line_buffer = [0u16; DISPLAY_WIDTH as usize];
        pf.render_scanline(0x7D, &chip_ram, &mut line_buffer);

        let black = amiga_to_rgb565(0x0000);
        let white = amiga_to_rgb565(0x0FFF);
        let start = active_start_px(&pf);
        assert_eq!(line_buffer[start - 1], black);
        assert_eq!(line_buffer[start], white);
        assert_eq!(line_buffer[start + 1], white);
    }

    #[test]
    fn pixels_outside_display_window_are_background() {
        let mut pf = PlayfieldState::new();
        pf.bplcon0 = 0x1000; // 1 plane
        pf.diwstrt = 0x2C81;
        pf.diwstop = 0x2CC1;
        pf.color[0] = 0x0123; // background
        pf.color[1] = 0x0FFF;

        let chip_ram = [0xFFu8; 1024];
        pf.bplpt[0] = 0;

        let mut line_buffer = [0u16; DISPLAY_WIDTH as usize];
        // Render a line outside the vertical display window
        pf.render_scanline(0x00, &chip_ram, &mut line_buffer);

        let bg = amiga_to_rgb565(0x0123);
        for px in &line_buffer {
            assert_eq!(*px, bg);
        }
    }

    #[test]
    fn normal_visible_raster_keeps_side_borders() {
        let mut pf = PlayfieldState::new();
        pf.bplcon0 = 0x1000;
        pf.diwstrt = 0x2C81;
        pf.diwstop = 0x2CC1;
        pf.color[0] = 0x0123;
        pf.color[1] = 0x0FFF;

        let chip_ram = [0xFFu8; 1024];
        let mut line_buffer = [0u16; DISPLAY_WIDTH as usize];
        pf.render_scanline(0x2C, &chip_ram, &mut line_buffer);

        let bg = amiga_to_rgb565(0x0123);
        let fg = amiga_to_rgb565(0x0FFF);
        let start = active_start_px(&pf);
        let (hstart, hstop, _, _) = pf.display_window();
        let end = start + usize::from(hstop - hstart) * 2;

        assert_eq!(start, 74);
        assert_eq!(line_buffer[start - 1], bg);
        assert_eq!(line_buffer[start], fg);
        assert_eq!(line_buffer[end - 1], fg);
        assert_eq!(line_buffer[end], bg);
    }

    #[test]
    fn diwhigh_decoding() {
        let mut pf = PlayfieldState::new();
        pf.diwstrt = 0x2C81;
        pf.diwstop = 0x2CC1;
        pf.diwhigh = 0x2020; // hstart bit 8 = 1, hstop bit 8 = 1
        let (hstart, hstop, vstart, vstop) = pf.display_window();
        assert_eq!(hstart, 0x181); // 0x81 | 0x100
        assert_eq!(hstop, 0x1C1); // 0xC1 | 0x100
        assert_eq!(vstart, 0x2C);
        assert_eq!(vstop, 0x2C);
    }

    #[test]
    fn ham8_hold_and_modify_decoding() {
        let mut pf = PlayfieldState::new();
        // HAM8 active: HAM bit set (0x0800) + 8 planes (0x0010)
        pf.bplcon0 = 0x0810;
        pf.diwstrt = 0x2C81;
        pf.diwstop = 0x2CC1;

        pf.color_aga[0] = 0x112233;
        pf.color_aga[1] = 0x445566;

        let mut chip_ram = [0u8; 1024];
        let p_words = [
            0xB000u16, 0x5000, 0x3000, 0x5000, 0x3000, 0x5000, 0x3000, 0x6000,
        ];
        for i in 0..8 {
            pf.bplpt[i] = (i * 10) as u32;
            let addr = i * 10;
            chip_ram[addr] = (p_words[i] >> 8) as u8;
            chip_ram[addr + 1] = (p_words[i] & 0xFF) as u8;
        }

        let mut line_buffer = [0u16; DISPLAY_WIDTH as usize];
        pf.render_scanline(0x2C, &chip_ram, &mut line_buffer);

        let start = active_start_px(&pf);

        let c0 = rgb888_to_rgb565(0x445566);
        assert_eq!(line_buffer[start], c0);
        assert_eq!(line_buffer[start + 1], c0);

        let c1 = rgb888_to_rgb565(0xA95566);
        assert_eq!(line_buffer[start + 2], c1);
        assert_eq!(line_buffer[start + 3], c1);

        let c2 = rgb888_to_rgb565(0xA95566);
        assert_eq!(line_buffer[start + 4], c2);
        assert_eq!(line_buffer[start + 5], c2);

        let c3 = rgb888_to_rgb565(0xA955FF);
        assert_eq!(line_buffer[start + 6], c3);
        assert_eq!(line_buffer[start + 7], c3);
    }

    #[test]
    fn ham6_hold_and_modify_decoding() {
        let mut pf = PlayfieldState::new();
        // HAM active (0x0800) + 6 planes (0x6000)
        pf.bplcon0 = 0x6800;
        pf.diwstrt = 0x2C81;
        pf.diwstop = 0x2CC1;

        pf.color_aga[0] = 0x112233;
        pf.color_aga[1] = 0x445566;

        let mut chip_ram = [0u8; 1024];
        let p_words = [0x8000u16, 0x4000, 0x0000, 0x4000, 0x0000, 0x4000];
        for i in 0..6 {
            pf.bplpt[i] = (i * 10) as u32;
            let addr = i * 10;
            chip_ram[addr] = (p_words[i] >> 8) as u8;
            chip_ram[addr + 1] = (p_words[i] & 0xFF) as u8;
        }

        let mut line_buffer = [0u16; DISPLAY_WIDTH as usize];
        pf.render_scanline(0x2C, &chip_ram, &mut line_buffer);

        let start = active_start_px(&pf);

        let c0 = rgb888_to_rgb565(0x445566);
        assert_eq!(line_buffer[start], c0);

        let c1 = rgb888_to_rgb565(0xAA5566);
        assert_eq!(line_buffer[start + 2], c1);
    }

    #[test]
    fn bplcon3_loct_and_banking() {
        let mut emu = crate::emulator::Emulator::new(crate::memory::MemoryConfig::a1200());

        emu.dispatch_register_write(0x106, 0x0000);
        emu.dispatch_register_write(0x182, 0x0F48);

        assert_eq!(emu.playfield.color_aga[1], 0xFF4488);

        emu.dispatch_register_write(0x106, 0x2200);
        emu.dispatch_register_write(0x182, 0x0ABC);

        assert_eq!(emu.playfield.color_aga[33], 0x0A0B0C);
    }
}
