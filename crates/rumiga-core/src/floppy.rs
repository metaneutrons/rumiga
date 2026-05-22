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

/// MFM words in one `AmigaDOS` sector, including gap/sync/header/data.
const MFM_WORDS_PER_SECTOR: usize = 544;

/// MFM words per track (standard DD track = 12668 bytes = 6334 words).
const MFM_TRACK_WORDS: u32 = 6334;

/// Leading gap before the first `AmigaDOS` sector.
const FLOPPY_GAP_WORDS: usize =
    MFM_TRACK_WORDS as usize - SECTORS_PER_TRACK as usize * MFM_WORDS_PER_SECTOR;

/// DMA state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DskDmaState {
    /// DMA is off.
    Off,
    /// DMA is in read mode (waiting for sync or transferring).
    Read,
}

/// State of a single floppy drive.
#[derive(Clone, Debug)]
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
    /// Motor spin-up delay (in scanlines).
    pub dskready_up_time: u16,
    /// Dynamic ready state of the drive.
    pub dskready: bool,
    /// Latch indicating disk was inserted/changed since last step.
    pub disk_changed: bool,
}

impl Default for DriveState {
    fn default() -> Self {
        Self {
            data: None,
            mfm_track: Vec::new(),
            cyl: 0,
            motor: false,
            mfm_pos: 0,
            drive_id: 0,
            id_shift_count: 0,
            dskready_up_time: 0,
            dskready: false,
            disk_changed: true, // starts true (disk changed/none state)
        }
    }
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
    /// Current step direction (0=inward, 1=outward/toward track 0).
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
    /// DSKBYTR register value.
    pub dskbytr_val: u16,
}

impl FloppyController {
    /// Create a new floppy controller with all drives empty.
    #[must_use]
    pub fn new() -> Self {
        let mut drives = core::array::from_fn(|_| DriveState::default());
        drives[0].drive_id = 0xFFFF_FFFF; // DF0 standard DD ID
        Self {
            drives,
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
            dskbytr_val: 0,
        }
    }

    /// Insert an ADF image into the specified drive.
    pub fn insert_disk(&mut self, drive: usize, data: Vec<u8>) {
        if let Some(d) = self.drives.get_mut(drive) {
            d.data = Some(data);
            d.mfm_track.clear();
            d.drive_id = 0xFFFF_FFFF; // Standard present DD drive ID
            d.disk_changed = true; // Mark disk as changed so /DSKCHANGE goes low until step!
            d.dskready = false;
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

        // Drive ID and motor protocol: the motor/id flip-flop only updates on
        // a drive select high→low transition, matching Paula/WinUAE behavior.
        for dr in 0..4u8 {
            let was_sel = prev_selected & (1 << dr) == 0;
            let now_sel = self.selected & (1 << dr) == 0;
            if !was_sel && now_sel {
                let d = &mut self.drives[dr as usize];
                d.id_shift_count = (d.id_shift_count + 1) & 31;

                let next_motor = (prev_data & 0x80 == 0) || (data & 0x80 == 0);
                let prev_motor = d.motor;
                d.motor = next_motor;
                if !prev_motor && next_motor {
                    d.dskready_up_time = 5616; // 18 frames * 312 scanlines
                    d.dskready = false;
                } else if prev_motor && !next_motor {
                    d.id_shift_count = 0;
                    d.dskready = false;
                    d.dskready_up_time = 0;
                }
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
                        d.cyl = d.cyl.saturating_sub(1);
                    } else if d.cyl < 79 {
                        d.cyl += 1;
                    }
                    // Invalidate MFM cache on track change
                    d.mfm_track.clear();

                    // Clear disk changed latch if drive is not empty
                    if d.data.is_some() {
                        d.disk_changed = false;
                    }
                }
            }
        }
        self.prev_step = step_pulse;
    }

    /// Write the DSKLEN register. Double-write with bit 15 starts DMA.
    pub fn write_dsklen(&mut self, value: u16, adkcon: u16) {
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
                    // If WORDSYNC (bit 10 of ADKCON) is disabled, enable DMA immediately
                    self.dma_enable = (adkcon & 0x0400) == 0;
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

        // Populate DSKBYTR value
        let mut bytr = mfm_word & 0x00FF;
        bytr |= 0x8000; // BYTERDY (byte is ready)
        if self.dma_state != DskDmaState::Off {
            bytr |= 0x4000; // DMAON
        }
        if self.dsklen & 0x4000 != 0 {
            bytr |= 0x2000; // DISKWRITE
        }
        if self.word == self.dsksync {
            bytr |= 0x1000; // WORDEQUAL
        }
        self.dskbytr_val = bytr;

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
        let d = &self.drives[dr];
        u8::from(d.drive_id & (1 << (31 - d.id_shift_count)) != 0)
    }

    /// Check if any drive is currently selected.
    #[must_use]
    pub const fn any_drive_selected(&self) -> bool {
        self.selected != 0x0F
    }

    /// Advance floppy drive motor spin-up delays by one scanline.
    pub fn tick_scanline(&mut self) {
        for d in &mut self.drives {
            if d.dskready_up_time > 0 && d.data.is_some() {
                d.dskready_up_time = d.dskready_up_time.saturating_sub(1);
                if d.dskready_up_time == 0 && d.motor {
                    d.dskready = true;
                }
            }
        }
    }
}

