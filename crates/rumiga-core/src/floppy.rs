// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Floppy disk controller emulation matching FS-UAE/WinUAE behavior.
//!
//! Implements per-word MFM streaming with sync word detection, proper DMA
//! transfer gating, and correct interrupt timing.

/// Sectors per track in an ADF image.
const SECTORS_PER_TRACK: u32 = 11;

/// Bytes per sector.
const SECTOR_SIZE: u32 = 512;

/// Raw bytes per track in an ADF image.
const TRACK_SIZE: u32 = SECTORS_PER_TRACK * SECTOR_SIZE;

/// MFM words per track (standard DD track = 12668 bytes = 6334 words).
const MFM_TRACK_WORDS: u32 = 6334;

/// DMA state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DskDmaState {
    /// DMA is off.
    Off,
    /// DMA is in read mode (waiting for sync or transferring).
    Read,
}

/// State of a single floppy drive.
#[derive(Clone, Debug, Default)]
pub struct DriveState {
    /// ADF image data (`None` = no disk inserted).
    pub data: Option<Vec<u8>>,
    /// MFM-encoded track buffer (built on demand).
    pub mfm_track: Vec<u16>,
    /// Current cylinder (0–79).
    pub cyl: u8,
    /// Motor on/off.
    pub motor: bool,
    /// Current position in MFM track (word index).
    pub mfm_pos: u32,
    /// Drive ID shift register (32 bits, shifted out via DSKRDY).
    pub drive_id: u32,
    /// Number of ID bits remaining to shift out.
    pub id_shift_count: u8,
}

/// Floppy disk controller managing up to four drives.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct FloppyController {
    /// Drive states for DF0–DF3.
    pub drives: [DriveState; 4],
    /// Currently selected drives (bitmask from CIA-B PRB bits 3-6, active low).
    pub selected: u8,
    /// Current side (0 or 1), derived from CIA-B PRB bit 2.
    pub side: u8,
    /// Current step direction (0=outward, 1=inward).
    pub direction: u8,
    /// Previous step pulse state (for edge detection).
    prev_step: bool,
    /// Previous CIA-B PRB value (for edge detection).
    prev_prb: u8,
    /// DSKLEN register value.
    pub dsklen: u16,
    /// Previous DSKLEN value (for double-write detection).
    prev_dsklen: u16,
    /// DMA state.
    dma_state: DskDmaState,
    /// Whether sync word has been found (gates DMA transfer).
    dma_enable: bool,
    /// Remaining words to transfer.
    pub dsk_length: u16,
    /// DSKSYNC register value (sync word to match, default $4489).
    pub dsksync: u16,
    /// Shift register for sync word detection.
    word: u16,
    /// Bit offset within current word (0-15).
    bit_offset: u8,
    /// Pending DSKSYNC interrupt.
    pub pending_sync_irq: bool,
    /// Pending DSKBLK interrupt (DMA complete).
    pub pending_blk_irq: bool,
    /// Disk DMA pointer (DSKPT).
    pub dskpt: u32,
}

impl FloppyController {
    /// Create a new floppy controller with all drives empty.
    #[must_use]
    pub fn new() -> Self {
        Self {
            drives: core::array::from_fn(|_| DriveState::default()),
            selected: 0x0F, // all deselected (active low)
            side: 0,
            direction: 0,
            prev_step: false,
            prev_prb: 0xFF,
            dsklen: 0,
            prev_dsklen: 0,
            dma_state: DskDmaState::Off,
            dma_enable: false,
            dsk_length: 0,
            dsksync: 0x4489,
            word: 0,
            bit_offset: 0,
            pending_sync_irq: false,
            pending_blk_irq: false,
            dskpt: 0,
        }
    }

    /// Insert an ADF image into the specified drive.
    pub fn insert_disk(&mut self, drive: usize, data: Vec<u8>) {
        if let Some(d) = self.drives.get_mut(drive) {
            d.data = Some(data);
            d.mfm_track.clear();
        }
    }

