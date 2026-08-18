# ADR-0017: Portability Boundaries

- Status: Accepted
- Date: 2026-08-18
- Owners: @metaneutrons
- Task: M1-011

## Context

The core targets a 32-bit RISC-V device and is developed on 64-bit hosts. Two
differences between them can produce a core that passes every host test and behaves
wrongly on the device: `usize` is narrower there, and the guest is big-endian while both
the hosts and the target are little-endian.

Neither had any stated assumption. There was no assertion anywhere about
`size_of::<usize>()` or `target_pointer_width`, and nothing prevented a native-endian
conversion from being introduced.

## Decision

Miri is not used, and the plan's mention of it is answered rather than followed. Miri
detects undefined behaviour; the workspace sets `unsafe_code = "forbid"`, so there is no
raw pointer arithmetic that could produce any. Silent truncation and a wrong byte order
are both well-defined behaviour, so Miri would report neither. The plan offers property
fixtures as an alternative and that is what this task provides, with two enforced
invariants alongside them.

Byte order is enforced, not merely conventional. The workspace already satisfied the
property: 35 explicit big-endian conversions and zero native-endian ones. That is a
property of today's code rather than of the design, so `crates/rumiga-core/clippy.toml`
now bans `from_ne_bytes` and `to_ne_bytes` on the integer types through
`disallowed_methods`, and `lib.rs` denies the lint. This is the same mechanism ADR-0011
used to keep host clocks out of the core, chosen for the same reason: a comment does not
survive a future contributor, and a source-text search is the approach ADR-0005 rejects.

Pointer width is asserted at compile time. `lib.rs` asserts that `usize` is at least as
wide as `u32`, so a build for a narrower target fails with a message naming the reason
instead of truncating every guest address silently. A second assertion states that a
chip RAM length fits in `u32`, which several sites assume when masking a guest pointer
against the RAM size.

The property fixtures cover what the invariants cannot: that a guest address survives
conversion through `usize` across the whole 32-bit range including the boundaries, that
word and long access through the CPU's own bus is big-endian in both directions with the
individual bytes checked rather than only the round trip, that every modelled RAM length
fits in `u32`, and that the framebuffer index space fits in 32 bits.

## Consequences

The audit found no production defect in this class, which is a result rather than an
absence of work. Of the 26 sites carrying an explicit `allow` for a lossy cast, all are
narrowings between fixed-width types such as `u32` to `u16`; those behave identically at
either pointer width and are value-range questions, not portability questions. The one
site that multiplies before casting to `usize` is a test helper.

The `allow` sites were the right audit list precisely because the workspace denies
`clippy::pedantic`, which includes `cast_possible_truncation`. Every risky cast is
therefore already marked, and the list is 26 entries rather than the 166 `as usize`
occurrences a naive search returns.

Both invariants were verified against probes rather than assumed. A temporary
`u32::from_ne_bytes` was rejected with `use of a disallowed method`; a temporary
assertion requiring a 128-bit `usize` failed the build with its own message. Both probes
were removed.

## Alternatives

Running the test suite with a 32-bit `usize` was rejected as impractical rather than
undesirable. It is the direct check, but no 32-bit target with a usable `std` exists on
the development host, and an `i686` CI leg would exercise a pointer width the product
never runs while adding a toolchain dependency. The pointer-width claim therefore rests
on compile-time assertions plus the existing `riscv32imafc` compile gate, which is
weaker than execution and is recorded as such.

Adding Miri anyway was rejected. It would add a nightly CI leg that cannot detect either
failure mode this task is about, which is worse than no leg because it would look like
coverage.

Auditing all 166 `as usize` occurrences was rejected in favour of the 26 `allow` sites.
A `u32 as usize` is lossless at every supported pointer width; treating it as a finding
would bury the sites that can actually lose data.

Converting the assertions to runtime checks was rejected. A truncating pointer width is
a property of the build, not of a run, so the build is where it should fail.

Asserting that the host is little-endian was considered and rejected. A big-endian host
would be equally correct; it would only make host tests less able to distinguish a
native-endian mistake, and failing a build over that would be hostile for no gain. The
fixture documents the fact instead.

## Evidence

`cargo +1.97.1 test --locked -p rumiga-core --test portability_boundaries` covers the
seven boundary properties. `cargo +1.97.1 clippy` under both explicit runtime profiles
enforces the native-endian ban, and `cargo +1.97.1 build` enforces the pointer-width
assertions. Both invariants were probe-verified as described above.

Hosted promotion confirms both instruments. Pull-request run `32172280478` and final `main`
run `32174822015` passed all ten required jobs, all seven fixtures appear twice per host leg,
once per explicit runtime profile, and the portable job compiles the core for
`riscv32imafc-unknown-none-elf`. That last point matters here specifically: the compile-time
pointer-width assertions are evaluated for the 32-bit target itself, not only for the 64-bit
hosts. Execution with a 32-bit `usize` remains unclaimed.

## Supersession

None. This closes the 32-bit assumption, alignment, and endianness measurement entry.

Alignment is not separately instrumented. The core stores guest memory as byte slices and
composes wider values from bytes, so it makes no alignment assumption to violate; that is
a consequence of the design rather than something this task enforces. The desktop shell's
own conversions are outside the ban, which applies to `rumiga-core`.
