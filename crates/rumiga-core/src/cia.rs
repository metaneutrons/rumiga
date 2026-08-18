// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! CIA 8520 emulation.
//!
//! The Amiga contains two CIA 8520 chips providing timers, I/O ports,
//! a time-of-day counter, and serial shift registers.

use crate::digest::StateDigest;

/// Register index: Peripheral Data Register A.
const REG_PRA: u8 = 0x0;
/// Register index: Peripheral Data Register B.
const REG_PRB: u8 = 0x1;
/// Register index: Data Direction Register A.
const REG_DDRA: u8 = 0x2;
/// Register index: Data Direction Register B.
const REG_DDRB: u8 = 0x3;
/// Register index: Timer A low byte.
const REG_TALO: u8 = 0x4;
/// Register index: Timer A high byte.
const REG_TAHI: u8 = 0x5;
/// Register index: Timer B low byte.
const REG_TBLO: u8 = 0x6;
/// Register index: Timer B high byte.
const REG_TBHI: u8 = 0x7;
/// Register index: TOD low byte.
const REG_TOD_LO: u8 = 0x8;
/// Register index: TOD mid byte.
const REG_TOD_MID: u8 = 0x9;
/// Register index: TOD high byte.
const REG_TOD_HI: u8 = 0xA;
/// Register index: Serial Data Register.
const REG_SDR: u8 = 0xC;
/// Register index: Interrupt Control Register.
const REG_ICR: u8 = 0xD;
/// Register index: Control Register A.
const REG_CRA: u8 = 0xE;
/// Register index: Control Register B.
const REG_CRB: u8 = 0xF;

/// CRA/CRB bit: timer start.
const CR_START: u8 = 1 << 0;
/// CRA/CRB bit: one-shot mode.
const CR_ONESHOT: u8 = 1 << 3;
/// CRA/CRB bit: force load.
const CR_LOAD: u8 = 1 << 4;

/// ICR bit: Timer A underflow.
const ICR_TA: u8 = 1 << 0;
/// ICR bit: Timer B underflow.
const ICR_TB: u8 = 1 << 1;
/// ICR bit: set/clear control.
const ICR_SET: u8 = 1 << 7;

/// Observable timer-control statistics for boot and scheduler evidence.
#[derive(Clone, Copy, Debug, Default)]
pub struct CiaTimerStats {
    /// Number of control writes that transitioned the timer from stopped to running.
    pub start_writes: u64,
    /// Number of control writes that transitioned the timer from running to stopped.
    pub stop_writes: u64,
    /// Number of control writes that used the force-load bit.
    pub force_load_writes: u64,
    /// Number of high-byte latch writes that auto-started a one-shot timer.
    pub auto_start_writes: u64,
    /// Number of timer underflows observed by the emulator.
    pub underflows: u64,
}

impl CiaTimerStats {
    const fn new() -> Self {
        Self {
            start_writes: 0,
            stop_writes: 0,
            force_load_writes: 0,
            auto_start_writes: 0,
            underflows: 0,
        }
    }
}

/// State of a single CIA 8520 chip.
#[derive(Clone, Debug)]
pub struct CiaState {
    /// Peripheral Data Register A.
    pub pra: u8,
    /// Peripheral Data Register B.
    pub prb: u8,
    /// Data Direction Register A.
    pub ddra: u8,
    /// Data Direction Register B.
    pub ddrb: u8,
    /// Timer A counter.
    pub timer_a: u16,
    /// Timer A latch (reload value).
    pub timer_a_latch: u16,
    /// Timer A control/write evidence counters.
    pub timer_a_stats: CiaTimerStats,
    /// Timer B counter.
    pub timer_b: u16,
    /// Timer B latch (reload value).
    pub timer_b_latch: u16,
    /// Timer B control/write evidence counters.
    pub timer_b_stats: CiaTimerStats,
    /// Control Register A.
    pub cra: u8,
    /// Control Register B.
    pub crb: u8,
    /// Interrupt control data (pending flags).
    pub icr_data: u8,
    /// Interrupt control mask (enabled interrupts).
    pub icr_mask: u8,
    /// Whether the interrupt line is currently asserted (IR flag, bit 7).
    pub icr_ir: bool,
    /// Time-of-day counter (low, mid, high).
    pub tod: [u8; 3],
    /// TOD alarm value.
    pub tod_alarm: [u8; 3],
    /// Whether TOD output is latched.
    pub tod_latched: bool,
    /// Latched TOD value for reading.
    pub tod_latch: [u8; 3],
    /// Serial Data Register.
    pub sdr: u8,
    /// Number of guest writes observed per CIA register index.
    pub register_write_counts: [u64; 16],
    /// Last guest-written value per CIA register index.
    pub last_register_writes: [u8; 16],
}

