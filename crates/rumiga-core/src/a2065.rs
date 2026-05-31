// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Commodore A2065-compatible Zorro II Ethernet device shell.

use crate::network::{MacAddress, NetworkCounters};

/// A2065 Zorro II autoconfig window base.
pub const A2065_AUTOCONFIG_BASE: u32 = 0x00E8_0000;
/// A2065 Zorro II autoconfig window end.
pub const A2065_AUTOCONFIG_END: u32 = A2065_AUTOCONFIG_BASE + A2065_BOARD_SIZE;
/// A2065 board aperture size.
pub const A2065_BOARD_SIZE: u32 = 0x0001_0000;

const A2065_CHIP_OFFSET: u32 = 0x4000;
const A2065_RDP: u32 = A2065_CHIP_OFFSET;
const A2065_RAP: u32 = A2065_CHIP_OFFSET + 2;
const RAM_OFFSET: u32 = 0x8000;
const RAM_SIZE: usize = 0x8000;
const RAM_MASK: u32 = 0x7FFF;
const RAP_SIZE: usize = 128;
const RAP_MASK: u16 = 0x0003;

const CSR0_ERR: u16 = 0x8000;
const CSR0_BABL: u16 = 0x4000;
const CSR0_CERR: u16 = 0x2000;
const CSR0_MISS: u16 = 0x1000;
const CSR0_MERR: u16 = 0x0800;
const CSR0_RINT: u16 = 0x0400;
const CSR0_TINT: u16 = 0x0200;
const CSR0_IDON: u16 = 0x0100;
const CSR0_INEA: u16 = 0x0040;
const CSR0_RXON: u16 = 0x0020;
const CSR0_TXON: u16 = 0x0010;
const CSR0_TDMD: u16 = 0x0008;
const CSR0_STOP: u16 = 0x0004;
const CSR0_STRT: u16 = 0x0002;
const CSR0_INIT: u16 = 0x0001;

const BASE_AUTOCONFIG_BYTES: [u8; 12] = [
    0xC1, 0x70, 0x00, 0x00, 0x02, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Runtime status for evidence manifests and API diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct A2065Status {
    /// Whether the card is present in the machine.
    pub enabled: bool,
    /// Whether Kickstart has configured the board aperture.
    pub configured: bool,
    /// Whether the autoconfig chain has been shut up instead of mapped.
    pub shut_up: bool,
    /// Configured board base address, when mapped.
    pub base_address: Option<u32>,
    /// Emulated A2065 station MAC address.
    pub mac_address: MacAddress,
    /// Whether the host backend currently reports link-up.
    pub link_up: bool,
    /// Packet counters for diagnostics.
    pub counters: NetworkCounters,
}

/// A2065-compatible device with Zorro II autoconfig, board RAM, and LANCE CSR shell.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct A2065 {
    enabled: bool,
    configured: bool,
    shut_up: bool,
    base_address: Option<u32>,
    map_hi: u8,
    map_lo: u8,
    mac_address: MacAddress,
    ram: Vec<u8>,
    csr: [u16; RAP_SIZE],
    rap: u16,
    link_up: bool,
    counters: NetworkCounters,
}

impl Default for A2065 {
    fn default() -> Self {
        Self::new_disabled()
    }
}

impl A2065 {
    /// Create an absent A2065 device.
    #[must_use]
    pub fn new_disabled() -> Self {
        let mut device = Self {
            enabled: false,
            configured: false,
            shut_up: false,
            base_address: None,
            map_hi: 0,
            map_lo: 0,
            mac_address: MacAddress::A2065_COMPATIBLE_DEFAULT,
            ram: vec![0; RAM_SIZE],
            csr: [0; RAP_SIZE],
            rap: 0,
            link_up: false,
            counters: NetworkCounters::default(),
        };
        device.reset_chip();
        device
    }

    /// Enable the A2065 card with a validated station MAC address.
    pub fn enable(&mut self, mac_address: MacAddress) {
        self.enabled = true;
        self.configured = false;
        self.shut_up = false;
        self.base_address = None;
        self.map_hi = 0;
        self.map_lo = 0;
        self.mac_address = mac_address;
        self.ram.fill(0);
        self.counters = NetworkCounters::default();
        self.reset_chip();
    }

    /// Disable the card and remove it from the memory map.
    pub fn disable(&mut self) {
        self.enabled = false;
        self.configured = false;
        self.shut_up = false;
        self.base_address = None;
        self.link_up = false;
        self.counters = NetworkCounters::default();
        self.reset_chip();
    }

