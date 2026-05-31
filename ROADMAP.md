# Rumiga WinUAE-Parity Roadmap

This roadmap defines a practical, enterprise-grade compatibility target for
Rumiga. The goal is not to clone every WinUAE feature. The goal is to make
Rumiga a trustworthy classic Amiga emulator for the stock machines we care
about, with repeatable evidence, predictable configuration, and a clean path to
network support.

The roadmap is based on the current Rumiga implementation and reference analysis
of WinUAE and FS-UAE. WinUAE remains the behavior reference for chipset,
scheduler, CIA, storage, and network-device semantics. FS-UAE is the primary
macOS-friendly operational reference, especially for local evidence runs and
SLIRP-style networking.

## Product Goal

Rumiga should reach high-confidence feature parity for these classic profiles:

- Amiga 500: Kickstart 1.2/1.3, OCS, 68000, chip/slow RAM, floppy boot.
- Amiga 500+: Kickstart 2.x, ECS, 68000, chip RAM, floppy boot.
- Amiga 600: ECS, 68000, Gayle IDE, PCMCIA address behavior, floppy and HDF
  boot.
- Amiga 1200: AGA, 68EC020-class stock profile, Gayle IDE, floppy and HDF boot.
- Desktop host: stable UI, REST API, screenshots, deterministic capture, runtime
  configuration, and evidence export.
- Network support: one supported WinUAE/FS-UAE-compatible virtual NIC path,
  preferably A2065-compatible Zorro II Ethernet with SLIRP/NAT backend first and
  optional host bridge/pcap later.

The target excludes accelerator and exotic expansion-board compatibility:

- No PPC, JIT, accelerator-board RAM, 68060 accelerator profiles, or board ROMs.
- No SCSI controller support, CDTV/CD32/Akiko target, RTG/Picasso96 target, or
  graphics/sound expansion-card parity.
- No broad network-card matrix. A2065-compatible Ethernet is the single blessed
  network device unless future evidence shows a better compatibility tradeoff.
- No raw-flux or IPF copy-protection target in the first enterprise milestone.
  Standard ADF, writeback ADF, and Gayle IDE HDF/RDB are the supported storage
  path.

## Current Starting Point

Rumiga already has meaningful foundations:

- Rust workspace with `m68k`, `rumiga-core`, desktop, API, web, and ESP platform
  crates.
- Machine profiles for A500/A500+/A600/A1200 and selectable CPU profiles.
- Chip, slow, fast, ROM, custom register, CIA, Gayle, and IDE address paths.
- Progressive OCS/ECS/AGA display work including bitplanes, palettes, sprites,
  HAM paths, and viewport controls.
- Floppy trackdisk work with MFM streaming, DMA gates, dirty writeback, and
  speed controls up to WinUAE-style 800 percent.
- Gayle IDE HDF boot path with basic ATA behavior.
- Desktop control surface, REST API shape, web UI integration, screenshots, and
  headless capture manifests.
- Paula audio and basic input support.

The next step is to turn these capabilities into a compatibility program with
clear baselines, reference traces, failure classification, and release gates.

## Reference Map

Use these local references when implementing or reviewing parity work:

- WinUAE:
  - `/Volumes/Dev/Source/WinUAE/custom.cpp`
  - `/Volumes/Dev/Source/WinUAE/cia.cpp`
  - `/Volumes/Dev/Source/WinUAE/disk.cpp`
  - `/Volumes/Dev/Source/WinUAE/ide.cpp`
  - `/Volumes/Dev/Source/WinUAE/a2065.cpp`
  - `/Volumes/Dev/Source/WinUAE/bsdsocket.cpp`
  - `/Volumes/Dev/Source/WinUAE/include/options.h`
  - `/Volumes/Dev/Source/WinUAE/include/ethernet.h`
- FS-UAE:
  - `/Volumes/Dev/Source/fs-uae/src/custom.cpp`
  - `/Volumes/Dev/Source/fs-uae/src/cia.cpp`
  - `/Volumes/Dev/Source/fs-uae/src/disk.cpp`
  - `/Volumes/Dev/Source/fs-uae/src/a2065.cpp`
  - `/Volumes/Dev/Source/fs-uae/src/slirp_uae.cpp`
