# Rumiga Implementation Plan

This is the executable backlog for `ROADMAP.md`. Milestone status is summarized
in `PROJECT_STATUS.md`; task status lives here.

## Status Legend

- `DONE`: merged and backed by the named evidence.
- `NEXT`: highest-priority unblocked work.
- `ACTIVE`: implementation is currently in progress.
- `PLANNED`: accepted but not started.
- `BLOCKED`: cannot proceed until the named dependency is resolved.

Task IDs are stable. Renaming a task must not change its ID. A task may move to
`DONE` only in the same commit that adds its required tests/evidence or links to
an immutable artifact produced by that revision.

## Current Focus

The next engineering milestone is **M1: Portable Deterministic Core**. M0 now
provides the reproducible host, target-build, policy, evidence, and governance
baseline required to separate core-portability defects from local setup drift.
M0-013 is a post-G0 governance hardening increment: implementation is complete
locally, hosted promotion is pending, and M1-002 remains the next emulator-core
task.

Critical path:

```text
M0 hermetic build
  -> M1 portable core -----------+
  -> M2 D1001 board bring-up ----+-> M3 bounded media
                                  +-> M4 display
                                  +-> M5 input/audio
                                      -> M6 A500 alpha
                                      -> M7 A1200 alpha
                                      -> M8 network/control
                                      -> M9 beta
                                      -> M10 release
```

M1 and M2 should run in parallel after M0. M3 can start once the block-device
contract from M1 and SD smoke from M2 are stable.

## Completed Foundation

| Task | Status | Result |
| --- | --- | --- |
| BASE-001 | DONE | Versioned `rumiga.capture.v1` manifest and validator coverage |
| BASE-002 | DONE | Native/presentation screenshots and first-line/edge-wrap diagnostics |
| BASE-003 | DONE | A500 Kickstart 1.3 and A1200 Kickstart evidence |
| BASE-004 | DONE | A1200 Workbench 3.1.4 ADF and HDF evidence scripts |
| BASE-005 | DONE | HDF read-only, snapshot, and explicit writeback host policy |
| BASE-006 | DONE | A2065 model, desktop SLIRP backend, counters, PCAP, API/web controls |
| BASE-007 | DONE | REST/TypeScript DTO and endpoint parity artifact |
| BASE-008 | DONE | Compatibility scenario catalog and report generator |
| BASE-009 | DONE | Current source, architecture, CI, evidence, and D1001 BSP audit |

The foundation proves selected desktop paths. It does not satisfy an embedded
milestone.

## M0 Backlog: Hermetic Engineering Baseline

| Task | Status | Deliverable | Acceptance evidence |
| --- | --- | --- | --- |
| M0-001 | DONE | Replace aspirational project docs with current-state, roadmap, and execution sources of truth | `PROJECT_STATUS.md`, `ROADMAP.md`, this plan, updated audit/architecture/README |
| M0-002 | DONE | Replace unpublished `../r68k` dependencies with a tracked `m68000` differential fixture and frozen checkpoints | Cargo metadata, formatting, Clippy, and all 450 tests pass without sibling directories |
| M0-003 | DONE | Track the root `Cargo.lock`, enforce both application lockfiles, and automate the documented update cadence | Repeated locked Rust and npm resolution leaves both lockfiles byte-identical |
| M0-004 | DONE | Integrate `rumiga-platform-esp` and `firmware` as unpublished workspace packages with centralized metadata, dependencies, and lints | Both manifests pass locked host checks; the full workspace remains green |
| M0-005 | DONE | Pin Rust, Node, ESP-IDF, ESP Rust crates, Seeed BSP SHA, and required tools | Machine-readable toolchain files, immutable source revisions, locked ESP crates, and cross-file Rust tests |
| M0-006 | DONE | Replace hard-coded REST storage path with configured root and canonical path policy | Unit tests cover traversal, symlink escape, bounded atomic uploads, deletion, REST media insertion, CLI limits, and stable HTTP errors |
| M0-007 | DONE | Add host CI matrix for Linux/macOS, Rust fmt/Clippy/test/doc, and web lint/build | Hosted x86_64/arm64 matrix and RustSec audit pass; protected `main` requires the fail-closed aggregate |
| M0-008 | DONE | Add current RISC-V `no_std` boundary and ESP32-P4 firmware evidence jobs | Local and hosted portable checks pass; hosted CI publishes and independently validates the full checksummed evidence bundle |
| M0-009 | DONE | Add advisory, license, source, and dependency-policy checks | Hosted policy evidence has no unreviewed vulnerability, yanked package, incompatible license, or source drift |
| M0-010 | DONE | Add `xtask` or equivalent single entry point for local/CI quality gates | One documented command runs the same gates as CI |
| M0-011 | DONE | Export current compatibility report and test counts as CI artifacts without private media | Hosted private-media-free artifact classifies all scenarios, inventories all tests, and passes independent checksum/privacy verification |
| M0-012 | DONE | Add contribution, review, release-note, and architecture-decision templates | Hosted checksummed artifact validates the M0-012 task/test/evidence traceability example from a clean PR checkout |
| M0-013 | ACTIVE | Enforce one Rust-owned Conventional Commit policy in local hooks, pull requests, and `main` pushes | Focused tests and the local gate pass; hosted PR/title/range and final `main` aggregate evidence is pending |

