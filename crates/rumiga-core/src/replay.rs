// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Deterministic input recording and replay.
//!
//! # Why frames rather than host time
//!
//! Every event is stamped with the emulated frame it belongs to. Host time is not
//! available here and would not help: a recording stamped with wall-clock instants
//! would replay differently on a faster or busier machine, which is exactly the
//! property replay exists to remove. ADR-0011 keeps host time out of the core; this
//! module is why that matters beyond tidiness.
//!
//! # Why the core records rather than the shell
//!
//! The three input entry points on [`crate::emulator::Emulator`] are the only way
//! input reaches the machine. Recording inside them makes a recording complete by
//! construction. A shell that recorded at its own call sites would be correct only
//! as long as every shell remembered to do it, and a second shell would silently
//! produce short recordings.
//!
//! # Format
//!
//! One event per line, preceded by a version header. The format is text so a
//! recording can be read, diffed, and hand-edited in a review, and so a difference
//! between two recordings is legible rather than a hex dump.
//!
//! ```text
//! rumiga.input-recording.v1
//! 0 key 40 down
//! 3 key 40 up
//! 7 mouse -4 12
//! 9 buttons 1 0
//! ```
//!
//! Frames must not decrease from line to line. Ordering carries meaning, so a
//! recording that jumps backwards is rejected rather than sorted: silently sorting
//! would hide a corrupted or hand-merged file.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::fmt::Write as _;

use crate::digest::StateDigest;

/// Header identifying the recording format.
pub const RECORDING_HEADER: &str = "rumiga.input-recording.v1";

/// One input action delivered to the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEvent {
    /// A key press or release, using Amiga raw keycodes.
    Key {
        /// Amiga raw keycode.
        keycode: u8,
        /// `true` for press, `false` for release.
        pressed: bool,
    },
    /// Relative pointer motion.
    MouseMove {
        /// Horizontal delta.
        dx: i16,
        /// Vertical delta.
        dy: i16,
    },
    /// Pointer button state.
    ///
    /// This is state rather than an event, so a recorder stores it only when it
    /// changes. Storing it every frame would grow the recording without adding
    /// information.
    MouseButtons {
        /// Left button held.
        left: bool,
        /// Right button held.
        right: bool,
    },
}

/// One event and the emulated frame it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordedInput {
    /// Emulated frame index, counted by the machine rather than by a shell.
    pub frame: u64,
    /// The action itself.
    pub event: InputEvent,
}

/// Why a recording could not be parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayError {
    /// The first line is not [`RECORDING_HEADER`].
    MissingHeader,
    /// A line does not have the shape its event kind requires.
    MalformedLine {
        /// One-based line number, counting the header.
        line: u32,
    },
    /// A line names an event kind this version does not define.
    UnknownEventKind {
        /// One-based line number, counting the header.
        line: u32,
    },
    /// A frame index is lower than the one before it.
    ///
    /// Ordering carries meaning, so this is rejected rather than sorted.
    FramesOutOfOrder {
        /// One-based line number, counting the header.
        line: u32,
    },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHeader => write!(f, "expected the header {RECORDING_HEADER}"),
            Self::MalformedLine { line } => write!(f, "malformed event on line {line}"),
            Self::UnknownEventKind { line } => write!(f, "unknown event kind on line {line}"),
            Self::FramesOutOfOrder { line } => {
                write!(f, "frame index decreases on line {line}")
            }
        }
    }
}

impl core::error::Error for ReplayError {}

/// An ordered sequence of input events with their frames.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputRecording {
    events: Vec<RecordedInput>,
}

impl InputRecording {
    /// Build a recording from events that are already in non-decreasing frame order.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayError::FramesOutOfOrder`] if a frame index decreases.
    pub fn from_events(events: Vec<RecordedInput>) -> Result<Self, ReplayError> {
        let mut previous = 0_u64;
        for (index, recorded) in events.iter().enumerate() {
            if recorded.frame < previous {
                return Err(ReplayError::FramesOutOfOrder {
                    line: u32::try_from(index + 2).unwrap_or(u32::MAX),
                });
            }
            previous = recorded.frame;
        }
        Ok(Self { events })
    }

    /// Events in recording order.
    #[must_use]
    pub fn events(&self) -> &[RecordedInput] {
        &self.events
    }

