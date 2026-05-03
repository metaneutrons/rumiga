// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! On-device OSD overlay using Slint.
//!
//! Renders status information (FPS, disk activity, volume) in the display
//! black bars surrounding the Amiga framebuffer. Touch zones in the bars
//! provide virtual controls (fire button, menu).
//!
//! ## Architecture
//!
//! The OSD renders to a separate buffer that is composited with the Amiga
//! framebuffer before presentation to the MIPI-DSI display. This ensures
//! the OSD never interferes with emulation rendering.
//!
//! ## Dependencies
//!
//! Requires the Slint ESP-IDF component (GPL-3.0 compatible).
//! Add to ESP-IDF project: `idf.py add-dependency slint/slint`

// TODO: implement when Slint ESP-IDF integration is configured
// - OSD elements: FPS counter, disk LED, drive status, volume
// - Touch zones: fire button, menu button
// - Menu overlay: pause, reset, eject, settings
