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
M0-013 and M0-014 are completed post-G0 hardening increments. Their local,
pull-request, and final `main` promotion paths are verified. M1-002 is also
complete with local, pull-request, and final `main` target-build evidence.
M1-003 is complete with local, pull-request, and final `main` portability
evidence. M1-004 is complete with local, pull-request, and final `main` trace
boundary evidence. M1-005 is the next implementation task.

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
| M0-013 | DONE | Enforce one Rust-owned Conventional Commit policy in local hooks, pull requests, and `main` pushes | Local eight-gate baseline, hosted PR commits/title, final `main` range, both strict aggregates, and checksummed governance evidence pass |
| M0-014 | DONE | Verify the merged firmware image against its own configuration evidence | The merged image embeds the ESP-IDF bootloader and partition table byte for byte, the application fits its declared partition, and the manifest records the decoded layout |
| M0-015 | DONE | Move the Node/npm pin to the current 26 line and align the Node type definitions | Every pin site agrees, the cross-file pin test passes, and the web install, lint, and static export succeed on the new runtime |

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

M0-008 evidence correction (2026-08-17):

The board-configuration claim above applied to the resolved `sdkconfig`,
`flasher_args.json`, `bootloader.bin`, and `partition-table.bin`, but not to the
merged flash image in the same bundle. `espflash save-image` was invoked without
`--bootloader`, `--partition-table`, and the flash geometry, so it substituted
its own defaults. Every bundle produced before this correction therefore shipped
a merged image that contradicts its own configuration evidence:

| Property | Configuration evidence | Merged image before the fix |
| --- | --- | --- |
| Application partition | 1,048,576 bytes | 4,128,768 bytes |
| Flash geometry in the image header | 16 MB | 4 MB |
| Flash frequency in the image header | 80 MHz | 40 MHz |
| Bootloader | ESP-IDF build from the pinned `sdkconfig` | espflash default with a rewritten header |

The evidence task now passes the ESP-IDF bootloader, the ESP-IDF partition
table, and the flash mode, size, and frequency derived from the resolved
`sdkconfig`, then verifies that the merged image embeds exactly those bytes and
that the application fits its declared partition. The manifest records the
merged-image regions, their digests, and the decoded partition table, so a
future layout change is visible in the manifest diff. The corrected local bundle
reports an identical bootloader and partition-table digest inside and outside the
merged image, an image header of 16 MB at 80 MHz, and an application occupying
175,040 of 1,048,576 partition bytes.

This correction does not change the partition layout itself. The bundle still
uses the stock ESP-IDF single-application table with no OTA slots and no data
partition beyond `nvs` and `phy_init`; defining the product layout remains
M2-003.
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

- commit `30fd64b` replaces the local shell regex with a bounded Rust parser
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
- the complete local eight-gate baseline passes in 91.516 seconds, including
  the pinned ESP32-P4 release build