impl Default for FloppyController {
    fn default() -> Self {
        Self::new()
    }
}

/// Encode one ADF track into standard `AmigaDOS` MFM format.
fn encode_mfm_track(adf: &[u8], cyl: u8, side: u8) -> Vec<u16> {
    let track_number = u32::from(cyl) * 2 + u32::from(side);
    let track_offset = track_number as usize * TRACK_SIZE as usize;

    let mut mfm = vec![0xAAAA; MFM_TRACK_WORDS as usize];
    let mut offset = FLOPPY_GAP_WORDS;
    let mut prev_bit = false;

    for sector in 0..SECTORS_PER_TRACK {
        let mut sector_mfm = [0u16; MFM_WORDS_PER_SECTOR + 1];
        sector_mfm[0] = if prev_bit { 0x2AAA } else { 0xAAAA };
        sector_mfm[1] = 0xAAAA;
        sector_mfm[2] = 0x4489;
        sector_mfm[3] = 0x4489;

        let header =
            (0xFF_u32 << 24) | (track_number << 16) | (sector << 8) | (SECTORS_PER_TRACK - sector);
        encode_mfm_long_raw(&mut sector_mfm, 4, header);

        // Four longwords of sector label are zero for plain ADF images.
        for label in 0..4 {
            encode_mfm_odd_raw(&mut sector_mfm, 8 + label * 2, 0);
            encode_mfm_even_raw(&mut sector_mfm, 16 + label * 2, 0);
        }

        let header_checksum = checksum_mfm_longs(&sector_mfm[4..24]);
        encode_mfm_long_raw(&mut sector_mfm, 24, header_checksum);

        let sec_offset = track_offset + (sector as usize) * SECTOR_SIZE as usize;
        for byte_offset in (0..SECTOR_SIZE as usize).step_by(4) {
            let idx = sec_offset + byte_offset;
            let long = if idx + 3 < adf.len() {
                u32::from_be_bytes([adf[idx], adf[idx + 1], adf[idx + 2], adf[idx + 3]])
            } else {
                0
            };
            let data_word = byte_offset / 2;
            encode_mfm_odd_raw(&mut sector_mfm, 32 + data_word, long);
            encode_mfm_even_raw(&mut sector_mfm, 288 + data_word, long);
        }

        let data_checksum = checksum_mfm_longs(&sector_mfm[32..544]);
        encode_mfm_long_raw(&mut sector_mfm, 28, data_checksum);

        mfmcode(&mut sector_mfm[4..=544]);

        for word in &sector_mfm[..MFM_WORDS_PER_SECTOR] {
            mfm[offset % MFM_TRACK_WORDS as usize] = *word;
            offset += 1;
        }
        prev_bit = sector_mfm[MFM_WORDS_PER_SECTOR - 1] & 1 != 0;
        mfm[offset % MFM_TRACK_WORDS as usize] = sector_mfm[MFM_WORDS_PER_SECTOR];
    }

    mfm
}

fn encode_mfm_long_raw(dst: &mut [u16], offset: usize, value: u32) {
    encode_mfm_odd_raw(dst, offset, value);
    encode_mfm_even_raw(dst, offset + 2, value);
}

#[allow(clippy::cast_possible_truncation)]
fn encode_mfm_odd_raw(dst: &mut [u16], offset: usize, value: u32) {
    let odd = (value >> 1) & 0x5555_5555;
    dst[offset] = (odd >> 16) as u16;
    dst[offset + 1] = odd as u16;
}

#[allow(clippy::cast_possible_truncation)]
fn encode_mfm_even_raw(dst: &mut [u16], offset: usize, value: u32) {
    let even = value & 0x5555_5555;
    dst[offset] = (even >> 16) as u16;
    dst[offset + 1] = even as u16;
}

fn checksum_mfm_longs(words: &[u16]) -> u32 {
    let mut checksum = 0u32;
    for pair in words.chunks_exact(2) {
        checksum ^= (u32::from(pair[0]) << 16) | u32::from(pair[1]);
    }
    checksum
}

