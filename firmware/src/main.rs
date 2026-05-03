// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Rumiga firmware entry point for the reTerminal D1001 (ESP32-P4).
//!
//! Boot sequence:
//! 1. Initialize ESP-IDF system (PSRAM, clocks, peripherals)
//! 2. Mount SD card (FAT32) and load Kickstart ROM
//! 3. Initialize MIPI-DSI display and show boot logo
//! 4. Initialize I2S audio (ES8311 codec)
//! 5. Start WiFi (SoftAP or client mode)
//! 6. Start REST API server (axum/tokio)
//! 7. Initialize input (touch + USB HID)
//! 8. Create emulator instance and enter main loop

fn main() {
    // TODO: implement when ESP-IDF toolchain is configured
}