    /// Number of recorded events.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the recording holds no events.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Highest frame index the recording mentions, if any.
    #[must_use]
    pub fn last_frame(&self) -> Option<u64> {
        self.events.last().map(|recorded| recorded.frame)
    }

    /// Digest identifying this recording.
    ///
    /// Not cryptographic; see [`crate::digest`]. It exists so recorded evidence can
    /// name which recording produced a result without embedding the whole file.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut digest = StateDigest::new();
        for recorded in &self.events {
            digest.write_u64(recorded.frame);
            match recorded.event {
                InputEvent::Key { keycode, pressed } => {
                    digest.write_u16(0);
                    digest.write_u16(u16::from(keycode));
                    digest.write_u16(u16::from(pressed));
                }
                InputEvent::MouseMove { dx, dy } => {
                    digest.write_u16(1);
                    digest.write_bytes(&dx.to_be_bytes());
                    digest.write_bytes(&dy.to_be_bytes());
                }
                InputEvent::MouseButtons { left, right } => {
                    digest.write_u16(2);
                    digest.write_u16(u16::from(left));
                    digest.write_u16(u16::from(right));
                }
            }
        }
        digest.finish()
    }

    /// Render the recording in the text format.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "{RECORDING_HEADER}");
        for recorded in &self.events {
            match recorded.event {
                InputEvent::Key { keycode, pressed } => {
                    let _ = writeln!(
                        text,
                        "{} key {keycode:02x} {}",
                        recorded.frame,
                        if pressed { "down" } else { "up" }
                    );
                }
                InputEvent::MouseMove { dx, dy } => {
                    let _ = writeln!(text, "{} mouse {dx} {dy}", recorded.frame);
                }
                InputEvent::MouseButtons { left, right } => {
                    let _ = writeln!(
                        text,
                        "{} buttons {} {}",
                        recorded.frame,
                        u8::from(left),
                        u8::from(right)
                    );
                }
            }
        }
        text
    }

    /// Parse the text format.
    ///
    /// Blank lines and lines starting with `#` are ignored, so a recording can carry
    /// comments explaining what a scenario does.
    ///
    /// # Errors
    ///
    /// Returns a [`ReplayError`] naming the offending line.
    pub fn parse(text: &str) -> Result<Self, ReplayError> {
        let mut lines = text.lines().enumerate();
        let header = lines
            .find(|(_, line)| !line.trim().is_empty())
            .ok_or(ReplayError::MissingHeader)?;
        if header.1.trim() != RECORDING_HEADER {
            return Err(ReplayError::MissingHeader);
        }

        let mut events = Vec::new();
        let mut previous = 0_u64;
        for (index, raw) in lines {
            let line_number = u32::try_from(index + 1).unwrap_or(u32::MAX);
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let frame = fields
                .next()
                .and_then(|field| field.parse::<u64>().ok())
                .ok_or(ReplayError::MalformedLine { line: line_number })?;
            if frame < previous {
                return Err(ReplayError::FramesOutOfOrder { line: line_number });
            }
            previous = frame;

            let kind = fields
                .next()
                .ok_or(ReplayError::MalformedLine { line: line_number })?;
            let event = match kind {
                "key" => {
                    let keycode = fields
                        .next()
                        .and_then(|field| u8::from_str_radix(field, 16).ok())
                        .ok_or(ReplayError::MalformedLine { line: line_number })?;
                    let pressed = match fields.next() {
                        Some("down") => true,
                        Some("up") => false,
                        _ => return Err(ReplayError::MalformedLine { line: line_number }),
                    };
                    InputEvent::Key { keycode, pressed }
                }
                "mouse" => {
                    let dx = parse_i16(fields.next(), line_number)?;
                    let dy = parse_i16(fields.next(), line_number)?;
                    InputEvent::MouseMove { dx, dy }
                }
                "buttons" => {
                    let left = parse_flag(fields.next(), line_number)?;
                    let right = parse_flag(fields.next(), line_number)?;
                    InputEvent::MouseButtons { left, right }
                }
                _ => return Err(ReplayError::UnknownEventKind { line: line_number }),
            };
            if fields.next().is_some() {
                return Err(ReplayError::MalformedLine { line: line_number });
            }
            events.push(RecordedInput { frame, event });
        }

        Ok(Self { events })
    }
}

