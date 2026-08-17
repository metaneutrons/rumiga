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
