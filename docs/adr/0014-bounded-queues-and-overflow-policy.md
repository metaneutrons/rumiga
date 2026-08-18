# ADR-0014: Bounded Queues And Overflow Policy

- Status: Accepted
- Date: 2026-08-18
- Owners: @metaneutrons
- Task: M1-008

## Context

`ARCHITECTURE.md` states that queue overflow policies are part of the platform contract
rather than implementation details. One queue existed and it stated nothing.

`Emulator::key_event` held guest keyboard events in a `Vec` and guarded it with
`if self.key_events.len() < MAX_KEY_EVENTS`. Past sixteen events the push was skipped and
nothing recorded it. The queue drains one event every three frames, about seventeen per
second under PAL, so a key-repeat burst or fast typing exceeds it in normal use. A guest
that missed a keystroke and a guest that received all of them looked identical from
outside, and there was no counter to consult.

Two further problems came with the shape rather than the bound. The overflow behaviour was
a consequence of where the length check sat rather than a stated decision, so nobody could
tell whether refusing the newest event was intended or incidental. Draining used
`Vec::remove(0)`, which shifts every remaining element on each event.

## Decision

`BoundedQueue<T>` in `rumiga-platform` carries a fixed capacity, a named
`OverflowPolicy`, and two counters. `push` returns `QueueAdmission`, so the effect of the
policy is visible at every push instead of being inferred by comparing lengths.

The policy is named per queue rather than chosen once for the crate, because the right
answer depends on what the items mean. `RejectNewest` keeps what is already queued, which
is what a full keyboard buffer does: the keystrokes are lost at the source rather than
reordered. `DropOldest` keeps the freshest items, which is what an audio queue wants,
since stale sound is worse than missing sound. A queue that does not state its policy
pushes the decision to a caller who cannot make it.

The counters exist because a queue that filled and then drained is otherwise
indistinguishable from one that was never busy. `high_water` records the deepest the queue
ever got and never decreases; `dropped` counts what the policy lost. `clear` empties the
queue and deliberately keeps both, because they describe history and a reset would hide a
saturation episode from whatever reports it later.

Capacity is fixed at construction. Growing under load would trade a visible loss for an
invisible latency increase and an unbounded allocation, which is the failure mode a bound
exists to prevent.

The guest keyboard queue adopts the type with capacity sixteen and `RejectNewest`, which
is exactly the behaviour the unnamed length check already had. The policy is now stated
and its effect counted; what the guest observes is unchanged.

`InputCapabilities` gained `max_events_per_poll`. `InputState::key_events` is an unbounded
`Vec` filled by the adapter, so the bound a consumer can size against belongs in the
descriptor rather than in a comment.

## Consequences

`Emulator::key_event` returns `QueueAdmission`. The desktop does not handle it per event,
because the aggregate is what a user can act on; it reports lost events and the peak depth
at shutdown, and capture manifests record the capacity, the policy, and all three counters.
In a capture run those read zero, which documents that no input pressure could have
influenced the frame.

Draining the keyboard queue is now `pop_front` on a `VecDeque` rather than `remove(0)` on a
`Vec`. That is incidental to the contract and was not the motivation.

The type is generic and ready for the audio and video queues the architecture describes,
but no such queue is created here. `AudioOutput` and `Storage` still have no backend, so
instantiating queues for them would add code with no producer and no consumer. The bound
`AudioCapabilities::max_queued_frames` declares therefore still describes an intention
rather than an enforced limit.

## Alternatives

A single crate-wide overflow policy was rejected. Keyboard input and audio want opposite
answers, so one policy would be wrong for one of them.

Returning `bool` from `push` was rejected. It cannot distinguish "queued with room" from
"queued by evicting something", and eviction loses an item while still accepting the new
one. `QueueAdmission::lost_an_item` and `queued` answer the two questions separately for
exactly that reason.

Resetting the counters on `clear` was rejected. A shell that clears a queue on reset would
erase the evidence that the queue had saturated before the reset.

Growing the queue when full was rejected. It converts a bounded, visible loss into an
unbounded allocation and a latency increase that nothing reports.

Making the return value of `Emulator::key_event` `#[must_use]` was rejected. Both desktop
call sites would need `let _ =`, which reads exactly like the silent discard this task
removes while communicating nothing; the aggregate counters are what make the loss visible.

Instantiating audio and video queues now was rejected. A queue with no producer and no
consumer cannot be tested against real pressure, and the acceptance criterion asks for
tested behaviour rather than declared structure.

## Evidence

`cargo +1.97.1 test --locked -p rumiga-platform` covers both policies including that they
disagree about which item survives, the admission outcomes, the high-water mark surviving a
drain, the counters surviving `clear`, and a zero capacity losing everything under either
policy.

`cargo +1.97.1 test --locked -p rumiga-core --lib` covers the real consumer: filling the
keyboard queue to capacity and pushing once more returns `Rejected`, the dropped counter
reads one, the first queued event is still at the head, and the high-water mark outlives a
full drain. All of it holds in both runtime profiles.

The rendered output is unchanged. A 1200-frame A1200 capture keeps the digest recorded for
M1-013 and M1-007.

Hosted promotion confirms both halves of the claim. Pull-request run `32137756215` and
final `main` run `32138307307` passed all ten required jobs. The queue tests passed on
Linux x86_64 and macOS arm64, and the three keyboard queue tests passed twice on each leg,
once per explicit runtime profile, because `rumiga-core` is in the host gate's matrix while
`rumiga-platform` is not. For the contract type the bare-metal evidence is that the
portable job compiles it for `riscv32imafc-unknown-none-elf`, not that its tests run there.

## Supersession

None. This closes the queue-contract entry in the platform contract table. Enforcing the
audio bound belongs with the audio backend that first needs it.
