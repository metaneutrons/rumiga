# Rumiga Architecture

This document distinguishes the architecture that exists at the audited
revision from the architecture required for the D1001 product. Current delivery
status is in `PROJECT_STATUS.md`; migration tasks are in
`IMPLEMENTATION_PLAN.md`.

## Architectural Goals

- One deterministic, platform-independent Amiga machine core.
- Rust-owned emulator and product logic on desktop and ESP32-P4.
- Explicit, bounded platform contracts for real-time I/O.
- Native chipset pixels separated from presentation and OSD composition.
- Storage proportional to cache size, never disk-image size.
- Host services scheduled outside the emulator state owner.
- Repeatable host and hardware evidence for every compatibility claim.

## Current Architecture

The active root workspace contains:

```text
rumiga-core ---------------------> m68k
     ^
     |
rumiga-desktop --> rumiga-api
     |          -> rumiga-platform
     +----------> rumiga-platform-desktop

rumiga-firmware -> rumiga-core
       |         -> rumiga-platform
       +---------> rumiga-platform-esp -> rumiga-platform

m68000  (tracked independent 68000 test oracle)
```

`rumiga-platform-esp` and `firmware` are workspace members and pass host-side
manifest, check, lint, and toolchain-pin gates. ESP-IDF Rust dependencies and
immutable SDK/reference inputs are configured. The locked ESP-IDF 6.0.0 stack
produces an ESP32-P4 ELF locally, but the modules remain stubs and there is no
flash, boot, peripheral, or performance HIL evidence. The ESP adapter depends
only on the platform contracts; firmware is the composition root that also owns
the emulator core.

Important current constraints:

- `rumiga-platform` is `no_std + alloc`.
- `rumiga-core` now has mutually exclusive `std` and `no_std` profiles; `std`
  remains the desktop default and the `no_std + alloc` source profile is
  host-compiled, linted, and tested.
- `m68k` remains a `std` crate, so the complete core dependency graph does not
  yet compile for bare-metal RISC-V. That boundary is M1-002.
- Under `std`, the core still opens CPU trace files and can spawn a blitter
  thread. The `no_std` profile excludes tracing and executes blits
  synchronously until M1-004 and M1-005 replace both host-owned services.
- The desktop binary owns CLI, REST, static web serving, presentation,
  evidence, media persistence, and loop scheduling in one module.
- Desktop REST media I/O is isolated in `desktop/src/storage.rs`, confined to a
  configured canonical root, streamed under a size limit, and published without
  overwriting existing files.
- Gayle/ATA owns a complete HDF `Vec<u8>`.
- The desktop REST listener binds to `127.0.0.1:8080`.

## Target Architecture

```text
                         +----------------------+
                         | Web UI / REST client |
                         +----------+-----------+
                                    |
                       versioned control contract
                                    |
              +---------------------+---------------------+
              |                                           |
     +--------v---------+                         +-------v--------+
     | Desktop product  |                         | D1001 firmware |
     | shell            |                         | shell          |
     +--------+---------+                         +-------+--------+
              |                                           |
     +--------v---------+                         +-------v--------+
     | Desktop adapters |                         | ESP adapters   |
     | fs/window/audio  |                         | BSP/IDF/SDIO   |
     +--------+---------+                         +-------+--------+
              |       platform service contracts         |
              +---------------------+---------------------+
                                    |
                         +----------v-----------+
                         | rumiga-core          |
                         | no_std + alloc       |
                         | deterministic owner  |
                         +----------+-----------+
                                    |
                         +----------v-----------+
                         | m68k CPU core        |
                         | no_std + alloc       |
                         +----------------------+
```

## Ownership Boundaries

### Emulator core

The core owns only emulated state:

- CPU registers, exceptions, and instruction execution;
- Amiga memory map and RAM/ROM contents;
- OCS/ECS/AGA custom registers and DMA engines;
- copper, blitter, playfields, sprites, and native RGB565 framebuffer;
- CIA, interrupts, keyboard serial protocol, and disk control;
- Paula channels and native audio samples;
- floppy and ATA/Gayle protocol state;
- A2065 device registers, descriptors, and interrupt state;
- deterministic emulated time, events, and state digests.

The core must not own:

- files, paths, directories, sockets, HTTP, DNS, or credentials;
- OS threads, CPU affinity, async runtimes, or task priorities;
- host wall clock, sleep, display swaps, audio devices, or USB reports;
- SD/MMC, FAT, MIPI-DSI, I2S, touch, ESP32-C6, or power management;
- release update, logging transport, or UI state.

### Platform services

Platform contracts are versioned and capability-driven. The final contract set
must cover:

| Service | Required behavior |
| --- | --- |
| Clock | Monotonic host time, pacing wakeup, and measured sleep/yield |
| Video | RGB565 frame ownership, dimensions, capabilities, backpressure, and present result |
| Audio | Format/rate negotiation, bounded queue, latency, underrun, and drain |
| Input | Timestamped key, pointer, touch, joystick, connect, and disconnect events |
| Block media | Sector capacity, read, write, flush, read-only, change generation, and errors |
| Network link | Bounded Ethernet frame TX/RX, link state, counters, and reset |
| Storage catalog | Safe file listing/upload/delete under a configured root |
| Lifecycle | Start, pause, resume, reset, shutdown, watchdog, and reset reason |
| Capabilities | Supported models, memory, media, display, input, audio, and network limits |
| Telemetry | Queue depth, memory, frame time, I/O latency, underrun, temperature, and errors |
| Logging | Structured records with redaction and bounded transport |

Methods that can fail or block return explicit results. Queue overflow and
device removal policies are part of the contract, not implementation details.

### Product shell

The desktop and firmware shells own:

- configuration validation and persistence;
- ROM/media selection and hash calculation;
- platform task creation, priorities, core assignment, and watchdog feeding;
- REST/web/serial control endpoints;
- screenshot encoding and support bundles;
- media writeback/snapshot orchestration;
- Wi-Fi provisioning and credential lifecycle;
- update, rollback, recovery, and release metadata.

## Deterministic Scheduling

The canonical emulator path has one mutable owner. A host calls into the core
with a bounded amount of work and exchanges data through explicit queues or
borrowed buffers.

```text
input events -> [bounded queue] -> emulator owner
                                  | CPU/custom/CIA/media/network state
                                  +-> native frame -> [video queue]
                                  +-> PCM samples  -> [audio queue]
                                  +-> disk requests-> [block adapter]
                                  +-> Ethernet     -> [network queue]
```

The core never waits for display, audio, storage, or network completion while
holding partially updated emulated state. The product shell defines pacing:

- PAL target: 50 emulated frames per second.
- NTSC target: approximately 60 emulated frames per second.
- Audio clock drift is corrected in the platform resampler/queue policy.
- Frames may be presented or deliberately skipped according to a documented
  backlog policy; emulated state is never skipped silently.

Deterministic evidence disables optional host parallelism and records all input
events, media hashes, configuration, and final state digest.

## Display Pipeline

Display correctness is divided into independent stages:

```text
chipset registers/DMA
  -> native RGB565 frame + beam metadata
  -> viewport policy (native/visible/overscan/auto/manual)
  -> pixel-aspect correction and vertical presentation
  -> nearest/integer or aspect-fit scale
  -> panel rotation and centered border
  -> optional OSD composition
  -> desktop window or D1001 MIPI-DSI buffer
```

Native screenshots are captured before viewport/presentation. Presented
screenshots are captured before or after OSD with that choice recorded. Right
edge data appearing at the left edge is always a native-generation failure if
present in the native capture; crop or border settings must never hide it.

### D1001 display mapping

- Physical panel: 800x1280 at the BSP's supported mode.
- Preferred emulator orientation: landscape presentation within 1280x800.
- Native core format: RGB565, matching the lowest-cost BSP format.
- DMA/panel buffers: platform-owned and PSRAM-budgeted.
- Scaling: measured CPU/PPA/DMA implementation behind the same presentation
  contract; no scaling algorithm belongs in chipset generation.

## Audio Pipeline

```text
Paula DMA/channels
  -> deterministic native samples
  -> stereo separation/mix
  -> bounded resampler and clock correction
  -> clipping/volume/mute
  -> I2S DMA
  -> ES8311
  -> controlled mono downmix for built-in speaker
```

The core emits samples according to emulated time. Platform code reports
accepted frames, queue depth, underruns, and effective rate. Starting audio with
an empty DMA buffer is avoided through explicit prefill.

## Input Pipeline

Platform drivers normalize input into timestamped logical events. The core sees
Amiga keys, relative mouse deltas/buttons, and joystick state, not macOS key
codes, USB usages, or raw touch coordinates.

- USB keyboard: HID usage -> configurable Amiga key mapping.
- USB mouse: HID report -> relative Amiga mouse event.
- USB gamepad: descriptor/profile -> joystick port and buttons.
- Touch: calibrated panel point -> OSD command or relative/absolute mouse policy.
- Disconnect: releases all owned buttons/keys to prevent stuck state.

## Storage Architecture

Disk images are host resources accessed through bounded contracts:

```text
Gayle/ATA -> BlockDevice sectors -> bounded cache -> file -> SD/MMC
                                 \-> COW overlay/snapshot

Trackdisk -> ADF image contract -> file/cache -> SD/MMC
```

The core owns controller and protocol state, not file bytes. A `BlockDevice`
must expose fixed capacity, 512-byte sectors for ATA, read-only state, flush,
media generation, and typed errors. Cache size is fixed by profile and appears
in telemetry.

Media policy:

- read-only is the default;
- snapshot uses a separate overlay and never mutates the base;
- writeback is explicit and flushes at defined boundaries;
- removal or reset cancels/settles in-flight operations deterministically;
- paths are canonicalized inside one configured media root.

## Network Architecture

The emulated side remains A2065-compatible:

```text
Amiga SANA-II driver
  <-> A2065/LANCE registers and descriptors in rumiga-core
  <-> bounded Ethernet-frame link contract
  <-> desktop SLIRP or D1001 ESP32-C6 host network service
```

Packet transport, DHCP/DNS/NAT, sockets, and Wi-Fi stay outside the core.
Network is disabled by default. Link transitions and packet counters are fed
back at deterministic scheduling boundaries.

## D1001 Runtime Model

The intended firmware task model is:

| Task | Responsibility | Constraint |
| --- | --- | --- |
| Emulator owner | CPU and all emulated state | One owner, highest sustained application priority |
| Display service | Buffer acquisition, scale/rotate, MIPI present | Bounded queue; never mutates native frame |
| Audio service | Resample, prefill, I2S DMA | Bounded latency; no blocking core call |
| Input service | Touch and USB HID polling/events | Releases state on disconnect |
| Storage service | SD file/block requests and flush | Bounded cache and request queue |
| Network service | C6 lifecycle and Ethernet frames | Disabled until configured |
| Control service | Serial/REST/web and support bundle | Rate/size limited and authenticated on Wi-Fi |
| Supervisor | Watchdog, health, reset, update, telemetry | Can recover failed services without corrupting media |

Exact core affinity and FreeRTOS priorities are selected from measurements, not
hard-coded in `rumiga-core`.

## Initial D1001 Memory Budget

The 32 MiB PSRAM budget is a design constraint. Initial caps must be measured
and refined, but the release profile starts with:

| Consumer | Initial cap/expectation |
| --- | ---: |
| Chip RAM | 2 MiB maximum stock profile |
| Fast/slow RAM | Profile-limited; disabled unless compatibility requires it |
| Native RGB565 frame | Less than 0.5 MiB for current 754x288 buffer |
| Panel RGB565 buffers | Approximately 2 MiB each at 800x1280; count is BSP/config dependent |
| HDF sector cache | At most 1 MiB |
| ADF image/cache | At most 1 MiB |
| Audio/input/network queues | Bounded, measured, and normally below 0.25 MiB combined |
| Firmware stacks/control/web | Measured from link map and runtime high-water |
| Required operational reserve | At least 4 MiB; total high-water at most 27 MiB |

A two-buffer full-panel configuration consumes roughly 4 MiB before allocator
and alignment overhead. The integrated map file and runtime largest-free-block
metric decide the final buffer strategy.

## FFI and Safety Policy

The existing workspace forbids unsafe Rust. ESP-IDF bindings necessarily cross
an unsafe FFI boundary. The exception is narrowly scoped:

- unsafe code is allowed only in named D1001 adapter modules approved by an
  architecture decision; no blanket BSP exception exists;
- every unsafe block states pointer, lifetime, alignment, ownership, interrupt,
  and task-context invariants;
- the public adapter API is safe Rust with typed handles and errors;
- callbacks do not outlive their owners and DMA buffers have explicit lifetime;
- host mocks and D1001 HIL test the safe contract;
- Seeed inputs and Vellum-derived adapters carry source and license provenance;
  each Vellum transfer records its immutable revision and paths under the
  copyright-holder authorization, while third-party inputs require a compatible
  license;
- no unsafe code is permitted in emulator logic.

## Configuration and API

Configuration has one canonical Rust model. CLI/serial, REST JSON, web forms,
persisted settings, and support bundles map to it. Capabilities determine which
fields are legal on each platform.

Every mutating API operation has:

- schema/version and stable error code;
- validation and an explicit default;
- authorization and request-size policy on network interfaces;
- asynchronous operation status where hardware work is not immediate;
- support-bundle representation without secrets or media bytes;
- contract and browser workflow tests.

## Evidence Architecture

Host and device runs emit the same logical manifest schema with platform-specific
extensions. A D1001 runner controls firmware through serial or a local test
interface, captures artifacts, and records board/toolchain metadata.

Reference emulators are behavior oracles, not linked runtime dependencies:

- WinUAE source is the detailed chipset/device reference.
- FS-UAE is the practical macOS reference runner and screenshot source.
- Reference version, configuration, ROM/media hashes, and milestone are recorded.
- Copyrighted inputs and reference media are never committed.

## Architecture Fitness Tests

Current automated checks enforce:

- exactly one `rumiga-core` runtime feature is required; explicit `std` and
  `no_std` host profiles compile while neither/both selections fail closed.
- the complete `rumiga-core` test suite and Clippy pass under `no_std`; the
  default workspace suite proves unchanged `std` behavior.

The remaining milestone fitness gates are:

- after M1-002, `rumiga-core` plus `m68k` must compile for
  `riscv32imafc-unknown-none-elf` before target portability is claimed.
- forbidden-import checks reject `std`, filesystem, thread, socket, and platform
  dependencies in canonical core modules.
- dependency graph checks reject unpublished paths outside the repository.
- deterministic replay produces stable state/frame/audio digests.
- bounded queues expose overflow tests and high-water metrics.
- a 2 GiB HDF integration test proves memory does not scale with image size.
- device HIL proves each service before integrated emulator promotion.