    /// Return the current diagnostics snapshot.
    #[must_use]
    pub const fn status(&self) -> A2065Status {
        A2065Status {
            enabled: self.enabled,
            configured: self.configured,
            shut_up: self.shut_up,
            base_address: self.base_address,
            mac_address: self.mac_address,
            link_up: self.link_up,
            counters: self.counters,
        }
    }

    /// Return the Amiga-facing autoconfig bytes before nibble encoding.
    #[must_use]
    pub const fn autoconfig_bytes(&self) -> [u8; 12] {
        let mut bytes = BASE_AUTOCONFIG_BYTES;
        let mac = self.mac_address.octets();
        bytes[6] = mac[2];
        bytes[7] = mac[3];
        bytes[8] = mac[4];
        bytes[9] = mac[5];
        bytes
    }

    /// Read a byte from either the active autoconfig window or mapped board aperture.
    #[must_use]
    pub fn read_byte(&self, addr: u32) -> Option<u8> {
        if self.autoconfig_active() && (A2065_AUTOCONFIG_BASE..A2065_AUTOCONFIG_END).contains(&addr)
        {
            return Some(self.read_autoconfig_byte(addr - A2065_AUTOCONFIG_BASE));
        }
        let offset = self.board_offset(addr)?;
        Some(self.read_board_byte(offset))
    }

    /// Read a word from either the active autoconfig window or mapped board aperture.
    #[must_use]
    pub fn read_word(&self, addr: u32) -> Option<u16> {
        if self.autoconfig_active() && (A2065_AUTOCONFIG_BASE..A2065_AUTOCONFIG_END).contains(&addr)
        {
            let hi = self.read_autoconfig_byte(addr - A2065_AUTOCONFIG_BASE);
            let lo = self.read_autoconfig_byte(addr.wrapping_add(1) - A2065_AUTOCONFIG_BASE);
            return Some(u16::from_be_bytes([hi, lo]));
        }

        let offset = self.board_offset(addr)?;
        match offset {
            A2065_RDP => Some(self.read_rdp()),
            A2065_RAP => Some(self.rap),
            _ => {
                let hi = self.read_board_byte(offset);
                let lo = self.read_board_byte(offset.wrapping_add(1));
                Some(u16::from_be_bytes([hi, lo]))
            }
        }
    }

    /// Write a byte to either the active autoconfig window or mapped board aperture.
    pub fn write_byte(&mut self, addr: u32, value: u8) -> bool {
        if self.autoconfig_active() && (A2065_AUTOCONFIG_BASE..A2065_AUTOCONFIG_END).contains(&addr)
        {
            self.write_autoconfig_byte(addr - A2065_AUTOCONFIG_BASE, value);
            return true;
        }
        let Some(offset) = self.board_offset(addr) else {
            return false;
        };
        self.write_board_byte(offset, value);
        true
    }

    /// Write a word to either the active autoconfig window or mapped board aperture.
    pub fn write_word(&mut self, addr: u32, value: u16) -> bool {
        if self.autoconfig_active() && (A2065_AUTOCONFIG_BASE..A2065_AUTOCONFIG_END).contains(&addr)
        {
            self.write_autoconfig_word(addr - A2065_AUTOCONFIG_BASE, value);
            return true;
        }

        let Some(offset) = self.board_offset(addr) else {
            return false;
        };
        match offset {
            A2065_RDP => self.write_rdp(value),
            A2065_RAP => {
                self.rap = value & RAP_MASK;
            }
            _ => {
                let [hi, lo] = value.to_be_bytes();
                self.write_board_byte(offset, hi);
                self.write_board_byte(offset.wrapping_add(1), lo);
            }
        }
        true
    }

    fn reset_chip(&mut self) {
        self.csr.fill(0);
        self.csr[0] = CSR0_STOP;
        self.csr[4] = 0x0115;
        self.rap = 0;
    }

    const fn autoconfig_active(&self) -> bool {
        self.enabled && !self.configured && !self.shut_up
    }

    fn board_offset(&self, addr: u32) -> Option<u32> {
        if !self.enabled || !self.configured {
            return None;
        }
        let base = self.base_address?;
        if (base..base + A2065_BOARD_SIZE).contains(&addr) {
            Some(addr - base)
        } else {
            None
        }
    }

