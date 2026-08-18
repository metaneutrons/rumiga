// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Platform abstraction traits for the Rumiga Amiga emulator.
//!
//! Defines the interfaces that platform backends must implement for video
//! output, audio output, input handling, storage access, and diagnostic
//! record transport.
//!
//! All traits are `no_std`-compatible and use only `core` and `alloc` types.
//!
//! # Failure and flow control are separate
//!
//! A backend answers two different questions and this crate keeps them apart.
//!
//! [`PlatformError`] reports that an operation could not be carried out. Its
//! [`PlatformError::Unsupported`] variant is the explicit answer for a service the
//! backend does not implement at all, so a caller never has to infer absence from
//! a silent no-op.
//!
//! Flow control is not failure. A display that was not ready or an audio queue
//! that is full is working as designed, so backpressure is reported on the success
//! path: [`FramePresentation`] says whether a frame reached the display, and
//! [`SamplesQueued`] says how many samples were taken. Folding either into an error
//! type would make normal operation indistinguishable from a fault.
//!
//! # Unsupported is representable twice, on purpose
//!
//! [`PlatformCapabilities`] describes what a backend offers before anything is
//! called, using `Option` so an absent service is structurally absent rather than
//! signalled by a magic value. A caller that ignores the descriptor and calls
//! anyway still receives [`PlatformError::Unsupported`]. The descriptor is the
//! polite path; the error is the backstop.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::time::Duration;

/// Version of the platform contract set defined by this crate.
///
/// A backend records the version it was built against in
/// [`PlatformCapabilities::contract_version`], and a shell rejects a mismatch
/// through [`PlatformCapabilities::validate`] rather than discovering the
/// disagreement one method at a time.
///
/// Increment this when an existing contract changes shape. Adding a new contract
/// that existing backends can leave absent does not require an increment.
pub const CONTRACT_VERSION: u32 = 1;

/// Typed failure of a platform operation.
///
/// This reports that something could not be done. Backpressure is not here: a full
/// queue or a display that was not ready is normal operation and is reported by
/// [`FramePresentation`] and [`SamplesQueued`] on the success path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformError {
    /// The backend does not implement this service.
    ///
    /// A caller should have seen the absence in [`PlatformCapabilities`] first;
    /// this is the answer for one that did not look.
    Unsupported,
    /// The arguments were inconsistent, such as stereo channels of unequal length.
    InvalidArgument,
    /// The named file or path does not exist.
    NotFound,
    /// No storage medium is present.
    NoMedia,
    /// The medium is full.
    Full,
    /// The medium is present but cannot be written.
    ReadOnly,
    /// The underlying transport failed.
    Io,
    /// The backend was built against a different contract version.
    ContractVersionMismatch {
        /// Version this crate defines.
        expected: u32,
        /// Version the backend reported.
        found: u32,
    },
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("the platform backend does not support this service"),
            Self::InvalidArgument => f.write_str("the arguments were inconsistent"),
            Self::NotFound => f.write_str("the file or path does not exist"),
            Self::NoMedia => f.write_str("no storage medium is present"),
            Self::Full => f.write_str("the storage medium is full"),
            Self::ReadOnly => f.write_str("the storage medium is read-only"),
            Self::Io => f.write_str("the underlying transport failed"),
            Self::ContractVersionMismatch { expected, found } => write!(
                f,
                "platform contract version mismatch: this build defines {expected}, the backend reported {found}"
            ),
        }
    }
}

impl core::error::Error for PlatformError {}

/// Framebuffer pixel layout a video backend consumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// 16 bits per pixel, 5 red, 6 green, 5 blue, host byte order.
    Rgb565,
}

/// What a video backend accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoCapabilities {
    /// Largest frame width in pixels.
    pub max_width: u32,
    /// Largest frame height in pixels.
    pub max_height: u32,
    /// Pixel layout the backend consumes.
    pub pixel_format: PixelFormat,
    /// Whether the backend can report [`FramePresentation::DroppedForBackpressure`].
    ///
    /// A backend that always presents or fails states `false` here, so a shell knows
    /// a dropped-frame counter would stay at zero rather than assuming it is healthy.
    pub reports_backpressure: bool,
}