- Rumiga:
  - `/Volumes/Dev/Source/rumiga/crates/rumiga-core/src/emulator.rs`
  - `/Volumes/Dev/Source/rumiga/crates/rumiga-core/src/memory.rs`
  - `/Volumes/Dev/Source/rumiga/crates/rumiga-core/src/playfield.rs`
  - `/Volumes/Dev/Source/rumiga/crates/rumiga-core/src/copper.rs`
  - `/Volumes/Dev/Source/rumiga/crates/rumiga-core/src/blitter.rs`
  - `/Volumes/Dev/Source/rumiga/crates/rumiga-core/src/floppy.rs`
  - `/Volumes/Dev/Source/rumiga/crates/rumiga-core/src/ide.rs`
  - `/Volumes/Dev/Source/rumiga/crates/rumiga-core/src/cia.rs`
  - `/Volumes/Dev/Source/rumiga/crates/rumiga-core/src/audio.rs`
  - `/Volumes/Dev/Source/rumiga/desktop`
  - `/Volumes/Dev/Source/rumiga/rumiga-api`
  - `/Volumes/Dev/Source/rumiga/web`
  - `/Volumes/Dev/Source/rumiga/crates/rumiga-platform-esp`

## Compatibility Tiers

### Tier 0: Evidence Infrastructure

This tier makes every later claim measurable.

Deliverables:

- Headless run mode that produces screenshot, frame metadata, machine profile,
  ROM/media hashes, emulator git SHA, timing mode, and relevant configuration.
- Stable screenshot format for native framebuffer capture before host scaling.
- Optional host-window screenshot for UI viewport validation.
- Reference-run folders for WinUAE/FS-UAE evidence where legal local ROM/media
  paths are supplied by the developer.
- Golden manifest schema for boot success, frame count, disk/HDF hashes, CPU
  counters, interrupt counters, and custom-register snapshots.
- CI-compatible tests that skip ROM/media evidence cleanly when assets are not
  present.

Evidence gates:

- `cargo test --workspace` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes or has an
  explicitly tracked exception list.
- Headless capture produces identical manifest fields across repeated runs with
  the same inputs.
- Every compatibility bug has a reproduction command and at least one captured
  artifact.

### Tier 1: A500 OCS Baseline

Target:

- Kickstart 1.3 insert-hand screen.
- Workbench 1.3 floppy boot.
- Common OCS games and demos that exercise copper, bitplanes, sprites, blitter,
  CIA timers, joystick, mouse, and floppy timing.

Deliverables:

- Correct PAL/NTSC model defaults and explicit override controls.
- Accurate OCS viewport, border, DIW/DDF interaction, and host scaling.
- Stable DSKSYNC/DSKBYTR behavior and disk-change semantics.
- Keyboard, joystick, mouse, and basic serial/parallel register behavior.
- Boot-time correctness without forced CIA timer workarounds.

Evidence gates:

- Kickstart 1.3 insert-hand screenshot matches reference within agreed visual
  tolerance.
- Workbench 1.3 reaches usable desktop from ADF.
- At least 20 curated OCS software titles reach documented milestones.
- No right-edge wraparound, left-edge ghosting, or bottom crop in native capture.

### Tier 2: ECS/A600 Baseline

Target:

- A500+ and A600 profiles with Kickstart 2.x.
- ECS display behavior, enhanced Denise edge cases, and Gayle address behavior.
- Gayle IDE boot for A600-style HDF.

Deliverables:

- ECS register coverage required by Workbench 2.x and common ECS software.
- PCMCIA address/open-bus behavior sufficient for OS compatibility, even if
  PCMCIA devices are not yet emulated.
- A600 Gayle IDE path with RDB-aware disk mounting, geometry detection, and
  writeback safety.

Evidence gates:

- A500+ Workbench 2.x ADF boots.
- A600 HDF boots from Gayle IDE.
- ECS viewport and border tests pass against FS-UAE/WinUAE reference captures.

### Tier 3: A1200 AGA Baseline

Target:

- A1200 stock profile with 68EC020-class CPU behavior.
- Workbench 3.1 and 3.1.4 boot from ADF and HDF.
- AGA native display modes without RTG.

Deliverables:

- AGA bitplane, palette banking, BPLCON3/BPLCON4, FMODE, HAM8, sprites, and
  fetch behavior required by Workbench and representative AGA software.