    fn read_autoconfig_byte(&self, offset: u32) -> u8 {
        let logical_index = offset / 4;
        let slot_offset = offset % 4;
        let Ok(index) = usize::try_from(logical_index) else {
            return 0xFF;
        };
        let bytes = self.autoconfig_bytes();
        if index >= bytes.len() {
            return 0xFF;
        }
        match slot_offset {
            0 => autoconfig_high_nibble(offset, bytes[index]),
            2 => autoconfig_low_nibble(offset, bytes[index]),
            _ => 0xFF,
        }
    }

    fn write_autoconfig_byte(&mut self, offset: u32, value: u8) {
        match offset & 0xFF {
            0x48 => {
                self.map_hi = value;
                self.configure_from_map_registers();
            }
            0x4A => {
                self.map_lo = value;
            }
            0x4C => {
                self.shut_up = true;
                self.configured = false;
                self.base_address = None;
            }
            _ => {}
        }
    }

    fn write_autoconfig_word(&mut self, offset: u32, value: u16) {
        match offset & 0xFF {
            0x48 => {
                self.map_hi = (value >> 8) as u8;
                self.map_lo = 0;
                self.configure_from_map_registers();
            }
            0x4C => {
                self.shut_up = true;
                self.configured = false;
                self.base_address = None;
            }
            _ => {}
        }
    }

    fn configure_from_map_registers(&mut self) {
        let base = (u32::from(self.map_hi) | (u32::from(self.map_lo) >> 4)) << 16;
        self.base_address = Some(base);
        self.configured = true;
        self.shut_up = false;
    }

    fn read_board_byte(&self, offset: u32) -> u8 {
        if offset >= RAM_OFFSET {
            self.ram[(offset & RAM_MASK) as usize]
        } else {
            0
        }
    }

    fn write_board_byte(&mut self, offset: u32, value: u8) {
        if offset >= RAM_OFFSET {
            self.ram[(offset & RAM_MASK) as usize] = value;
        }
    }

    fn read_rdp(&self) -> u16 {
        let index = usize::from(self.rap);
        if index >= RAP_SIZE {
            return 0;
        }
        let mut value = self.csr[index];
        if self.rap == 0 && value & (CSR0_BABL | CSR0_CERR | CSR0_MISS | CSR0_MERR) != 0 {
            value |= CSR0_ERR;
        }
        value
    }

    fn write_rdp(&mut self, value: u16) {
        match self.rap {
            0 => self.write_csr0(value),
            1 if self.csr[0] & CSR0_STOP != 0 => {
                self.csr[1] = value & !0x0001;
            }
            2 if self.csr[0] & CSR0_STOP != 0 => {
                self.csr[2] = value & 0x00FF;
            }
            3 if self.csr[0] & CSR0_STOP != 0 => {
                self.csr[3] = value & 0x0007;
            }
            _ => {}
        }
    }

    fn write_csr0(&mut self, value: u16) {
        let previous = self.csr[0];
        self.csr[0] = (self.csr[0] & !CSR0_INEA) | (value & CSR0_INEA);
        self.csr[0] |= value & (CSR0_INIT | CSR0_STRT | CSR0_STOP | CSR0_TDMD);
        self.csr[0] &= !(value
            & (CSR0_IDON | CSR0_TINT | CSR0_RINT | CSR0_MERR | CSR0_MISS | CSR0_CERR | CSR0_BABL));
        self.csr[0] &= !CSR0_ERR;

        if self.csr[0] & CSR0_STOP != 0 && previous & CSR0_STOP == 0 {
            self.csr[0] = CSR0_STOP;
            self.csr[3] = 0;
        } else if self.csr[0] & CSR0_STRT != 0
            && previous & CSR0_STRT == 0
            && previous & (CSR0_STOP | CSR0_INIT) != 0
        {
            self.csr[0] &= !CSR0_STOP;
            self.csr[0] |= CSR0_TXON | CSR0_RXON;
            if self.csr[0] & CSR0_INIT != 0 && previous & CSR0_INIT == 0 {
                self.csr[0] |= CSR0_IDON;
            }
        } else if self.csr[0] & CSR0_INIT != 0
            && previous & CSR0_INIT == 0
            && previous & CSR0_STOP != 0
        {
            self.csr[0] |= CSR0_IDON;
            self.csr[0] &= !(CSR0_RXON | CSR0_TXON | CSR0_STOP);
            self.csr[3] = 0;
        }

        self.csr[0] &= !CSR0_TDMD;
    }
}

const fn autoconfig_high_nibble(offset: u32, value: u8) -> u8 {
    let nibble = value & 0xF0;
    if is_non_inverted_autoconfig_offset(offset) {
        nibble
    } else {
        !nibble
    }
}

