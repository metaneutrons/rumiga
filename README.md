# rumiga

[![CI](https://github.com/metaneutrons/rumiga/actions/workflows/ci.yml/badge.svg)](https://github.com/metaneutrons/rumiga/actions/workflows/ci.yml)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)

Enterprise-grade Amiga emulator targeting the Seeed reTerminal D1001 (ESP32-P4), with a modular multi-platform architecture.

## Features

- **OCS/ECS/AGA** chipset emulation (progressive implementation)
- **Motorola 68000** CPU with full instruction set, disassembler, and assembler
- **Multi-platform**: Desktop (macOS/Linux) for development, ESP32-P4 for production
- **Web UI**: Next.js management interface for file management, WiFi config, and machine setup
- **On-device OSD**: Slint-based overlay in display black bars
- **SD card storage**: FAT32 for ADF/HDA disk images
- **Audio**: 4-channel Paula emulation with configurable stereo mixing
- **Input**: USB HID keyboard/gamepad + capacitive touch-as-mouse

## Project Structure

```
rumiga/
├── crates/
│   ├── m68000/                  # Motorola 68000 CPU emulation
│   ├── rumiga-core/             # no_std + alloc — emulation engine
│   │   ├── audio.rs            # Paula 4-channel audio
│   │   ├── blitter.rs          # Blitter DMA engine
│   │   ├── chipset.rs          # Custom chip register state
│   │   ├── cia.rs              # CIA-A/B timers and I/O
│   │   ├── copper.rs           # Copper coprocessor
│   │   ├── custom.rs           # Custom register address decoding
│   │   ├── emulator.rs         # Main emulation loop
│   │   ├── events.rs           # Cycle-accurate event scheduler
│   │   ├── floppy.rs           # Floppy disk controller
│   │   ├── memory.rs           # Memory subsystem (Chip/Fast/ROM)
│   │   ├── playfield.rs        # Bitplane-to-pixel rendering
│   │   └── sprites.rs          # Hardware sprite engine
│   ├── rumiga-platform/         # Platform trait definitions
│   ├── rumiga-platform-desktop/ # Desktop backend (minifb, cpal)
│   ├── rumiga-platform-esp/     # ESP-IDF backend (MIPI-DSI, I2S, SD, OSD)
│   └── rumiga-api/              # Shared REST API types (OpenAPI)
├── desktop/                     # Desktop binary
├── firmware/                    # ESP-IDF binary
└── web/                         # Next.js 16.x + Tailwind v4
```

## Hardware Target

- **Seeed reTerminal D1001**: ESP32-P4 @ 400 MHz, 32 MB PSRAM, 8" 800×1280 MIPI-DSI touch display
- WiFi 6 + BLE 5 via ESP32-C6, MicroSD, USB 2.0, I2S audio (ES8311)

## Build Instructions

### Prerequisites

- Rust 1.85+ (edition 2024)
- Git

### Desktop Target

```bash
git clone https://github.com/metaneutrons/rumiga.git
cd rumiga
git config core.hooksPath .githooks

# Build all workspace crates
cargo build --workspace

# Run tests
cargo test --workspace

# Run desktop emulator
cargo run -p rumiga-desktop -- [--model a500|a500-plus|a600|a1200] [--scale 1|2|4|8|16|32] [--viewport auto|raw] [--no-vertical-stretch] [--floppy-speed 100|200|400|800|turbo] <kickstart.rom> [df0.adf] [df1.adf] [df2.adf] [df3.adf]
```

### ESP-IDF Target

Prerequisites:
- [ESP-IDF v5.4+](https://docs.espressif.com/projects/esp-idf/en/latest/esp32p4/get-started/)
- `espup` toolchain installer
- ESP32-P4 target support (`xtensa-esp32p4-none-elf`)

```bash
# Install ESP-IDF toolchain
cargo install espup
espup install

# Build firmware (from project root)
cd firmware
cargo build --release --target xtensa-esp32p4-none-elf

# Flash to device
espflash flash target/xtensa-esp32p4-none-elf/release/rumiga-firmware
```

### Web UI Development

```bash
cd web
npm install
npm run dev    # http://localhost:3000
npm run build  # Production build
```

The web UI connects to the device REST API for:
- File management (upload/download ADF/HDA images)
- WiFi configuration
- Machine state control (pause, reset, eject)

## Current Status

### Implemented

- M68000 CPU: full instruction set, interpreter, disassembler, assembler
- Memory subsystem: Chip RAM, Fast RAM, ROM mapping with configurable sizes
- Custom chipset registers: address decoding and read/write dispatch
- Copper coprocessor: MOVE, WAIT, SKIP instructions
- Blitter: all 256 minterms, line draw mode, fill mode
- Playfield: dual-playfield, bitplane-to-chunky conversion, HAM
- Sprite engine: 8 hardware sprites with attach mode
- Paula audio: 4-channel DMA with period/volume, stereo mixing
- CIA-A/B: timers, TOD clock, keyboard handshake, disk control
- Floppy controller: MFM decode, ADF read/write, step/seek
- Event scheduler: cycle-accurate timing with priority queue
- Emulation loop: frame-based execution with video/audio sync
- Platform traits: video, audio, input, storage abstractions
- Desktop backend: minifb window, keyboard/mouse input
- ESP platform stubs: display, audio, input, storage, WiFi, API, OSD
- REST API types: OpenAPI-derived request/response types
- Web UI: Next.js dashboard with file manager, WiFi config, machine control
- CI/CD: GitHub Actions (fmt, clippy, test, audit)

### Not Yet Implemented

- Slint OSD rendering (stub only)
- ESP-IDF hardware drivers (stubs only)
- AGA chipset extensions
- Hard drive (HDA) emulation
- Network stack on ESP32

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feat/my-feature`
3. Activate git hooks: `git config core.hooksPath .githooks`
4. Make changes, ensuring `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` passes
5. Commit using [conventional commits](https://www.conventionalcommits.org/): `feat(core): add copper SKIP instruction`
6. Open a pull request against `main`

### Code Style

- Rust edition 2024, `#![forbid(unsafe_code)]`
- Clippy: `deny` for `all`, `pedantic`, `nursery`, `cargo`
- `no_std` + `alloc` for core/platform crates
- All public items documented with `///` doc comments

## License

GPL-3.0-only — see [LICENSE](LICENSE).