- Correct PAL/NTSC and overscan handling with configurable host viewport.
- Interlace and high-resolution modes with predictable aspect and scaling.
- No host scaling setting should modify native chipset state.

Evidence gates:

- A1200 Kickstart insert screen renders correctly.
- Workbench 3.1 and 3.1.4 boot from known-good ADF/HDF assets.
- AGA screen modes render without right-edge wrap, left-edge injected pixels, or
  bottom crop in native capture.
- At least 20 curated AGA titles reach documented milestones.

### Tier 4: Network Support

Target:

- WinUAE/FS-UAE-style A2065-compatible Ethernet as the first supported Amiga
  network device.
- SLIRP/NAT backend first for safe default networking.
- Optional host bridge/pcap backend later for advanced users.
- Optional bsdsocket-style host integration only after hardware NIC parity is
  reliable.

Rationale:

- A2065 is a known Amiga Ethernet device with mature WinUAE/FS-UAE reference
  behavior.
- It gives real Amiga OS drivers a hardware target instead of inventing a
  Rumiga-only interface.
- SLIRP avoids privileged host networking and is a safer default for desktop and
  web-controlled runs.

Deliverables:

- Zorro II autoconfig for a single A2065-compatible card.
- LANCE-style CSR/RAP/RDP register behavior, descriptor rings, interrupts,
  transmit, receive, multicast/broadcast filtering, MAC address handling, and
  reset behavior.
- SLIRP backend with deterministic event integration into the emulator scheduler.
- REST API and web UI controls for enabling/disabling network, backend type, MAC
  address, NAT port forwards, and link state.
- Packet capture evidence mode for debugging, redacted when needed.
- Security defaults: network off unless requested, no inbound host exposure
  unless explicitly configured, clear UI/API warning for bridged mode.

Evidence gates:

- Amiga OS driver detects the card.
- Static IP works.
- DHCP/BOOTP works if supported by the selected Amiga-side stack.
- Guest can ping SLIRP gateway.
- Guest can resolve DNS through the backend.
- Guest can fetch a known HTTP resource and validate checksum.
- Sustained transfer test passes without descriptor leaks, missed interrupts, or
  emulator stalls.
- Network settings round-trip through CLI, REST API, and web UI.

### Tier 5: Desktop, REST, Web UI, and ESP

Target:

- Desktop app is the primary local emulator front end.
- REST API and web UI expose the same operational controls.
- ESP target remains a constrained-port target with clearly defined feature
  support rather than pretending to match desktop.

Deliverables:

- Runtime controls for machine model, CPU profile, PAL/NTSC, viewport, scaling,
  border policy, floppy speed, disk insert/eject, HDF mount, audio, input,
  screenshot, pause/resume/reset, and network.
- REST API schemas with versioning and error contracts.
- Web UI controls that map one-to-one to stable API operations.
- ESP stubs replaced with explicit capability reports and implemented features
  where hardware allows.
- Failure states visible in logs, API responses, and UI.

Evidence gates:

- CLI, desktop UI, REST API, and web UI can start the same machine profile.
- Screenshot and manifest capture are available from CLI and API.
- Viewport settings can be changed without corrupting native framebuffer state.
- API contract tests cover every public endpoint.

## Feature Workstreams

### 1. Deterministic Scheduler and DMA

Why it matters:

WinUAE compatibility is built on strict ordering of CPU, copper, blitter,
bitplane DMA, sprite DMA, audio DMA, CIA, disk, and interrupts. Many visible
bugs are scheduler bugs wearing a display costume.

Tasks:

- Define a cycle-domain model for PAL and NTSC.
- Track custom-chip DMA slots explicitly enough for compatibility.
- Make copper waits/skips, blitter completion, and CPU bus stealing observable
  in tests.
- Remove temporary timing hacks by replacing them with evidence-backed behavior.
- Add per-frame scheduler counters to manifests.

Pitfalls:

- Immediate blitter completion can make Workbench look alive while breaking
  games and demos.
- CIA timer shortcuts can hide boot issues and then fail under real software.
- PAL/NTSC mismatches can masquerade as viewport bugs.

### 2. CPU and Exception Semantics

Why it matters:

