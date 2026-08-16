# ADR-0004: Stock CPU Portability And FPU Boundary

- Status: Accepted
- Date: 2026-08-16
- Owners: @metaneutrons
- Task: M1-002

## Context

The active `m68k` crate implicitly required `std`, so `rumiga-core` could not
enter the bare-metal RISC-V dependency graph even though its own source already
had an allocator-backed `no_std` profile. The integer CPU path has no operating
system dependency, but the 68881/68882/68040 FPU implementation uses
standard-library floating-point functions that are unavailable on the pinned
bare-metal target. Stock A500 and A1200 release profiles require M68000 and
68EC020-class integer behavior, not an FPU.

## Decision

`m68k` has exactly one explicit runtime profile: `std` or `no_std`. `std`
remains the default. The `no_std` profile uses `core` plus `alloc`; allocation is
currently required only by the public disassembler string API.

The FPU implementation is a separate `fpu` feature. It remains enabled in the
default desktop profile and requires `std`. Selecting `fpu` with `no_std` is
rejected with a stable compile-time diagnostic. Integer CPU, MMU, cache, and
MOVE16 paths remain available without the FPU feature. An FPU opcode that is
not handled in the stock profile follows the existing Line-F interception and
exception path.

`rumiga-core` disables dependency defaults and forwards its selected runtime
profile to `m68k`. Desktop composition explicitly selects `std` and `fpu`.
Repository-owned portable profiles compile the foundational crates and the
complete stock Amiga core separately; the latter is an optimized
`--no-default-features --features no_std` build for
`riscv32imafc-unknown-none-elf`.

## Consequences

Default desktop and 68040 FPU behavior remain enabled. Embedded consumers gain
a complete stock M68000/68EC020 core graph without `std`, while invalid feature
selection fails closed. The CPU and core feature matrices add host CI time, and
portable integrations must provide a global allocator before linking an
executable.

The FPU is not yet available in the portable profile. Adding portable FPU
support would require a separately reviewed deterministic math strategy and is
outside the stock A500/A1200 release scope.

## Alternatives

Adding a `libm` dependency to every portable CPU build was rejected because
stock profiles do not need the FPU and it would enlarge the target graph before
cross-platform floating-point determinism is specified. Removing the existing
FPU implementation was rejected because it would regress desktop 68040
support. Treating absence of `std` as an implicit portable mode was rejected
because accidental feature omissions must fail with a targeted diagnostic.

## Evidence

`cargo +1.97.1 xtask ci --gate host` validates both valid `m68k` profiles, the
default FPU-enabled workspace, all invalid combinations, and the stock Line-F
behavior. `cargo +1.97.1 xtask ci --gate portable` compiles `m68k` and
`rumiga-core` as optimized `no_std` release artifacts for bare-metal 32-bit
RISC-V. Pull-request run
[`31955508417`](https://github.com/metaneutrons/rumiga/actions/runs/31955508417)
and final `main` run
[`31955947410`](https://github.com/metaneutrons/rumiga/actions/runs/31955947410)
pass every required job. Their governance artifacts have independently
verified archive digests, complete payload checksums, and clean source
revisions.

## Supersession

This decision completes the CPU dependency limitation recorded by ADR-0002. It
does not supersede the core runtime feature model.
