# ADR-0005: Canonical Core Primitive Boundary

- Status: Proposed
- Date: 2026-08-16
- Owners: @metaneutrons
- Task: M1-003

## Context

The core already selected an allocator-backed `no_std` profile, and its
collections and cells had largely moved to `alloc` and `core`. That build alone
does not prevent a future desktop-only change from using `std` for a primitive
that is equally available to the portable profile. Such drift delays discovery
until a target build and makes otherwise reusable core APIs needlessly host
specific.

Some standard-library use is intentionally still present. File-backed tracing
and background blitter workers are host services behind the `std` feature; their
replacement belongs to M1-004 and M1-005 rather than a primitive migration.

## Decision

`rumiga-core` denies Clippy's `std_instead_of_core` and
`std_instead_of_alloc` lints. The canonical host matrix explicitly runs strict
Clippy for the `std` profile as well as the existing `no_std` profile, so both
views enforce the same portable primitive rule.

The same host matrix compiles the stock `no_std` core graph with the declared
Rust 1.85 MSRV. The CPU's newer `let`-chain expressions are expressed as
equivalent match guards so that the workspace's published language minimum is
an executable contract.

Portable equivalents are used directly: the core uses `core::mem::take`, and
`MacAddressError` implements `core::error::Error`. Inherently host-bound APIs
remain explicitly feature-gated and are not waived by this decision.

## Consequences

Desktop behavior remains unchanged while primitives retain their bare-metal
compatibility. The error type is usable by no-std consumers through the shared
core trait. CI adds a focused strict-Clippy invocation to each host matrix leg.

This is not a claim that the canonical core has no host services. Trace files,
threads, affinity, deterministic replay, and allocation bounds remain separate
M1 tasks and G1 remains open.

## Alternatives

Static text searches for `std::` were rejected because conditional compilation,
comments, and legitimate host adapters make them brittle and easy to evade.
Relying only on the bare-metal compile was rejected because it does not examine
the desktop-selected source path for portable primitive regressions. Removing
all host services now was rejected because it would conflate this narrow rule
with M1-004 and M1-005's behavioral migration.

## Evidence

`cargo +1.97.1 xtask ci --gate host` runs strict Clippy in both explicit core
profiles and the complete host suite. `cargo +1.97.1 xtask ci --gate portable`
continues to compile the complete stock core as an optimized bare-metal RISC-V
release. The same host gate compiles the stock core with Rust 1.85. The
`MacAddressError` unit test proves that the shared error contract is available
in the allocator-backed profile. Acceptance requires a clean
pull-request and final `main` hosted run with independently verified governance
artifacts.

## Supersession

None. This extends the runtime feature boundary in ADR-0002 and complements the
stock CPU boundary in ADR-0004.