    /// Handle CIA-B PRB write (drive selection, motor, step, side).
    /// This is the equivalent of FS-UAE's `DISK_select()`.
    pub fn disk_select(&mut self, data: u8) {
        let prev_data = self.prev_prb;
        self.prev_prb = data;

        // Extract fields
        let prev_selected = self.selected;
        self.selected = (data >> 3) & 0x0F;
        self.side = 1 - ((data >> 2) & 1);
        self.direction = (data >> 1) & 1;

        // Drive ID protocol: deselect→select resets, select→deselect shifts
        for dr in 0..4u8 {
            let was_sel = prev_selected & (1 << dr) == 0;
            let now_sel = self.selected & (1 << dr) == 0;
            if !was_sel && now_sel {
                // Reset ID register: DF0 = $FFFFFFFF (standard DD), others = $00000000 (no drive)
                let id = if dr == 0 { 0xFFFF_FFFF } else { 0x0000_0000 };
                self.drives[dr as usize].drive_id = id;
                self.drives[dr as usize].id_shift_count = 32;
            } else if was_sel && !now_sel && self.drives[dr as usize].id_shift_count > 0 {
                // Shift out one ID bit
                self.drives[dr as usize].drive_id <<= 1;
                self.drives[dr as usize].id_shift_count -= 1;
            }
        }

        // Motor: bit 7 (0=on, 1=off). Applies to selected drives.
        let motor_on = data & 0x80 == 0;
        for dr in 0..4u8 {
            if self.selected & (1 << dr) == 0 {
                // Drive is selected (active low)
                self.drives[dr as usize].motor = motor_on;
            }
        }

        // Step: rising edge of bit 0 triggers step on previously selected drives.
        let step_pulse = data & 1 != 0;
        if step_pulse && !self.prev_step {
            let prev_selected = (prev_data >> 3) & 0x0F;
            for dr in 0..4u8 {
                if prev_selected & (1 << dr) == 0 {
                    // Drive was selected
                    let d = &mut self.drives[dr as usize];
                    if self.direction != 0 {
                        if d.cyl < 79 {
                            d.cyl += 1;
                        }
                    } else {
                        d.cyl = d.cyl.saturating_sub(1);
                    }
                    // Invalidate MFM cache on track change
                    d.mfm_track.clear();
                }
            }
        }
        self.prev_step = step_pulse;
    }

    /// Write the DSKLEN register. Double-write with bit 15 starts DMA.
    pub fn write_dsklen(&mut self, value: u16) {
        let prev = self.prev_dsklen;
        self.prev_dsklen = value;
        self.dsklen = value;

        if (value & 0x8000 != 0) && (prev & 0x8000 != 0) {
            // Double-write with bit 15: start read DMA
            if value & 0x4000 == 0 {
                // Check if any selected drive has a disk
                let has_disk = (0..4u8).any(|dr| {
                    self.selected & (1 << dr) == 0 && self.drives[dr as usize].data.is_some()
                });
                if has_disk {
                    self.dma_state = DskDmaState::Read;
                    self.dma_enable = false;
                    self.dsk_length = value & 0x3FFF;
                    self.word = 0;
                    self.bit_offset = 0;
                } else {
                    // No disk: fire DSKSYNC and DSKBLK immediately so trackdisk
                    // sees sync "found" and DMA "complete" with invalid data.
                    self.dma_state = DskDmaState::Off;
                    self.pending_sync_irq = true;
                    self.pending_blk_irq = true;
                }
            }
        } else if value & 0x8000 == 0 {
            // Bit 15 clear: abort DMA
            self.dma_state = DskDmaState::Off;
            self.dma_enable = false;
        }
    }

    /// Write the DSKSYNC register.
    pub fn write_dsksync(&mut self, value: u16) {
        self.dsksync = value;
    }

