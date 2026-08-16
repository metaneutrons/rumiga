// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Blitter DMA engine emulation.
//!
//! The blitter performs bulk memory operations (copy, fill, line draw) using
//! up to four DMA channels (A, B, C, D) and a configurable minterm logic
//! function. This implementation executes blits immediately (non-cycle-accurate).

#[cfg(test)]
use alloc::vec;
#[cfg(test)]
use alloc::vec::Vec;

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
    /// Active blit height in rows after legacy or ECS/AGA size decoding.
    pub vblitsize: u32,
    /// Active blit width in words after legacy or ECS/AGA size decoding.
    pub hblitsize: u32,
    /// Channel A data register.
    pub bltadat: u16,
    /// Channel B data register.
    pub bltbdat: u16,
    /// Channel C data register.
    pub bltcdat: u16,
    /// Previous raw B word for the blitter shift pipeline.
    bltbold: u16,
    /// Held/shifted B value used when channel B DMA is disabled.
    bltbhold: u16,
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
            vblitsize: 0,
            hblitsize: 0,
            bltadat: 0,
            bltbdat: 0,
            bltcdat: 0,
            bltbold: 0,
            bltbhold: 0,
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
        if self.vblitsize == 0 && self.hblitsize == 0 {
            self.decode_legacy_size();
        }
        self.bltbold = 0;
        self.busy = true;
        self.done = false;
    }

    /// Decode and start a legacy `BLTSIZE` write.
    pub fn start_legacy_size_blit(&mut self, value: u16) {
        self.bltsize = value;
        self.decode_legacy_size();
        self.start_blit();
    }

    /// Decode the ECS/AGA vertical size register.
    pub fn set_vertical_size(&mut self, value: u16) {
        self.vblitsize = u32::from(value & 0x7FFF);
        if self.vblitsize == 0 {
            self.vblitsize = 0x8000;
        }
    }

    /// Decode and start an ECS/AGA horizontal size register write.
    pub fn start_horizontal_size_blit(&mut self, value: u16) {
        self.hblitsize = u32::from(value & 0x07FF);
        if self.vblitsize == 0 {
            self.vblitsize = 0x8000;
        }
        if self.hblitsize == 0 {
            self.hblitsize = 0x0800;
        }
        self.start_blit();
    }

    fn decode_legacy_size(&mut self) {
        self.vblitsize = u32::from(self.bltsize >> 6);
        self.hblitsize = u32::from(self.bltsize & 0x3F);
        if self.vblitsize == 0 {
            self.vblitsize = 1024;
        }
        if self.hblitsize == 0 {
            self.hblitsize = u32::from(MAX_WIDTH);
        }
    }

    /// Load the B data register and update the held shifted B value.
    pub fn load_bdat(&mut self, value: u16) {
        self.bltbdat = value;
        let shift = (self.bltcon1 >> 12) & 0xF;
        let desc = (self.bltcon1 & 0x02) != 0;
        self.bltbhold = shift_dma_pair(value, self.bltbold, shift, desc);
        self.bltbold = value;
    }

    /// Execute the full blit operation immediately.
    ///
    /// Reads/writes chip RAM according to the configured channels, shifts,
    /// masks, and minterm function. Non-cycle-accurate: completes in one call.
    pub fn execute_blit(&mut self, chip_ram: &mut [u8]) {
        if self.bltcon1 & 1 != 0 {
            self.execute_line(chip_ram);
        } else {
            self.execute_area(chip_ram);
        }
        self.busy = false;
        self.done = true;
    }

    /// Execute a line-draw blit (BLTCON1 bit 0 = LINE).
    ///
    /// Implements the Amiga blitter Bresenham line algorithm using the
    /// hardware register conventions from the HRM.
    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::similar_names
    )]
    fn execute_line(&mut self, chip_ram: &mut [u8]) {
        let vblitsize = self.vblitsize;
        let hblitsize = self.hblitsize;
        if vblitsize == 0 || hblitsize < 2 {
            return;
        }
        let single = (self.bltcon1 & 0x02) != 0;
        let minterm = (self.bltcon0 & 0xFF) as u8;
        let mut blitonedot: bool = false;

        for pixel in 0..vblitsize {
            let draw_pixel = !single || !blitonedot;
            blitonedot = true;
            let current_addr = self.bltcpt;
            let negative = (self.bltcon1 & 0x40) != 0;

            if self.bltcon0 & USE_A != 0 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    let apt = self.bltapt as i16;
                    let new_apt = if negative {
                        apt.wrapping_add(self.bltbmod)
                    } else {
                        apt.wrapping_add(self.bltamod)
                    };
                    self.bltapt = u32::from(new_apt as u16);
                }
            }

            // In line mode the D channel enable bit is ignored; C must be enabled.
            if draw_pixel && (self.bltcon0 & USE_C) != 0 {
                let ashift = (self.bltcon0 >> 12) & 0xF;
                let a = (self.bltadat & self.bltafwm) >> ashift;
                let bshift = (self.bltcon1 >> 12) & 0xF;
                let blineb =
                    (self.bltbdat >> bshift) | (self.bltbdat << (16u16.wrapping_sub(bshift) & 15));
                let b: u16 = if blineb & 1 != 0 { 0xFFFF } else { 0 };
                let c = if self.bltcon0 & USE_C != 0 {
                    read_word(chip_ram, current_addr)
                } else {
                    self.bltcdat
                };
                let result = apply_minterm(a, b, c, minterm);
                let dest_addr = if pixel == 0 {
                    self.bltdpt
                } else {
                    current_addr
                };
                write_word(chip_ram, dest_addr, result);
            }

            let sud = (self.bltcon1 & 0x10) != 0;
            let sul = (self.bltcon1 & 0x08) != 0;
            let aul = (self.bltcon1 & 0x04) != 0;

            if !negative {
                if sud {
                    if sul {
                        self.line_decy();
                    } else {
                        self.line_incy();
                    }
                    blitonedot = false;
                } else if sul {
                    self.line_decx();
                } else {
                    self.line_incx();
                }
            }

            if sud {
                if aul {
                    self.line_decx();
                } else {
                    self.line_incx();
                }
            } else {
                if aul {
                    self.line_decy();
                } else {
                    self.line_incy();
                }
                blitonedot = false;
            }

            if (self.bltapt & 0x8000) != 0 {
                self.bltcon1 |= 0x40;
            } else {
                self.bltcon1 &= !0x40;
            }

            let mut bs = (self.bltcon1 >> 12) & 0xF;
            bs = bs.wrapping_sub(1) & 15;
            self.bltcon1 = (self.bltcon1 & 0x0FFF) | (bs << 12);

            self.bltdpt = self.bltcpt;
        }
    }

    fn line_incx(&mut self) {
        let ashift = (self.bltcon0 >> 12) & 0xF;
        if ashift == 15 {
            self.bltcpt = self.bltcpt.wrapping_add(2);
        }
        let new_shift = (ashift + 1) & 15;
        self.bltcon0 = (self.bltcon0 & 0x0FFF) | (new_shift << 12);
    }

    fn line_decx(&mut self) {
        let ashift = (self.bltcon0 >> 12) & 0xF;
        if ashift == 0 {
            self.bltcpt = self.bltcpt.wrapping_sub(2);
        }
        let new_shift = ashift.wrapping_sub(1) & 15;
        self.bltcon0 = (self.bltcon0 & 0x0FFF) | (new_shift << 12);
    }

    fn line_incy(&mut self) {
        self.bltcpt = add_modulo(self.bltcpt, self.bltcmod);
    }

    fn line_decy(&mut self) {
        self.bltcpt = sub_modulo(self.bltcpt, self.bltcmod);
    }

    /// Execute an area (copy/fill) blit.
    fn execute_area(&mut self, chip_ram: &mut [u8]) {
        let height = self.vblitsize;
        let width = self.hblitsize;
        if height == 0 || width == 0 {
            return;
        }
        let efe = (self.bltcon1 & 0x10) != 0; // Exclusive fill enable
        let ife = (self.bltcon1 & 0x08) != 0; // Inclusive fill enable
        let fill = efe || ife;

        if fill {
            self.execute_area_fill(chip_ram);
        } else {
            self.execute_area_nofill(chip_ram);
        }
    }

    #[inline]
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn execute_area_nofill(&mut self, chip_ram: &mut [u8]) {
        let height = self.vblitsize;
        let width = self.hblitsize;
        let ash = (self.bltcon0 >> 12) & 0xF;
        let bsh = (self.bltcon1 >> 12) & 0xF;
        let minterm = (self.bltcon0 & 0xFF) as u8;
        let desc = (self.bltcon1 & 0x02) != 0; // Descending mode
        let step: u32 = if desc { 0u32.wrapping_sub(2) } else { 2 };
        let mut a_prev: u16 = 0;
        let mut b_prev: u16 = 0;
        let mut b_hold = self.bltbhold;
        let mut pending_d: Option<(u32, u16)> = None;

        for _row in 0..height {
            for col in 0..width {
                let a_raw = if self.bltcon0 & USE_A != 0 {
                    let value = read_word(chip_ram, self.bltapt);
                    self.bltadat = value;
                    value
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
                let a_shifted = shift_dma_word(a_masked, &mut a_prev, ash, desc);

                if self.bltcon0 & USE_B != 0 {
                    let b_raw = read_word(chip_ram, self.bltbpt);
                    self.bltbdat = b_raw;
                    b_hold = shift_dma_word(b_raw, &mut b_prev, bsh, desc);
                    self.bltbold = b_raw;
                }
                let b_shifted = b_hold;

                let c = if self.bltcon0 & USE_C != 0 {
                    let value = read_word(chip_ram, self.bltcpt);
                    self.bltcdat = value;
                    if desc {
                        self.bltbdat = value;
                    }
                    value
                } else {
                    self.bltcdat
                };

                if let Some((addr, value)) = pending_d.take() {
                    write_word(chip_ram, addr, value);
                }

                let result = apply_minterm(a_shifted, b_shifted, c, minterm);

                if self.bltcon0 & USE_D != 0 {
                    pending_d = Some((self.bltdpt, result));
                }

                if self.bltcon0 & USE_A != 0 {
                    self.bltapt = self.bltapt.wrapping_add(step);
                }
                if self.bltcon0 & USE_B != 0 {
                    self.bltbpt = self.bltbpt.wrapping_add(step);
                }
                if self.bltcon0 & USE_C != 0 {
                    self.bltcpt = self.bltcpt.wrapping_add(step);
                }
                if self.bltcon0 & USE_D != 0 {
                    self.bltdpt = self.bltdpt.wrapping_add(step);
                }
            }

            // Add modulo after each row
            if self.bltcon0 & USE_A != 0 {
                self.bltapt = apply_area_modulo(self.bltapt, self.bltamod, desc);
            }
            if self.bltcon0 & USE_B != 0 {
                self.bltbpt = apply_area_modulo(self.bltbpt, self.bltbmod, desc);
            }
            if self.bltcon0 & USE_C != 0 {
                self.bltcpt = apply_area_modulo(self.bltcpt, self.bltcmod, desc);
            }
            if self.bltcon0 & USE_D != 0 {
                self.bltdpt = apply_area_modulo(self.bltdpt, self.bltdmod, desc);
            }
        }
        if let Some((addr, value)) = pending_d {
            write_word(chip_ram, addr, value);
        }
        self.bltbhold = b_hold;
    }

    #[inline]
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn execute_area_fill(&mut self, chip_ram: &mut [u8]) {
        let height = self.vblitsize;
        let width = self.hblitsize;
        let ash = (self.bltcon0 >> 12) & 0xF;
        let bsh = (self.bltcon1 >> 12) & 0xF;
        let minterm = (self.bltcon0 & 0xFF) as u8;
        let desc = (self.bltcon1 & 0x02) != 0; // Descending mode
        let ife = (self.bltcon1 & 0x08) != 0; // Inclusive fill enable
        let fci = (self.bltcon1 & 0x04) != 0; // Fill carry in
        let step: u32 = if desc { 0u32.wrapping_sub(2) } else { 2 };
        let mut a_prev: u16 = 0;
        let mut b_prev: u16 = 0;
        let mut b_hold = self.bltbhold;
        let mut pending_d: Option<(u32, u16)> = None;

        for _row in 0..height {
            let mut fill_state = fci;
            for col in 0..width {
                let a_raw = if self.bltcon0 & USE_A != 0 {
                    let value = read_word(chip_ram, self.bltapt);
                    self.bltadat = value;
                    value
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
                let a_shifted = shift_dma_word(a_masked, &mut a_prev, ash, desc);

                if self.bltcon0 & USE_B != 0 {
                    let b_raw = read_word(chip_ram, self.bltbpt);
                    self.bltbdat = b_raw;
                    b_hold = shift_dma_word(b_raw, &mut b_prev, bsh, desc);
                    self.bltbold = b_raw;
                }
                let b_shifted = b_hold;

                let c = if self.bltcon0 & USE_C != 0 {
                    let value = read_word(chip_ram, self.bltcpt);
                    self.bltcdat = value;
                    if desc {
                        self.bltbdat = value;
                    }
                    value
                } else {
                    self.bltcdat
                };

                if let Some((addr, value)) = pending_d.take() {
                    write_word(chip_ram, addr, value);
                }

                let result = apply_minterm(a_shifted, b_shifted, c, minterm);

                // Apply fill mode (processes bits right-to-left within each word)
                let mut filled: u16 = 0;
                for bit in 0..16u16 {
                    let src_bit = (result >> bit) & 1;
                    if src_bit != 0 {
                        if ife {
                            // Inclusive: set bit, then toggle state
                            filled |= 1 << bit;
                            fill_state = !fill_state;
                        } else {
                            // Exclusive: toggle state, then set if state
                            fill_state = !fill_state;
                            if fill_state {
                                filled |= 1 << bit;
                            }
                        }
                    } else if fill_state {
                        filled |= 1 << bit;
                    }
                }

                if self.bltcon0 & USE_D != 0 {
                    pending_d = Some((self.bltdpt, filled));
                }

                if self.bltcon0 & USE_A != 0 {
                    self.bltapt = self.bltapt.wrapping_add(step);
                }
                if self.bltcon0 & USE_B != 0 {
                    self.bltbpt = self.bltbpt.wrapping_add(step);
                }
                if self.bltcon0 & USE_C != 0 {
                    self.bltcpt = self.bltcpt.wrapping_add(step);
                }
                if self.bltcon0 & USE_D != 0 {
                    self.bltdpt = self.bltdpt.wrapping_add(step);
                }
            }

            // Add modulo after each row
            if self.bltcon0 & USE_A != 0 {
                self.bltapt = apply_area_modulo(self.bltapt, self.bltamod, desc);
            }
            if self.bltcon0 & USE_B != 0 {
                self.bltbpt = apply_area_modulo(self.bltbpt, self.bltbmod, desc);
            }
            if self.bltcon0 & USE_C != 0 {
                self.bltcpt = apply_area_modulo(self.bltcpt, self.bltcmod, desc);
            }
            if self.bltcon0 & USE_D != 0 {
                self.bltdpt = apply_area_modulo(self.bltdpt, self.bltdmod, desc);
            }
        }
        if let Some((addr, value)) = pending_d {
            write_word(chip_ram, addr, value);
        }
        self.bltbhold = b_hold;
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
    let idx = (addr as usize) % chip_ram.len();
    if idx + 1 < chip_ram.len() {
        u16::from_be_bytes([chip_ram[idx], chip_ram[idx + 1]])
    } else {
        0
    }
}

/// Write a big-endian word to chip RAM at the given address.
fn write_word(chip_ram: &mut [u8], addr: u32, value: u16) {
    let idx = (addr as usize) % chip_ram.len();
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

/// Subtract a signed modulo from a pointer using wrapping arithmetic.
const fn sub_modulo(ptr: u32, modulo: i16) -> u32 {
    if modulo >= 0 {
        ptr.wrapping_sub(modulo.unsigned_abs() as u32)
    } else {
        ptr.wrapping_add(modulo.unsigned_abs() as u32)
    }
}

/// Apply an area-blit row modulo in the active pointer direction.
const fn apply_area_modulo(ptr: u32, modulo: i16, desc: bool) -> u32 {
    if desc {
        sub_modulo(ptr, modulo)
    } else {
        add_modulo(ptr, modulo)
    }
}

/// Shift a DMA source word through the blitter's 32-bit shift pipeline.
fn shift_dma_word(value: u16, previous: &mut u16, shift: u16, desc: bool) -> u16 {
    let shifted = shift_dma_pair(value, *previous, shift, desc);
    *previous = value;
    shifted
}

/// Shift a source word with an explicit previous pipeline word.
fn shift_dma_pair(value: u16, previous: u16, shift: u16, desc: bool) -> u16 {
    let shifted = if shift == 0 {
        u32::from(value)
    } else if desc {
        (((u32::from(value) << 16) | u32::from(previous)) >> (16 - shift)) & 0xFFFF
    } else {
        (((u32::from(previous) << 16) | u32::from(value)) >> shift) & 0xFFFF
    };
    u16::try_from(shifted).unwrap_or(0)
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
    fn legacy_zero_bltsize_decodes_hardware_maxima() {
        let mut blitter = BlitterState::new();

        blitter.start_legacy_size_blit(0);

        assert_eq!(blitter.vblitsize, 1024);
        assert_eq!(blitter.hblitsize, u32::from(MAX_WIDTH));
    }

    #[test]
    fn ecs_aga_size_registers_decode_extended_maxima() {
        let mut blitter = BlitterState::new();

        blitter.set_vertical_size(0);
        blitter.start_horizontal_size_blit(0);

        assert_eq!(blitter.vblitsize, 0x8000);
        assert_eq!(blitter.hblitsize, 0x0800);
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

    #[test]
    fn area_shift_pipelines_bits_from_previous_word() {
        let mut chip_ram = vec![0u8; 256];
        write_word(&mut chip_ram, 0, 0x1234);
        write_word(&mut chip_ram, 2, 0x5678);

        let mut blitter = BlitterState::new();
        blitter.bltcon0 = (4 << 12) | USE_A | USE_D | 0xF0;
        blitter.bltapt = 0;
        blitter.bltdpt = 128;
        blitter.bltsize = (1 << 6) | 2;
        blitter.start_blit();
        blitter.execute_blit(&mut chip_ram);

        assert_eq!(read_word(&chip_ram, 128), 0x0123);
        assert_eq!(read_word(&chip_ram, 130), 0x4567);
    }

    #[test]
    fn descending_area_blit_subtracts_modulo_between_rows() {
        let mut chip_ram = vec![0u8; 256];
        write_word(&mut chip_ram, 0, 0xAAAA);
        write_word(&mut chip_ram, 4, 0xBBBB);

        let mut blitter = BlitterState::new();
        blitter.bltcon0 = USE_A | USE_D | 0xF0;
        blitter.bltcon1 = 0x0002;
        blitter.bltapt = 4;
        blitter.bltdpt = 36;
        blitter.bltamod = 2;
        blitter.bltdmod = 2;
        blitter.bltsize = (2 << 6) | 1;
        blitter.start_blit();
        blitter.execute_blit(&mut chip_ram);

        assert_eq!(read_word(&chip_ram, 32), 0xAAAA);
        assert_eq!(read_word(&chip_ram, 36), 0xBBBB);
    }

    #[test]
    fn line_mode_draws_steep_positive_slope() {
        assert_blitter_line_matches_bresenham((3, 1), (8, 12));
    }

    #[test]
    fn line_mode_draws_shallow_negative_slope() {
        assert_blitter_line_matches_bresenham((2, 12), (18, 4));
    }

    fn assert_blitter_line_matches_bresenham(start: (u16, u16), end: (u16, u16)) {
        const WIDTH: u16 = 32;
        const HEIGHT: u16 = 16;
        const ROW_BYTES: u16 = WIDTH / 8;

        let mut chip_ram = vec![0u8; usize::from(ROW_BYTES * HEIGHT)];
        let mut blitter = configured_line_blitter(start, end, ROW_BYTES);
        blitter.start_blit();
        blitter.execute_blit(&mut chip_ram);

        let expected = bresenham_points(start, end);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let expected_set = expected.contains(&(x, y));
                assert_eq!(
                    pixel_is_set(&chip_ram, ROW_BYTES, x, y),
                    expected_set,
                    "pixel ({x},{y}) mismatch for line {start:?}->{end:?}"
                );
            }
        }
    }

    #[allow(clippy::cast_possible_wrap)]
    fn configured_line_blitter(start: (u16, u16), end: (u16, u16), row_bytes: u16) -> BlitterState {
        let (x1, y1) = start;
        let (x2, y2) = end;
        let dx = x1.abs_diff(x2);
        let dy = y1.abs_diff(y2);
        let dmax = dx.max(dy);
        let dmin = dx.min(dy);
        let initial_error = 4 * i16::try_from(dmin).unwrap() - 2 * i16::try_from(dmax).unwrap();

        let mut octant = 0u16;
        if (dx >= dy && x1 >= x2) || (dx < dy && y1 >= y2) {
            octant |= 0x04;
        }
        if (dx >= dy && y1 >= y2) || (dx < dy && x1 >= x2) {
            octant |= 0x08;
        }
        if dx >= dy {
            octant |= 0x10;
        }

        let start_addr = u32::from(y1 * row_bytes + (x1 / 16) * 2);
        let start_bit = x1 & 0x0F;
        let mut blitter = BlitterState::new();
        blitter.bltcon0 = (start_bit << 12) | USE_A | USE_C | USE_D | 0xCA;
        blitter.bltcon1 = (start_bit << 12) | octant | 0x01;
        if initial_error < 0 {
            blitter.bltcon1 |= 0x40;
        }
        blitter.bltafwm = 0xFFFF;
        blitter.bltalwm = 0xFFFF;
        blitter.bltadat = 0x8000;
        blitter.bltbdat = 0xFFFF;
        blitter.bltamod = 4 * (i16::try_from(dmin).unwrap() - i16::try_from(dmax).unwrap());
        blitter.bltbmod = 4 * i16::try_from(dmin).unwrap();
        blitter.bltcmod = i16::try_from(row_bytes).unwrap();
        blitter.bltdmod = i16::try_from(row_bytes).unwrap();
        blitter.bltapt = u32::from(u16::from_be_bytes(initial_error.to_be_bytes()));
        blitter.bltcpt = start_addr;
        blitter.bltdpt = start_addr;
        blitter.bltsize = ((dmax + 1) << 6) | 2;
        blitter
    }

    fn bresenham_points(start: (u16, u16), end: (u16, u16)) -> Vec<(u16, u16)> {
        let (mut x0, mut y0) = (i32::from(start.0), i32::from(start.1));
        let (x1, y1) = (i32::from(end.0), i32::from(end.1));
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut points = Vec::new();

        loop {
            points.push((u16::try_from(x0).unwrap(), u16::try_from(y0).unwrap()));
            if x0 == x1 && y0 == y1 {
                return points;
            }
            let err2 = 2 * err;
            if err2 >= dy {
                err += dy;
                x0 += sx;
            }
            if err2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    fn pixel_is_set(chip_ram: &[u8], row_bytes: u16, x: u16, y: u16) -> bool {
        let addr = usize::from(y * row_bytes + (x / 16) * 2);
        let word = u16::from_be_bytes([chip_ram[addr], chip_ram[addr + 1]]);
        let bit = 15 - (x & 0x0F);
        (word >> bit) & 1 != 0
    }
}
