// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Rumiga desktop binary — development and debugging target.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use minifb::Key;
use rumiga_core::emulator::Emulator;
use rumiga_core::floppy::{
    FLOPPY_SPEED_COMPATIBLE_PERCENT, FLOPPY_SPEED_TURBO_PERCENT, is_supported_floppy_speed_percent,
};
use rumiga_core::memory::MemoryConfig;
use rumiga_core::playfield::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use rumiga_platform::VideoOutput;
use rumiga_platform_desktop::DesktopVideo;
use sha2::{Digest, Sha256};

const WIDTH: usize = DISPLAY_WIDTH as usize;
const HEIGHT: usize = DISPLAY_HEIGHT as usize;
const DEFAULT_SCALE: usize = 1;
const DEFAULT_CAPTURE_FRAMES: u64 = 300;
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

    const fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Raw => "raw",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct LaunchArgs {
    model: Option<MachineModel>,
    scale: usize,
    viewport_mode: ViewportMode,
    vertical_stretch: bool,
    floppy_speed_percent: u16,
    rom_path: String,
    adf_paths: Vec<String>,
    hdf_path: Option<String>,
    cpu: Option<m68k::CpuType>,
    chip_ram: Option<u32>,
    slow_ram: Option<u32>,
    fast_ram: Option<u32>,
    pal: bool,
    ntsc: bool,
    df0: Option<String>,
    df1: Option<String>,
    df2: Option<String>,
    df3: Option<String>,
    trace_cpu: Option<String>,
    trace_limit: Option<u64>,
    capture_path: Option<String>,
    capture_manifest_path: Option<String>,
    capture_frames: u64,
}

#[derive(Clone, Debug)]
struct FileEvidence {
    path: String,
    bytes: usize,
    sha256: String,
}

struct CaptureFrame {
    pixels: Vec<u16>,
    width: usize,
    height: usize,
    source_y_start: usize,
    source_y_end: usize,
}

struct CaptureEvidenceContext<'a> {
    args: &'a LaunchArgs,
    model: MachineModel,
    config: &'a MemoryConfig,
    rom: &'a FileEvidence,
    floppies: &'a [Option<FileEvidence>; 4],
    hdf: Option<&'a FileEvidence>,
    capture_path: &'a str,
}

struct CaptureManifestContext<'a> {
    image_path: &'a Path,
    frame: &'a CaptureFrame,
    args: &'a LaunchArgs,
    model: MachineModel,
    config: &'a MemoryConfig,
    emulator: &'a Emulator,
    rom: &'a FileEvidence,
    floppies: &'a [Option<FileEvidence>; 4],
    hdf: Option<&'a FileEvidence>,
}

