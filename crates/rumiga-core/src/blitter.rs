// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Blitter DMA engine emulation.
//!
//! The blitter performs bulk memory operations (copy, fill, line draw) using
//! up to four DMA channels (A, B, C, D) and a configurable minterm logic
//! function. This implementation executes blits immediately (non-cycle-accurate).

/// Channel enable bit for source A.
const USE_A: u16 = 1 << 11;
/// Channel enable bit for source B.
const USE_B: u16 = 1 << 10;
/// Channel enable bit for source C.
const USE_C: u16 = 1 << 9;
/// Channel enable bit for destination D.
const USE_D: u16 = 1 << 8;

/// Maximum width value (width field of 0 means 64 words).
const MAX_WIDTH: u16 = 64;

/// Blitter state holding all registers and status flags.
#[derive(Clone, Debug)]
pub struct BlitterState {
    /// Control register 0: ASH\[15:12\], use flags\[11:8\], minterm\[7:0\].
    pub bltcon0: u16,
    /// Control register 1: BSH\[15:12\], EFE/IFE/FCI/DESC/LINE.
    pub bltcon1: u16,
    /// First word mask for channel A.
    pub bltafwm: u16,
    /// Last word mask for channel A.
    pub bltalwm: u16,
    /// Channel A pointer.
    pub bltapt: u32,
    /// Channel B pointer.
    pub bltbpt: u32,
    /// Channel C pointer.
    pub bltcpt: u32,
    /// Channel D pointer.
    pub bltdpt: u32,
    /// Channel A modulo.
    pub bltamod: i16,
    /// Channel B modulo.
    pub bltbmod: i16,
    /// Channel C modulo.
    pub bltcmod: i16,
    /// Channel D modulo.
    pub bltdmod: i16,
    /// Size register (height\[15:6\], width\[5:0\] in words). Writing starts blit.
    pub bltsize: u16,
    /// Channel A data register.
    pub bltadat: u16,
    /// Channel B data register.
    pub bltbdat: u16,
    /// Channel C data register.
    pub bltcdat: u16,
    /// Blitter busy flag.
    pub busy: bool,
    /// Blit complete flag (for interrupt generation).
    pub done: bool,
}

impl Default for BlitterState {
    fn default() -> Self {
        Self::new()
    }
}