/// What an audio backend accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioCapabilities {
    /// Playback sample rate in hertz.
    pub sample_rate_hz: u32,
    /// Number of interleaved output channels.
    pub channels: u8,
    /// Largest number of frames the backend will hold before applying backpressure.
    pub max_queued_frames: usize,
}

/// Which input sources a backend can report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputCapabilities {
    /// Keyboard events are reported.
    pub keyboard: bool,
    /// Pointer motion and buttons are reported.
    pub mouse: bool,
    /// Number of joystick ports reported.
    pub joysticks: u8,
}

/// What a storage backend allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageCapabilities {
    /// Whether writes and deletes are permitted.
    pub writable: bool,
    /// Whether the medium can be reformatted.
    pub formattable: bool,
}

/// What a platform backend offers, published before anything is called.
///
/// An absent service is `None` rather than a zeroed structure, so a caller cannot
/// mistake "no audio" for "audio at 0 Hz".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformCapabilities {
    /// Contract version the backend was built against.
    pub contract_version: u32,
    /// Video service, absent if the backend presents no frames.
    pub video: Option<VideoCapabilities>,
    /// Audio service, absent if the backend plays no sound.
    pub audio: Option<AudioCapabilities>,
    /// Input sources, all `false` if the backend reports no input.
    pub input: InputCapabilities,
    /// Storage service, absent if the backend exposes no filesystem.
    pub storage: Option<StorageCapabilities>,
}

impl PlatformCapabilities {
    /// Reject a backend built against a different contract version.
    ///
    /// A shell calls this once at startup. Without it a version disagreement would
    /// surface as a confusing failure inside an unrelated call, or not at all.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::ContractVersionMismatch`] when the reported version
    /// is not [`CONTRACT_VERSION`].
    pub const fn validate(&self) -> Result<(), PlatformError> {
        if self.contract_version == CONTRACT_VERSION {
            Ok(())
        } else {
            Err(PlatformError::ContractVersionMismatch {
                expected: CONTRACT_VERSION,
                found: self.contract_version,
            })
        }
    }
}

/// Publishes what a backend offers.
///
/// This is a contract rather than a plain structure so a backend cannot ship
/// without answering the question.
pub trait CapabilityReport {
    /// Describe this backend.
    fn capabilities(&self) -> PlatformCapabilities;
}

/// Outcome of presenting one frame.
///
/// Dropping a frame under backpressure is not a failure, so it is reported here
/// rather than as a [`PlatformError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePresentation {
    /// The frame reached the display.
    Presented,
    /// The display was not ready, so the frame was dropped rather than queued.
    ///
    /// A shell should keep emulating. Queueing instead would trade latency for a
    /// backlog that can never be caught up.
    DroppedForBackpressure,
}

impl FramePresentation {
    /// Whether the frame reached the display.
    #[must_use]
    pub const fn presented(self) -> bool {
        matches!(self, Self::Presented)
    }
}

/// Outcome of queueing audio samples.
///
/// A backend takes what fits and reports the rest as backpressure. Partial
/// acceptance is normal, so the caller is told the split rather than having to
/// infer it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SamplesQueued {
    /// Frames the backend took.
    pub accepted: usize,
    /// Frames the backend could not take because its queue is full.
    pub rejected: usize,
}

impl SamplesQueued {
    /// Every frame was taken.
    #[must_use]
    pub const fn all(accepted: usize) -> Self {
        Self {
            accepted,
            rejected: 0,
        }
    }

    /// Whether the backend applied backpressure.
    #[must_use]
    pub const fn backpressured(self) -> bool {
        self.rejected > 0
    }
}