#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let launch_args = parse_args(&args).unwrap_or_else(|e| {
        if e.is_empty() {
            // Help requested cleanly (-h or --help)
            print_usage(true);
            process::exit(0);
        }
        eprintln!("Error: {e}");
        print_usage(false);
        process::exit(1);
    });

    let rom_path = &launch_args.rom_path;
    let rom_data = fs::read(rom_path).unwrap_or_else(|e| {
        eprintln!("Failed to read ROM file '{rom_path}': {e}");
        process::exit(1);
    });
    let rom_evidence = file_evidence_from_bytes(rom_path, &rom_data);

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

    // Apply CLI overrides to MemoryConfig
    if let Some(cpu_override) = launch_args.cpu {
        config.cpu_type = cpu_override;
    }
    if let Some(chip_ram_override) = launch_args.chip_ram {
        config.chip_ram_size = chip_ram_override;
    }
    if let Some(slow_ram_override) = launch_args.slow_ram {
        config.slow_ram_size = slow_ram_override;
    }
    if let Some(fast_ram_override) = launch_args.fast_ram {
        config.fast_ram_size = fast_ram_override;
    }
    let config_summary = config.clone();

    // Print hardware configuration summary
    eprintln!("--- Hardware Configuration ---");
    eprintln!("  Model:          {}", model.name());
    eprintln!("  CPU Type:       {:?}", config.cpu_type);
    eprintln!("  Chip RAM:       {} KB", config.chip_ram_size / 1024);
    eprintln!("  Slow RAM:       {} KB", config.slow_ram_size / 1024);
    eprintln!("  Fast RAM:       {} KB", config.fast_ram_size / 1024);
    let video_std = if launch_args.ntsc {
        "NTSC (60Hz)"
    } else {
        "PAL (50Hz)"
    };
    eprintln!("  Video Standard: {video_std}");
    if launch_args.ntsc {
        eprintln!(
            "  [WARNING] The core graphics timing is currently optimized for PAL; NTSC overrides may not be fully supported."
        );
    }
    if let Some(ref trace_path) = launch_args.trace_cpu {
        eprintln!("  CPU Tracing:    Enabled -> {trace_path}");
        if let Some(limit) = launch_args.trace_limit {
            eprintln!("  Trace Limit:    {limit} instructions");
        }
    }
    eprintln!("------------------------------");

    let mut emulator = Emulator::new(config);
    if let Some(ref trace_path) = launch_args.trace_cpu {
        if let Err(e) = emulator.enable_cpu_trace(trace_path, launch_args.trace_limit) {
            eprintln!("Failed to enable CPU tracing to '{trace_path}': {e}");
            process::exit(1);
        }
    }
    emulator.set_floppy_speed_percent(launch_args.floppy_speed_percent);
    emulator.load_rom(&rom_data);

    let mut floppy_paths: [Option<String>; 4] = [None, None, None, None];
    let mut floppy_evidence: [Option<FileEvidence>; 4] = std::array::from_fn(|_| None);

    // Helper closure to load floppy disk image into specified drive
    let mut load_floppy = |drive_idx: usize, path: &str| {
        let adf_data = fs::read(path).unwrap_or_else(|e| {
            eprintln!("Failed to read ADF file '{path}': {e}");
            process::exit(1);
        });
        eprintln!("Inserted {path} as DF{drive_idx}");
        let evidence = file_evidence_from_bytes(path, &adf_data);
        emulator.insert_floppy(drive_idx, adf_data);
        floppy_paths[drive_idx] = Some(path.to_owned());
        floppy_evidence[drive_idx] = Some(evidence);
    };

    // Load positional floppies
    for (drive_idx, adf_path) in launch_args.adf_paths.iter().enumerate() {
        load_floppy(drive_idx, adf_path);
    }

    // Load explicit named floppies (overriding positional)
    if let Some(ref df0_path) = launch_args.df0 {
        load_floppy(0, df0_path);
    }
    if let Some(ref df1_path) = launch_args.df1 {
        load_floppy(1, df1_path);
    }
    if let Some(ref df2_path) = launch_args.df2 {
        load_floppy(2, df2_path);
    }
    if let Some(ref df3_path) = launch_args.df3 {
        load_floppy(3, df3_path);
    }

    // Mount HDF if provided
    let mut hdf_evidence = None;
    if let Some(ref hdf_path) = launch_args.hdf_path {
        let hdf_data = fs::read(hdf_path).unwrap_or_else(|e| {
            eprintln!("Failed to read HDF file '{hdf_path}': {e}");
            process::exit(1);
        });
        hdf_evidence = Some(file_evidence_from_bytes(hdf_path, &hdf_data));
        eprintln!(
            "Mounted Gayle IDE HDF: {hdf_path} ({} bytes)",
            hdf_data.len()
        );
        emulator.insert_hdf(hdf_data);
    }

    if let Some(ref capture_path) = launch_args.capture_path {
        if let Err(e) = capture_evidence(
            &mut emulator,
            &CaptureEvidenceContext {
                args: &launch_args,
                model,
                config: &config_summary,
                rom: &rom_evidence,
                floppies: &floppy_evidence,
                hdf: hdf_evidence.as_ref(),
                capture_path,
            },
        ) {
            eprintln!("Capture failed: {e}");
            process::exit(1);
        }
        return;
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

    flush_dirty_media(&mut emulator, &launch_args, &floppy_paths);
}

fn flush_dirty_media(
    emulator: &mut Emulator,
    launch_args: &LaunchArgs,
    floppy_paths: &[Option<String>; 4],
) {
    // Write back dirty HDF sectors before exiting
    if let Some(ref hdf_path) = launch_args.hdf_path {
        if emulator.hdf_dirty() {
            if let Some(data) = emulator.extract_hdf() {
                eprintln!("Writing dirty HDF sectors back to {hdf_path}...");
                if let Err(e) = fs::write(hdf_path, data) {
                    eprintln!("Failed to write HDF file '{hdf_path}': {e}");
                } else {
                    emulator.clear_hdf_dirty();
                }
            }
        }
    }

    // Write back dirty floppy disk data before exiting
    for (drive_idx, path_opt) in floppy_paths.iter().enumerate() {
        if let Some(path) = path_opt {
            if emulator.floppy_dirty(drive_idx) {
                if let Some(data) = emulator.extract_floppy(drive_idx) {
                    eprintln!("Writing dirty floppy sectors back to {path}...");
                    if let Err(e) = fs::write(path, data) {
                        eprintln!("Failed to write ADF file '{path}': {e}");
                    } else {
                        emulator.clear_floppy_dirty(drive_idx);
                    }
                }
            }
        }
    }
}

