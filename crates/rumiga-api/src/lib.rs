// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Shared REST API types for the Rumiga emulator.
//!
//! These types are used by both the ESP32 firmware (axum server) and the
//! Next.js web UI (via TypeScript code generation).

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

// ─── File Management ─────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    pub is_directory: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileListResponse {
    pub path: String,
    pub files: Vec<FileEntry>,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FormatRequest {
    pub confirm_token: String,
}

// ─── WiFi ────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WifiNetwork {
    pub ssid: String,
    pub rssi: i8,
    pub secured: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WifiStatus {
    pub connected: bool,
    pub ssid: Option<String>,
    pub ip: Option<String>,
    pub mode: WifiMode,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum WifiMode {
    SoftAp,
    Client,
    Disconnected,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WifiConnectRequest {
    pub ssid: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WifiScanResponse {
    pub networks: Vec<WifiNetwork>,
}

// ─── Machine Configuration ───────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum AmigaModel {
    A500,
    A500Plus,
    A1200,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChannelMixConfig {
    pub left_pct: u8,
    pub right_pct: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AudioConfig {
    pub channel_mix: [ChannelMixConfig; 4],
}

impl Default for AudioConfig {
    fn default() -> Self {
        // Standard Amiga stereo: channels 0,3 left; channels 1,2 right
        Self {
            channel_mix: [
                ChannelMixConfig {
                    left_pct: 100,
                    right_pct: 0,
                },
                ChannelMixConfig {
                    left_pct: 0,
                    right_pct: 100,
                },
                ChannelMixConfig {
                    left_pct: 0,
                    right_pct: 100,
                },
                ChannelMixConfig {
                    left_pct: 100,
                    right_pct: 0,
                },
            ],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ScalingMode {
    Integer,
    AspectFit,
    Stretch,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DisplayConfig {
    pub scaling: ScalingMode,
    pub orientation_landscape: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            scaling: ScalingMode::Integer,
            orientation_landscape: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MachineConfig {
    pub model: AmigaModel,
    pub chip_ram_kb: u32,
    pub slow_ram_kb: u32,
    pub fast_ram_kb: u32,
    pub rom_file: String,
    pub floppy: [Option<String>; 4],
    pub audio: AudioConfig,
    pub display: DisplayConfig,
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self {
            model: AmigaModel::A500,
            chip_ram_kb: 512,
            slow_ram_kb: 512,
            fast_ram_kb: 0,
            rom_file: String::new(),
            floppy: [None, None, None, None],
            audio: AudioConfig::default(),
            display: DisplayConfig::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MachineStatus {
    pub running: bool,
    pub fps: f32,
    pub model: AmigaModel,
}

// ─── Generic API Response ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    #[must_use]
    pub const fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    #[must_use]
    pub const fn err(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
        }
    }
}