Workbench and most games tolerate some CPU timing drift. Copy protection,
debuggers, demos, and 68020+ OS paths do not.

Tasks:

- Lock stock profile expectations: A500/A600 use 68000, A1200 uses 68EC020-class
  behavior.
- Treat 68030/68040 modes as diagnostic or future non-stock profiles unless
  explicitly promoted.
- Expand instruction, flag, prefetch, exception, bus error, and address error
  tests.
- Document unsupported FPU/MMU behavior clearly.

Pitfalls:

- 68020+ exception stack frames affect real software.
- Prefetch and PC reporting bugs can break loaders and debuggers.
- FPU/MMU stubs must fail predictably instead of producing silent corruption.

### 3. Display, Viewport, and Scaling

Why it matters:

The current user-visible pain is display correctness: right-edge pixels wrapping
to the left, uneven gray border, bottom crop, and confusing vertical stretching.
WinUAE separates chipset display generation from host filter, border, autoscale,
and aspect controls. Rumiga needs the same conceptual separation.

Tasks:

- Preserve a native chipset framebuffer with exact beam/display-window behavior.
- Model DIWSTRT, DIWSTOP, DIWHIGH, DDFSTRT, DDFSTOP, BPLCON registers, and
  fetch alignment as chipset behavior.
- Implement host viewport as a separate crop/scale/filter layer.
- Provide named viewport presets: native full border, visible area, overscan,
  auto center, integer scale, aspect-correct, stretch.
- Make border crop adjustable and reversible from CLI, REST API, and web UI.
- Add first-20-lines and right-edge regression tests for wraparound bugs.

Pitfalls:

- Cropping cannot fix a native framebuffer wrap bug.
- Stretching is a host presentation option, not a chipset fix.
- RTG/P96 has no classic chipset border problem, but RTG is out of scope for the
  main target.
- OCS/ECS/AGA and PAL/NTSC have subtle edge differences. Do not hardcode one
  Workbench screenshot as universal truth.

Evidence:

- Native capture proves no wraparound before host scaling.
- Host-window capture proves the selected viewport preset presents correctly.
- Reference captures from FS-UAE/WinUAE are stored with the same ROM/media hash
  metadata.

### 4. Floppy and Trackdisk

Why it matters:

ADF boot and trackdisk behavior are the first compatibility gate for A500 and
Workbench installs.

Tasks:

- Keep per-word MFM streaming and DSKSYNC behavior observable.
- Validate DMA enable/disable transitions, index pulses, disk ready, disk change,
  write protect, side select, motor behavior, and interrupts.
- Keep 100/200/400/800 percent speed modes, plus turbo mode if intentionally
  supported.
- Ensure speed-up modes do not change guest-visible semantics beyond timing.
- Add writeback tests with temporary disk images and hash verification.

Pitfalls:

- Fast floppy mode can break software that relies on realistic ready/index
  timing.
- ADF is not raw flux. Do not claim copy-protected disk parity from ADF tests.
- Dirty writeback must never mutate source media accidentally in read-only runs.

### 5. Gayle IDE and HDF

Why it matters:

A600/A1200 Workbench practicality depends on reliable HDF boot and safe writes.

Tasks:

- Implement RDB-aware HDF mounting and geometry detection.
- Support common ATA commands used by Amiga OS and installers.
- Define writeback flushing and crash-safety behavior.
- Add HDF snapshot and diff tooling for evidence.
- Keep SCSI controller support out of scope.

Pitfalls:

- CHS guessing can boot one image and corrupt another.
- RDB parsing needs strong validation and explicit error messages.
- Host file writes need atomicity and clear read-only mode.

### 6. Audio and Input

Why it matters:

Software compatibility depends on Paula interrupts, DMA timing, mouse quadrature,
joystick bits, and raw keyboard behavior, not just audible sound.

Tasks:

- Validate four Paula channels, period reloads, volume changes, DMA start/stop,
  interrupts, stereo separation, and LED filter.
- Add audio underrun metrics to manifests.
- Implement raw Amiga keycode mapping with keyboard handshake tests.
- Add mouse quadrature tests and joystick state tests.
- Expose audio/input settings through CLI, REST API, and web UI.

Pitfalls:

