// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Bitplane DMA and playfield rendering for the Amiga OCS chipset.

/// Lores pixels per line.
pub const DISPLAY_WIDTH: u32 = 320;

/// PAL visible lines.
pub const DISPLAY_HEIGHT: u32 = 256;

/// OCS maximum number of bitplanes.
pub const MAX_PLANES: usize = 6;

/// Number of pixels per bitplane word.
const PIXELS_PER_WORD: u16 = 16;

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
    /// Bitplane pointers (24-bit addresses stored as u32).
    pub bplpt: [u32; MAX_PLANES],
    /// Bitplane data shift registers.
    pub bpldat: [u16; MAX_PLANES],
    /// Color palette (32 entries, 12-bit Amiga RGB).
    pub color: [u16; 32],
}

impl PlayfieldState {
    /// Create a new `PlayfieldState` with default (zeroed) registers.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bplcon0: 0,
            bplcon1: 0,
            bplcon2: 0,
            ddfstrt: 0,
            ddfstop: 0,
            diwstrt: 0,
            diwstop: 0,
            bplpt: [0; MAX_PLANES],
            bpldat: [0; MAX_PLANES],
            color: [0; 32],
        }
    }

    /// Extract the number of active bitplanes from BPLCON0 bits 14-12.
    #[must_use]
    pub const fn num_planes(&self) -> usize {
        ((self.bplcon0 >> 12) & 0x7) as usize
    }

    /// Returns the display window coordinates `(hstart, hstop, vstart, vstop)`.
    ///
    /// Extracted from DIWSTRT (vstart high byte, hstart low byte) and
    /// DIWSTOP (vstop high byte, hstop low byte). The hardware adds 256 to
    /// hstop and vstop implicitly for standard OCS displays.
    #[must_use]
    pub const fn display_window(&self) -> (u16, u16, u16, u16) {
        let hstart = self.diwstrt & 0xFF;
        let vstart = self.diwstrt >> 8;
        let hstop = (self.diwstop & 0xFF) | 0x100;
        let vstop = (self.diwstop >> 8) | 0x100;
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
        let bg = amiga_to_rgb565(self.color[0]);
        let num_planes = self.num_planes().min(MAX_PLANES);
        let line_visible = line >= vstart && line < vstop;

        let width = LINE_WIDTH;
        for px in 0..width {
            let hpos = px + hstart;
            if !line_visible || hpos < hstart || hpos >= hstop {
                if let Some(dest) = line_buffer.get_mut(usize::from(px)) {
                    *dest = bg;
                }
                continue;
            }

            // Fetch new words every 16 pixels
            if px % PIXELS_PER_WORD == 0 {
                for plane in 0..num_planes {
                    self.fetch_bitplane_word(plane, chip_ram);
                }
            }

            // Combine bits from each plane (bit 15 = leftmost pixel)
            let bit_index = 15 - (px % PIXELS_PER_WORD);
            let mut color_index: u16 = 0;
            for plane in 0..num_planes {
                color_index |= ((self.bpldat[plane] >> bit_index) & 1) << plane;
            }

            let rgb = amiga_to_rgb565(self.color[usize::from(color_index) & 0x1F]);
            if let Some(dest) = line_buffer.get_mut(usize::from(px)) {
                *dest = rgb;
            }
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

#[cfg(test)]
mod tests {
    use super::*;

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

        // First 8 pixels should be white (bits 15-8 of 0xFF00)
        for px in &line_buffer[0..8] {
            assert_eq!(*px, white);
        }
        // Next 8 pixels should be black (bits 7-0 of 0xFF00)
        for px in &line_buffer[8..16] {
            assert_eq!(*px, black);
        }
        // Pixels 16-23 should be black (bits 15-8 of 0x00FF)
        for px in &line_buffer[16..24] {
            assert_eq!(*px, black);
        }
        // Pixels 24-31 should be white (bits 7-0 of 0x00FF)
        for px in &line_buffer[24..32] {
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

        // Pixel 0: plane0=1, plane1=1 -> index 3 (blue)
        assert_eq!(line_buffer[0], amiga_to_rgb565(0x000F));
        // Pixel 1: plane0=0, plane1=1 -> index 2 (green)
        assert_eq!(line_buffer[1], amiga_to_rgb565(0x00F0));
        // Pixel 2: plane0=1, plane1=0 -> index 1 (red)
        assert_eq!(line_buffer[2], amiga_to_rgb565(0x0F00));
        // Pixel 3: plane0=0, plane1=0 -> index 0 (black)
        assert_eq!(line_buffer[3], amiga_to_rgb565(0x0000));
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
}
