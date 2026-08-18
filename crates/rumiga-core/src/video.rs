// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Video standard selection: everything that differs between PAL and NTSC.
//!
//! One type answers every question a caller can ask about the difference, so a
//! new standard cannot be half-implemented by adding a constant in one module
//! and forgetting another. The alternative, scattering `if ntsc` across the
//! frame loop, the register shadow, and the renderer, is how the two host-service
//! leaks that M1-004 and M1-005 closed survived for so long.
//!
//! Every value here is sourced from `WinUAE`, which the display geometry
//! constants in [`crate::playfield`] already follow:
//!
//! | Value | `WinUAE` symbol | PAL | NTSC |
//! | --- | --- | --- | --- |
//! | Total scanlines | `MAXVPOS_PAL` / `MAXVPOS_NTSC` | 312 | 262 |
//! | Colour clocks per line | `MAXHPOS_PAL` / `MAXHPOS_NTSC` | 227 | 227 |
//! | Colour clock | `CHIPSET_CLOCK_PAL` / `CHIPSET_CLOCK_NTSC` | 3 546 895 Hz | 3 579 545 Hz |
//! | Active buffer height | `AMIGA_HEIGHT_MAX_PAL` / `AMIGA_HEIGHT_MAX_NTSC` | 576/2 = 288 | 486/2 = 243 |
//! | First line after vertical blank | `VBLANK_ENDLINE_PAL` / `VBLANK_ENDLINE_NTSC` | 26 | 21 |
//! | `BEAMCON0` reset value | `BEAMCON0_PAL` | `0x0020` | `0x0000` |
//! | `VPOSR` standard bit | `csbit` in `VPOSR()` | clear | `0x1000` |
//!
//! The line length is identical in both standards. Only the line count and the
//! clock differ, which is why a frame is shorter in NTSC in both senses: fewer
//! lines and less time.

use crate::custom;
use crate::events;
use crate::playfield;

/// Video standard: the beam geometry and colour clock the chipset runs at.
///
/// This is a property of the machine rather than of the guest software. Guest
/// code observes it through `VPOSR` and `BEAMCON0` and adapts its display
/// window, which is why both registers must report the selected standard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VideoStandard {
    /// 312 lines at 3 546 895 Hz, 50.08 frames per second.
    #[default]
    Pal,
    /// 262 lines at 3 579 545 Hz, 60.19 frames per second.
    Ntsc,
}

/// Active buffer height for NTSC in non-interlaced lines.
///
/// Matches `WinUAE`'s `AMIGA_HEIGHT_MAX_NTSC` (`486 / 2`), the counterpart of the
/// PAL height that [`playfield::DISPLAY_HEIGHT`] already follows.
pub const NTSC_ACTIVE_HEIGHT: u32 = 243;

/// The PAL frame is the taller of the two, so a PAL-sized framebuffer holds an
/// NTSC frame as well and the buffer never needs resizing at runtime.
///
/// This assertion is the reason [`crate::emulator`] can keep a constant
/// framebuffer size. If a future standard breaks it, the build fails here rather
/// than writing past the end of a line.
const _: () = assert!(NTSC_ACTIVE_HEIGHT <= playfield::DISPLAY_HEIGHT);

/// PAL active height as a beam-comparable `u16`.
///
/// The width check happens here, once, at compile time. The accessors below are
/// then plain lookups with no failure path to document.
#[allow(clippy::cast_possible_truncation)]
const PAL_ACTIVE_HEIGHT_LINES: u16 = {
    assert!(playfield::DISPLAY_HEIGHT <= u16::MAX as u32);
    playfield::DISPLAY_HEIGHT as u16
};

/// NTSC active height as a beam-comparable `u16`.
#[allow(clippy::cast_possible_truncation)]
const NTSC_ACTIVE_HEIGHT_LINES: u16 = {
    assert!(NTSC_ACTIVE_HEIGHT <= u16::MAX as u32);
    NTSC_ACTIVE_HEIGHT as u16
};

/// Index of the last PAL line, which is where the beam wraps.
#[allow(clippy::cast_possible_truncation)]
const PAL_LAST_LINE: u16 = {
    assert!(events::SCANLINES_PAL >= 1 && events::SCANLINES_PAL <= u16::MAX as u64);
    events::SCANLINES_PAL as u16 - 1
};

/// Index of the last NTSC line.
#[allow(clippy::cast_possible_truncation)]
const NTSC_LAST_LINE: u16 = {
    assert!(events::SCANLINES_NTSC >= 1 && events::SCANLINES_NTSC <= u16::MAX as u64);
    events::SCANLINES_NTSC as u16 - 1
};