    /// Execute one disk DMA word cycle. Called every 2 µs (once per word time).
    /// Returns true if chip RAM was written (for DMA slot accounting).
    ///
    /// # Panics
    /// Panics if MFM track encoding fails (should not happen with valid ADF data).
    #[allow(clippy::cast_possible_truncation, clippy::same_item_push)]
    pub fn disk_dma_cycle(&mut self, chip_ram: &mut [u8]) -> bool {
        if self.dma_state != DskDmaState::Read {
            return false;
        }

        // Find the first selected drive with a disk
        let drv_idx = (0..4u8)
            .find(|&dr| self.selected & (1 << dr) == 0 && self.drives[dr as usize].data.is_some());

        let Some(drv_idx) = drv_idx else {
            // No disk in any selected drive — no data streams, sync never found.
            // Advance bit_offset to simulate time passing (for timeout).
            return false;
        };

        let drv = &mut self.drives[drv_idx as usize];

        // Ensure MFM track is built
        if drv.mfm_track.is_empty() {
            drv.mfm_track = encode_mfm_track(drv.data.as_deref().unwrap(), drv.cyl, self.side);
        }

        // Read next word from MFM track
        #[allow(clippy::cast_possible_truncation, clippy::same_item_push)]
        let track_len = drv.mfm_track.len() as u32;
        let mfm_word = drv.mfm_track[drv.mfm_pos as usize % track_len as usize];
        drv.mfm_pos = (drv.mfm_pos + 1) % track_len;

        // Shift into word register for sync detection
        self.word = mfm_word;

        // Check for sync word match
        if !self.dma_enable && self.word == self.dsksync {
            // Sync found! Fire DSKSYNC interrupt and enable DMA transfer.
            self.dma_enable = true;
            self.pending_sync_irq = true;
        }

        // If DMA is enabled (sync was found), transfer word to chip RAM
        if self.dma_enable && self.dsk_length > 0 {
            let addr = self.dskpt as usize;
            if addr + 1 < chip_ram.len() {
                chip_ram[addr] = (mfm_word >> 8) as u8;
                chip_ram[addr + 1] = (mfm_word & 0xFF) as u8;
            }
            self.dskpt = self.dskpt.wrapping_add(2);
            self.dsk_length -= 1;

            if self.dsk_length == 0 {
                // DMA complete — fire DSKBLK interrupt
                self.pending_blk_irq = true;
                self.dma_state = DskDmaState::Off;
                self.dma_enable = false;
            }
            return true;
        }

        false
    }

    /// Returns the first selected drive index (active low), or 0 if none.
    #[must_use]
    pub fn first_selected_drive(&self) -> usize {
        for dr in 0..4u8 {
            if self.selected & (1 << dr) == 0 {
                return dr as usize;
            }
        }
        0
    }

    /// Check if a drive is at track 0.
    #[must_use]
    pub fn at_track0(&self) -> bool {
        let dr = self.first_selected_drive();
        self.drives[dr].cyl == 0
    }

    /// Check if a selected drive has a disk.
    #[must_use]
    pub fn has_disk(&self) -> bool {
        let dr = self.first_selected_drive();
        self.drives[dr].data.is_some()
    }

    /// Check if motor is on for a selected drive.
    #[must_use]
    pub fn motor_on(&self) -> bool {
        let dr = self.first_selected_drive();
        self.drives[dr].motor
    }

    /// Get the current drive ID bit (MSB of shift register) for the selected drive.
    /// Returns 0 for standard DD drive, 1 for no drive / HD drive.
    #[must_use]
    pub fn drive_id_bit(&self) -> u8 {
        let dr = self.first_selected_drive();
        u8::from(self.drives[dr].drive_id & 0x8000_0000 != 0)
    }

    /// Check if any drive is currently selected.
    #[must_use]
    pub const fn any_drive_selected(&self) -> bool {
        self.selected != 0x0F
    }
}

impl Default for FloppyController {
    fn default() -> Self {
        Self::new()
    }
}

/// Encode one ADF track into MFM format.
/// Produces a standard Amiga MFM track with sync words and sector headers.
#[allow(clippy::same_item_push)]
fn encode_mfm_track(adf: &[u8], cyl: u8, side: u8) -> Vec<u16> {
    let track_idx = (u32::from(cyl) * 2 + u32::from(side)) as usize;
    let track_offset = track_idx * TRACK_SIZE as usize;

    let mut mfm = Vec::with_capacity(MFM_TRACK_WORDS as usize);

    // Gap before first sector
    for _ in 0..2 {
        mfm.push(0xAAAA);
    }

    for sector in 0..SECTORS_PER_TRACK {
        // Sync words
        mfm.push(0x4489);
        mfm.push(0x4489);

        // Sector header (simplified MFM encoding of header info)
        let info = (0xFF_u32 << 24)
            | ((u32::from(cyl) * 2 + u32::from(side)) << 16)
            | (sector << 8)
            | (SECTORS_PER_TRACK - sector);
        mfm.push(mfm_encode_long_odd(info));
        mfm.push(mfm_encode_long_even(info));

        // Sector label (8 words of 0)
        for _ in 0..8 {
            mfm.push(0xAAAA);
        }

        // Header checksum (simplified)
        mfm.push(0xAAAA);
        mfm.push(0xAAAA);

        // Data checksum (simplified)
        mfm.push(0xAAAA);
        mfm.push(0xAAAA);

        // Sector data (256 words = 512 bytes MFM encoded)
        let sec_offset = track_offset + (sector as usize) * SECTOR_SIZE as usize;
        for i in (0..SECTOR_SIZE as usize).step_by(4) {
            let idx = sec_offset + i;
            if idx + 3 < adf.len() {
                let long = u32::from_be_bytes([adf[idx], adf[idx + 1], adf[idx + 2], adf[idx + 3]]);
                mfm.push(mfm_encode_long_odd(long));
                mfm.push(mfm_encode_long_even(long));
            } else {
                mfm.push(0xAAAA);
                mfm.push(0xAAAA);
            }
        }

        // Inter-sector gap
        mfm.push(0xAAAA);
    }

    // Pad to standard track length
    while mfm.len() < MFM_TRACK_WORDS as usize {
        mfm.push(0xAAAA);
    }
    mfm.truncate(MFM_TRACK_WORDS as usize);
    mfm
}

