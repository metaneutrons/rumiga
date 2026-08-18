# ADR-0013: Platform Capabilities And Typed Errors

- Status: Accepted
- Date: 2026-08-18
- Owners: @metaneutrons
- Task: M1-007

## Context

`ARCHITECTURE.md` states that platform contracts are versioned and capability-driven,
and that methods which can fail or block return explicit results. Neither held.

There was no version. A backend built against an older contract would have been
accepted, and the disagreement would have surfaced as a confusing failure inside
whichever method changed shape, or not at all.

There was no way to ask what a backend supports. `AudioOutput` and `Storage` had no
implementor anywhere in the workspace, so a shell calling them would have found out by
linking failure at best and by silence at worst.

Failure was discarded rather than reported. `VideoOutput::present_frame` returned `()`,
and the desktop adapter wrote `let _ = window.update_with_buffer(...)`. A dead window
was indistinguishable from a healthy one: the shell kept emulating and kept measuring a
frame rate for frames that never reached a display. `AudioOutput::queue_samples` also
returned `()`, so a backend had no way to say it had taken only part of what it was
given.

A second error model existed in parallel. `StorageError` duplicated variants that a
platform-wide error type needs anyway, so a shell would have had to translate between
two enums that mean the same things.

## Decision

Failure and flow control are modelled separately, because they are different questions.

`PlatformError` reports that an operation could not be carried out, with `Unsupported`
as the explicit answer for a service a backend does not implement. It replaces
`StorageError`, so there is one error model rather than two.

Backpressure is not an error. A display that was not ready and an audio queue that is
full are both working as designed, so they are reported on the success path:
`FramePresentation` says whether a frame reached the display, and `SamplesQueued` says
how many frames were taken and how many were refused. Folding either into the error
type would make normal operation indistinguishable from a fault, which is the mistake
this decision exists to avoid.

Absence is representable twice, deliberately. `PlatformCapabilities` describes a
backend before anything is called, using `Option` per service so an absent service is
structurally absent rather than signalled by zeroed limits: a caller cannot mistake "no
audio" for "audio at 0 Hz on 0 channels". A caller that ignores the descriptor and calls
anyway still receives `PlatformError::Unsupported`. The descriptor is the polite path;
the error is the backstop.

`CONTRACT_VERSION` versions the contract set, a backend reports the version it was built
against, and `PlatformCapabilities::validate` rejects a mismatch. The desktop shell calls
it before it builds anything on top of the backend, on both the interactive and the
capture path.

A backend states whether it can report backpressure at all. The desktop reports `false`,
because minifb either presents or fails and its own rate limiting blocks rather than
refusing. Without that flag a shell would read a dropped-frame count of zero as evidence
of health when it is evidence of nothing.

## Consequences

The desktop shell now acts on the presentation result. A transport failure prints and
ends the loop instead of being discarded, and refused frames are counted and reported at
shutdown. On this backend that count stays at zero, which the capability flag explains.

The startup summary reports the contract version, the video bounds, whether backpressure
is reported, and whether audio is present. What the backend can do is visible before the
first frame rather than inferred from behaviour.

`PlatformCapabilities` bounds are supplied by the caller. `DesktopBackend::new` takes the
framebuffer bounds the shell actually uses, so the reported maxima cannot drift from the
buffer that is allocated.

The rendered output is unchanged. A 1200-frame A1200 capture keeps the digest recorded
for M1-013.

Three contracts remain unimplemented by any backend: `AudioOutput`, `Storage`, and the
services `ARCHITECTURE.md` lists for later milestones. They now describe their failures
in the shared error model, which is a smaller claim than being implemented.

## Alternatives

A `WouldBlock` or `Backpressure` variant on `PlatformError` was rejected. It reads well
in a signature and is wrong in use: every caller would have to treat one error variant as
success, and any code that logs errors would report normal flow control as a fault.

Reporting only a count from `queue_samples` was rejected. A caller would then have to
compare the returned count against what it sent to learn whether backpressure occurred,
which is the inference the explicit split removes.

Keeping `StorageError` alongside `PlatformError` was rejected. It has no implementor, so
the migration cost is zero now and grows with every backend added later.

Deriving capabilities from the trait implementations, for example treating a missing
`AudioOutput` implementation as absence, was rejected. That information is not available
at runtime, and it would tie the descriptor to Rust's type system rather than to what the
hardware offers.

Zeroed limits instead of `Option` were rejected. `sample_rate_hz: 0` is a value a buggy
backend can also report, so absence and misconfiguration would be indistinguishable.

## Evidence

`cargo +1.97.1 test --locked -p rumiga-platform` exercises both states this task is
about, using a backend double: the desktop backend can produce neither, since it has no
audio at all and its display never refuses. The double reports `audio: None` and returns
`Unsupported` when called anyway, and refuses a frame so that the dropped outcome is
reachable, distinguishable from a presented one, and distinguishable from an argument
error.

The version rejection was verified against a probe rather than only asserted. A desktop
backend was temporarily changed to report a version seven ahead of the contract; the
shell exited with status 1, named both versions, and wrote no capture. The probe was then
removed and the capture digest re-checked.

Hosted promotion confirms the contract beyond the development host. Pull-request run
`32130769524` and final `main` run `32132116892` passed all ten required jobs, the 14
contract tests and the 5 desktop capability tests passed on Linux x86_64 and macOS arm64,
and the portable job checked `rumiga-platform` for `riscv32imafc-unknown-none-elf`.

That last point is compilation, not execution. The host gate's explicit `std` and `no_std`
matrix covers `rumiga-core` and `m68k`, so what is shown for these types is that they
build for a bare-metal target, which is the claim the portable gate is designed to make.

## Supersession

None. This closes the capability and typed-error entries in the platform contract table.
The bounded queues and high-water marks that `AudioCapabilities::max_queued_frames`
describes belong to M1-008, which owns the queue itself.
