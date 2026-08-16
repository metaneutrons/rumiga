// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Shared network primitives used by emulated network devices.

use alloc::format;
use alloc::string::String;
use core::fmt;

/// A validated Ethernet MAC address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MacAddress {
    octets: [u8; 6],
}

/// Parsing or validation error for a MAC address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacAddressError {
    /// The address is not exactly six hexadecimal octets separated by colons.
    InvalidFormat,
    /// The address is all zeroes.
    AllZero,
    /// The address is the Ethernet broadcast address.
    Broadcast,
    /// The address is multicast and cannot be used as a station address.
    Multicast,
}

impl fmt::Display for MacAddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => {
                f.write_str("expected six hexadecimal octets separated by colons")
            }
            Self::AllZero => f.write_str("all-zero MAC addresses are not valid"),
            Self::Broadcast => f.write_str("broadcast MAC addresses are not valid"),
            Self::Multicast => f.write_str("multicast MAC addresses are not valid"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for MacAddressError {}

impl MacAddress {
    /// WinUAE-compatible default for A2065 drivers, which expect Commodore's OUI.
    pub const A2065_COMPATIBLE_DEFAULT: Self = Self {
        octets: [0x00, 0x80, 0x10, 0x4D, 0x49, 0x47],
    };

    /// Build a MAC address from octets after validating it can be used as a station address.
    ///
    /// # Errors
    /// Returns an error when the address is all zeroes, broadcast, or multicast.
    pub fn from_unicast_octets(octets: [u8; 6]) -> Result<Self, MacAddressError> {
        let address = Self { octets };
        address.validate_unicast()?;
        Ok(address)
    }

    /// Parse a colon-separated unicast MAC address.
    ///
    /// # Errors
    /// Returns an error when the format is malformed or the address is not a unicast station
    /// address.
    pub fn from_unicast_str(value: &str) -> Result<Self, MacAddressError> {
        let bytes = value.as_bytes();
        if bytes.len() != 17 {
            return Err(MacAddressError::InvalidFormat);
        }

        let mut octets = [0; 6];
        for (index, octet) in octets.iter_mut().enumerate() {
            let offset = index * 3;
            let Some(high) = hex_nibble(bytes[offset]) else {
                return Err(MacAddressError::InvalidFormat);
            };
            let Some(low) = hex_nibble(bytes[offset + 1]) else {
                return Err(MacAddressError::InvalidFormat);
            };
            *octet = (high << 4) | low;
            if index < 5 && bytes[offset + 2] != b':' {
                return Err(MacAddressError::InvalidFormat);
            }
        }

        Self::from_unicast_octets(octets)
    }

    /// Return the raw MAC address octets.
    #[must_use]
    pub const fn octets(self) -> [u8; 6] {
        self.octets
    }

    /// Format the address as lower-case colon-separated hexadecimal.
    #[must_use]
    pub fn to_colon_string(self) -> String {
        let octets = self.octets;
        format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            octets[0], octets[1], octets[2], octets[3], octets[4], octets[5]
        )
    }

    fn validate_unicast(self) -> Result<(), MacAddressError> {
        if self.octets.iter().all(|&octet| octet == 0) {
            return Err(MacAddressError::AllZero);
        }
        if self.octets.iter().all(|&octet| octet == 0xFF) {
            return Err(MacAddressError::Broadcast);
        }
        if self.octets[0] & 0x01 != 0 {
            return Err(MacAddressError::Multicast);
        }
        Ok(())
    }
}

/// Packet counters exposed to diagnostics and evidence manifests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetworkCounters {
    /// Packets accepted from the guest transmit ring.
    pub tx_packets: u64,
    /// Packets delivered into the guest receive ring.
    pub rx_packets: u64,
    /// Packets dropped because the emulated device or backend could not accept them.
    pub dropped_packets: u64,
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unicast_mac_address() {
        let mac = MacAddress::from_unicast_str("00:80:10:4d:49:47").expect("valid MAC");

        assert_eq!(mac.octets(), [0x00, 0x80, 0x10, 0x4D, 0x49, 0x47]);
        assert_eq!(mac.to_colon_string(), "00:80:10:4d:49:47");
    }

    #[test]
    fn rejects_non_station_addresses() {
        assert_eq!(
            MacAddress::from_unicast_str("00:00:00:00:00:00"),
            Err(MacAddressError::AllZero)
        );
        assert_eq!(
            MacAddress::from_unicast_str("ff:ff:ff:ff:ff:ff"),
            Err(MacAddressError::Broadcast)
        );
        assert_eq!(
            MacAddress::from_unicast_str("01:80:10:4d:49:47"),
            Err(MacAddressError::Multicast)
        );
        assert_eq!(
            MacAddress::from_unicast_str("00-80-10-4d-49-47"),
            Err(MacAddressError::InvalidFormat)
        );
    }
}
