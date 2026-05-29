// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Rumiga desktop binary — development and debugging target.

use std::fs;
use std::process;

use minifb::Key;
use rumiga_core::emulator::Emulator;
use rumiga_core::memory::MemoryConfig;
use rumiga_core::playfield::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use rumiga_platform::VideoOutput;
use rumiga_platform_desktop::DesktopVideo;

const WIDTH: usize = DISPLAY_WIDTH as usize;
const HEIGHT: usize = DISPLAY_HEIGHT as usize;
const DEFAULT_SCALE: usize = 1;
const VERTICAL_STRETCH_FACTOR: usize = 2;
const ROM_SIZE_256K: usize = 256 * 1024;
const ROM_SIZE_512K: usize = 512 * 1024;

/// Amiga ESC keycode.
const AMIGA_KEY_ESC: u8 = 0x45;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MachineModel {
    A500,
    A500Plus,
    A600,
    A1200,
}

impl MachineModel {
    const fn config(self) -> MemoryConfig {
        match self {
            Self::A500 => MemoryConfig::a500(),
            Self::A500Plus | Self::A600 => MemoryConfig::a500_plus(),
            Self::A1200 => MemoryConfig::a1200(),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::A500 => "a500",
            Self::A500Plus => "a500-plus",
            Self::A600 => "a600",
            Self::A1200 => "a1200",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "a500" => Some(Self::A500),
            "a500+" | "a500-plus" | "a500plus" => Some(Self::A500Plus),
            "a600" => Some(Self::A600),
            "a1200" => Some(Self::A1200),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewportMode {
    Auto,
    Raw,
}

impl ViewportMode {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "raw" => Some(Self::Raw),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct LaunchArgs {
    model: Option<MachineModel>,
    scale: usize,
    viewport_mode: ViewportMode,
    vertical_stretch: bool,
    rom_path: String,
    adf_paths: Vec<String>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let launch_args = parse_args(&args).unwrap_or_else(|e| {
        eprintln!("{e}");
        print_usage();
        process::exit(1);
    });

    let rom_path = &launch_args.rom_path;
    let rom_data = fs::read(rom_path).unwrap_or_else(|e| {
        eprintln!("Failed to read ROM file '{rom_path}': {e}");
        process::exit(1);
    });

    let model = select_model(&launch_args, rom_data.len()).unwrap_or_else(|e| {
        eprintln!("{e}");
        process::exit(1);
    });
    eprintln!("Starting Rumiga with {} profile", model.name());

    let rom_size = u32::try_from(rom_data.len()).unwrap_or_else(|_| {
        eprintln!("ROM file is too large: {} bytes", rom_data.len());
        process::exit(1);
    });
    let mut config = model.config();
    config.rom_size = rom_size;
    let mut emulator = Emulator::new(config);
    emulator.load_rom(&rom_data);

    // Load ADF disk images into DF0-DF3 in argument order.
    for (drive, adf_path) in launch_args.adf_paths.iter().enumerate() {
        let adf_data = fs::read(adf_path).unwrap_or_else(|e| {
            eprintln!("Failed to read ADF file '{adf_path}': {e}");
            process::exit(1);
        });
        eprintln!("Inserted {adf_path} as DF{drive}");
        emulator.insert_floppy(drive, adf_data);
    }

    let presented_height = presented_height(launch_args.vertical_stretch);
    let mut video = DesktopVideo::new("Rumiga", WIDTH, presented_height, launch_args.scale)
        .unwrap_or_else(|| {
            eprintln!("Failed to create video window");
            process::exit(1);
        });

    let window_handle = video.window_handle();

    #[allow(clippy::cast_possible_truncation)]
    let (w, h) = (WIDTH as u32, presented_height as u32);
    let mut presented_framebuffer = vec![0u16; WIDTH * presented_height];

    while video.is_open() {
        // Check ESC to quit
        if window_handle.borrow().is_key_down(Key::Escape) {
            break;
        }

        // Pass key events to emulator
        {
            let win = window_handle.borrow();
            for key in win.get_keys_pressed(minifb::KeyRepeat::No) {
                if let Some(keycode) = map_key_to_amiga(key) {
                    emulator.key_event(keycode, true);
                }
            }
            for key in win.get_keys_released() {
                if let Some(keycode) = map_key_to_amiga(key) {
                    emulator.key_event(keycode, false);
                }
            }
        }

        emulator.run_frame();
        let framebuffer = emulator.framebuffer();
        if launch_args.vertical_stretch {
            let (y_start, y_end) = if launch_args.viewport_mode == ViewportMode::Auto {
                auto_vertical_bounds(framebuffer, WIDTH, HEIGHT).unwrap_or((0, HEIGHT))
            } else {
                (0, HEIGHT)
            };
            if !stretch_vertical_viewport(
                framebuffer,
                WIDTH,
                HEIGHT,
                y_start,
                y_end,
                presented_height,
                &mut presented_framebuffer,
            ) {
                eprintln!("Failed to prepare video frame");
                break;
            }
            video.present_frame(&presented_framebuffer, w, h);
        } else {
            video.present_frame(framebuffer, w, h);
        }
        emulator.clear_frame_ready();
    }
}

fn parse_args(args: &[String]) -> Result<LaunchArgs, String> {
    let mut model = None;
    let mut scale = DEFAULT_SCALE;
    let mut viewport_mode = ViewportMode::Auto;
    let mut vertical_stretch = true;
    let mut positional = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--model" | "-m" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--model requires a value".to_owned());
                };
                model = Some(
                    MachineModel::parse(value)
                        .ok_or_else(|| format!("Unsupported machine model '{value}'"))?,
                );
                index += 2;
            }
            "--scale" | "-s" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--scale requires a value".to_owned());
                };
                scale = parse_scale(value)?;
                index += 2;
            }
            "--viewport" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--viewport requires a value".to_owned());
                };
                viewport_mode = ViewportMode::parse(value)
                    .ok_or_else(|| format!("Unsupported viewport mode '{value}'"))?;
                index += 2;
            }
            "--no-vertical-stretch" => {
                vertical_stretch = false;
                index += 1;
            }
            "--help" | "-h" => return Err(String::new()),
            value if value.starts_with('-') => return Err(format!("Unknown option '{value}'")),
            value => {
                positional.push(value.to_owned());
                index += 1;
            }
        }
    }

    let Some(rom_path) = positional.first() else {
        return Err("Missing Kickstart ROM path".to_owned());
    };
    if positional.len() > 5 {
        return Err("Too many disk images; Rumiga supports DF0 through DF3".to_owned());
    }

    Ok(LaunchArgs {
        model,
        scale,
        viewport_mode,
        vertical_stretch,
        rom_path: rom_path.clone(),
        adf_paths: positional.iter().skip(1).cloned().collect(),
    })
}

