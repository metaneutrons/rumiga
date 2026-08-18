# ADR-0016: Allocation Instrumentation

- Status: Accepted
- Date: 2026-08-18
- Owners: @metaneutrons
- Task: M1-010

## Context

The product target is an ESP32-P4 with 32 MiB of PSRAM. A frame loop that allocates
steadily is a latency and fragmentation problem there long before it is a capacity
problem, and nothing measured whether the loop allocated at all.

Reading the source suggested one culprit: the copper path built a fresh `Vec` for pending
register writes on every scanline. That reading produced an upper bound of 312 allocations
per frame, which turned out not to be the interesting number.

## Decision

Measure before fixing. The order is the substance of this decision, not process decoration:
both allocation figures in this task contradicted what the source suggested, and a fix
applied first would have left nothing to compare against.

The counting allocator comes from outside the workspace. `unsafe_code = "forbid"` is set at
the workspace root and cannot be relaxed per crate or per target, while a counting
`#[global_allocator]` requires `unsafe impl GlobalAlloc`. Writing one would mean either a
crate that opts out of the invariant or a hole in it, so `stats_alloc` is a dev-dependency
instead: its unsafe is its own, the workspace keeps its invariant, and the supply-chain gate
reviews the crate like any other.

The emulator also reports its own buffer capacities. A total allocation count says that
something allocated, not what; a capacity that stops growing names the buffer. The capacity
accessors work in the `no_std` profile and on a device, where a global allocator hook is not
available, so the two instruments are complementary rather than redundant.

Two buffers are retained across scanlines instead of rebuilt: the copper pending writes and
the guest register writes drained from the memory log. Both are moved out with
`core::mem::take`, cleared, filled, drained, and put back, which keeps their capacity.

Clippy suggests `into_iter()` in place of `drain(..)` at both sites. Following it would
consume the buffer and reintroduce exactly the allocation being removed, so the lint is
allowed with that reason stated at each site.

The evidence is two tiers, and the gate enforces the weaker one. A one-minute run needs a
real Kickstart, and ROMs are not committed, so CI runs a 64-frame test with a fixture that
reaches both allocation paths. The one-minute figure is measured locally and recorded.

## Consequences

The measured numbers, not the predicted ones:

| Measurement | Before | After |
| --- | --- | --- |
| 64-frame fixture, copper only | 64 allocations | 0 |
| One minute of real Kickstart, 3005 frames | 978 521 allocations, 3.95 MB | 0 |

The first fix left the second defect in place and the synthetic test passed anyway. With
the copper buffer retained, the fixture reported zero while a real boot still allocated
978 521 times per minute, because a booting guest writes custom registers on nearly every
scanline and the fixture reached that path never. This is the failure mode the task exists
to prevent, and it survived the first round of work.

The fixture was therefore strengthened rather than accepted. It now runs a two-instruction
68k loop in the guest that writes a colour register, so guest register writes occur on every
scanline. Reverting the fix with that fixture in place fails on the allocation count itself,
at 658 944 allocations over 64 frames, not merely on a guard.

Both guards are load-bearing. The test asserts that each buffer's capacity is non-zero after
warmup, so a future change that stops reaching a path fails loudly instead of quietly
measuring a quieter loop.

Emulated behaviour is unchanged. The state digest after 3605 frames of a real boot is
`0xc2d77aefee1ec32c` before and after, and the 1200-frame capture keeps its recorded digest.

## Alternatives

Writing the counting allocator in the workspace was rejected: `forbid` makes it impossible
without either a crate that opts out or removing the invariant, and the invariant is worth
more than the convenience.

A new workspace member permitted to contain unsafe was rejected for the same reason.

Measuring only through capacity accessors, with no allocator at all, was rejected as the
sole instrument. It cannot answer whether anything allocated, only whether these buffers
grew, and the second defect was found by the count.

Measuring only through the allocator, without capacity accessors, was rejected because the
count does not attribute and does not work in the `no_std` profile that the device runs.

Growing the retained buffers on demand and never shrinking them is accepted rather than
capped. Both are bounded by the work one scanline can produce, and the measurement shows
them settling at 64 and 32 entries.

Making the CI test run a real ROM was rejected: ROMs are not committed, so the test would
skip in CI and the gate would enforce nothing.

## Evidence

`cargo +1.97.1 test --locked -p rumiga-core --test allocation_test` runs 64 frames with the
copper active and a guest loop writing registers, and asserts zero allocations and zero
reallocations. Its own binary holds one test, because a global allocator counts every thread
in the process.

Locally, against Kickstart 46.143 on an A1200 profile, one minute of PAL is 3005 frames and
allocates nothing. The same measurement before the second fix reports 978 521 allocations
and 3 949 644 bytes.

Reverting either fix fails the test. Reverting the guest register write buffer reports
658 944 allocations over 64 frames.

Hosted promotion confirms it. Pull-request run `32170645437` and final `main` run
`32171339632` passed all ten required jobs, and the steady-state assertion appears twice per
host leg, once per explicit runtime profile. The Supply Chain Policy job passing is the
material result for the dev-dependency this decision introduces.

## Supersession

None. This closes the allocation instrumentation entry.

The measurement covers the core's frame loop. The desktop shell allocates per frame in its
presentation and screenshot paths, which this does not measure and does not claim to; the
shell is not the loop the device will run. Peak resident memory is also out of scope: this
counts allocation calls, not footprint.