impl CiaState {
    /// Fold this CIA's guest-visible state into `digest`.
    ///
    /// The CIA carries keyboard serial data and the timers, so it is machine state.
    /// It sat outside the state digest until M1-009, which meant a keystroke that had
    /// already reached `sdr` was invisible to a state comparison.
    ///
    /// The write-evidence counters are included because they are part of what a
    /// recorded run reports, so a replay that produced them differently should not
    /// compare equal.
    pub fn write_digest(&self, digest: &mut StateDigest) {
        digest.write_u16(u16::from(self.pra));
        digest.write_u16(u16::from(self.prb));
        digest.write_u16(u16::from(self.ddra));
        digest.write_u16(u16::from(self.ddrb));
        digest.write_u16(self.timer_a);
        digest.write_u16(self.timer_a_latch);
        digest.write_u16(self.timer_b);
        digest.write_u16(self.timer_b_latch);
        digest.write_u16(u16::from(self.cra));
        digest.write_u16(u16::from(self.crb));
        digest.write_u16(u16::from(self.icr_data));
        digest.write_u16(u16::from(self.icr_mask));
        digest.write_u16(u16::from(self.icr_ir));
        digest.write_u16(u16::from(self.sdr));
        digest.write_bytes(&self.tod);
        digest.write_bytes(&self.tod_alarm);
        digest.write_u16(u16::from(self.tod_latched));
        digest.write_bytes(&self.tod_latch);
        for count in self.register_write_counts {
            digest.write_u64(count);
        }
        digest.write_bytes(&self.last_register_writes);
    }
}