fn parse_scale(value: &str) -> Result<usize, String> {
    let scale = value
        .parse::<usize>()
        .map_err(|_| format!("Unsupported scale '{value}'"))?;
    match scale {
        1 | 2 | 4 | 8 | 16 | 32 => Ok(scale),
        _ => Err(format!("Unsupported scale '{value}'")),
    }
}

fn select_model(args: &LaunchArgs, rom_size: usize) -> Result<MachineModel, String> {
    let model = args
        .model
        .unwrap_or_else(|| infer_model_from_rom(&args.rom_path, rom_size));

    let is_valid_size = match model {
        MachineModel::A500 => rom_size == ROM_SIZE_256K || rom_size == ROM_SIZE_512K,
        MachineModel::A500Plus | MachineModel::A600 | MachineModel::A1200 => {
            rom_size == ROM_SIZE_512K
        }
    };

    if !is_valid_size {
        let expected = match model {
            MachineModel::A500 => "256 or 512 KB",
            _ => "512 KB",
        };
        return Err(format!(
            "{} profile expects a {} ROM, got {} bytes",
            model.name(),
            expected,
            rom_size
        ));
    }

    Ok(model)
}

fn infer_model_from_rom(rom_path: &str, rom_size: usize) -> MachineModel {
    if rom_size == ROM_SIZE_256K {
        return MachineModel::A500;
    }

    let lower_path = rom_path.to_ascii_lowercase();
    if lower_path.contains("a1200") {
        MachineModel::A1200
    } else if lower_path.contains("a600") {
        MachineModel::A600
    } else {
        MachineModel::A500Plus
    }
}

fn print_usage() {
    eprintln!(
        "Usage: rumiga-desktop [--model a500|a500-plus|a600|a1200] [--scale 1|2|4|8|16|32] [--viewport auto|raw] [--no-vertical-stretch] <kickstart.rom> [df0.adf] [df1.adf] [df2.adf] [df3.adf]"
    );
}

const fn presented_height(vertical_stretch: bool) -> usize {
    if vertical_stretch {
        HEIGHT * VERTICAL_STRETCH_FACTOR
    } else {
        HEIGHT
    }
}

fn auto_vertical_bounds(
    framebuffer: &[u16],
    width: usize,
    height: usize,
) -> Option<(usize, usize)> {
    if width == 0 || height == 0 {
        return None;
    }

    let pixel_count = width.checked_mul(height)?;
    if framebuffer.len() < pixel_count {
        return None;
    }

    let background = framebuffer[0];
    let mut first_content_row = None;
    let mut last_content_row = 0usize;

    for y in 0..height {
        let row_start = y * width;
        let row = &framebuffer[row_start..row_start + width];
        if row_has_video_content(row, background) {
            first_content_row.get_or_insert(y);
            last_content_row = y;
        }
    }

    first_content_row.map(|first| (first.saturating_sub(1), (last_content_row + 2).min(height)))
}