/// Monotonic clock and pacing trait — owns host time on behalf of the shell.
///
/// The emulator core never reads host time; it advances in emulated cycles. This
/// contract exists so a product shell can decide when to run the next frame and
/// measure what the host actually did, without the core learning about wall time.
///
/// `now` must be monotonic: it may stall but must never go backwards. Its epoch is
/// unspecified, so only differences are meaningful.
pub trait Clock {
    /// Monotonic time since an unspecified epoch.
    fn now(&self) -> Duration;

    /// Wait for at most `requested`, returning the time actually spent.
    ///
    /// The return value is the measurement, not the request. Hosts routinely
    /// oversleep, so a caller that needs to pace must use what it is told rather
    /// than what it asked for.
    fn pace(&mut self, requested: Duration) -> Duration;
}

/// Trace sink trait — transports diagnostic records emitted by the core.
///
/// The core formats a complete record and hands it to the sink. Record layout
/// is therefore deterministic and independent of the transport, while file,
/// serial, and in-memory transports stay in platform adapters. The core never
/// opens a file, holds a path, or owns a buffered writer.
///
/// Records carry no line terminator. Each implementation appends the
/// terminator its transport requires; a host file sink appends `\n`.
///
/// Both methods are infallible. Diagnostics must not change emulated state or
/// abort emulation, so transport errors are absorbed by the implementation.
pub trait TraceSink {
    /// Write one complete record.
    fn write_record(&mut self, record: fmt::Arguments<'_>);

    /// Flush buffered records to the underlying transport.
    ///
    /// Callers must not rely on drop order for durability. Implementations
    /// that buffer should also flush on drop as a backstop.
    fn flush(&mut self);
}

/// Video output trait — presents rendered frames to the display.
pub trait VideoOutput {
    /// Present a completed frame to the display.
    ///
    /// `framebuffer` contains RGB565 pixel data, row-major, top-to-bottom, and must
    /// hold at least `width * height` pixels.
    ///
    /// Returning [`FramePresentation::DroppedForBackpressure`] is a success: the
    /// display was not ready and the frame was discarded rather than queued. The
    /// result is `#[must_use]` through `Result`, so a caller cannot ignore either
    /// outcome by accident, which the previous signature allowed.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::InvalidArgument`] if the buffer is too small for the
    /// stated dimensions or the dimensions exceed
    /// [`VideoCapabilities::max_width`] or [`VideoCapabilities::max_height`], and
    /// [`PlatformError::Io`] if the display transport failed.
    fn present_frame(
        &mut self,
        framebuffer: &[u16],
        width: u32,
        height: u32,
    ) -> Result<FramePresentation, PlatformError>;
}

/// Audio output trait — queues stereo audio samples for playback.
pub trait AudioOutput {
    /// Queue stereo audio samples for playback.
    ///
    /// Both slices must have the same length. Samples are signed 16-bit PCM.
    ///
    /// A backend takes what fits and reports the remainder as
    /// [`SamplesQueued::rejected`]. Partial acceptance is normal operation, not a
    /// failure, so the caller is told the split instead of having to compare a
    /// returned count against what it sent.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::InvalidArgument`] if the two slices differ in
    /// length, [`PlatformError::Unsupported`] if the backend plays no audio, and
    /// [`PlatformError::Io`] if the audio transport failed.
    fn queue_samples(
        &mut self,
        left: &[i16],
        right: &[i16],
    ) -> Result<SamplesQueued, PlatformError>;
}

/// Joystick button and direction state packed as a bitfield.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JoystickState {
    /// Packed bits: bit 0=up, 1=down, 2=left, 3=right, 4=fire.
    bits: u8,
}

impl JoystickState {
    const UP: u8 = 1 << 0;
    const DOWN: u8 = 1 << 1;
    const LEFT: u8 = 1 << 2;
    const RIGHT: u8 = 1 << 3;
    const FIRE: u8 = 1 << 4;

