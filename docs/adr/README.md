# Architecture Decision Records

ADRs capture decisions that are expensive, cross-cutting, safety-relevant, or
hard to infer from code. They complement `ARCHITECTURE.md`: the architecture
document describes the current model, while ADRs preserve why it changed.

## When An ADR Is Required

Create an ADR for component ownership, public API contracts, persistence or
migration policy, concurrency, unsafe/FFI boundaries, target hardware
assumptions, dependency strategy, security posture, and shared quality rules.
Routine implementation choices that stay inside an accepted boundary do not
need an ADR.

## Lifecycle

1. Copy `0000-template.md` to the next zero-padded number and a short slug.
2. Use `Proposed` while discussion is open.
3. Change to `Accepted` only with the approving pull request.
4. Never rewrite an accepted outcome to hide history.
5. Replace a decision with a new ADR and mark the old one `Superseded`, linking
   both directions.

Allowed statuses are `Proposed`, `Accepted`, `Rejected`, and `Superseded`.
Numbering is contiguous; `0000-template.md` is reserved and is not a decision.

## Index

| ADR | Status | Task | Decision |
| --- | --- | --- | --- |
| [ADR-0001](0001-governance-traceability.md) | Accepted | M0-012 | Version governance contracts and machine-readable change records |
| [ADR-0002](0002-core-runtime-feature-model.md) | Accepted | M1-001 | Define explicit, mutually exclusive `std` and `no_std` core runtime profiles |
| [ADR-0003](0003-conventional-commit-policy.md) | Accepted | M0-013 | Enforce one Rust-owned Conventional Commit policy locally and in hosted CI |
| [ADR-0004](0004-stock-cpu-portability.md) | Accepted | M1-002 | Separate the portable stock CPU path from the host-only FPU implementation |
| [ADR-0005](0005-canonical-core-primitive-boundary.md) | Accepted | M1-003 | Require `core`/`alloc` primitives throughout the canonical emulator core |
| [ADR-0006](0006-injected-trace-sink.md) | Accepted | M1-004 | Move diagnostic transport out of the core behind an injected trace sink |
| [ADR-0007](0007-product-flash-partition-layout.md) | Accepted | M2-013 | Own the product flash layout with two 6 MiB OTA slots and a Secure Boot bootloader window |
| [ADR-0008](0008-reversible-security-posture.md) | Accepted | M2-014 | Exercise flash encryption with virtual eFuses and reject any configuration that would burn one |
| [ADR-0009](0009-node-current-line-pin.md) | Accepted | M0-015 | Pin Node to the current 26 line as a documented exception to the LTS rule |
| [ADR-0010](0010-deterministic-blitter-ownership.md) | Accepted | M1-005 | Execute the blitter in place under one owner and prove it with state digests |
| [ADR-0011](0011-emulated-time-and-host-pacing.md) | Accepted | M1-006 | Own host time in the shell behind a Clock contract and lint the core against host clock types |
| [ADR-0012](0012-selectable-video-standard.md) | Accepted | M1-013 | Make the video standard selectable through one type that owns every PAL/NTSC difference |
| [ADR-0013](0013-platform-capabilities-and-typed-errors.md) | Accepted | M1-007 | Version the platform contracts, describe capabilities, and separate typed failure from backpressure |
| [ADR-0014](0014-bounded-queues-and-overflow-policy.md) | Accepted | M1-008 | Bound queues with a per-queue overflow policy and counters that make saturation visible |
| [ADR-0015](0015-deterministic-input-replay.md) | Accepted | M1-009 | Stamp input against emulated frames, record and apply it in the core, and widen the state digest to match |
| [ADR-0016](0016-allocation-instrumentation.md) | Accepted | M1-010 | Measure allocations before fixing them, from outside the workspace, and retain the two per-scanline buffers |
| [ADR-0017](0017-portability-boundaries.md) | Accepted | M1-011 | Enforce guest byte order and pointer-width assumptions instead of reaching for Miri, which cannot see either failure |