fn row_has_video_content(row: &[u16], background: u16) -> bool {
    let Some((&first, rest)) = row.split_first() else {
        return false;
    };
    let mut differs_from_first = false;
    let mut differs_from_background = first != background;
    let mut first_non_background = if first == background {
        None
    } else {
        Some(0usize)
    };
    let mut last_non_background = first_non_background;

    for (index, &pixel) in rest.iter().enumerate() {
        differs_from_first |= pixel != first;
        differs_from_background |= pixel != background;
        if pixel != background {
            let pixel_index = index + 1;
            first_non_background.get_or_insert(pixel_index);
            last_non_background = Some(pixel_index);
        }
    }

    let Some(first_non_background) = first_non_background else {
        return false;
    };
    let Some(last_non_background) = last_non_background else {
        return false;
    };

    let minimum_screen_span = row.len() * 4 / 5;
    differs_from_first
        && differs_from_background
        && last_non_background.saturating_sub(first_non_background) >= minimum_screen_span
}

fn stretch_vertical_viewport(
    framebuffer: &[u16],
    width: usize,
    height: usize,
    y_start: usize,
    y_end: usize,
    output_height: usize,
    output: &mut [u16],
) -> bool {
    let Some(input_pixel_count) = width.checked_mul(height) else {
        return false;
    };
    let Some(output_pixel_count) = width.checked_mul(output_height) else {
        return false;
    };
    if width == 0
        || height == 0
        || output_height == 0
        || y_start >= y_end
        || y_end > height
        || framebuffer.len() < input_pixel_count
        || output.len() < output_pixel_count
    {
        return false;
    }

    let viewport_height = y_end - y_start;
    for dest_y in 0..output_height {
        let source_y = y_start + (dest_y * viewport_height / output_height);
        let source_start = source_y * width;
        let dest_start = dest_y * width;
        output[dest_start..dest_start + width]
            .copy_from_slice(&framebuffer[source_start..source_start + width]);
    }

    true
}

