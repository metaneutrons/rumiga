# Rumiga Project Status

This document is the single source of truth for delivery status. `ROADMAP.md`
defines where the product is going; `IMPLEMENTATION_PLAN.md` defines the
ordered work; this file records what is actually proven now.

## Snapshot

| Field | Value |
| --- | --- |
| Status date | 2026-08-14 |
| Audited baseline revision | `c66069059a5c` |
| Latest completed task | M0-005: pinned host and ESP build inputs |
| Development host | macOS, Apple Silicon |
| Product target | Seeed reTerminal D1001, ESP32-P4 |
| Product maturity | Desktop compatibility prototype |
| D1001 maturity | Skeleton only; no firmware or hardware evidence |
| Release readiness | Not a release candidate |

## Product Intent

Rumiga is a Rust implementation of a classic Amiga emulator. macOS and Linux
are development and reference hosts. The primary product is a self-contained
emulator on the Seeed reTerminal D1001 with:

- stock A500 and A1200 compatibility as the release-critical profiles;
- A500+ and A600 compatibility as supported secondary profiles;
- the D1001 touch display as the local control surface and mouse input;
- USB host support for keyboards, mice, and game controllers;
- Paula audio through the built-in ES8311 speaker path;
- ADF and HDF media on MicroSD with safe write policies;
- Wi-Fi through the onboard ESP32-C6 and an A2065-compatible guest NIC;
- a local REST API and web control surface;
- no bundled Kickstart ROM, Workbench media, or other copyrighted software.

Accelerator boards, PPC, third-party SCSI controllers, RTG/Picasso96, CDTV,
and CD32 are outside the first release scope.

## Status Vocabulary

- **Verified**: implementation plus repeatable evidence at the audited revision.
- **Implemented**: code and focused tests exist, but release evidence is incomplete.
- **Partial**: a useful path exists, but required behavior or proof is missing.
- **Planned**: no production implementation is present.
- **Blocked**: work cannot pass its gate until a named dependency is resolved.

No feature is called done merely because it compiled or booted once.

## Executive Assessment

### What is verified

- The Rust workspace builds and `cargo test --locked --workspace` passes on the
  audited Mac. Cargo discovers 450 Rust unit, integration, and documentation
  tests.
- The host Cargo graph contains no unpublished sibling dependency. A synthetic
  boot trace compares the active 68000 core with the tracked independent
  `m68000` implementation and frozen architectural checkpoints.
- Root Cargo and web npm lockfiles are tracked. CI, Git hooks, and evidence
  commands reject stale Rust resolution; CI verifies npm with `npm ci` and a
  high-severity advisory gate.
- ESP platform and firmware manifests are regular unpublished workspace
  packages and pass locked host checks under the same strict lint policy.
- The desktop runner supports A500, A500+, A600, and A1200 profiles and exposes
  68000 through 68040 CPU selections.
- Native and presentation screenshots, versioned manifests, support bundles,
  edge-wrap checks, and compatibility-report generation exist.
- Current evidence has 16 cataloged scenarios: 6 pass, 7 are skipped for
  missing legal/local assets, and 3 are explicitly out of scope.
- Passing evidence covers A500 Kickstart 1.3, A1200 Kickstart 3.x, A1200
  Workbench 3.1.4 ADF, A1200 HDF, A2065/SLIRP link setup, and REST/web contract
  parity.
- The web application passes ESLint and a production Next.js build.

### What is not yet true

- `rumiga-core` is not currently `no_std`; it directly uses host files,
  `std::thread`, `JoinHandle`, and `core_affinity`.
- `rumiga-platform-esp` and `firmware` are host-checkable workspace members, but
  every ESP platform module and the firmware entry point remain stubs. Their
  toolchain and SDK inputs are pinned, but there is no ESP32-P4 cross-build,
  flash, boot, display, touch, USB, audio, SD, Wi-Fi, or performance artifact.
- HDF media is loaded into one `Vec<u8>`. The local 2 GiB images cannot fit in
  the D1001's 32 MiB PSRAM. Sector-based block I/O is a release blocker.
- The A2065/SLIRP evidence proves link/configuration only. Guest TX/RX counters
  are zero, so guest TCP/IP is not yet proven.
- Full OCS/ECS/AGA compatibility is not proven by the current Workbench and
  Kickstart scenarios.
- The repository is not fully hermetic yet because ESP32-P4 cross-build
  provisioning and automation are incomplete. The host graph, application
  dependency resolution, package topology, toolchains, ESP-IDF commit, ESP Rust
  crates, and BSP revision are repository-owned or immutably pinned.

## Current Capability Matrix