/// MFM encode the odd bits of a longword.
#[allow(clippy::cast_possible_truncation)]
const fn mfm_encode_long_odd(v: u32) -> u16 {
    let odd = ((v >> 1) & 0x5555_5555) as u16; // truncation intended
    odd | 0xAAAA & !odd
}

/// MFM encode the even bits of a longword.
#[allow(clippy::cast_possible_truncation)]
const fn mfm_encode_long_even(v: u32) -> u16 {
    let even = (v & 0x5555_5555) as u16; // truncation intended
    even | 0xAAAA & !even
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsklen_double_write_starts_dma() {
        let mut ctrl = FloppyController::new();
        let adf = vec![0u8; (TRACK_SIZE * 160) as usize];
        ctrl.insert_disk(0, adf);
        ctrl.selected = 0x0E; // DF0 selected
        ctrl.write_dsklen(0x8000 | 100);
        assert_eq!(ctrl.dma_state, DskDmaState::Off);
        ctrl.write_dsklen(0x8000 | 100);
        assert_eq!(ctrl.dma_state, DskDmaState::Read);
    }

    #[test]
    fn no_disk_no_sync_no_dma() {
        let mut ctrl = FloppyController::new();
        ctrl.selected = 0x0E; // DF0 selected
        ctrl.write_dsklen(0x8000 | 100);
        ctrl.write_dsklen(0x8000 | 100);

        let mut ram = vec![0u8; 65536];
        // Run many cycles — no sync should be found
        for _ in 0..20000 {
            ctrl.disk_dma_cycle(&mut ram);
        }
        assert!(!ctrl.dma_enable);
        assert!(ctrl.pending_blk_irq); // DSKBLK fires immediately with no disk
        assert!(ctrl.pending_sync_irq); // DSKSYNC fires immediately with no disk
    }

    #[test]
    fn with_disk_sync_found_and_dma_completes() {
        let mut ctrl = FloppyController::new();
        let adf = vec![0u8; (TRACK_SIZE * 160) as usize];
        ctrl.insert_disk(0, adf);
        ctrl.selected = 0x0E; // DF0 selected
        ctrl.drives[0].motor = true;
        ctrl.dskpt = 0x1000;
        ctrl.write_dsklen(0x8000 | 10); // 10 words
        ctrl.write_dsklen(0x8000 | 10);

        let mut ram = vec![0u8; 65536];
        let mut cycles = 0;
        while !ctrl.pending_blk_irq && cycles < 20000 {
            ctrl.disk_dma_cycle(&mut ram);
            cycles += 1;
        }
        assert!(ctrl.pending_sync_irq, "sync should have been found");
        assert!(ctrl.pending_blk_irq, "DMA should have completed");
    }

    #[test]
    fn step_changes_track() {
        let mut ctrl = FloppyController::new();
        ctrl.selected = 0x0E; // DF0 selected
        // Step inward: direction=1, then rising edge on step
        ctrl.disk_select(0b1000_0010); // SEL0=0, DIR=1, STEP=0, MTR=0
        ctrl.disk_select(0b1000_0011); // STEP rising edge
        assert_eq!(ctrl.drives[0].cyl, 1);
    }

    #[test]
    fn motor_control() {
        let mut ctrl = FloppyController::new();
        // Select DF0, motor on (bit 7 = 0)
        ctrl.disk_select(0b0000_0110); // SEL0=0, MTR=0
        assert!(ctrl.drives[0].motor);
        // Motor off (bit 7 = 1)
        ctrl.disk_select(0b1000_0110); // SEL0=0, MTR=1
        assert!(!ctrl.drives[0].motor);
    }
}
