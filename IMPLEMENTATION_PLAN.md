# Rumiga Implementation Plan

This plan turns `ROADMAP.md` into an executable sequence of functional commits.
It is intentionally evidence-first: a compatibility feature is only complete
when it can be configured, tested, captured, and compared.

## Operating Rules

- Keep commits functional and reviewable. One subsystem or evidence gate per
  commit.
- Do not commit ROMs, Workbench media, generated HDFs, packet captures with real
  traffic, or local screenshots unless they are synthetic or explicitly approved.
- Prefer native framebuffer evidence before host-window screenshots.
- Separate chipset correctness from host presentation controls.
- Every public setting must have matching CLI, REST API, and web UI behavior
  once it becomes user-facing.
- Every milestone must leave the repo in a runnable state.

## Functional Commit Sequence

1. `docs: add implementation plan`
2. `test(evidence): stabilize capture manifest schema`
3. `test(display): add right-edge and first-lines viewport regressions`
4. `fix(display): correct native AGA/Workbench edge wrap`
5. `feat(display): separate native frame, viewport crop, and host scale`
6. `feat(api): expose viewport evidence controls`
7. `test(a1200): add workbench hdf evidence harness`
8. `fix(cia): replace boot timing workaround with tested timer behavior`
9. `feat(storage): harden gayle hdf geometry and writeback policy`
10. `feat(network): add a2065 device skeleton and zorro autoconfig`
11. `feat(network): add slirp backend and packet scheduling`
12. `feat(network): expose network controls in cli api and web ui`
13. `test(network): add amiga tcp evidence scenarios`
14. `docs(release): publish compatibility evidence report`

The exact commit messages can change, but the functional grouping should not.

## Milestone 0: Evidence Foundation

Goal:

Make every emulator run explain itself with a stable manifest and enough
artifacts to debug regressions.

Primary paths:

- `desktop/src/main.rs`
- `crates/rumiga-core/src/emulator.rs`
- `crates/rumiga-api/src/lib.rs`
- `web/src/lib/api.ts`
- `crates/rumiga-core/tests`

Tasks:

- Promote the current capture manifest into a stable schema with a version
  number.
- Add explicit fields for native framebuffer width/height, host viewport preset,
  crop rectangle, scale mode, PAL/NTSC mode, machine model, CPU profile, frame
  count, PC/SR, ROM hash, media hashes, dirty writeback state, and git SHA.
- Add a small manifest validator test that fails on accidental field removal.
- Add evidence output naming rules:
  - `artifacts/evidence/<scenario>/<timestamp>/rumiga.png`
  - `artifacts/evidence/<scenario>/<timestamp>/rumiga.json`
  - `artifacts/evidence/<scenario>/<timestamp>/notes.md`
- Add asset-skip behavior for tests that need local ROMs or Workbench media.

Acceptance:

- `cargo test -p rumiga-api`
- `cargo test -p rumiga-core`
- A headless capture produces a manifest with the new schema version.
- Re-running the same fixed-frame capture produces stable non-time fields.

## Milestone 1: Viewport and Native Frame Correctness

Goal:

Fix the visible Workbench issue: pixels from the right edge appearing on the
left, uneven border behavior, and bottom crop confusion.

Primary paths:

- `crates/rumiga-core/src/playfield.rs`
- `crates/rumiga-core/src/custom.rs`
- `crates/rumiga-core/src/chipset.rs`
- `crates/rumiga-core/src/copper.rs`
- `crates/rumiga-core/src/sprites.rs`
- `desktop/src/main.rs`
- `crates/rumiga-api/src/lib.rs`
- `web/src/app/machine/page.tsx`

Tasks:

- Add a native-frame inspection helper that can scan the first visible lines and
  both horizontal edges.
- Add regression tests for:
  - no right-edge pixels injected at x=0
  - no left-edge pixels duplicated at the right edge
  - stable line width across the first 20 visible lines
  - bottom visible line not hidden by host crop
- Audit the bitplane fetch to framebuffer write path for modulo/wrap behavior.
- Audit DIW/DDF handling against WinUAE/FS-UAE reference behavior.
- Split presentation into three explicit layers:
  - native chipset framebuffer
  - viewport crop/center policy
  - host scale/aspect policy
- Add named viewport presets:
  - `native-full-border`
  - `visible-area`
  - `overscan`
  - `auto-center`
  - `integer-scale`
  - `aspect-correct`
  - `stretch`
- Ensure REST and web UI can round-trip the selected viewport preset.

Acceptance:

- A1200 Workbench native capture has no edge wrap.
- Host-window screenshot can be stretched or aspect-correct without changing the
  native capture.
- The user-reported first-20-lines bug has an automated regression test.
- Existing A500 insert-hand capture still renders correctly.

## Milestone 2: A1200 Workbench 3.1.4 HDF Evidence Pack

Goal:

Make A1200 Workbench 3.1.4 HDF boot the first enterprise evidence pack.

Primary command:

```sh
cargo run --release -p rumiga-desktop --bin rumiga-desktop -- \
  --model a1200 \
  --cpu 68020 \
  --hdf assets/workbench-39.hdf \
  assets/kick.a1200.47.102.rom
```

Tasks:

- Convert the current manual boot command into a reproducible evidence scenario.
- Add a scenario config file without embedding local-only absolute paths.
- Capture native screenshot, host screenshot, manifest, and notes.
- Compare against FS-UAE reference output created from the same ROM/HDF inputs.
- Record boot milestone states:
  - Kickstart screen visible
  - HDF detected
  - Workbench loaded
  - System requester state if libraries are missing
- Make the missing `LIBS/Workbench.library` case a classified media/config
  outcome, not an emulator failure.

Acceptance:

- Scenario can run locally with asset env vars or a local asset config.
- Manifest proves model, CPU, HDF, ROM, viewport, and frame count.
- Screenshot shows correct Workbench geometry without edge wrap or bottom crop.
- Evidence notes explain whether the result is full Workbench or a media
  requester state.

## Milestone 3: Scheduler, CIA, and Boot Timing

Goal:

Remove temporary boot workarounds and replace them with tested timer,
interrupt, and DMA ordering behavior.

Primary paths:

- `crates/rumiga-core/src/emulator.rs`
- `crates/rumiga-core/src/cia.rs`
- `crates/rumiga-core/src/events.rs`
- `crates/rumiga-core/src/blitter.rs`
- `crates/rumiga-core/src/copper.rs`
- `crates/rumiga-core/src/floppy.rs`

Tasks:

- Locate and eliminate the forced CIA timer threshold workaround once tests
  explain the required behavior.
- Add CIA timer A/B tests for one-shot, continuous, reload, ICR, IRQ masking,
  TOD basics, and keyboard handshake.
- Add scheduler counters for CPU cycles, copper waits, blitter active cycles,
  bitplane DMA, floppy DMA, and interrupt delivery.
- Add boot regressions that fail if scheduler shortcuts reappear.
- Compare suspicious behavior against WinUAE/FS-UAE `cia.cpp` and scheduler
  ordering.

Acceptance:

- Kickstart 1.3 and A1200 Workbench evidence still boot without the workaround.
- CIA timer tests cover the boot-sensitive path.
- Manifest includes scheduler counters useful for future regressions.

## Milestone 4: Floppy and Trackdisk Hardening

Goal:

Make ADF boot, speed-up, and writeback trustworthy.

Primary paths:

- `crates/rumiga-core/src/floppy.rs`
- `desktop/src/main.rs`
- `crates/rumiga-api/src/lib.rs`
- `web/src/app/machine/page.tsx`

Tasks:

- Add tests for DSKSYNC, DSKBYTR, index, ready, disk change, side select, motor,
  write protect, and DMA transitions.
- Validate speed modes: 100, 200, 400, 800 percent, and turbo if enabled.
- Add writeback tests using temporary ADF images and final hash checks.
- Ensure read-only mode is default for user media unless explicitly changed.
- Expose speed and writeback policy consistently across CLI, REST, and web UI.

Acceptance:

- Workbench ADF boot passes at compatible speed.
- Speed-up modes do not alter guest-visible disk content.
- Writeback tests prove source media is protected unless writeback is enabled.

## Milestone 5: Gayle IDE and HDF Safety

Goal:

Make A600/A1200 HDF boot and writeback reliable enough for daily use.

Primary paths:

- `crates/rumiga-core/src/ide.rs`
- `crates/rumiga-core/src/memory.rs`
- `desktop/src/main.rs`
- `crates/rumiga-api/src/lib.rs`

Tasks:

- Add RDB parsing and validation before relying on guessed CHS geometry.
- Keep CHS fallback explicit and visible in the manifest.
- Add ATA command tests for identify, read, write, status, IRQ, and error paths.
- Add read-only HDF mode and explicit writeback mode.
- Add flush behavior and dirty-state reporting.
- Add HDF snapshot/diff helper for evidence runs.

Acceptance:

- A600 and A1200 HDF scenarios boot with known-good images.
- Bad or unsupported HDF geometry fails with a precise error.
- Writeback path is covered by temporary-image tests.