#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
fn parse_args(args: &[String]) -> Result<LaunchArgs, String> {
    let mut model = None;
    let mut scale = DEFAULT_SCALE;
    let mut viewport_mode = ViewportMode::Auto;
    let mut vertical_stretch = true;
    let mut floppy_speed_percent = FLOPPY_SPEED_COMPATIBLE_PERCENT;
    let mut hdf_path = None;
    let mut cpu = None;
    let mut chip_ram = None;
    let mut slow_ram = None;
    let mut fast_ram = None;
    let mut pal = false;
    let mut ntsc = false;
    let mut df0 = None;
    let mut df1 = None;
    let mut df2 = None;
    let mut df3 = None;
    let mut trace_cpu = None;
    let mut trace_limit = None;
    let mut capture_path = None;
    let mut capture_manifest_path = None;
    let mut capture_frames = DEFAULT_CAPTURE_FRAMES;
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
            "--floppy-speed" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--floppy-speed requires a value".to_owned());
                };
                floppy_speed_percent = parse_floppy_speed(value)?;
                index += 2;
            }
            "--hdf" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--hdf requires a value".to_owned());
                };
                hdf_path = Some(value.clone());
                index += 2;
            }
            "--cpu" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--cpu requires a value".to_owned());
                };
                cpu = Some(parse_cpu_type(value)?);
                index += 2;
            }
            "--chip-ram" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--chip-ram requires a value".to_owned());
                };
                chip_ram = Some(parse_ram_size(value)?);
                index += 2;
            }
            "--slow-ram" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--slow-ram requires a value".to_owned());
                };
                slow_ram = Some(parse_ram_size(value)?);
                index += 2;
            }
            "--fast-ram" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--fast-ram requires a value".to_owned());
                };
                fast_ram = Some(parse_ram_size(value)?);
                index += 2;
            }
            "--pal" => {
                pal = true;
                index += 1;
            }
            "--ntsc" => {
                ntsc = true;
                index += 1;
            }
            "--df0" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--df0 requires a value".to_owned());
                };
                df0 = Some(value.clone());
                index += 2;
            }
            "--df1" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--df1 requires a value".to_owned());
                };
                df1 = Some(value.clone());
                index += 2;
            }
            "--df2" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--df2 requires a value".to_owned());
                };
                df2 = Some(value.clone());
                index += 2;
            }
            "--df3" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--df3 requires a value".to_owned());
                };
                df3 = Some(value.clone());
                index += 2;
            }
            "--trace-cpu" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--trace-cpu requires a value".to_owned());
                };
                trace_cpu = Some(value.clone());
                index += 2;
            }
            "--trace-limit" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--trace-limit requires a value".to_owned());
                };
                let limit = value
                    .parse::<u64>()
                    .map_err(|_| format!("Unsupported trace-limit '{value}'"))?;
                trace_limit = Some(limit);
                index += 2;
            }
            "--capture" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--capture requires a value".to_owned());
                };
                capture_path = Some(value.clone());
                index += 2;
            }
            "--capture-manifest" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--capture-manifest requires a value".to_owned());
                };
                capture_manifest_path = Some(value.clone());
                index += 2;
            }
            "--capture-frames" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--capture-frames requires a value".to_owned());
                };
                capture_frames = parse_capture_frames(value)?;
                index += 2;
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

    // 1. Validate mutually exclusive video timings
    if pal && ntsc {
        return Err("Options --pal and --ntsc are mutually exclusive".to_owned());
    }
    if capture_path.is_none() && capture_manifest_path.is_some() {
        return Err("--capture-manifest requires --capture".to_owned());
    }

    // 2. Validate custom Chip RAM constraints (critical for Alice/Lisa DMA masking)
    if let Some(chip) = chip_ram {
        match chip {
            524_288 | 1_048_576 | 2_097_152 => {} // 512K, 1M, 2M are valid
            _ => {
                return Err(
                    "Invalid Chip RAM size. Amiga custom chips only support 512K, 1M, or 2M."
                        .to_owned(),
                );
            }
        }
    }

    // 3. Validate Slow RAM constraints (trapdoor slow space at 0xC00000)
    if let Some(slow) = slow_ram {
        if slow > 1_835_008 {
            // Max 1.75 MB
            return Err("Slow RAM cannot exceed 1.75 MB.".to_owned());
        }
        if slow % 262_144 != 0 {
            // Must be a multiple of 256K
            return Err("Slow RAM size must be a multiple of 256 KB.".to_owned());
        }
    }

    // 4. Validate Fast RAM constraints (Zorro II space at 0x200000)
    if let Some(fast) = fast_ram {
        if fast > 8_388_608 {
            // Max 8 MB
            return Err("Fast RAM cannot exceed 8 MB in Zorro II address space.".to_owned());
        }
        if fast % 1_048_576 != 0 {
            // Must be a multiple of 1MB
            return Err("Fast RAM size must be a multiple of 1 MB.".to_owned());
        }
    }

    // 5. Validate Floppy Drive slot allocations and conflicts
    let positional_floppies = positional.iter().skip(1).cloned().collect::<Vec<_>>();
    let mut drive_allocations = [false; 4];

    // Count explicit drive maps
    if df0.is_some() {
        drive_allocations[0] = true;
    }
    if df1.is_some() {
        drive_allocations[1] = true;
    }
    if df2.is_some() {
        drive_allocations[2] = true;
    }
    if df3.is_some() {
        drive_allocations[3] = true;
    }

    // Count positional maps
    for (i, _) in positional_floppies.iter().enumerate() {
        if i >= 4 {
            return Err(
                "Too many disk images. Physical hardware is limited to 4 floppy drives.".to_owned(),
            );
        }
        if drive_allocations[i] {
            return Err(format!(
                "Conflict: Positionally supplied floppy {} overlaps with explicit --df{} parameter.",
                i + 1,
                i
            ));
        }
        drive_allocations[i] = true;
    }

    Ok(LaunchArgs {
        model,
        scale,
        viewport_mode,
        vertical_stretch,
        floppy_speed_percent,
        rom_path: rom_path.clone(),
        adf_paths: positional.iter().skip(1).cloned().collect(),
        hdf_path,
        cpu,
        chip_ram,
        slow_ram,
        fast_ram,
        pal,
        ntsc,
        df0,
        df1,
        df2,
        df3,
        trace_cpu,
        trace_limit,
        capture_path,
        capture_manifest_path,
        capture_frames,
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

fn parse_floppy_speed(value: &str) -> Result<u16, String> {
    if value.eq_ignore_ascii_case("turbo") {
        return Ok(FLOPPY_SPEED_TURBO_PERCENT);
    }

    let numeric = value
        .strip_suffix('%')
        .unwrap_or(value)
        .parse::<u16>()
        .map_err(|_| format!("Unsupported floppy speed '{value}'"))?;
    if is_supported_floppy_speed_percent(numeric) {
        Ok(numeric)
    } else {
        Err(format!("Unsupported floppy speed '{value}'"))
    }
}

fn parse_capture_frames(value: &str) -> Result<u64, String> {
    let frames = value
        .parse::<u64>()
        .map_err(|_| format!("Unsupported capture frame count '{value}'"))?;
    if frames == 0 {
        Err("Capture frame count must be greater than zero".to_owned())
    } else {
        Ok(frames)
    }
}

fn parse_ram_size(value: &str) -> Result<u32, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("Empty RAM size value".to_owned());
    }

    let trimmed_lower = trimmed.to_ascii_lowercase();
    let (num_part, suffix) = if trimmed_lower.ends_with("kb") {
        (&trimmed[..trimmed.len() - 2], Some("kb"))
    } else if trimmed_lower.ends_with("mb") {
        (&trimmed[..trimmed.len() - 2], Some("mb"))
    } else if trimmed_lower.ends_with('k') {
        (&trimmed[..trimmed.len() - 1], Some("k"))
    } else if trimmed_lower.ends_with('m') {
        (&trimmed[..trimmed.len() - 1], Some("m"))
    } else {
        (trimmed, None)
    };

    let multiplier = match suffix {
        Some("k" | "kb") => 1024u32,
        Some("m" | "mb") => 1024 * 1024u32,
        _ => 1u32,
    };

    let base = num_part
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("Invalid RAM size number '{num_part}'"))?;

    base.checked_mul(multiplier)
        .ok_or_else(|| format!("RAM size value '{value}' overflows u32"))
}