    /// Create a new joystick state from raw packed bits.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self { bits }
    }

    #[must_use]
    pub const fn with_up(self) -> Self {
        Self {
            bits: self.bits | Self::UP,
        }
    }
    #[must_use]
    pub const fn with_down(self) -> Self {
        Self {
            bits: self.bits | Self::DOWN,
        }
    }
    #[must_use]
    pub const fn with_left(self) -> Self {
        Self {
            bits: self.bits | Self::LEFT,
        }
    }
    #[must_use]
    pub const fn with_right(self) -> Self {
        Self {
            bits: self.bits | Self::RIGHT,
        }
    }
    #[must_use]
    pub const fn with_fire(self) -> Self {
        Self {
            bits: self.bits | Self::FIRE,
        }
    }

    #[must_use]
    pub const fn up(self) -> bool {
        self.bits & Self::UP != 0
    }
    #[must_use]
    pub const fn down(self) -> bool {
        self.bits & Self::DOWN != 0
    }
    #[must_use]
    pub const fn left(self) -> bool {
        self.bits & Self::LEFT != 0
    }
    #[must_use]
    pub const fn right(self) -> bool {
        self.bits & Self::RIGHT != 0
    }
    #[must_use]
    pub const fn fire(self) -> bool {
        self.bits & Self::FIRE != 0
    }
}

/// Mouse state as relative deltas and button presses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseState {
    pub dx: i16,
    pub dy: i16,
    /// Packed bits: bit 0=left, 1=right, 2=middle.
    buttons: u8,
}

impl MouseState {
    const LEFT_BTN: u8 = 1 << 0;
    const RIGHT_BTN: u8 = 1 << 1;
    const MIDDLE_BTN: u8 = 1 << 2;

    /// Create a new mouse state.
    #[must_use]
    pub const fn new(dx: i16, dy: i16, buttons: u8) -> Self {
        Self { dx, dy, buttons }
    }

    #[must_use]
    pub const fn left_button(self) -> bool {
        self.buttons & Self::LEFT_BTN != 0
    }
    #[must_use]
    pub const fn right_button(self) -> bool {
        self.buttons & Self::RIGHT_BTN != 0
    }
    #[must_use]
    pub const fn middle_button(self) -> bool {
        self.buttons & Self::MIDDLE_BTN != 0
    }
}

/// Keyboard key event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// Amiga raw keycode (0x00–0x7F).
    pub keycode: u8,
    /// `true` for key press, `false` for key release.
    pub pressed: bool,
}

/// Combined input state from all sources.
#[derive(Clone, Debug, Default)]
pub struct InputState {
    pub joystick: [JoystickState; 2],
    pub mouse: MouseState,
    pub key_events: Vec<KeyEvent>,
}

/// Input source trait — polls for joystick, mouse, and keyboard input.
pub trait InputSource {
    /// Poll for the current input state. Consumes pending key events.
    fn poll(&mut self) -> InputState;
}

/// File metadata for directory listings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    pub is_directory: bool,
}

/// Storage trait — provides access to disk images and file management.
///
/// All methods may return [`PlatformError`] on failure. This contract previously
/// had its own `StorageError` whose variants duplicated the platform error model;
/// one model means a shell handles one error type rather than translating between
/// two that mean the same things.
pub trait Storage {
    /// List files in the given directory path.
    ///
    /// # Errors
    /// Returns [`PlatformError::NotFound`] if the path doesn't exist,
    /// or [`PlatformError::Io`] on I/O failure.
    fn list_files(&mut self, path: &str) -> Result<Vec<FileInfo>, PlatformError>;