#[allow(clippy::cast_possible_truncation)]
fn mfmcode(words: &mut [u16]) {
    let mut last_word = 0u32;
    for word in words {
        let value = u32::from(*word) & 0x5555_5555;
        let last_value = (last_word << 16) | value;
        let not_last_value = 0x5555_5555 & !last_value;
        let clock_bits = (not_last_value << 1) & (not_last_value >> 1);
        *word = (value | clock_bits) as u16;
        last_word = value;
    }
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
        ctrl.write_dsklen(0x8000 | 100, 0x0400);
        assert_eq!(ctrl.dma_state, DskDmaState::Off);
        ctrl.write_dsklen(0x8000 | 100, 0x0400);
        assert_eq!(ctrl.dma_state, DskDmaState::Read);
    }

    #[test]
    fn no_disk_no_sync_no_dma() {
        let mut ctrl = FloppyController::new();
        ctrl.selected = 0x0E; // DF0 selected
        ctrl.write_dsklen(0x8000 | 100, 0x0400);
        ctrl.write_dsklen(0x8000 | 100, 0x0400);

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
        ctrl.write_dsklen(0x8000 | 10, 0x0400); // 10 words
        ctrl.write_dsklen(0x8000 | 10, 0x0400);

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
    fn encoded_track_decodes_back_to_adf_sectors() {
        let mut adf = vec![0u8; (TRACK_SIZE * 160) as usize];
        for (idx, byte) in adf.iter_mut().enumerate() {
            *byte = idx.wrapping_mul(37).wrapping_add(11).to_le_bytes()[0];
        }

        let track = encode_mfm_track(&adf, 0, 0);
        let decoded = decode_amigados_track(&track, 0);

        assert_eq!(&decoded, &adf[..TRACK_SIZE as usize]);
    }

    #[test]
    fn step_changes_track() {
        let mut ctrl = FloppyController::new();
        ctrl.selected = 0x0E; // DF0 selected
        // Step inward: direction=0, then rising edge on step
        ctrl.disk_select(0b1111_0000); // DF0 selected, DIR=0, STEP=0, motor off
        ctrl.disk_select(0b1111_0001); // STEP rising edge
        assert_eq!(ctrl.drives[0].cyl, 1);

        // Step outward: direction=1 moves back toward track 0
        ctrl.disk_select(0b1111_0010); // DIR=1, STEP=0
        ctrl.disk_select(0b1111_0011); // STEP rising edge
        assert_eq!(ctrl.drives[0].cyl, 0);
    }

    #[test]
    fn motor_control() {
        let mut ctrl = FloppyController::new();
        // Select DF0 with motor bit low: motor turns on.
        ctrl.disk_select(0b0111_0000);
        assert!(ctrl.drives[0].motor);

        // The motor flip-flop only changes on the next high→low select edge.
        ctrl.disk_select(0b1111_1000); // all drives deselected
        assert!(ctrl.drives[0].motor);
        ctrl.disk_select(0b1111_0000); // DF0 selected with motor bit high
        assert!(!ctrl.drives[0].motor);
    }

    fn decode_amigados_track(track: &[u16], track_number: u8) -> Vec<u8> {
        let mut decoded = vec![0u8; TRACK_SIZE as usize];
        let mut seen = [false; SECTORS_PER_TRACK as usize];
        let mut offset = 0usize;

        while offset + 1 < track.len() && !seen.iter().all(|s| *s) {
            if track[offset] != 0x4489 || track[offset + 1] != 0x4489 {
                offset += 1;
                continue;
            }
            while offset < track.len() && track[offset] == 0x4489 {
                offset += 1;
            }

            if offset + MFM_WORDS_PER_SECTOR - 4 > track.len() {
                break;
            }

            let id_odd = get_mfm_long(track, offset);
            let id_even = get_mfm_long(track, offset + 2);
            let id = (id_odd << 1) | id_even;
            offset += 4;

            let sector = ((id >> 8) & 0xFF) as usize;
            assert_eq!(((id >> 16) & 0xFF) as u8, track_number);
            assert!(sector < SECTORS_PER_TRACK as usize);

            let mut header_checksum = id_odd ^ id_even;
            for _ in 0..4 {
                let odd = get_mfm_long(track, offset);
                let even = get_mfm_long(track, offset + 8);
                header_checksum ^= odd ^ even;
                offset += 2;
            }
            offset += 8;

            let checksum_odd = get_mfm_long(track, offset);
            let checksum_even = get_mfm_long(track, offset + 2);
            let expected_header_checksum = (checksum_odd << 1) | checksum_even;
            assert_eq!(expected_header_checksum, header_checksum);
            offset += 4;

            let data_checksum_odd = get_mfm_long(track, offset);
            let data_checksum_even = get_mfm_long(track, offset + 2);
            let mut data_checksum = (data_checksum_odd << 1) | data_checksum_even;
            offset += 4;

            for long_idx in 0..128 {
                let odd = get_mfm_long(track, offset);
                let even = get_mfm_long(track, offset + 256);
                data_checksum ^= odd ^ even;
                let data = (odd << 1) | even;
                let dst = sector * SECTOR_SIZE as usize + long_idx * 4;
                decoded[dst..dst + 4].copy_from_slice(&data.to_be_bytes());
                offset += 2;
            }
            assert_eq!(data_checksum, 0);
            offset += 256;
            seen[sector] = true;
        }

        assert!(seen.iter().all(|s| *s));
        decoded
    }

    fn get_mfm_long(track: &[u16], offset: usize) -> u32 {
        ((u32::from(track[offset]) << 16) | u32::from(track[offset + 1])) & 0x5555_5555
    }
}