- Host key layout and Amiga raw keycodes are separate concerns.
- Mouse acceleration belongs in host/UI policy, not CIA register behavior.
- Audio buffer smoothing can hide timing bugs.

### 7. Network

Why it matters:

Network support is explicitly in scope even though most other expansion boards
are not. This must be done as a real Amiga-compatible device, not as a
Rumiga-only shortcut.

Tasks:

- Implement A2065-compatible Zorro II autoconfig.
- Implement register and descriptor behavior based on WinUAE/FS-UAE references.
- Add SLIRP/NAT backend with guest-to-host event pumping.
- Add optional pcap/bridge backend behind explicit permissions and warnings.
- Add API/UI controls and evidence capture for packets and link state.
- Define packet redaction policy for logs and test artifacts.

Pitfalls:

- MAC address expectations matter for some Amiga drivers.
- Missed interrupt behavior causes intermittent network stalls.
- Host networking is security-sensitive and must default to off.
- Bridged networking may need elevated host permissions and cannot be assumed in
  CI.

### 8. API, Web UI, and Operations

Why it matters:

Enterprise grade means the emulator is operable, observable, testable, and
scriptable.

Tasks:

- Version REST API schemas.
- Add contract tests for all endpoints.
- Keep CLI, REST, and web UI configuration names aligned.
- Add structured logs for machine profile, media mount, viewport, network,
  speed, and error states.
- Provide a support bundle command that collects manifest, config, logs, and
  screenshots without bundling copyrighted ROM/media.

Pitfalls:

- Web-only settings can drift from CLI behavior.
- Screenshots must identify whether they are native framebuffer captures or host
  presentation captures.
- Error messages should name the failing subsystem and likely corrective action.

## Testing Strategy

### Test Layers

- Unit tests: pure register behavior, instruction semantics, MFM encoding,
  palette conversion, blitter math, ATA command handling, CIA timer behavior,
  A2065 descriptors, and API schemas.
- Integration tests: boot loops, media insertion, HDF reads/writes, network
  packet flow, audio/input event flow, and REST operations.
- Headless evidence tests: run fixed frame budgets and capture screenshot plus
  manifest.
- Reference comparison tests: compare Rumiga artifacts to FS-UAE/WinUAE artifacts
  using hashes where deterministic and perceptual metrics where presentation can
  differ.
- Manual acceptance tests: reserved for interactive UI, audio perception,
  network bridge permissions, and copyrighted media that cannot run in CI.

### Evidence Matrix

Required scenarios:

- A500 Kickstart 1.3 no-disk insert screen.
- A500 Workbench 1.3 ADF boot.
- A500 OCS game/demo set.
- A500+ Workbench 2.x ADF boot.
- A600 Gayle IDE HDF boot.
- A1200 Kickstart 3.x no-disk insert screen.
- A1200 Workbench 3.1 ADF boot.
- A1200 Workbench 3.1.4 HDF boot.
- AGA title set with high-color, HAM8, sprites, and scroll tests.
- Floppy speed matrix: 100, 200, 400, 800 percent on safe boot/install tests.
- Network static IP: ping gateway, DNS lookup, HTTP fetch, checksum validation.
- Network sustained transfer: large download/upload loop with packet counters.
- REST/Web UI: configure, run, pause, reset, screenshot, insert/eject media,
  change viewport, change floppy speed, toggle network.

Each evidence run must record:

- Rumiga git SHA and dirty flag.
- Host OS and architecture.
- Machine model, CPU profile, PAL/NTSC mode, RAM sizes, and chipset mode.
- ROM path hash, media path hashes, and writeback policy.
- Frame count, elapsed host time, emulated time, and speed factor.
- Screenshot path and native framebuffer dimensions.
- Viewport preset and host scaling settings.
- Disk/HDF dirty state and output hashes when writes are enabled.
- Network backend, MAC address, packet counters, and redacted endpoint summary.
- Pass/fail milestone and human-readable notes.

### Visual Comparison Rules

- Native framebuffer capture is the primary correctness artifact.
- Host-window screenshots validate presentation only.
- Border color, crop, and aspect must be evaluated separately from chipset pixel
  generation.
- A wraparound bug is any pixel group from the right edge appearing on the left
  edge in native capture.
- A viewport bug is any valid native content hidden or shifted by host crop/scale
  settings.