impl VideoStandard {
    /// Total scanlines per frame, including vertical blanking.
    #[must_use]
    pub const fn scanlines(self) -> u64 {
        match self {
            Self::Pal => events::SCANLINES_PAL,
            Self::Ntsc => events::SCANLINES_NTSC,
        }
    }

    /// Colour clock frequency in hertz.
    #[must_use]
    pub const fn colour_clock_hz(self) -> u64 {
        match self {
            Self::Pal => events::COLOUR_CLOCK_PAL_HZ,
            Self::Ntsc => events::COLOUR_CLOCK_NTSC_HZ,
        }
    }

    /// Active picture height in non-interlaced lines.
    ///
    /// This bounds the framebuffer lines the renderer fills. It is the maximum
    /// the standard can show, not what a given guest screen uses: an NTSC
    /// Workbench is typically 200 lines inside these 243.
    #[must_use]
    pub const fn active_height(self) -> u16 {
        match self {
            Self::Pal => PAL_ACTIVE_HEIGHT_LINES,
            Self::Ntsc => NTSC_ACTIVE_HEIGHT_LINES,
        }
    }

    /// Last line index of a frame, which is where the beam wraps.
    ///
    /// The beam wrap and the frame length must agree. If they disagree the beam
    /// drifts relative to the frame boundary, and guest code that waits for a
    /// specific line either waits a frame too long or never sees it.
    #[must_use]
    pub const fn last_line(self) -> u16 {
        match self {
            Self::Pal => PAL_LAST_LINE,
            Self::Ntsc => NTSC_LAST_LINE,
        }
    }

    /// First line after vertical blanking.
    ///
    /// Recorded for conformance rather than used by the renderer, which derives
    /// the vertical window from `DIWSTRT` as the hardware does.
    #[must_use]
    pub const fn first_visible_line(self) -> u16 {
        match self {
            Self::Pal => 26,
            Self::Ntsc => 21,
        }
    }

    /// Value the chipset reports in `BEAMCON0`.
    ///
    /// The `PAL` bit is set for PAL and clear for NTSC, so guest code reading
    /// `BEAMCON0` sees the standard the machine actually runs.
    #[must_use]
    pub const fn beamcon0(self) -> u16 {
        match self {
            Self::Pal => custom::BEAMCON0_PAL,
            Self::Ntsc => 0x0000,
        }
    }

    /// Standard bit the chipset reports in the `VPOSR` identification field.
    ///
    /// NTSC sets bit 12 on top of whatever the Agnus revision contributes. This
    /// is how `graphics.library` learns the standard without probing the beam.
    #[must_use]
    pub const fn vposr_standard_bits(self) -> u16 {
        match self {
            Self::Pal => 0x0000,
            Self::Ntsc => 0x1000,
        }
    }

    /// Duration of one emulated frame in nanoseconds.
    ///
    /// Derived from the line count and the colour clock rather than from a
    /// rounded frame rate, for the reasons ADR-0011 records.
    #[must_use]
    pub const fn frame_period_nanos(self) -> u64 {
        let cycles = events::CYCLES_PER_SCANLINE * self.scanlines();
        cycles * 1_000_000_000 / self.colour_clock_hz()
    }

