# Rumiga Project Status

This document is the single source of truth for delivery status. `ROADMAP.md`
defines where the product is going; `IMPLEMENTATION_PLAN.md` defines the
ordered work; this file records what is actually proven now.

## Snapshot

| Field | Value |
| --- | --- |
| Status date | 2026-08-18 |
| Audited baseline revision | Repository revision containing this document |
| Latest completed task | M1-012: published portability contract, hosted evidence verified. M1 complete |
| Current implementation | M2-001: D1001 hardware manifest |
| Next task | M2-002: reproducible ESP-IDF/Rust firmware build |
| Development host | macOS, Apple Silicon |
| Product target | Seeed reTerminal D1001, ESP32-P4 |
| Product maturity | Desktop compatibility prototype |
| D1001 maturity | Cross-built Rust firmware skeleton; no device boot or HIL evidence |
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
  audited Mac. The Cargo-backed inventory discovers 493 Rust unit, integration,
  and documentation tests: 489 runnable and 4 reviewed ignored.
- The host Cargo graph contains no unpublished sibling dependency. A synthetic
  boot trace compares the active 68000 core with the tracked independent
  `m68000` implementation and frozen architectural checkpoints.
- Root Cargo and web npm lockfiles are tracked. CI, Git hooks, and evidence
  commands reject stale resolution; the hosted supply-chain gate enforces
  source, integrity, SPDX license, duplicate, advisory, lifecycle-script, and
  immutable-Action policy.
