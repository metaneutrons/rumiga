// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Platform abstraction traits for the Rumiga Amiga emulator.
//!
//! Defines the interfaces that platform backends must implement for video
//! output, audio output, input handling, storage access, and diagnostic
//! record transport.
//!
//! All traits are `no_std`-compatible and use only `core` and `alloc` types.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

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
    /// `framebuffer` contains RGB565 pixel data, row-major, top-to-bottom.
    fn present_frame(&mut self, framebuffer: &[u16], width: u32, height: u32);
}

/// Audio output trait — queues stereo audio samples for playback.
pub trait AudioOutput {
    /// Queue stereo audio samples for playback.
    ///
    /// Both slices must have the same length. Samples are signed 16-bit PCM.
    fn queue_samples(&mut self, left: &[i16], right: &[i16]);
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

/// Storage error type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageError {
    /// File or path not found.
    NotFound,
    /// I/O error during read/write.
    IoError,
    /// Storage medium not present.
    NoMedia,
    /// Filesystem is full.
    Full,
    /// Operation not supported.
    Unsupported,
}

/// Storage trait — provides access to disk images and file management.
///
/// All methods may return [`StorageError`] on failure.
pub trait Storage {
    /// List files in the given directory path.
    ///
    /// # Errors
    /// Returns [`StorageError::NotFound`] if the path doesn't exist,
    /// or [`StorageError::IoError`] on I/O failure.
    fn list_files(&mut self, path: &str) -> Result<Vec<FileInfo>, StorageError>;

    /// Read bytes from a file at the given offset. Returns bytes read.
    ///
    /// # Errors
    /// Returns [`StorageError::NotFound`] if the file doesn't exist,
    /// or [`StorageError::IoError`] on I/O failure.
    fn read_file(&mut self, path: &str, offset: u64, buf: &mut [u8])
    -> Result<usize, StorageError>;

    /// Write bytes to a file at the given offset.
    ///
    /// # Errors
    /// Returns [`StorageError::Full`] if storage is exhausted,
    /// or [`StorageError::IoError`] on I/O failure.
    fn write_file(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<(), StorageError>;

    /// Delete a file.
    ///
    /// # Errors
    /// Returns [`StorageError::NotFound`] if the file doesn't exist.
    fn delete_file(&mut self, path: &str) -> Result<(), StorageError>;

    /// Get the total and free space in bytes as `(total, free)`.
    ///
    /// # Errors
    /// Returns [`StorageError::NoMedia`] if no storage is present.
    fn space_info(&mut self) -> Result<(u64, u64), StorageError>;

    /// Format the storage medium as FAT32.
    ///
    /// # Errors
    /// Returns [`StorageError::IoError`] on failure,
    /// or [`StorageError::Unsupported`] if formatting is not available.
    fn format(&mut self) -> Result<(), StorageError>;
}