/// Map a minifb key to an Amiga raw keycode.
const fn map_key_to_amiga(key: Key) -> Option<u8> {
    match key {
        Key::Escape => Some(AMIGA_KEY_ESC),
        Key::Space => Some(0x40),
        Key::Enter => Some(0x44),
        Key::Up => Some(0x4C),
        Key::Down => Some(0x4D),
        Key::Left => Some(0x4F),
        Key::Right => Some(0x4E),
        Key::Backspace => Some(0x41),
        Key::Tab => Some(0x42),
        Key::A => Some(0x20),
        Key::B => Some(0x35),
        Key::C => Some(0x33),
        Key::D => Some(0x22),
        Key::E => Some(0x12),
        Key::F => Some(0x23),
        Key::G => Some(0x24),
        Key::H => Some(0x25),
        Key::I => Some(0x17),
        Key::J => Some(0x26),
        Key::K => Some(0x27),
        Key::L => Some(0x28),
        Key::M => Some(0x37),
        Key::N => Some(0x36),
        Key::O => Some(0x18),
        Key::P => Some(0x19),
        Key::Q => Some(0x10),
        Key::R => Some(0x13),
        Key::S => Some(0x21),
        Key::T => Some(0x14),
        Key::U => Some(0x16),
        Key::V => Some(0x34),
        Key::W => Some(0x11),
        Key::X => Some(0x32),
        Key::Y => Some(0x15),
        Key::Z => Some(0x31),
        Key::Key0 => Some(0x0A),
        Key::Key1 => Some(0x01),
        Key::Key2 => Some(0x02),
        Key::Key3 => Some(0x03),
        Key::Key4 => Some(0x04),
        Key::Key5 => Some(0x05),
        Key::Key6 => Some(0x06),
        Key::Key7 => Some(0x07),
        Key::Key8 => Some(0x08),
        Key::Key9 => Some(0x09),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_accepts_model_rom_and_disk() {
        let args = vec![
            "--model".to_owned(),
            "a1200".to_owned(),
            "kick.rom".to_owned(),
            "workbench.adf".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                model: Some(MachineModel::A1200),
                scale: DEFAULT_SCALE,
                viewport_mode: ViewportMode::Auto,
                vertical_stretch: true,
                rom_path: "kick.rom".to_owned(),
                adf_paths: vec!["workbench.adf".to_owned()],
            })
        );
    }

    #[test]
    fn parse_args_accepts_up_to_four_disks() {
        let args = vec![
            "kick.rom".to_owned(),
            "df0.adf".to_owned(),
            "df1.adf".to_owned(),
            "df2.adf".to_owned(),
            "df3.adf".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                model: None,
                scale: DEFAULT_SCALE,
                viewport_mode: ViewportMode::Auto,
                vertical_stretch: true,
                rom_path: "kick.rom".to_owned(),
                adf_paths: vec![
                    "df0.adf".to_owned(),
                    "df1.adf".to_owned(),
                    "df2.adf".to_owned(),
                    "df3.adf".to_owned(),
                ],
            })
        );
    }

    #[test]
    fn parse_args_accepts_scale() {
        let args = vec!["--scale".to_owned(), "1".to_owned(), "kick.rom".to_owned()];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                model: None,
                scale: 1,
                viewport_mode: ViewportMode::Auto,
                vertical_stretch: true,
                rom_path: "kick.rom".to_owned(),
                adf_paths: Vec::new(),
            })
        );
    }

    #[test]
    fn parse_args_rejects_unsupported_scale() {
        let args = vec!["--scale".to_owned(), "3".to_owned(), "kick.rom".to_owned()];

        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn select_model_infers_a1200_from_rom_name() {
        let args = LaunchArgs {
            model: None,
            scale: DEFAULT_SCALE,
            viewport_mode: ViewportMode::Auto,
            vertical_stretch: true,
            rom_path: "kick.a1200.46.143.rom".to_owned(),
            adf_paths: Vec::new(),
        };

        assert_eq!(select_model(&args, ROM_SIZE_512K), Ok(MachineModel::A1200));
    }

    #[test]
    fn select_model_rejects_wrong_rom_size() {
        let args = LaunchArgs {
            model: Some(MachineModel::A1200),
            scale: DEFAULT_SCALE,
            viewport_mode: ViewportMode::Auto,
            vertical_stretch: true,
            rom_path: "kick.a500.34.005.rom".to_owned(),
            adf_paths: Vec::new(),
        };

        assert!(select_model(&args, ROM_SIZE_256K).is_err());
    }

    #[test]
    fn parse_args_accepts_raw_viewport_without_vertical_stretch() {
        let args = vec![
            "--viewport".to_owned(),
            "raw".to_owned(),
            "--no-vertical-stretch".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                model: None,
                scale: DEFAULT_SCALE,
                viewport_mode: ViewportMode::Raw,
                vertical_stretch: false,
                rom_path: "kick.rom".to_owned(),
                adf_paths: Vec::new(),
            })
        );
    }

    #[test]
    fn presented_height_line_doubles_when_vertical_stretch_is_enabled() {
        assert_eq!(presented_height(true), HEIGHT * 2);
        assert_eq!(presented_height(false), HEIGHT);
    }

    #[test]
    fn auto_vertical_bounds_ignore_uniform_bottom_blank() {
        let width = 4usize;
        let height = 6usize;
        let bg = 1u16;
        let blue = 2u16;
        let black = 0u16;
        let framebuffer = [
            bg, bg, bg, bg, //
            blue, bg, bg, blue, //
            blue, bg, bg, blue, //
            bg, bg, bg, bg, //
            black, black, black, black, //
            black, black, black, black,
        ];

        assert_eq!(
            auto_vertical_bounds(&framebuffer, width, height),
            Some((0, 4))
        );
    }

    #[test]
    fn auto_vertical_bounds_ignore_centered_insert_disk_art() {
        let width = 8usize;
        let height = 4usize;
        let bg = 1u16;
        let fg = 2u16;
        let framebuffer = [
            bg, bg, bg, bg, bg, bg, bg, bg, //
            bg, bg, bg, fg, fg, bg, bg, bg, //
            bg, bg, bg, fg, fg, bg, bg, bg, //
            bg, bg, bg, bg, bg, bg, bg, bg,
        ];

        assert_eq!(auto_vertical_bounds(&framebuffer, width, height), None);
    }

    #[test]
    fn stretch_vertical_viewport_maps_crop_to_full_height() {
        let width = 2usize;
        let height = 4usize;
        let framebuffer = [
            10u16, 10, //
            20, 20, //
            30, 30, //
            40, 40,
        ];
        let mut output = [0u16; 8];

        assert!(stretch_vertical_viewport(
            &framebuffer,
            width,
            height,
            1,
            3,
            height,
            &mut output
        ));

        assert_eq!(output, [20, 20, 20, 20, 30, 30, 30, 30]);
    }

    #[test]
    fn stretch_vertical_viewport_line_doubles_full_frame() {
        let width = 2usize;
        let height = 2usize;
        let framebuffer = [
            10u16, 11, //
            20, 21,
        ];
        let mut output = [0u16; 8];

        assert!(stretch_vertical_viewport(
            &framebuffer,
            width,
            height,
            0,
            height,
            height * 2,
            &mut output
        ));

        assert_eq!(output, [10, 11, 10, 11, 20, 21, 20, 21]);
    }
}
