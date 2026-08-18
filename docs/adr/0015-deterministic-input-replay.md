# ADR-0015: Deterministic Input Replay

- Status: Accepted
- Date: 2026-08-18
- Owners: @metaneutrons
- Task: M1-009

## Context

A state digest existed since M1-005, but nothing could reproduce a session. A bug that
appeared after a particular sequence of keystrokes and pointer motion could only be
described in prose, and a fix could not be shown to address the same sequence.

The machine also had no notion of a frame. `run_frame` incremented nothing, and every
frame counter in the tree belonged to a shell. A recording stamped with a shell's counter
would mean whatever that shell happened to count.

The digest covered less than it appeared to. It hashed the CPU, cycles, custom registers,
part of the chipset, and chip RAM. It did not cover slow RAM, fast RAM, CIA state, pending
input, or drive state, so two runs could differ in any of those and be reported as
identical.

## Decision

Events are stamped with the emulated frame they belong to, and the machine counts frames.
Host time would be the obvious alternative and is the wrong one: a recording stamped with
wall-clock instants replays differently on a faster or busier machine, which is the
property replay exists to remove. ADR-0011 already keeps host time out of the core; this
is where that pays.

The core records, not the shell. The three input entry points are the only way input
reaches the machine, so recording inside them makes a recording complete by construction.
A shell that recorded at its own call sites would be correct only while every shell
remembered to, and a second shell would silently produce short recordings.

`run_frame` applies the current frame's events itself rather than expecting the caller to.
Ordering between input and emulation is then a property of the machine. A shell that
applied events at the wrong point would produce a different digest from the same
recording, and nothing would flag it.

Input application lives in one place per action. The first version of the replay path
reimplemented the effects and updated the mouse deltas but not the mouse counters, so a
recorded run and its replay diverged. The determinism test caught it. The fix was not to
correct the copy but to remove it: `key_event`, `mouse_move`, and `mouse_button` now record
and then call private `apply_*` helpers that replay also calls.

The recording format is text with a version header, one event per line. A recording can
then be read, diffed, and hand-written in a review, and a difference between two
recordings is legible rather than a hex dump. Frames must not decrease; a recording that
jumps backwards is rejected rather than sorted, because silently sorting would hide a
corrupted or hand-merged file.

The state digest was widened to cover the frame counter, pending input including the
keyboard queue's contents and its dropped count, the mouse deltas, counters, and buttons,
both CIA chips, slow and fast RAM, and per-drive metadata. Media contents moved into a
separate `media_digest`, because hashing a hardfile can cost gigabytes of reads that a
caller comparing state after every frame should not pay.

## Consequences

The digest widening is what makes the replay claim meaningful. Two recordings differing
only in one keycode initially reached the same digest: the keystroke had already been
consumed into the CIA serial register, which the digest did not cover. Without that fix,
"the same replay yields the same digest" would have held for the uninteresting reason that
the digest could not see the difference.

Recorded evidence names its input. Capture manifests gain the frame count, the number of
replayed events, the recording's digest, whether replay is exhausted, and the state, frame,
and media digests.

The rendered frame turns out to be a weak instrument for comparing runs. A replay that
presses keys and moves the pointer on the Kickstart insert-disk screen produces an image
byte-identical to a run with no input at all, because that screen does not react. The state
digest separates them; the frame digest does not. Anyone comparing runs by screenshot alone
would have concluded the input never arrived.

Recording and replay are mutually exclusive at the shell. Recording a replay would copy the
input file back out while claiming to have observed it.

Rendered output is unchanged. A 1200-frame A1200 capture keeps the digest recorded since
M1-013.

## Alternatives

Stamping events with host time was rejected for the reason the decision gives.

Counting frames in the shell was rejected. The index would then mean whatever a particular
shell counted, and two shells could disagree about the same recording.

Letting the shell apply replayed events was rejected. It puts the ordering between input
and emulation outside the machine, where a mistake changes the digest silently.

A binary recording format was rejected. Text costs a little size and buys reviewable
diffs, hand-written scenarios, and error messages that name a line.

Sorting an out-of-order recording was rejected. Rejecting it surfaces a corrupted or
badly merged file instead of quietly producing a session nobody recorded.

Digesting media contents inside `state_digest` was rejected on cost. A hardfile can be
gigabytes, and a caller that digests state per frame would pay for it every time.

Extending the state digest to cover every field of the machine was not attempted. The
additions here are the ones that input replay can actually reach; the remaining gaps are
recorded below rather than claimed as covered.

## Evidence

`cargo +1.97.1 test --locked -p rumiga-core --lib` covers the format round trip, the
rejection cases with their line numbers, the recorder skipping unchanged button state and
zero deltas, and the replay itself: the same recording reaches the same digest twice, a
recording differing in one keycode reaches a different digest, each event lands on its own
frame, and a recorded session replays to the digest it was captured from. All of it holds
in both runtime profiles.

On the host, three replays of a hand-written scenario reach state digest
`0x3530b85cc280ec97`, while the same run with no input reaches `0x5446697654ab27f7`. All
four share frame digest `0x6d7c2de83b7b6725`, which is what makes the separate state digest
necessary rather than decorative.

## Supersession

None. This closes the deterministic replay and machine-state digest entries.

The state digest still does not cover the copper and blitter register shadows beyond what
`custom_regs` holds, the audio channel state, the floppy MFM track buffers, or the IDE
controller's transfer state. Replay determinism is also conditional on the network being
disabled, which is the default: the SLIRP backend injects host-received Ethernet frames
into the machine, and those are not recorded. Recording them is separate work.
