// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! ESP-IDF platform backend for the Rumiga emulator.
//!
//! Targets the Seeed reTerminal D1001 (ESP32-P4) with:
//! - MIPI-DSI display output (800x1280, 8")
//! - I2S audio via ES8311 codec
//! - SD/MMC storage with FAT32
//! - WiFi 6 via ESP32-C6 (SDIO)
//! - USB HID input + capacitive touch (GSL3670)
//! - REST API via axum/tokio

pub mod api;
pub mod audio;
pub mod display;
pub mod input;
pub mod osd;
pub mod storage;
pub mod wifi;