fn parse_i16(field: Option<&str>, line: u32) -> Result<i16, ReplayError> {
    field
        .and_then(|value| value.parse::<i16>().ok())
        .ok_or(ReplayError::MalformedLine { line })
}

fn parse_flag(field: Option<&str>, line: u32) -> Result<bool, ReplayError> {
    match field {
        Some("0") => Ok(false),
        Some("1") => Ok(true),
        _ => Err(ReplayError::MalformedLine { line }),
    }
}

/// Collects input as it is delivered to the machine.
///
/// Button state is stored only when it changes, because it is state rather than an
/// event and is delivered every frame by at least one shell.
#[derive(Clone, Debug, Default)]
pub struct InputRecorder {
    events: Vec<RecordedInput>,
    last_buttons: Option<(bool, bool)>,
}

impl InputRecorder {
    /// Create an empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a key event for `frame`.
    pub fn key(&mut self, frame: u64, keycode: u8, pressed: bool) {
        self.events.push(RecordedInput {
            frame,
            event: InputEvent::Key { keycode, pressed },
        });
    }

    /// Record pointer motion for `frame`, ignoring a zero delta.
    pub fn mouse_move(&mut self, frame: u64, dx: i16, dy: i16) {
        if dx == 0 && dy == 0 {
            return;
        }
        self.events.push(RecordedInput {
            frame,
            event: InputEvent::MouseMove { dx, dy },
        });
    }

    /// Record button state for `frame` only if it differs from the last recorded state.
    pub fn mouse_buttons(&mut self, frame: u64, left: bool, right: bool) {
        if self.last_buttons == Some((left, right)) {
            return;
        }
        self.last_buttons = Some((left, right));
        self.events.push(RecordedInput {
            frame,
            event: InputEvent::MouseButtons { left, right },
        });
    }

    /// Events recorded so far.
    #[must_use]
    pub fn events(&self) -> &[RecordedInput] {
        &self.events
    }

    /// Finish recording.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayError::FramesOutOfOrder`] if events were recorded with a
    /// decreasing frame index, which would mean the caller drove frames backwards.
    pub fn finish(self) -> Result<InputRecording, ReplayError> {
        InputRecording::from_events(self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InputEvent, InputRecorder, InputRecording, RECORDING_HEADER, RecordedInput, ReplayError,
    };
    use alloc::vec;

    fn fixture() -> InputRecording {
        InputRecording::from_events(vec![
            RecordedInput {
                frame: 0,
                event: InputEvent::Key {
                    keycode: 0x40,
                    pressed: true,
                },
            },
            RecordedInput {
                frame: 3,
                event: InputEvent::Key {
                    keycode: 0x40,
                    pressed: false,
                },
            },
            RecordedInput {
                frame: 7,
                event: InputEvent::MouseMove { dx: -4, dy: 12 },
            },
            RecordedInput {
                frame: 9,
                event: InputEvent::MouseButtons {
                    left: true,
                    right: false,
                },
            },
        ])
        .expect("fixture is ordered")
    }

    #[test]
    fn text_round_trips() {
        let recording = fixture();

        let parsed = InputRecording::parse(&recording.to_text()).expect("round trip parses");

        assert_eq!(parsed, recording);
        assert_eq!(parsed.digest(), recording.digest());
    }

    #[test]
    fn the_text_form_is_readable() {
        let text = fixture().to_text();

        assert!(text.starts_with(RECORDING_HEADER));
        assert!(text.contains("0 key 40 down"));
        assert!(text.contains("3 key 40 up"));
        assert!(text.contains("7 mouse -4 12"));
        assert!(text.contains("9 buttons 1 0"));
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let text = "rumiga.input-recording.v1\n\n# open the Workbench menu\n2 key 40 down\n";

        let parsed = InputRecording::parse(text).expect("comments are allowed");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed.events()[0].frame, 2);
    }

    #[test]
    fn a_missing_header_is_rejected() {
        assert_eq!(
            InputRecording::parse("0 key 40 down\n"),
            Err(ReplayError::MissingHeader)
        );
        assert_eq!(InputRecording::parse(""), Err(ReplayError::MissingHeader));
    }

    #[test]
    fn out_of_order_frames_are_rejected_rather_than_sorted() {
        let text = "rumiga.input-recording.v1\n5 key 40 down\n2 key 40 up\n";

        // Sorting here would hide a corrupted or hand-merged recording.
        assert_eq!(
            InputRecording::parse(text),
            Err(ReplayError::FramesOutOfOrder { line: 3 })
        );
    }

