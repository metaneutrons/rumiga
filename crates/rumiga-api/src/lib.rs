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

pub const API_RESPONSE_SCHEMA_ID: &str = "rumiga.api.response.v1";
pub const API_RESPONSE_SCHEMA_VERSION: u16 = 1;

pub const MACHINE_STATUS_PATH: &str = "/api/machine/status";
pub const MACHINE_CONFIG_PATH: &str = "/api/machine/config";
pub const MACHINE_SUPPORT_BUNDLE_PATH: &str = "/api/machine/support-bundle";
pub const MACHINE_RESET_PATH: &str = "/api/machine/reset";
pub const MACHINE_PAUSE_PATH: &str = "/api/machine/pause";
pub const MACHINE_RESUME_PATH: &str = "/api/machine/resume";
pub const MACHINE_START_PATH: &str = "/api/machine/start";
pub const MACHINE_STOP_PATH: &str = "/api/machine/stop";
pub const MACHINE_FLOPPY_INSERT_PATH: &str = "/api/machine/floppy/insert";
pub const MACHINE_FLOPPY_EJECT_PATH: &str = "/api/machine/floppy/eject";
pub const MACHINE_AUDIO_SEPARATION_PATH: &str = "/api/machine/audio/separation";
pub const MACHINE_SCREENSHOT_PATH: &str = "/api/machine/screenshot";
pub const FILES_PATH: &str = "/api/files";
pub const FILES_UPLOAD_PATH: &str = "/api/files/upload";
pub const FILES_DELETE_PATH: &str = "/api/files/:name";
pub const FILES_FORMAT_PATH: &str = "/api/files/format";
pub const WIFI_STATUS_PATH: &str = "/api/wifi/status";
pub const WIFI_SCAN_PATH: &str = "/api/wifi/scan";
pub const WIFI_CONNECT_PATH: &str = "/api/wifi/connect";

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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FloppyInsertRequest {
    pub drive_idx: usize,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FloppyEjectRequest {
    pub drive_idx: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AudioSeparationRequest {
    pub separation: u8,
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
    #[serde(default)]
    pub hdf_path: Option<String>,
    #[serde(default)]
    pub hdf_write_policy: HdfWritePolicy,
    pub audio: AudioConfig,
    pub display: DisplayConfig,
    #[serde(default)]
    pub network: NetworkConfig,
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
            hdf_path: None,
            hdf_write_policy: HdfWritePolicy::ReadOnly,
            audio: AudioConfig::default(),
            display: DisplayConfig::default(),
            network: NetworkConfig::default(),
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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdfWritePolicy {
    /// Keep guest writes in the session buffer and protect the source file.
    ReadOnly,
    /// Persist dirty sectors back to the source HDF on exit.
    Writeback,
}

impl HdfWritePolicy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::Writeback => "writeback",
        }
    }
}

impl Default for HdfWritePolicy {
    fn default() -> Self {
        Self::ReadOnly
    }
}

pub const DEFAULT_NETWORK_MAC_ADDRESS: &str = "00:80:10:4d:49:47";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkDevice {
    A2065,
}

impl Default for NetworkDevice {
    fn default() -> Self {
        Self::A2065
    }
}

