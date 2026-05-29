# Rumiga System Audit

This audit captures the repository state after the m68k/A1200 compatibility
work. It is intentionally concrete: what is present now, what is partial, and
what should be stabilized next.

## Current Architecture

Rumiga now has four active layers:

- `crates/m68k`: a vendored M68000-family CPU core with 68000, 68010, 68020,
  68030, and 68040 profiles, disassembly, tracing support, MMU/FPU scaffolding,
  and enough 68020-class behavior for A1200 Kickstart progress.
- `crates/rumiga-core`: the Amiga machine core with CPU, memory map, custom
  chips, CIA, floppy, Gayle IDE, display, audio, and frame scheduling.
- `desktop`: a minifb development runner with model, CPU, RAM, floppy, HDF,
  viewport, and trace controls.
- `crates/rumiga-api` plus `web`: shared REST DTOs and a Next.js control UI.
  These still need a real desktop or firmware REST server.

The older `crates/m68000` crate remains in the workspace for legacy comparison
and tests. The desktop comparison test still uses the sibling `r68k` dependency
as a regression oracle, but `rumiga-core` itself now runs through `m68k`.

## Implemented Baseline

- A500/A500+/A600/A1200 model selection.
- 68000 through 68040 CPU profile selection at the CLI.
- A1200 memory profile with 2MB Chip RAM and 68020 default CPU.
- 32-bit logical CPU address path in the Amiga memory bus.
- PCMCIA open-bus ranges and Gayle low/high register ranges.
- Gayle ATA/IDE controller with identify, read sectors, write sectors, and
  in-memory HDF dirty tracking.
- ADF writeback path for floppy DMA writes.
- AGA-adjacent display work: 8 bitplanes, 256-entry 24-bit palette storage,
  BPLCON3 palette banking/LOCT handling, HAM6/HAM8 rendering, FMODE/BPLCON4
  register plumbing, and sprite palette bank support.
- CPU instruction tracing from the desktop runner.
- Expanded desktop CLI validation for model, CPU, RAM, video standard, HDF,
  explicit DF0-DF3 mapping, floppy speed, and trace limits.

## Verification Status

The current working tree has passed:

- `cargo check --workspace --all-targets`
- `cargo test --workspace`

The local machine also has Kickstart ROMs available, so the integration suite
exercises A500 Kickstart 1.3 and A1200 Kickstart 3.1 boot progress. These tests
skip cleanly on machines without user-provided ROM files.

## Remaining Stabilization

- Keep `cargo fmt --check` and clippy green after the large import.
- Remove stale references to `r68k` from docs and core-only dependencies.
- Keep ROM and disk images out of git; they belong in ignored local `assets/`
  or the user's configured ROM/ADF paths.
- Add screenshot/trace-based boot baselines for A500, A1200, Workbench ADF,
  and HDF boot once the runtime behavior is ready to freeze.

## Next Engineering Steps

1. Stabilize and commit the m68k/A1200 baseline in functional groups.
2. Validate HDF boot against WinUAE behavior and document the required disk
   geometry assumptions.
3. Add desktop REST endpoints backed by the same runtime config used by the
   CLI, then point the Web UI at that server.
4. Extend AGA coverage toward full Lisa behavior: complete FMODE fetch widths,
   BPLCON4 sprite rules, dual-playfield bank interactions, and edge-case HAM8.
5. Replace ESP stubs with display/audio/storage/WiFi/API drivers once the
   desktop core is behaviorally stable.
