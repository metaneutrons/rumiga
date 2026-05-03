# rumiga

[![CI](https://github.com/metaneutrons/rumiga/actions/workflows/ci.yml/badge.svg)](https://github.com/metaneutrons/rumiga/actions/workflows/ci.yml)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)

Enterprise-grade Amiga emulator targeting the Seeed reTerminal D1001 (ESP32-P4), with a modular multi-platform architecture.

## Features

- **OCS/ECS/AGA** chipset emulation (progressive implementation)
- **Multi-platform**: Desktop (macOS/Linux) for development, ESP32-P4 for production
- **Web UI**: Next.js management interface for file management, WiFi config, and machine setup
- **On-device OSD**: Slint-based overlay in display black bars
- **SD card storage**: FAT32 for ADF/HDA disk images
- **Audio**: 4-channel Paula emulation with configurable stereo mixing
- **Input**: USB HID keyboard/gamepad + capacitive touch-as-mouse

## Architecture

```
rumiga/
├── crates/
│   ├── rumiga-core/             # no_std + alloc — emulation engine
│   ├── rumiga-platform/         # Platform trait definitions
│   ├── rumiga-platform-desktop/ # Desktop backend (minifb, cpal)
│   ├── rumiga-platform-esp/     # ESP-IDF backend (MIPI-DSI, I2S, SD)
│   └── rumiga-api/              # Shared REST API types
├── firmware/                    # ESP-IDF binary
├── desktop/                     # Desktop binary
└── web/                         # Next.js 16.x + Tailwind v4
```

## Hardware Target

- **Seeed reTerminal D1001**: ESP32-P4 @ 400MHz, 32MB PSRAM, 8" 800×1280 MIPI-DSI touch display
- WiFi 6 + BLE 5 via ESP32-C6, MicroSD, USB 2.0, I2S audio (ES8311)

## Development

```bash
# Clone
git clone https://github.com/metaneutrons/rumiga.git
cd rumiga

# Activate git hooks
git config core.hooksPath .githooks

# Build (desktop target)
cargo build --workspace

# Run tests
cargo test --workspace

# Run desktop emulator
cargo run -p rumiga-desktop
```

## License

GPL-3.0-only — see [LICENSE](LICENSE).
