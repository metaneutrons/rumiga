// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Copper coprocessor emulation.
//!
//! The Copper is a simple coprocessor that executes a list of instructions
//! synchronised to the video beam. It supports three instructions: MOVE
//! (write a register), WAIT (wait for beam position), and SKIP (conditionally
//! skip the next instruction).

#[cfg(test)]
use alloc::vec::Vec;

/// Minimum register offset the Copper can write without the danger bit.
const SAFE_REG_MIN: u16 = 0x40;

/// Execution state of the Copper coprocessor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CopperExecState {
    /// Copper is idle (DMA disabled or waiting forever).
    #[default]
    Idle,
    /// Fetching the first instruction word.
    FetchFirst,
    /// Fetching the second instruction word.
    FetchSecond,
    /// Executing the decoded instruction.
    Execute,
}

/// Action produced by the Copper on a given cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopperAction {
    /// Copper MOVE: write `value` to custom register at `offset`.
    WriteRegister {
        /// Register offset from `$DFF000`.
        offset: u16,
        /// Value to write.
        value: u16,
    },
}

/// Copper coprocessor state.
#[derive(Clone, Debug)]
pub struct CopperState {
    /// Copper list 1 pointer.
    pub cop1lc: u32,
    /// Copper list 2 pointer.
    pub cop2lc: u32,
    /// Current program counter.
    pub pc: u32,
    /// First instruction word (fetched).
    pub ir1: u16,
    /// Second instruction word (fetched).
    pub ir2: u16,
    /// Current execution state.
    pub state: CopperExecState,
    /// Copper DMA enabled.
    pub enabled: bool,
    /// COPCON danger bit (allows writes to registers `$00`–`$3F`).
    pub danger: bool,
}

impl Default for CopperState {
    fn default() -> Self {
        Self::new()
    }
}

