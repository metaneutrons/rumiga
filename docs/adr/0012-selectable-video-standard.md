# ADR-0012: Selectable Video Standard

- Status: Accepted
- Date: 2026-08-18
- Owners: @metaneutrons
- Task: M1-013

## Context

The emulator was PAL only, and the `--ntsc` flag was inert. It printed a warning and
changed nothing: a 1200-frame Kickstart 46.143 capture taken with `--ntsc` was
byte-identical to the same capture without it. The interface offered a choice the
machine could not make.

PAL was not merely the default; it was compiled in at five independent places. The
frame loop iterated a PAL constant, the framebuffer line filter compared against a
PAL-derived constant, `frame_period` divided by the PAL colour clock, `BEAMCON0`
reported a literal `BEAMCON0_PAL`, and the beam wrapped at a literal line 311 in the
scanline loop with a second PAL constant in `advance_beam`. Any of them could have
been changed alone, which is how a half-implemented standard would arise.

Two of those five were coupled in a way that is not obvious. The frame length and the
beam wrap must agree: if the loop runs 262 lines while the beam wraps at 311, the beam
does not return to the top of the frame and drifts by 50 lines per frame. Guest code
waiting for a specific line would then wait a frame too long or never see it.

## Decision

One type, `VideoStandard` in `rumiga-core::video`, answers every question that differs
between the two standards: line count, colour clock, active height, last line, first
line after vertical blank, `BEAMCON0` value, the `VPOSR` standard bit, the frame
period, and a digest tag. Callers ask it rather than deciding for themselves, so a
third standard cannot be added by changing four of five sites.

Every value is sourced from `WinUAE`, which the display geometry constants in
`playfield` already followed:

| Value | `WinUAE` symbol | PAL | NTSC |
| --- | --- | --- | --- |
| Total scanlines | `MAXVPOS_PAL` / `MAXVPOS_NTSC` | 312 | 262 |
| Colour clocks per line | `MAXHPOS_PAL` / `MAXHPOS_NTSC` | 227 | 227 |
| Colour clock | `CHIPSET_CLOCK_PAL` / `CHIPSET_CLOCK_NTSC` | 3 546 895 Hz | 3 579 545 Hz |
| Active height | `AMIGA_HEIGHT_MAX_PAL` / `AMIGA_HEIGHT_MAX_NTSC` | 576/2 = 288 | 486/2 = 243 |
| First line after vertical blank | `VBLANK_ENDLINE_PAL` / `VBLANK_ENDLINE_NTSC` | 26 | 21 |
| `BEAMCON0` reset value | `BEAMCON0_PAL` | `0x0020` | `0x0000` |
| `VPOSR` standard bit | `csbit` in `VPOSR()` | clear | `0x1000` |

The standard is a field on `MemoryConfig` rather than a consequence of the model
profile, because both standards were sold for every model this crate models. It is
also digested into `state_digest`, so two machines that differ only in standard are
not reported as being in the same state.

The framebuffer stays PAL-sized and constant. PAL is the taller standard, so a
PAL-sized buffer holds an NTSC frame with room to spare, and no buffer is resized at
runtime. A compile-time assertion enforces that this remains true, and the emulator
publishes `active_height` so a presenter crops to the picture instead of emitting the
45 lines the chipset never wrote under NTSC.

`VPOSR` gained a single implementation on `CustomChipState`. It previously had two
that disagreed: the register shadow the guest reads included the `LOF` bit while the
direct register read omitted it. Adding a standard bit to both would have preserved
the disagreement, so they were merged instead.

## Consequences

`--ntsc` now selects a machine. The same Kickstart ROM boots both standards; there is
no separate NTSC ROM, because the standard is reported by the chipset rather than
compiled into the ROM.

The guest detects the standard and acts on it. Under `--ntsc`, Kickstart 46.143 writes
`DIWSTRT` `0x1595` and `DIWSTOP` `0x06AD`, a window from line 21 to line 262, against
PAL's `0x1D95`/`0x38AD` from line 29 to line 312. The two stop lines are exactly the
two standards' line counts, which the guest can only have derived from the standard it
read. The start lines differ too, though neither equals its `VBLANK_ENDLINE` constant,
so the window is an overscan screen rather than the classic 44/256 and 44/200 defaults.

PAL output is unchanged. A 1200-frame A1200 insert-disk capture is byte-identical
before and after this change, and the 60-frame capture keeps the digest recorded for
M1-005 and M1-006.

The Agnus revision is still not modelled. `VPOSR` reports revision `0x00`, which is
OCS, and only the standard bit is now variable. That matches how `WinUAE` reports an
OCS machine, and the NTSC detection above shows the ROM acts on the bit, but a guest
that identifies ECS or AGA Agnus by its revision field still sees OCS on every
profile, as it did before.

The frame period is now standard-dependent, so a shell that paces against
`Emulator::frame_period`, as ADR-0011 requires, follows the standard with no further
change. An NTSC frame is 16.615 ms, implying 60.19 Hz.

## Alternatives

Sizing the framebuffer per standard was rejected. NTSC is the shorter standard, so a
runtime-sized buffer would add allocation and a resize path to gain nothing. The
compile-time assertion documents why the constant is safe.

Deriving the standard from the model profile was rejected. Both PAL and NTSC machines
of every modelled model were sold, so tying them would make one of the two
unreachable for each model.

Scattering the per-standard values across the modules that use them was rejected for
the reason the context gives: five independent PAL constants are five chances to
implement a standard halfway, and the beam wrap and frame length must agree or the
beam drifts.

Rounding the NTSC frame to 60 Hz or to broadcast NTSC's 59.94 Hz was rejected. The
Amiga's NTSC frame is 227 × 262 colour clocks at 3 579 545 Hz, which is 60.19 Hz.
Both of the usual figures are wrong for this machine, as ADR-0011 records for PAL.

Keeping the two `VPOSR` implementations and adding the standard bit to each was
rejected. One register with two answers is a defect regardless of the standard, and
the direct read was the one that omitted `LOF`.

## Evidence

`cargo +1.97.1 test --locked -p rumiga-core --lib` pins every constant above in both
runtime profiles and asserts that the beam wrap agrees with the frame length. That
last test fails on an implementation that changes the frame length alone: it reports
the beam sitting at line 262 instead of back at the top.

On the host, `--ntsc` produces a stable image across three consecutive 1200-frame
captures, and the recorded manifest shows the guest's own display window changing with
the standard. A PAL capture from the pre-change revision is byte-identical to one
taken after it.

Hosted promotion confirms the model beyond the development host. Pull-request run
`32127572185` and final `main` run `32128162254` passed all ten required jobs, and every
`video::tests` case together with every standard-related `emulator::tests` case appears
twice in each host job log, once per explicit runtime profile. The beam wrap agreement in
particular is a property of the frame loop rather than of one host, so seeing it hold on
Linux x86_64 and macOS arm64 under both profiles is what makes the single-owner design
worth its cost.

## Supersession

None. This closes the video standard entry that ADR-0011 left open. Agnus revision
identification, interlace, and long/short frame alternation remain unmodelled.