impl BlitterState {
    /// Create a new blitter in its initial (idle) state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bltcon0: 0,
            bltcon1: 0,
            bltafwm: 0xFFFF,
            bltalwm: 0xFFFF,
            bltapt: 0,
            bltbpt: 0,
            bltcpt: 0,
            bltdpt: 0,
            bltamod: 0,
            bltbmod: 0,
            bltcmod: 0,
            bltdmod: 0,
            bltsize: 0,
            bltadat: 0,
            bltbdat: 0,
            bltcdat: 0,
            busy: false,
            done: false,
        }
    }

    /// Returns `true` if the blitter is currently executing a blit.
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        self.busy
    }

    /// Called when `BLTSIZE` is written; initiates a blit operation.
    pub fn start_blit(&mut self) {
        self.busy = true;
        self.done = false;
    }

    /// Execute the full blit operation immediately.
    ///
    /// Reads/writes chip RAM according to the configured channels, shifts,
    /// masks, and minterm function. Non-cycle-accurate: completes in one call.
    pub fn execute_blit(&mut self, chip_ram: &mut [u8]) {
        let height = self.bltsize >> 6;
        let mut width = self.bltsize & 0x3F;
        if width == 0 {
            width = MAX_WIDTH;
        }

        let ash = (self.bltcon0 >> 12) & 0xF;
        let bsh = (self.bltcon1 >> 12) & 0xF;
        let minterm = (self.bltcon0 & 0xFF) as u8;

        for _row in 0..height {
            for col in 0..width {
                let a_raw = if self.bltcon0 & USE_A != 0 {
                    read_word(chip_ram, self.bltapt)
                } else {
                    self.bltadat
                };

                let mask = match (col == 0, col == width - 1) {
                    (true, true) => self.bltafwm & self.bltalwm,
                    (true, false) => self.bltafwm,
                    (false, true) => self.bltalwm,
                    (false, false) => 0xFFFF,
                };
                let a_masked = a_raw & mask;
                let a_shifted = barrel_shift(a_masked, ash);

                let b_raw = if self.bltcon0 & USE_B != 0 {
                    read_word(chip_ram, self.bltbpt)
                } else {
                    self.bltbdat
                };
                let b_shifted = barrel_shift(b_raw, bsh);

                let c = if self.bltcon0 & USE_C != 0 {
                    read_word(chip_ram, self.bltcpt)
                } else {
                    self.bltcdat
                };

                let result = apply_minterm(a_shifted, b_shifted, c, minterm);

                if self.bltcon0 & USE_D != 0 {
                    write_word(chip_ram, self.bltdpt, result);
                }

                if self.bltcon0 & USE_A != 0 {
                    self.bltapt = self.bltapt.wrapping_add(2);
                }
                if self.bltcon0 & USE_B != 0 {
                    self.bltbpt = self.bltbpt.wrapping_add(2);
                }
                if self.bltcon0 & USE_C != 0 {
                    self.bltcpt = self.bltcpt.wrapping_add(2);
                }
                if self.bltcon0 & USE_D != 0 {
                    self.bltdpt = self.bltdpt.wrapping_add(2);
                }
            }

            // Add modulo after each row
            if self.bltcon0 & USE_A != 0 {
                self.bltapt = add_modulo(self.bltapt, self.bltamod);
            }
            if self.bltcon0 & USE_B != 0 {
                self.bltbpt = add_modulo(self.bltbpt, self.bltbmod);
            }
            if self.bltcon0 & USE_C != 0 {
                self.bltcpt = add_modulo(self.bltcpt, self.bltcmod);
            }
            if self.bltcon0 & USE_D != 0 {
                self.bltdpt = add_modulo(self.bltdpt, self.bltdmod);
            }
        }

        self.busy = false;
        self.done = true;
    }
}

/// Compute the minterm logic function for all 256 possible minterms.
///
/// For each bit position, the minterm byte selects which combination of
/// A, B, C inputs produces a 1 in the output.
#[must_use]
pub const fn apply_minterm(a: u16, b: u16, c: u16, minterm: u8) -> u16 {
    let mut result: u16 = 0;
    if minterm & 0x01 != 0 {
        result |= !a & !b & !c;
    }
    if minterm & 0x02 != 0 {
        result |= !a & !b & c;
    }
    if minterm & 0x04 != 0 {
        result |= !a & b & !c;
    }
    if minterm & 0x08 != 0 {
        result |= !a & b & c;
    }
    if minterm & 0x10 != 0 {
        result |= a & !b & !c;
    }
    if minterm & 0x20 != 0 {
        result |= a & !b & c;
    }
    if minterm & 0x40 != 0 {
        result |= a & b & !c;
    }
    if minterm & 0x80 != 0 {
        result |= a & b & c;
    }
    result
}

/// Read a big-endian word from chip RAM at the given address.
const fn read_word(chip_ram: &[u8], addr: u32) -> u16 {
    let idx = addr as usize;
    if idx + 1 < chip_ram.len() {
        u16::from_be_bytes([chip_ram[idx], chip_ram[idx + 1]])
    } else {
        0
    }
}

/// Write a big-endian word to chip RAM at the given address.
fn write_word(chip_ram: &mut [u8], addr: u32, value: u16) {
    let idx = addr as usize;
    if idx + 1 < chip_ram.len() {
        let bytes = value.to_be_bytes();
        chip_ram[idx] = bytes[0];
        chip_ram[idx + 1] = bytes[1];
    }
}

