// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Host clock contract for the desktop shell.

use std::time::Duration;

use rumiga_platform::Clock;
use rumiga_platform_desktop::DesktopClock;

#[test]
fn now_is_monotonic() {
    let clock = DesktopClock::new();
    let mut previous = clock.now();
    for _ in 0..1000 {
        let current = clock.now();
        assert!(current >= previous, "clock went backwards");
        previous = current;
    }
}

#[test]
fn pace_reports_at_least_the_requested_duration() {
    let mut clock = DesktopClock::new();
    let requested = Duration::from_millis(5);
    let actual = clock.pace(requested);
    // The contract promises a measurement, and a host sleep never returns early.
    assert!(
        actual >= requested,
        "reported {actual:?} for a requested {requested:?}"
    );
}

#[test]
fn pace_with_zero_yields_without_claiming_time_it_did_not_spend() {
    let mut clock = DesktopClock::new();
    let actual = clock.pace(Duration::ZERO);
    // A yield is allowed to take a moment, but it must not report a full sleep.
    assert!(actual < Duration::from_millis(50), "yield took {actual:?}");
}

#[test]
fn now_advances_across_a_pace_call() {
    let mut clock = DesktopClock::new();
    let before = clock.now();
    clock.pace(Duration::from_millis(2));
    assert!(clock.now() > before, "the clock did not advance");
}