impl CiaState {
    /// Create a new CIA in its reset state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pra: 0,
            prb: 0xFF, // All outputs high after reset (active-low signals deasserted)
            ddra: 0,
            ddrb: 0,
            timer_a: 0xFFFF,
            timer_a_latch: 0xFFFF,
            timer_a_stats: CiaTimerStats::new(),
            timer_b: 0xFFFF,
            timer_b_latch: 0xFFFF,
            timer_b_stats: CiaTimerStats::new(),
            cra: 0,
            crb: 0,
            icr_data: 0,
            icr_mask: 0,
            icr_ir: false,
            tod: [0; 3],
            tod_alarm: [0; 3],
            tod_latched: false,
            tod_latch: [0; 3],
            sdr: 0,
            register_write_counts: [0; 16],
            last_register_writes: [0; 16],
        }
    }

    /// Read a CIA register by index (0x0–0xF).
    #[must_use]
    pub fn read(&mut self, reg: u8) -> u8 {
        match reg {
            REG_PRA => self.pra,
            REG_PRB => self.prb,
            REG_DDRA => self.ddra,
            REG_DDRB => self.ddrb,
            REG_TALO => (self.timer_a & 0xFF) as u8,
            REG_TAHI => (self.timer_a >> 8) as u8,
            REG_TBLO => (self.timer_b & 0xFF) as u8,
            REG_TBHI => (self.timer_b >> 8) as u8,
            REG_TOD_LO => {
                let val = if self.tod_latched {
                    self.tod_latch[0]
                } else {
                    self.tod[0]
                };
                self.tod_latched = false;
                val
            }
            REG_TOD_MID => {
                if self.tod_latched {
                    self.tod_latch[1]
                } else {
                    self.tod[1]
                }
            }
            REG_TOD_HI => {
                self.tod_latched = true;
                self.tod_latch = self.tod;
                self.tod_latch[2]
            }
            REG_SDR => self.sdr,
            REG_ICR => {
                let mut val = self.icr_data;
                if self.icr_ir {
                    val |= 0x80;
                }
                self.icr_data = 0;
                self.icr_ir = false;
                val
            }
            REG_CRA => self.cra,
            REG_CRB => self.crb,
            _ => 0,
        }
    }

    /// Write a CIA register by index (0x0–0xF).
    pub fn write(&mut self, reg: u8, value: u8) {
        let reg_index = usize::from(reg & 0x0F);
        self.register_write_counts[reg_index] += 1;
        self.last_register_writes[reg_index] = value;

        match reg {
            REG_PRA => self.pra = value,
            REG_PRB => self.prb = value,
            REG_DDRA => self.ddra = value,
            REG_DDRB => self.ddrb = value,
            REG_TALO => self.timer_a_latch = (self.timer_a_latch & 0xFF00) | u16::from(value),
            REG_TAHI => {
                self.timer_a_latch = (self.timer_a_latch & 0x00FF) | (u16::from(value) << 8);
                // Writing high byte loads counter if timer stopped. In one-shot
                // mode, the high-byte write also starts the timer.
                if self.cra & (CR_START | CR_ONESHOT) != CR_START {
                    self.timer_a = self.timer_a_latch;
                }
                if self.cra & CR_ONESHOT != 0 {
                    if self.cra & CR_START == 0 {
                        self.timer_a_stats.auto_start_writes += 1;
                    }
                    self.cra |= CR_START;
                }
            }
            REG_TBLO => self.timer_b_latch = (self.timer_b_latch & 0xFF00) | u16::from(value),
            REG_TBHI => {
                self.timer_b_latch = (self.timer_b_latch & 0x00FF) | (u16::from(value) << 8);
                if self.crb & (CR_START | CR_ONESHOT) != CR_START {
                    self.timer_b = self.timer_b_latch;
                }
                if self.crb & CR_ONESHOT != 0 {
                    if self.crb & CR_START == 0 {
                        self.timer_b_stats.auto_start_writes += 1;
                    }
                    self.crb |= CR_START;
                }
            }
            REG_TOD_LO => {
                if self.crb & 0x80 != 0 {
                    self.tod_alarm[0] = value;
                } else {
                    self.tod[0] = value;
                }
            }
            REG_TOD_MID => {
                if self.crb & 0x80 != 0 {
                    self.tod_alarm[1] = value;
                } else {
                    self.tod[1] = value;
                }
            }
            REG_TOD_HI => {
                if self.crb & 0x80 != 0 {
                    self.tod_alarm[2] = value;
                } else {
                    self.tod[2] = value;
                }
            }
            REG_SDR => self.sdr = value,
            REG_ICR => {
                if value & ICR_SET != 0 {
                    self.icr_mask |= value & 0x1F;
                } else {
                    self.icr_mask &= !(value & 0x1F);
                }
                // RethinkICR: if pending bits now match mask, assert IR
                if self.icr_data & self.icr_mask & 0x1F != 0 && !self.icr_ir {
                    self.icr_ir = true;
                }
            }
            REG_CRA => {
                update_timer_control_stats(&mut self.timer_a_stats, self.cra, value);
                self.cra = value & !CR_LOAD;
                if value & CR_LOAD != 0 {
                    self.timer_a = self.timer_a_latch;
                }
            }
            REG_CRB => {
                update_timer_control_stats(&mut self.timer_b_stats, self.crb, value);
                self.crb = value & !CR_LOAD;
                if value & CR_LOAD != 0 {
                    self.timer_b = self.timer_b_latch;
                }
            }
            _ => {}
        }
    }
}

