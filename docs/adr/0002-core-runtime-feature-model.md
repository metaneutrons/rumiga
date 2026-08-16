# ADR-0002: Core Runtime Feature Model

- Status: Accepted
- Date: 2026-08-16
- Owners: @metaneutrons
- Task: M1-001

## Context

`rumiga-core` previously had no Cargo feature boundary. Consequently,
`--no-default-features` appeared to succeed while still compiling the normal
`std` crate, and host-only file tracing, thread creation, CPU affinity, and
standard-library collections were inseparable from the machine model. The
D1001 target needs an allocator-backed core without an operating-system
runtime, but desktop behavior must remain stable while that migration proceeds.

## Decision

`rumiga-core` has exactly two explicit, mutually exclusive runtime profiles:
`std` and `no_std`. The default is `std` for desktop compatibility. Embedded
consumers must select `--no-default-features --features no_std` and provide an
allocator.

The `std` profile retains file-backed CPU tracing, background blitter execution,
and optional `core_affinity`. The `no_std` profile excludes those host services
and executes the current immediate blitter implementation synchronously. Both
valid profiles compile, lint, and run the core test suite in the host gate;
selecting neither or both fails with a stable diagnostic.

This decision establishes the core source boundary only. It does not claim a
bare-metal RISC-V dependency graph: `m68k` remains a `std` dependency until
M1-002. Trace injection and deterministic single-owner blitter ownership remain
M1-004 and M1-005.

## Consequences

Desktop users retain existing behavior without extra flags. Portable work can
now compile and test the core's own `no_std + alloc` paths independently of the
CPU migration. Feature-specific APIs are explicit: filesystem CPU tracing is
available only under `std`. Maintaining two profiles adds CI time and requires
new code to use `core` or `alloc` unless it is deliberately host-gated.

## Alternatives

Using only a positive `std` feature and treating its absence as portable mode
was rejected because accidental `--no-default-features` builds would not state
their intent. Keeping `std` unconditional until the whole dependency graph was
portable was rejected because it would hide incremental regressions. Making
`no_std` the default was rejected because it would silently change desktop
tracing and blitter behavior.

## Evidence

`cargo +1.97.1 xtask ci --gate host` validates the explicit `std` profile,
Clippy and all tests under `no_std`, both invalid feature selections, and the
unchanged default workspace profile on Linux and macOS.

## Supersession

None.
