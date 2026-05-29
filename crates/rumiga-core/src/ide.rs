// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Fabian Schmieder

//! Gayle ATA/IDE controller emulation for Rumiga.

#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::unreadable_literal
)]

/// ATA Status bits.
pub const IDE_STATUS_BSY: u8 = 0x80;
pub const IDE_STATUS_DRDY: u8 = 0x40;
pub const IDE_STATUS_DF: u8 = 0x20;
pub const IDE_STATUS_DSC: u8 = 0x10;
pub const IDE_STATUS_DRQ: u8 = 0x08;
pub const IDE_STATUS_ERR: u8 = 0x01;

/// ATA registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataDirection {
    None,
    Read,
    Write,
}

/// Simplified ATA-2 controller state machine.
#[derive(Debug, Clone)]
pub struct AtaController {
    pub status: u8,
    pub error: u8,
    pub nsector: u8,
    pub sector: u8,
    pub lcyl: u8,
    pub hcyl: u8,
    pub select: u8,
    pub devcon: u8,
    pub command: u8,
    pub command_log: Vec<u8>,

    /// 512-byte sector data buffer for active transfers.
    pub data_buffer: Vec<u8>,
    pub data_index: usize,
    pub data_direction: DataDirection,

    /// Backing hardfile image contents.
    pub disk_data: Option<Vec<u8>>,
    pub hdf_dirty: bool,

    /// Triggers Level 2 interrupt to the custom chipset/CPU.
    pub pending_irq: bool,

    /// CHS parameters.
    pub cylinders: u16,
    pub heads: u8,
    pub sectors_per_track: u8,
}

impl Default for AtaController {
    fn default() -> Self {
        Self::new()
    }
}

