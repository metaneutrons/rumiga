# rumiga

[![CI](https://github.com/metaneutrons/rumiga/actions/workflows/ci.yml/badge.svg)](https://github.com/metaneutrons/rumiga/actions/workflows/ci.yml)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)

Enterprise-grade Amiga emulator targeting the Seeed reTerminal D1001 (ESP32-P4), with a modular multi-platform architecture.

## Features

- **OCS/ECS/AGA** chipset emulation (progressive implementation)
- **Motorola 68000-family** CPU emulation with 68000/68010/68020/68030/68040 profiles
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
│   ├── m68k/                    # Motorola 68000-family CPU emulation
│   ├── m68000/                  # Legacy 68000 CPU comparison/reference crate
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
│   │   ├── ide.rs              # Gayle ATA/IDE controller
│   │   ├── memory.rs           # Memory subsystem (Chip/Fast/ROM/Gayle)
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

# Show desktop emulator options
cargo run -p rumiga-desktop -- --help

# Run desktop emulator
cargo run -p rumiga-desktop -- --model a1200 --cpu 68020 --hdf workbench.hdf <kickstart.rom> [df0.adf]

# Freeze a headless screenshot and evidence manifest
cargo run -p rumiga-desktop -- \
  --model a1200 \
  --capture target/evidence/a1200/workbench.png \
  --capture-frames 1200 \
  --hdf workbench.hdf \
  <kickstart.rom>
```

`--capture` runs without opening a window, saves the same RGB565 framebuffer
presentation used by the desktop viewport path, and writes a sibling JSON
manifest by default. The manifest records model, CPU, RAM, frame count, PC/SR,
viewport crop/stretch settings, framebuffer statistics, floppy controller state,
and SHA-256 hashes for the ROM and mounted media. Capture mode does not write
dirty floppy or HDF buffers back to the source files.

For external reference captures on macOS, FS-UAE already has screenshot support
via `screenshots_output_dir`, `screenshots_output_prefix`, and
`screenshots_output_mask`. Rumiga evidence should come from Rumiga first; FS-UAE
is the comparison oracle when validating A1200 viewport or boot behavior.

Generate a local compatibility report from evidence manifests:

```bash
scripts/generate-compatibility-report.py \
  --evidence-root target/evidence \
  --output target/evidence/compatibility-report.md
```

The desktop REST API also exposes `GET /api/machine/support-bundle` for a
redacted JSON support snapshot. It includes current status, display settings,
network counters, screenshot metadata, and media file names, but not ROM, HDF,
ADF, screenshot, or packet-capture bytes.

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

- M68000-family CPU: 68000/68010/68020/68030/68040 profiles, disassembler, tracing
- Memory subsystem: Chip RAM, Fast RAM, ROM mapping with configurable sizes
- A1200 baseline: 2MB Chip RAM profile, 32-bit CPU address bus, PCMCIA/Gayle ranges
- Custom chipset registers: address decoding and read/write dispatch
- Copper coprocessor: MOVE, WAIT, SKIP instructions
- Blitter: all 256 minterms, line draw mode, fill mode
- Playfield: dual-playfield, bitplane-to-chunky conversion, HAM6/HAM8, AGA palette banking
- Sprite engine: 8 hardware sprites with attach mode
- Paula audio: 4-channel DMA with period/volume, stereo mixing
- CIA-A/B: timers, TOD clock, keyboard handshake, disk control
- Floppy controller: MFM decode, ADF read/write, step/seek, configurable speed
- Gayle IDE: in-memory HDF mount, ATA identify/read/write sectors
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
- Full AGA chipset coverage
- Desktop/ESP REST backend serving the Web UI
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