impl NetworkDevice {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::A2065 => "a2065",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkBackend {
    Disabled,
    Slirp,
}

impl Default for NetworkBackend {
    fn default() -> Self {
        Self::Disabled
    }
}

impl NetworkBackend {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Slirp => "slirp",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct NetworkConfig {
    #[serde(default)]
    pub device: NetworkDevice,
    #[serde(default)]
    pub backend: NetworkBackend,
    #[serde(default = "default_network_mac_address")]
    pub mac_address: String,
}

impl NetworkConfig {
    #[must_use]
    pub const fn enabled(&self) -> bool {
        !matches!(self.backend, NetworkBackend::Disabled)
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            device: NetworkDevice::A2065,
            backend: NetworkBackend::Disabled,
            mac_address: default_network_mac_address(),
        }
    }
}

fn default_network_mac_address() -> String {
    String::from(DEFAULT_NETWORK_MAC_ADDRESS)
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkPacketCounters {
    #[serde(default)]
    pub tx_packets: u64,
    #[serde(default)]
    pub rx_packets: u64,
    #[serde(default)]
    pub dropped_packets: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct NetworkStatus {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub device: NetworkDevice,
    #[serde(default)]
    pub backend: NetworkBackend,
    #[serde(default = "default_network_mac_address")]
    pub mac_address: String,
    #[serde(default)]
    pub a2065_present: bool,
    #[serde(default)]
    pub a2065_configured: bool,
    #[serde(default)]
    pub a2065_shut_up: bool,
    #[serde(default)]
    pub a2065_base_address: Option<String>,
    #[serde(default = "default_network_mac_address")]
    pub a2065_card_mac_address: String,
    #[serde(default)]
    pub link_up: bool,
    #[serde(default)]
    pub counters: NetworkPacketCounters,
}

impl NetworkStatus {
    #[must_use]
    pub fn from_config(config: &NetworkConfig) -> Self {
        Self {
            enabled: config.enabled(),
            device: config.device,
            backend: config.backend,
            mac_address: config.mac_address.clone(),
            a2065_card_mac_address: config.mac_address.clone(),
            ..Self::default()
        }
    }
}

impl Default for NetworkStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            device: NetworkDevice::A2065,
            backend: NetworkBackend::Disabled,
            mac_address: default_network_mac_address(),
            a2065_present: false,
            a2065_configured: false,
            a2065_shut_up: false,
            a2065_base_address: None,
            a2065_card_mac_address: default_network_mac_address(),
            link_up: false,
            counters: NetworkPacketCounters::default(),
        }
    }
}