impl CopperState {
    /// Create a new Copper in its initial (idle) state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cop1lc: 0,
            cop2lc: 0,
            pc: 0,
            ir1: 0,
            ir2: 0,
            state: CopperExecState::Idle,
            enabled: false,
            danger: false,
        }
    }

    /// Reset the Copper to its initial state, loading PC from `cop1lc`.
    pub fn reset(&mut self) {
        self.pc = self.cop1lc;
        self.ir1 = 0;
        self.ir2 = 0;
        self.state = CopperExecState::FetchFirst;
    }

    /// Called on vertical blank — restarts execution from `cop1lc`.
    pub fn restart_vertical_blank(&mut self) {
        self.pc = self.cop1lc;
        self.ir1 = 0;
        self.ir2 = 0;
        self.state = CopperExecState::FetchFirst;
    }

    /// Set the Copper list 1 address from high and low words.
    pub fn set_cop1lc(&mut self, high: u16, low: u16) {
        self.cop1lc = (u32::from(high) << 16) | u32::from(low);
    }

    /// Set the Copper list 2 address from high and low words.
    pub fn set_cop2lc(&mut self, high: u16, low: u16) {
        self.cop2lc = (u32::from(high) << 16) | u32::from(low);
    }

    /// Strobe: restart execution from `cop1lc`.
    pub fn strobe_cop1(&mut self) {
        self.pc = self.cop1lc;
        self.state = CopperExecState::FetchFirst;
    }

    /// Strobe: restart execution from `cop2lc`.
    pub fn strobe_cop2(&mut self) {
        self.pc = self.cop2lc;
        self.state = CopperExecState::FetchFirst;
    }

    /// Execute one Copper DMA cycle.
    ///
    /// Reads instruction words directly from `chip_ram` (the Copper can only
    /// access chip RAM). Returns a [`CopperAction`] when the Copper wants to
    /// write a custom register.
    pub fn cycle(&mut self, chip_ram: &[u8], vpos: u16, hpos: u16) -> Option<CopperAction> {
        if !self.enabled {
            return None;
        }

        match self.state {
            CopperExecState::Idle => None,
            CopperExecState::FetchFirst => {
                self.ir1 = self.read_word(chip_ram);
                self.pc = self.pc.wrapping_add(2);
                self.state = CopperExecState::FetchSecond;
                None
            }
            CopperExecState::FetchSecond => {
                self.ir2 = self.read_word(chip_ram);
                self.pc = self.pc.wrapping_add(2);
                self.state = CopperExecState::Execute;
                None
            }
            CopperExecState::Execute => self.execute(vpos, hpos),
        }
    }

    /// Read a big-endian word from chip RAM at the current PC.
    /// The copper can only access chip RAM; addresses wrap within chip RAM size.
    const fn read_word(&self, chip_ram: &[u8]) -> u16 {
        if chip_ram.is_empty() {
            return 0;
        }

        let addr = (self.pc as usize) % chip_ram.len();
        if addr + 1 < chip_ram.len() {
            u16::from_be_bytes([chip_ram[addr], chip_ram[addr + 1]])
        } else {
            0
        }
    }

    /// Execute the decoded instruction pair (IR1/IR2).
    fn execute(&mut self, vpos: u16, hpos: u16) -> Option<CopperAction> {
        // All-zero = uninitialized memory, treat as end-of-list
        if self.ir1 == 0 && self.ir2 == 0 {
            self.state = CopperExecState::Idle;
            return None;
        }

        if self.ir1 & 1 == 0 {
            // MOVE instruction
            self.state = CopperExecState::FetchFirst;
            let offset = self.ir1 & 0x01FE;
            if offset < SAFE_REG_MIN && !self.danger {
                return None;
            }
            Some(CopperAction::WriteRegister {
                offset,
                value: self.ir2,
            })
        } else {
            // WAIT or SKIP
            let target_v = (self.ir1 >> 8) & 0xFF;
            let target_h = (self.ir1 >> 1) & 0x7F;
            let vmask = (self.ir2 >> 8) | 0x80;
            let hmask = (self.ir2 >> 1) & 0x7F;
            let is_skip = self.ir2 & 1 != 0;

            let beam_v = vpos & vmask;
            let wait_v = target_v & vmask;
            // Vertical priority: if beam is past target vertically, pass immediately
            let condition_met =
                beam_v > wait_v || (beam_v == wait_v && (hpos & hmask) >= (target_h & hmask));

            if is_skip {
                self.state = CopperExecState::FetchFirst;
                if condition_met {
                    self.pc = self.pc.wrapping_add(4);
                }
            } else if condition_met {
                // WAIT: condition met, advance
                self.state = CopperExecState::FetchFirst;
            }
            // else: WAIT stays in Execute state (keep waiting)
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build chip RAM with a copper list at address 0.
    fn make_chip_ram(instructions: &[(u16, u16)]) -> Vec<u8> {
        let mut ram = Vec::new();
        for &(w1, w2) in instructions {
            ram.extend_from_slice(&w1.to_be_bytes());
            ram.extend_from_slice(&w2.to_be_bytes());
        }
        ram
    }

    fn enabled_copper(chip_ram: &[(u16, u16)]) -> (CopperState, Vec<u8>) {
        let ram = make_chip_ram(chip_ram);
        let mut copper = CopperState::new();
        copper.enabled = true;
        copper.state = CopperExecState::FetchFirst;
        (copper, ram)
    }

    #[test]
    fn move_writes_correct_register_and_value() {
        // MOVE COLOR00 ($180), value $0ABC
        let (mut copper, ram) = enabled_copper(&[(0x0180, 0x0ABC)]);

        // Fetch first word
        assert_eq!(copper.cycle(&ram, 0, 0), None);
        // Fetch second word
        assert_eq!(copper.cycle(&ram, 0, 0), None);
        // Execute
        let action = copper.cycle(&ram, 0, 0);
        assert_eq!(
            action,
            Some(CopperAction::WriteRegister {
                offset: 0x0180,
                value: 0x0ABC,
            })
        );
    }

    #[test]
    fn empty_chip_ram_reads_as_zero() {
        let mut copper = CopperState::new();
        copper.enabled = true;
        copper.state = CopperExecState::FetchFirst;

        assert_eq!(copper.cycle(&[], 0, 0), None);
        assert_eq!(copper.ir1, 0);
    }

    #[test]
    fn move_blocked_when_offset_below_40_and_no_danger() {
        // MOVE to $02E (COPCON), should be blocked
        let (mut copper, ram) = enabled_copper(&[(0x002E, 0x0002)]);

        copper.cycle(&ram, 0, 0); // fetch first
        copper.cycle(&ram, 0, 0); // fetch second
        let action = copper.cycle(&ram, 0, 0);
        assert_eq!(action, None);
    }

    #[test]
    fn move_allowed_below_40_with_danger() {
        let (mut copper, ram) = enabled_copper(&[(0x002E, 0x0002)]);
        copper.danger = true;

        copper.cycle(&ram, 0, 0);
        copper.cycle(&ram, 0, 0);
        let action = copper.cycle(&ram, 0, 0);
        assert_eq!(
            action,
            Some(CopperAction::WriteRegister {
                offset: 0x002E,
                value: 0x0002,
            })
        );
    }

    #[test]
    fn wait_blocks_until_beam_matches() {
        // WAIT for vpos=100, hpos=0, full masks
        let ir1: u16 = (100 << 8) | 1;
        let ir2: u16 = (0x7F << 8) | (0x7F << 1); // WAIT (bit 0 = 0)
        let (mut copper, ram) = enabled_copper(&[(ir1, ir2)]);

        copper.cycle(&ram, 0, 0); // fetch first
        copper.cycle(&ram, 0, 0); // fetch second

        // Execute at vpos=50 — should stay waiting
        assert_eq!(copper.cycle(&ram, 50, 0), None);
        assert_eq!(copper.state, CopperExecState::Execute);

        // Execute at vpos=100 — should pass
        assert_eq!(copper.cycle(&ram, 100, 0), None);
        assert_eq!(copper.state, CopperExecState::FetchFirst);
    }

    #[test]
    fn wait_passes_when_beam_already_past() {
        // WAIT for vpos=50, hpos=10
        let ir1: u16 = (50 << 8) | (10 << 1) | 1;
        let ir2: u16 = (0x7F << 8) | (0x7F << 1);
        let (mut copper, ram) = enabled_copper(&[(ir1, ir2)]);

        copper.cycle(&ram, 0, 0); // fetch first
        copper.cycle(&ram, 0, 0); // fetch second

        // Beam is already past (vpos=200, hpos=50)
        assert_eq!(copper.cycle(&ram, 200, 50), None);
        assert_eq!(copper.state, CopperExecState::FetchFirst);
    }

    #[test]
    fn end_of_list_never_completes() {
        // End of copper list: WAIT $FFFF/$FFFE
        let (mut copper, ram) = enabled_copper(&[(0xFFFF, 0xFFFE)]);

        copper.cycle(&ram, 0, 0); // fetch first
        copper.cycle(&ram, 0, 0); // fetch second

        // On real hardware, hpos never reaches 0x7F (127) in copper coordinates.
        // Max hpos is ~226 color clocks, and 226 & 0x7F = 98 < 127.
        // So the horizontal condition is never satisfied.
        assert_eq!(copper.cycle(&ram, 255, 100), None);
        assert_eq!(copper.state, CopperExecState::Execute);

        assert_eq!(copper.cycle(&ram, 311, 113), None);
        assert_eq!(copper.state, CopperExecState::Execute);
    }

    #[test]
    fn skip_advances_pc_when_condition_met() {
        // SKIP if vpos >= 0, hpos >= 0 (always true), then a MOVE
        let ir1: u16 = 1;
        let ir2: u16 = (0x7F << 8) | (0x7F << 1) | 1; // SKIP (bit 0 = 1)
        // Second instruction (should be skipped): MOVE COLOR00
        let (mut copper, ram) = enabled_copper(&[(ir1, ir2), (0x0180, 0x0FFF)]);

        copper.cycle(&ram, 10, 10); // fetch first
        copper.cycle(&ram, 10, 10); // fetch second

        let pc_before = copper.pc;
        assert_eq!(copper.cycle(&ram, 10, 10), None); // execute SKIP
        assert_eq!(copper.state, CopperExecState::FetchFirst);
        assert_eq!(copper.pc, pc_before + 4); // skipped one instruction
    }

    #[test]
    fn vertical_blank_restart_resets_pc() {
        let mut copper = CopperState::new();
        copper.set_cop1lc(0x0000, 0x1000);
        copper.pc = 0x5000;
        copper.state = CopperExecState::Execute;

        copper.restart_vertical_blank();

        assert_eq!(copper.pc, 0x0000_1000);
        assert_eq!(copper.state, CopperExecState::FetchFirst);
    }
}
