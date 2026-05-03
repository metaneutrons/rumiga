// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Floppy disk controller with ADF image support.
//!
//! Emulates the Amiga floppy disk subsystem supporting up to four drives
//! (DF0–DF3) with standard 880 KB ADF images.

use alloc::vec::Vec;

/// Total number of tracks (80 cylinders × 2 sides).
pub const TRACKS: u32 = 160;

/// Sectors per track in an ADF image.
pub const SECTORS_PER_TRACK: u32 = 11;

/// Bytes per sector.
pub const SECTOR_SIZE: u32 = 512;

/// Raw bytes per track in an ADF image.
pub const TRACK_SIZE: u32 = SECTORS_PER_TRACK * SECTOR_SIZE;

/// Total size of a standard ADF image in bytes (880 KB).
pub const ADF_SIZE: u32 = TRACKS * TRACK_SIZE;

/// Encoded MFM track size in bytes for DMA transfers.
pub const MFM_TRACK_SIZE: usize = 12668;

/// State of a single floppy drive.
#[derive(Clone, Debug, Default)]
pub struct DriveState {
    /// ADF image data (`None` = no disk inserted).
    pub data: Option<Vec<u8>>,
    /// Current track (0–79).
    pub track: u8,
    /// Current side (0 or 1).
    pub side: u8,
    /// Motor on/off.
    pub motor: bool,
    /// Disk change signal.
    pub disk_changed: bool,
}

/// Floppy disk controller managing up to four drives.
#[derive(Clone, Debug)]
pub struct FloppyController {
    /// Drive states for DF0–DF3.
    pub drives: [DriveState; 4],
    /// Currently selected drive index (0–3).
    pub selected: u8,
    /// DSKLEN register value.
    pub dsklen: u16,
    /// DSKBYTR register value.
    pub dskbytr: u16,
    /// Whether disk DMA is in progress.
    pub dma_active: bool,
    /// Whether DMA transfer is complete (for interrupt).
    pub dma_done: bool,
    /// Tracks whether a previous write to DSKLEN had bit 15 set.
    dsklen_pending: bool,
}

impl FloppyController {
    /// Create a new floppy controller with all drives empty.
    #[must_use]
    pub fn new() -> Self {
        Self {
            drives: core::array::from_fn(|_| DriveState::default()),
            selected: 0,
            dsklen: 0,
            dskbytr: 0,
            dma_active: false,
            dma_done: false,
            dsklen_pending: false,
        }
    }

    /// Insert an ADF image into the specified drive.
    pub fn insert_disk(&mut self, drive: usize, data: Vec<u8>) {
        if let Some(d) = self.drives.get_mut(drive) {
            d.data = Some(data);
            d.disk_changed = true;
        }
    }

    /// Eject the disk from the specified drive.
    pub fn eject_disk(&mut self, drive: usize) {
        if let Some(d) = self.drives.get_mut(drive) {
            d.data = None;
            d.disk_changed = true;
        }
    }

    /// Step the head on the selected drive.
    ///
    /// `direction`: `true` = inward (higher track), `false` = outward (lower track).
    pub fn step(&mut self, direction: bool) {
        let d = &mut self.drives[self.selected as usize];
        if direction {
            if d.track < 79 {
                d.track += 1;
            }
        } else {
            d.track = d.track.saturating_sub(1);
        }
    }

    /// Set the active side for the selected drive.
    pub fn set_side(&mut self, side: u8) {
        self.drives[self.selected as usize].side = side & 1;
    }

    /// Set motor state for the specified drive.
    pub fn set_motor(&mut self, drive: usize, on: bool) {
        if let Some(d) = self.drives.get_mut(drive) {
            d.motor = on;
        }
    }

    /// Write the DSKLEN register.
    ///
    /// The Amiga requires DSKLEN to be written twice with bit 15 set to start
    /// DMA (safety mechanism). Bit 14 selects write mode (unsupported). Bits
    /// 13–0 hold the word count.
    pub fn write_dsklen(&mut self, value: u16) {
        self.dsklen = value;

        let enable = value & 0x8000 != 0;
        if enable {
            if self.dsklen_pending {
                self.dma_active = true;
                self.dsklen_pending = false;
            } else {
                self.dsklen_pending = true;
            }
        } else {
            self.dma_active = false;
            self.dsklen_pending = false;
        }
    }