    #[test]
    fn malformed_lines_name_the_line() {
        for (text, expected) in [
            (
                "rumiga.input-recording.v1\n0 key\n",
                ReplayError::MalformedLine { line: 2 },
            ),
            (
                "rumiga.input-recording.v1\n0 key 40 sideways\n",
                ReplayError::MalformedLine { line: 2 },
            ),
            (
                "rumiga.input-recording.v1\n0 mouse 1\n",
                ReplayError::MalformedLine { line: 2 },
            ),
            (
                "rumiga.input-recording.v1\n0 buttons 2 0\n",
                ReplayError::MalformedLine { line: 2 },
            ),
            (
                "rumiga.input-recording.v1\n0 key 40 down extra\n",
                ReplayError::MalformedLine { line: 2 },
            ),
            (
                "rumiga.input-recording.v1\n0 wiggle 1 2\n",
                ReplayError::UnknownEventKind { line: 2 },
            ),
        ] {
            assert_eq!(InputRecording::parse(text), Err(expected), "{text:?}");
        }
    }

    #[test]
    fn the_digest_separates_different_recordings() {
        let baseline = fixture();
        let one_frame_later = InputRecording::from_events(vec![RecordedInput {
            frame: 1,
            event: InputEvent::Key {
                keycode: 0x40,
                pressed: true,
            },
        }])
        .unwrap();
        let different_key = InputRecording::from_events(vec![RecordedInput {
            frame: 1,
            event: InputEvent::Key {
                keycode: 0x41,
                pressed: true,
            },
        }])
        .unwrap();

        assert_ne!(baseline.digest(), one_frame_later.digest());
        assert_ne!(one_frame_later.digest(), different_key.digest());
    }

    #[test]
    fn the_digest_distinguishes_event_kinds_with_equal_payloads() {
        let buttons = InputRecording::from_events(vec![RecordedInput {
            frame: 0,
            event: InputEvent::MouseButtons {
                left: true,
                right: false,
            },
        }])
        .unwrap();
        let key = InputRecording::from_events(vec![RecordedInput {
            frame: 0,
            event: InputEvent::Key {
                keycode: 1,
                pressed: false,
            },
        }])
        .unwrap();

        // Without a kind tag in the digest these would collide.
        assert_ne!(buttons.digest(), key.digest());
    }

    #[test]
    fn the_recorder_skips_unchanged_button_state() {
        let mut recorder = InputRecorder::new();

        for frame in 0..10 {
            recorder.mouse_buttons(frame, false, false);
        }
        recorder.mouse_buttons(10, true, false);
        recorder.mouse_buttons(11, true, false);

        // A shell delivers button state every frame; recording each delivery would
        // grow the file without adding information.
        let recording = recorder.finish().expect("frames only advance");
        assert_eq!(recording.len(), 2);
        assert_eq!(
            recording.events()[0].event,
            InputEvent::MouseButtons {
                left: false,
                right: false
            }
        );
        assert_eq!(recording.events()[1].frame, 10);
    }

    #[test]
    fn the_recorder_skips_zero_mouse_deltas() {
        let mut recorder = InputRecorder::new();

        recorder.mouse_move(0, 0, 0);
        recorder.mouse_move(1, 0, 3);

        let recording = recorder.finish().unwrap();
        assert_eq!(recording.len(), 1);
        assert_eq!(recording.events()[0].frame, 1);
    }

    #[test]
    fn the_recorder_rejects_frames_that_go_backwards() {
        let mut recorder = InputRecorder::new();
        recorder.key(5, 0x40, true);
        recorder.key(2, 0x40, false);

        assert_eq!(
            recorder.finish(),
            Err(ReplayError::FramesOutOfOrder { line: 3 })
        );
    }

    #[test]
    fn last_frame_reports_the_end_of_the_recording() {
        assert_eq!(fixture().last_frame(), Some(9));
        assert_eq!(InputRecording::default().last_frame(), None);
        assert!(InputRecording::default().is_empty());
    }

    #[test]
    fn replay_error_uses_the_portable_error_contract() {
        fn assert_core_error<T: core::error::Error>() {}

        assert_core_error::<ReplayError>();
    }
}