    /// Short lowercase name for manifests and diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pal => "pal",
            Self::Ntsc => "ntsc",
        }
    }

    /// Stable discriminant for state digests.
    ///
    /// Digesting the name would make the digest depend on display text; this
    /// keeps the two independent.
    #[must_use]
    pub const fn digest_tag(self) -> u16 {
        match self {
            Self::Pal => 0,
            Self::Ntsc => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NTSC_ACTIVE_HEIGHT, VideoStandard};
    use crate::custom;
    use crate::events;
    use crate::playfield;

    #[test]
    fn line_count_matches_documented_constants() {
        assert_eq!(VideoStandard::Pal.scanlines(), 312);
        assert_eq!(VideoStandard::Ntsc.scanlines(), 262);
        assert_eq!(VideoStandard::Pal.scanlines(), events::SCANLINES_PAL);
        assert_eq!(VideoStandard::Ntsc.scanlines(), events::SCANLINES_NTSC);
    }

    #[test]
    fn colour_clock_matches_documented_constants() {
        assert_eq!(VideoStandard::Pal.colour_clock_hz(), 3_546_895);
        assert_eq!(VideoStandard::Ntsc.colour_clock_hz(), 3_579_545);
    }

    #[test]
    fn line_length_is_the_same_in_both_standards() {
        // MAXHPOS_PAL == MAXHPOS_NTSC == 227: only the line count and the clock
        // differ, so a per-standard line length would be wrong.
        assert_eq!(events::CYCLES_PER_SCANLINE, 227);
    }

    #[test]
    fn last_line_agrees_with_the_frame_length() {
        assert_eq!(VideoStandard::Pal.last_line(), 311);
        assert_eq!(VideoStandard::Ntsc.last_line(), 261);
        for standard in [VideoStandard::Pal, VideoStandard::Ntsc] {
            assert_eq!(
                u64::from(standard.last_line()) + 1,
                standard.scanlines(),
                "the beam must wrap exactly at the end of a frame"
            );
        }
    }

    #[test]
    fn active_height_matches_documented_constants() {
        assert_eq!(VideoStandard::Pal.active_height(), 288);
        assert_eq!(VideoStandard::Ntsc.active_height(), 243);
        assert_eq!(NTSC_ACTIVE_HEIGHT, 243);
    }

    #[test]
    fn pal_is_the_taller_standard_so_its_buffer_holds_both() {
        assert!(VideoStandard::Ntsc.active_height() <= VideoStandard::Pal.active_height());
        assert_eq!(
            u32::from(VideoStandard::Pal.active_height()),
            playfield::DISPLAY_HEIGHT
        );
    }

    #[test]
    fn first_visible_line_matches_documented_constants() {
        assert_eq!(VideoStandard::Pal.first_visible_line(), 26);
        assert_eq!(VideoStandard::Ntsc.first_visible_line(), 21);
    }

    #[test]
    fn beamcon0_reports_the_standard() {
        assert_eq!(VideoStandard::Pal.beamcon0(), custom::BEAMCON0_PAL);
        assert_eq!(VideoStandard::Pal.beamcon0(), 0x0020);
        assert_eq!(VideoStandard::Ntsc.beamcon0(), 0x0000);
        assert_eq!(
            VideoStandard::Ntsc.beamcon0() & custom::BEAMCON0_PAL,
            0,
            "the PAL bit must be clear under NTSC"
        );
    }

    #[test]
    fn vposr_reports_the_standard_in_bit_12() {
        assert_eq!(VideoStandard::Pal.vposr_standard_bits(), 0x0000);
        assert_eq!(VideoStandard::Ntsc.vposr_standard_bits(), 0x1000);
    }

    #[test]
    fn frame_period_follows_the_colour_clock_in_both_standards() {
        // 227 * 312 cycles / 3_546_895 Hz, implying 50.0804 Hz
        assert_eq!(VideoStandard::Pal.frame_period_nanos(), 19_967_887);
        // 227 * 262 cycles / 3_579_545 Hz, implying 60.1867 Hz
        assert_eq!(VideoStandard::Ntsc.frame_period_nanos(), 16_614_960);
    }

    #[test]
    fn frame_period_is_not_the_rounded_rate() {
        // 20 ms would be 32 microseconds long per PAL frame.
        assert_ne!(VideoStandard::Pal.frame_period_nanos(), 20_000_000);
        // The Amiga's NTSC frame is 60.19 Hz, neither the 60 Hz nor the 59.94 Hz
        // that broadcast NTSC is usually quoted at.
        assert_ne!(VideoStandard::Ntsc.frame_period_nanos(), 16_666_666);
        assert_ne!(VideoStandard::Ntsc.frame_period_nanos(), 16_683_350);
    }

    #[test]
    fn ntsc_frames_are_shorter_in_both_lines_and_time() {
        assert!(VideoStandard::Ntsc.scanlines() < VideoStandard::Pal.scanlines());
        assert!(
            VideoStandard::Ntsc.frame_period_nanos() < VideoStandard::Pal.frame_period_nanos(),
            "a shorter frame on a faster clock must take less time"
        );
    }

    #[test]
    fn pal_is_the_default_standard() {
        assert_eq!(VideoStandard::default(), VideoStandard::Pal);
    }

    #[test]
    fn digest_tags_are_distinct() {
        assert_ne!(
            VideoStandard::Pal.digest_tag(),
            VideoStandard::Ntsc.digest_tag()
        );
    }

    #[test]
    fn names_are_stable() {
        assert_eq!(VideoStandard::Pal.as_str(), "pal");
        assert_eq!(VideoStandard::Ntsc.as_str(), "ntsc");
    }
}