- A scaling bug is any incorrect aspect/stretch behavior after native capture is
  known good.

### Network Test Rules

- CI default uses SLIRP/NAT and local test servers.
- Bridge/pcap tests are opt-in because they may require host permissions.
- Tests must avoid leaking external traffic by default.
- Packet captures are redacted or generated only against local fixtures unless
  explicitly requested.
- Network is off by default for normal emulator launches.

### Release Gates

A release candidate cannot be called enterprise grade until:

- All unit and integration tests pass.
- Evidence matrix has current artifacts for the targeted compatibility tier.
- New regressions are either fixed or documented with owner, severity, and
  expected fix milestone.
- CLI, REST, and web UI expose the same user-facing controls for completed
  features.
- All completed features have docs, tests, and failure-mode behavior.
- Copyrighted ROMs, Workbench disks, and HDFs are not committed or embedded.

## Definition of Done for a Feature

A feature is not done when it first boots once. It is done when all of these are
true:

- The WinUAE/FS-UAE reference behavior has been identified and summarized.
- The Rumiga implementation has focused unit tests.
- At least one integration or headless evidence test covers the behavior.
- User-facing controls are wired through CLI, REST API, and web UI when relevant.
- Failure modes are explicit and testable.
- Logs and manifests include enough detail to debug regressions.
- Performance impact is measured.
- Documentation explains scope, unsupported cases, and known limitations.

## Priority Order

1. Stabilize evidence infrastructure and native screenshot capture.
2. Fix native display edge correctness before adding more scaling options.
3. Remove or replace timing workarounds with scheduler/CIA evidence.
4. Lock A500 Kickstart 1.3 and Workbench 1.3 baselines.
5. Lock A1200 Workbench 3.1/3.1.4 HDF boot with correct viewport.
6. Harden Gayle IDE and RDB/HDF write safety.
7. Expand curated OCS/ECS/AGA compatibility corpus.
8. Implement A2065-compatible Ethernet with SLIRP.
9. Integrate network, viewport, floppy speed, and media controls across REST and
   web UI.
10. Promote ESP from stubs to explicit capability-driven features.

## Known High-Risk Areas

- Display edge behavior: right-edge wrap and left-edge injection must be solved
  in native generation, not hidden by crop.
- CIA timing: current shortcuts can boot software while leaving deeper
  compatibility broken.
- Scheduler ordering: copper/blitter/CPU/DMA race behavior is the core emulator
  quality bar.
- AGA fetch modes: Workbench can pass while games and demos still fail.
- HDF write safety: a single bad geometry assumption can corrupt user data.
- Network interrupts: A2065 can appear detected while transfers stall under load.
- Host presentation: aspect, border, crop, and vertical stretch must be separate
  and reversible settings.
- Asset management: ROMs and Workbench media are legal/user-provided inputs and
  must never become repository artifacts.

## Enterprise-Grade Operating Model

Rumiga should be managed like a serious emulator product:

- Every compatibility claim is backed by a command, artifact, and manifest.
- Every regression has a small reproduction and a subsystem label.
- Every user-facing setting has a CLI name, API field, web UI control, default,
  validation rule, and persisted representation if applicable.
- Every media write path has read-only mode, explicit writeback mode, atomicity
  notes, and test coverage.
- Every network path is disabled by default, documented, and observable.
- Every release has a compatibility report, known-issues list, and artifact
  bundle excluding copyrighted inputs.

## Near-Term Milestone

The next milestone should be:

**A1200 Workbench 3.1.4 HDF Boot Evidence Pack**

Scope:

- Stock A1200 profile.
- 68EC020-compatible behavior as the release target.
- Gayle IDE HDF boot.
- Correct native viewport with no right-edge wrap, no left-edge injected pixels,
  no bottom crop, and documented host scaling.
- CLI, REST, and web UI controls for viewport preset and screenshot capture.

Exit evidence:

- Native screenshot plus host screenshot.
- Manifest with ROM/HDF hashes, git SHA, model, PAL/NTSC mode, viewport preset,
  frame count, and disk dirty state.
- FS-UAE reference screenshot and configuration notes.
- Regression test that inspects the first 20 visible lines and right-edge pixels.

After this milestone is stable, start the A2065 + SLIRP network milestone.
