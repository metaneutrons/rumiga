// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Commodore A2065-compatible Zorro II Ethernet device shell.

#![allow(
    clippy::cast_possible_truncation,
    clippy::similar_names,
    clippy::too_many_lines
)]

use std::collections::VecDeque;

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
const CSR0_INTR: u16 = 0x0080;
const CSR0_INEA: u16 = 0x0040;
const CSR0_RXON: u16 = 0x0020;
const CSR0_TXON: u16 = 0x0010;
const CSR0_TDMD: u16 = 0x0008;
const CSR0_STOP: u16 = 0x0004;
const CSR0_STRT: u16 = 0x0002;
const CSR0_INIT: u16 = 0x0001;

const MODE_PROM: u16 = 0x8000;
const MODE_DTCR: u16 = 0x0008;
const MODE_DTX: u16 = 0x0002;
const MODE_DRX: u16 = 0x0001;

const TX_OWN: u16 = 0x8000;
const TX_ERR: u16 = 0x4000;
const TX_MORE: u16 = 0x1000;
const TX_ONE: u16 = 0x0800;
const TX_DEF: u16 = 0x0400;
const TX_STP: u16 = 0x0200;
const TX_ENP: u16 = 0x0100;
const TX_UFLO: u16 = 0x4000;

const RX_OWN: u16 = 0x8000;
const RX_ERR: u16 = 0x4000;
const RX_OFLO: u16 = 0x1000;
const RX_BUFF: u16 = 0x0400;
const RX_STP: u16 = 0x0200;
const RX_ENP: u16 = 0x0100;

