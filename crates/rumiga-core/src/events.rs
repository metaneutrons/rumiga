// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Cycle-accurate event scheduler for the Amiga timing engine.
//!
//! Uses a fixed-size array of event slots — no heap allocation for the
//! scheduler itself.

/// Color clocks per scanline (PAL and NTSC).
pub const CYCLES_PER_SCANLINE: u64 = 227;

/// Total scanlines per frame (PAL).
pub const SCANLINES_PAL: u64 = 312;

/// Total scanlines per frame (NTSC).
pub const SCANLINES_NTSC: u64 = 262;

/// PAL colour clock in hertz.
///
/// Emulated time is derived from this rather than from a rounded frame rate, so a
/// PAL frame is 19.968 ms and not the frequently quoted 20 ms.
pub const COLOUR_CLOCK_PAL_HZ: u64 = 3_546_895;

/// NTSC colour clock in hertz.
pub const COLOUR_CLOCK_NTSC_HZ: u64 = 3_579_545;

/// Number of event slots (one per event type).
const EVENT_SLOTS: usize = 7;

/// Types of scheduled events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventType {
    /// Horizontal sync (end of scanline).
    HSync = 0,
    /// Vertical sync (end of frame).
    VSync = 1,
    /// CIA timer event.
    Cia = 2,
    /// Audio DMA event.
    Audio = 3,
    /// Blitter completion event.
    Blitter = 4,
    /// Copper event.
    Copper = 5,
    /// Miscellaneous event.
    Misc = 6,
}

/// A single event slot.
#[derive(Clone, Copy, Debug)]
struct EventSlot {
    /// Whether this event is pending.
    active: bool,
    /// Absolute cycle at which this event fires.
    trigger_cycle: u64,
}

/// Fixed-size cycle-accurate event scheduler.
#[derive(Clone, Debug)]
pub struct EventScheduler {
    /// Current cycle counter.
    current_cycle: u64,
    /// Event slots indexed by `EventType`.
    slots: [EventSlot; EVENT_SLOTS],
}

impl Default for EventScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl EventScheduler {
    /// Create a new scheduler with no pending events.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            current_cycle: 0,
            slots: [EventSlot {
                active: false,
                trigger_cycle: 0,
            }; EVENT_SLOTS],
        }
    }

    /// Current cycle count.
    #[must_use]
    pub const fn current_cycle(&self) -> u64 {
        self.current_cycle
    }

    /// Schedule an event to fire after `cycles_from_now` color clocks.
    pub fn schedule(&mut self, event_type: EventType, cycles_from_now: u64) {
        let slot = &mut self.slots[event_type as usize];
        slot.active = true;
        slot.trigger_cycle = self.current_cycle + cycles_from_now;
    }

    /// Cancel a pending event.
    pub fn cancel(&mut self, event_type: EventType) {
        self.slots[event_type as usize].active = false;
    }

    /// Advance the scheduler by `cycles` color clocks.
    pub fn advance(&mut self, cycles: u64) {
        self.current_cycle += cycles;
    }

    /// Check for and collect all events whose trigger cycle has been reached.
    ///
    /// Returns fired event types. Fired events are deactivated.
    pub fn check_and_fire(&mut self) -> FiredEvents {
        let mut fired = FiredEvents::new();
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.active && self.current_cycle >= slot.trigger_cycle {
                slot.active = false;
                fired.add(i);
            }
        }
        fired
    }

    /// Check if a specific event is pending.
    #[must_use]
    pub const fn is_pending(&self, event_type: EventType) -> bool {
        self.slots[event_type as usize].active
    }
}

/// Collection of fired events from a single `check_and_fire` call.
#[derive(Clone, Debug)]
pub struct FiredEvents {
    /// Bitmask of fired event type indices.
    mask: u8,
}

impl FiredEvents {
    const fn new() -> Self {
        Self { mask: 0 }
    }

    fn add(&mut self, index: usize) {
        self.mask |= 1 << index;
    }

    /// Check if a specific event type fired.
    #[must_use]
    pub const fn contains(&self, event_type: EventType) -> bool {
        self.mask & (1 << event_type as u8) != 0
    }

    /// Returns true if no events fired.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.mask == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_and_fire() {
        let mut sched = EventScheduler::new();
        sched.schedule(EventType::HSync, 10);
        assert!(sched.is_pending(EventType::HSync));

        sched.advance(9);
        let fired = sched.check_and_fire();
        assert!(fired.is_empty());

        sched.advance(1);
        let fired = sched.check_and_fire();
        assert!(fired.contains(EventType::HSync));
        assert!(!sched.is_pending(EventType::HSync));
    }

    #[test]
    fn multiple_events_fire_together() {
        let mut sched = EventScheduler::new();
        sched.schedule(EventType::HSync, 5);
        sched.schedule(EventType::Copper, 5);
        sched.advance(5);
        let fired = sched.check_and_fire();
        assert!(fired.contains(EventType::HSync));
        assert!(fired.contains(EventType::Copper));
        assert!(!fired.contains(EventType::Blitter));
    }

    #[test]
    fn cancel_event() {
        let mut sched = EventScheduler::new();
        sched.schedule(EventType::Cia, 100);
        assert!(sched.is_pending(EventType::Cia));
        sched.cancel(EventType::Cia);
        assert!(!sched.is_pending(EventType::Cia));
        sched.advance(100);
        let fired = sched.check_and_fire();
        assert!(fired.is_empty());
    }

    #[test]
    fn scanline_timing_constants() {
        assert_eq!(CYCLES_PER_SCANLINE, 227);
        assert_eq!(SCANLINES_PAL, 312);
        assert_eq!(SCANLINES_NTSC, 262);
        // Full PAL frame
        assert_eq!(CYCLES_PER_SCANLINE * SCANLINES_PAL, 70_824);
    }

    #[test]
    fn event_fires_exactly_at_trigger() {
        let mut sched = EventScheduler::new();
        sched.schedule(EventType::VSync, CYCLES_PER_SCANLINE * SCANLINES_PAL);
        sched.advance(CYCLES_PER_SCANLINE * SCANLINES_PAL - 1);
        assert!(sched.check_and_fire().is_empty());
        sched.advance(1);
        let fired = sched.check_and_fire();
        assert!(fired.contains(EventType::VSync));
    }
}
