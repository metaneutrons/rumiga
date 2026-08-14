# Rumiga Repository Audit

Audit date: 2026-08-12

Audited revision: `c66069059a5c`

Target: Seeed reTerminal D1001 / ESP32-P4

This audit separates code that exists from behavior that has been proven. The
current delivery state is maintained in `PROJECT_STATUS.md`; remediation tasks
are tracked by stable IDs in `IMPLEMENTATION_PLAN.md`.

## Overall Verdict

Rumiga is a substantial desktop emulator prototype with unusually useful visual
and boot evidence. It is not yet a platform-independent embedded emulator. The
D1001 path consists of stubs, the core has direct host dependencies, and HDF
ownership is incompatible with the target memory budget.

The highest-value next action is not another chipset feature. It is M0: make the
repository hermetic, buildable, and cross-target checked. After that, portable
core work and D1001 board bring-up can proceed in parallel.

## Audit Method

The audit covered:

- workspace metadata and dependency graph;
- Rust source and platform boundaries;
- desktop CLI, REST server, web UI, and evidence tools;
- ESP platform crate, firmware crate, and build configuration;
- CI and local git hooks;
- current generated evidence report;
- official Seeed D1001 documentation and BSP revision
  `5074d3b2f45626b261298e305aaf792036febc5a`;
- official ESP32-P4 and Rust ESP target documentation.

Local verification results:

- `cargo test --workspace`: pass, 450 discovered tests.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass, with 3,013
  warnings emitted by the sibling legacy `r68k` dependency.
- `cargo fmt --all --check`: fail because rustfmt traverses the unformatted
  sibling `../r68k` source.
- web ESLint and production build: pass.
- standalone firmware and ESP platform checks: fail at workspace discovery.

## Findings

### P0: Release-blocking

#### A-001: ESP firmware is not a build unit

`firmware` and `crates/rumiga-platform-esp` are inside the repository but absent
from the root workspace. Cargo rejects standalone checks because each package
believes it belongs to the parent workspace. The ESP dependencies are commented
out and `firmware/src/main.rs` has no implementation.

Impact: no cross-compile, firmware artifact, flash, boot, or CI evidence exists.

Remediation: M0-004, M0-005, M0-008, then M2.

Resolution update (2026-08-14): M0-004 makes both packages unpublished members
of the root workspace with inherited metadata, dependencies, and strict lints.
Their locked host checks now pass. ESP-IDF toolchain pinning, ESP32-P4
cross-compilation, drivers, firmware artifacts, and hardware boot evidence
remain open under M0-005, M0-008, and M2.

#### A-002: HDF design cannot fit the D1001

`crates/rumiga-core/src/ide.rs` stores the entire HDF in
`Option<Vec<u8>>`; desktop startup reads the whole image before insertion and
snapshot/writeback clones or writes the resident data. Local HDF assets are 2
GiB each, while the target has 32 MiB PSRAM.

Impact: A1200 HDF support cannot run on the target regardless of CPU speed.

Remediation: M3-001 through M3-009. Do not attempt an embedded HDF boot before
the block-device conversion.

#### A-003: The emulator core is not portable

`rumiga-core` has no `#![no_std]` boundary and directly imports host file I/O,
thread handles, thread spawning, memory drains under `std`, and
`core_affinity`. CPU tracing creates files from the core and blitter execution
can move chip RAM into a host thread.

Impact: firmware cannot own scheduling deterministically, RISC-V cross-build is
blocked, and the evidence path can differ from the optimized path.

Remediation: M1-001 through M1-012.

#### A-004: The repository is not hermetic

Desktop development tests depend on `../r68k/emu` and `../r68k/common`. Those
paths are neither workspace content nor provisioned by CI. Formatting also
operates on that sibling source, producing a 22,000-line diff on this machine.

Impact: a clean clone cannot reproduce local validation; the CI definition is
not credible as a release gate.

Remediation: M0-002, M0-007, M0-010.

Resolution update (2026-08-14): M0-002 removed both external path
dependencies and replaced the non-asserting private-ROM comparison with a
tracked `m68000` differential test plus frozen checkpoints. Cargo metadata,
formatting, strict Clippy, and all 450 workspace tests now pass without sibling
repositories. CI parity remains tracked by M0-007 and M0-010.

### P1: High priority

#### A-005: Platform contracts are too weak for real-time hardware

`rumiga-platform` is `no_std`, but its traits are fire-and-forget. Video and
audio methods cannot report backpressure, capabilities, DMA ownership, sample
rate, buffer limits, or recoverable errors. There are no clock, lifecycle,
block-device, network-link, power, logging, or telemetry contracts.

Impact: a D1001 implementation would either hide failures or leak ESP details
through the core.

Remediation: M1-006 through M1-008 before production drivers are integrated.

#### A-006: D1001 hardware assumptions have no HIL proof

The repository names MIPI-DSI, I2S, SD, touch, Wi-Fi, and USB, but every ESP
module is a TODO. Even USB-C host feasibility is not qualified against the board
connector, VBUS switching, and required adapter/hub.

Impact: product requirements may depend on wiring or power behavior not exposed
by the enclosure/board.

Remediation: M2-001 through M2-012. Board evidence is mandatory before making a
USB-C input compatibility claim.

#### A-007: Device performance and memory are unknown

There is no ESP32-P4 benchmark, map file, runtime high-water mark, queue depth,
frame-time distribution, SD latency, PSRAM bandwidth, audio underrun count,
temperature, or watchdog evidence.