- GitHub Actions run
  [`31894500079`](https://github.com/metaneutrons/rumiga/actions/runs/31894500079)
  verifies 350 Rust packages, 440 npm packages, and 13 Action references with
  zero vulnerabilities and publishes the independently revalidated M0-009
  evidence artifact.
- The host CI contract uses pinned Ubuntu x86_64 and macOS arm64 runners,
  immutable action revisions, minimal token permissions, complete Rust/web
  gates, per-job summaries, and one fail-closed aggregate result.
- `cargo +1.97.1 xtask ci` is the single complete local entry point. The fully
  promoted baseline covers eight required gate categories, including commit
  history. GitHub uses the same implementations in
  parallel, repository tests reject workflow topology drift, and each gate
  rejects tool drift, tracked-file mutation, or incomplete evidence checksums.
- The public compatibility gate emits a private-media-free, checksummed
  `rumiga.public-evidence.bundle.v1` baseline: 1 asset-free REST/web scenario
  passes, 12 media scenarios are explicitly skipped, and 3 roadmap exclusions
  are unsupported. Its Cargo-backed inventory records 482 tests: 478 runnable
  and 4 reviewed ignored.
- GitHub Actions run
  [`31910408906`](https://github.com/metaneutrons/rumiga/actions/runs/31910408906)
  publishes compatibility artifact `9253512112`; its six-file archive,
  checksums, clean revision, scenario/test totals, privacy flags, and absence of
  private filesystem paths pass independent download verification.
- Repository-owned contribution, review, PR/issue, ADR, release-note, and
  change-record contracts now validate through the Rust-owned governance gate.
  GitHub Actions run
  [`31933087138`](https://github.com/metaneutrons/rumiga/actions/runs/31933087138)
  passes every required job and publishes governance artifact `9259855560`.
  Independent verification confirms its four-file archive, all checksums,
  clean revision, public scope, traceability links, and private-path exclusion.
- GitHub Actions run
  [`31899884533`](https://github.com/metaneutrons/rumiga/actions/runs/31899884533)
  passes every M0-010 gate on Linux x86_64 and macOS arm64 and publishes both
  independently revalidated evidence bundles.
- GitHub Actions run `31889431633` passes both host architectures, lockfile
  integrity, and RustSec at `b83dd51`; protected `main` requires the strict
  aggregate check through pull requests.
- ESP platform and firmware manifests are regular unpublished workspace
  packages and pass locked host checks under the same strict lint policy.
- The locked ESP-IDF 6.0.0 and esp-rs matrix produces a checksummed ESP32-P4
  evidence bundle locally and in hosted CI, with static RISC-V ELF, final map,
  merged image, bootloader, partition table, resolved configuration, and size
  report. GitHub Actions run
  [`31890919057`](https://github.com/metaneutrons/rumiga/actions/runs/31890919057)
  publishes the independently revalidated artifact.
- `m68000`, `rumiga-api`, `rumiga-platform`, `m68k`, and the complete
  `rumiga-core` graph compile for bare-metal
  `riscv32imafc-unknown-none-elf`. The stock CPU/core profile is an optimized
  `no_std` release build. Pull-request run
  [`31955508417`](https://github.com/metaneutrons/rumiga/actions/runs/31955508417)
  and final `main` run
  [`31955947410`](https://github.com/metaneutrons/rumiga/actions/runs/31955947410)
  pass the portable, host, firmware, policy, evidence, and strict aggregate
  jobs from clean revisions.
- Desktop REST media operations use a configured canonical storage root,
  bounded streaming uploads, atomic no-overwrite publication, stable errors,
  and traversal/symlink escape tests.
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
- `rumiga-core` has explicit, mutually exclusive `std` and `no_std` runtime
  profiles. `std` remains the default and preserves desktop tracing and the
  background blitter worker.
- The allocator-backed `no_std` source profile excludes core-owned files,
  threads, and CPU-affinity calls. GitHub Actions run
  [`31934749529`](https://github.com/metaneutrons/rumiga/actions/runs/31934749529)
  passes the complete dual-profile gate on Linux x86_64 and macOS arm64 plus
  the strict aggregate.
- Governance artifact `9260313104` records the clean pull-request merge
  revision and M1-001 traceability. Its archive digest and all three payload
  checksums were independently verified.
- `m68k` has explicit `std` and allocator-backed `no_std` profiles. The default
  desktop graph retains the optional FPU; the stock profile rejects FPU,
  preserves Line-F handling, and contains no `std::` source references.
- The canonical host gate validates both CPU runtime profiles, three invalid
  feature combinations, the FPU-less 68EC020 regression, and unchanged default
  workspace behavior. The canonical portable gate now builds `m68k` and
  `rumiga-core` together in release mode for bare-metal RISC-V.
- Governance artifacts `9265830160` and `9265939161` record the clean
  pull-request merge and final `main` revisions. Their archive SHA-256 digests
  (`c3c392ae6c7fe20d3e4e001b013f2b25f9fb337eb0fa9da2114ffb2d28a69208`
  and `ea3b1043d8b54a0dded2a5d61764ca77c0547398029869367c0ba04d1ef6113d`)
  and every internal payload checksum were independently verified.
- One bounded Rust parser now owns Conventional Commit syntax for the local
  `commit-msg` hook and canonical `commits` gate. It validates raw Git objects,
  event ranges, merge-free history, and pull-request titles without npm.
- All 31 `rumiga-xtask` tests, strict Clippy, the local 91.516-second eight-gate
  baseline, and the workflow topology contract pass. Pull-request run
  [`31952285487`](https://github.com/metaneutrons/rumiga/actions/runs/31952285487)
  verifies the three commits, PR title, and strict aggregate. Final `main` run
  [`31952671051`](https://github.com/metaneutrons/rumiga/actions/runs/31952671051)
  verifies the exact promoted three-commit range and strict aggregate.
- Governance artifacts `9264985708` and `9265088957` have independently
  verified archive digests, complete payload checksums, clean source revisions,
  public scope, and passing M0-013 traceability reports.

### What is not yet true

- M1-002 proves the stock CPU/core graph compiles for the bare-metal target;
  it does not prove D1001 allocator integration, target execution, or
  performance. Those remain gated by later M1 instrumentation and M2 HIL work.
- M1-003 is verified by clean pull-request and final `main` promotion evidence.
- M2-013 and M2-014 are verified by clean pull-request and final `main` promotion
  evidence. They give the device a product flash layout and an enforced
  reversibility invariant, but they prove nothing about hardware: no board has
  been flashed, no eFuse has been burned, and with virtual eFuses the encryption
  is simulated rather than enforced.
- M1-012 published the portability contract and enforced the one rule that nothing
  checked: the core dependency graph is now a closed set of exactly `m68k`, `rumiga-core`,
  and `rumiga-platform`, compared in both directions by the portable gate and pinned by the
  manifest test. That is stricter than the acceptance criterion's allowlist wording, because
  "approved `no_std`" is not stable across versions. Both comparison directions are
  probe-verified, and the lockfile gate was shown to be only a partial defence. It is verified
  by clean pull-request and final `main` evidence, with the portable job executing the graph
  comparison in CI and both host legs pinning the declaration. The contract covers
  `rumiga-core` and what it pulls in; the shell and the ESP platform crate are outside it by
  design.
- M1-011 enforced the two assumptions that separate the 64-bit hosts from the 32-bit
  device: guest values must be converted with an explicit byte order, banned in the core
  through Clippy, and `usize` must hold a guest address, asserted at compile time. Both
  were probe-verified. Miri was answered rather than adopted, because the workspace forbids
  unsafe code and neither truncation nor a wrong byte order is undefined behaviour. The
  cast audit found no production defect. Hosted evidence is pending. Execution with a
  32-bit `usize` is not claimed, and alignment is not separately instrumented. It is verified
  by clean pull-request and final `main` evidence, with all seven fixtures passing in both
  runtime profiles on both host operating systems and the assertions evaluated for the 32-bit
  target by the portable job.
- M1-010 made the core's frame loop allocation-free in steady state, measured rather than
  asserted. One minute of a real Kickstart boot allocated 978,521 times and now allocates
  nothing. The first fix was insufficient and the synthetic test passed anyway, so the
  fixture was strengthened until reverting the fix fails on the allocation count itself.
  Behaviour is unchanged, with an identical state digest and capture digest. It is verified by
  clean pull-request and final `main` evidence, with the assertion passing in both runtime
  profiles on both host operating systems. The one-minute figure is local because ROMs are not
  committed; the desktop shell's own per-frame allocations and peak resident memory are out of
  scope.
- M1-009 made input recordable and replayable against emulated frames, and widened the
  state digest to cover what replay can reach. Three replays of one recording reach the
  same state digest while a run with no input reaches a different one; all four share the
  same frame digest, because the screen under test does not react, which is why the state
  digest is separate. Two defects surfaced during the work and were fixed at the cause: a
  duplicated replay path that missed the mouse counters, and a digest that could not see a
  keystroke already consumed into CIA state. It is verified by clean pull-request and final
  `main` evidence, with every replay and digest test passing in both runtime profiles on both
  host operating systems. Replay assumes networking is disabled, carries no media reference,
  and the digest still omits the copper and blitter shadows, audio state, MFM buffers, and
  IDE transfer state.
- M1-008 bounded the queues that exist and named their overflow policy. The guest
  keyboard queue previously dropped events past sixteen with nothing recorded, and its
  bound is reached in normal use because it drains about seventeen events per second.
  Guest-visible behaviour is unchanged; the loss is now counted, reported at shutdown,
  and recorded in capture manifests. It is verified by clean pull-request and final
  `main` evidence, with the keyboard queue tests passing in both runtime profiles on both
  host operating systems. No audio or video queue is created, because neither has a
  backend, so the audio bound remains declared rather than enforced, and the counters are
  cumulative with no windowed rate.
- M1-007 versioned the platform contracts and separated typed failure from
  backpressure. A display failure was previously discarded, so a dead window looked
  like a healthy one; the shell now reports it and stops. An absent service is `None`
  in the capability descriptor and `Unsupported` when called anyway. It is verified by
  clean pull-request and final `main` evidence, with the contract tests passing on both
  host operating systems and the new types compiling for bare-metal RISC-V.
  `AudioOutput` and `Storage` still have no backend, the bound that
  `AudioCapabilities::max_queued_frames` describes is not enforced yet, and
  capabilities are not published over REST or serial.
- M1-013 made the video standard selectable. An NTSC machine runs 262 lines at
  3,579,545 Hz with a 243-line active height, and the guest detects the standard: under
  `--ntsc`, Kickstart 46.143 sets a display window from line 21 to line 262 against
  PAL's line 29 to line 312. The flag was previously inert and produced byte-identical
  output to PAL. PAL rendering is byte-identical before and after the change. It is
  verified by clean pull-request and final `main` evidence, with every constant test and
  every standard-related emulator test passing twice per host leg, once per runtime
  profile. The Agnus revision is still reported as OCS on every profile,
  interlace and long/short frame alternation are not modelled, and nothing has been
  diffed against `WinUAE` or FS-UAE output.
- M1-005 removed the threaded blitter, so no `std::thread`, `JoinHandle`, or
  `core_affinity` remains in the core and both runtime profiles reach an identical
  pinned fixture digest. It also closed three defects that the thread had hidden:
  the blitter interrupt was never raised under `no_std`, the guest-visible BBUSY bit
  reported host thread state, and a state digest taken during a blit read an empty
  chip RAM slice. It is verified by clean pull-request and final `main` evidence, with the pinned digest confirmed on both host operating systems.
- M1-006 moved host time into a platform `Clock` contract owned by the shell, so the
  core declares an emulated frame period derived from the colour clock and cannot name
  a host clock type in either runtime profile. It is verified by clean pull-request and
  final `main` evidence, with the four contract tests and the frame period test passing
  on both host operating systems. Whether the desktop sustains the paced rate under
  load is not measured; the loop requests the correct period and reports what it
  achieves.
- M1-004 is verified by clean pull-request and final `main` promotion evidence.
  It removed core-owned trace files, so CPU tracing now runs through an injected
  sink in both runtime profiles, and a differential capture proves the desktop
  trace bytes are unchanged. `std::thread`, `JoinHandle`, and `core_affinity`
  no longer exist in either core profile; M1-005 removed them.
- `rumiga-platform-esp` and `firmware` are host-checkable workspace members, but
  every ESP platform module and the firmware entry point remain stubs. Their
  toolchain and SDK inputs now cross-build, but there is no flash, boot,
  display, touch, USB, audio, SD, Wi-Fi, or performance HIL artifact.
- HDF media is loaded into one `Vec<u8>`. The local 2 GiB images cannot fit in
  the D1001's 32 MiB PSRAM. Sector-based block I/O is a release blocker.
- The A2065/SLIRP evidence proves link/configuration only. Guest TX/RX counters
  are zero, so guest TCP/IP is not yet proven.
- Full OCS/ECS/AGA compatibility is not proven by the current Workbench and
  Kickstart scenarios.
- The 32 MB physical flash is intentionally configured as a conservative 16 MB
  firmware geometry matching the pinned Seeed and Vellum baselines. Accessing
  the upper half is not qualified without a D1001 HIL flash/boot test.

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
| REST API | Partial | Desktop localhost server, shared DTOs, and sandboxed media storage exist; authentication, browser workflows, and the device server are missing |
| Web UI | Partial | Lint/build and static contract parity pass; browser workflow and device evidence are missing |
| A2065 networking | Partial | Device model and desktop SLIRP link exist; guest packet flow and D1001 Wi-Fi bridge are missing |
| Core portability | Partial | M1-002 verifies explicit CPU/core profiles and the complete stock graph as a bare-metal RISC-V release; deterministic replay, allocation bounds, and removal of canonical host dependencies remain open |
| Platform abstraction | Partial | A small `no_std` trait crate exists; contracts lack backpressure, capabilities, clock, block media, network, lifecycle, and telemetry |
| D1001 firmware | Partial | Locked IDF 6.0.0 build produces a validated, checksummed release bundle; firmware services are stubs and no hardware evidence exists |
| Release operations | Planned | No device image, signed release, OTA rollback, HIL, SBOM, or soak evidence |

## Verification Baseline

The following commands were run during this audit:

| Check | Result | Interpretation |
| --- | --- | --- |
| `cargo metadata --locked --no-deps --format-version 1 --quiet` | Pass | Root Cargo manifest and lockfile agree |
| `cargo test --locked --workspace` | Pass | 493 tests are discovered: 489 runnable and 4 reviewed ignored, including quality-orchestrator, storage-confinement, ESP-pin, and evidence tooling |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Pass | All workspace targets pass without warnings |
| `cargo check --locked -p rumiga-core --no-default-features --features std` | Pass | The desktop runtime profile is independently selectable |
| `cargo test --locked -p rumiga-core --no-default-features --features no_std` | Pass | The core source profile passes 145 unit tests plus all applicable integration and golden-vector suites without its `std` feature |
| Invalid core feature selections | Pass | Neither and both runtime profiles fail with one stable compile-time diagnostic |
| `cargo test --locked -p m68k --no-default-features --features no_std` | Pass | Eight CPU tests plus the doctest pass; an FPU opcode on stock 68EC020 remains a Line-F trap |
| Invalid CPU feature selections | Pass | Missing, conflicting, and `no_std,fpu` selections fail with their stable single diagnostics |
| GitHub Actions run `31934749529` | Pass | Linux x86_64, macOS arm64, every supporting gate, and the strict aggregate validate M1-001 from a clean pull-request merge revision |
| `cargo fmt --all --check` | Pass | Formatting is confined to repository-owned workspace sources |
| `cargo check --locked --manifest-path firmware/Cargo.toml` | Pass | Firmware is a valid host-side workspace build unit; this is not target evidence |
| `cargo check --locked --manifest-path crates/rumiga-platform-esp/Cargo.toml` | Pass | ESP adapter is a valid host-side workspace build unit; drivers remain stubs |
| `cargo test --locked -p rumiga-firmware --test toolchain_manifest` | Pass | Rust, Node/npm, ESP-IDF, BSP, Cargo config, and locked ESP crate pins agree |
| Bare-metal RISC-V package check | Pass | Foundation packages plus `m68k` and complete `rumiga-core` compile for `riscv32imafc-unknown-none-elf`; stock core uses `no_std` release mode |
| `cargo +1.97.1 xtask firmware-evidence` | Pass | IDF 6.0.0 firmware compile, link, board configuration, image generation, and all artifact checksums pass locally; this is not boot evidence |
| GitHub Actions run `31890919057` | Pass | Portable RISC-V and ESP32-P4 jobs pass; artifact `9248602076` contains the checksummed firmware bundle built from a clean pull-request merge revision |
| `npm run lint` | Pass | Web static lint baseline is green |
| `npm run build` | Pass | Next.js 16.3.1 production build is green |
| `(cd web && npm ci --ignore-scripts)` | Pass | npm manifest and tracked lockfile agree |
| `(cd web && npm audit --audit-level=high)` | Pass | No known npm vulnerabilities reported |
| `actionlint .github/workflows/ci.yml` | Pass | Workflow syntax, matrix expressions, and action inputs are structurally valid |
| `cargo +1.97.1 xtask ci --gate commits` | Pass | Local, hosted PR/title, and final `main` ranges satisfy the shared Conventional Commit policy |
| `cargo +1.97.1 xtask ci` | Pass | The complete eight-gate local M1-002 baseline is green in 95.478 seconds, including the optimized stock-core RISC-V and ESP32-P4 release builds |
| GitHub Actions run `31952285487` | Pass | PR commits, title, all required jobs, and strict aggregate validate from a clean pull-request merge revision |
| GitHub Actions run `31952671051` | Pass | The final three-commit `main` push range, all required jobs, and strict aggregate validate from a clean checkout |
| GitHub Actions run `31955508417` | Pass | M1-002 pull-request commits/title and all ten final-attempt jobs pass; the portable job builds `m68k` plus `rumiga-core` as optimized bare-metal RISC-V releases |
| GitHub Actions run `31955947410` | Pass | The exact promoted three-commit range, both host systems, portable and ESP32-P4 builds, evidence jobs, and required aggregate pass on `main` |
| GitHub Actions run `31899884533` | Pass | The same named gate implementations pass on hosted Linux x86_64 and macOS arm64 and feed the strict aggregate |

The CI workflow validates both lockfiles, the complete host Rust/web matrix, the
current portable Rust boundary, and ESP32-P4 firmware evidence through the same
repository-owned gates as the local command. A green badge still proves no
D1001 runtime behavior; flash, boot, and peripherals require HIL.

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
`5074d3b2f45626b261298e305aaf792036febc5a`. It targets ESP-IDF 5.4.2 and is a
hardware reference, not part of Rumiga's build. Vellum revision
`15bff64d316c3751861d02fcf7ace6b47afab176` independently proves ESP-IDF 6.0.0
bring-up on the D1001. Rumiga now cross-builds on that baseline. The copyright
holder has authorized reuse of their Vellum board code in Rumiga under
`GPL-3.0-only`; device services may selectively port that proven implementation
through Rust-first adapters with transfer provenance, third-party license
review, and Rumiga HIL evidence.

Both the pinned Seeed BSP and Vellum select a 16 MB firmware flash geometry even
though the board physically exposes 32 MB. Rumiga preserves that proven baseline
until an explicit HIL test qualifies the larger geometry.

The flash layout is repository-owned in `firmware/partitions.csv`: 320 KiB `nvs`,
4 KiB `nvs_keys`, 8 KiB `otadata`, 4 KiB `phy_init`, 108 KiB `coredump`, two 6 MiB
application slots, and `storage` last. Because the variable-size partition is
last, both slots keep identical offsets on either geometry, so qualifying the
upper 16 MB only extends `storage` from 3.5 MiB to 19.5 MiB. The partition table
sits at `0x10000`, which gives the bootloader a 57,344-byte window; the stock
`0x8000` offset left 480 bytes and could not hold a Secure Boot V2 signature
block. Secure Boot is reserved rather than enabled, because signed binaries
require a private key that must stay out of the repository and the evidence
bundle.

Flash encryption is exercised in Development mode with virtual eFuses, so no board
that boots this firmware is permanently altered. That is enforced rather than
documented: the firmware gate rejects flash encryption or Secure Boot without
virtual eFuses, release-mode flash encryption, and HMAC-based NVS encryption,
because each burns an eFuse that cannot be cleared. With virtual eFuses the
encryption is simulated, so no confidentiality claim follows; the manifest records
this through the `encryption-not-enforced` exclusion.

## Critical Risks

| ID | Severity | Risk | Required response |
| --- | --- | --- | --- |
| R-001 | Critical | Whole HDF images are resident in RAM | Introduce a bounded sector `BlockDevice` contract before A1200 device integration |
| R-003 | Low | The core owns no host threads, files, CPU affinity, or clock, and a lint prevents a host clock type from reappearing. Remaining M1 work covers bounded queues, replay, and allocation bounds | Complete M1-007 through M1-012 |
| R-004 | High | No D1001 firmware has booted | The pinned M0-008 build artifact is published; capture serial boot evidence in M2 |
| R-005 | High | USB-C host wiring and VBUS behavior are not qualified | Verify schematic and actual board before promising direct USB-C peripherals; document required adapter/hub |
| R-006 | High | Performance on ESP32-P4 is unknown | Add cycle, frame, PSRAM bandwidth, and memory benchmarks before compatibility expansion |
| R-007 | High | Device API authentication, authorization, CSRF, and provisioning security are undefined | Preserve the completed desktop storage sandbox and close the remote threat model in M8-005 through M8-007 |
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

1. The emulator and product logic remain Rust. D1001 services use maintained
   Rust APIs first and may call ESP-IDF through narrow audited FFI boundaries.
   Seeed remains a hardware reference. Vellum is hardware evidence and an
   owner-authorized implementation source, but reuse stays selective;
   provenance and third-party license review are release gates.
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
| M0: Hermetic engineering baseline | Verified | All fifteen M0 tasks pass local, pull-request, and final `main` promotion evidence |
| M1: Portable deterministic core | Active | M1-001 through M1-004 are verified; G1 remains open |
| M2: D1001 board bring-up | Active | M2-013 and M2-014 are verified; flashable firmware, serial manifest, and memory/display smoke remain |
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