- pull-request run
  [`31952285487`](https://github.com/metaneutrons/rumiga/actions/runs/31952285487)
  validates three commits and the PR title at branch head
  `58bf7b5c8b85633251f8817054af614f8c831994`; commit-policy job
  `95177500999` and aggregate job `95178194590` pass
- PR merge revision `cb87993e1b5671b4bd33753a54eb80504bd6310a`
  produced governance artifact `9264985708` with archive SHA-256
  `beafa7e754e75df43be4a5ea9f5f0a852195a54c999798a1a265a7471c88085e`;
  all three payload checksums and the clean-source claim pass independent
  download verification
- final `main` run
  [`31952671051`](https://github.com/metaneutrons/rumiga/actions/runs/31952671051)
  validates all three promoted commits in
  `89be3955ecf77841f659d95354e7186af27d5847..f2505b34676929b0a2bc99ee3b4203db7a9ed80b`;
  commit-policy job `95178459474` and aggregate job `95179110310` pass
- main governance artifact `9265088957` has archive SHA-256
  `a88ff2e04d9e623094baf83e351a5a134ddda82a856bdcde14a0eed2f038b81d`;
  its clean `f2505b3` source, report result, and all three payload checksums pass
  independent download verification

M0-014 verified evidence (2026-08-17):

- the firmware evidence task passes the ESP-IDF bootloader, the ESP-IDF
  partition table, and the flash mode, size, and frequency derived from the
  resolved `sdkconfig` to the image tool, then asserts that the merged image
  embeds those bytes and that the application fits its declared partition
- the manifest gains a `merged_image` section with both region offsets, their
  digests, the application size against its partition size, and the decoded
  partition table, so a layout change is visible in the manifest diff
- passing the flash geometry is load bearing: without it the image tool rewrites
  byte `0x2003` of the supplied bootloader and recomputes its appended digest,
  so 66 bytes differ from the ESP-IDF build
- six unit tests cover the partition-table decoder, the region bounds check, and
  the `sdkconfig` to image-tool value mapping in both directions
- the defect this task closes is recorded in the M0-008 evidence correction above
- pull-request run
  [`32012799294`](https://github.com/metaneutrons/rumiga/actions/runs/32012799294)
  passes all ten jobs for branch head
  `d74790959953befeca4b9b68b55fc665901f4094`
- the hosted firmware bundle from that run was downloaded and independently
  verified: all nine payload checksums pass, the merged image embeds a bootloader
  and partition table identical to the standalone artifacts, its header encodes
  16 MB at 80 MHz, and the application occupies 175,040 of 1,048,576 partition
  bytes
- final `main` run
  [`32013305043`](https://github.com/metaneutrons/rumiga/actions/runs/32013305043)
  passes all ten jobs for clean revision
  `7d162b7345e7a1d2d6ab48e9dc9bdbe7fc9685e1`

M0-015 verified evidence (2026-08-17):

- `toolchain/manifest.toml` moves Node from `24.19.0` to `26.7.0` and npm from
  `11.17.0` to `11.19.0`, the pairing the Node release index records for that
  release. `.node-version`, `web/package.json` engines, and `packageManager`
  follow, and `firmware/tests/toolchain_manifest.rs` proves they agree
- `@types/node` moves from `^22.20.1` to `^26.2.0`. The previous value was a major
  behind the pinned runtime, so the type definitions did not describe the Node
  version the web build actually ran on
- this is a deliberate exception to the otherwise long-term-support selection. At
  the decision date the Node release index lists 26 as `lts: false` with `24.19.0`
  as the current `Krypton` LTS; Node 26 is expected to enter LTS around October
  2026, and the pin moves ahead of that to avoid stepping it twice
- the exception is affordable because Node is a build-time tool only. It produces
  the static export under `web/out`, which is embedded into the desktop binary and
  later the firmware; no Node runtime ships in the product
- nothing in the web stack forbids it: `next@16.3.1` declares `node >=20.9.0` and
  `eslint@9.39.5` declares `^18.18.0 || ^20.9.0 || >=21.1.0`
- on Node `26.7.0` with npm `11.19.0`, `npm ci` installs 355 packages,
  `npm run lint` is clean, and `npm run build` produces the five static routes
- Dependabot pull request 2, which proposed `@types/node` 26 while the runtime was
  still pinned to 24, is superseded by this change
- pull-request run
  [`32070931258`](https://github.com/metaneutrons/rumiga/actions/runs/32070931258)
  passes all ten jobs; Linux job `95513773424`, macOS job `95513773474`, and
  aggregate job `95516139977` pass, so both host legs installed Node `26.7.0`,
  validated it against the repository files, and built the web export
- pull-request governance artifact `9301688782`, produced from clean merge
  revision `690502a33b98c9fb901b09dfe5708eb22af9bb45`, has archive SHA-256
  `e9e60d858b18d0b946500cd52855630edacfff694e828b5df4fc598486bcd1b8`; all payload
  checksums and the M0-015 traceability record were independently verified
- final `main` run
  [`32072021615`](https://github.com/metaneutrons/rumiga/actions/runs/32072021615)
  passes all ten jobs for clean revision
  `60443ca7b45499cb099f92f6bb1ecf1622ce18d8`; Linux job `95517110078`, macOS job
  `95517110152`, and aggregate job `95518955331` pass
- final governance artifact `9302065297` has archive SHA-256
  `eeaf3756244b1fcd6f1bc45d2b530efe9f910cdf0a1004e65fa32a58205f80c3`; all payload
  checksums, the clean-source claim, and the M0-015 traceability record were
  independently verified

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
30. `fix(ci): make the merged firmware image match its configuration evidence`
31. `docs(project): record the merged firmware image contract`
32. `chore(toolchain): move the node pin to the current 26 line`

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
| M1-002 | DONE | Make `m68k` compile under `no_std + alloc`; isolate FPU constants/features | Local and hosted 68000/68EC020 stock-core release profiles compile on the RISC-V target; final `main` evidence is independently verified |
| M1-003 | DONE | Enforce `core`/`alloc` primitives in the canonical core | Both explicit profiles reject `std` replacements with portable equivalents; the stock core remains a bare-metal RISC-V release build |
| M1-004 | DONE | Introduce injected trace/log sink and remove core file creation | Golden records captured from the file-writing implementation are reproduced byte for byte by an in-memory sink under both runtime profiles |
| M1-005 | DONE | Remove core thread spawning and affinity; restore deterministic single-owner blitter | Both runtime profiles reach a pinned fixture digest, and a host capture is byte-identical before and after |
| M1-006 | DONE | Introduce emulated clock, host yield, and monotonic scheduling contracts | The core cannot name a host clock type, emulated frame duration comes from the colour clock, and the shell paces against it with a measured frame rate |
| M1-007 | DONE | Version platform capabilities and typed error model | Unsupported and backpressure states are explicit and tested |
| M1-008 | DONE | Add bounded video/audio/input/event queue contracts | Overflow policy and high-water marks have tests |
| M1-009 | DONE | Add deterministic input replay and machine-state digest | Same replay yields same digest on repeated host runs |
| M1-010 | DONE | Add allocation instrumentation and steady-state no-allocation assertion | One-minute host run has no scanline-loop allocations |
| M1-011 | DONE | Measure 32-bit assumptions, alignment, endianness, and `usize` conversions | Miri/sanitizer/property fixtures cover critical boundaries |
| M1-012 | DONE | Publish portability contract in architecture docs | Core dependency graph contains only approved `no_std` crates |
| M1-013 | DONE | Make the video standard selectable and model NTSC geometry, colour clock, and Agnus identification | An NTSC Kickstart boots, detects the standard, and produces a stable screenshot digest; conformance to documented constants is asserted |

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

M1-002 implementation evidence (2026-08-16):

- `m68k` has mutually exclusive `std` and `no_std` profiles; its default
  desktop graph retains `fpu`, while `no_std,fpu` fails with one stable
  diagnostic
- `cargo +1.97.1 test --locked -p m68k --no-default-features --features no_std`
  passes eight unit tests, including the FPU-less 68EC020 Line-F regression,
  plus its doctest
- `cargo +1.97.1 xtask ci --gate host` passes both CPU/core feature matrices,
  strict Clippy, the complete default workspace, web production build, and
  warning-free Rustdoc in 27.390 seconds
- `cargo +1.97.1 xtask ci --gate portable` compiles the foundation profile and
  then `m68k` plus `rumiga-core` as optimized `no_std` releases for
  `riscv32imafc-unknown-none-elf` in 864 milliseconds
- the complete local eight-gate promotion baseline passes in 95.478 seconds,
  including the pinned ESP32-P4 release build
- pull-request run
  [`31955508417`](https://github.com/metaneutrons/rumiga/actions/runs/31955508417)
  passes all ten jobs in its final attempt for branch head
  `8c5cbae426f84c2da42f1b7292df6cc0ba17a8d2`; portable job `95186193261`,
  Linux job `95186193077`, macOS job `95186193298`, firmware job
  `95186203507`, and aggregate job `95186299985` pass
- pull-request merge revision `114f73b9962372832603424ab4620ddb7bbeee43`
  produced governance artifact `9265830160` with archive SHA-256
  `c3c392ae6c7fe20d3e4e001b013f2b25f9fb337eb0fa9da2114ffb2d28a69208`;
  the clean-source claim and every internal payload checksum pass independent
  download verification
- final `main` run
  [`31955947410`](https://github.com/metaneutrons/rumiga/actions/runs/31955947410)
  validates the promoted three-commit range
  `31e6f37366caec9055e2ab3a7827f69551ed433e..0e07e17028bd249ec44c5c4d1ca87feace4a2dba`;
  portable job `95186495955`, commit-policy job `95186495962`, Linux job
  `95186496046`, macOS job `95186496057`, firmware job `95186496064`, and
  aggregate job `95187164742` pass
- final governance artifact `9265939161`, built from clean `main` revision
  `0e07e17028bd249ec44c5c4d1ca87feace4a2dba`, has archive SHA-256
  `ea3b1043d8b54a0dded2a5d61764ca77c0547398029869367c0ba04d1ef6113d`;
  all payload checksums and the M1-002 traceability record were independently
  verified

M1-003 verified evidence (2026-08-16):

- `rumiga-core` denies `clippy::std_instead_of_core` and
  `clippy::std_instead_of_alloc`, so portable primitive replacements are caught
  in the default desktop profile as well as the `no_std` profile
- the core now uses `core::mem::take`; `MacAddressError` implements the shared
  `core::error::Error` contract and has a profile-neutral regression test
- `cargo +1.97.1 clippy --locked -p rumiga-core --all-targets
  --no-default-features --features std -- -D warnings` passes the explicit
  desktop boundary check
- `cargo +1.97.1 test --locked -p rumiga-core --no-default-features --features
  no_std` passes 146 unit tests plus applicable integration, golden-vector, and
  differential suites; the optimized stock-core bare-metal RISC-V release check
  passes
- `cargo +1.85.0 check --locked -p rumiga-core --no-default-features --features
  no_std` passes after replacing five newer `let`-chain expressions in `m68k`
  with equivalent match guards; the host gate now installs and enforces the
  declared MSRV
- pull-request run
  [`31961164165`](https://github.com/metaneutrons/rumiga/actions/runs/31961164165)
  passes all ten jobs for the promoted change; Linux job `95199261022`, macOS
  job `95199260986`, firmware job `95199260976`, and aggregate job
  `95199880195` pass
- pull-request governance artifact `9267277447`, produced from clean merge
  revision `c4063f31a171a867de5698788eaa2134e52e9e3e`, has archive SHA-256
  `94de57f43b28bdb031ba2851a69bb8e1701073301f0d888ea4585f37f44fe272`; all
  internal payload checksums and the M1-003 traceability record were
  independently verified
- final `main` run
  [`31961501684`](https://github.com/metaneutrons/rumiga/actions/runs/31961501684)
  passes all ten jobs for clean revision
  `917d316902cba5aa77e7d50589eb6f52e70529c3`; Linux job `95200065207`, macOS
  job `95200065227`, firmware job `95200065244`, and aggregate job
  `95200667602` pass
- final governance artifact `9267358611` has archive SHA-256
  `2c5b42f7ad9384f7ca81d4fdbff633005fd122f000de6349ec7cc46bea05e68e`; all
  internal payload checksums, the clean-source claim, and the M1-003
  traceability record were independently verified

M1-004 verified evidence (2026-08-17):

- `rumiga-platform` defines the `TraceSink` contract; `rumiga-core` re-exports
  it, holds an optional boxed sink, and no longer creates files or accepts host
  paths for CPU tracing
- the core keeps record formatting, the trace limit, and the recorded count and
  passes `core::fmt::Arguments` to the sink, so the record layout exists in one
  place and no intermediate `String` is allocated
- `enable_cpu_trace`, `trace_writer`, `trace_limit`, and the public
  `trace_count` field are replaced by `set_trace_sink`, `flush_trace`,
  `clear_trace_sink`, and a `trace_count` accessor; trace state is private
- tracing is no longer feature-gated and runs in both runtime profiles
- `rumiga-platform-desktop` owns `FileTraceSink`, which creates the file,
  buffers writes, and appends `\n`; the desktop flushes explicitly when the
  interactive loop ends and after a capture run
- `cargo +1.97.1 test --locked -p rumiga-core --test trace_test` and the same
  suite with `--no-default-features --features no_std` both reproduce golden
  records captured from the previous file-writing implementation, so byte
  compatibility holds without the core creating a file
- `cargo +1.97.1 test --locked -p rumiga-platform-desktop` proves real file
  creation, truncation, the newline terminator, and host error reporting
- `cargo +1.85.0 check --locked -p rumiga-core --no-default-features --features
  no_std` passes, so the declared MSRV still covers the new contract
- a three-frame Kickstart 46.143 capture with a 20000-instruction trace limit
  was run at pre-change revision `1a6da29` and at the implementation branch;
  both trace files have SHA-256
  `222caf36e1f9c12b9a051ae792da8091680ea84435f96075dba40fd8f1015bde`, both
  manifests report `trace_count` 20000, and both PNG captures are identical
- the complete local promotion baseline `cargo +1.97.1 xtask ci` passes all
  eight gates in 90.732 seconds
- pull-request run
  [`31998824989`](https://github.com/metaneutrons/rumiga/actions/runs/31998824989)
  passes all ten jobs for branch head
  `af7ef4af571750bc56620978bfdc28712fa51378`; Linux job `95295246690`, macOS
  job `95295246675`, firmware job `95295246715`, and aggregate job
  `95296126338` pass
- pull-request governance artifact `9277710435`, produced from clean merge
  revision `e700c4fdf58e328dfcd905a33df6310056b8821d`, has archive SHA-256
  `034fd3df10f7be6ca0e1e6b0733ee7160ad0a489df46266df36bbfb6341f2115`; all
  internal payload checksums and the M1-004 traceability record were
  independently verified
- final `main` run
  [`31999223974`](https://github.com/metaneutrons/rumiga/actions/runs/31999223974)
  passes all ten jobs for clean revision
  `4b958f88fe18af897e0c4a5328cec801bb5a6a7c`; Linux job `95296309635`, macOS
  job `95296309498`, firmware job `95296309561`, and aggregate job
  `95297244733` pass
- final governance artifact `9277831470` has archive SHA-256
  `9de663eceb3284882630e29c1bb8b251839b08b6e4d5b97b24319edc49dc6d45`; all
  internal payload checksums, the clean-source claim, and the M1-004
  traceability record digest
  `3998e06035db8cebda232344be2e3928e131985d03cd55b449c456a6b9727b5c` were
  independently verified

M1-005 verified evidence (2026-08-18):

- both profiles execute the blit in place through one implementation; no
  `std::thread`, `JoinHandle`, or `core_affinity` remains in the core, and
  `core_affinity` leaves the workspace because the desktop declared it unused
- `start_blitter_execution` raises `INT_BLIT` where completion happens and updates
  the readable interrupt shadow
- three defects fell out with the thread, each now covered by a test that fails on
  the previous implementation: the blitter interrupt was raised only by a later
  synchronisation and therefore never under `no_std`; the guest-visible BBUSY bit
  reported whether a host thread handle existed; and a state digest taken while the
  worker held chip RAM digested an empty slice
- the address bus loses its eager and lazy synchronisation branches, so every
  memory access is shorter
- blits take no emulated time, so BBUSY reads clear and a guest `WaitBlit()` loop
  exits immediately. Cycle-accurate blitter timing is separate future work
- a 64-bit FNV-1a state and frame digest was added in the same task, without a
  dependency, so it works in the portable profile. It is explicitly not
  cryptographic; its unit tests pin published FNV-1a vectors and assert that field
  order and width both change the result
- `cargo +1.97.1 test --locked -p rumiga-core --lib` passes 154 tests under both
  explicit profiles, and `both_runtime_profiles_reach_the_pinned_state` pins the
  fixture digest so the two paths cannot diverge silently
- three consecutive 60-frame Kickstart 46.143 captures are byte-identical to each
  other and to the same capture taken from the threaded implementation at revision
  `1a5bee2`, digest `03a0b882b85474795554180cfa138110`
- public API: `Emulator::sync_blitter` and `sync_blitter_lazy` are removed with the
  `AmigaMemory` thread fields and their sync methods; the blit result is visible to
  the next access
- not measured: frame time. The thread presumably existed for throughput, so the
  cost of removing it belongs to the M9 performance work rather than this task
- pull-request run
  [`32078987151`](https://github.com/metaneutrons/rumiga/actions/runs/32078987151)
  passes all ten jobs; Linux job `95538016620`, macOS job `95538016682`, portable
  job `95538016691`, and aggregate job `95539228051` pass. Both host legs confirm
  the pinned fixture digest, so it is stable across operating systems and
  architectures, and the portable job resolves the core for bare-metal RISC-V
  without `core_affinity`
- pull-request governance artifact `9304428904`, produced from clean merge revision
  `fa4baca41257ad34a0d9b8853261056d5687bcf2`, has archive SHA-256
  `9f12870bc0013f459299e06c6d125838d1ed3489b52b714430e90ad2cf854346`; all payload
  checksums and the M1-005 traceability record were independently verified
- final `main` run
  [`32104990662`](https://github.com/metaneutrons/rumiga/actions/runs/32104990662)
  passes all ten jobs for clean revision
  `4df7ff5a62bb73aabf521d3f1a060720934d7f36`; Linux job `95612536886`, macOS job
  `95612536873`, portable job `95612536863`, and aggregate job `95613657710` pass
- final governance artifact `9312830673` has archive SHA-256
  `9d3a20e597a0014dbcd985612c8d9ea19877395cc3de663b6d6e88fa1629587a`; all payload
  checksums, the clean-source claim, and the M1-005 traceability record were
  independently verified

M1-006 implementation evidence (2026-08-18):

- `rumiga-platform` gains a `Clock` contract with a monotonic `now` and a `pace`
  that returns the time the host actually spent rather than the time requested,
  because a host sleep routinely overshoots and a pacing caller must correct
  against the measurement
- `rumiga-platform-desktop` implements it as `DesktopClock`; four contract tests
  cover monotonicity, that `pace` never reports less than requested, that a zero
  request yields instead of sleeping, and that `now` advances across a `pace` call
- the core declares its emulated frame duration through `Emulator::frame_period`,
  derived from the colour clock and scanline count rather than a rounded rate. A
  PAL frame is 19,967,887 ns, so the frequently quoted 20 ms would be wrong by 32
  microseconds per frame, and the implied rate is PAL's 50.08 Hz. A unit test pins
  this in both runtime profiles
- because the shell paces against the core's declared period rather than a
  constant, pacing follows the video standard with no further change. M1-013 made
  the standard selectable and needed no pacing change to do it
- the desktop frame loop now paces deliberately. It previously ran flat out and
  slept 16 ms only when no frame was ready, so nothing enforced the 50 frames per
  second that `ROADMAP.md` states as the PAL target
- the REST `fps` field is measured over a 500 ms window instead of being reported as
  a hardcoded 50.0. The interface previously published a constant as if it were a
  measurement
- the core cannot name a host clock type: `crates/rumiga-core/clippy.toml` disallows
  `std::time::Instant` and `std::time::SystemTime`, and `lib.rs` denies
  `clippy::disallowed_types`. This was verified by temporarily adding a
  `std`-gated function returning `Instant`, which the lint rejected, so the ban also
  covers the feature-gated path where the trace file and the blitter thread had
  previously hidden. A source text search was deliberately not used, for the reasons
  ADR-0005 records
- the headless capture path is unaffected: a 60-frame Kickstart 46.143 capture keeps
  digest `03a0b882b85474795554180cfa138110`, unchanged from before this task, so
  pacing touches only the interactive loop
- not measured: whether the desktop sustains the paced rate under load. The loop now
  requests the correct period and reports what it achieves, which is the
  precondition for that measurement rather than the measurement itself
- pull-request run
  [`32107349807`](https://github.com/metaneutrons/rumiga/actions/runs/32107349807)
  passes all ten jobs; Linux job `95619373576`, macOS job `95619373631`, portable
  job `95619373640`, and aggregate job `95620531593` pass. Both host legs run the
  four `DesktopClock` contract tests and the frame period test in both runtime
  profiles, so the monotonicity and pacing assertions hold on two operating systems
  and two architectures rather than on the development host alone
- pull-request governance artifact `9313620696`, produced from clean merge revision
  `01668e33584a4ccb0138659659b6f2909f77f873`, has archive SHA-256
  `414bcd7c6463b495071a6b5b8089abe778b75d854e194bbbc6eda290993fd09d` as reported by
  the Actions API; all payload checksums and the M1-006 traceability record were
  independently verified
- final `main` run
  [`32108657023`](https://github.com/metaneutrons/rumiga/actions/runs/32108657023)
  passes all ten jobs for clean revision
  `e00264421c36a3d96eec9b98a491cd75df63ce8c`; Linux job `95623225417`, macOS job
  `95623225438`, portable job `95623225387`, and aggregate job `95624446734` pass
- final governance artifact `9314060110` has archive SHA-256
  `69990cdd57cf0b86f3091c41229459bc9bcb6c54527a7ead4667cd0e0aa908ff` as reported by
  the Actions API. The payload checksums were recomputed with two independent
  implementations and match the artifact's own `SHA256SUMS`, the manifest records
  `dirty: false`, and its recorded input digests match the git tree at that revision
  for the change record, ADR-0011, the release note, and
  `crates/rumiga-platform/src/lib.rs`
- the Clippy ban on host clock types is hosted-verified rather than local only: the
  host gate runs `rumiga-core` Clippy separately under the explicit `std` and
  `no_std` profiles with `-D warnings`, and both legs pass, so the
  `disallowed_types` configuration is enforced in CI in both profiles
- the archive SHA-256 values above are GitHub's reported artifact digests. The
  independent verification covers the payload, not the archive container, because
  the API does not serve the archive bytes to a plain token fetch

M1-013 implementation evidence (2026-08-18):

- the `--ntsc` flag was inert before this task. A 1200-frame A1200 Kickstart 46.143
  capture taken with `--ntsc` at revision `e002644` is byte-identical to the same
  capture without it, so the option offered a choice the machine could not make
- PAL was compiled in at five independent places: the frame loop, the framebuffer
  line filter, `frame_period`, the `BEAMCON0` shadow, and the beam wrap, which was a
  literal 311 in the scanline loop plus a second constant in `advance_beam`. One type,
  `VideoStandard` in `rumiga-core::video`, now answers all of them
- two of those five are coupled and the coupling is not obvious: the frame length and
  the beam wrap must agree. A 262-line frame with a wrap at line 311 leaves the beam
  50 lines further down every frame, so guest code waiting for a line either waits an
  extra frame or never sees it. A test asserts the agreement and fails on the
  half-implemented version, reporting the beam at line 262
- every constant is sourced from `WinUAE` rather than recalled. `include/custom.h`
  gives `MAXVPOS_PAL` 312 and `MAXVPOS_NTSC` 262, `MAXHPOS_PAL` and `MAXHPOS_NTSC`
  both 227, `CHIPSET_CLOCK_PAL` 3,546,895 and `CHIPSET_CLOCK_NTSC` 3,579,545,
  `VBLANK_ENDLINE_PAL` 26 and `VBLANK_ENDLINE_NTSC` 21, and `BEAMCON0_PAL` `0x0020`;
  `include/drawing.h` gives `AMIGA_HEIGHT_MAX_PAL` 576/2 and `AMIGA_HEIGHT_MAX_NTSC`
  486/2; `custom.cpp` sets `0x1000` in `VPOSR` for NTSC and `beamcon0` to `0x00`
- the NTSC frame period is 16,614,960 ns, implying 60.19 Hz. Both the 60 Hz the option
  is usually labelled with and broadcast NTSC's 59.94 Hz are wrong for this machine.
  Both periods were computed independently in Python before being asserted
- the framebuffer stays PAL-sized and constant because PAL is the taller standard. A
  compile-time assertion enforces that, and `Emulator::active_height` lets a presenter
  crop rather than emit the 45 lines the chipset never writes under NTSC
- the guest detects the standard and acts on it. Under `--ntsc`, Kickstart 46.143
  writes `DIWSTRT` `0x1595` and `DIWSTOP` `0x06AD`, a window from line 21 to line 262,
  against PAL's `0x1D95` and `0x38AD` from line 29 to line 312. The two stop lines are
  exactly the two standards' line counts, which the guest can only have derived from
  the standard it read. The NTSC start line happens to equal `VBLANK_ENDLINE_NTSC`,
  but PAL's 29 does not equal `VBLANK_ENDLINE_PAL`'s 26, so that single agreement is a
  coincidence rather than evidence
- `VPOSR` had two implementations that disagreed: the register shadow the guest reads
  included `LOF` while the direct register read omitted it. Adding a standard bit to
  both would have preserved the disagreement, so they were merged into one. The
  guest-visible value is unchanged and one golden vector gained bit 15
- three consecutive 1200-frame NTSC captures share SHA-256
  `06d225152680aa41b640d1c721b1f482c80d7157727b88190111e69f66e29ff6` at 754x482
- PAL is unchanged: a 1200-frame capture is byte-identical to the same capture from
  revision `e002644` in a separate worktree, and the 60-frame capture keeps digest
  `03a0b882b85474795554180cfa138110` recorded for M1-005 and M1-006
- `state_digest` now includes the standard, so the pinned blit fixture digest moved to
  `0x08e6ace72721e3cd`. The frame digest did not move, because the fixture renders no
  frame
- not modelled: the Agnus revision, which `VPOSR` still reports as `0x00` on every
  profile with only the standard bit varying; interlace and long/short frame
  alternation, so `LOF` is always set and an NTSC frame is a flat 262 lines rather
  than alternating 262 and 263; runtime switching through a guest `BEAMCON0` write
- not compared against a reference emulator. The constants come from `WinUAE` sources
  and the guest behaviour is self-consistent, but no frame has been diffed against
  `WinUAE` or FS-UAE output. That comparison is deliberately deferred
- pull-request run
  [`32127572185`](https://github.com/metaneutrons/rumiga/actions/runs/32127572185)
  passes all ten jobs for the rebased head; Linux job `95681428540`, macOS job
  `95681428603`, portable job `95681428496`, and aggregate job `95683119327` pass
- pull-request governance artifact `9320942928`, produced from clean merge revision
  `f6692b20c53107966467a7278c462b32e177b2b0`, has archive SHA-256
  `f3c4ec001cd7bf1d495db5d2ee7c18ac1828753800abcb016905b4e3469b14b9` as reported by
  the Actions API. Its manifest records a bundle of 12 architecture decisions, 13
  release notes, and 13 change records, so ADR-0012 and the M1-013 record are inside
  the validated set rather than alongside it
- final `main` run
  [`32128162254`](https://github.com/metaneutrons/rumiga/actions/runs/32128162254)
  passes all ten jobs for clean revision
  `764cf9cf583ae5debb2bdbc40c1d778737e97e1a`; Linux job `95683225319`, macOS job
  `95683225329`, portable job `95683225532`, and aggregate job `95684641723` pass
- final governance artifact `9321149701` has archive SHA-256
  `db8b036cb976c0f45f98b0e7d4b2aa776fdb2995905537b8513c71f0aa906090` as reported by
  the Actions API. The payload checksums were recomputed with two independent
  implementations and match the artifact's own `SHA256SUMS`, the manifest records
  `dirty: false`, and its recorded input digests match the git tree at that revision
  for the change record, ADR-0012, the release note, the plan, and the status document
- every `video::tests` case and every standard-related `emulator::tests` case appears
  twice in each host job log, once per explicit runtime profile, so the sourced
  constants, the beam wrap agreement, the register reporting, and the digest separation
  hold on Linux x86_64 and macOS arm64 in both profiles
- the archive SHA-256 values above are GitHub's reported artifact digests. The
  independent verification covers the payload, not the archive container, because the
  API does not serve the archive bytes to a plain token fetch

M1-007 implementation evidence (2026-08-18):

- `ARCHITECTURE.md` already required versioned, capability-driven contracts and
  explicit results from methods that can fail or block. None of the three held: there
  was no version, no way to ask what a backend supports, and
  `VideoOutput::present_frame` returned unit while the desktop adapter discarded the
  window update result with `let _ =`. A dead window was indistinguishable from a
  healthy one, and the shell kept measuring a frame rate for frames that reached no
  display
- failure and flow control are now separate contracts, because they answer different
  questions. `PlatformError` reports that something could not be done, with
  `Unsupported` explicit. A display that was not ready or a full audio queue is
  working as designed, so backpressure is reported on the success path through
  `FramePresentation` and `SamplesQueued`
- a `Backpressure` variant on the error type was rejected deliberately. Every caller
  would have to treat one error variant as success, and any code that logs errors
  would report normal flow control as a fault
- absence is representable twice on purpose. `PlatformCapabilities` uses `Option` per
  service, so a caller cannot mistake "no audio" for "audio at 0 Hz on 0 channels",
  and a caller that ignores the descriptor and calls anyway still receives
  `Unsupported`. The descriptor is the polite path; the error is the backstop
- `StorageError` is removed. It duplicated variants a platform-wide error needs
  anyway, and no backend implemented `Storage`, so the migration cost was zero now and
  would have grown with every backend added later
- a backend states whether it can report backpressure at all. The desktop reports
  `false`, because minifb either presents or fails and its own rate limiting blocks
  rather than refusing. Without the flag a shell would read a dropped-frame count of
  zero as evidence of health when it is evidence of nothing
- `DesktopBackend::new` takes the framebuffer bounds the shell uses, so the reported
  maxima cannot drift from the buffer that is allocated
- the two states named in the acceptance criterion are exercised against a backend
  double, because the desktop backend can produce neither. 14 contract tests pass: an
  absent service is `None`, calling it returns `Unsupported`, a refused frame is
  `Ok(DroppedForBackpressure)` and is distinguishable both from a presented frame and
  from the `InvalidArgument` a short buffer produces, and a partly accepted audio
  buffer reports the split
- the version rejection was verified against a probe rather than asserted. A desktop
  backend temporarily reporting `CONTRACT_VERSION + 7` made the shell exit with status
  1, name both versions, and write no capture. The check runs on the capture path as
  well as the interactive one
- rendered output is unchanged: a 1200-frame A1200 capture keeps digest
  `b190d54b1bbf1e6a9bba3f36d34b74c95ab8fc6fe7796f2f6c694b70165ea1aa`, the value
  recorded for M1-013
- not implemented: `AudioOutput` and `Storage` still have no backend. They now
  describe their failures in the shared error model, which is a smaller claim than
  being implemented
- deferred: the bound `AudioCapabilities::max_queued_frames` describes is not enforced
  by anything yet, which is M1-008; publishing capabilities over REST or serial is
  M8-008
- pull-request run
  [`32130769524`](https://github.com/metaneutrons/rumiga/actions/runs/32130769524)
  passes all ten jobs; Linux job `95691198842`, macOS job `95691198708`, portable job
  `95691198712`, and aggregate job `95692638255` pass
- pull-request governance artifact `9322086217`, produced from clean merge revision
  `963803a704756f83fbe257998cb0526c0538dd1c`, has archive SHA-256
  `62c3747e03d0ebbe3c4667c8ee3b7b7adc61b7fd8c9d62122233e6cfe4d74f6f` as reported by
  the Actions API. Its manifest records 13 architecture decisions, 14 release notes,
  and 14 change records, so ADR-0013 and the M1-007 record are inside the validated set
- final `main` run
  [`32132116892`](https://github.com/metaneutrons/rumiga/actions/runs/32132116892)
  passes all ten jobs for clean revision
  `407483750b6f10382f66e904403a592dd95af2c0`; Linux job `95695335410`, macOS job
  `95695335441`, portable job `95695335380`, and aggregate job `95696696391` pass
- final governance artifact `9322578190` has archive SHA-256
  `aafc5d29f2888fa237c8f872d7a9138b0b2e4288a9ca1f10538c3d7aa2b17437` as reported by
  the Actions API. The payload checksums were recomputed with two independent
  implementations and match the artifact's own `SHA256SUMS`, the manifest records
  `dirty: false`, and its recorded input digests match the git tree at that revision
  for the change record, ADR-0013, the release note, `ARCHITECTURE.md`, the plan, the
  status document, and `crates/rumiga-platform/src/lib.rs`
- the 14 contract tests and the 5 desktop capability tests pass once per host leg on
  Linux x86_64 and macOS arm64, and the portable job checks `rumiga-platform` for
  `riscv32imafc-unknown-none-elf` in the `foundation` profile that
  `toolchain/manifest.toml` declares
- the contract tests are deliberately not claimed for a second runtime profile. The
  host gate's explicit `std` and `no_std` matrix covers `rumiga-core` and `m68k`, so the
  portable evidence for these types is compilation rather than execution
- the archive SHA-256 values above are GitHub's reported artifact digests. The
  independent verification covers the payload, not the archive container, because the
  API does not serve the archive bytes to a plain token fetch

M1-008 implementation evidence (2026-08-18):

- `ARCHITECTURE.md` already required overflow policies to be part of the contract. One
  queue existed and it stated nothing: `Emulator::key_event` guarded a `Vec` with
  `if self.key_events.len() < MAX_KEY_EVENTS` and skipped the push otherwise, with no
  counter and no named policy
- the bound is reached in normal use. The queue drains one event every three frames,
  about seventeen per second under PAL, so a key-repeat burst exceeds sixteen events. A
  guest that missed a keystroke was indistinguishable from one that received everything
- `BoundedQueue<T>` fixes capacity at construction, names its `OverflowPolicy`, and
  returns a `QueueAdmission` so the effect of the policy is visible at every push rather
  than inferred by comparing lengths
- the policy is named per queue rather than once for the crate, because the two
  consumers want opposite answers. `RejectNewest` keeps typing order, which is what a
  full keyboard buffer does; `DropOldest` keeps the freshest audio, because stale sound
  is worse than missing sound. A paired test shows the two policies retain different
  items from identical input
- a boolean push result was rejected. Eviction both queues the new item and loses an old
  one, so `QueueAdmission::queued` and `lost_an_item` answer two questions that a
  boolean conflates
- `high_water` and `dropped` survive `clear` on purpose. They describe the queue's
  history, and a shell that clears on reset would otherwise erase the evidence that the
  queue had saturated
- capacity never grows. Growing under load trades a visible loss for an unbounded
  allocation and a latency increase that nothing reports
- the keyboard queue keeps capacity sixteen and `RejectNewest`, which is exactly what
  the unnamed length check already did. The policy is now stated and its effect counted;
  guest-visible behaviour is unchanged
- the counters have a real consumer rather than being decoration. The shell reports lost
  events and the peak depth at shutdown, and capture manifests record the capacity, the
  policy, and all three counters. In a capture run they read zero, which documents that
  no input pressure could have influenced the frame
- `InputCapabilities` gained `max_events_per_poll`, because `InputState::key_events` is
  an unbounded `Vec` and the bound a consumer sizes against belongs in the descriptor
- incidental: draining is now `pop_front` on a `VecDeque` rather than `remove(0)` on a
  `Vec`, which no longer shifts the remaining events. That was not the motivation
- not created: audio and video queues. `AudioOutput` and `Storage` still have no
  backend, so such a queue would have neither producer nor consumer and could not be
  tested against real pressure. The bound `AudioCapabilities::max_queued_frames`
  declares therefore still describes an intention rather than an enforced limit
- not provided: a windowed rate. The counters are cumulative, so a shell cannot
  distinguish a burst an hour ago from one happening now
- rendered output is unchanged: a 1200-frame A1200 capture keeps digest
  `b190d54b1bbf1e6a9bba3f36d34b74c95ab8fc6fe7796f2f6c694b70165ea1aa`
- pull-request run
  [`32137756215`](https://github.com/metaneutrons/rumiga/actions/runs/32137756215)
  passes all ten jobs for the rebased head; Linux job `95712953387`, macOS job
  `95712953256`, portable job `95712953359`, and aggregate job `95714560217` pass
- pull-request governance artifact `9324647028`, produced from clean merge revision
  `9dd6d183c42742ead28dcee843eb3db221f8d207`, has archive SHA-256
  `ac66e1850c1cc554d69f37a6f760b65fc4986256829642377486c1b7e4aece93` as reported by
  the Actions API. Its manifest records 14 architecture decisions, 15 release notes,
  and 15 change records, so ADR-0014 and the M1-008 record are inside the validated set
- final `main` run
  [`32138307307`](https://github.com/metaneutrons/rumiga/actions/runs/32138307307)
  passes all ten jobs for clean revision
  `2e20bef4c5473d63d40e809e97c6d63ba0b865c2`; Linux job `95714712460`, macOS job
  `95714712490`, portable job `95714712533`, and aggregate job `95716753364` pass
- final governance artifact `9324852852` has archive SHA-256
  `02eaf91a82789f98bc60e527dbb87b6a59f6fe9f4131d10c5d2ecbf546d53f29` as reported by
  the Actions API. The payload checksums were recomputed with two independent
  implementations and match the artifact's own `SHA256SUMS`, the manifest records
  `dirty: false`, and its recorded input digests match the git tree at that revision
  for the change record, ADR-0014, the release note, `ARCHITECTURE.md`, the plan, the
  status document, and `crates/rumiga-platform/src/lib.rs`
- the hosted coverage differs by crate and is recorded as such. The `rumiga-platform`
  queue tests appear once per host leg, while the three `rumiga-core` keyboard queue
  tests appear twice per leg, once per explicit runtime profile, because the host gate's
  `std` and `no_std` matrix covers `rumiga-core` and not `rumiga-platform`. For the
  contract type itself the bare-metal claim is compilation rather than execution
- the archive SHA-256 values above are GitHub's reported artifact digests. The
  independent verification covers the payload, not the archive container, because the
  API does not serve the archive bytes to a plain token fetch

M1-009 implementation evidence (2026-08-18):

- the machine had no frame counter. `run_frame` incremented nothing, and every frame
  counter in the tree belonged to a shell, so a recording stamped with one would mean
  whatever that shell happened to count. `Emulator::frames_run` is now the replay clock
- events are stamped with emulated frames rather than host time. A wall-clock stamp
  would replay differently on a faster or busier machine, which is the property replay
  exists to remove; ADR-0011 keeps host time out of the core and this is where it pays
- the core records, not the shell. The three input entry points are the only way input
  reaches the machine, so recording inside them makes a recording complete by
  construction rather than by every shell remembering to do it
- `run_frame` applies the current frame's events itself, so the ordering between input
  and emulation is a property of the machine. A shell that applied them at the wrong
  point would produce a different digest from the same recording with nothing flagging it
- two defects were found by the new tests rather than reasoned about. The replay path
  initially reimplemented input application and updated `mouse_dx` and `mouse_dy` but not
  `mouse_x_counter` and `mouse_y_counter`, so a recorded run and its replay differed in
  JOY0DAT, recorded (3, 255) against replayed (0, 0). The fix was to remove the copy:
  there is now one `apply_*` helper per action that both the public method and replay call
- the second defect was in the digest. Two recordings differing only in keycode `0x40`
  against `0x41` reached the same state digest, because the keystroke had already been
  consumed into the CIA serial register, which the digest did not cover. Without that fix
  "the same replay yields the same digest" would have held for the uninteresting reason
  that the digest could not see the difference
- the state digest now covers the frame counter, the keyboard queue contents and its
  dropped count, mouse deltas, counters and buttons, both CIA chips, slow and fast RAM,
  and per-drive metadata. Media contents moved to a separate `media_digest`, because
  hashing a hardfile can cost gigabytes that a caller digesting state per frame should
  not pay
- the recording format is text with a version header, one event per line, so a scenario
  can be hand-written, reviewed, and diffed. Frames must not decrease: a backwards jump
  is rejected rather than sorted, because sorting would hide a corrupted or hand-merged
  file
- on the host, three 300-frame replays of a seven-event recording each reach state digest
  `0x3530b85cc280ec97`, while the same run with no input reaches `0x5446697654ab27f7`.
  All four share frame digest `0x6d7c2de83b7b6725`, because the Kickstart insert-disk
  screen does not react to these inputs. Comparing screenshots would have shown no
  difference and proved nothing, which is precisely why the state digest is separate
- rendered output is unchanged: a 1200-frame A1200 capture keeps digest
  `b190d54b1bbf1e6a9bba3f36d34b74c95ab8fc6fe7796f2f6c694b70165ea1aa`
- conditional, and recorded as such: replay determinism assumes the network is disabled,
  which is the default. The SLIRP backend injects host-received Ethernet frames and those
  are not recorded. A recording also carries no reference to the ROM or disk images it was
  made against, so replaying it against different media is silently a different session;
  the manifest records the media digest so a reader can notice afterwards
- still outside the digest: the copper and blitter shadows beyond `custom_regs`, the audio
  channel state, the floppy MFM track buffers, and the IDE transfer state
- pull-request run
  [`32141958096`](https://github.com/metaneutrons/rumiga/actions/runs/32141958096)
  passes all ten jobs; Linux job `95726506467`, macOS job `95726506342`, portable job
  `95726506509`, and aggregate job `95728140474` pass
- pull-request governance artifact `9326236047`, produced from clean merge revision
  `dc92de4ca0964a662bbcc1efad0b32cb06d037c8`, has archive SHA-256
  `1a4712e02eed556ae3d1b9212339b1e99db67a1347b645a50bf0aa1ba6d74b6a` as reported by
  the Actions API. Its manifest records 15 architecture decisions, 16 release notes, and
  16 change records, so ADR-0015 and the M1-009 record are inside the validated set
- final `main` run
  [`32146258140`](https://github.com/metaneutrons/rumiga/actions/runs/32146258140)
  passes all ten jobs for clean revision
  `bb60a24e8b27fd4a49191736383368768b8c5cb5`; Linux job `95740616196`, macOS job
  `95740616159`, portable job `95740616185`, and aggregate job `95742459679` pass
- final governance artifact `9327891821` has archive SHA-256
  `42a8071589c930d880e2d27970b5d8e684f5bbd154bbf65c8d4972484a93ffd0` as reported by the
  Actions API. The payload checksums were recomputed with two independent implementations
  and match the artifact's own `SHA256SUMS`, the manifest records `dirty: false`, and its
  recorded input digests match the git tree at that revision for the change record,
  ADR-0015, the release note, `ARCHITECTURE.md`, the plan, and the status document
- all ten replay-module tests and all ten emulator-level replay and digest tests appear
  twice per host leg, once per explicit runtime profile. No crate-boundary caveat applies
  here, unlike M1-007 and M1-008: every test in question lives in `rumiga-core`, which the
  host gate's profile matrix covers
- the archive SHA-256 values above are GitHub's reported artifact digests. The independent
  verification covers the payload, not the archive container, because the API does not
  serve the archive bytes to a plain token fetch

M1-010 implementation evidence (2026-08-18):

- the measurement was built before any fix, and that order changed the outcome twice
- reading the source suggested the copper path allocated up to 312 times per frame.
  Measured, the 64-frame fixture reported 64 allocations, one per frame, because
  `Vec::new` does not allocate until the first push and the fixture's copper list
  produced writes on one scanline per frame. The source figure was an upper bound
- the counting allocator comes from outside the workspace. `unsafe_code = "forbid"` is
  set at the workspace root and cannot be relaxed per crate or per target, while a
  counting `#[global_allocator]` needs `unsafe impl GlobalAlloc`. `stats_alloc` is a
  test-only dev-dependency pinned at `=0.1.10`, MIT, inside the supply-chain allowlist
- the emulator also reports its own buffer capacities. A count says something allocated;
  a capacity that stops growing names the buffer, and the accessors work in the `no_std`
  profile and on a device where an allocator hook does not
- after retaining the copper buffer the fixture reported zero allocations while one
  minute of Kickstart 46.143, 3005 frames, still reported 978,521 allocations and
  3,949,644 bytes. The source was `drain_reg_writes().collect()` once per scanline: a
  booting guest writes custom registers on nearly every line, and the ROM-free fixture
  never reached that path
- that is the failure mode this task exists to prevent, and it survived the first round
  of work. A passing allocation test is not an allocation-free loop
- the fixture was strengthened rather than accepted. It now runs a two-instruction 68k
  loop in the guest that writes COLOR00, so guest register writes occur on every
  scanline. With the fix reverted the test fails on the allocation count itself at
  658,944 allocations over 64 frames, not only on the capacity guard
- both capacity guards are load-bearing: the test asserts each buffer is non-zero after
  warmup, so a change that stops reaching a path fails loudly instead of measuring a
  quieter loop
- Clippy suggests `into_iter()` in place of `drain(..)` at both sites. Following it would
  consume the retained buffer and reintroduce the allocation, so the lint is allowed with
  that reason stated at each site
- one minute of PAL, 3005 frames after a 600-frame warmup, now allocates nothing.
  Retained capacities settle at 64 copper entries and 32 early-scanline entries
- behaviour is unchanged: the state digest after 3605 frames is `0xc2d77aefee1ec32c`
  before and after, and the 1200-frame capture keeps digest
  `b190d54b1bbf1e6a9bba3f36d34b74c95ab8fc6fe7796f2f6c694b70165ea1aa`
- two evidence tiers, and the gate enforces the weaker one. The one-minute run needs a
  real Kickstart and ROMs are not committed, so CI runs the 64-frame test and the minute
  figure is recorded from a local run
- not measured: the desktop shell's own per-frame allocations in presentation and
  screenshot paths, which are not the loop a device runs; and peak resident memory, since
  this counts allocation calls rather than footprint
- pull-request run
  [`32170645437`](https://github.com/metaneutrons/rumiga/actions/runs/32170645437)
  passes all ten jobs for the rebased head. The Supply Chain Policy job is the material
  one here, because this task adds the `stats_alloc` dev-dependency
- pull-request governance artifact `9337123534`, produced from clean merge revision
  `565f62d45b8fed97a6dbd52faf406356ca096c00`, has archive SHA-256
  `32974092ad0dc9978eb7d506e512ce7feda1d91245a7082d1e48bdb05cdb77e6` as reported by the
  Actions API
- final `main` run
  [`32171339632`](https://github.com/metaneutrons/rumiga/actions/runs/32171339632)
  passes all ten jobs for clean revision
  `2276344dd5abbf4391d470aca9e0c65ff5a8f70a`
- final governance artifact `9337376831` has archive SHA-256
  `6e8c84fb976d445ded0e3b5266a82cf5461a72da3b545ca639d2f5530a199f30` as reported by the
  Actions API. The payload checksums were recomputed with two independent implementations
  and match the artifact's own `SHA256SUMS`, the manifest records `dirty: false`, and its
  recorded input digests match the git tree at that revision for the change record,
  ADR-0016, the release note, `ARCHITECTURE.md`, the plan, and the status document
- the allocation assertion appears twice per host leg, once per explicit runtime profile.
  The one-minute figure stays a local measurement because ROMs are not committed; the
  enforced claim is the 64-frame test

M1-011 implementation evidence (2026-08-18):

- Miri is answered rather than adopted. It detects undefined behaviour, and the
  workspace sets `unsafe_code = "forbid"` at the root, so no crate contains raw pointer
  arithmetic that could produce any. The two failure modes that actually separate the
  64-bit hosts from the 32-bit device, a truncating `usize` and a native-endian
  conversion, are both well-defined behaviour and invisible to Miri. A Miri leg would
  look like coverage while detecting neither; the plan's property-fixture option is used
- the byte-order property already held and was unenforced. A workspace-wide search found
  zero uses of `from_ne_bytes` or `to_ne_bytes` and 35 explicit big-endian conversions
  across nine core modules. That is a property of today's code, not of the design
- `crates/rumiga-core/clippy.toml` now bans both families through `disallowed_methods`
  and `lib.rs` denies the lint, the same mechanism ADR-0011 used for host clocks and for
  the same reason: a comment does not survive a future contributor, and ADR-0005 rejects
  source-text searches
- `lib.rs` asserts at compile time that `usize` is at least as wide as `u32`, so a build
  for a narrower target fails with a message naming the reason instead of truncating
  every guest address silently, and that a chip RAM length fits in `u32`, which several
  guest pointer masking sites assume
- both invariants were probe-verified. A temporary `u32::from_ne_bytes` was rejected with
  `use of a disallowed method`; a temporary assertion requiring a 128-bit `usize` failed
  the build with its own message. Both probes were removed and the tree re-checked
- the cast audit found no production defect, which is a result rather than missing work.
  The workspace denies `clippy::pedantic`, which includes `cast_possible_truncation`, so
  every lossy cast already carries an explicit `allow`. There are 26 such sites; all are
  narrowings between fixed-width types such as `u32` to `u16`, which behave identically
  at either pointer width, and the one site that multiplies before casting to `usize` is
  a test helper
- the `allow` sites were the right audit list rather than the 166 `as usize` occurrences
  a naive search returns: a `u32 as usize` is lossless at every supported width, so
  treating it as a finding would bury the sites that can lose data
- seven property fixtures cover the boundaries: a guest address across the whole 32-bit
  range including its edges, word and long access through the CPU's own `AddressBus` in
  both directions with individual bytes checked rather than only the round trip, every
  modelled RAM length fitting in `u32`, and the framebuffer index space fitting in 32
  bits
- checking bytes rather than only round trips is deliberate: a round trip alone passes
  under a consistent host-endian implementation, which is exactly the defect
- not claimed: execution with a 32-bit `usize`. No such target with a usable `std` exists
  on the development host, and an `i686` leg would exercise a width the product never
  runs, so the pointer-width claim rests on the compile-time assertions plus the existing
  `riscv32imafc` compile gate. That is weaker than execution and recorded as such
- not enforced: alignment. The core stores guest memory as byte slices and composes wider
  values from bytes, so it makes no alignment assumption to violate; that follows from the
  design rather than from this task
- an assertion about the host's own byte order was considered and rejected. A big-endian
  host would be equally correct and would only make host tests less discriminating;
  failing a build over that would be hostile for no gain
- pull-request run
  [`32172280478`](https://github.com/metaneutrons/rumiga/actions/runs/32172280478)
  passes all ten jobs; governance artifact `9337705867` from clean merge revision
  `065ca28bb1ebe3446c42510deed696e6d449d4e7` has archive SHA-256
  `53de71adb68d03b71316bbe8f37e3d36ba6bb73d1c24334f06b2d35469a8e6bf`
- final `main` run
  [`32174822015`](https://github.com/metaneutrons/rumiga/actions/runs/32174822015)
  passes all ten jobs for clean revision
  `1b85cd490ad98381e736a748fa5bdeea225e6f97`; Linux job `95834173990`, macOS job
  `95834174064`, portable job `95834174094`, and aggregate job `95836306608` pass
- final governance artifact `9338616328` has archive SHA-256
  `56a53971709da61c6df435650bd3c6d7d77b221e19691379a1b3fc0be7565755` as reported by the
  Actions API. The payload checksums were recomputed with two independent implementations
  and match the artifact's own `SHA256SUMS`, the manifest records `dirty: false`, and its
  recorded input digests match the git tree at that revision for the change record,
  ADR-0017, the release note, the plan, the status document, and the core crate root that
  carries the assertions
- all seven fixtures appear twice per host leg, once per explicit runtime profile, and the
  portable job compiles the core for `riscv32imafc-unknown-none-elf`, so the compile-time
  pointer-width assertions are evaluated for the 32-bit target itself rather than only for
  the 64-bit hosts. Execution with a 32-bit `usize` is still not claimed
- the archive SHA-256 values above are GitHub's reported artifact digests; the independent
  verification covers the payload rather than the archive container

M1-012 implementation evidence (2026-08-18):

- the acceptance criterion was checked by nothing. The supply-chain policy constrains
  licences, registries, Git sources, and advisories, and the portable gate compiled the
  core for `riscv32imafc-unknown-none-elf`, but no gate constrained which crates may
  appear in the core dependency graph
- publishing a document alone would have left it that way, which would have made this the
  weakest result in the M1 sequence. The contract is published *and* the criterion is
  enforced
- the portable core graph resolves to exactly `m68k`, `rumiga-core`, and
  `rumiga-platform`, all local, with no third-party crate. `toolchain/manifest.toml`
  therefore declares a closed set rather than an allowlist of approved `no_std` crates
- a closed set is stricter than the plan's wording asks, deliberately. "Approved `no_std`"
  is not a stable property: a crate can gain a `std` path, an allocator assumption, or a
  platform dependency in a patch release, and the portable compile would catch some of
  those and not others
- the portable gate compares the resolved graph against the declaration in both
  directions. An unexpected crate is the motivating case; a declared crate that is absent
  matters too, because a drifted declaration would constrain nothing while still looking
  like a constraint
- both directions are probe-verified. Removing `rumiga-platform` from the declaration
  produced `portable core graph contains crates the manifest does not permit`; adding
  `serde` produced `portable core graph no longer contains declared crates`. Both probes
  edited only the manifest and were reverted, leaving dependencies and `Cargo.lock`
  untouched
- the lockfile gate is a partial defence, not a substitute. A probe adding a real
  dependency failed at `--locked` before the graph check ran. That stops an unintended
  resolution change; it does not stop a dependency committed deliberately together with
  its lockfile update, which is what the graph check catches
- the declaration is pinned by the firmware manifest test as well as by the gate, so
  widening the set fails in two places and appears twice in a reviewer's diff
- `ARCHITECTURE.md` publishes eight rules, each naming the gate, lint, or assertion that
  enforces it and the task it arrived with. Every row was checked against the mechanism it
  names
- two rows state their own weakness rather than reading stronger than they are: the
  pointer-width rules are compile-time assertions evaluated for the target rather than
  tests executed at a 32-bit `usize`, and the allocation rule is enforced over a 64-frame
  fixture with the one-minute figure measured locally, because ROM images are not
  committed
- scope: the contract covers `rumiga-core` and what it pulls in. The desktop shell and the
  ESP platform crate are outside it by design, and the platform crate's own graph is
  covered by the `foundation` portable profile. Build and dev dependencies are excluded
  because they do not ship in the core, which is why M1-010's `stats_alloc` is not part of
  the set
- pull-request run
  [`32178866587`](https://github.com/metaneutrons/rumiga/actions/runs/32178866587)
  passes all ten jobs for the rebased head; governance artifact `9340030504` from clean
  merge revision `d2e2ad4fe5edfc18942e27cb0f16acbd0813216a` has archive SHA-256
  `a584a05a9267dd62a79b406677c8839b445e66314c7a6daf1b7fd0ddf08188fc`
- final `main` run
  [`32179481741`](https://github.com/metaneutrons/rumiga/actions/runs/32179481741)
  passes all ten jobs for clean revision
  `aab34af87fb0f7a7f8c2f44e03833d78507d84bf`; Linux job `95848904724`, macOS job
  `95848904723`, portable job `95848904975`, and aggregate job `95850617417` pass
- final governance artifact `9340243763` has archive SHA-256
  `363cc760b8e0cbe44fdf8d0c525bf42e20b1070c62066a1f70236b8373f18f92` as reported by the
  Actions API. The payload checksums were recomputed with two independent implementations
  and match the artifact's own `SHA256SUMS`, the manifest records `dirty: false`, and its
  recorded input digests match the git tree at that revision for the change record,
  ADR-0018, the release note, `ARCHITECTURE.md`, the plan, and the status document
- the portable job executes the graph comparison in CI, so the closed set resolves the same
  way there as locally rather than being a property of the development host, and both host
  legs run `pins_match_their_consuming_manifests`, whose `assert_target_baseline` helper
  pins the declared profile, root, and crate list. The two mechanisms are independent: the
  gate compares the resolved graph, the test pins what the manifest claims
- the archive SHA-256 values above are GitHub's reported artifact digests; the independent
  verification covers the payload rather than the archive container

### M1 functional commits

1. `refactor(core): define std and no-std runtime profiles`
2. `ci(core): enforce the runtime feature matrix`
3. `docs(core): document the runtime profile contract`
4. `docs(project): close M1-001 with hosted evidence`
5. `refactor(cpu): make stock m68k profiles no-std`
6. `ci(core): enforce riscv no-std portability`
7. `docs(core): document stock cpu portability`
8. `docs(project): close M1-002 with hosted evidence`
9. `refactor(core): enforce portable primitive boundary`
10. `ci(core): enforce portable primitive boundary`
11. `docs(core): document portable primitive boundary`
12. `docs(project): close M1-003 with hosted evidence`
13. `feat(platform): add injected trace sink contract`
14. `refactor(core): move trace file ownership to the desktop adapter`
15. `test(core): pin trace record layout and sink bounds`
16. `docs(core): document the injected trace sink`
17. `docs(project): record trace sink differential evidence`
18. `docs(project): close M1-004 with hosted evidence`
19. `refactor(blitter): restore deterministic single-owner execution`
20. `feat(platform): add capabilities errors and bounded queues`
21. `test(core): add deterministic replay and state digests`
22. `feat(core): make the video standard selectable`
23. `feat(platform): version capabilities and type the error model`
24. `feat(platform): bound queues with a named overflow policy`
25. `feat(core): record and replay input against emulated frames`
26. `perf(core): stop allocating in the scanline loop`
27. `test(core): enforce byte order and pointer-width boundaries`
28. `ci(core): close the portable core dependency graph`

M2-001 implementation evidence (2026-08-18):

- the vendor wiki main page and the product page were both fetched first, and neither
  lists connectors, pin counts, or designators. The connector inventory exists only in
  the schematic, so the manifest was built from that rather than from a specification
  table
- schematic revision is **V01 dated 2025-10-15**, read from the RevisionHistory sheet
  (`2025/10/15  V01  Initail release.`), with the root title block reading `Rev: v01`,
  13 sheets, KiCad `10.0.0-19-g65df3ab11c`, CC BY-SA 4.0
- the downloadable archive and PDF are stamped `260715`. That is a publication date, not
  a revision; recording the filename as the revision would have implied a 2026 revision
  the document does not claim, so the two are recorded separately
- board revision is **Main Board V1.0**, from the archive title
- the BSP is pinned by commit `5074d3b2f45626b261298e305aaf792036febc5a` dated
  2026-04-17. The repository publishes no tags, so a SHA is the only stable reference
- eleven connectors are recorded with designators and manufacturer parts: `USB1`
  Type-C 16 pin doubling as JTAG and power input, `J1` micro-SD with card detect, `J2`
  and `J8` 31 pin 0.3 mm FPC for MIPI-CSI and MIPI-DSI, `J3` 6 pin touch FPC, `J4` SIM,
  `U21` mini-PCIe, `J5` 1x5 header for C6 programming, `J6` speaker, `J7` battery with
  NTC, `J9` SMA
- reading the schematic contradicted the vendor overview twice, and both are left open.
  The wiki advertises "flexible expansion interfaces (GPIO, I2C, UART)", but the only
  2.54 mm header is `J5` carrying the C6's programming signals, and `EXP_GPO0`-
  `EXP_GPO15` belong to `U27`, a `PCA9535RGER` at I2C `0x20` whose outputs drive
  `LCD_PWR_EN`, `LCD_RST`, `TP_RST`, and `EN_PA`. Separately the `/mPCIE&Lora/` sheet
  carries a `Wio-LR1121` module and `J9` that the wiki never mentions
- neither is resolved, deliberately. A schematic cannot say what a shipped unit
  populates, and the absence of a do-not-populate marking is not evidence of population.
  Guessing would put a fabricated fact into the document the rest of M2 will trust
- one existing architecture claim turned out weaker than it read. The 32 MiB PSRAM figure
  the memory budget rests on is a wiki claim about the `ESP32-P4NRW32` variant; the
  schematic symbol carries only the family name. The external flash `W25Q256JVEIQ` does
  corroborate the 32 MB QSPI figure
- the schematic PDF is checksummed, `sha256:c488b1ae...`, and not vendored: it is 2 MB of
  CC BY-SA 4.0 material that Seeed hosts. The manifest states that the checksum was
  computed on download and is not a vendor attestation
- derived values are deliberately not duplicated. Flash layout stays in
  `firmware/partitions.csv`, memory budgets in `ARCHITECTURE.md`, toolchain pins in
  `toolchain/manifest.toml`; a manifest that repeated them would drift invisibly
- not verified: anything against a physical board. No unit has been powered and no
  connector probed, which the manifest states plainly
- not read: the SCH and PCB source archive, the SoC and peripheral datasheets, and the 3D
  model that would answer the dimensions question
- verified by hosted evidence. Pull-request run [`32182892039`](https://github.com/metaneutrons/rumiga/actions/runs/32182892039)
  passes all ten required jobs for merge revision `6abfb978c1ef`,
  and final `main` run [`32183737238`](https://github.com/metaneutrons/rumiga/actions/runs/32183737238)
  passes all ten for `77fefa7992dd`. Both governance artifacts were
  checksum verified with two independent implementations against the artifact's own
  `SHA256SUMS`, both record `dirty` false, and all 72 document digests the governance
  report records match the git tree at the promoted revision
- the `main` run needed a second attempt for two jobs. The Linux host leg and the
  compatibility gate hung in their `apt-get` step for thirty minutes with no output; both
  passed on re-run. Nothing in the change is implicated, and the eight other jobs are from
  the first attempt

## M2 Backlog: D1001 Board Bring-Up

| Task | Status | Deliverable | Acceptance evidence |
| --- | --- | --- | --- |
| M2-001 | DONE | Record D1001 schematic revision, board revision, BSP SHA, and connector inventory | Reviewed hardware manifest under `docs/hardware` |
| M2-002 | PLANNED | Create reproducible ESP-IDF/Rust firmware build using `riscv32imafc-esp-espidf` | CI produces ELF, binary, map, size report, and checksums |
| M2-003 | PLANNED | Define PSRAM allocator, panic, watchdog, logging, and reset policy | Boot manifest reports all values and reset reason |
| M2-004 | PLANNED | Port proven Vellum D1001 services into Rust-first adapters and establish the safety/provenance contract | Exact source-transfer records, narrowly scoped unsafe code, host mocks, and third-party license audit pass |
| M2-005 | PLANNED | Add serial command protocol for capabilities, self-test, metrics, and reset | Versioned protocol test and captured cold-boot log |
| M2-006 | PLANNED | Bring up RGB565 display test pattern and framebuffer checksum | HIL screenshot/checksum artifact |
| M2-007 | PLANNED | Bring up GSL3670 touch and calibration capture | HIL touch-point matrix |
| M2-008 | PLANNED | Bring up ES8311/PCA9535 speaker tone | Frequency/amplitude/underrun artifact |
| M2-009 | PLANNED | Bring up SD/MMC read/write/flush and fault reporting | Fixture file checksum and removal/reinsert test |
| M2-010 | PLANNED | Bring up ESP32-C6 SDIO link and local network smoke | Link/reconnect counters without guest emulation |
| M2-011 | PLANNED | Qualify USB host connector, role, VBUS, hub, keyboard, and mouse | Schematic note plus actual-board enumeration matrix |
| M2-012 | PLANNED | Automate 20 cold boots and board service report | HIL job with zero unexplained resets |
| M2-013 | DONE | Define the product flash partition layout with two OTA slots and a Secure Boot bootloader window | The flashable image carries the repository-owned layout, the bootloader fits its window, and the layout is contiguous, aligned, and fills the configured geometry |
| M2-014 | DONE | Enable flash encryption in a reversible posture and reject any configuration that would burn an eFuse | The gate fails on flash encryption or Secure Boot without virtual eFuses, on release-mode encryption, and on HMAC-based NVS encryption; the manifest records the posture |

M2-013 and M2-014 land before M2-004 in execution order. They are listed last
because task IDs are stable audit labels, not a sequence.

M2-013 verified evidence (2026-08-17):

- `firmware/partitions.csv` owns the product layout: 320 KiB `nvs`, 4 KiB
  `nvs_keys`, 8 KiB `otadata`, 4 KiB `phy_init`, 108 KiB `coredump`, two 6 MiB
  application slots at `0x80000` and `0x680000`, and `storage` last
- `CONFIG_PARTITION_TABLE_OFFSET` moves from `0x8000` to `0x10000`. The stock
  offset left the 24,096-byte bootloader 480 bytes of headroom, which cannot
  hold a Secure Boot V2 signature block; the window is now 57,344 bytes
- the variable-size data partition is last, so both application slots keep
  identical offsets on the configured 16 MB geometry and on the full 32 MB part.
  Qualifying the upper half only extends `storage` from 3.5 MiB to 19.5 MiB and
  does not invalidate an already deployed OTA image
- `esp-idf-sys` documents that a custom table in `sdkconfig.defaults` is ignored
  by its generated CMake project, so the layout is applied when the flashable
  image is generated and the table the ESP-IDF build emits is a build artifact.
  `CONFIG_PARTITION_TABLE_SINGLE_APP_LARGE` keeps that internal table larger
  than the current application
- the evidence task verifies that the merged image embeds the layout declared by
  `firmware/partitions.csv` entry by entry, that the bootloader fits its window,
  and that the application fits its slot; the manifest records the decoded layout
- slot sizing rests on a measured forecast. A bare-metal RISC-V probe that forces
  full instantiation of the emulator measures 540 KiB of code and read-only data
  for `rumiga-core`, `m68k`, and `rumiga-platform`. With ESP-IDF, Wi-Fi, display,
  audio, storage, USB, an HTTP stack, and the 195 KiB gzipped web bundle, a
  feature-complete image is forecast between 1.8 MiB and 3.8 MiB, so a 6 MiB slot
  leaves at least 2.2 MiB of headroom
- `cargo +1.97.1 test --locked -p rumiga-xtask` asserts that the shipped layout
  is contiguous, 4 KiB aligned with 64 KiB aligned application slots, fills the
  configured geometry exactly, and declares two equal-sized OTA slots
- Secure Boot is not enabled in the build. Signed binaries require a private key,
  which must not enter the repository or the evidence bundle; key lifecycle and
  offline signing belong to M10
- local `cargo +1.97.1 xtask ci --gate firmware` passes and reports an
  application occupying 175,040 of 6,291,456 slot bytes
- pull-request run
  [`32046813352`](https://github.com/metaneutrons/rumiga/actions/runs/32046813352)
  passes all ten jobs; Linux job `95436522068`, macOS job `95436522095`, firmware
  job `95436522087`, and aggregate job `95437948183` pass
- its firmware artifact `9293319559` was downloaded and independently verified:
  all nine payload checksums pass, and the eight partitions decoded straight out
  of the flashable image match `firmware/partitions.csv` entry by entry
- pull-request governance artifact `9293179791`, produced from clean merge
  revision `0c2ee6c50dfeccbce32168747eefc3fe09c9297d`, has archive SHA-256
  `24145f2b11d6c3dcc7845f8a4e558b1fe303e4f1004e47788a794da1b7f9e0f4`; all payload
  checksums and the M2-013 traceability record were independently verified
- final `main` run
  [`32047348837`](https://github.com/metaneutrons/rumiga/actions/runs/32047348837)
  passes all ten jobs for clean revision
  `b6579467386d4123773c002157b74fa5d4eeba9f`; Linux job `95438191321`, macOS job
  `95438191377`, firmware job `95438191423`, and aggregate job `95439634504` pass
- final governance artifact `9293361473` has archive SHA-256
  `ba3800552435675429ea659c25856a9dc4423f6bc64b13d88464fec663d0f049`; all payload
  checksums, the clean-source claim, and the M2-013 traceability record were
  independently verified

M2-014 verified evidence (2026-08-17):

- flash encryption is enabled in Development mode together with
  `CONFIG_EFUSE_VIRTUAL`, so no board that boots this firmware is permanently
  altered. Release mode is additionally unselectable while virtual eFuses are on,
  because `SECURE_FLASH_ENCRYPTION_MODE_RELEASE` depends on `!EFUSE_VIRTUAL`
- the evidence task rejects any configuration that could burn an eFuse: flash
  encryption or Secure Boot without virtual eFuses, release-mode flash
  encryption, and NVS encryption through the HMAC scheme, which would consume an
  eFuse key block. The reversibility claim is therefore machine-checked on every
  build rather than documented
- the manifest records the posture as `flash_encryption: development`,
  `secure_boot: disabled`, `nvs_encryption: flash-encryption-scheme`,
  `efuse_virtual: true`, `burns_efuses: false`, and adds the `no-efuse-burn` and
  `encryption-not-enforced` exclusions
- flash encryption implies NVS encryption. ESP-IDF defaults to the HMAC scheme on
  SoCs with an HMAC peripheral, whose eFuse key id defaults to `-1` and fails the
  build with `NVS Encryption (HMAC): Configured eFuse block ... out of range`.
  The flash-encryption scheme instead uses the reserved `nvs_keys` partition and
  consumes no key block. The resolved configuration is AES-128, so one of six key
  blocks would be used for flash encryption and five remain for Secure Boot
- measured bootloader growth: 24,096 bytes without flash encryption, 34,800 bytes
  with it, and 45,056 bytes with Secure Boot V2 additionally enabled and built
  unsigned. The previous `0x8000` table offset gave a 24,576-byte window, so
  flash encryption alone would not have fit; the M2-013 offset move was already
  necessary rather than merely prudent. The 57,344-byte window leaves 22,544
  bytes free today and roughly 8 KiB with Secure Boot and its 4 KiB signature
  block
- the Secure Boot measurement used `SECURE_BOOT_BUILD_SIGNED_BINARIES=n` so that
  no signing key was required, and the configuration was reverted rather than
  committed
- `cargo +1.97.1 test --locked -p rumiga-xtask` covers the accepted posture, an
  absent posture, and all four rejected configurations
- Secure Boot remains disabled in the build. Signed binaries require a private
  key that must not enter the repository or the evidence bundle, and enabling it
  on hardware is irreversible; key lifecycle and offline signing belong to M10
- pull-request run
  [`32049854368`](https://github.com/metaneutrons/rumiga/actions/runs/32049854368)
  passes all ten jobs; Linux job `95446351688`, macOS job `95446351502`, firmware
  job `95446351401`, and aggregate job `95447703666` pass
- its firmware artifact `9294405573` was downloaded and independently verified:
  all payload checksums pass, the manifest reports `efuse_virtual: true` and
  `burns_efuses: false`, and the resolved `sdkconfig` in the bundle confirms
  `CONFIG_EFUSE_VIRTUAL=y`, AES-128, the flash-encryption NVS scheme, and no
  `CONFIG_SECURE_BOOT`
- pull-request governance artifact `9294271555`, produced from clean merge
  revision `0502d8601132cbd483f2b3ee84a4ec5d0895aa80`, has archive SHA-256
  `db92e6e0a918d78368cde2a78f7cd48da2ba19e3f7276a4fb6a63325e04ccff5`; all payload
  checksums and the M2-014 traceability record were independently verified
- final `main` run
  [`32065256994`](https://github.com/metaneutrons/rumiga/actions/runs/32065256994)
  passes all ten jobs for clean revision
  `d4e51779e40d0376f3bd713e328c747c582fae5a`; Linux job `95495707594`, macOS job
  `95495707672`, firmware job `95495707546`, and aggregate job `95497399760` pass
- final governance artifact `9299639002` has archive SHA-256
  `1c5afd8dc30fc599a041bbc7ffa089506c8c89772294003c7b83ae9f2db41acc`; all payload
  checksums, the clean-source claim, and the M2-014 traceability record were
  independently verified
- the pull request for M2-014 had to be created through the REST endpoint because
  GitHub's GraphQL API returned repeated 503 responses during that window; the
  artifact download needed one retry for the same reason. No quality job was
  affected

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
