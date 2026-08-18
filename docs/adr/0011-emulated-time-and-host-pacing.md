# ADR-0011: Emulated Time And Host Pacing

- Status: Accepted
- Date: 2026-08-18
- Owners: @metaneutrons
- Task: M1-006

## Context

The core already read no host clock: `rumiga-core` and `m68k` contained no
`Instant`, `SystemTime`, `sleep`, or `elapsed`. Nothing enforced that, though, and
the two previous host services that leaked into the core, the trace file and the
blitter thread, both hid behind `#[cfg(feature = "std")]`. A clock would have had
the same escape route.

The shell had the opposite problem. The desktop frame loop ran flat out and slept
16 ms only when no frame was ready, so nothing enforced the 50 frames per second
that `ROADMAP.md` states as the PAL target. The REST interface reported `fps` as a
hardcoded `50.0`, publishing a constant as if it were a measurement.

## Decision

`rumiga-platform` gains a `Clock` contract with a monotonic `now` and a `pace` that
returns the time the host actually spent. The return value is the measurement, not
the request: a host sleep routinely overshoots, so a caller that needs to pace must
correct against what it is told rather than what it asked for. `rumiga-platform-desktop`
implements it as `DesktopClock`.

The core declares its emulated frame duration through `Emulator::frame_period`,
derived from the colour clock and the scanline count rather than a rounded rate. A
PAL frame is 19,967,887 ns, so the frequently quoted 20 ms is wrong by 32
microseconds per frame and the implied rate is PAL's 50.08 Hz. `core::time::Duration`
is emulated time here, not host time; the distinction is the point.

The shell paces against that declared period rather than a constant of its own, so
pacing follows automatically when M1-013 makes the video standard selectable. The
reported frame rate is measured over a 500 ms window.

The core is prevented from naming a host clock type. `crates/rumiga-core/clippy.toml`
disallows `std::time::Instant` and `std::time::SystemTime`, and `lib.rs` denies
`clippy::disallowed_types`.

## Consequences

The guarantee covers the feature-gated path. This was verified by temporarily adding
a `std`-gated function returning `Instant`; the lint rejected it. That is the escape
route the trace file and the blitter thread had used, so closing it is the point of
choosing a lint over the structural argument that the portable gate already fails
when the core uses `std`.

A source text search was deliberately not used. ADR-0005 rejected that approach
because conditional compilation, comments, and legitimate host adapters make it
brittle, and that reasoning applies unchanged here.

The desktop now requests the correct frame period and reports what it achieves.
Whether it sustains the paced rate under load is not established by this change; the
change is the precondition for that measurement rather than the measurement itself.

The headless capture path is unaffected. A 60-frame Kickstart 46.143 capture keeps
its digest, so pacing touches only the interactive loop and evidence generation stays
independent of host timing.

## Alternatives

Pacing against a constant frame period in the shell was rejected because it would
silently disagree with the core once the video standard becomes selectable. Deriving
it from the core makes one of them authoritative.

Reporting the requested duration from `pace` was rejected because it would let a
caller believe it paced correctly while drifting. The contract returns a measurement
precisely so drift is visible.

Relying only on the portable gate to keep the core clock-free was rejected. That gate
fails when the core uses `std` unconditionally, but a `#[cfg(feature = "std")]` clock
would pass it, which is exactly how the two previous leaks survived.

Rounding the PAL frame to 20 ms was rejected. It is a 32 microsecond error per frame,
about 1.6 ms per second, which would be visible as audio drift over minutes and would
make any later timing comparison rest on the wrong constant.

## Evidence

`cargo +1.97.1 test --locked -p rumiga-core --lib` pins the frame period in both
runtime profiles. `cargo +1.97.1 test --locked -p rumiga-platform-desktop` covers
monotonicity, that `pace` never reports less than requested, that a zero request
yields, and that `now` advances across a `pace` call. `cargo +1.97.1 xtask ci` passes
all eight gates. The 60-frame capture digest is unchanged from before the task.

Hosted promotion confirms the contract beyond the development host. Pull-request run
`32107349807` and final `main` run `32108657023` passed all ten required jobs, and both
host legs ran the four `DesktopClock` contract tests together with the frame period test
in both runtime profiles. This matters for two of the assertions in particular: `now`
monotonicity and `pace` never reporting less than requested are properties of the host
clock, so seeing them hold on Linux x86_64 and macOS arm64 under CI load is stronger
evidence than a single-machine run.

The ban on host clock types is enforced in CI rather than only locally. The host gate
runs `rumiga-core` Clippy separately under the explicit `std` and `no_std` profiles with
`-D warnings`, so a future `#[cfg(feature = "std")]` clock in the core fails a required
job rather than a developer's local habit.

## Supersession

None. This closes the clock and yield entry in the platform contract table and leaves
bounded queues and typed errors to M1-007 and M1-008.