fn parse_cpu_type(value: &str) -> Result<m68k::CpuType, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "68000" | "m68000" => Ok(m68k::CpuType::M68000),
        "68010" | "m68010" => Ok(m68k::CpuType::M68010),
        "68ec020" | "m68ec020" => Ok(m68k::CpuType::M68EC020),
        "68020" | "m68020" => Ok(m68k::CpuType::M68020),
        "68ec030" | "m68ec030" => Ok(m68k::CpuType::M68EC030),
        "68030" | "m68030" => Ok(m68k::CpuType::M68030),
        "68ec040" | "m68ec040" => Ok(m68k::CpuType::M68EC040),
        "68lc040" | "m68lc040" => Ok(m68k::CpuType::M68LC040),
        "68040" | "m68040" => Ok(m68k::CpuType::M68040),
        _ => Err(format!(
            "Unsupported CPU type '{value}'. Supported: 68000, 68010, 68020, 68030, 68040"
        )),
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

fn print_usage(to_stdout: bool) {
    let msg = "Usage: rumiga-desktop [options] <kickstart.rom> [floppy1.adf] [floppy2.adf] ...\n\n\
               Options:\n  \
                 -m, --model <model>     Machine profile: a500, a500-plus, a600, a1200\n  \
                 -s, --scale <factor>    Window scale: 1, 2, 4, 8, 16, 32 [default: 1]\n  \
                     --viewport <mode>   Viewport mode: auto, raw [default: auto]\n  \
                     --no-vertical-stretch  Disable vertical line doubling\n  \
                     --floppy-speed <%>  Floppy read speed: 100%, 200%, 400%, 800%, turbo\n  \
                     --hdf <file.hdf>    Mount Gayle IDE virtual hardfile (.hdf)\n  \
                     --cpu <type>        Override CPU: 68000, 68010, 68020, 68030, 68040\n  \
                     --chip-ram <size>   Override Chip RAM size: e.g. 512K, 1M, 2M\n  \
                     --slow-ram <size>   Override Slow RAM size: e.g. 512K, 1M\n  \
                     --fast-ram <size>   Override Fast RAM size: e.g. 1M, 2M, 4M, 8M\n  \
                     --pal               Force PAL video timing\n  \
                     --ntsc              Force NTSC video timing\n  \
                     --df0 <file.adf>    Explicitly mount floppy in DF0\n  \
                     --df1 <file.adf>    Explicitly mount floppy in DF1\n  \
                     --df2 <file.adf>    Explicitly mount floppy in DF2\n  \
                     --df3 <file.adf>    Explicitly mount floppy in DF3\n  \
                     --trace-cpu <file>  Save assembly instruction trace to a file\n  \
                     --trace-limit <n>   Stop tracing after N instructions\n  \
                     --capture <file.png>  Run headless and save a PNG screenshot\n  \
                     --capture-frames <n>  Frames to run before capture [default: 300]\n  \
                     --capture-manifest <file.json>  Save capture evidence manifest";
    if to_stdout {
        println!("{msg}");
    } else {
        eprintln!("{msg}");
    }
}

