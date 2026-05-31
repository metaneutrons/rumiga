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
    #[serde(default = "default_stereo_separation")]
    pub stereo_separation: u8,
}

const fn default_stereo_separation() -> u8 {
    100
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
            stereo_separation: default_stereo_separation(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ScalingMode {
    Integer,
    AspectFit,
    Stretch,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ViewportMode {
    /// Use the emulator's raw framebuffer.
    Raw,
    /// Derive a sane viewport from the active Amiga display.
    Auto,
    /// Use the explicit viewport rectangle.
    Manual,
}

impl Default for ViewportMode {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ViewportPreset {
    /// Keep the complete native framebuffer, including chipset border.
    NativeFullBorder,
    /// Derive the active Amiga display area from DIW/DDF state.
    VisibleArea,
    /// Keep the full overscan-capable native framebuffer.
    Overscan,
    /// Center the active display while preserving native frame evidence.
    AutoCenter,
}

impl Default for ViewportPreset {
    fn default() -> Self {
        Self::AutoCenter
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ViewportConfig {
    #[serde(default)]
    pub mode: ViewportMode,
    #[serde(default)]
    pub preset: ViewportPreset,
    #[serde(default)]
    pub x: i16,
    #[serde(default)]
    pub y: i16,
    #[serde(default = "default_viewport_width")]
    pub width: u16,
    #[serde(default = "default_viewport_height")]
    pub height: u16,
    #[serde(default = "default_vertical_stretch")]
    pub vertical_stretch: bool,
}

impl Default for ViewportConfig {
    fn default() -> Self {
        Self {
            mode: ViewportMode::Auto,
            preset: ViewportPreset::AutoCenter,
            x: 0,
            y: 0,
            width: default_viewport_width(),
            height: default_viewport_height(),
            vertical_stretch: true,
        }
    }
}

const fn default_viewport_width() -> u16 {
    754
}

const fn default_viewport_height() -> u16 {
    288
}

const fn default_vertical_stretch() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DisplayConfig {
    pub scaling: ScalingMode,
    pub orientation_landscape: bool,
    #[serde(default)]
    pub viewport: ViewportConfig,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            scaling: ScalingMode::Integer,
            orientation_landscape: true,
            viewport: ViewportConfig::default(),
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
    #[serde(default = "default_floppy_speed_percent")]
    pub floppy_speed_percent: u16,
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
            floppy_speed_percent: default_floppy_speed_percent(),
            audio: AudioConfig::default(),
            display: DisplayConfig::default(),
        }
    }
}

const fn default_floppy_speed_percent() -> u16 {
    100
}

#[must_use]
pub const fn is_supported_floppy_speed_percent(percent: u16) -> bool {
    matches!(percent, 0 | 100 | 200 | 400 | 800)
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