    /// Read the current track from the selected drive into chip RAM.
    ///
    /// Copies raw ADF sector data (simplified — real hardware uses MFM encoding).
    /// The word count is taken from DSKLEN bits 13–0.
    pub fn read_track_to_ram(&self, chip_ram: &mut [u8], dma_ptr: u32) {
        let d = &self.drives[self.selected as usize];
        let Some(disk) = &d.data else { return };

        let track_index = u32::from(d.track) * 2 + u32::from(d.side);
        let offset = (track_index * TRACK_SIZE) as usize;
        let word_count = usize::from(self.dsklen & 0x3FFF);
        let byte_count = word_count * 2;

        let src_end = (offset + byte_count).min(disk.len());
        let dst_start = dma_ptr as usize;
        let dst_end = (dst_start + byte_count).min(chip_ram.len());
        let copy_len = (src_end - offset).min(dst_end - dst_start);

        chip_ram[dst_start..dst_start + copy_len].copy_from_slice(&disk[offset..offset + copy_len]);
    }

    /// Returns `true` if the DMA transfer is complete.
    #[must_use]
    pub const fn is_dma_done(&self) -> bool {
        self.dma_done
    }

    /// Clear the DMA-done flag.
    pub fn clear_dma_done(&mut self) {
        self.dma_done = false;
    }
}

impl Default for FloppyController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[allow(clippy::cast_possible_truncation)]
    fn make_adf() -> Vec<u8> {
        let mut data = vec![0u8; ADF_SIZE as usize];
        // Fill track 0 side 0 with a recognizable pattern
        for (i, byte) in data.iter_mut().take(TRACK_SIZE as usize).enumerate() {
            *byte = (i & 0xFF) as u8;
        }
        data
    }

    #[test]
    fn insert_and_eject_disk() {
        let mut ctrl = FloppyController::new();
        assert!(ctrl.drives[0].data.is_none());

        ctrl.insert_disk(0, make_adf());
        assert!(ctrl.drives[0].data.is_some());
        assert!(ctrl.drives[0].disk_changed);

        ctrl.eject_disk(0);
        assert!(ctrl.drives[0].data.is_none());
        assert!(ctrl.drives[0].disk_changed);
    }

    #[test]
    fn step_increments_and_clamps() {
        let mut ctrl = FloppyController::new();
        ctrl.insert_disk(0, make_adf());

        // Step inward
        ctrl.step(true);
        assert_eq!(ctrl.drives[0].track, 1);
        ctrl.step(true);
        assert_eq!(ctrl.drives[0].track, 2);

        // Step outward
        ctrl.step(false);
        assert_eq!(ctrl.drives[0].track, 1);

        // Clamp at 0
        ctrl.step(false);
        ctrl.step(false);
        assert_eq!(ctrl.drives[0].track, 0);

        // Clamp at 79
        for _ in 0..100 {
            ctrl.step(true);
        }
        assert_eq!(ctrl.drives[0].track, 79);
    }

    #[test]
    fn dsklen_double_write_starts_dma() {
        let mut ctrl = FloppyController::new();

        // Single write with bit 15 — should not start DMA
        ctrl.write_dsklen(0x8000 | 100);
        assert!(!ctrl.dma_active);

        // Second write with bit 15 — starts DMA
        ctrl.write_dsklen(0x8000 | 100);
        assert!(ctrl.dma_active);
    }

    #[test]
    fn dsklen_without_enable_resets() {
        let mut ctrl = FloppyController::new();

        ctrl.write_dsklen(0x8000 | 50);
        // Write without bit 15 resets pending state
        ctrl.write_dsklen(50);
        assert!(!ctrl.dma_active);

        // Now two writes with bit 15 should be needed again
        ctrl.write_dsklen(0x8000 | 50);
        assert!(!ctrl.dma_active);
        ctrl.write_dsklen(0x8000 | 50);
        assert!(ctrl.dma_active);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn read_track_copies_correct_data() {
        let mut ctrl = FloppyController::new();
        ctrl.insert_disk(0, make_adf());

        // Set up DSKLEN with word count for one sector (256 words = 512 bytes)
        ctrl.dsklen = 0x8000 | 256;

        let mut chip_ram = vec![0u8; 65536];
        let dma_ptr = 0x1000_u32;

        ctrl.read_track_to_ram(&mut chip_ram, dma_ptr);

        // Verify data matches track 0 side 0
        for i in 0..512 {
            assert_eq!(chip_ram[0x1000 + i], (i & 0xFF) as u8);
        }
    }

    #[test]
    fn motor_on_off() {
        let mut ctrl = FloppyController::new();
        assert!(!ctrl.drives[0].motor);

        ctrl.set_motor(0, true);
        assert!(ctrl.drives[0].motor);

        ctrl.set_motor(0, false);
        assert!(!ctrl.drives[0].motor);
    }
}