const fn autoconfig_low_nibble(offset: u32, value: u8) -> u8 {
    let nibble = (value & 0x0F) << 4;
    if is_non_inverted_autoconfig_offset(offset - 2) {
        nibble
    } else {
        !nibble
    }
}

const fn is_non_inverted_autoconfig_offset(offset: u32) -> bool {
    offset == 0 || offset == 2 || offset == 0x40 || offset == 0x42
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP_BASE: u32 = 0x00EA_0000;

    #[test]
    fn disabled_device_is_absent_from_autoconfig() {
        let device = A2065::new_disabled();

        assert_eq!(device.read_byte(A2065_AUTOCONFIG_BASE), None);
        assert!(!device.status().enabled);
    }

    #[test]
    fn enabled_device_exposes_winuae_style_autoconfig_nibbles() {
        let mut device = A2065::new_disabled();
        device.enable(MacAddress::A2065_COMPATIBLE_DEFAULT);

        assert_eq!(device.read_byte(A2065_AUTOCONFIG_BASE), Some(0xC0));
        assert_eq!(device.read_byte(A2065_AUTOCONFIG_BASE + 2), Some(0x10));
        assert_eq!(device.read_byte(A2065_AUTOCONFIG_BASE + 4), Some(0x8F));
        assert_eq!(device.read_byte(A2065_AUTOCONFIG_BASE + 6), Some(0xFF));
        assert_eq!(
            device.autoconfig_bytes(),
            [
                0xC1, 0x70, 0x00, 0x00, 0x02, 0x02, 0x10, 0x4D, 0x49, 0x47, 0x00, 0x00
            ]
        );
    }

    #[test]
    fn zorro_mapping_hides_autoconfig_and_exposes_board_ram() {
        let mut device = A2065::new_disabled();
        device.enable(MacAddress::A2065_COMPATIBLE_DEFAULT);

        assert!(device.write_byte(A2065_AUTOCONFIG_BASE + 0x48, 0xEA));

        let status = device.status();
        assert!(status.configured);
        assert_eq!(status.base_address, Some(MAP_BASE));
        assert_eq!(device.read_byte(A2065_AUTOCONFIG_BASE), None);

        assert!(device.write_byte(MAP_BASE + RAM_OFFSET + 3, 0xA5));
        assert_eq!(device.read_byte(MAP_BASE + RAM_OFFSET + 3), Some(0xA5));
        assert_eq!(device.read_byte(MAP_BASE + 0x10), Some(0));
    }

    #[test]
    fn word_mapping_uses_high_byte_like_winuae() {
        let mut device = A2065::new_disabled();
        device.enable(MacAddress::A2065_COMPATIBLE_DEFAULT);

        assert!(device.write_word(A2065_AUTOCONFIG_BASE + 0x48, 0xEA00));

        assert_eq!(device.status().base_address, Some(MAP_BASE));
    }

    #[test]
    fn lance_rap_rdp_shell_tracks_stop_init_and_start() {
        let mut device = A2065::new_disabled();
        device.enable(MacAddress::A2065_COMPATIBLE_DEFAULT);
        assert!(device.write_byte(A2065_AUTOCONFIG_BASE + 0x48, 0xEA));

        assert_eq!(device.read_word(MAP_BASE + A2065_RDP), Some(CSR0_STOP));

        assert!(device.write_word(MAP_BASE + A2065_RAP, 1));
        assert!(device.write_word(MAP_BASE + A2065_RDP, 0x1235));
        assert_eq!(device.read_word(MAP_BASE + A2065_RDP), Some(0x1234));

        assert!(device.write_word(MAP_BASE + A2065_RAP, 2));
        assert!(device.write_word(MAP_BASE + A2065_RDP, 0x12FF));
        assert_eq!(device.read_word(MAP_BASE + A2065_RDP), Some(0x00FF));

        assert!(device.write_word(MAP_BASE + A2065_RAP, 0));
        assert!(device.write_word(MAP_BASE + A2065_RDP, CSR0_INIT));
        assert_eq!(
            device.read_word(MAP_BASE + A2065_RDP),
            Some(CSR0_INIT | CSR0_IDON)
        );

        assert!(device.write_word(MAP_BASE + A2065_RDP, CSR0_STRT));
        let csr0 = device.read_word(MAP_BASE + A2065_RDP).expect("CSR0");
        assert_eq!(csr0 & CSR0_STOP, 0);
        assert_eq!(csr0 & (CSR0_RXON | CSR0_TXON), CSR0_RXON | CSR0_TXON);
    }
}
