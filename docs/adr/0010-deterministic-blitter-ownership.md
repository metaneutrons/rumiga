# ADR-0010: Deterministic Single-Owner Blitter

- Status: Accepted
- Date: 2026-08-18
- Owners: @metaneutrons
- Task: M1-005

## Context

Under `std` the blitter ran on a spawned thread pinned to the last CPU core. To
hand chip RAM to that worker the emulator removed it from itself with
`core::mem::take`, and every address-bus access then synchronised: eagerly for
chip RAM and custom registers, lazily elsewhere. The `no_std` profile executed the
same blit in place.

That arrangement leaked host scheduling into emulated state in three ways, all of
which are now covered by tests that fail on the previous implementation.

The blitter interrupt was raised in `finish_completed_blitter`, which only ran
when something later synchronised. Completion itself raised nothing, so under
`no_std` the interrupt was never raised at all and guest code waiting on it would
wait forever. The two profiles were therefore not equivalent, which the
architecture claimed they were.

The guest-visible BBUSY bit in `DMACONR` was set from whether a host thread handle
still existed. BBUSY is the standard blitter-wait idiom, so a guest polling it
observed host scheduling. Repeated captures on this host produced identical
frames, which made the leak easy to mistake for determinism; that was an accident
of host speed, not a property.

A state digest taken while the worker held chip RAM digested an empty slice,
because the emulator's own copy had been taken away. Any diagnostic that read chip
RAM without synchronising first saw nothing rather than stale data.

## Decision

Both profiles execute the blit in place, in one implementation. The emulator owns
its state for the whole operation. `std::thread`, `JoinHandle`, and
`core_affinity` leave the core, and `core_affinity` leaves the workspace because
the desktop declared it without using it.

Completion is signalled where it happens: `start_blitter_execution` executes the
blit and raises `INT_BLIT`, updating the readable interrupt shadow.

A blit therefore takes no emulated time and BBUSY always reads clear. A guest
`WaitBlit()` loop exits immediately, which is the honest representation of an
instantaneous blit. Cycle-accurate blitter timing is separate future work and is
named as such rather than implied by a busy flag that meant something else.

Determinism is proven with digests introduced in the same task: a 64-bit FNV-1a
digest over CPU registers, elapsed cycles, the custom register shadow, interrupt
and DMA state, and chip RAM. It is not cryptographic and carries no integrity
claim. It exists so two runs can be compared, and it works in the portable
profile without a dependency.

## Consequences

The desktop and portable paths are now provably the same machine: both reach an
identical pinned fixture digest, so a future divergence fails a test rather than
appearing as a hardware bug.

The address bus loses its synchronisation branches, so every memory access is
shorter and the read and write paths are simpler.

`Emulator::sync_blitter` and `sync_blitter_lazy` are removed from the public API,
together with the `AmigaMemory` thread fields and their sync methods. Callers do
not need a replacement because a blit result is visible to the next access.

Blits no longer run concurrently with CPU execution. The thread presumably existed
for throughput, so this is the change's real risk. A 60-frame Kickstart 46.143
capture produces byte-identical output before and after, which shows correctness
but not cost; a dedicated frame-time measurement belongs to the performance work
in M9 rather than to this task.

## Alternatives

Keeping the thread and making BBUSY independent of it was rejected. It would have
hidden the leak rather than removed it, and the detached chip RAM would still make
any unsynchronised reader see an empty slice.

Deferring the state digest to M1-009 was rejected. Without it the acceptance
criterion for this task cannot be met as written, and the three defects above are
invisible to a frame comparison on a fixture that does not expose them.

A cryptographic digest was rejected because the core graph is limited to approved
`no_std` crates and equality comparison needs no cryptography. Saying so in the
module documentation matters more than the choice itself.

Implementing cycle-accurate blitter timing in this task was rejected as scope. It
would replace one inaccuracy with a much larger change and is not required to
remove host scheduling from emulated state.

## Evidence

`cargo +1.97.1 test --locked -p rumiga-core --lib` passes under both explicit
profiles, including four blitter tests that fail on the previous implementation:
repeated runs agree, completion requests the interrupt, BBUSY reads clear, and the
digest distinguishes distinct machine states. `both_runtime_profiles_reach_the_pinned_state`
pins the fixture digest so the two profiles cannot diverge silently.

On the host, three consecutive 60-frame Kickstart 46.143 captures are
byte-identical to each other and to the same capture taken from the threaded
implementation at revision `1a5bee2`.

Clean pull-request run
[`32078987151`](https://github.com/metaneutrons/rumiga/actions/runs/32078987151)
produced governance artifact `9304428904` with archive SHA-256
`9f12870bc0013f459299e06c6d125838d1ed3489b52b714430e90ad2cf854346`. Final `main`
run
[`32104990662`](https://github.com/metaneutrons/rumiga/actions/runs/32104990662)
produced governance artifact `9312830673` with archive SHA-256
`9d3a20e597a0014dbcd985612c8d9ea19877395cc3de663b6d6e88fa1629587a`. Both were
independently downloaded and verified.

Both host legs confirmed the pinned fixture digest on Linux x86_64 and macOS arm64,
which is the property that matters for a digest whose purpose is comparison across
time and machines. The portable job resolved the core for bare-metal RISC-V with
`core_affinity` absent from the target graph.

## Supersession

None. This closes the threaded blitter that ADR-0002 and ADR-0005 recorded as a
remaining `std` service, and it leaves emulated clock and host yield contracts to
M1-006.