| Area | Status | Evidence and limitation |
| --- | --- | --- |
| M68000 family CPU | Implemented | 68000-68040 profiles, extensive tests, and a hermetic 68000 differential boot trace; stock 68000 and 68EC020 are release targets; complete instruction/timing parity remains unproven |
| OCS | Partial | Kickstart 1.3 visual baseline passes; Workbench 1.3 and curated software corpus are missing |
| ECS | Partial | A500+/A600 profiles exist; no current A500+ or A600 release evidence |
| AGA | Partial | Kickstart and Workbench paths pass; curated AGA modes/titles are missing |
| CIA and scheduler | Partial | Boot-capable and tested; broad cycle-order differential evidence is missing |
| Floppy/trackdisk | Partial | ADF MFM path, writes, and 100-800% speed controls exist; A500 Workbench and stress matrices are incomplete |
| Gayle IDE | Partial | A1200 HDF evidence passes; in-memory whole-image design blocks the D1001 |
| Paula audio | Implemented | Core mixing exists; no objective waveform suite or D1001 playback evidence |
| Keyboard/mouse/joystick | Partial | Desktop path exists; D1001 touch and USB host paths do not |
| Native display | Verified on desktop | Edge-wrap regression checks and A1200 captures pass at the audited revision |
| Host presentation | Verified on desktop | Crop, border, aspect, and vertical presentation policy are captured in manifests |
| REST API | Partial | Desktop localhost server and shared DTOs exist; authentication, hardened file roots, and device server are missing |
| Web UI | Partial | Lint/build and static contract parity pass; browser workflow and device evidence are missing |
| A2065 networking | Partial | Device model and desktop SLIRP link exist; guest packet flow and D1001 Wi-Fi bridge are missing |
| Platform abstraction | Partial | A small `no_std` trait crate exists; contracts lack backpressure, capabilities, clock, block media, network, lifecycle, and telemetry |
| D1001 firmware | Planned | Workspace-integrated host stub with commented ESP dependencies; no target build or hardware evidence |
| Release operations | Planned | No device image, signed release, OTA rollback, HIL, SBOM, or soak evidence |

## Verification Baseline

The following commands were run during this audit:

| Check | Result | Interpretation |
| --- | --- | --- |
| `cargo metadata --locked --no-deps --format-version 1 --quiet` | Pass | Root Cargo manifest and lockfile agree |
| `cargo test --locked --workspace` | Pass | 452 workspace tests pass locally, including host builds of both ESP packages and two toolchain consistency tests |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Pass | All workspace targets pass without warnings |
| `cargo fmt --all --check` | Pass | Formatting is confined to repository-owned workspace sources |
| `cargo check --locked --manifest-path firmware/Cargo.toml` | Pass | Firmware is a valid host-side workspace build unit; this is not target evidence |
| `cargo check --locked --manifest-path crates/rumiga-platform-esp/Cargo.toml` | Pass | ESP adapter is a valid host-side workspace build unit; drivers remain stubs |
| `cargo test --locked -p rumiga-firmware --test toolchain_manifest` | Pass | Rust, Node/npm, ESP-IDF, BSP, Cargo config, and locked ESP crate pins agree |
| `npm run lint` | Pass | Web static lint baseline is green |
| `npm run build` | Pass | Next.js 16.3.1 production build is green |
| `(cd web && npm ci --ignore-scripts)` | Pass | npm manifest and tracked lockfile agree |
| `(cd web && npm audit --audit-level=high)` | Pass | No known npm vulnerabilities reported |

The CI workflow validates both lockfiles and host-builds all workspace packages,
but does not yet lint/build the web app or compile the ESP32-P4 target. A green
badge must therefore not be used as evidence of an embedded-ready product until
milestone M0 closes.

## Evidence Baseline

The current generated report at `target/evidence/current-report.md` records:

| Classification | Count |
| --- | ---: |
| Pass | 6 |
| Partial | 0 |
| Fail | 0 |
| Skipped: missing assets | 7 |
| Unsupported: out of scope | 3 |

Passing scenarios are useful regression evidence, not a percentage-complete
claim. In particular, an A1200 Workbench screen does not prove all AGA fetch,
sprite, HAM8, copper, blitter, or timing behavior.

## D1001 Hardware Baseline

The official Seeed material establishes the following target:

| Capability | Hardware |
| --- | --- |
| CPU | ESP32-P4NRW32, dual-core 32-bit RISC-V, 400 MHz |
| Memory | 32 MiB PSRAM, 32 MiB QSPI flash |
| Display | 8 inch, 800x1280, MIPI-DSI, JD9365-family controller |
| Touch | GSL3670 capacitive controller |
| Audio output | ES8311 codec and 2 W speaker amplifier |
| Storage | MicroSD through SD/MMC |
| Connectivity | ESP32-C6 over SDIO, Wi-Fi 6 and BLE 5 |
| USB | ESP32-P4 USB 2.0 host capability; exact D1001 connector/VBUS role must be qualified on the board |

The official Seeed BSP is cloned locally for analysis at revision
`5074d3b2f45626b261298e305aaf792036febc5a`. It targets ESP-IDF 5.4.2 and
contains board support for display, touch, audio, SD/MMC, and Wi-Fi. It is a
reference dependency, not part of Rumiga's current build. ESP-IDF 6.0.2 is
tracked as an upgrade candidate, but Seeed's bundled audio manifests declare
`<6.0`; promotion requires a BSP port plus compile and HIL evidence.

