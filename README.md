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
- Explicit `m68k` and `rumiga-core` `std` and allocator-backed `no_std`
  profiles, including a bare-metal RISC-V release check for the stock core.
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
  rumiga-core/              Amiga machine core; std default plus no_std source profile
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

- Rust 1.97.1 for the pinned host toolchain and 1.85.0 for the declared MSRV
  check.
- Git.
- Node.js 24.19.0 and npm 11.17.0 for clean workspace builds; the desktop
  binary embeds the generated web application.
- User-provided Kickstart and disk images for boot evidence.

The default Cargo graph is self-contained. Its CPU differential test uses the
tracked `m68000` workspace crate and a synthetic ROM, so no private Kickstart or
sibling repository is required for that gate.

### Core Runtime Profiles

Desktop builds use the default `std` profile. The core source can also be
compiled and tested with its explicit allocator-backed profile:

```sh
cargo test --locked -p m68k --no-default-features --features no_std
cargo +1.85.0 check --locked -p rumiga-core --no-default-features --features no_std
cargo check --locked -p rumiga-core --no-default-features --features std
cargo clippy --locked -p rumiga-core --all-targets --no-default-features --features std -- -D warnings
cargo test --locked -p rumiga-core --no-default-features --features no_std
cargo +1.97.1 xtask ci --gate portable
```

Exactly one runtime feature is required in both crates. `std` preserves the
desktop FPU and the current background blitter worker; `no_std` keeps the stock
integer CPU path, removes the host services, and executes the immediate blitter
synchronously. CPU tracing works in both profiles because the core writes
records to an injected sink instead of creating a file. The portable gate compiles
`m68k` and the complete `rumiga-core` graph as optimized `no_std` releases for
`riscv32imafc-unknown-none-elf`; it is compile evidence, not device execution.
Within the core, portable `core`/`alloc` primitives are mandatory even for the
desktop profile; strict Clippy rejects a `std` replacement when an equivalent
portable primitive exists.

### Desktop

```sh
git config core.hooksPath .githooks
(cd web && npm ci --ignore-scripts --no-audit --no-fund && npm run build)
cargo build --locked --workspace
cargo test --locked --workspace
cargo run --locked -p rumiga-desktop -- --help
```

Run a stock A1200 host session:

```sh
cargo run --locked --release -p rumiga-desktop --bin rumiga-desktop -- \
  --model a1200 \
  --cpu 68020 \
  --storage-root /path/to/rumiga-media \
  --hdf /path/to/workbench.hdf \
  /path/to/kickstart.rom
```

The desktop server listens on <http://127.0.0.1:8080> while the emulator runs.
It serves the embedded web UI and REST endpoints. File listing, upload, delete,
and REST floppy insertion are confined to the canonical `--storage-root` path.
`RUMIGA_STORAGE_ROOT` is the fallback and `./rumiga-media` is the local-safe
default. Uploads are streamed to an atomic temporary file, reject overwrite and
unsupported extensions, and default to a 2048 MiB limit configurable with
`--upload-limit-mib`. The desktop server still has no authentication, so it
remains localhost-only.

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

Generate the public, private-media-free compatibility and test-inventory bundle:

```sh
cargo +1.97.1 xtask compatibility-evidence
```

The checksummed bundle under `target/m0-011-compatibility-evidence` classifies
every catalog scenario with stable reasons, verifies the asset-free REST/web
contract, inventories Cargo-built tests, and rejects unreviewed ignored tests.
It does not read local `target/evidence` or promote missing ROM/ADF/HDF
scenarios to passes.

Check Rust/TypeScript API contract parity:

```sh
scripts/check-api-dto-parity.py
```

### Web UI

The web app is the control surface embedded into the desktop server. Generate
`web/out` before a clean desktop or workspace build.

```sh
cd web
npm ci --ignore-scripts --no-audit --no-fund
npm run lint
npm run build
npm run dev
```

Both application lockfiles are tracked. CI rejects stale Rust or npm locks;
routine updates follow the [dependency policy](DEPENDENCY_POLICY.md). Exact host
and embedded build inputs are documented in the [toolchain baseline](TOOLCHAIN.md).

### Continuous Integration

Run the complete repository quality baseline with one command after installing
the exact tools from `toolchain/manifest.toml`:

```sh
cargo +1.97.1 xtask ci
```

It runs commit policy, lockfile, governance, host, compatibility, supply-chain,
portable Rust, and ESP32-P4 firmware gates in canonical order. Invalid commit
ranges or pull-request titles, tool-version drift, workflow drift, evidence
checksum errors, and tracked-file mutation fail closed. For diagnosis, list or
select individual gates with `cargo +1.97.1 xtask ci --list` and
`cargo +1.97.1 xtask ci --gate <name>`; only the command without `--gate`
constitutes the complete local baseline.

Pull requests and pushes to `main` invoke those same gate implementations in
parallel on pinned `ubuntu-24.04` x86_64 and `macos-15` arm64 runners. Both host
legs enforce the `rumiga-core` runtime feature matrix, Rust formatting, Clippy,
all workspace tests, warning-free documentation, web lint, and the production
web build. Separate jobs compile the current bare-metal RISC-V `no_std`
boundary and produce checksummed ESP32-P4 release evidence. Commit policy,
lockfile, governance, compatibility, supply-chain, host, portable, and firmware
jobs feed one stable `Required Quality Gate` result and publish GitHub job
summaries and evidence artifacts.