fn capture_evidence(
    emulator: &mut Emulator,
    context: &CaptureEvidenceContext<'_>,
) -> Result<(), String> {
    for _ in 0..context.args.capture_frames {
        emulator.run_frame();
    }

    let frame = prepare_capture_frame(emulator.framebuffer(), context.args)?;
    let image_path = Path::new(context.capture_path);
    write_rgb565_png(image_path, &frame.pixels, frame.width, frame.height)?;

    let manifest_path = context
        .args
        .capture_manifest_path
        .as_deref()
        .map_or_else(|| default_manifest_path(image_path), PathBuf::from);
    let manifest_context = CaptureManifestContext {
        image_path,
        frame: &frame,
        args: context.args,
        model: context.model,
        config: context.config,
        emulator,
        rom: context.rom,
        floppies: context.floppies,
        hdf: context.hdf,
    };
    write_capture_manifest(&manifest_path, &manifest_context)?;

    eprintln!(
        "Captured {}x{} after {} frames: {}",
        frame.width,
        frame.height,
        context.args.capture_frames,
        image_path.display()
    );
    eprintln!("Capture manifest: {}", manifest_path.display());
    Ok(())
}

fn prepare_capture_frame(
    framebuffer: &[u16],
    launch_args: &LaunchArgs,
) -> Result<CaptureFrame, String> {
    if launch_args.vertical_stretch {
        let output_height = presented_height(true);
        let (source_y_start, source_y_end) = if launch_args.viewport_mode == ViewportMode::Auto {
            auto_vertical_bounds(framebuffer, WIDTH, HEIGHT).unwrap_or((0, HEIGHT))
        } else {
            (0, HEIGHT)
        };
        let mut pixels = vec![0u16; WIDTH * output_height];
        if !stretch_vertical_viewport(
            framebuffer,
            WIDTH,
            HEIGHT,
            source_y_start,
            source_y_end,
            output_height,
            &mut pixels,
        ) {
            return Err("Failed to prepare capture frame".to_owned());
        }

        Ok(CaptureFrame {
            pixels,
            width: WIDTH,
            height: output_height,
            source_y_start,
            source_y_end,
        })
    } else {
        let pixel_count = WIDTH * HEIGHT;
        if framebuffer.len() < pixel_count {
            return Err("Framebuffer is smaller than the visible display".to_owned());
        }

        Ok(CaptureFrame {
            pixels: framebuffer[..pixel_count].to_vec(),
            width: WIDTH,
            height: HEIGHT,
            source_y_start: 0,
            source_y_end: HEIGHT,
        })
    }
}

fn write_rgb565_png(
    path: &Path,
    pixels: &[u16],
    width: usize,
    height: usize,
) -> Result<(), String> {
    let expected_pixels = width
        .checked_mul(height)
        .ok_or_else(|| "Capture dimensions overflow".to_owned())?;
    if pixels.len() != expected_pixels {
        return Err(format!(
            "Capture buffer length mismatch: expected {expected_pixels}, got {}",
            pixels.len()
        ));
    }

    create_parent_dirs(path)?;
    let file = fs::File::create(path)
        .map_err(|e| format!("Failed to create PNG '{}': {e}", path.display()))?;
    let width = u32::try_from(width).map_err(|_| "Capture width exceeds u32".to_owned())?;
    let height = u32::try_from(height).map_err(|_| "Capture height exceeds u32".to_owned())?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder
        .write_header()
        .map_err(|e| format!("Failed to write PNG header '{}': {e}", path.display()))?;
    let mut rgb = Vec::with_capacity(expected_pixels * 3);
    for &pixel in pixels {
        let [r, g, b] = rgb565_to_rgb8(pixel);
        rgb.push(r);
        rgb.push(g);
        rgb.push(b);
    }
    writer
        .write_image_data(&rgb)
        .map_err(|e| format!("Failed to write PNG data '{}': {e}", path.display()))
}