Impact: A500 or A1200 real-time delivery cannot be forecast reliably.

Remediation: instrument in M1/M2; enforce budgets in M3, M6, M7, and M9.

#### A-008: Network proof stops before guest traffic

The A2065 model, SLIRP backend, API controls, and link evidence are valuable,
but the current passing manifest records `tx=0` and `rx=0`. The guest TCP
scenario remains skipped.

Impact: driver detection, interrupts, descriptor rings, and real traffic are not
proven end to end.

Remediation: M8-002 through M8-004 with a local deterministic network fixture.

#### A-009: REST file handling is development-only

Desktop file endpoints use the hard-coded path
`/Volumes/Dev/Source/rumiga`. Upload buffers the complete multipart field in
memory, filename sanitization is ad hoc, capacity values are synthetic, and the
device authentication/authorization model is undefined. The desktop server is
limited to `127.0.0.1`, which reduces current exposure but does not solve the
device design.

Impact: behavior is non-portable and cannot be exposed on Wi-Fi safely.

Remediation: M0-006 for desktop correctness; M8-005 through M8-007 for device
security.

#### A-010: No hardware evidence pipeline exists

The evidence framework is host-oriented. There is no flash automation, serial
protocol, framebuffer extraction, USB HID injection, audio loopback, power
cycle, SD fault, or multi-board soak runner.

Impact: embedded claims would rely on manual observation and cannot gate a
release.

Remediation: M2-005, M2-012, M4-006, M5-008, M9-006, M10-008.

### P2: Important engineering debt

#### A-011: Dependency versions are not reproducible

At the audited revision, the root ignored `Cargo.lock`. The web app tracked
`package-lock.json`, but the documented workflow used `npm install` instead of
locked `npm ci`. The Next.js build also reported a Node API deprecation warning.

Impact: local, CI, and release builds can resolve different dependency graphs.

Remediation: M0-003 and M0-005.

Resolution update (2026-08-14): M0-003 tracks the root `Cargo.lock`, enforces
locked Cargo resolution and npm `ci` in CI, and defines monthly update, review,
exception, and rollback rules in `DEPENDENCY_POLICY.md`. Exact toolchain and BSP
pinning remains M0-005.

#### A-012: CI covers only the default desktop workspace

The workflow checks Rust formatting, Clippy, tests, and advisories. It does not
build web, firmware, RISC-V `no_std`, evidence schemas, or release artifacts.
At the audited revision it also could not reconstruct the local sibling
dependency layout; M0-002 has since removed that specific blocker.

Impact: CI does not test the actual product target.

Remediation: M0-007 through M0-011.

#### A-013: Compatibility evidence is narrow

Six scenarios pass, seven require additional local assets, and three are
excluded. There is no complete Workbench 1.3, A500+, A600, curated OCS/ECS/AGA,
audio, input, or guest TCP evidence set. Current visual evidence is strong for
the exact captures but cannot imply broad chipset parity.

Impact: compatibility claims can outrun proof.

Remediation: M6, M7, M8, and M9 with exact scenario milestones.

#### A-014: Documentation mixed present and target states

Prior documentation called the core `no_std`, listed a nonexistent Xtensa
ESP32-P4 target, described ESP stubs as features, claimed an OSD implementation,
and said the REST server was missing even though the desktop server now exists.

Impact: developers can choose the wrong toolchain and users cannot tell what is
functional.

Remediation: M0-001. Future claims follow `PROJECT_STATUS.md` evidence rules.

#### A-015: Desktop product code is concentrated in one binary file

`desktop/src/main.rs` combines CLI parsing, REST handlers, static serving,
presentation, evidence generation, storage policy, and the emulation loop.

Impact: ownership boundaries are hard to test independently and device reuse is
more difficult.

Remediation: after M0, extract host-neutral orchestration and desktop adapters
only when each extraction supports M1/M8. Avoid a cosmetic rewrite.

## Strengths to Preserve

- Safe Rust policy across the current workspace.
- Broad CPU and chipset unit coverage.
- Explicit A500/A500+/A600/A1200 machine profiles.
- Read-only-by-default media behavior and evidence snapshots.
- Native-versus-presentation display separation.
- Captured edge-wrap diagnostics for the previously observed viewport bug.
- Versioned manifests, support bundle redaction, and compatibility catalog.
- API/web contract parity checks.
- Network disabled by default and SLIRP as the safe desktop backend.
- Functional commit discipline visible in repository history.

## Recommended Technical Strategy

1. Close M0 and prove a clean checkout.
2. Run M1 core portability and M2 D1001 service smokes in parallel.
3. Convert storage before attempting an integrated A1200 device boot.
4. Prove display, USB/touch, and audio independently with HIL.
5. Integrate A500 first to establish real-time scheduling and fault handling.
6. Integrate stock A1200 with bounded HDF access.
7. Add device Wi-Fi, guest packet evidence, REST, and web workflows.
8. Expand compatibility only after device performance budgets are measured.
9. Treat production security, recovery, and multi-board soak as release work,
   not post-release cleanup.

## Audit Closure Criteria

This audit can be superseded when:

- all P0 findings are closed with evidence;
- each remaining finding has a task, severity, and milestone;
- the D1001 firmware has an E5 board-service evidence pack;
- `PROJECT_STATUS.md` reflects the new revision and measured state.