#[must_use]
pub fn is_valid_unicast_mac_address(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 17 {
        return false;
    }

    let mut octets = [0u8; 6];
    for (index, octet) in octets.iter_mut().enumerate() {
        let offset = index * 3;
        let Some(high) = hex_nibble(bytes[offset]) else {
            return false;
        };
        let Some(low) = hex_nibble(bytes[offset + 1]) else {
            return false;
        };
        *octet = (high << 4) | low;
        if index < 5 && bytes[offset + 2] != b':' {
            return false;
        }
    }

    let all_zero = octets.iter().all(|&octet| octet == 0);
    let all_ff = octets.iter().all(|&octet| octet == 0xFF);
    octets[0] & 0x01 == 0 && !all_zero && !all_ff
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MachineStatus {
    pub running: bool,
    pub fps: f32,
    pub model: AmigaModel,
    #[serde(default)]
    pub network: NetworkStatus,
}

// ─── Support Bundle ─────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SupportBundle {
    pub schema: String,
    pub machine: SupportMachineSummary,
    pub status: MachineStatus,
    pub display: DisplayConfig,
    pub media: SupportMediaSummary,
    pub screenshot: SupportScreenshotSummary,
    pub notes: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SupportMachineSummary {
    pub model: AmigaModel,
    pub chip_ram_kb: u32,
    pub slow_ram_kb: u32,
    pub fast_ram_kb: u32,
    pub floppy_speed_percent: u16,
    pub hdf_write_policy: HdfWritePolicy,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SupportMediaSummary {
    pub rom_name: Option<String>,
    pub hdf_name: Option<String>,
    pub floppies: [Option<String>; 4],
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SupportScreenshotSummary {
    pub available: bool,
    pub width: u32,
    pub height: u32,
    pub endpoint: String,
    pub pixel_format: String,
}

// ─── Generic API Response ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiResponseFormat {
    Json,
    Png,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiEndpoint {
    pub method: &'static str,
    pub path: &'static str,
    pub response_format: ApiResponseFormat,
}

impl ApiEndpoint {
    #[must_use]
    pub const fn new(
        method: &'static str,
        path: &'static str,
        response_format: ApiResponseFormat,
    ) -> Self {
        Self {
            method,
            path,
            response_format,
        }
    }
}

pub const API_ENDPOINTS: &[ApiEndpoint] = &[
    ApiEndpoint::new("GET", MACHINE_STATUS_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("GET", MACHINE_CONFIG_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("PUT", MACHINE_CONFIG_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("GET", MACHINE_SUPPORT_BUNDLE_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("POST", MACHINE_RESET_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("POST", MACHINE_PAUSE_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("POST", MACHINE_RESUME_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("POST", MACHINE_START_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("POST", MACHINE_STOP_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("POST", MACHINE_FLOPPY_INSERT_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("POST", MACHINE_FLOPPY_EJECT_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new(
        "POST",
        MACHINE_AUDIO_SEPARATION_PATH,
        ApiResponseFormat::Json,
    ),
    ApiEndpoint::new("GET", MACHINE_SCREENSHOT_PATH, ApiResponseFormat::Png),
    ApiEndpoint::new("GET", FILES_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("POST", FILES_UPLOAD_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("DELETE", FILES_DELETE_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("POST", FILES_FORMAT_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("GET", WIFI_STATUS_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("POST", WIFI_SCAN_PATH, ApiResponseFormat::Json),
    ApiEndpoint::new("POST", WIFI_CONNECT_PATH, ApiResponseFormat::Json),
];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiResponse<T> {
    pub schema: String,
    pub version: u16,
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

impl<T> ApiResponse<T> {
    #[must_use]
    pub fn ok(data: T) -> Self {
        Self {
            schema: String::from(API_RESPONSE_SCHEMA_ID),
            version: API_RESPONSE_SCHEMA_VERSION,
            success: true,
            data: Some(data),
            error: None,
            error_code: None,
        }
    }

    #[must_use]
    pub fn err(message: String) -> Self {
        Self::err_with_code("request_failed", message)
    }

    #[must_use]
    pub fn err_with_code(code: &'static str, message: String) -> Self {
        Self {
            schema: String::from(API_RESPONSE_SCHEMA_ID),
            version: API_RESPONSE_SCHEMA_VERSION,
            success: false,
            data: None,
            error: Some(message),
            error_code: Some(String::from(code)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_config_defaults_network_off() {
        let config = MachineConfig::default();

        assert_eq!(config.network.backend, NetworkBackend::Disabled);
        assert_eq!(config.network.device, NetworkDevice::A2065);
        assert!(!config.network.enabled());
        assert!(is_valid_unicast_mac_address(&config.network.mac_address));
    }

    #[test]
    fn network_mac_validation_rejects_broadcast_multicast_and_malformed_values() {
        assert!(is_valid_unicast_mac_address("02:52:55:4d:49:47"));
        assert!(!is_valid_unicast_mac_address("01:52:55:4d:49:47"));
        assert!(!is_valid_unicast_mac_address("ff:ff:ff:ff:ff:ff"));
        assert!(!is_valid_unicast_mac_address("00:00:00:00:00:00"));
        assert!(!is_valid_unicast_mac_address("02-52-55-4d-49-47"));
        assert!(!is_valid_unicast_mac_address("02:52:55:4d:49"));
    }

    #[test]
    fn network_status_defaults_to_disabled_a2065() {
        let status = NetworkStatus::default();

        assert!(!status.enabled);
        assert_eq!(status.backend, NetworkBackend::Disabled);
        assert_eq!(status.device, NetworkDevice::A2065);
        assert_eq!(status.mac_address, DEFAULT_NETWORK_MAC_ADDRESS);
        assert_eq!(status.a2065_card_mac_address, DEFAULT_NETWORK_MAC_ADDRESS);
        assert_eq!(status.counters, NetworkPacketCounters::default());
    }

    #[test]
    fn api_response_is_schema_versioned() {
        let ok = ApiResponse::ok(());
        let err = ApiResponse::<()>::err_with_code("invalid_request", String::from("bad input"));

        assert_eq!(ok.schema, API_RESPONSE_SCHEMA_ID);
        assert_eq!(ok.version, API_RESPONSE_SCHEMA_VERSION);
        assert!(ok.success);
        assert!(ok.error_code.is_none());
        assert_eq!(err.schema, API_RESPONSE_SCHEMA_ID);
        assert_eq!(err.version, API_RESPONSE_SCHEMA_VERSION);
        assert!(!err.success);
        assert_eq!(err.error_code, Some(String::from("invalid_request")));
    }

    #[test]
    fn api_endpoint_contract_lists_public_paths() {
        assert!(API_ENDPOINTS.iter().any(|endpoint| {
            endpoint.method == "GET" && endpoint.path == MACHINE_SCREENSHOT_PATH
        }));
        assert!(
            API_ENDPOINTS.iter().any(|endpoint| {
                endpoint.method == "POST" && endpoint.path == FILES_FORMAT_PATH
            })
        );
    }
}