fn write_capture_manifest(path: &Path, context: &CaptureManifestContext<'_>) -> Result<(), String> {
    create_parent_dirs(path)?;

    let mut json = String::new();
    let _ = writeln!(json, "{{");
    let _ = writeln!(
        json,
        "  \"image\": {},",
        json_string(&context.image_path.display().to_string())
    );
    let _ = writeln!(json, "  \"model\": {},", json_string(context.model.name()));
    let _ = writeln!(
        json,
        "  \"cpu\": {},",
        json_string(&format!("{:?}", context.config.cpu_type))
    );
    let _ = writeln!(
        json,
        "  \"video_standard\": {},",
        json_string(if context.args.ntsc { "ntsc" } else { "pal" })
    );
    let _ = writeln!(
        json,
        "  \"memory\": {{ \"chip_ram_bytes\": {}, \"slow_ram_bytes\": {}, \"fast_ram_bytes\": {}, \"rom_bytes\": {} }},",
        context.config.chip_ram_size,
        context.config.slow_ram_size,
        context.config.fast_ram_size,
        context.config.rom_size
    );
    let _ = writeln!(
        json,
        "  \"run\": {{ \"frames\": {}, \"total_cycles\": {}, \"pc\": {}, \"sr\": {}, \"stopped\": {}, \"trace_count\": {} }},",
        context.args.capture_frames,
        context.emulator.total_cycles,
        json_string(&format!("0x{:08X}", context.emulator.cpu.pc)),
        json_string(&format!("0x{:04X}", context.emulator.cpu.get_sr())),
        context.emulator.cpu.is_stopped(),
        context.emulator.trace_count
    );
    let _ = writeln!(
        json,
        "  \"viewport\": {{ \"mode\": {}, \"vertical_stretch\": {}, \"source_width\": {}, \"source_height\": {}, \"source_y_start\": {}, \"source_y_end\": {}, \"output_width\": {}, \"output_height\": {} }},",
        json_string(context.args.viewport_mode.name()),
        context.args.vertical_stretch,
        WIDTH,
        HEIGHT,
        context.frame.source_y_start,
        context.frame.source_y_end,
        context.frame.width,
        context.frame.height
    );
    let _ = writeln!(
        json,
        "  \"framebuffer\": {{ \"background_rgb565\": {}, \"pixels_different_from_background\": {}, \"non_zero_rgb565_pixels\": {}, \"distinct_colors\": {} }},",
        json_string(&rgb565_hex(first_pixel(&context.frame.pixels))),
        count_pixels_different_from_first(&context.frame.pixels),
        count_non_zero_pixels(&context.frame.pixels),
        count_distinct_colors(&context.frame.pixels)
    );
    push_floppy_state_json(&mut json, &context.emulator.floppy);
    json.push_str("  \"media\": {\n");
    push_file_evidence_json(&mut json, "rom", context.rom, "    ", true);
    for drive in 0..4 {
        let key = format!("df{drive}");
        if let Some(ref evidence) = context.floppies[drive] {
            push_file_evidence_json(&mut json, &key, evidence, "    ", true);
        } else {
            let _ = writeln!(json, "    {key:?}: null,");
        }
    }
    if let Some(hdf) = context.hdf {
        push_file_evidence_json(&mut json, "hdf", hdf, "    ", false);
    } else {
        json.push_str("    \"hdf\": null\n");
    }
    json.push_str("  }\n");
    json.push_str("}\n");

    fs::write(path, json).map_err(|e| format!("Failed to write manifest '{}': {e}", path.display()))
}

fn push_floppy_state_json(json: &mut String, floppy: &rumiga_core::floppy::FloppyController) {
    let _ = writeln!(json, "  \"floppy\": {{");
    let _ = writeln!(json, "    \"speed_percent\": {},", floppy.speed_percent());
    let _ = writeln!(
        json,
        "    \"selected_mask\": {},",
        json_string(&format!("0x{:02X}", floppy.selected))
    );
    let _ = writeln!(
        json,
        "    \"any_drive_selected\": {},",
        floppy.any_drive_selected()
    );
    let _ = writeln!(
        json,
        "    \"first_selected_drive\": {},",
        floppy.first_selected_drive()
    );
    let _ = writeln!(json, "    \"side\": {},", floppy.side);
    let _ = writeln!(json, "    \"direction\": {},", floppy.direction);
    let _ = writeln!(
        json,
        "    \"dma_state\": {},",
        json_string(&format!("{:?}", floppy.dma_state))
    );
    let _ = writeln!(
        json,
        "    \"dsklen\": {},",
        json_string(&format!("0x{:04X}", floppy.dsklen))
    );
    let _ = writeln!(json, "    \"dsk_length\": {},", floppy.dsk_length);
    let _ = writeln!(
        json,
        "    \"dskpt\": {},",
        json_string(&format!("0x{:08X}", floppy.dskpt))
    );
    let _ = writeln!(
        json,
        "    \"dsksync\": {},",
        json_string(&format!("0x{:04X}", floppy.dsksync))
    );
    let _ = writeln!(
        json,
        "    \"dskbytr\": {},",
        json_string(&format!("0x{:04X}", floppy.dskbytr_val))
    );
    let _ = writeln!(
        json,
        "    \"pending_sync_irq\": {},",
        floppy.pending_sync_irq
    );
    let _ = writeln!(json, "    \"pending_blk_irq\": {},", floppy.pending_blk_irq);
    json.push_str("    \"drives\": [\n");
    for (index, drive) in floppy.drives.iter().enumerate() {
        let comma = if index + 1 == floppy.drives.len() {
            ""
        } else {
            ","
        };
        let bytes = drive.data.as_ref().map_or(0, Vec::len);
        let _ = writeln!(
            json,
            "      {{ \"name\": {}, \"inserted\": {}, \"bytes\": {}, \"cylinder\": {}, \"motor\": {}, \"dskready\": {}, \"dskready_up_time\": {}, \"disk_changed\": {}, \"dirty\": {}, \"mfm_pos\": {}, \"mfm_track_words\": {} }}{comma}",
            json_string(&format!("DF{index}")),
            drive.data.is_some(),
            bytes,
            drive.cyl,
            drive.motor,
            drive.dskready,
            drive.dskready_up_time,
            drive.disk_changed,
            drive.dirty,
            drive.mfm_pos,
            drive.mfm_track.len()
        );
    }
    json.push_str("    ]\n");
    json.push_str("  },\n");
}

