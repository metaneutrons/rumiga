# ADR-0006: Injected Diagnostic Trace Sink

- Status: Accepted
- Date: 2026-08-17
- Owners: @metaneutrons
- Task: M1-004

## Context

`rumiga-core` had a valid allocator-backed `no_std` profile, but its default
desktop profile still owned host filesystem tracing. `Emulator` held a
`BufWriter<File>`, `enable_cpu_trace()` accepted a host path and called
`std::fs::File::create()`, and both trace call sites were compiled out of the
portable profile. A core that opens files cannot be treated as a portable
deterministic boundary, and a diagnostic facility that exists only on the
desktop is unavailable exactly where a D1001 bring-up would need it.

The CPU trace format is also an evidence surface. Existing capture manifests
report a trace count, and trace files are compared during CPU investigations,
so a migration that changes record bytes would silently invalidate previous
comparisons.

## Decision

Diagnostic transport is a platform service. `rumiga-platform` defines
`TraceSink` with `write_record` and `flush`; `rumiga-core` re-exports it and
holds an optional `Box<dyn TraceSink + Send>`.

The core keeps record formatting, the trace limit, and the recorded count. It
passes `core::fmt::Arguments` to the sink, so the record layout is produced in
exactly one place, no intermediate `String` is allocated, and the layout cannot
drift per transport. Records carry no line terminator; each transport appends
what it needs, and the desktop file sink appends `\n`.

Flushing is explicit. `Emulator::flush_trace()` exists so durability is not a
side effect of drop order, and the desktop calls it when the interactive loop
ends and after the capture run. Buffered sinks still flush on drop as a
backstop.

Both trace methods are infallible. Diagnostics must not alter emulated state or
abort emulation, so a sink absorbs its own transport errors. Host errors that a
user can act on surface where the adapter is constructed, which is where the
desktop already reported a failed trace path.

Tracing is no longer feature-gated. It is available in both runtime profiles
because it no longer requires host facilities.

## Consequences

The core neither creates files nor accepts host paths. `rumiga-core` gains a
dependency on `rumiga-platform`, which is the first core-to-contract edge; it
matches the target architecture, where platform service contracts sit above the
core, and it does not introduce a cycle because `rumiga-platform` has no
dependencies.

The public API changes. `enable_cpu_trace`, the `trace_writer` and `trace_limit`
fields, and the public `trace_count` field are replaced by `set_trace_sink`,
`flush_trace`, `clear_trace_sink`, and a `trace_count` accessor. Trace state is
now private, so it can only be changed through the documented contract.

Core tests no longer write files. An in-memory sink records the same bytes the
file transport receives, which removes a working-directory artifact from the
test suite and makes trace assertions available to the portable profile.

The `Send` bound on the boxed sink keeps `Emulator` movable across threads. That
property is asserted in a test so a later sink cannot remove it silently.

## Alternatives

A generic `Emulator<S: TraceSink>` was rejected because the type parameter would
propagate through the emulator's entire public surface for an optional
diagnostic facility.

A structured `TraceRecord` formatted by the adapter was rejected because it
moves an evidence-relevant byte layout into each transport, and the disassembler
that produces the record already lives in the core.

Implementing `core::fmt::Write` instead of a dedicated trait was rejected
because `fmt::Error` carries no host detail and the contract needs an explicit
flush, which `fmt::Write` does not express.

A global callback or a static logger was rejected because it hides ownership,
which is the property this task exists to make visible.

## Evidence

`crates/rumiga-core/tests/trace_test.rs` asserts golden records captured from
the previous file-writing implementation, and it passes under both
`--features std` and `--no-default-features --features no_std`, so byte
compatibility holds without the core creating a file. The same suite covers the
trace limit, the count reset on attach, detach behavior, and the emulator's
`Send` property.
`crates/rumiga-platform-desktop/tests/file_trace_sink_test.rs` covers real file
creation, truncation, the `\n` terminator, and host error reporting.

End to end, a three-frame Kickstart 46.143 capture with a 20000-instruction
trace limit was produced at pre-change revision `1a6da29` and at this change.
Both trace files have SHA-256
`222caf36e1f9c12b9a051ae792da8091680ea84435f96075dba40fd8f1015bde`, both
capture manifests report `trace_count` 20000, and both PNG captures are
identical, so the CLI, transport, flush point, and manifest are unchanged.

## Supersession

None. This replaces the host-owned tracing that ADR-0002 and ADR-0005 recorded
as a remaining `std` service. Host threads and CPU affinity in the blitter path
remain open and belong to M1-005.