    /// Read bytes from a file at the given offset. Returns bytes read.
    ///
    /// # Errors
    /// Returns [`PlatformError::NotFound`] if the file doesn't exist,
    /// or [`PlatformError::Io`] on I/O failure.
    fn read_file(
        &mut self,
        path: &str,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<usize, PlatformError>;

    /// Write bytes to a file at the given offset.
    ///
    /// # Errors
    /// Returns [`PlatformError::Full`] if storage is exhausted,
    /// [`PlatformError::ReadOnly`] if the medium cannot be written, or
    /// [`PlatformError::Io`] on I/O failure.
    fn write_file(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<(), PlatformError>;

    /// Delete a file.
    ///
    /// # Errors
    /// Returns [`PlatformError::NotFound`] if the file doesn't exist, or
    /// [`PlatformError::ReadOnly`] if the medium cannot be written.
    fn delete_file(&mut self, path: &str) -> Result<(), PlatformError>;

    /// Get the total and free space in bytes as `(total, free)`.
    ///
    /// # Errors
    /// Returns [`PlatformError::NoMedia`] if no storage is present.
    fn space_info(&mut self) -> Result<(u64, u64), PlatformError>;

    /// Format the storage medium as FAT32.
    ///
    /// # Errors
    /// Returns [`PlatformError::Io`] on failure, or
    /// [`PlatformError::Unsupported`] if formatting is not available.
    fn format(&mut self) -> Result<(), PlatformError>;
}

#[cfg(test)]
mod tests {
    use super::{
        AudioCapabilities, AudioOutput, CONTRACT_VERSION, CapabilityReport, FramePresentation,
        InputCapabilities, PixelFormat, PlatformCapabilities, PlatformError, SamplesQueued,
        StorageCapabilities, VideoCapabilities, VideoOutput,
    };

    /// Backend with no audio and a display that can refuse a frame.
    ///
    /// This exists because the desktop backend can do neither: it has no audio at all
    /// and its display never refuses. Without a double, the two states this task is
    /// about would be documented rather than exercised.
    struct StubBackend {
        display_ready: bool,
        audio_room_frames: usize,
    }

    impl CapabilityReport for StubBackend {
        fn capabilities(&self) -> PlatformCapabilities {
            PlatformCapabilities {
                contract_version: CONTRACT_VERSION,
                video: Some(VideoCapabilities {
                    max_width: 754,
                    max_height: 288,
                    pixel_format: PixelFormat::Rgb565,
                    reports_backpressure: true,
                }),
                audio: None,
                input: InputCapabilities::default(),
                storage: None,
            }
        }
    }

    impl VideoOutput for StubBackend {
        fn present_frame(
            &mut self,
            framebuffer: &[u16],
            width: u32,
            height: u32,
        ) -> Result<FramePresentation, PlatformError> {
            let needed = width as usize * height as usize;
            if framebuffer.len() < needed {
                return Err(PlatformError::InvalidArgument);
            }
            if self.display_ready {
                Ok(FramePresentation::Presented)
            } else {
                Ok(FramePresentation::DroppedForBackpressure)
            }
        }
    }

    impl AudioOutput for StubBackend {
        fn queue_samples(
            &mut self,
            left: &[i16],
            right: &[i16],
        ) -> Result<SamplesQueued, PlatformError> {
            if left.len() != right.len() {
                return Err(PlatformError::InvalidArgument);
            }
            if self.capabilities().audio.is_none() {
                return Err(PlatformError::Unsupported);
            }
            let accepted = left.len().min(self.audio_room_frames);
            Ok(SamplesQueued {
                accepted,
                rejected: left.len() - accepted,
            })
        }
    }

    fn stub() -> StubBackend {
        StubBackend {
            display_ready: true,
            audio_room_frames: 0,
        }
    }

    #[test]
    fn an_absent_service_is_absent_in_the_descriptor() {
        let caps = stub().capabilities();

        // `None` rather than a zeroed AudioCapabilities: a caller cannot mistake
        // "no audio" for "audio at 0 Hz on 0 channels".
        assert!(caps.audio.is_none());
        assert!(caps.storage.is_none());
        assert!(caps.video.is_some());
    }

    #[test]
    fn calling_an_absent_service_returns_unsupported() {
        let mut backend = stub();

        assert_eq!(
            backend.queue_samples(&[0, 0], &[0, 0]),
            Err(PlatformError::Unsupported),
            "a caller that ignored the descriptor must still be told plainly"
        );
    }

    #[test]
    fn backpressure_is_a_success_not_an_error() {
        let mut backend = stub();
        backend.display_ready = false;
        let frame = [0u16; 16];

        let outcome = backend
            .present_frame(&frame, 4, 4)
            .expect("backpressure must not be reported as a failure");

        assert_eq!(outcome, FramePresentation::DroppedForBackpressure);
        assert!(!outcome.presented());
    }

    #[test]
    fn presented_and_dropped_are_distinguishable() {
        let mut backend = stub();
        let frame = [0u16; 16];

        backend.display_ready = true;
        let presented = backend.present_frame(&frame, 4, 4).unwrap();
        backend.display_ready = false;
        let dropped = backend.present_frame(&frame, 4, 4).unwrap();

        assert_ne!(presented, dropped);
        assert!(presented.presented());
        assert!(!dropped.presented());
    }

    #[test]
    fn a_short_buffer_is_an_error_not_a_dropped_frame() {
        let mut backend = stub();
        let frame = [0u16; 4];

        // A caller mistake must not look like flow control.
        assert_eq!(
            backend.present_frame(&frame, 4, 4),
            Err(PlatformError::InvalidArgument)
        );
    }

    #[test]
    fn partial_audio_acceptance_reports_the_split() {
        let queued = SamplesQueued {
            accepted: 64,
            rejected: 192,
        };

        assert!(queued.backpressured());
        assert_eq!(queued.accepted + queued.rejected, 256);

        let complete = SamplesQueued::all(256);
        assert!(!complete.backpressured());
        assert_eq!(complete.accepted, 256);
    }

    #[test]
    fn unequal_stereo_slices_are_an_error() {
        let mut backend = stub();

        assert_eq!(
            backend.queue_samples(&[0, 0], &[0]),
            Err(PlatformError::InvalidArgument)
        );
    }

    #[test]
    fn a_matching_contract_version_validates() {
        assert_eq!(stub().capabilities().validate(), Ok(()));
    }

    #[test]
    fn a_mismatched_contract_version_is_rejected() {
        let mut caps = stub().capabilities();
        caps.contract_version = CONTRACT_VERSION + 1;

        assert_eq!(
            caps.validate(),
            Err(PlatformError::ContractVersionMismatch {
                expected: CONTRACT_VERSION,
                found: CONTRACT_VERSION + 1,
            }),
            "a version disagreement must surface at startup, not inside an unrelated call"
        );
    }

    #[test]
    fn a_backend_states_whether_it_can_report_backpressure() {
        // A shell that sees `false` knows a zero dropped-frame count proves nothing.
        let video = stub().capabilities().video.expect("stub has video");

        assert!(video.reports_backpressure);
        assert_eq!(video.pixel_format, PixelFormat::Rgb565);
    }

    #[test]
    fn storage_capabilities_separate_writable_from_formattable() {
        let read_only = StorageCapabilities {
            writable: false,
            formattable: false,
        };
        let writable = StorageCapabilities {
            writable: true,
            formattable: false,
        };

        assert_ne!(read_only, writable);
    }

    #[test]
    fn platform_error_uses_the_portable_error_contract() {
        fn assert_core_error<T: core::error::Error>() {}

        assert_core_error::<PlatformError>();
    }

    #[test]
    fn platform_error_messages_name_the_mismatched_versions() {
        extern crate alloc;
        use alloc::format;

        let rendered = format!(
            "{}",
            PlatformError::ContractVersionMismatch {
                expected: 1,
                found: 7,
            }
        );

        assert!(
            rendered.contains('1') && rendered.contains('7'),
            "{rendered}"
        );
    }

    #[test]
    fn audio_capabilities_state_the_queue_bound() {
        // The bound belongs to the descriptor so a shell can size its own buffers
        // before the first sample is queued. M1-008 owns the queue itself.
        let caps = AudioCapabilities {
            sample_rate_hz: 48_000,
            channels: 2,
            max_queued_frames: 4_096,
        };

        assert_eq!(caps.max_queued_frames, 4_096);
    }
}
