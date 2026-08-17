// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Deterministic digests over emulated state.
//!
//! These digests exist to answer one question: did two runs reach the same
//! state? They are not cryptographic and must never be used for integrity or
//! authenticity claims. The algorithm is FNV-1a, chosen because it needs no
//! dependency, works in the portable profile, and is stable across hosts.
//!
//! Stability is the contract. The digest of a given state must not change
//! between builds or platforms, because recorded evidence compares values
//! produced at different times.

/// FNV-1a 64-bit offset basis.
const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a 64-bit prime.
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// Incremental FNV-1a 64-bit digest.
///
/// Field order matters: two states that differ only in the order of otherwise
/// identical values must produce different digests, so callers feed a fixed
/// sequence.
#[derive(Debug, Clone, Copy)]
pub struct StateDigest {
    value: u64,
}

impl Default for StateDigest {
    fn default() -> Self {
        Self::new()
    }
}

impl StateDigest {
    /// Start a new digest.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            value: OFFSET_BASIS,
        }
    }

    /// Absorb raw bytes.
    pub const fn write_bytes(&mut self, bytes: &[u8]) {
        let mut index = 0;
        while index < bytes.len() {
            self.value ^= bytes[index] as u64;
            self.value = self.value.wrapping_mul(PRIME);
            index += 1;
        }
    }

    /// Absorb a 16-bit value in little-endian order.
    pub const fn write_u16(&mut self, value: u16) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Absorb a 32-bit value in little-endian order.
    pub const fn write_u32(&mut self, value: u32) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Absorb a 64-bit value in little-endian order.
    pub const fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Absorb a sequence of 16-bit values.
    pub const fn write_u16_slice(&mut self, values: &[u16]) {
        let mut index = 0;
        while index < values.len() {
            self.write_u16(values[index]);
            index += 1;
        }
    }

    /// Finish and return the digest.
    #[must_use]
    pub const fn finish(self) -> u64 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::StateDigest;

    /// Published FNV-1a 64-bit vectors, so a refactor cannot silently change the
    /// algorithm that recorded evidence depends on.
    #[test]
    fn matches_published_fnv1a_vectors() {
        for (input, expected) in [
            (&b""[..], 0xcbf2_9ce4_8422_2325_u64),
            (&b"a"[..], 0xaf63_dc4c_8601_ec8c),
            (&b"foobar"[..], 0x8594_4171_f739_67e8),
            (&b"hello"[..], 0xa430_d846_80aa_bd0b),
        ] {
            let mut digest = StateDigest::new();
            digest.write_bytes(input);
            assert_eq!(digest.finish(), expected, "input {input:?}");
        }
    }

    #[test]
    fn field_order_changes_the_digest() {
        let mut first = StateDigest::new();
        first.write_u16(1);
        first.write_u32(2);

        let mut second = StateDigest::new();
        second.write_u32(2);
        second.write_u16(1);

        assert_ne!(first.finish(), second.finish());
    }

    #[test]
    fn width_is_part_of_the_digest() {
        let mut narrow = StateDigest::new();
        narrow.write_u16(1);

        let mut wide = StateDigest::new();
        wide.write_u32(1);

        assert_ne!(narrow.finish(), wide.finish());
    }
}