## Critical Risks

| ID | Severity | Risk | Required response |
| --- | --- | --- | --- |
| R-001 | Critical | Whole HDF images are resident in RAM | Introduce a bounded sector `BlockDevice` contract before A1200 device integration |
| R-003 | Critical | Core owns host threads and files | Move host services behind adapters and make the deterministic core `no_std + alloc` in M1 |
| R-004 | High | No D1001 firmware has booted | Produce a pinned firmware artifact and serial boot evidence in M0-008/M2 |
| R-005 | High | USB-C host wiring and VBUS behavior are not qualified | Verify schematic and actual board before promising direct USB-C peripherals; document required adapter/hub |
| R-006 | High | Performance on ESP32-P4 is unknown | Add cycle, frame, PSRAM bandwidth, and memory benchmarks before compatibility expansion |
| R-007 | High | API file handling is hard-coded to a developer path and device security is undefined | Add a configured storage root, canonical path checks, size limits, authentication, and local-safe defaults |
| R-008 | High | Network evidence has no guest packets | Require guest ping, DNS, HTTP checksum, and sustained transfer evidence |
| R-011 | Medium | Platform traits cannot report backpressure or capability limits | Version richer contracts before D1001 drivers are implemented |
| R-012 | Medium | Existing docs contain implemented/target-state confusion | Keep claims tied to this status file and evidence levels |

Retired risks:

| ID | Retired | Evidence |
| --- | --- | --- |
| R-009 | 2026-08-14 by M0-002 | External `../r68k` paths removed; tracked `m68000` differential fixture passes in the 450-test workspace run |
| R-010 | 2026-08-14 by M0-003 | Both application lockfiles are tracked and enforced; monthly update automation and review policy exist |
| R-002 | 2026-08-14 by M0-004 | ESP platform and firmware are locked workspace members with passing host checks; target build risk remains R-004 |

## Engineering Decisions

These decisions are binding until replaced by a reviewed architecture decision:

1. The emulator and product logic remain Rust. The initial D1001 backend may
   call the vendor ESP-IDF/Seeed C BSP through one narrow, audited FFI boundary.
   Reimplementing MIPI-DSI and the board BSP in Rust is not a release gate.
2. `rumiga-core` targets `no_std + alloc`. It does not open files, spawn tasks,
   select CPU cores, serve HTTP, or know about D1001 hardware.
3. The core has one deterministic owner. Desktop and firmware schedule it;
   platform I/O may run concurrently through bounded queues.
4. ROM, ADF, and HDF bytes are user-provided. Media defaults to read-only;
   writeback is explicit, flushable, and recoverable.
5. Native framebuffer correctness and presentation scaling are separate layers
   with separate screenshots and tests.
6. Network access is disabled by default. The first guest NIC remains A2065;
   desktop uses SLIRP and D1001 uses the ESP32-C6 host network path.
7. A feature reaches **Verified** only with repeatable artifacts from the target
   level: host evidence for desktop claims and HIL evidence for D1001 claims.

## Milestone Dashboard

Detailed gates are in `ROADMAP.md`; task IDs are in `IMPLEMENTATION_PLAN.md`.

| Milestone | Status | Promotion evidence |
| --- | --- | --- |
| BASE: Desktop evidence foundation | Verified | Six current host scenarios and versioned evidence tooling |
| M0: Hermetic engineering baseline | Active | Host graph, lockfiles, package topology, and toolchains are pinned; broader CI and target compile gates remain |
| M1: Portable deterministic core | Planned | `no_std` RISC-V compile and deterministic replay parity |
| M2: D1001 board bring-up | Planned | Flashable firmware, serial manifest, memory/display smoke |
| M3: Bounded media and memory | Planned | 2 GiB HDF boots through bounded sector cache |
| M4: D1001 display pipeline | Planned | Correct 50/60 Hz presentation and device framebuffer captures |
| M5: Touch, USB, and audio | Planned | HIL input hotplug and zero-underrun audio evidence |
| M6: A500 device alpha | Planned | Kickstart 1.3 and Workbench 1.3 usable on D1001 |
| M7: A1200 device alpha | Planned | Stock A1200 Workbench boots from ADF and HDF on D1001 |
| M8: Network and control plane | Planned | Wi-Fi, guest A2065 TCP, REST, and web workflows pass on device |
| M9: Compatibility and performance beta | Planned | Corpus, latency, memory, thermal, and soak gates pass |
| M10: Production release | Planned | Signed reproducible image, rollback, SBOM, release evidence |

## Updating This File

Update this document in the same commit whenever a milestone changes state.
Every promotion must include:

- the exact git revision;
- the command or HIL job that generated evidence;
- artifact identifiers without copyrighted bytes;
- measured values for performance or reliability gates;
- newly discovered risks and any retired risks;
- a link from the relevant task in `IMPLEMENTATION_PLAN.md`.

Do not rewrite historical evidence. Generate a new evidence set for the new
revision and retain the prior report as a release artifact when appropriate.