## Milestone 6: A2065 Network Support

Goal:

Add one real Amiga-compatible network device with a safe default backend.

Primary new paths:

- `crates/rumiga-core/src/network.rs`
- `crates/rumiga-core/src/a2065.rs`
- `crates/rumiga-core/src/slirp.rs` or a desktop backend module if libslirp is
  host-only

Primary existing paths:

- `crates/rumiga-core/src/memory.rs`
- `desktop/src/main.rs`
- `crates/rumiga-api/src/lib.rs`
- `web/src/app/machine/page.tsx`
- `web/src/lib/api.ts`

Implementation steps:

1. Add network configuration types to `rumiga-api`.
2. Add network-off default to CLI, REST, and web UI.
3. Add Zorro II autoconfig shell for a single A2065-compatible device.
4. Implement RAP/RDP/CSR register model and reset behavior.
5. Implement descriptor rings and guest memory DMA reads/writes.
6. Add transmit path with a fake loopback backend for unit tests.
7. Add receive path with deterministic queued packets for unit tests.
8. Add interrupts and missed-interrupt regression tests.
9. Add SLIRP/NAT backend for desktop.
10. Add optional packet capture against local test fixtures.
11. Add Amiga-side evidence scenarios for static IP, ping, DNS, HTTP fetch, and
    sustained transfer.

Acceptance:

- Network is disabled by default.
- API can enable A2065 + SLIRP with explicit MAC address.
- Amiga OS driver detects the card.
- Guest can ping gateway and fetch a local HTTP resource.
- Packet counters and link state appear in the manifest.
- No bridged/pcap mode is enabled without explicit user choice.

## Milestone 7: API and Web UI Parity

Goal:

Make the emulator scriptable and controllable without hidden desktop-only
behavior.

Primary paths:

- `crates/rumiga-api/src/lib.rs`
- `desktop/src/main.rs`
- `web/src/lib/api.ts`
- `web/src/app/page.tsx`
- `web/src/app/machine/page.tsx`
- `web/src/app/files/page.tsx`

Tasks:

- Version API responses and errors.
- Add contract tests for machine config, status, start, stop, reset, screenshot,
  floppy insert/eject, HDF mount, viewport, audio, and network.
- Align TypeScript API types with Rust API types.
- Add UI controls only for stable backend features.
- Add support-bundle endpoint or CLI command that collects manifest, logs, and
  screenshots without ROM/media.

Acceptance:

- API tests fail if Rust and TypeScript DTOs drift.
- Every completed user-facing feature is reachable from CLI, REST, and web UI.
- Screenshot endpoint clearly identifies native or host presentation capture.

## Milestone 8: Compatibility Corpus and Release Reports

Goal:

Turn compatibility into an auditable release artifact.

Tasks:

- Define curated software corpus:
  - 20 OCS titles
  - 10 ECS titles
  - 20 AGA titles
  - Workbench 1.3, 2.x, 3.1, 3.1.4 scenarios
  - Network stack scenarios
- Add scenario metadata with legal asset notes and local path placeholders.
- Generate a compatibility report per release candidate.
- Track each scenario as pass, partial, fail, skipped-missing-assets, or
  unsupported-out-of-scope.

Acceptance:

- Release report is generated from evidence manifests.
- Failures link to reproduction commands.
- Known unsupported features are separated from regressions.

## First Execution Sprint

The next implementation sprint should be narrowly focused:

1. Add manifest schema versioning and validator tests.
2. Add native-frame edge inspection helpers.
3. Add first-20-lines/right-edge regression test.
4. Fix native frame wrap if the test reproduces the current screenshot issue.
5. Add A1200 HDF evidence scenario with local asset config.
6. Expose viewport preset round-trip through API and web UI.

Exit criteria:

- The reported "right edge appears on the left" bug is either fixed or pinned to
  one exact subsystem with a failing regression test.
- A1200 Workbench HDF evidence can be regenerated from one documented command.
- ROADMAP and this plan agree on the next milestone.

## Validation Commands

Use these before each functional commit:

```sh
cargo fmt --all --check
cargo test -p rumiga-api
cargo test -p rumiga-core
cargo test -p rumiga-desktop
```

Use this when touching the web UI:

```sh
cd web
npm run lint
npm run build
```

Use this before release-candidate claims:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Known caveat:

- The repository currently still references legacy `r68k` paths during some
  workspace or hook operations, and those emit many pre-existing warnings. Track
  this separately so parity work is not buried under legacy warning noise.