fn push_file_evidence_json(
    json: &mut String,
    key: &str,
    evidence: &FileEvidence,
    indent: &str,
    trailing_comma: bool,
) {
    let comma = if trailing_comma { "," } else { "" };
    let _ = writeln!(
        json,
        "{indent}{}: {{ \"path\": {}, \"bytes\": {}, \"sha256\": {} }}{comma}",
        json_string(key),
        json_string(&evidence.path),
        evidence.bytes,
        json_string(&evidence.sha256)
    );
}

fn create_parent_dirs(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory '{}': {e}", parent.display()))?;
        }
    }
    Ok(())
}

fn default_manifest_path(image_path: &Path) -> PathBuf {
    let mut path = image_path.to_path_buf();
    path.set_extension("json");
    path
}

fn file_evidence_from_bytes(path: &str, data: &[u8]) -> FileEvidence {
    FileEvidence {
        path: path.to_owned(),
        bytes: data.len(),
        sha256: sha256_hex(data),
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn rgb565_to_rgb8(pixel: u16) -> [u8; 3] {
    [
        expand_5_to_8((pixel >> 11) & 0x1F),
        expand_6_to_8((pixel >> 5) & 0x3F),
        expand_5_to_8(pixel & 0x1F),
    ]
}

fn expand_5_to_8(value: u16) -> u8 {
    u8::try_from((value * 255 + 15) / 31).unwrap_or(u8::MAX)
}

fn expand_6_to_8(value: u16) -> u8 {
    u8::try_from((value * 255 + 31) / 63).unwrap_or(u8::MAX)
}

fn first_pixel(pixels: &[u16]) -> u16 {
    pixels.first().copied().unwrap_or_default()
}

fn rgb565_hex(pixel: u16) -> String {
    format!("0x{pixel:04X}")
}

fn count_pixels_different_from_first(pixels: &[u16]) -> usize {
    let background = first_pixel(pixels);
    pixels.iter().filter(|&&pixel| pixel != background).count()
}

fn count_non_zero_pixels(pixels: &[u16]) -> usize {
    pixels.iter().filter(|&&pixel| pixel != 0).count()
}

fn count_distinct_colors(pixels: &[u16]) -> usize {
    let mut colors = Vec::new();
    for &pixel in pixels {
        if !colors.contains(&pixel) {
            colors.push(pixel);
        }
    }
    colors.len()
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => {
                let _ = write!(escaped, "\\u{:04x}", ch as u32);
            }
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
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

    fn default_test_args() -> LaunchArgs {
        LaunchArgs {
            model: None,
            scale: DEFAULT_SCALE,
            viewport_mode: ViewportMode::Auto,
            vertical_stretch: true,
            floppy_speed_percent: FLOPPY_SPEED_COMPATIBLE_PERCENT,
            rom_path: "kick.rom".to_owned(),
            adf_paths: Vec::new(),
            hdf_path: None,
            cpu: None,
            chip_ram: None,
            slow_ram: None,
            fast_ram: None,
            pal: false,
            ntsc: false,
            df0: None,
            df1: None,
            df2: None,
            df3: None,
            trace_cpu: None,
            trace_limit: None,
            capture_path: None,
            capture_manifest_path: None,
            capture_frames: DEFAULT_CAPTURE_FRAMES,
        }
    }

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
                adf_paths: vec!["workbench.adf".to_owned()],
                ..default_test_args()
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
                adf_paths: vec![
                    "df0.adf".to_owned(),
                    "df1.adf".to_owned(),
                    "df2.adf".to_owned(),
                    "df3.adf".to_owned(),
                ],
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_accepts_scale() {
        let args = vec!["--scale".to_owned(), "1".to_owned(), "kick.rom".to_owned()];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                scale: 1,
                ..default_test_args()
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
            rom_path: "kick.a1200.46.143.rom".to_owned(),
            ..default_test_args()
        };

        assert_eq!(select_model(&args, ROM_SIZE_512K), Ok(MachineModel::A1200));
    }

    #[test]
    fn select_model_rejects_wrong_rom_size() {
        let args = LaunchArgs {
            model: Some(MachineModel::A1200),
            rom_path: "kick.a500.34.005.rom".to_owned(),
            ..default_test_args()
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
                viewport_mode: ViewportMode::Raw,
                vertical_stretch: false,
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_accepts_floppy_speed() {
        let args = vec![
            "--floppy-speed".to_owned(),
            "800%".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                floppy_speed_percent: 800,
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_accepts_turbo_floppy_speed() {
        let args = vec![
            "--floppy-speed".to_owned(),
            "turbo".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                floppy_speed_percent: FLOPPY_SPEED_TURBO_PERCENT,
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_accepts_hdf() {
        let args = vec![
            "--hdf".to_owned(),
            "system.hdf".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                hdf_path: Some("system.hdf".to_owned()),
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_accepts_capture_options() {
        let args = vec![
            "--capture".to_owned(),
            "evidence/a1200.png".to_owned(),
            "--capture-frames".to_owned(),
            "1200".to_owned(),
            "--capture-manifest".to_owned(),
            "evidence/a1200.json".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                capture_path: Some("evidence/a1200.png".to_owned()),
                capture_manifest_path: Some("evidence/a1200.json".to_owned()),
                capture_frames: 1200,
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_rejects_manifest_without_capture() {
        let args = vec![
            "--capture-manifest".to_owned(),
            "evidence/a1200.json".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn parse_args_rejects_unsupported_floppy_speed() {
        let args = vec![
            "--floppy-speed".to_owned(),
            "300".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert!(parse_args(&args).is_err());
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

    #[test]
    fn prepare_capture_frame_uses_presented_height() {
        let framebuffer = vec![0xFFFFu16; WIDTH * HEIGHT];
        let frame =
            prepare_capture_frame(&framebuffer, &default_test_args()).expect("valid frame buffer");

        assert_eq!(frame.width, WIDTH);
        assert_eq!(frame.height, HEIGHT * 2);
        assert_eq!(frame.pixels.len(), WIDTH * HEIGHT * 2);
    }

    #[test]
    fn json_string_escapes_control_characters() {
        assert_eq!(json_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn test_parse_ram_size() {
        assert_eq!(parse_ram_size("512k"), Ok(512 * 1024));
        assert_eq!(parse_ram_size("1M"), Ok(1024 * 1024));
        assert_eq!(parse_ram_size("2MB"), Ok(2 * 1024 * 1024));
        assert_eq!(parse_ram_size("8mb"), Ok(8 * 1024 * 1024));
        assert_eq!(parse_ram_size("  256  KB  "), Ok(256 * 1024));
        assert_eq!(parse_ram_size("0"), Ok(0));
        assert!(parse_ram_size("").is_err());
        assert!(parse_ram_size("abc").is_err());
    }

    #[test]
    fn test_parse_cpu_type() {
        assert_eq!(parse_cpu_type("68000"), Ok(m68k::CpuType::M68000));
        assert_eq!(parse_cpu_type("m68020"), Ok(m68k::CpuType::M68020));
        assert_eq!(parse_cpu_type("68030"), Ok(m68k::CpuType::M68030));
        assert_eq!(parse_cpu_type("68040"), Ok(m68k::CpuType::M68040));
        assert!(parse_cpu_type("68060").is_err());
    }

    #[test]
    fn parse_args_accepts_overrides() {
        let args = vec![
            "--cpu".to_owned(),
            "68030".to_owned(),
            "--chip-ram".to_owned(),
            "2M".to_owned(),
            "--slow-ram".to_owned(),
            "512K".to_owned(),
            "--fast-ram".to_owned(),
            "4M".to_owned(),
            "--pal".to_owned(),
            "--df0".to_owned(),
            "disk0.adf".to_owned(),
            "--df1".to_owned(),
            "disk1.adf".to_owned(),
            "--trace-cpu".to_owned(),
            "trace.log".to_owned(),
            "--trace-limit".to_owned(),
            "5000".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                cpu: Some(m68k::CpuType::M68030),
                chip_ram: Some(2 * 1024 * 1024),
                slow_ram: Some(512 * 1024),
                fast_ram: Some(4 * 1024 * 1024),
                pal: true,
                df0: Some("disk0.adf".to_owned()),
                df1: Some("disk1.adf".to_owned()),
                trace_cpu: Some("trace.log".to_owned()),
                trace_limit: Some(5000),
                ..default_test_args()
            })
        );
    }

    #[test]
    fn test_parse_args_rejects_conflicting_video() {
        let args = vec![
            "--pal".to_owned(),
            "--ntsc".to_owned(),
            "kick.rom".to_owned(),
        ];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn test_parse_args_rejects_invalid_chip_ram() {
        // Invalid size (3MB)
        let args = vec![
            "--chip-ram".to_owned(),
            "3M".to_owned(),
            "kick.rom".to_owned(),
        ];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn test_parse_args_rejects_invalid_slow_ram() {
        // Not a multiple of 256KB (100KB)
        let args = vec![
            "--slow-ram".to_owned(),
            "100k".to_owned(),
            "kick.rom".to_owned(),
        ];
        assert!(parse_args(&args).is_err());

        // Too large (2MB)
        let args2 = vec![
            "--slow-ram".to_owned(),
            "2M".to_owned(),
            "kick.rom".to_owned(),
        ];
        assert!(parse_args(&args2).is_err());
    }

    #[test]
    fn test_parse_args_rejects_invalid_fast_ram() {
        // Not a multiple of 1MB (512KB)
        let args = vec![
            "--fast-ram".to_owned(),
            "512k".to_owned(),
            "kick.rom".to_owned(),
        ];
        assert!(parse_args(&args).is_err());

        // Too large (10MB)
        let args2 = vec![
            "--fast-ram".to_owned(),
            "10M".to_owned(),
            "kick.rom".to_owned(),
        ];
        assert!(parse_args(&args2).is_err());
    }

    #[test]
    fn test_parse_args_rejects_conflicting_floppies() {
        // DF0 maps explicitly via --df0 AND positionally via workbench.adf
        let args = vec![
            "--df0".to_owned(),
            "disk.adf".to_owned(),
            "kick.rom".to_owned(),
            "workbench.adf".to_owned(),
        ];
        assert!(parse_args(&args).is_err());
    }
}