impl CiaState {
    /// Advance timers by one tick. Returns `true` if an interrupt should fire.
    pub fn tick(&mut self) -> bool {
        if self.cra & CR_START != 0 {
            self.timer_a = self.timer_a.wrapping_sub(1);
            if self.timer_a == 0xFFFF {
                self.icr_data |= ICR_TA;
                self.timer_a_stats.underflows += 1;
                self.timer_a = self.timer_a_latch;
                if self.cra & CR_ONESHOT != 0 {
                    self.cra &= !CR_START;
                }
            }
        }

        if self.crb & CR_START != 0 {
            self.timer_b = self.timer_b.wrapping_sub(1);
            if self.timer_b == 0xFFFF {
                self.icr_data |= ICR_TB;
                self.timer_b_stats.underflows += 1;
                self.timer_b = self.timer_b_latch;
                if self.crb & CR_ONESHOT != 0 {
                    self.crb &= !CR_START;
                }
            }
        }

        // RethinkICR: only assert interrupt line on 0→1 transition of IR
        if self.icr_data & self.icr_mask & 0x1F != 0 && !self.icr_ir {
            self.icr_ir = true;
            return true;
        }
        false
    }

    /// Advance the TOD counter by one tick.
    pub fn tick_tod(&mut self) {
        self.tod[0] = self.tod[0].wrapping_add(1);
        if self.tod[0] == 0 {
            self.tod[1] = self.tod[1].wrapping_add(1);
            if self.tod[1] == 0 {
                self.tod[2] = self.tod[2].wrapping_add(1);
            }
        }
        // TOD alarm check disabled - causes spurious INT_EXTER during boot
        // TODO: investigate proper TOD alarm timing
    }

    /// Check if the interrupt line is asserted.
    #[must_use]
    pub const fn irq_pending(&self) -> bool {
        self.icr_ir
    }

    /// Trigger the /FLAG interrupt (bit 4, 0x10).
    /// Returns true if an interrupt should be asserted to the CPU.
    pub fn set_flag(&mut self) -> bool {
        self.icr_data |= 0x10; // ICR_FLG (bit 4)
        if self.icr_data & self.icr_mask & 0x1F != 0 && !self.icr_ir {
            self.icr_ir = true;
            true
        } else {
            false
        }
    }
}

fn update_timer_control_stats(stats: &mut CiaTimerStats, previous: u8, next: u8) {
    if previous & CR_START == 0 && next & CR_START != 0 {
        stats.start_writes += 1;
    }
    if previous & CR_START != 0 && next & CR_START == 0 {
        stats.stop_writes += 1;
    }
    if next & CR_LOAD != 0 {
        stats.force_load_writes += 1;
    }
}

impl Default for CiaState {
    fn default() -> Self {
        Self::new()
    }
}

/// A pair of CIA chips (CIA-A and CIA-B) as found in the Amiga.
#[derive(Clone, Debug)]
pub struct CiaPair {
    /// CIA-A (directly drives keyboard, gameports, disk, LED).
    pub cia_a: CiaState,
    /// CIA-B (directly drives parallel port, disk control).
    pub cia_b: CiaState,
}

impl CiaPair {
    /// Create a new CIA pair in reset state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cia_a: CiaState::new(),
            cia_b: CiaState::new(),
        }
    }
}

impl Default for CiaPair {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_a_countdown_and_underflow() {
        let mut cia = CiaState::new();
        cia.write(REG_TALO, 3);
        cia.write(REG_TAHI, 0);
        // Enable timer A interrupt
        cia.write(REG_ICR, ICR_SET | ICR_TA);
        // Start timer
        cia.write(REG_CRA, CR_START);