impl AtaController {
    /// Create a new ATA controller in default signature state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            status: IDE_STATUS_DRDY | IDE_STATUS_DSC,
            error: 0x01,
            nsector: 0x01,
            sector: 0x01,
            lcyl: 0x00,
            hcyl: 0x00,
            select: 0x00,
            devcon: 0x00,
            command: 0x00,
            command_log: Vec::new(),
            data_buffer: Vec::new(),
            data_index: 0,
            data_direction: DataDirection::None,
            disk_data: None,
            hdf_dirty: false,
            pending_irq: false,
            cylinders: 0,
            heads: 16,
            sectors_per_track: 63,
        }
    }

    /// Mount a host HDF file in-memory.
    pub fn insert_disk(&mut self, data: Vec<u8>) {
        let size = data.len();
        self.disk_data = Some(data);
        self.hdf_dirty = false;

        // Auto-configure geometry based on standard LBA-CHS translations
        let total_sectors = (size / 512) as u32;
        if total_sectors > 0 {
            self.heads = 16;
            self.sectors_per_track = 63;
            self.cylinders = (total_sectors / (16 * 63)).min(16383) as u16;
        } else {
            self.cylinders = 0;
        }
    }

    /// Total addressable 512-byte sectors in the mounted disk.
    #[must_use]
    pub fn total_sectors(&self) -> u32 {
        self.disk_data
            .as_ref()
            .map_or(0, |data| (data.len() / 512).min(u32::MAX as usize) as u32)
    }

    /// Read an ATA taskfile register.
    pub fn read_register(&mut self, reg: usize, is_control: bool) -> u8 {
        if self.disk_data.is_none() {
            if reg == 7 && !is_control {
                return 0x7F;
            }
            return 0xFF;
        }

        // If drive 1 (slave) is selected, emulate FS-UAE's missing-drive
        // response while the master exists: status/alt-status reports ERR,
        // other taskfile registers read as zero.
        let is_slave = (self.select & 0x10) != 0;
        if is_slave {
            match reg {
                7 => return IDE_STATUS_ERR,
                _ => return 0x00,
            }
        }

        if is_control {
            // Control Block: index 7 represents Alternate Status.
            if reg == 7 {
                return self.status;
            }
            return 0xFF;
        }

        match reg {
            0 => {
                // DATA register byte reads (fallback, usually word reads are used)
                self.read_data_byte()
            }
            1 => self.error,
            2 => self.nsector,
            3 => self.sector,
            4 => self.lcyl,
            5 => self.hcyl,
            6 => self.select,
            7 => {
                // Reading STATUS clears Gayle interrupts on Gayle status line.
                self.status
            }
            _ => 0xFF,
        }
    }

    /// Read the ATA drive address register.
    #[must_use]
    pub fn read_drive_address(&self) -> u8 {
        ((if (self.select & 0x10) != 0 { 2 } else { 1 }) | ((self.select & 0x0F) << 2)) ^ 0xFF
    }

    /// Write an ATA taskfile register.
    pub fn write_register(&mut self, reg: usize, is_control: bool, value: u8) {
        if is_control {
            if reg == 7 {
                let old_devcon = self.devcon;
                self.devcon = value;
                if (old_devcon & 0x04) == 0 && (value & 0x04) != 0 {
                    // Software reset bit set
                    self.status = IDE_STATUS_BSY;
                    self.error = 0x01;
                    self.nsector = 0x01;
                    self.sector = 0x01;
                    self.lcyl = 0x00;
                    self.hcyl = 0x00;
                    self.data_direction = DataDirection::None;
                } else if (old_devcon & 0x04) != 0 && (value & 0x04) == 0 {
                    // Software reset bit cleared
                    self.status = IDE_STATUS_DRDY | IDE_STATUS_DSC;
                }
            }
            return;
        }

        let is_slave = (self.select & 0x10) != 0;
        if is_slave && reg != 6 {
            // Writing to slave when not present is ignored.
            return;
        }

        match reg {
            0 => {
                // DATA register byte writes (fallback)
                self.write_data_byte(value);
            }
            1 => {} // FEATURES register
            2 => self.nsector = value,
            3 => self.sector = value,
            4 => self.lcyl = value,
            5 => self.hcyl = value,
            6 => self.select = value,
            7 => {
                self.write_command(value);
            }
            _ => {}
        }
    }

    /// Execute an ATA command.
    #[allow(clippy::too_many_lines)]
    pub fn write_command(&mut self, cmd: u8) {
        self.command = cmd;
        if self.command_log.len() == 32 {
            self.command_log.remove(0);
        }
        self.command_log.push(cmd);
        self.status |= IDE_STATUS_BSY;
        self.status &= !IDE_STATUS_ERR;
        self.error = 0x01;

        match cmd {
            0xEC => {
                // Identify Device
                self.populate_identify_buffer();
                self.status &= !IDE_STATUS_BSY;
                self.status |= IDE_STATUS_DRQ | IDE_STATUS_DRDY;
                self.data_index = 0;
                self.data_direction = DataDirection::Read;
                self.pending_irq = true;
            }
            0x20 | 0x21 | 0xC4 => {
                // Read Sectors / Read Multiple
                let lba = self.current_lba();
                let count = if self.nsector == 0 {
                    256
                } else {
                    self.nsector as u32
                };

                self.data_buffer = vec![0; (count * 512) as usize];
                if let Some(ref data) = self.disk_data {
                    let start_offset = (lba as usize) * 512;
                    let end_offset = start_offset + (count as usize) * 512;
                    if start_offset < data.len() {
                        let actual_end = end_offset.min(data.len());
                        let len = actual_end - start_offset;
                        self.data_buffer[0..len].copy_from_slice(&data[start_offset..actual_end]);
                    }
                }

                self.status &= !IDE_STATUS_BSY;
                self.status |= IDE_STATUS_DRQ | IDE_STATUS_DRDY;
                self.data_index = 0;
                self.data_direction = DataDirection::Read;
                self.pending_irq = true;
            }
            0x30 | 0x31 | 0xC5 => {
                // Write Sectors / Write Multiple
                let count = if self.nsector == 0 {
                    256
                } else {
                    self.nsector as u32
                };

                self.data_buffer = vec![0; (count * 512) as usize];
                self.status &= !IDE_STATUS_BSY;
                self.status |= IDE_STATUS_DRQ | IDE_STATUS_DRDY;
                self.data_index = 0;
                self.data_direction = DataDirection::Write;
            }
            0x91 => {
                // Initialize Drive Parameters
                self.heads = (self.select & 0x0F) + 1;
                self.sectors_per_track = self.nsector;
                self.status &= !IDE_STATUS_BSY;
                self.status |= IDE_STATUS_DRDY;
                self.data_direction = DataDirection::None;
                self.pending_irq = true;
            }
            0x90 => {
                // Execute Diagnostics
                self.error = 0x01; // Passed
                self.nsector = 0x01;
                self.sector = 0x01;
                self.lcyl = 0x00;
                self.hcyl = 0x00;
                self.status &= !IDE_STATUS_BSY;
                self.status |= IDE_STATUS_DRDY;
                self.data_direction = DataDirection::None;
                self.pending_irq = true;
            }
            0xEF => {
                // Set Features (Success NOP)
                self.status &= !IDE_STATUS_BSY;
                self.status |= IDE_STATUS_DRDY;
                self.data_direction = DataDirection::None;
                self.pending_irq = true;
            }
            0x10..=0x1F => {
                // Recalibrate / Seek
                self.sector = 0x01;
                self.lcyl = 0x00;
                self.hcyl = 0x00;
                self.status &= !IDE_STATUS_BSY;
                self.status |= IDE_STATUS_DRDY;
                self.data_direction = DataDirection::None;
                self.pending_irq = true;
            }
            0x40 | 0x41 => {
                // Verify Sectors (Success NOP)
                self.status &= !IDE_STATUS_BSY;
                self.status |= IDE_STATUS_DRDY;
                self.data_direction = DataDirection::None;
                self.pending_irq = true;
            }
            0xC6 => {
                // Set Multiple Mode (Success NOP)
                self.status &= !IDE_STATUS_BSY;
                self.status |= IDE_STATUS_DRDY;
                self.data_direction = DataDirection::None;
                self.pending_irq = true;
            }
            _ => {
                // Aborted/Unsupported command
                self.status &= !IDE_STATUS_BSY;
                self.status |= IDE_STATUS_DRDY | IDE_STATUS_ERR;
                self.error = 0x04; // Aborted command
                self.data_direction = DataDirection::None;
                self.pending_irq = true;
            }
        }
    }

    /// Read a 16-bit word from the IDE data port.
    pub fn read_data_word(&mut self) -> u16 {
        if self.data_direction != DataDirection::Read || self.data_buffer.is_empty() {
            return 0xFFFF;
        }

        let idx = self.data_index;
        let mut val = if idx + 1 < self.data_buffer.len() {
            let high = self.data_buffer[idx] as u16;
            let low = self.data_buffer[idx + 1] as u16;
            (high << 8) | low
        } else {
            0xFFFF
        };

        if self.command == 0xEC {
            val = val.swap_bytes();
        }

        self.data_index += 2;
        if self.data_index >= self.data_buffer.len() {
            self.status &= !IDE_STATUS_DRQ;
            self.data_direction = DataDirection::None;
        } else if self.data_index % 512 == 0 {
            // Finished reading a sector, but more sectors remain!
            // Fire another interrupt to signal next sector data is ready.
            self.pending_irq = true;
        }

        val
    }

    /// Write a 16-bit word to the IDE data port.
    pub fn write_data_word(&mut self, val: u16) {
        if self.data_direction != DataDirection::Write || self.data_buffer.is_empty() {
            return;
        }

        let idx = self.data_index;
        if idx + 1 < self.data_buffer.len() {
            self.data_buffer[idx] = (val >> 8) as u8;
            self.data_buffer[idx + 1] = (val & 0xFF) as u8;
        }

        self.data_index += 2;
        if self.data_index >= self.data_buffer.len() {
            // Finished receiving data: commit to in-memory hardfile buffer
            self.status |= IDE_STATUS_BSY;
            self.status &= !IDE_STATUS_DRQ;

            let lba = self.current_lba();
            let count = (self.data_buffer.len() / 512) as u32;

            if let Some(ref mut data) = self.disk_data {
                let start_offset = (lba as usize) * 512;
                let end_offset = start_offset + (count as usize) * 512;
                if end_offset > data.len() {
                    data.resize(end_offset, 0);
                }
                data[start_offset..end_offset].copy_from_slice(&self.data_buffer);
                self.hdf_dirty = true;
            }

            self.status &= !IDE_STATUS_BSY;
            self.data_direction = DataDirection::None;
            self.pending_irq = true;
        } else if self.data_index % 512 == 0 {
            // Finished receiving one sector of data, but more sectors remain!
            // Fire another interrupt to signal ready for next sector.
            self.pending_irq = true;
        }
    }

    fn read_data_byte(&mut self) -> u8 {
        if self.data_direction != DataDirection::Read || self.data_buffer.is_empty() {
            return 0xFF;
        }
        let val = self.data_buffer[self.data_index];
        self.data_index += 1;
        if self.data_index >= self.data_buffer.len() {
            self.status &= !IDE_STATUS_DRQ;
            self.data_direction = DataDirection::None;
        } else if self.data_index % 512 == 0 {
            self.pending_irq = true;
        }
        val
    }

    fn write_data_byte(&mut self, val: u8) {
        if self.data_direction != DataDirection::Write || self.data_buffer.is_empty() {
            return;
        }
        let idx = self.data_index;
        self.data_buffer[idx] = val;
        self.data_index += 1;
        if self.data_index >= self.data_buffer.len() {
            self.status |= IDE_STATUS_BSY;
            self.status &= !IDE_STATUS_DRQ;

            let lba = self.current_lba();
            let count = (self.data_buffer.len() / 512) as u32;

            if let Some(ref mut data) = self.disk_data {
                let start_offset = (lba as usize) * 512;
                let end_offset = start_offset + (count as usize) * 512;
                if end_offset > data.len() {
                    data.resize(end_offset, 0);
                }
                data[start_offset..end_offset].copy_from_slice(&self.data_buffer);
                self.hdf_dirty = true;
            }

            self.status &= !IDE_STATUS_BSY;
            self.data_direction = DataDirection::None;
            self.pending_irq = true;
        } else if self.data_index % 512 == 0 {
            self.pending_irq = true;
        }
    }

    /// Calculate active LBA address from taskfile registers.
    #[must_use]
    pub fn current_lba(&self) -> u32 {
        if (self.select & 0x40) != 0 {
            let lba0 = self.sector as u32;
            let lba1 = self.lcyl as u32;
            let lba2 = self.hcyl as u32;
            let lba3 = (self.select & 0x0F) as u32;
            lba0 | (lba1 << 8) | (lba2 << 16) | (lba3 << 24)
        } else {
            let c = self.lcyl as u32 | ((self.hcyl as u32) << 8);
            let h = (self.select & 0x0F) as u32;
            let s = self.sector.saturating_sub(1) as u32;
            (c * (self.heads as u32) + h) * (self.sectors_per_track as u32) + s
        }
    }

    /// Populate standard IDE Identify Device buffer (512 bytes).
    fn populate_identify_buffer(&mut self) {
        self.data_buffer = vec![0; 512];
        let total_lba_sectors = self.total_sectors();
        let chs_sectors =
            u32::from(self.cylinders) * u32::from(self.heads) * u32::from(self.sectors_per_track);

        // Word 0: Configuration
        self.write_word(0, 0x0040);
        // Word 1: Default cylinders
        self.write_word(1, self.cylinders);
        // Word 2: Specific configuration (matches FS-UAE's generic ATA identity)
        self.write_word(2, 0xC837);
        // Word 3: Default heads
        self.write_word(3, self.heads as u16);
        // Words 4-5: Unformatted bytes per track/sector.
        self.write_word(4, 512 * u16::from(self.sectors_per_track));
        self.write_word(5, 512);
        // Word 6: Default sectors per track
        self.write_word(6, self.sectors_per_track as u16);

        // Word 10-19: Serial number (ATA byte-swapped ASCII)
        self.write_string(10, "RUMIGA-000001", 20);

        self.write_word(20, 3);
        self.write_word(21, 512);
        self.write_word(22, 4);

        // Word 23-26: Firmware revision
        self.write_string(23, "1.0", 8);

        // Word 27-46: Model number
        self.write_string(27, "RUMIGA VIRTUAL ATA HARDDISK", 40);

        // FS-UAE-compatible capability/validity fields.
        self.write_word(47, 0x0000); // multiple sector commands not supported
        self.write_word(48, 0x0001);
        self.write_word(49, (1 << 9) | (1 << 8)); // LBA and DMA supported
        self.write_word(51, 0x0200);
        self.write_word(52, 0x0200);
        self.write_word(53, 0x0007); // words 54-58, 64-70, and 88 are valid

        // Word 54-56: Current CHS parameters
        self.write_word(54, self.cylinders);
        self.write_word(55, self.heads as u16);
        self.write_word(56, self.sectors_per_track as u16);
        self.write_u32_words(57, chs_sectors);
        self.write_word(59, 0x0000); // multiple sector setting not valid

        // Word 60-61: LBA total sectors capacity (32-bit LBA)
        self.write_u32_words(60, total_lba_sectors.min(0x0FFF_FFFF));
        self.write_word(62, 0x000F);
        self.write_word(63, 0x000F);
        self.write_word(64, 0x0003); // PIO3/PIO4 supported
        self.write_word(65, 120);
        self.write_word(66, 120);
        self.write_word(67, 120);
        self.write_word(68, 120);
        self.write_word(80, 0x007E); // ATA-1 through ATA-6
        self.write_word(81, 0x001C);
        self.write_word(82, 1 << 14);
        self.write_word(83, (1 << 14) | (1 << 13) | (1 << 12));
        self.write_word(84, 1 << 14);
        self.write_word(85, 1 << 14);
        self.write_word(86, (1 << 14) | (1 << 13) | (1 << 12));
        self.write_word(87, 1 << 14);
        self.write_word(88, 0x003F);
        self.write_word(93, (1 << 14) | (1 << 13) | 1);
    }

    fn write_word(&mut self, word_index: usize, val: u16) {
        let offset = word_index * 2;
        if offset + 1 < self.data_buffer.len() {
            self.data_buffer[offset] = (val >> 8) as u8;
            self.data_buffer[offset + 1] = (val & 0xFF) as u8;
        }
    }

    fn write_u32_words(&mut self, word_index: usize, val: u32) {
        self.write_word(word_index, (val & 0xFFFF) as u16);
        self.write_word(word_index + 1, (val >> 16) as u16);
    }

    fn write_string(&mut self, word_index: usize, s: &str, max_len: usize) {
        let mut bytes = vec![b' '; max_len];
        for (i, b) in s.bytes().take(max_len).enumerate() {
            bytes[i] = b;
        }
        for i in (0..max_len).step_by(2) {
            if i + 1 < max_len {
                let offset = word_index * 2 + i;
                self.data_buffer[offset] = bytes[i];
                self.data_buffer[offset + 1] = bytes[i + 1];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identify_word(controller: &AtaController, word_index: usize) -> u16 {
        let offset = word_index * 2;
        (u16::from(controller.data_buffer[offset]) << 8)
            | u16::from(controller.data_buffer[offset + 1])
    }

    #[test]
    fn test_current_lba_chs() {
        let mut controller = AtaController::new();
        controller.heads = 16;
        controller.sectors_per_track = 63;
        controller.sector = 5; // 1-indexed CHS sector
        controller.lcyl = 10;
        controller.hcyl = 0;
        controller.select = 2; // head = 2, LBA = 0

        // CHS calculation: (c * heads + h) * sectors_per_track + (s - 1)
        // (10 * 16 + 2) * 63 + (5 - 1) = 162 * 63 + 4 = 10206 + 4 = 10210
        assert_eq!(controller.current_lba(), 10210);
    }

    #[test]
    fn test_current_lba_lba() {
        let mut controller = AtaController::new();
        controller.sector = 0x12;
        controller.lcyl = 0x34;
        controller.hcyl = 0x56;
        controller.select = 0x47; // LBA mode active (bit 6 = 1), head = 7

        // LBA calculation: sector | (lcyl << 8) | (hcyl << 16) | ((select & 0x0F) << 24)
        // 0x12 | (0x34 << 8) | (0x56 << 16) | (0x07 << 24) = 0x07563412
        assert_eq!(controller.current_lba(), 0x07563412);
    }

    #[test]
    fn test_identify_device() {
        let mut controller = AtaController::new();
        // Insert disk so it's not offline
        controller.insert_disk(vec![0; 10 * 1024 * 1024]); // 10MB
        controller.write_command(0xEC);

        assert_eq!(controller.data_direction, DataDirection::Read);
        assert!((controller.status & IDE_STATUS_DRQ) != 0);
        assert_eq!(controller.data_buffer.len(), 512);

        // Word 0 should be configuration (0x0040, byte-swapped)
        let word_0 = controller.read_data_word();
        assert_eq!(word_0, 0x4000);

        // Word 1 should have cylinders (byte-swapped)
        let word_1 = controller.read_data_word();
        assert_eq!(word_1, controller.cylinders.swap_bytes());
    }

    #[test]
    fn test_identify_device_reports_fs_uae_style_capabilities() {
        let mut controller = AtaController::new();
        controller.insert_disk(vec![0; 10 * 1024 * 1024]);
        controller.write_command(0xEC);

        assert_eq!(identify_word(&controller, 47), 0x0000);
        assert_eq!(identify_word(&controller, 49), 0x0300);
        assert_eq!(identify_word(&controller, 53), 0x0007);
        assert_eq!(identify_word(&controller, 59), 0x0000);
        assert_eq!(identify_word(&controller, 60), 20480);
        assert_eq!(identify_word(&controller, 61), 0);
        assert_eq!(identify_word(&controller, 80), 0x007E);
        assert_eq!(identify_word(&controller, 27), 0x5255); // "RU"
    }

    #[test]
    fn test_missing_slave_status_reports_error() {
        let mut controller = AtaController::new();
        controller.insert_disk(vec![0; 10 * 1024 * 1024]);
        controller.select = 0x10;

        assert_eq!(controller.read_register(7, false), IDE_STATUS_ERR);
        assert_eq!(controller.read_register(7, true), IDE_STATUS_ERR);
        assert_eq!(controller.read_register(2, false), 0);
    }

    #[test]
    fn test_read_sectors() {
        let mut controller = AtaController::new();
        // 10 sectors of data, each has unique byte pattern
        let mut disk_data = vec![0; 5120];
        disk_data[512] = 0xAA;
        disk_data[513] = 0xBB;
        controller.insert_disk(disk_data);

        // Read 1 sector starting from LBA 1
        controller.sector = 2; // CHS s=2 is LBA 1 (if c=0, h=0, sectors_per_track=63)
        controller.lcyl = 0;
        controller.hcyl = 0;
        controller.select = 0;
        controller.nsector = 1;

        assert_eq!(controller.current_lba(), 1);

        controller.write_command(0x20); // Read Sectors

        assert_eq!(controller.data_direction, DataDirection::Read);
        let first_word = controller.read_data_word();
        assert_eq!(first_word, 0xAABB);
    }

    #[test]
    fn test_write_sectors() {
        let mut controller = AtaController::new();
        controller.insert_disk(vec![0; 5120]);

        controller.sector = 2; // LBA 1
        controller.lcyl = 0;
        controller.hcyl = 0;
        controller.select = 0;
        controller.nsector = 1;

        controller.write_command(0x30); // Write Sectors

        assert_eq!(controller.data_direction, DataDirection::Write);

        // Write 256 words (512 bytes)
        for i in 0..256 {
            controller.write_data_word(i as u16);
        }

        assert_eq!(controller.data_direction, DataDirection::None);
        assert!(controller.hdf_dirty);

        let written_disk = controller.disk_data.as_ref().unwrap();
        // Word 0 is at offset 512 and 513
        assert_eq!(written_disk[512], 0);
        assert_eq!(written_disk[513], 0);
        // Word 1 is 0x0001, so offset 514 is 0x00, 515 is 0x01
        assert_eq!(written_disk[514], 0);
        assert_eq!(written_disk[515], 1);
    }

    #[test]
    fn test_software_reset_initializes_signatures() {
        let mut controller = AtaController::new();
        controller.insert_disk(vec![0; 5120]);

        // Manually modify registers to non-default values
        controller.nsector = 0xAA;
        controller.sector = 0x55;
        controller.lcyl = 0x12;
        controller.hcyl = 0x34;

        // Devcon software reset transition 0 -> 4
        controller.write_register(7, true, 0x04);

        // Verify signatures are reset
        assert_eq!(controller.nsector, 1);
        assert_eq!(controller.sector, 1);
        assert_eq!(controller.lcyl, 0);
        assert_eq!(controller.hcyl, 0);
        assert_eq!(controller.error, 0x01);
    }

    #[test]
    fn test_execute_diagnostics_initializes_signatures() {
        let mut controller = AtaController::new();
        controller.insert_disk(vec![0; 5120]);

        // Manually modify registers to non-default values
        controller.nsector = 0xAA;
        controller.sector = 0x55;
        controller.lcyl = 0x12;
        controller.hcyl = 0x34;

        // Command 0x90
        controller.write_command(0x90);

        // Verify signatures are initialized
        assert_eq!(controller.nsector, 1);
        assert_eq!(controller.sector, 1);
        assert_eq!(controller.lcyl, 0);
        assert_eq!(controller.hcyl, 0);
        assert_eq!(controller.error, 0x01);
        assert!(controller.pending_irq);
    }
}