M0-002 evidence (2026-08-14):

- `cargo metadata --no-deps --format-version 1 --quiet`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` (450 discovered tests)
- `crates/rumiga-core/tests/cpu_differential.rs`

M0-003 evidence (2026-08-14):

- `cargo metadata --locked --no-deps --format-version 1 --quiet`
- `cargo clippy --locked --workspace --all-targets -- -D warnings`
- `cargo test --locked --workspace` (450 discovered tests)
- `(cd web && npm ci --ignore-scripts)`
- repeated SHA-256 checks leave `Cargo.lock`
  (`500e663dc114a147811cbe3661990fed6131830d333ccd5d840e667fb752fb4a`)
  and `web/package-lock.json`
  (`22359de2367abab4b83c7ccd3e58c5300fee4fcf27ca9d7697434c487652c1dc`)
  unchanged
- `npm audit --audit-level=high`, web lint, and production build pass after
  compatible security updates
- `.github/workflows/ci.yml`, `.github/dependabot.yml`, and
  `DEPENDENCY_POLICY.md`

M0-004 evidence (2026-08-14):

- `cargo metadata --locked --no-deps --format-version 1 --quiet`
- `cargo check --locked --manifest-path crates/rumiga-platform-esp/Cargo.toml`
- `cargo check --locked --manifest-path firmware/Cargo.toml`
- `cargo clippy --locked --workspace --all-targets -- -D warnings`
- `cargo test --locked --workspace` (450 discovered tests)

M0-005 evidence (2026-08-14):

- `toolchain/manifest.toml`, root and firmware `rust-toolchain.toml` files,
  `.node-version`, `.cargo/config.toml`, and exact Cargo/npm manifests
- `cargo test --locked -p rumiga-firmware --test toolchain_manifest`
- resolved `esp-idf-svc 0.52.1`, `esp-idf-hal 0.46.2`, and
  `esp-idf-sys 0.37.2` from immutable upstream IDF 6 fix revisions, plus
  `embuild 0.33.3`, in `Cargo.lock`
- installed `nightly-2026-07-27` with `rust-src`
- `cargo clippy --locked --workspace --all-targets -- -D warnings` under Rust
  1.97.1
- `cargo test --locked --workspace` (452 discovered tests)
- repeated resolution preserves `Cargo.lock`
  (`125ceebff9b79160ebe88ed4943cda4e3311fb66b87006fc397a6681619c0ca9`)
  and `web/package-lock.json`
  (`9cc4ae0079f8fb3126e7a80a43b5cb7e8460a79608f66440214d756ee1712074`)
- IDF 6.0.0 and 6.0.2 tag commits, current esp-rs patch revisions, and the
  Seeed hardware-reference SHA independently verified against their Git
  repositories
- Vellum revision `15bff64d316c3751861d02fcf7ace6b47afab176` records working
  IDF 6.0.0 D1001 bring-up; owner-authored implementation code is authorized
  for provenance-tracked reuse in Rumiga under `GPL-3.0-only`

M0-006 evidence (2026-08-15):

- `desktop/src/storage.rs` owns canonical root resolution, path confinement,
  extension policy, deterministic listing, and real filesystem capacity
- `--storage-root`, `RUMIGA_STORAGE_ROOT`, and `--upload-limit-mib` define the
  desktop policy without a developer-specific path
- multipart uploads stream into a bounded temporary file, call `sync_all`, and
  publish through an atomic no-overwrite hard link
- REST listing, upload, delete, and floppy insertion return stable versioned
  errors with meaningful HTTP status codes
- focused tests cover traversal, symlink escape, upload limit/cleanup,
  unsupported types, overwrite, deletion, CLI validation, and REST confinement
- `cargo clippy --locked -p rumiga-desktop --all-targets -- -D warnings`
- `cargo test --locked --workspace` (462 discovered tests)

M0-007 evidence (2026-08-15):

- `.github/workflows/ci.yml` targets explicit `ubuntu-24.04` x86_64 and
  `macos-15` arm64 runners with `fail-fast: false`
- immutable action revisions, non-persistent checkout credentials, read-only
  default token permissions, exact Rust/Node/npm verification, and monthly
  GitHub Actions updates
- both host legs require Rust format, Clippy, tests, warning-free docs, npm
  clean install without lifecycle scripts, ESLint, Next.js production build,
  and a clean tracked worktree
- clean Ubuntu validation identified and closed the native `libslirp`
  prerequisite; both matrix legs provision their explicit system dependency
- lockfile, host matrix, and RustSec results feed an unconditional
  `Required Quality Gate`; every job publishes a Markdown summary
- `actionlint .github/workflows/ci.yml`
- full locked macOS host command set and npm advisory gate pass locally
- a private-asset-free Git archive passes web install/lint/build and the full
  locked Rust command set in a clean Ubuntu 24.04 arm64 container
- GitHub Actions run
  [`31889431633`](https://github.com/metaneutrons/rumiga/actions/runs/31889431633)
  passes lockfile, Linux x86_64, macOS arm64, RustSec, and aggregate jobs at
  `b83dd51`
- `main` requires the strict, GitHub-Actions-bound `Required Quality Gate`, pull
  requests, linear history, resolved conversations, and forbids force pushes
  and deletion

M0-008 evidence (2026-08-15):

- `cargo +1.97.1 check --locked --target
  riscv32imafc-unknown-none-elf -p m68000 -p rumiga-api -p rumiga-platform`
  passes for the packages that are genuinely `no_std` today
- `cargo +1.97.1 xtask firmware-evidence` builds from a dedicated clean target,
  verifies ESP-IDF `6.0.0` at the pinned commit, validates the static 32-bit
  RISC-V single-float ELF and final Rust linker map, and enforces the D1001
  board configuration
- `target/m0-008-firmware-evidence` contains the ELF, map, merged flash image,
  bootloader, partition table, resolved `sdkconfig`, flash layout, size report,
  `rumiga.firmware.build.v1` manifest, and independently passing `SHA256SUMS`
- evidence distinguishes the 32 MB physical flash from the conservative 16 MB
  Seeed/Vellum firmware geometry and records QIO runtime versus DIO bootloader
  flashing
- GitHub Actions run
  [`31890919057`](https://github.com/metaneutrons/rumiga/actions/runs/31890919057)
  passes the portable and firmware jobs plus the aggregate gate for head commit
  `3cd47ddb3bb02eb9eecde59a651dcebe0badcf99`
- hosted artifact ID `9248602076` contains all expected files, has archive
  SHA-256 `a49535d56c0be4740ce6711a99e28829608044e99ada9be66e7b5cf593c5cc7e`,
  and passes all nine payload checksums after download
- conversion of `rumiga-core` and `m68k` to `no_std + alloc` remains M1, while
  flash, boot, peripherals, and performance remain M2+

M0-009 evidence (2026-08-15):

- `supply-chain-policy.toml` records exact Cargo/npm/Action sources, SPDX
  allowlists, duplicate baselines, owners, reasons, compensating controls, and
  expiring exceptions
- `deny.toml` checks every Cargo feature for licenses, advisories, sources,
  wildcard dependencies, and workspace dependency policy
- `cargo +1.97.1 xtask supply-chain-evidence` validates all repository policy
  inputs, invokes pinned scanners, and emits `rumiga.supply-chain.evidence.v1`
- local evidence covers 350 Rust packages, 440 npm packages, and 13 immutable
  Action references; it reports zero Rust vulnerabilities, zero yanked
  packages, and zero npm vulnerabilities at every severity
- the RustSec database freshness limit is seven days; all registry packages
  have locked checksums, npm packages have exact SHA-512 integrity or a
  protected bundle parent, and all script-bearing npm packages are denied
- `target/m0-009-supply-chain-evidence/SHA256SUMS` validates every scanner
  report and the manifest
- GitHub Actions run
  [`31894500079`](https://github.com/metaneutrons/rumiga/actions/runs/31894500079)
  passes every prerequisite and the `Required Quality Gate` for branch head
  `53e154d8cecc0d3f9359ba023be6e5803c251b87`
- hosted artifact `supply-chain-055b0ae3ed36a44c44aa7314ac928545dc7262ae`
  (ID `9249484883`) has archive SHA-256
  `2c477e759400e0d12e7139b3613fd7bd10f4f0dd07d20f4016c5edc48387f0c9`;
  all seven payload hashes pass after independent download

M0-010 evidence (2026-08-15):

- `cargo +1.97.1 xtask ci` runs `lockfiles`, `host`, `supply-chain`,
  `portable`, and `firmware` in canonical order; the clean local run at
  `e2f7d653df91ce53842d649ec85edc756d4b6f2f` passes all five in 56.440 seconds
- 13 `rumiga-xtask` tests cover CLI selection, canonical ordering, checksum
  parsing, workflow topology, and the static supply-chain policy; the complete
  workspace discovers 475 unit, integration, and documentation tests
- every gate checks relevant tool pins, preserves staged and unstaged tracked
  state, and rejects a dirty CI checkout; evidence checksum verification is
  implemented in portable Rust and requires exact directory coverage
- each GitHub prerequisite job invokes the same implementation with one
  `--gate` selector; a structural repository test rejects workflow invocation
  or aggregate-dependency drift
- GitHub Actions run
  [`31899884533`](https://github.com/metaneutrons/rumiga/actions/runs/31899884533)
  passes lockfile, Linux x86_64, macOS arm64, supply-chain, portable Rust,
  firmware, and `Required Quality Gate` jobs for branch head
  `e2f7d653df91ce53842d649ec85edc756d4b6f2f`
- pull-request merge revision `20c280bddd2a28597534efb1bac053f6c5ea859b`
  produced supply-chain artifact `9250843826` with archive SHA-256
  `9bdc8283b6fbf8faaf1d766df658e4df927c07d1411b02da4cf0786595cb9440`
  and firmware artifact `9250846613` with archive SHA-256
  `71b4fc0c6f05109b441dbd91eb8c5d3bee86c69e9c67da58a0037783ed7eea91`;
  all 7 and 9 payload hashes and both clean-revision claims pass independent
  download verification

M0-011 evidence (2026-08-15):

- `cargo +1.97.1 xtask ci --gate compatibility` emits and verifies
  `rumiga.public-evidence.bundle.v1` without reading `target/evidence`
- the complete local `cargo +1.97.1 xtask ci` baseline passes all six canonical
  gates; the compatibility gate also passes from a clean checkout without a
  generated `web/out` directory
- all 16 catalog scenarios are classified: 1 asset-free REST/web contract
  passes, 12 private-media scenarios have explicit skipped reason codes, and 3
  roadmap exclusions are unsupported
- Cargo-built harness and rustdoc discovery reports 482 tests: 478 runnable
  and 4 ignored with exact entries, reasons, and tracking IDs in
  `evidence/ignored-tests.json`
- GitHub Actions run
  [`31910408906`](https://github.com/metaneutrons/rumiga/actions/runs/31910408906)
  publishes the first hosted baseline for branch head
  `aff4a6e680ab71aeff94f7416823008319156582`; pull-request merge revision
  `c61242bd545fc4fd6bedc28f217bcd2695955529` produced artifact
  `compatibility-c61242bd545fc4fd6bedc28f217bcd2695955529` (artifact ID
  `9253512112`, archive SHA-256
  `ee634d0f429c673e465776cb70de002adaf3867a539623374e57e3332444d00a`)
- independent download verification confirms the exact six-file archive, all
  five payload checksums, the clean source revision, the 16-scenario and
  482-test totals, reviewed ignores, privacy flags, and absence of private
  filesystem paths

M0-012 evidence (2026-08-16):

- `cargo +1.97.1 xtask ci --gate governance` validates 13 versioned contracts,
  one accepted ADR, one unreleased note, and one machine-readable change record
- `governance/changes/M0-012.json` links the stable task to three test commands,
  the checksummed CI artifact, eight documentation sources, its release note,
  risk/rollback, and ADR-0001
- five focused tests reject malformed task IDs, unsafe paths, duplicate/missing
  Markdown contracts, private filesystem markers, and repository contract drift
- the complete local seven-gate baseline passes in 78.601 seconds, including
  the pinned ESP32-P4 release build
- GitHub Actions run
  [`31933087138`](https://github.com/metaneutrons/rumiga/actions/runs/31933087138)
  passes every required job for branch head
  `ad461580287229366c6b0492e9cfedad2f6610fe`; PR merge revision
  `11e68bddf0f7739ed11711c97de0483f8381b6a6` produced artifact
  `governance-11e68bddf0f7739ed11711c97de0483f8381b6a6` (artifact ID
  `9259855560`, archive SHA-256
  `249614ac364af890f92da3dcb8a1a3e3917f4be553fb54eade2d4c314ccbb480`)
- independent download verification confirms exactly four regular files, all
  three payload checksums, a clean source revision, the 13-contract and
  task-link totals, public scope flags, and absence of private filesystem paths

M0-013 implementation evidence (2026-08-16):

- commit `0ed5f53` replaces the local shell regex with a bounded Rust parser
  shared by `.githooks/commit-msg` and the canonical `commits` gate
- the parser accepts the documented types, optional lowercase scopes, `!`,
  breaking-change footers, revert messages, and existing Dependabot prefixes;
  it rejects malformed, WIP, autosquash, merge, unsafe-control, non-UTF-8, and
  oversized inputs
- CI checks complete history with immutable event object IDs, validates every
  commit after the merge base, validates the pull-request title for squash
  safety, and validates the resulting `main` push range again
- `cargo +1.97.1 test --locked -p rumiga-xtask` passes all 31 tests;
  `cargo +1.97.1 clippy --locked -p rumiga-xtask --all-targets -- -D warnings`
  and `cargo +1.97.1 xtask ci --gate commits` pass locally
- the workflow structure test requires all eight canonical gate invocations and
  the `commits` dependency in `Required Quality Gate`
- hosted pull-request and final `main` evidence is pending; local hook success
  alone is not promotion evidence

### M0 functional commits

1. `docs(project): establish embedded-first roadmap and status`
2. `chore(workspace): remove sibling r68k dependency`
3. `chore(deps): enforce reproducible dependency resolution`
4. `chore(esp): make firmware workspace topology explicit`
5. `chore(toolchain): pin host and ESP build inputs`
6. `fix(toolchain): adopt validated ESP-IDF 6 baseline`
7. `fix(api): sandbox desktop media storage root`
8. `ci: add pinned host quality matrix`
9. `ci: publish quality and evidence summaries`
10. `ci(embedded): publish ESP32-P4 build evidence`
11. `feat(supply-chain): enforce reviewed dependency policy`
12. `ci(security): publish supply-chain evidence`
13. `docs(project): close M0-009 with hosted evidence`
14. `feat(quality): unify local and CI gates`
15. `docs(quality): document unified validation entry point`
16. `docs(project): close M0-010 with hosted evidence`
17. `feat(evidence): build public compatibility reports`
18. `ci(evidence): publish compatibility baseline`
19. `docs(evidence): document public CI baseline`
20. `fix(desktop): decouple test assets from web build`
21. `docs(project): close M0-011 with hosted evidence`
22. `feat(governance): version engineering change contracts`
23. `ci(governance): publish traceability evidence`
24. `docs(governance): document engineering workflow`
25. `docs(project): close M0-012 with hosted evidence`
26. `feat(quality): enforce conventional commit policy`
27. `docs(governance): document conventional commit policy`
28. `test(quality): cover commit range enforcement`
29. `docs(project): close M0-013 with hosted evidence`

### M0 promotion command set

```sh
cargo +1.97.1 xtask ci
```

The individual operations remain visible under named gates. `--gate <name>` is
available for diagnosis and CI parallelism, but a subset is not a complete
local promotion result.

## M1 Backlog: Portable Deterministic Core

| Task | Status | Deliverable | Acceptance evidence |
| --- | --- | --- | --- |
| M1-001 | DONE | Add `std`/`no_std` feature model to `rumiga-core` | Local and hosted Linux/macOS gates compile, lint, and test both valid profiles and reject invalid selections |
| M1-002 | NEXT | Make `m68k` compile under `no_std + alloc`; isolate FPU constants/features | 68000 and 68EC020 release profiles compile on RISC-V target |
| M1-003 | PLANNED | Replace `std` collections/cells with `core`/`alloc` where required | No `std::` use in canonical core build |
| M1-004 | PLANNED | Introduce injected trace/log sink and remove core file creation | Host trace output remains byte-compatible in integration tests |
| M1-005 | PLANNED | Remove core thread spawning and affinity; restore deterministic single-owner blitter | Frame/state digests match before/after on host fixtures |
| M1-006 | PLANNED | Introduce emulated clock, host yield, and monotonic scheduling contracts | PAL/NTSC timing tests do not read host wall clock |
| M1-007 | PLANNED | Version platform capabilities and typed error model | Unsupported and backpressure states are explicit and tested |
| M1-008 | PLANNED | Add bounded video/audio/input/event queue contracts | Overflow policy and high-water marks have tests |
| M1-009 | PLANNED | Add deterministic input replay and machine-state digest | Same replay yields same digest on repeated host runs |
| M1-010 | PLANNED | Add allocation instrumentation and steady-state no-allocation assertion | One-minute host run has no scanline-loop allocations |
| M1-011 | PLANNED | Measure 32-bit assumptions, alignment, endianness, and `usize` conversions | Miri/sanitizer/property fixtures cover critical boundaries |
| M1-012 | PLANNED | Publish portability contract in architecture docs | Core dependency graph contains only approved `no_std` crates |

M1-001 evidence (2026-08-16):

- commit `4349c73` defines mutually exclusive `std` and `no_std` profiles;
  `std` remains the documented default
- the core uses `core`/`alloc` primitives where required; filesystem tracing,
  host threads, and CPU affinity are excluded from `no_std`
- commit `c692571` adds the feature matrix to the canonical host gate on both
  supported hosted operating systems
- local `cargo +1.97.1 xtask ci --gate host` passes the explicit `std` check,
  `no_std` Clippy and test suites, both expected-failure checks, the default
  workspace, Rustdoc, and the web build
- the complete local seven-gate promotion baseline passes in 82.621 seconds
- GitHub Actions run
  [`31934749529`](https://github.com/metaneutrons/rumiga/actions/runs/31934749529)
  passes every prerequisite and the strict aggregate for branch head
  `f538f0ba811691703dd88b1c75d7cceaa5dc8676`; Linux host job `95134810493`
  and macOS host job `95134810516` independently pass the feature matrix
- governance artifact `9260313104`, built from pull-request merge revision
  `aab85a06bd8c893397b5e9ac719c77863628c5a1`, has archive digest
  `e59760be76a9b1be3599fed1dc8300c08c64ed667c13506d224a122e7042c7b6`;
  all three payload checksums, clean revision, two ADRs, two release notes, two
  change records, and six test references were independently verified
- this is a source-profile result, not RISC-V target evidence: `m68k` remains
  `std` until M1-002

### M1 functional commits

1. `refactor(core): define std and no-std runtime profiles`
2. `ci(core): enforce the runtime feature matrix`
3. `docs(core): document the runtime profile contract`
4. `docs(project): close M1-001 with hosted evidence`
5. `refactor(cpu): make stock m68k profiles no-std`
6. `refactor(core): inject trace and host services`
7. `refactor(blitter): restore deterministic single-owner execution`
8. `feat(platform): add capabilities errors and bounded queues`
9. `test(core): add deterministic replay and state digests`
10. `ci(core): enforce riscv no-std portability`

## M2 Backlog: D1001 Board Bring-Up

| Task | Status | Deliverable | Acceptance evidence |
| --- | --- | --- | --- |
| M2-001 | PLANNED | Record D1001 schematic revision, board revision, BSP SHA, and connector inventory | Reviewed hardware manifest under `docs/hardware` |
| M2-002 | PLANNED | Create reproducible ESP-IDF/Rust firmware build using `riscv32imafc-esp-espidf` | CI produces ELF, binary, map, size report, and checksums |
| M2-003 | PLANNED | Define partitions, PSRAM allocator, panic, watchdog, logging, and reset policy | Boot manifest reports all values and reset reason |
| M2-004 | PLANNED | Port proven Vellum D1001 services into Rust-first adapters and establish the safety/provenance contract | Exact source-transfer records, narrowly scoped unsafe code, host mocks, and third-party license audit pass |
| M2-005 | PLANNED | Add serial command protocol for capabilities, self-test, metrics, and reset | Versioned protocol test and captured cold-boot log |
| M2-006 | PLANNED | Bring up RGB565 display test pattern and framebuffer checksum | HIL screenshot/checksum artifact |
| M2-007 | PLANNED | Bring up GSL3670 touch and calibration capture | HIL touch-point matrix |
| M2-008 | PLANNED | Bring up ES8311/PCA9535 speaker tone | Frequency/amplitude/underrun artifact |
| M2-009 | PLANNED | Bring up SD/MMC read/write/flush and fault reporting | Fixture file checksum and removal/reinsert test |
| M2-010 | PLANNED | Bring up ESP32-C6 SDIO link and local network smoke | Link/reconnect counters without guest emulation |
| M2-011 | PLANNED | Qualify USB host connector, role, VBUS, hub, keyboard, and mouse | Schematic note plus actual-board enumeration matrix |
| M2-012 | PLANNED | Automate 20 cold boots and board service report | HIL job with zero unexplained resets |

### M2 functional commits

1. `build(firmware): add pinned esp32-p4 image pipeline`
2. `feat(d1001): port authorized Vellum board services`
3. `feat(firmware): expose boot manifest and serial self-test`
4. `feat(d1001): bring up display and touch`
5. `feat(d1001): bring up audio and sdmmc`
6. `feat(d1001): bring up c6 link and usb host`
7. `test(hil): qualify d1001 board services`

## M3 Backlog: Bounded Media and Memory

| Task | Status | Deliverable | Acceptance evidence |
| --- | --- | --- | --- |
| M3-001 | PLANNED | Design object-safe or generic sector `BlockDevice` contract | Contract tests cover capacity, partial failure, read-only, flush, and change |
| M3-002 | PLANNED | Refactor ATA/Gayle away from owned whole-image `Vec<u8>` | Existing host HDF evidence remains green |
| M3-003 | PLANNED | Add memory and desktop-file block-device adapters | Unit and integration tests use identical ATA suite |
| M3-004 | PLANNED | Add SD/MMC file adapter with bounded sector cache | Cache cap/high-water metrics and randomized read tests |
| M3-005 | PLANNED | Add copy-on-write snapshot overlay and atomic metadata | Crash/fault injection preserves base image |
| M3-006 | PLANNED | Harden raw/RDB geometry and overflow handling | Fuzz corpus and malformed image tests |
| M3-007 | PLANNED | Add media-change generation and in-flight I/O cancellation | Eject/removal tests return typed errors without stale writes |
| M3-008 | PLANNED | Define release memory budgets for A500 and A1200 | Link map plus runtime high-water report |
| M3-009 | PLANNED | Boot local 2 GiB Workbench HDF on D1001 | HIL manifest proves <=1 MiB cache and <=27 MiB total PSRAM |

### M3 functional commits

1. `feat(storage): add bounded block-device contract`
2. `refactor(ide): stream sectors through block devices`
3. `feat(storage): add desktop sdmmc and snapshot adapters`
4. `fix(storage): harden geometry flush and media-change behavior`
5. `test(storage): add corruption and power-loss fault matrix`
6. `test(hil): boot large hdf within d1001 memory budget`

## M4 Backlog: D1001 Display Pipeline

| Task | Status | Deliverable | Acceptance evidence |
| --- | --- | --- | --- |
| M4-001 | PLANNED | Define native-frame, presentation-frame, and panel contracts | Geometry tests cover PAL/NTSC and every viewport preset |
| M4-002 | PLANNED | Add DMA-safe RGB565 MIPI-DSI buffers and ownership protocol | No buffer race under stress instrumentation |
| M4-003 | PLANNED | Implement rotation and landscape layout for 1280x800 presentation | Panel screenshot has symmetric intended border |
| M4-004 | PLANNED | Implement aspect-fit/nearest scaling and explicit border policy | Golden transforms and pixel-edge tests |
| M4-005 | PLANNED | Add tear-free swap or bounded update scheduling | Frame timing and tear-line measurement |
| M4-006 | PLANNED | Add native/presented screenshot service on firmware | PNG/raw artifact plus metadata through serial/REST |
| M4-007 | PLANNED | Port first-20-lines and edge-wrap diagnostics to device evidence | A500/A1200 HIL captures pass |
| M4-008 | PLANNED | Add OSD compositor outside native evidence buffer | OSD on/off does not change native framebuffer hash |

### M4 functional commits

1. `refactor(display): version native and presentation contracts`
2. `feat(d1001): present rgb565 through mipi-dsi`
3. `feat(display): add rotation aspect and border policy`
4. `feat(firmware): capture native and presented frames`
5. `test(hil): guard d1001 edge crop stretch and tearing`

## M5 Backlog: Touch, USB, and Audio

| Task | Status | Deliverable | Acceptance evidence |
| --- | --- | --- | --- |
| M5-001 | PLANNED | Define platform-neutral key, pointer, joystick, and hot-plug events | Mapping/property tests contain no host key codes in core |
| M5-002 | PLANNED | Implement USB HID keyboard and rollover handling | Scripted report matrix and 100 hot-plug cycles |
| M5-003 | PLANNED | Implement USB HID mouse and common gamepad mappings | Movement/button/axis HIL matrix |
| M5-004 | PLANNED | Implement touch calibration, OSD routing, and Amiga mouse mode | <=2 percent calibration error artifact |
| M5-005 | PLANNED | Define bounded audio sink, resampler, and clock-drift policy | Waveform and queue-overflow tests |
| M5-006 | PLANNED | Configure ES8311 I2S DMA and safe mono downmix | Audio loopback frequency/THD/clipping artifact |
| M5-007 | PLANNED | Expose volume, mute, latency, underrun, and input metrics | REST/serial contract tests |
| M5-008 | PLANNED | Measure end-to-end input latency and audio stability | G5 latency and 60-minute zero-underrun report |

## M6 Backlog: A500 Device Alpha

| Task | Status | Deliverable | Acceptance evidence |
| --- | --- | --- | --- |
| M6-001 | PLANNED | Integrate stock A500 profile into firmware shell | Capability/config manifest |
| M6-002 | PLANNED | Run Kickstart 1.3 insert-hand on D1001 | Native/presented/reference HIL pack |
| M6-003 | PLANNED | Boot Workbench 1.3 ADF with input/audio | Scripted interactive milestone |
| M6-004 | PLANNED | Validate 100/200/400/800 percent trackdisk modes | Timing, boot/install, and compatibility matrix |
| M6-005 | PLANNED | Curate and execute ten OCS alpha scenarios | Per-title manifests and reference notes |
| M6-006 | PLANNED | Optimize only measured A500 hot paths | Before/after host and D1001 benchmark report |
| M6-007 | PLANNED | Run eight-hour A500 mixed soak | No crash, reset, leak trend, underrun, or corruption |

## M7 Backlog: A1200 Device Alpha

| Task | Status | Deliverable | Acceptance evidence |
| --- | --- | --- | --- |
| M7-001 | PLANNED | Integrate stock 68EC020/A1200 profile | Config and CPU exception/timing diagnostics |
| M7-002 | PLANNED | Run Kickstart 3.x insert screen on D1001 | Native/presented/reference HIL pack |
| M7-003 | PLANNED | Boot Workbench 3.1/3.1.4 ADF | Usable desktop/input/audio evidence |
| M7-004 | PLANNED | Boot 2 GiB Workbench HDF from SD | Bounded memory, I/O latency, and safe-write evidence |
| M7-005 | PLANNED | Add focused AGA mode fixtures | 8-bitplane, HAM8, sprites, dual-playfield, hires, scroll artifacts |
| M7-006 | PLANNED | Curate and execute ten AGA alpha scenarios | Per-title manifests and reference notes |
| M7-007 | PLANNED | Optimize only measured A1200 hot paths | >=0.98 Workbench and >=0.95 scenario real-time ratios |
| M7-008 | PLANNED | Run 12-hour A1200 mixed soak | No crash, reset, leak trend, underrun, or corruption |

## M8 Backlog: Network and Control Plane

| Task | Status | Deliverable | Acceptance evidence |
| --- | --- | --- | --- |
| M8-001 | PLANNED | Stabilize C6 SDIO/Wi-Fi lifecycle and reconnect | Link-loss/recovery HIL matrix |
| M8-002 | PLANNED | Connect A2065 packet boundary to device host network | Non-zero guest TX/RX with deterministic scheduling counters |
| M8-003 | PLANNED | Prove guest ping, DNS, HTTP, and checksum | Local fixture PCAP/counters and guest result |
| M8-004 | PLANNED | Run sustained guest transfer and interrupt stress | One-hour report with no stalls/leaks |
| M8-005 | PLANNED | Serve versioned REST and embedded static web app | Device API contract and browser tests |
| M8-006 | PLANNED | Add secure Wi-Fi provisioning and credential storage | Threat-model tests and redacted support bundle |
| M8-007 | PLANNED | Harden auth, CSRF, upload, paths, rate/size limits, and defaults | Negative security suite passes |
| M8-008 | PLANNED | Align CLI/serial, REST, web, persistence, and capabilities | Generated contract/round-trip artifact |
| M8-009 | PLANNED | Add optional local-only redacted packet capture | Privacy and fixture-only test policy |

## M9 Backlog: Compatibility and Performance Beta

| Task | Status | Deliverable | Acceptance evidence |
| --- | --- | --- | --- |
| M9-001 | PLANNED | Expand catalog to 20 OCS, 10 ECS, and 20 AGA scenarios | Versioned legal/local corpus metadata |
| M9-002 | PLANNED | Add FS-UAE/WinUAE version/config/reference metadata | Differential report for every release-critical scenario |
| M9-003 | PLANNED | Add CPU/CIA/copper/blitter/disk/audio differential fixtures | Subsystem trace comparisons |
| M9-004 | PLANNED | Add fuzz/property targets and seed corpus | No critical/high crash after defined campaign budget |
| M9-005 | PLANNED | Add frame/memory/audio/input/storage/network telemetry | HIL time-series artifacts and thresholds |
| M9-006 | PLANNED | Automate power, reset, SD, USB, Wi-Fi, and brownout faults | Recovery matrix |
| M9-007 | PLANNED | Run 24-hour beta qualification on multiple boards | G9 report with no leak/thermal/watchdog issue |
| M9-008 | PLANNED | Triage every partial/fail with severity and disposition | No unowned or unexplained result |

## M10 Backlog: Production Release

| Task | Status | Deliverable | Acceptance evidence |
| --- | --- | --- | --- |
| M10-001 | PLANNED | Define versioning, release branches, changelog, and support policy | Reviewed release procedure |
| M10-002 | PLANNED | Produce reproducible firmware/web image and checksums | Independent rebuild comparison |
| M10-003 | PLANNED | Generate SBOM, licenses, advisories, and source offer | Release compliance bundle |
| M10-004 | PLANNED | Add signed update, rollback, and factory recovery | Interrupted-upgrade HIL matrix |
| M10-005 | PLANNED | Add configuration migrations and downgrade behavior | Version matrix tests |
| M10-006 | PLANNED | Apply secure production defaults and key lifecycle | Security review and provisioning evidence |
| M10-007 | PLANNED | Publish user, operator, troubleshooting, and compatibility docs | Documentation acceptance review |
| M10-008 | PLANNED | Run 72-hour qualification on at least three D1001 units | E6 release evidence pack |
| M10-009 | PLANNED | Sign and publish release with known issues | No critical/high open release blocker |

## Quality-Gate Checklist for Every Functional Commit

- Scope maps to one or more stable task IDs.
- Existing user changes remain intact and unrelated files are not reformatted.
- Tests scale with the behavior and failure modes changed.
- No new unbounded allocation, queue, I/O, or retry path.
- Public configuration includes validation, defaults, errors, persistence, API,
  web, and support-bundle behavior where applicable.
- Logs and evidence exclude secrets and copyrighted bytes.
- Host and device performance are measured when a hot path changes.
- `PROJECT_STATUS.md` changes in the same commit if a verified claim or
  milestone status changes.
- Commit message describes one functional result, not a batch of unrelated
  cleanup.

## Evidence Layout

Generated host and HIL artifacts stay outside git unless synthetic and approved:

```text
target/evidence/<scenario>/<revision>/
  rumiga.json
  native.png
  presented.png
  audio.wav
  input.json
  serial.log
  metrics.json
  notes.md
```

Each manifest must include:

- schema/version, scenario ID, git SHA, dirty flag, build profile, and toolchain;
- platform, board revision, firmware revision, reset reason, and capability set;
- model, CPU, chipset, memory, PAL/NTSC, ROM hash, and media hashes;
- native/presentation geometry, border, crop, aspect, scale, rotation, and hashes;
- emulated time, wall time, speed ratio, frame-time distribution, and queue peaks;
- audio rate, underruns, clipping, and digest;
- media policy, cache high-water, writes, flush state, and snapshot hash;
- input devices, hot-plug events, latency, and replay digest;
- network backend, MAC, link events, packet counters, and redacted endpoints;
- memory high-water, largest free block, temperature, watchdog, and reset data;
- exact pass/partial/fail gate and human-readable notes.

## Tracking Cadence

- Update task state whenever a functional commit lands.
- Regenerate the compatibility report for any compatibility-affecting revision.
- Review risks and milestone dashboard at least once per milestone or after a
  material hardware/toolchain discovery.
- Never close a task based on a stale artifact from another git revision.
- Preserve failed evidence: it is diagnostic history, not clutter.
- Record blocked dependencies explicitly; do not relabel blocked work as done or
  unsupported.