        assert!(!cia.tick()); // 3 -> 2
        assert!(!cia.tick()); // 2 -> 1
        assert!(!cia.tick()); // 1 -> 0
        assert!(cia.tick()); // 0 -> underflow, reload
        assert_eq!(cia.timer_a, 3);
    }

    #[test]
    fn one_shot_stops_after_underflow() {
        let mut cia = CiaState::new();
        cia.write(REG_TALO, 1);
        cia.write(REG_TAHI, 0);
        cia.write(REG_CRA, CR_START | CR_ONESHOT);

        cia.tick(); // 1 -> 0
        cia.tick(); // 0 -> underflow, stop
        assert_eq!(cia.cra & CR_START, 0);
        // Timer should not decrement further
        let val = cia.timer_a;
        cia.tick();
        assert_eq!(cia.timer_a, val);
    }

    #[test]
    fn one_shot_timer_a_high_byte_write_starts_timer() {
        let mut cia = CiaState::new();
        cia.write(REG_CRA, CR_ONESHOT);
        cia.write(REG_TALO, 3);
        cia.write(REG_TAHI, 0);

        assert_eq!(cia.timer_a, 3);
        assert_eq!(cia.cra & CR_START, CR_START);
        assert_eq!(cia.timer_a_stats.start_writes, 0);
        assert_eq!(cia.timer_a_stats.auto_start_writes, 1);
    }

    #[test]
    fn timer_control_write_stats_track_start_stop_and_load() {
        let mut cia = CiaState::new();

        cia.write(REG_CRA, CR_LOAD);
        cia.write(REG_CRA, CR_START | CR_LOAD);
        cia.write(REG_CRA, CR_START);
        cia.write(REG_CRA, 0);

        assert_eq!(cia.timer_a_stats.force_load_writes, 2);
        assert_eq!(cia.timer_a_stats.start_writes, 1);
        assert_eq!(cia.timer_a_stats.stop_writes, 1);
    }

    #[test]
    fn register_write_evidence_tracks_counts_and_last_values() {
        let mut cia = CiaState::new();

        cia.write(REG_TALO, 0x34);
        cia.write(REG_TALO, 0x56);
        cia.write(REG_CRA, CR_START);

        assert_eq!(cia.register_write_counts[usize::from(REG_TALO)], 2);
        assert_eq!(cia.last_register_writes[usize::from(REG_TALO)], 0x56);
        assert_eq!(cia.register_write_counts[usize::from(REG_CRA)], 1);
        assert_eq!(cia.last_register_writes[usize::from(REG_CRA)], CR_START);
    }

    #[test]
    fn icr_mask_enable_disable() {
        let mut cia = CiaState::new();
        // Enable TA interrupt
        cia.write(REG_ICR, ICR_SET | ICR_TA);
        assert_eq!(cia.icr_mask & ICR_TA, ICR_TA);
        // Disable TA interrupt
        cia.write(REG_ICR, ICR_TA);
        assert_eq!(cia.icr_mask & ICR_TA, 0);
    }

    #[test]
    fn icr_read_clears_pending() {
        let mut cia = CiaState::new();
        cia.icr_data = ICR_TA | ICR_TB;
        let val = cia.read(REG_ICR);
        assert_eq!(val, ICR_TA | ICR_TB);
        assert_eq!(cia.icr_data, 0);
    }

    #[test]
    fn tod_counter_increments() {
        let mut cia = CiaState::new();
        for _ in 0..256 {
            cia.tick_tod();
        }
        assert_eq!(cia.tod[0], 0);
        assert_eq!(cia.tod[1], 1);
    }

    #[test]
    fn register_read_write_roundtrip() {
        let mut cia = CiaState::new();
        cia.write(REG_PRA, 0xAB);
        assert_eq!(cia.read(REG_PRA), 0xAB);
        cia.write(REG_DDRA, 0xCD);
        assert_eq!(cia.read(REG_DDRA), 0xCD);
        cia.write(REG_SDR, 0x42);
        assert_eq!(cia.read(REG_SDR), 0x42);
    }
}