Actions are pinned to immutable revisions, credentials are not persisted, and
the workflow token is read-only. See the [continuous integration contract](CI.md)
for branch protection, reproduction, and evidence rules.

### D1001 / ESP32-P4

The correct ESP-IDF Rust target is:

```text
riscv32imafc-esp-espidf
```

It is not an Xtensa target. The ESP platform and firmware are now regular
workspace packages and pass host-side checks. M0-005 pins their Rust nightly,
ESP-IDF commit, ESP Rust crates, Seeed BSP revision, linker, and flash tooling.
The driver modules remain stubs, but the locked ESP-IDF 6.0.0 stack now produces
a verified ELF, linker map, merged image, size report, resolved configuration,
and checksum manifest. M0-008 publishes that build evidence in CI; M2 establishes
flash, boot, and peripheral evidence. Run from the repository root:

```sh
cargo +1.97.1 xtask ci --gate firmware
```

ESP-IDF 6.0.0 is the active, cross-built baseline. The separate Vellum project
also proves this SDK on D1001 hardware. ESP-IDF 6.0.2 is tracked as a patchlevel
candidate pending an upstream Rust DSI compatibility fix and fresh HIL; see
[Toolchain](TOOLCHAIN.md#esp-idf-6).

The implementation remains Rust-first. The official Seeed BSP and Vellum are
hardware and behavior references; Vellum is also an owner-authorized source for
the board implementation. Required MIPI-DSI, touch, audio, SD/MMC, Wi-Fi, and
USB services will be exposed through safe platform contracts with only the
smallest reviewed ESP-IDF FFI surface. Vellum-derived code records exact source
provenance and is distributed here under `GPL-3.0-only`; third-party inputs keep
their own license obligations.

## Quality Baseline

Current baseline on 2026-08-16:

| Check | Result |
| --- | --- |
| Cargo test inventory | Pass; 493 discovered, 4 reviewed ignored, and 489 runnable unit, integration, and documentation tests |
| Clippy with `-D warnings` | Pass without warnings |
| `cargo fmt --all --check` | Pass |
| Cargo/npm lockfile integrity | Pass |
| Web ESLint | Pass |
| Web production build | Pass |
| npm audit | Pass; no known vulnerabilities reported |
| ESP platform/firmware host checks | Pass; topology, pins, and strict lints |
| Bare-metal RISC-V boundaries | Pass locally for `m68000`, `rumiga-api`, and `rumiga-platform`; full core portability remains M1 |
| ESP32-P4 firmware evidence | Pass locally and on GitHub for locked IDF 6.0.0; checksummed artifact published by run [`31890919057`](https://github.com/metaneutrons/rumiga/actions/runs/31890919057) |
| Linux/macOS host CI | Pass on GitHub-hosted x86_64 and arm64 runners |
| Public compatibility evidence | Pass locally and on GitHub; 1 asset-free scenario passes, 12 private-media scenarios are explicitly skipped, and 3 roadmap exclusions are unsupported; artifact published by run [`31910408906`](https://github.com/metaneutrons/rumiga/actions/runs/31910408906) |
| Engineering governance evidence | Pass locally and on GitHub; 13 contracts and the M0-012 task/test/evidence traceability record validate; artifact published by run [`31933087138`](https://github.com/metaneutrons/rumiga/actions/runs/31933087138) |
| Conventional Commit policy | Pass locally and in hosted PR/main CI; one Rust parser validates hooks, raw ranges, PR titles, and merge-free history; runs [`31952285487`](https://github.com/metaneutrons/rumiga/actions/runs/31952285487) and [`31952671051`](https://github.com/metaneutrons/rumiga/actions/runs/31952671051) |
| Unified local quality command | Pass; all eight gates complete locally in 91.516 seconds and feed the hosted fail-closed aggregate |
| Protected branch gate | `Required Quality Gate` from GitHub Actions required on `main` |

Do not interpret the CI badge as D1001 runtime readiness. M0-008 proves portable
package compilation plus firmware compile, link, configuration, and image
generation. It does not prove that the stub firmware flashes, boots, drives a
peripheral, or meets performance targets; those are M2 and later gates.

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
- [Continuous Integration](CI.md): required jobs, permissions, summaries, and
  branch-protection contract.
- [Contributing](CONTRIBUTING.md): task, test, evidence, ADR, release-note, and
  pull-request workflow.
- [Reviewing](REVIEWING.md): correctness, embedded, security, and evidence
  review contract.
- [Architecture Decisions](docs/adr/README.md): numbered decision lifecycle and
  accepted history.
- [Governance Records](governance/README.md): machine-readable task
  traceability contract.
- [Vellum Reuse Authorization](docs/provenance/VELLUM_REUSE.md): owner
  authorization, exclusions, and source-transfer tracking.

## Legal Inputs

Kickstart ROMs, Workbench disks, HDFs, games, demos, screenshots derived from
private media, and packet captures are not committed. Evidence records hashes,
safe metadata, and reproduction commands without redistributing those inputs.

## Contributing

Follow [CONTRIBUTING.md](CONTRIBUTING.md) and [REVIEWING.md](REVIEWING.md).
Material work maps a stable task to tests, evidence, documentation, release
notes, and architecture decisions through a versioned change record.

The default branch currently forbids unsafe Rust. The future ESP-IDF FFI adapter
will be the only reviewed exception; emulator logic remains safe Rust.

## License

GPL-3.0-only. See [LICENSE](LICENSE).
