# ADR-0018: Published Portability Contract

- Status: Accepted
- Date: 2026-08-18
- Owners: @metaneutrons
- Task: M1-012

## Context

M1-001 through M1-011 each added one portability rule and enforced it. The rules were
scattered across seven ADRs, two Clippy configurations, a `compile_error!`, two
compile-time assertions, and a gate, so no single place said what the contract was.

The task's acceptance criterion is that the core dependency graph contains only approved
`no_std` crates. Nothing checked that. The supply-chain gate holds licences, registries,
Git sources, and advisories, and the portable gate compiles the core for
`riscv32imafc-unknown-none-elf`, but neither constrains which crates may appear in the
core graph at all.

## Decision

The contract is published as a table in `ARCHITECTURE.md`, one row per rule, each naming
where it is enforced and since which task. A list of rules without their enforcement would
be an intention; naming the enforcement is what makes the table a contract.

The dependency rule is enforced rather than documented, and it is stricter than the
acceptance criterion asks. The stock core resolves to exactly three workspace crates,
`m68k`, `rumiga-core`, and `rumiga-platform`, with no third-party dependency at all, so
the manifest declares that closed set instead of an allowlist of vetted `no_std` crates.
The portable gate resolves the graph for the bare-metal target with normal edges only and
compares it against the declaration.

The comparison runs both ways. An unexpected crate is the case that motivates the check. A
missing one matters too: it means the declaration has drifted from what the core needs, and
a stale declaration would stop catching additions while still looking like a constraint.

A closed set was chosen over an allowlist because "approved `no_std` crate" is not a stable
property. A crate that is `no_std` today can gain a `std` path, an allocator assumption, or
a platform dependency in a patch release. The portable compile would catch some of those
and not others, and nothing would prompt a review. Widening the set is an edit to the
manifest and to the test that pins its shape, which is visible in a diff.

## Consequences

The lockfile gate turned out to be a partial defence, which is why the graph check is not
redundant. Adding a real dependency as a probe failed at `--locked` before the graph check
ran, because a new dependency changes `Cargo.lock`. That stops an unintended resolution
change; it does not stop a dependency committed deliberately together with its lockfile
update, which is exactly the case the graph check catches.

Both directions of the comparison are probe-verified. Removing `rumiga-platform` from the
declaration produced `portable core graph contains crates the manifest does not permit:
["rumiga-platform"]`. Adding a crate the graph does not contain produced `portable core
graph no longer contains declared crates ["serde"]`. Both probes touched only the manifest,
so no dependency or lockfile was disturbed.

The declared set is pinned by the firmware manifest test as well as by the gate. Widening
it therefore fails two places rather than one, and both are in the diff a reviewer reads.

## Alternatives

An allowlist of approved `no_std` crates, as the plan's wording suggests, was rejected for
the reason the decision gives: the property it approves is not stable across versions, and
the core does not currently need any third-party crate, so an allowlist would grant
permission nobody has asked for.

Deriving the permitted set from the resolved graph at gate time was rejected. It would
always pass, because it would compare the graph against itself.

Documenting the rules without the dependency check was rejected. That is the option that
would have satisfied the plan's wording while leaving the acceptance criterion unenforced,
and it would have been the weakest result in the M1 sequence.

Including build and dev dependencies in the comparison was rejected. Neither ships in the
core, and `stats_alloc`, a dev-dependency added by M1-010, would otherwise have to be
declared as part of the core graph, which would misstate what the device runs.

## Evidence

`cargo +1.97.1 xtask ci --gate portable` resolves the graph and compares it against the
manifest. `cargo +1.97.1 test --locked -p rumiga-firmware --test toolchain_manifest` pins
the declaration's shape and contents. Both probes above were run and reverted.

The published table was checked row by row against the enforcement it names: the two
Clippy configurations in `crates/rumiga-core/clippy.toml` with their `deny` attributes in
the crate root, the `compile_error!` pair for the feature profiles, the two compile-time
assertions, the allocation test, and the two portable-gate checks.

## Supersession

None. This closes the portability contract entry and, with it, milestone M1.

The contract covers `rumiga-core` and the crates it pulls in. The desktop shell and the ESP
platform crate are outside it by design: they exist to hold what the core must not, and the
platform crate's own graph is checked by the `foundation` portable profile rather than by
the closed-set rule.
