# Rumiga

[![CI](https://github.com/metaneutrons/rumiga/actions/workflows/ci.yml/badge.svg)](https://github.com/metaneutrons/rumiga/actions/workflows/ci.yml)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)

Rumiga is a Rust Amiga emulator under active development. macOS and Linux are
the current development hosts; the primary product target is the Seeed
reTerminal D1001 with an ESP32-P4 RISC-V processor.

The desktop emulator is functional and has repeatable A500/A1200 boot and visual
evidence. The D1001 firmware is not functional yet: its crate and platform
modules are currently scaffolding. See [Project Status](PROJECT_STATUS.md) for
the evidence-backed baseline and [Roadmap](ROADMAP.md) for the embedded plan.

## Product Goal

- Stock A500 and A1200 as release-critical profiles.
- A500+ and A600 as supported secondary profiles.
- OCS, ECS, AGA, CIA, copper, blitter, sprites, Paula, trackdisk, and Gayle IDE.
- ADF and bounded, SD-backed HDF media with safe write policies.
- Correct PAL/NTSC native output and explicit viewport/presentation controls.
- D1001 touch, USB keyboard/mouse/gamepad, built-in audio, MicroSD, and Wi-Fi.
- A2065-compatible guest networking.
- Versioned REST API, web controls, screenshots, support bundles, and evidence.

PPC/accelerator boards, third-party SCSI controllers, RTG/Picasso96, CDTV, and
CD32 are outside the first release scope.

## Current Highlights

- M68000-family interpreter with 68000 through 68040 selectable host profiles.
- A500, A500+, A600, and A1200 machine profiles.
- Progressive OCS/ECS/AGA chipset implementation.
- Paula audio, CIA timers/I/O, MFM floppy, ADF writes, and 100-800% floppy speed.
- Gayle ATA/IDE, raw/RDB HDF behavior, read-only default, snapshot, and explicit
  host writeback modes.
- Native and presentation screenshots with versioned evidence manifests.
- Regression checks for right-edge wrap, left-edge injection, crop, and stretch.
- Desktop localhost REST server and embedded Next.js static UI.
- A2065 device model with desktop SLIRP, packet counters, and optional PCAP.

These are implementation highlights, not blanket compatibility claims. The
current report has 6 passing scenarios out of 16 catalog entries; 7 need
additional legal/local media and 3 are explicitly out of scope.

## Repository Layout

```text
crates/
  m68k/                     active M68000-family CPU core
  m68000/                   legacy/reference no_std CPU crate
  rumiga-core/              Amiga machine core; not no_std yet
  rumiga-platform/          no_std platform contracts
  rumiga-platform-desktop/  desktop platform adapter
  rumiga-platform-esp/      workspace-integrated D1001 adapter scaffolding
  rumiga-api/               shared REST DTO and endpoint contracts
desktop/                    current emulator binary and localhost server
firmware/                   workspace-integrated D1001 firmware scaffolding
web/                        Next.js control UI
evidence/                   versioned scenario catalog
scripts/                    capture, parity, and report tools
```

## Development

### Requirements

- Rust 1.85.0 for the audited host baseline.
- Git.
- Node.js/npm only when changing or validating `web/`.
- User-provided Kickstart and disk images for boot evidence.

The default Cargo graph is self-contained. Its CPU differential test uses the
tracked `m68000` workspace crate and a synthetic ROM, so no private Kickstart or
sibling repository is required for that gate.

### Desktop

```sh
git config core.hooksPath .githooks
cargo build --locked --workspace
cargo test --locked --workspace
cargo run --locked -p rumiga-desktop -- --help
```

Run a stock A1200 host session:

```sh
cargo run --locked --release -p rumiga-desktop --bin rumiga-desktop -- \
  --model a1200 \
  --cpu 68020 \
  --hdf /path/to/workbench.hdf \
  /path/to/kickstart.rom
```

The desktop server listens on <http://127.0.0.1:8080> while the emulator runs.
It serves the embedded web UI and REST endpoints. File-management endpoints are
still development-only and currently contain a machine-specific storage root;
do not expose this server beyond localhost.

### Headless Evidence

```sh
cargo run --locked --release -p rumiga-desktop --bin rumiga-desktop -- \
  --model a1200 \
  --cpu 68020 \
  --capture target/evidence/a1200-local/rumiga.png \
  --capture-manifest target/evidence/a1200-local/rumiga.json \
  --capture-frames 4000 \
  --hdf /path/to/workbench.hdf \
  /path/to/kickstart.rom
```

`--capture-kind native-framebuffer` records chipset pixels before viewport and
host presentation. The default records the viewport presentation. Manifests
include model, CPU, RAM, PAL/NTSC, frame count, PC/SR, dimensions, crop/stretch,
framebuffer diagnostics, media state, and input hashes. Capture mode does not
write dirty media back to source files.

Use `--hdf-snapshot /path/to/session.hdf` when post-run HDF bytes are needed
without changing the base image.

Generate a current-revision compatibility report:

```sh
scripts/generate-compatibility-report.py \
  --current-git-only \
  --strict \
  --output target/evidence/current-report.md
```

Check Rust/TypeScript API contract parity:

```sh
scripts/check-api-dto-parity.py
```

### Web UI

The web app is a real control surface for the desktop server; it is not required
to build the Rust emulator.

```sh
cd web
npm ci
npm run lint
npm run build
npm run dev
```

Both application lockfiles are tracked. CI rejects stale Rust or npm locks;
routine updates follow the [dependency policy](DEPENDENCY_POLICY.md). Exact host
and embedded build inputs are documented in the [toolchain baseline](TOOLCHAIN.md).

### D1001 / ESP32-P4

The correct ESP-IDF Rust target is:

```text
riscv32imafc-esp-espidf
```

It is not an Xtensa target. The ESP platform and firmware are now regular
workspace packages and pass host-side checks. M0-005 pins their Rust nightly,
ESP-IDF commit, ESP Rust crates, Seeed BSP revision, linker, and flash tooling.
The packages still contain no drivers and have no ESP32-P4 build artifact, so
there is intentionally no claim that the command below works at this revision.
M0-008 and M2 establish cross-build and hardware evidence, expected to follow
this shape from the firmware directory:

```sh
env -u IDF_PATH cargo build --locked --release \
  --target riscv32imafc-esp-espidf
```

ESP-IDF 6.0.2 is tracked as the upgrade candidate. The reproducible baseline
remains IDF 5.4.2 because the current official Seeed BSP names that version and
contains component constraints below IDF 6. Promotion requires a separate
compile and D1001 HIL gate; see [Toolchain](TOOLCHAIN.md#esp-idf-6).

The implementation will use the official Seeed D1001 ESP-IDF BSP through a
narrow audited Rust FFI adapter. Emulator/product logic remains in Rust, while
vendor MIPI-DSI, touch, audio, SD/MMC, Wi-Fi, and USB services stay behind safe
platform contracts.

## Quality Baseline

Current host baseline on 2026-08-14:

| Check | Result |
| --- | --- |
| `cargo test --locked --workspace` | Pass; 452 discovered tests |
| Clippy with `-D warnings` | Pass without warnings |
| `cargo fmt --all --check` | Pass |
| Cargo/npm lockfile integrity | Pass |
| Web ESLint | Pass |
| Web production build | Pass |
| npm audit | Pass; no known vulnerabilities reported |
| ESP platform/firmware host checks | Pass; topology, pins, and strict lints only |
| ESP32-P4 firmware cross-build | Not yet available |

Do not interpret the CI badge as D1001 readiness. The current workflow does not
cross-build firmware, compile RISC-V `no_std`, or build the web app.

## Documentation

- [Project Status](PROJECT_STATUS.md): what is verified, partial, planned, and
  blocked now.
- [Roadmap](ROADMAP.md): milestones M0-M10 and release quality gates.
- [Implementation Plan](IMPLEMENTATION_PLAN.md): stable task IDs and functional
  commit sequence.
- [Architecture](ARCHITECTURE.md): current and target boundaries.
- [Toolchain](TOOLCHAIN.md): exact host/ESP pins and IDF compatibility policy.
- [Audit](AUDIT.md): prioritized findings and remediation mapping.
- [Dependency Policy](DEPENDENCY_POLICY.md): lockfiles, update cadence, review,
  and rollback rules.

## Legal Inputs

Kickstart ROMs, Workbench disks, HDFs, games, demos, screenshots derived from
private media, and packet captures are not committed. Evidence records hashes,
safe metadata, and reproduction commands without redistributing those inputs.

## Contributing

Keep commits functional and reviewable. Activate the repository hooks, map work
to an implementation task ID, add tests/evidence appropriate to the behavior,
and update `PROJECT_STATUS.md` when a verified claim changes.

The default branch currently forbids unsafe Rust. The future ESP-IDF FFI adapter
will be the only reviewed exception; emulator logic remains safe Rust.

## License

GPL-3.0-only. See [LICENSE](LICENSE).