/// Add a signed modulo to a pointer using wrapping arithmetic.
const fn add_modulo(ptr: u32, modulo: i16) -> u32 {
    if modulo >= 0 {
        ptr.wrapping_add(modulo.unsigned_abs() as u32)
    } else {
        ptr.wrapping_sub(modulo.unsigned_abs() as u32)
    }
}

/// Barrel-shift a 16-bit value right by `shift` positions.
const fn barrel_shift(value: u16, shift: u16) -> u16 {
    if shift == 0 {
        value
    } else {
        (value >> shift) | (value << (16 - shift))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minterm_f0_copies_a() {
        assert_eq!(apply_minterm(0xAAAA, 0x5555, 0xFF00, 0xF0), 0xAAAA);
        assert_eq!(apply_minterm(0x1234, 0x0000, 0xFFFF, 0xF0), 0x1234);
    }

    #[test]
    fn minterm_ca_cookie_cut() {
        // 0xCA = A ? B : C
        let a: u16 = 0xFF00;
        let b: u16 = 0x1234;
        let c: u16 = 0x5678;
        // Where A=1, take B; where A=0, take C
        let expected = (a & b) | (!a & c);
        assert_eq!(apply_minterm(a, b, c, 0xCA), expected);
    }

    #[test]
    fn simple_a_to_d_copy() {
        let mut chip_ram = vec![0u8; 256];
        // Source at offset 0: two words
        chip_ram[0] = 0xDE;
        chip_ram[1] = 0xAD;
        chip_ram[2] = 0xBE;
        chip_ram[3] = 0xEF;

        let mut blitter = BlitterState::new();
        blitter.bltcon0 = USE_A | USE_D | 0xF0; // A enabled, D enabled, minterm=copy A
        blitter.bltapt = 0;
        blitter.bltdpt = 128;
        // 1 row, 2 words wide
        blitter.bltsize = (1 << 6) | 2;
        blitter.start_blit();
        blitter.execute_blit(&mut chip_ram);

        assert_eq!(chip_ram[128], 0xDE);
        assert_eq!(chip_ram[129], 0xAD);
        assert_eq!(chip_ram[130], 0xBE);
        assert_eq!(chip_ram[131], 0xEF);
    }

    #[test]
    fn busy_done_flags_transition() {
        let mut chip_ram = vec![0u8; 64];
        let mut blitter = BlitterState::new();
        blitter.bltcon0 = USE_D | 0xF0;
        blitter.bltsize = (1 << 6) | 1;

        assert!(!blitter.is_busy());
        assert!(!blitter.done);

        blitter.start_blit();
        assert!(blitter.is_busy());
        assert!(!blitter.done);

        blitter.execute_blit(&mut chip_ram);
        assert!(!blitter.is_busy());
        assert!(blitter.done);
    }

    #[test]
    fn first_last_word_masks_applied() {
        let mut chip_ram = vec![0u8; 256];
        // Source: 3 words of 0xFFFF
        for byte in chip_ram.iter_mut().take(6) {
            *byte = 0xFF;
        }

        let mut blitter = BlitterState::new();
        blitter.bltcon0 = USE_A | USE_D | 0xF0;
        blitter.bltafwm = 0x0FFF; // mask out top nibble of first word
        blitter.bltalwm = 0xFFF0; // mask out bottom nibble of last word
        blitter.bltapt = 0;
        blitter.bltdpt = 128;
        // 1 row, 3 words wide
        blitter.bltsize = (1 << 6) | 3;
        blitter.start_blit();
        blitter.execute_blit(&mut chip_ram);

        // First word: 0xFFFF & 0x0FFF = 0x0FFF
        assert_eq!(chip_ram[128], 0x0F);
        assert_eq!(chip_ram[129], 0xFF);
        // Middle word: no mask = 0xFFFF
        assert_eq!(chip_ram[130], 0xFF);
        assert_eq!(chip_ram[131], 0xFF);
        // Last word: 0xFFFF & 0xFFF0 = 0xFFF0
        assert_eq!(chip_ram[132], 0xFF);
        assert_eq!(chip_ram[133], 0xF0);
    }
}
