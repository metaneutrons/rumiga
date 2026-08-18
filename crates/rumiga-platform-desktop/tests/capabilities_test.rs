// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Capability report contract for the desktop backend.
//!
//! These tests hold the desktop report to what the adapter actually implements.
//! A descriptor that overstates a backend is worse than no descriptor, because a
//! shell would then trust it.

use rumiga_platform::{CONTRACT_VERSION, CapabilityReport, PixelFormat};
use rumiga_platform_desktop::DesktopBackend;

#[test]
fn the_desktop_backend_matches_this_contract_version() {
    let capabilities = DesktopBackend::new(754, 288).capabilities();

    assert_eq!(capabilities.contract_version, CONTRACT_VERSION);
    assert_eq!(capabilities.validate(), Ok(()));
}

#[test]
fn reported_video_bounds_are_the_ones_the_caller_gave() {
    // The shell passes its own framebuffer bounds, so the report cannot drift from
    // the buffer that is actually allocated.
    let capabilities = DesktopBackend::new(754, 288).capabilities();
    let video = capabilities.video.expect("the desktop presents frames");

    assert_eq!(video.max_width, 754);
    assert_eq!(video.max_height, 288);
    assert_eq!(video.pixel_format, PixelFormat::Rgb565);
}

#[test]
fn the_desktop_admits_it_cannot_report_backpressure() {
    // minifb either presents or fails. Claiming backpressure support would make a
    // zero dropped-frame count look like evidence of health.
    let video = DesktopBackend::new(754, 288)
        .capabilities()
        .video
        .expect("the desktop presents frames");

    assert!(!video.reports_backpressure);
}

#[test]
fn the_desktop_reports_no_audio_and_no_platform_storage() {
    let capabilities = DesktopBackend::new(754, 288).capabilities();

    // This adapter implements neither AudioOutput nor the platform Storage trait.
    // Absence is reported as absence rather than as zeroed limits.
    assert!(capabilities.audio.is_none());
    assert!(capabilities.storage.is_none());
}

#[test]
fn reported_input_matches_what_the_adapter_polls() {
    // DesktopInput maps a fixed key set and reports no pointer or joystick state;
    // the shell reads the mouse from its own window handle, not through this contract.
    let input = DesktopBackend::new(754, 288).capabilities().input;

    assert!(input.keyboard);
    assert!(!input.mouse);
    assert_eq!(input.joysticks, 0);
}
