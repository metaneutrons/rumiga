// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Desktop platform backend using minifb for video and basic keyboard input,
//! plus the host file transport for core diagnostic records.
//!
//! This backend is used for development and debugging on macOS/Linux/Windows.

use std::cell::RefCell;
use std::fmt;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use minifb::{Key, Scale, Window, WindowOptions};
use rumiga_platform::{Clock, InputSource, InputState, KeyEvent, TraceSink, VideoOutput};

/// Convert an RGB565 pixel to u32 ARGB (`0xFF_RR_GG_BB`).
#[must_use]
pub const fn rgb565_to_argb(pixel: u16) -> u32 {
    let r = ((pixel >> 11) & 0x1F) as u32 * 255 / 31;
    let g = ((pixel >> 5) & 0x3F) as u32 * 255 / 63;
    let b = (pixel & 0x1F) as u32 * 255 / 31;
    0xFF00_0000 | (r << 16) | (g << 8) | b
}

/// Shared window handle used by both video and input.
pub type SharedWindow = Rc<RefCell<Window>>;

/// Desktop video output using minifb.
pub struct DesktopVideo {
    window: SharedWindow,
    buffer: Vec<u32>,
}

impl DesktopVideo {
    /// Create a new desktop video window.
    ///
    /// Returns `None` if the window cannot be created.
    #[must_use]
    pub fn new(title: &str, width: usize, height: usize, scale: usize) -> Option<Self> {
        let scale = match scale {
            1 => Scale::X1,
            4 => Scale::X4,
            8 => Scale::X8,
            16 => Scale::X16,
            32 => Scale::X32,
            _ => Scale::X2,
        };
        let mut window = Window::new(
            title,
            width,
            height,
            WindowOptions {
                scale,
                ..WindowOptions::default()
            },
        )
        .ok()?;
        window.set_target_fps(60);
        Some(Self {
            window: Rc::new(RefCell::new(window)),
            buffer: vec![0; width * height],
        })
    }

    /// Returns a shared reference to the underlying window for use with [`DesktopInput`].
    #[must_use]
    pub fn window_handle(&self) -> SharedWindow {
        Rc::clone(&self.window)
    }

    /// Returns `true` if the window is still open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.window.borrow().is_open()
    }
}

impl VideoOutput for DesktopVideo {
    fn present_frame(&mut self, framebuffer: &[u16], width: u32, height: u32) {
        let count = width as usize * height as usize;
        self.buffer.resize(count, 0);
        for (i, &pixel) in framebuffer.iter().take(count).enumerate() {
            self.buffer[i] = rgb565_to_argb(pixel);
        }
        let _ = self.window.borrow_mut().update_with_buffer(
            &self.buffer,
            width as usize,
            height as usize,
        );
    }
}

/// Desktop keyboard input using minifb key state.
pub struct DesktopInput {
    window: SharedWindow,
}

impl DesktopInput {
    /// Create a new desktop input source from a shared window handle.
    #[must_use]
    pub const fn new(window: SharedWindow) -> Self {
        Self { window }
    }
}

/// Map a minifb key to an Amiga raw keycode.
const fn map_key(key: Key) -> Option<u8> {
    match key {
        Key::Escape => Some(0x45),
        Key::Space => Some(0x40),
        Key::Enter => Some(0x44),
        Key::Up => Some(0x4C),
        Key::Down => Some(0x4D),
        Key::Left => Some(0x4F),
        Key::Right => Some(0x4E),
        _ => None,
    }
}

/// Keys polled for input mapping.
const POLLED_KEYS: &[Key] = &[
    Key::Escape,
    Key::Space,
    Key::Enter,
    Key::Up,
    Key::Down,
    Key::Left,
    Key::Right,
];

impl InputSource for DesktopInput {
    fn poll(&mut self) -> InputState {
        let window = self.window.borrow();
        let mut state = InputState::default();
        for &key in POLLED_KEYS {
            if let Some(keycode) = map_key(key) {
                if window.is_key_down(key) {
                    state.key_events.push(KeyEvent {
                        keycode,
                        pressed: true,
                    });
                }
            }
        }
        state
    }
}

/// Buffered file transport for core diagnostic records.
///
/// This adapter owns host file creation and buffering; the core only formats
/// records. Each record is terminated with `\n`.
pub struct FileTraceSink {
    writer: BufWriter<File>,
}

impl FileTraceSink {
    /// Create a trace sink writing to `path`, truncating any existing file.
    ///
    /// # Errors
    ///
    /// Returns any file creation error from the host filesystem.
    pub fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self {
            writer: BufWriter::new(File::create(path)?),
        })
    }
}

impl TraceSink for FileTraceSink {
    fn write_record(&mut self, record: fmt::Arguments<'_>) {
        // Diagnostics must not abort emulation, so transport errors are dropped.
        let _ = writeln!(self.writer, "{record}");
    }

    fn flush(&mut self) {
        let _ = self.writer.flush();
    }
}

/// Host clock and pacing for the desktop shell.
///
/// The epoch is the moment of construction, which keeps `now` monotonic and small.
pub struct DesktopClock {
    origin: Instant,
}

impl Default for DesktopClock {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopClock {
    /// Create a clock whose epoch is now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Clock for DesktopClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }

    fn pace(&mut self, requested: Duration) -> Duration {
        let started = Instant::now();
        if requested.is_zero() {
            std::thread::yield_now();
        } else {
            std::thread::sleep(requested);
        }
        // Report what the host did, not what was asked for; sleep routinely
        // overshoots and a pacing caller must correct against the measurement.
        started.elapsed()
    }
}