const MAX_PACKET_SIZE: usize = 4_000;
const ETHERNET_MIN_FRAME_SIZE: usize = 60;
const ETHERNET_FCS_SIZE: usize = 4;

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
    initialized: bool,
    mode: u16,
    logical_address_filter: u64,
    receive_ring_addr: u32,
    receive_ring_len: usize,
    transmit_ring_addr: u32,
    transmit_ring_len: usize,
    receive_ring_offset: usize,
    transmit_ring_offset: usize,
    loopback_enabled: bool,
    transmitted_frames: VecDeque<Vec<u8>>,
    pending_receive_frames: VecDeque<Vec<u8>>,
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
            initialized: false,
            mode: 0,
            logical_address_filter: 0,
            receive_ring_addr: 0,
            receive_ring_len: 0,
            transmit_ring_addr: 0,
            transmit_ring_len: 0,
            receive_ring_offset: 0,
            transmit_ring_offset: 0,
            loopback_enabled: false,
            transmitted_frames: VecDeque::new(),
            pending_receive_frames: VecDeque::new(),
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
        self.transmitted_frames.clear();
        self.pending_receive_frames.clear();
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
        self.transmitted_frames.clear();
        self.pending_receive_frames.clear();
        self.reset_chip();
    }

    /// Enable or disable an in-core loopback backend for deterministic tests.
    pub fn set_loopback_enabled(&mut self, enabled: bool) {
        self.loopback_enabled = enabled;
        self.link_up = enabled;
    }

    /// Queue a host-supplied Ethernet frame for guest receive-ring delivery.
    pub fn queue_receive_frame(&mut self, frame: Vec<u8>) {
        self.pending_receive_frames.push_back(frame);
        self.service_receive();
    }

    /// Take the next Ethernet frame transmitted by the guest.
    pub fn take_transmitted_frame(&mut self) -> Option<Vec<u8>> {
        self.transmitted_frames.pop_front()
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
        self.initialized = false;
        self.mode = 0;
        self.logical_address_filter = 0;
        self.receive_ring_addr = 0;
        self.receive_ring_len = 0;
        self.transmit_ring_addr = 0;
        self.transmit_ring_len = 0;
        self.receive_ring_offset = 0;
        self.transmit_ring_offset = 0;
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
        if self.rap == 0 {
            if value & (CSR0_BABL | CSR0_CERR | CSR0_MISS | CSR0_MERR) != 0 {
                value |= CSR0_ERR;
            }
            if value & CSR0_INEA != 0 && value & interrupt_event_mask() != 0 {
                value |= CSR0_INTR;
            }
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
            if self.mode & MODE_DTX == 0 {
                self.csr[0] |= CSR0_TXON;
            }
            if self.mode & MODE_DRX == 0 {
                self.csr[0] |= CSR0_RXON;
            }
            if self.csr[0] & CSR0_INIT != 0 && previous & CSR0_INIT == 0 {
                self.initialize_chip();
                self.csr[0] |= CSR0_IDON;
            }
        } else if self.csr[0] & CSR0_INIT != 0
            && previous & CSR0_INIT == 0
            && previous & CSR0_STOP != 0
        {
            self.initialize_chip();
            self.csr[0] |= CSR0_IDON;
            self.csr[0] &= !(CSR0_RXON | CSR0_TXON | CSR0_STOP);
            self.csr[3] = 0;
        }

        if self.initialized && self.csr[0] & CSR0_STRT != 0 {
            self.service_transmit(self.csr[0] & CSR0_TDMD != 0);
            self.service_receive();
        }
        self.csr[0] &= !CSR0_TDMD;
        self.refresh_interrupt_summary();
    }

    fn initialize_chip(&mut self) {
        let init_addr = ((u32::from(self.csr[2] & 0x00FF)) << 16) | u32::from(self.csr[1]);
        let offset = init_addr & RAM_MASK;
        self.mode = self.read_ram_word(offset);
        self.logical_address_filter = (u64::from(self.read_ram_word(offset + 14)) << 48)
            | (u64::from(self.read_ram_word(offset + 12)) << 32)
            | (u64::from(self.read_ram_word(offset + 10)) << 16)
            | u64::from(self.read_ram_word(offset + 8));

        let receive_descriptor = (u32::from(self.read_ram_word(offset + 18)) << 16)
            | u32::from(self.read_ram_word(offset + 16));
        let transmit_descriptor = (u32::from(self.read_ram_word(offset + 22)) << 16)
            | u32::from(self.read_ram_word(offset + 20));

        self.receive_ring_len = 1 << ((receive_descriptor >> 29) & 0x07);
        self.transmit_ring_len = 1 << ((transmit_descriptor >> 29) & 0x07);
        self.receive_ring_addr = receive_descriptor & 0x00FF_FFF8 & RAM_MASK;
        self.transmit_ring_addr = transmit_descriptor & 0x00FF_FFF8 & RAM_MASK;
        self.receive_ring_offset = 0;
        self.transmit_ring_offset = 0;
        self.initialized = true;
    }

    fn service_transmit(&mut self, transmit_demand: bool) {
        if !transmit_demand || self.transmit_ring_len == 0 || self.csr[0] & CSR0_TXON == 0 {
            return;
        }

        for _ in 0..self.transmit_ring_len {
            let descriptor_addr =
                self.transmit_ring_addr + u32::try_from(self.transmit_ring_offset * 8).unwrap_or(0);
            let desc = Descriptor::read_from(self, descriptor_addr);
            if desc.flags & TX_OWN == 0 {
                return;
            }
            if desc.flags & TX_STP == 0 {
                self.advance_transmit_ring();
                continue;
            }

            let Some((frame, last_descriptor_addr, mut last_desc)) = self.read_transmit_frame()
            else {
                return;
            };

            if frame.len() < ETHERNET_MIN_FRAME_SIZE {
                last_desc.flags |= TX_ERR;
                last_desc.status |= TX_UFLO;
                last_desc.flags &= !TX_OWN;
                last_desc.write_to(self, last_descriptor_addr);
                self.csr[0] &= !CSR0_TXON;
                self.csr[0] |= CSR0_TINT;
                self.counters.dropped_packets = self.counters.dropped_packets.saturating_add(1);
                self.refresh_interrupt_summary();
                return;
            }

            self.transmitted_frames.push_back(frame.clone());
            self.counters.tx_packets = self.counters.tx_packets.saturating_add(1);
            if self.loopback_enabled {
                self.pending_receive_frames.push_back(frame);
            }
            self.csr[0] |= CSR0_TINT;
            last_desc.flags &= !TX_OWN;
            last_desc.status &= !(TX_UFLO | TX_DEF | TX_MORE | TX_ONE);
            last_desc.write_to(self, last_descriptor_addr);
            self.service_receive();
            self.refresh_interrupt_summary();
            return;
        }
    }

    fn read_transmit_frame(&mut self) -> Option<(Vec<u8>, u32, Descriptor)> {
        let mut frame = Vec::new();
        for _ in 0..self.transmit_ring_len {
            let descriptor_addr =
                self.transmit_ring_addr + u32::try_from(self.transmit_ring_offset * 8).ok()?;
            let mut desc = Descriptor::read_from(self, descriptor_addr);
            if desc.flags & TX_OWN == 0 {
                desc.flags |= TX_ERR;
                desc.status |= TX_UFLO;
                desc.write_to(self, descriptor_addr);
                self.csr[0] &= !CSR0_TXON;
                self.csr[0] |= CSR0_TINT;
                self.refresh_interrupt_summary();
                return None;
            }

            let size = usize::from(0u16.wrapping_sub(desc.length));
            let buffer_addr = desc.buffer_addr();
            for index in 0..size.min(MAX_PACKET_SIZE.saturating_sub(frame.len())) {
                frame.push(self.read_ram_byte(buffer_addr + u32::try_from(index).ok()?));
            }
            desc.flags &= !TX_OWN;
            desc.write_to(self, descriptor_addr);
            self.advance_transmit_ring();

            if desc.flags & TX_ENP != 0 {
                return Some((frame, descriptor_addr, desc));
            }
        }

        self.csr[0] |= CSR0_TINT;
        self.counters.dropped_packets = self.counters.dropped_packets.saturating_add(1);
        self.refresh_interrupt_summary();
        None
    }

    fn service_receive(&mut self) {
        if !self.initialized
            || self.receive_ring_len == 0
            || self.csr[0] & CSR0_RXON == 0
            || self.pending_receive_frames.is_empty()
        {
            return;
        }

        while let Some(frame) = self.pending_receive_frames.pop_front() {
            if !self.accepts_receive_frame(&frame) {
                self.counters.dropped_packets = self.counters.dropped_packets.saturating_add(1);
                continue;
            }
            if self.write_receive_frame(&frame) {
                self.csr[0] |= CSR0_RINT;
                self.counters.rx_packets = self.counters.rx_packets.saturating_add(1);
                self.refresh_interrupt_summary();
            } else {
                self.counters.dropped_packets = self.counters.dropped_packets.saturating_add(1);
                self.refresh_interrupt_summary();
                break;
            }
        }
    }

    fn accepts_receive_frame(&self, frame: &[u8]) -> bool {
        if frame.len() < 14 {
            return false;
        }
        if self.mode & MODE_PROM != 0 || self.loopback_enabled {
            return true;
        }
        let mac = self.mac_address.octets();
        let destination = &frame[0..6];
        destination == mac
            || destination == [0xFF; 6]
            || (destination[0] & 0x01 != 0 && self.logical_address_filter != 0)
    }

    fn write_receive_frame(&mut self, frame: &[u8]) -> bool {
        let mut data = frame.to_vec();
        if self.mode & MODE_DTCR == 0 {
            data.extend_from_slice(&ethernet_crc32(frame));
        }
        let mut written = 0;
        let mut first = true;

        for _ in 0..self.receive_ring_len {
            let descriptor_addr =
                self.receive_ring_addr + u32::try_from(self.receive_ring_offset * 8).unwrap_or(0);
            let mut desc = Descriptor::read_from(self, descriptor_addr);
            if desc.flags & RX_OWN == 0 {
                if first {
                    self.csr[0] |= CSR0_MISS;
                } else {
                    desc.flags |= RX_ERR | RX_BUFF;
                    desc.status |= RX_OFLO;
                    self.csr[0] &= !CSR0_RXON;
                    desc.write_to(self, descriptor_addr);
                }
                return false;
            }

            desc.flags &= !RX_OWN;
            if first {
                desc.flags |= RX_STP;
                first = false;
            }

            let buffer_size = usize::from(0u16.wrapping_sub(desc.length));
            let remaining = data.len().saturating_sub(written);
            let copy_len = buffer_size.min(remaining);
            for (index, byte) in data[written..written + copy_len].iter().enumerate() {
                self.write_ram_byte(
                    desc.buffer_addr() + u32::try_from(index).unwrap_or(0),
                    *byte,
                );
            }
            written += copy_len;

            if written >= data.len() {
                desc.flags |= RX_ENP;
                desc.status = u16::try_from(data.len()).unwrap_or(u16::MAX);
            }
            desc.write_to(self, descriptor_addr);
            self.advance_receive_ring();

            if written >= data.len() {
                return true;
            }
        }

        self.csr[0] |= CSR0_MISS;
        false
    }

    fn advance_transmit_ring(&mut self) {
        self.transmit_ring_offset = (self.transmit_ring_offset + 1) % self.transmit_ring_len.max(1);
    }

    fn advance_receive_ring(&mut self) {
        self.receive_ring_offset = (self.receive_ring_offset + 1) % self.receive_ring_len.max(1);
    }

    fn refresh_interrupt_summary(&mut self) {
        self.csr[0] &= !CSR0_INTR;
        if self.csr[0] & CSR0_INEA != 0 && self.csr[0] & interrupt_event_mask() != 0 {
            self.csr[0] |= CSR0_INTR;
        }
    }

    fn read_ram_byte(&self, offset: u32) -> u8 {
        self.ram[(offset & RAM_MASK) as usize]
    }

    fn read_ram_word(&self, offset: u32) -> u16 {
        u16::from_be_bytes([self.read_ram_byte(offset), self.read_ram_byte(offset + 1)])
    }

    fn write_ram_byte(&mut self, offset: u32, value: u8) {
        self.ram[(offset & RAM_MASK) as usize] = value;
    }

    fn write_ram_word(&mut self, offset: u32, value: u16) {
        let [hi, lo] = value.to_be_bytes();
        self.write_ram_byte(offset, hi);
        self.write_ram_byte(offset + 1, lo);
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Descriptor {
    address_low: u16,
    flags: u16,
    length: u16,
    status: u16,
}

impl Descriptor {
    fn read_from(device: &A2065, offset: u32) -> Self {
        Self {
            address_low: device.read_ram_word(offset),
            flags: device.read_ram_word(offset + 2),
            length: device.read_ram_word(offset + 4),
            status: device.read_ram_word(offset + 6),
        }
    }

    fn write_to(self, device: &mut A2065, offset: u32) {
        device.write_ram_word(offset, self.address_low);
        device.write_ram_word(offset + 2, self.flags);
        device.write_ram_word(offset + 4, self.length);
        device.write_ram_word(offset + 6, self.status);
    }

    const fn buffer_addr(self) -> u32 {
        (self.address_low as u32 | (((self.flags & 0x00FF) as u32) << 16)) & RAM_MASK
    }
}

const fn interrupt_event_mask() -> u16 {
    CSR0_BABL | CSR0_CERR | CSR0_MISS | CSR0_MERR | CSR0_RINT | CSR0_TINT | CSR0_IDON
}

fn ethernet_crc32(frame: &[u8]) -> [u8; ETHERNET_FCS_SIZE] {
    let mut crc = 0xFFFF_FFFF_u32;
    for byte in frame {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    (!crc).to_be_bytes()
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
    const INIT_BLOCK: u32 = RAM_OFFSET;
    const RX_RING: u32 = RAM_OFFSET + 0x100;
    const TX_RING: u32 = RAM_OFFSET + 0x180;
    const RX_BUFFER: u32 = RAM_OFFSET + 0x300;
    const TX_BUFFER: u32 = RAM_OFFSET + 0x500;

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

    #[test]
    fn transmit_descriptor_ring_emits_frames_and_interrupts() {
        let mut device = configured_device();
        seed_init_block(&mut device);
        let frame = ethernet_frame(0x33, ETHERNET_MIN_FRAME_SIZE);
        seed_transmit_descriptor(&mut device, &frame);

        start_device(&mut device);
        assert!(device.write_word(MAP_BASE + A2065_RDP, CSR0_TDMD | CSR0_INEA));

        let transmitted = device.take_transmitted_frame().expect("transmitted frame");
        assert_eq!(transmitted, frame);
        assert_eq!(device.status().counters.tx_packets, 1);
        assert_eq!(
            device.read_word(MAP_BASE + A2065_RDP).expect("CSR0")
                & (CSR0_TINT | CSR0_INTR | CSR0_INEA),
            CSR0_TINT | CSR0_INTR | CSR0_INEA
        );
        assert_eq!(
            device.read_word(MAP_BASE + TX_RING + 2),
            Some(TX_STP | TX_ENP)
        );

        assert!(device.write_word(MAP_BASE + A2065_RDP, CSR0_TINT | CSR0_INEA));
        assert_eq!(
            device.read_word(MAP_BASE + A2065_RDP).expect("CSR0") & CSR0_TINT,
            0
        );
    }

    #[test]
    fn receive_descriptor_ring_accepts_queued_packets() {
        let mut device = configured_device();
        seed_init_block(&mut device);
        seed_receive_descriptor(&mut device);
        start_device(&mut device);

        let frame = ethernet_frame(0x44, ETHERNET_MIN_FRAME_SIZE);
        device.queue_receive_frame(frame.clone());

        let packet_len = frame.len() + ETHERNET_FCS_SIZE;
        assert_eq!(device.status().counters.rx_packets, 1);
        assert_eq!(
            device.read_word(MAP_BASE + A2065_RDP).expect("CSR0") & CSR0_RINT,
            CSR0_RINT
        );
        assert_eq!(
            device.read_word(MAP_BASE + RX_RING + 2),
            Some(RX_STP | RX_ENP)
        );
        assert_eq!(
            device.read_word(MAP_BASE + RX_RING + 6),
            Some(u16::try_from(packet_len).expect("packet len fits"))
        );
        for (index, byte) in frame.iter().enumerate() {
            assert_eq!(
                device.read_byte(MAP_BASE + RX_BUFFER + u32::try_from(index).expect("index")),
                Some(*byte)
            );
        }
    }

    #[test]
    fn loopback_backend_delivers_transmit_to_receive_ring() {
        let mut device = configured_device();
        device.set_loopback_enabled(true);
        seed_init_block(&mut device);
        seed_receive_descriptor(&mut device);
        let frame = ethernet_frame(0x55, ETHERNET_MIN_FRAME_SIZE);
        seed_transmit_descriptor(&mut device, &frame);
        start_device(&mut device);

        assert!(device.write_word(MAP_BASE + A2065_RDP, CSR0_TDMD | CSR0_INEA));

        assert_eq!(device.status().counters.tx_packets, 1);
        assert_eq!(device.status().counters.rx_packets, 1);
        assert_eq!(
            device.read_word(MAP_BASE + A2065_RDP).expect("CSR0")
                & (CSR0_RINT | CSR0_TINT | CSR0_INTR),
            CSR0_RINT | CSR0_TINT | CSR0_INTR
        );
    }

    #[test]
    fn receive_without_owned_descriptor_sets_miss_interrupt() {
        let mut device = configured_device();
        seed_init_block(&mut device);
        start_device(&mut device);

        device.queue_receive_frame(ethernet_frame(0x66, ETHERNET_MIN_FRAME_SIZE));

        assert_eq!(device.status().counters.dropped_packets, 1);
        assert_eq!(
            device.read_word(MAP_BASE + A2065_RDP).expect("CSR0") & CSR0_MISS,
            CSR0_MISS
        );
    }

    fn configured_device() -> A2065 {
        let mut device = A2065::new_disabled();
        device.enable(MacAddress::A2065_COMPATIBLE_DEFAULT);
        assert!(device.write_byte(A2065_AUTOCONFIG_BASE + 0x48, 0xEA));
        device
    }

    fn seed_init_block(device: &mut A2065) {
        device.write_word(MAP_BASE + INIT_BLOCK, 0);
        let mac = MacAddress::A2065_COMPATIBLE_DEFAULT.octets();
        device.write_byte(MAP_BASE + INIT_BLOCK + 2, mac[1]);
        device.write_byte(MAP_BASE + INIT_BLOCK + 3, mac[0]);
        device.write_byte(MAP_BASE + INIT_BLOCK + 4, mac[3]);
        device.write_byte(MAP_BASE + INIT_BLOCK + 5, mac[2]);
        device.write_byte(MAP_BASE + INIT_BLOCK + 6, mac[5]);
        device.write_byte(MAP_BASE + INIT_BLOCK + 7, mac[4]);
        for offset in 8..16 {
            device.write_byte(MAP_BASE + INIT_BLOCK + offset, 0);
        }
        write_pointer(device, INIT_BLOCK + 16, RX_RING);
        write_pointer(device, INIT_BLOCK + 20, TX_RING);
        assert!(device.write_word(MAP_BASE + A2065_RAP, 1));
        assert!(device.write_word(MAP_BASE + A2065_RDP, INIT_BLOCK as u16));
        assert!(device.write_word(MAP_BASE + A2065_RAP, 2));
        assert!(device.write_word(MAP_BASE + A2065_RDP, 0));
        assert!(device.write_word(MAP_BASE + A2065_RAP, 0));
    }

    fn seed_transmit_descriptor(device: &mut A2065, frame: &[u8]) {
        for (index, byte) in frame.iter().enumerate() {
            device.write_byte(
                MAP_BASE + TX_BUFFER + u32::try_from(index).expect("index"),
                *byte,
            );
        }
        write_descriptor(
            device,
            TX_RING,
            TX_BUFFER,
            TX_OWN | TX_STP | TX_ENP,
            0u16.wrapping_sub(u16::try_from(frame.len()).expect("frame len")),
            0,
        );
    }

    fn seed_receive_descriptor(device: &mut A2065) {
        write_descriptor(
            device,
            RX_RING,
            RX_BUFFER,
            RX_OWN,
            0u16.wrapping_sub(256),
            0,
        );
    }

    fn start_device(device: &mut A2065) {
        assert!(device.write_word(MAP_BASE + A2065_RAP, 0));
        assert!(device.write_word(MAP_BASE + A2065_RDP, CSR0_INIT | CSR0_INEA));
        assert!(device.write_word(MAP_BASE + A2065_RDP, CSR0_STRT | CSR0_INEA));
    }

    fn write_pointer(device: &mut A2065, offset: u32, pointer: u32) {
        let ring_len_bits = 0_u32;
        device.write_word(
            MAP_BASE + offset,
            u16::try_from(pointer & 0xFFFF).expect("low word"),
        );
        device.write_word(
            MAP_BASE + offset + 2,
            u16::try_from(((pointer & 0x00FF_FFFF) >> 16) | (ring_len_bits << 13))
                .expect("high word"),
        );
    }

    fn write_descriptor(
        device: &mut A2065,
        offset: u32,
        buffer: u32,
        flags: u16,
        length: u16,
        status: u16,
    ) {
        device.write_word(
            MAP_BASE + offset,
            u16::try_from(buffer & 0xFFFF).expect("buffer low"),
        );
        device.write_word(
            MAP_BASE + offset + 2,
            flags | u16::try_from((buffer >> 16) & 0x00FF).expect("buffer high"),
        );
        device.write_word(MAP_BASE + offset + 4, length);
        device.write_word(MAP_BASE + offset + 6, status);
    }

    fn ethernet_frame(seed: u8, len: usize) -> Vec<u8> {
        let mut frame = vec![0; len];
        let mac = MacAddress::A2065_COMPATIBLE_DEFAULT.octets();
        frame[0..6].copy_from_slice(&mac);
        frame[6..12].copy_from_slice(&[0x02, 0x52, 0x55, 0x4D, 0x49, seed]);
        frame[12] = 0x08;
        frame[13] = 0x00;
        for (index, byte) in frame[14..].iter_mut().enumerate() {
            *byte = seed.wrapping_add(u8::try_from(index).unwrap_or(0));
        }
        frame
    }
}
