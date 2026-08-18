// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Rumiga desktop binary — development and debugging target.

mod network;
mod storage;

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use minifb::Key;
use network::DesktopNetworkBackend;
use rumiga_core::cia::CiaState;
use rumiga_core::custom;
use rumiga_core::emulator::{
    EARLY_VIDEO_SCANLINE_DUMP, Emulator, VIDEO_SCANLINE_WORD_DUMP, VideoScanlineSnapshot,
};
use rumiga_core::floppy::{
    FLOPPY_SPEED_COMPATIBLE_PERCENT, FLOPPY_SPEED_TURBO_PERCENT, is_supported_floppy_speed_percent,
};
use rumiga_core::memory::MemoryConfig;
use rumiga_core::playfield::{DISPLAY_HEIGHT, DISPLAY_LEFT_HPOS, DISPLAY_WIDTH, PlayfieldState};
use rumiga_platform::Clock;
use rumiga_platform::VideoOutput;
use rumiga_platform_desktop::{DesktopClock, DesktopVideo, FileTraceSink};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use storage::{
    DEFAULT_STORAGE_ROOT, DEFAULT_UPLOAD_LIMIT_BYTES, MAX_UPLOAD_LIMIT_BYTES, MediaStore,
    StorageError,
};

#[derive(rust_embed::RustEmbed)]
#[cfg_attr(test, folder = "test-assets/")]
#[cfg_attr(not(test), folder = "../web/out/")]
struct Asset;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
enum ApiCommand {
    Reset,
    Pause,
    Resume,
    InsertFloppy {
        drive_idx: usize,
        path: String,
        data: Vec<u8>,
    },
    EjectFloppy {
        drive_idx: usize,
    },
    UpdateAudioSeparation {
        separation: u8,
    },
    UpdateFloppySpeed {
        percent: u16,
    },
    UpdateNetwork {
        config: rumiga_api::NetworkConfig,
    },
}

struct SharedState {
    pub running: bool,
    pub fps: f32,
    pub model: String,
    pub chip_ram_kb: u32,
    pub slow_ram_kb: u32,
    pub fast_ram_kb: u32,
    pub rom_file: String,
    pub floppy: [Option<String>; 4],
    pub floppy_speed_percent: u16,
    pub hdf_path: Option<String>,
    pub hdf_write_policy: rumiga_api::HdfWritePolicy,
    pub network: rumiga_api::NetworkConfig,
    pub network_status: rumiga_api::NetworkStatus,
    pub stereo_separation: u8,
    pub display: rumiga_api::DisplayConfig,
    pub screenshot: Vec<u32>,
    pub screenshot_width: u32,
    pub screenshot_height: u32,
    pub native_screenshot: Vec<u32>,
    pub native_screenshot_width: u32,
    pub native_screenshot_height: u32,
    pub pending_commands: Vec<ApiCommand>,
}

#[derive(Clone)]
struct MachineState(Arc<Mutex<SharedState>>);

impl MachineState {
    fn lock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, SharedState>> {
        self.0.lock()
    }
}

#[derive(Clone)]
struct AppState {
    machine: MachineState,
    media_store: MediaStore,
}

impl axum::extract::FromRef<AppState> for MachineState {
    fn from_ref(state: &AppState) -> Self {
        state.machine.clone()
    }
}

impl axum::extract::FromRef<AppState> for MediaStore {
    fn from_ref(state: &AppState) -> Self {
        state.media_store.clone()
    }
}

fn get_mime_type(path: &str) -> &'static str {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if ext.eq_ignore_ascii_case("html") {
        "text/html; charset=utf-8"
    } else if ext.eq_ignore_ascii_case("css") {
        "text/css"
    } else if ext.eq_ignore_ascii_case("js") || ext.eq_ignore_ascii_case("mjs") {
        "application/javascript"
    } else if ext.eq_ignore_ascii_case("json") {
        "application/json"
    } else if ext.eq_ignore_ascii_case("png") {
        "image/png"
    } else if ext.eq_ignore_ascii_case("svg") {
        "image/svg+xml"
    } else if ext.eq_ignore_ascii_case("ico") {
        "image/x-icon"
    } else {
        "application/octet-stream"
    }
}

async fn static_handler(
    State(_state): State<MachineState>,
    req: axum::http::Request<axum::body::Body>,
) -> impl axum::response::IntoResponse {
    let path = req.uri().path().trim_start_matches('/');
    let asset_path = if path.is_empty() {
        "index.html".to_string()
    } else if path.ends_with('/') {
        format!("{path}index.html")
    } else {
        path.to_string()
    };

    let file = if let Some(content) = Asset::get(&asset_path) {
        Some((asset_path, content))
    } else if let Some(content) = Asset::get(&format!("{}.html", asset_path.trim_end_matches('/')))
    {
        Some((
            format!("{}.html", asset_path.trim_end_matches('/')),
            content,
        ))
    } else {
        Asset::get("index.html").map(|content| ("index.html".to_string(), content))
    };

    if let Some((resolved_path, data)) = file {
        let mime_type = get_mime_type(&resolved_path);
        axum::response::Response::builder()
            .header(axum::http::header::CONTENT_TYPE, mime_type)
            .body(axum::body::Body::from(data.data))
            .unwrap()
    } else {
        axum::response::Response::builder()
            .status(axum::http::StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("404 Not Found"))
            .unwrap()
    }
}

async fn get_status(State(state): State<MachineState>) -> axum::response::Json<serde_json::Value> {
    let s = state.lock().unwrap();
    let model_enum = api_model_from_name(&s.model);
    axum::response::Json(serde_json::json!(rumiga_api::ApiResponse::ok(
        rumiga_api::MachineStatus {
            running: s.running,
            fps: s.fps,
            model: model_enum,
            network: s.network_status.clone(),
        }
    )))
}

async fn get_config(State(state): State<MachineState>) -> axum::response::Json<serde_json::Value> {
    let config = {
        let s = state.lock().unwrap();
        let model_enum = api_model_from_name(&s.model);
        rumiga_api::MachineConfig {
            model: model_enum,
            chip_ram_kb: s.chip_ram_kb,
            slow_ram_kb: s.slow_ram_kb,
            fast_ram_kb: s.fast_ram_kb,
            rom_file: s.rom_file.clone(),
            floppy: s.floppy.clone(),
            floppy_speed_percent: s.floppy_speed_percent,
            hdf_path: s.hdf_path.clone(),
            hdf_write_policy: s.hdf_write_policy,
            network: s.network.clone(),
            audio: rumiga_api::AudioConfig {
                channel_mix: [
                    rumiga_api::ChannelMixConfig {
                        left_pct: 100,
                        right_pct: 0,
                    },
                    rumiga_api::ChannelMixConfig {
                        left_pct: 0,
                        right_pct: 100,
                    },
                    rumiga_api::ChannelMixConfig {
                        left_pct: 0,
                        right_pct: 100,
                    },
                    rumiga_api::ChannelMixConfig {
                        left_pct: 100,
                        right_pct: 0,
                    },
                ],
                stereo_separation: s.stereo_separation,
            },
            display: s.display.clone(),
        }
    };
    axum::response::Json(serde_json::json!(rumiga_api::ApiResponse::ok(config)))
}

async fn get_support_bundle(
    State(state): State<MachineState>,
) -> axum::response::Json<serde_json::Value> {
    let bundle = {
        let s = state.lock().unwrap();
        support_bundle_from_state(&s)
    };
    axum::response::Json(serde_json::json!(rumiga_api::ApiResponse::ok(bundle)))
}

fn api_model_from_name(model: &str) -> rumiga_api::AmigaModel {
    match model {
        "a1200" => rumiga_api::AmigaModel::A1200,
        "a500-plus" | "a600" => rumiga_api::AmigaModel::A500Plus,
        _ => rumiga_api::AmigaModel::A500,
    }
}

fn support_bundle_from_state(s: &SharedState) -> rumiga_api::SupportBundle {
    let model = api_model_from_name(&s.model);
    let screenshot_available =
        !s.screenshot.is_empty() && s.screenshot_width > 0 && s.screenshot_height > 0;
    let native_screenshot_available = !s.native_screenshot.is_empty()
        && s.native_screenshot_width > 0
        && s.native_screenshot_height > 0;
    let status = rumiga_api::MachineStatus {
        running: s.running,
        fps: s.fps,
        model: model.clone(),
        network: s.network_status.clone(),
    };

    rumiga_api::SupportBundle {
        schema: SUPPORT_BUNDLE_SCHEMA_ID.to_string(),
        machine: rumiga_api::SupportMachineSummary {
            model,
            chip_ram_kb: s.chip_ram_kb,
            slow_ram_kb: s.slow_ram_kb,
            fast_ram_kb: s.fast_ram_kb,
            floppy_speed_percent: s.floppy_speed_percent,
            hdf_write_policy: s.hdf_write_policy,
        },
        status,
        display: s.display.clone(),
        media: rumiga_api::SupportMediaSummary {
            rom_name: support_file_name(&s.rom_file),
            hdf_name: s.hdf_path.as_deref().and_then(support_file_name),
            floppies: std::array::from_fn(|index| {
                s.floppy[index].as_deref().and_then(support_file_name)
            }),
        },
        screenshot: rumiga_api::SupportScreenshotSummary {
            available: screenshot_available,
            kind: rumiga_api::ScreenshotKind::ViewportPresentation,
            width: if screenshot_available {
                s.screenshot_width
            } else {
                0
            },
            height: if screenshot_available {
                s.screenshot_height
            } else {
                0
            },
            endpoint: screenshot_endpoint_for_kind(
                &rumiga_api::ScreenshotKind::ViewportPresentation,
            ),
            pixel_format: "rgba8888-png".to_string(),
            available_kinds: vec![
                rumiga_api::ScreenshotKind::ViewportPresentation,
                rumiga_api::ScreenshotKind::NativeFramebuffer,
            ],
            native_width: if native_screenshot_available {
                s.native_screenshot_width
            } else {
                0
            },
            native_height: if native_screenshot_available {
                s.native_screenshot_height
            } else {
                0
            },
            presentation_width: if screenshot_available {
                s.screenshot_width
            } else {
                0
            },
            presentation_height: if screenshot_available {
                s.screenshot_height
            } else {
                0
            },
        },
        notes: vec![
            "Media paths are redacted to file names; ROM/HDF/ADF bytes are not included."
                .to_string(),
            "Use the screenshot endpoint separately when a visual artifact is needed.".to_string(),
        ],
    }
}

fn support_file_name(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let name = trimmed.rsplit(['/', '\\']).find(|part| !part.is_empty())?;
    Some(name.to_string())
}

async fn put_config(
    State(state): State<MachineState>,
    axum::Json(payload): axum::Json<rumiga_api::MachineConfig>,
) -> axum::response::Json<serde_json::Value> {
    if !is_supported_floppy_speed_percent(payload.floppy_speed_percent) {
        return axum::response::Json(serde_json::json!(
            rumiga_api::ApiResponse::<()>::err_with_code(
                "invalid_floppy_speed",
                "Unsupported floppy speed".to_string()
            )
        ));
    }
    if payload.audio.stereo_separation > 100 {
        return axum::response::Json(serde_json::json!(
            rumiga_api::ApiResponse::<()>::err_with_code(
                "invalid_audio_separation",
                "Stereo separation must be 0-100".to_string()
            )
        ));
    }
    if payload.display.viewport.width == 0 || payload.display.viewport.height == 0 {
        return axum::response::Json(serde_json::json!(
            rumiga_api::ApiResponse::<()>::err_with_code(
                "invalid_viewport",
                "Viewport width and height must be greater than zero".to_string()
            )
        ));
    }
    if payload.network.backend != rumiga_api::NetworkBackend::Disabled
        && !rumiga_api::is_valid_unicast_mac_address(&payload.network.mac_address)
    {
        return axum::response::Json(serde_json::json!(
            rumiga_api::ApiResponse::<()>::err_with_code(
                "invalid_network_mac",
                "Network MAC address must be a unicast address like 02:52:55:4d:49:47".to_string()
            )
        ));
    }

    let mut s = state.lock().unwrap();
    if s.floppy_speed_percent != payload.floppy_speed_percent {
        s.pending_commands.push(ApiCommand::UpdateFloppySpeed {
            percent: payload.floppy_speed_percent,
        });
    }
    if s.stereo_separation != payload.audio.stereo_separation {
        s.pending_commands.push(ApiCommand::UpdateAudioSeparation {
            separation: payload.audio.stereo_separation,
        });
    }
    if s.network != payload.network {
        s.pending_commands.push(ApiCommand::UpdateNetwork {
            config: payload.network.clone(),
        });
    }

    s.chip_ram_kb = payload.chip_ram_kb;
    s.slow_ram_kb = payload.slow_ram_kb;
    s.fast_ram_kb = payload.fast_ram_kb;
    s.rom_file = payload.rom_file;
    s.floppy = payload.floppy;
    s.floppy_speed_percent = payload.floppy_speed_percent;
    s.hdf_path = payload.hdf_path;
    s.hdf_write_policy = payload.hdf_write_policy;
    s.network = payload.network;
    s.stereo_separation = payload.audio.stereo_separation;
    s.display = payload.display;
    drop(s);

    axum::response::Json(serde_json::json!(rumiga_api::ApiResponse::<()>::ok(())))
}

async fn post_reset(State(state): State<MachineState>) -> axum::response::Json<serde_json::Value> {
    state
        .lock()
        .unwrap()
        .pending_commands
        .push(ApiCommand::Reset);
    axum::response::Json(serde_json::json!(rumiga_api::ApiResponse::<()>::ok(())))
}

async fn post_pause(State(state): State<MachineState>) -> axum::response::Json<serde_json::Value> {
    state
        .lock()
        .unwrap()
        .pending_commands
        .push(ApiCommand::Pause);
    axum::response::Json(serde_json::json!(rumiga_api::ApiResponse::<()>::ok(())))
}

async fn post_resume(State(state): State<MachineState>) -> axum::response::Json<serde_json::Value> {
    state
        .lock()
        .unwrap()
        .pending_commands
        .push(ApiCommand::Resume);
    axum::response::Json(serde_json::json!(rumiga_api::ApiResponse::<()>::ok(())))
}

async fn post_floppy_insert(
    State(state): State<AppState>,
    axum::Json(payload): axum::Json<rumiga_api::FloppyInsertRequest>,
) -> ApiJsonResponse {
    if payload.drive_idx >= 4 {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_drive_index",
            "Invalid drive index",
        );
    }
    let (path, data) = match state.media_store.read_file(&payload.path, &["adf"]).await {
        Ok(media) => media,
        Err(error) => return storage_error_response(&error),
    };
    state
        .machine
        .lock()
        .unwrap()
        .pending_commands
        .push(ApiCommand::InsertFloppy {
            drive_idx: payload.drive_idx,
            path: path.to_string_lossy().into_owned(),
            data,
        });
    api_ok_response(())
}

async fn post_floppy_eject(
    State(state): State<MachineState>,
    axum::Json(payload): axum::Json<rumiga_api::FloppyEjectRequest>,
) -> axum::response::Json<serde_json::Value> {
    if payload.drive_idx >= 4 {
        return axum::response::Json(serde_json::json!(
            rumiga_api::ApiResponse::<()>::err_with_code(
                "invalid_drive_index",
                "Invalid drive index".to_string()
            )
        ));
    }
    state
        .lock()
        .unwrap()
        .pending_commands
        .push(ApiCommand::EjectFloppy {
            drive_idx: payload.drive_idx,
        });
    axum::response::Json(serde_json::json!(rumiga_api::ApiResponse::<()>::ok(())))
}

async fn post_audio_separation(
    State(state): State<MachineState>,
    axum::Json(payload): axum::Json<rumiga_api::AudioSeparationRequest>,
) -> axum::response::Json<serde_json::Value> {
    if payload.separation > 100 {
        return axum::response::Json(serde_json::json!(
            rumiga_api::ApiResponse::<()>::err_with_code(
                "invalid_audio_separation",
                "Separation must be 0-100".to_string()
            )
        ));
    }
    state
        .lock()
        .unwrap()
        .pending_commands
        .push(ApiCommand::UpdateAudioSeparation {
            separation: payload.separation,
        });
    axum::response::Json(serde_json::json!(rumiga_api::ApiResponse::<()>::ok(())))
}

async fn get_screenshot(
    State(state): State<MachineState>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> axum::response::Response<axum::body::Body> {
    let kind = match query.get("kind") {
        Some(value) => match parse_screenshot_kind(value) {
            Ok(kind) => kind,
            Err(message) => {
                return axum::response::Response::builder()
                    .status(axum::http::StatusCode::BAD_REQUEST)
                    .body(axum::body::Body::from(message))
                    .unwrap();
            }
        },
        None => rumiga_api::ScreenshotKind::default(),
    };
    let (pixels, width, height) = {
        let s = state.lock().unwrap();
        screenshot_buffer_for_kind(&s, &kind)
    };
    if pixels.is_empty() || width == 0 || height == 0 {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::NO_CONTENT)
            .body(axum::body::Body::from("No screenshot available"))
            .unwrap();
    }

    let png_bytes = match encode_argb_png(&pixels, width, height) {
        Ok(bytes) => bytes,
        Err(message) => {
            return axum::response::Response::builder()
                .status(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from(message))
                .unwrap();
        }
    };

    axum::response::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "image/png")
        .header(
            axum::http::header::CACHE_CONTROL,
            "no-store, must-revalidate",
        )
        .body(axum::body::Body::from(png_bytes))
        .unwrap()
}

fn screenshot_buffer_for_kind(
    s: &SharedState,
    kind: &rumiga_api::ScreenshotKind,
) -> (Vec<u32>, u32, u32) {
    match kind {
        rumiga_api::ScreenshotKind::NativeFramebuffer => (
            s.native_screenshot.clone(),
            s.native_screenshot_width,
            s.native_screenshot_height,
        ),
        rumiga_api::ScreenshotKind::ViewportPresentation => (
            s.screenshot.clone(),
            s.screenshot_width,
            s.screenshot_height,
        ),
    }
}

fn copy_rgb565_to_argb(source: &[u16], destination: &mut Vec<u32>) {
    destination.resize(source.len(), 0);
    for (index, &pixel) in source.iter().enumerate() {
        destination[index] = rumiga_platform_desktop::rgb565_to_argb(pixel);
    }
}

const fn screenshot_kind_label(kind: &rumiga_api::ScreenshotKind) -> &'static str {
    match kind {
        rumiga_api::ScreenshotKind::NativeFramebuffer => "native-framebuffer",
        rumiga_api::ScreenshotKind::ViewportPresentation => "viewport-presentation",
    }
}

fn screenshot_endpoint_for_kind(kind: &rumiga_api::ScreenshotKind) -> String {
    format!(
        "{}?kind={}",
        rumiga_api::MACHINE_SCREENSHOT_PATH,
        screenshot_kind_label(kind)
    )
}

fn encode_argb_png(pixels: &[u32], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let expected_pixels = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .ok_or_else(|| "Screenshot dimensions overflow".to_owned())?;
    if pixels.len() != expected_pixels {
        return Err(format!(
            "Screenshot buffer length mismatch: expected {expected_pixels}, got {}",
            pixels.len()
        ));
    }

    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("Failed to write screenshot PNG header: {e}"))?;
        let mut rgba_bytes = vec![0u8; pixels.len() * 4];
        for (index, &argb) in pixels.iter().enumerate() {
            let base = index * 4;
            rgba_bytes[base] = ((argb >> 16) & 0xFF) as u8;
            rgba_bytes[base + 1] = ((argb >> 8) & 0xFF) as u8;
            rgba_bytes[base + 2] = (argb & 0xFF) as u8;
            rgba_bytes[base + 3] = ((argb >> 24) & 0xFF) as u8;
        }
        writer
            .write_image_data(&rgba_bytes)
            .map_err(|e| format!("Failed to write screenshot PNG data: {e}"))?;
    }
    Ok(png_bytes)
}

type ApiJsonResponse = (StatusCode, Json<serde_json::Value>);

fn api_ok_response<T: serde::Serialize>(value: T) -> ApiJsonResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!(rumiga_api::ApiResponse::ok(value))),
    )
}

fn api_error_response(
    status: StatusCode,
    code: &'static str,
    message: impl Into<String>,
) -> ApiJsonResponse {
    (
        status,
        Json(serde_json::json!(
            rumiga_api::ApiResponse::<()>::err_with_code(code, message.into())
        )),
    )
}

fn storage_error_response(error: &StorageError) -> ApiJsonResponse {
    eprintln!("Storage API error: {error}");
    match error {
        StorageError::InvalidPath => api_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_storage_path",
            "Path must be relative to the configured storage root",
        ),
        StorageError::AccessDenied => api_error_response(
            StatusCode::FORBIDDEN,
            "storage_access_denied",
            "Path is outside the configured storage root",
        ),
        StorageError::NotFound => api_error_response(
            StatusCode::NOT_FOUND,
            "storage_entry_not_found",
            "Storage entry was not found",
        ),
        StorageError::NotDirectory => api_error_response(
            StatusCode::BAD_REQUEST,
            "storage_entry_not_directory",
            "Storage entry is not a directory",
        ),
        StorageError::NotFile => api_error_response(
            StatusCode::BAD_REQUEST,
            "storage_entry_not_file",
            "Storage entry is not a regular file",
        ),
        StorageError::UnsupportedMediaType => api_error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Supported file types are ROM, ADF, ADZ, and HDF",
        ),
        StorageError::AlreadyExists => api_error_response(
            StatusCode::CONFLICT,
            "storage_entry_exists",
            "A storage entry with that name already exists",
        ),
        StorageError::UploadTooLarge { limit_bytes } => api_error_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            "upload_too_large",
            format!("Upload exceeds the configured {limit_bytes}-byte limit"),
        ),
        StorageError::InvalidConfiguration | StorageError::Io(_) => api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_io_error",
            "The storage operation failed",
        ),
    }
}

async fn get_files(
    State(media_store): State<MediaStore>,
    axum::extract::Query(req): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiJsonResponse {
    let virtual_path = req.get("path").map_or("/", String::as_str);
    match media_store.list(virtual_path) {
        Ok(listing) => api_ok_response(listing),
        Err(error) => storage_error_response(&error),
    }
}

async fn post_upload(
    State(media_store): State<MediaStore>,
    mut multipart: axum::extract::Multipart,
) -> ApiJsonResponse {
    let mut field = match multipart.next_field().await {
        Ok(Some(field)) => field,
        Ok(None) => {
            return api_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_upload",
                "Multipart upload must contain one file field",
            );
        }
        Err(error) => {
            eprintln!("Invalid multipart upload: {error}");
            return api_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_upload",
                "Malformed multipart upload",
            );
        }
    };
    if field.name() != Some("file") {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_upload",
            "Multipart upload field must be named file",
        );
    }
    let Some(file_name) = field.file_name().map(ToOwned::to_owned) else {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_upload",
            "Multipart upload is missing a file name",
        );
    };
    let mut upload = match media_store.begin_upload(&file_name).await {
        Ok(upload) => upload,
        Err(error) => return storage_error_response(&error),
    };
    loop {
        match field.chunk().await {
            Ok(Some(chunk)) => {
                if let Err(error) = upload.write_chunk(&chunk).await {
                    return storage_error_response(&error);
                }
            }
            Ok(None) => break,
            Err(error) => {
                eprintln!("Multipart upload stream failed: {error}");
                return api_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_upload",
                    "Malformed multipart upload",
                );
            }
        }
    }
    match multipart.next_field().await {
        Ok(None) => {}
        Ok(Some(_)) => {
            return api_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_upload",
                "Only one file may be uploaded per request",
            );
        }
        Err(error) => {
            eprintln!("Invalid multipart upload trailer: {error}");
            return api_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_upload",
                "Malformed multipart upload",
            );
        }
    }

    match upload.commit().await {
        Ok(bytes) => {
            eprintln!("Uploaded {bytes} bytes to {file_name}");
            api_ok_response(())
        }
        Err(error) => storage_error_response(&error),
    }
}

async fn post_format(
    axum::Json(payload): axum::Json<rumiga_api::FormatRequest>,
) -> axum::response::Json<serde_json::Value> {
    if payload.confirm_token != "CONFIRM" {
        return axum::response::Json(serde_json::json!(
            rumiga_api::ApiResponse::<()>::err_with_code(
                "invalid_confirm_token",
                "Format confirmation token must be CONFIRM".to_string()
            )
        ));
    }

    axum::response::Json(serde_json::json!(
        rumiga_api::ApiResponse::<()>::err_with_code(
            "unsupported_on_desktop",
            "Formatting removable media is not available on the desktop target".to_string()
        )
    ))
}

async fn delete_file_handler(
    State(media_store): State<MediaStore>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> ApiJsonResponse {
    match media_store.delete_file(&name) {
        Ok(()) => api_ok_response(()),
        Err(error) => storage_error_response(&error),
    }
}

async fn get_wifi_status() -> axum::response::Json<serde_json::Value> {
    axum::response::Json(serde_json::json!(rumiga_api::ApiResponse::ok(
        rumiga_api::WifiStatus {
            connected: true,
            ssid: Some("RumigaHostWiFi".to_string()),
            ip: Some("192.168.1.100".to_string()),
            mode: rumiga_api::WifiMode::Client,
        }
    )))
}

async fn post_wifi_scan() -> axum::response::Json<serde_json::Value> {
    axum::response::Json(serde_json::json!(rumiga_api::ApiResponse::ok(
        rumiga_api::WifiScanResponse {
            networks: vec![
                rumiga_api::WifiNetwork {
                    ssid: "RumigaHostWiFi".to_string(),
                    rssi: -45,
                    secured: true
                },
                rumiga_api::WifiNetwork {
                    ssid: "GuestNetwork".to_string(),
                    rssi: -70,
                    secured: true
                },
            ]
        }
    )))
}

async fn post_wifi_connect() -> axum::response::Json<serde_json::Value> {
    axum::response::Json(serde_json::json!(rumiga_api::ApiResponse::<()>::ok(())))
}

const WIDTH: usize = DISPLAY_WIDTH as usize;
const HEIGHT: usize = DISPLAY_HEIGHT as usize;
const DEFAULT_SCALE: usize = 1;
/// Window over which the reported frame rate is measured.
const FPS_SAMPLE_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

const DEFAULT_CAPTURE_FRAMES: u64 = 300;
const VERTICAL_STRETCH_FACTOR: usize = 2;
const ROM_SIZE_256K: usize = 256 * 1024;
const ROM_SIZE_512K: usize = 512 * 1024;
const CAPTURE_MANIFEST_SCHEMA_ID: &str = "rumiga.capture.v1";
const CAPTURE_MANIFEST_SCHEMA_VERSION: u16 = 1;
const EDGE_INSPECTION_LINES: usize = 20;
const EDGE_INSPECTION_WIDTH: usize = 16;
const SUPPORT_BUNDLE_SCHEMA_ID: &str = "rumiga.support.v1";
#[cfg(test)]
const DESKTOP_API_ENDPOINTS: &[rumiga_api::ApiEndpoint] = &[
    rumiga_api::ApiEndpoint::new(
        "GET",
        rumiga_api::MACHINE_STATUS_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "GET",
        rumiga_api::MACHINE_CONFIG_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "PUT",
        rumiga_api::MACHINE_CONFIG_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "GET",
        rumiga_api::MACHINE_SUPPORT_BUNDLE_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "POST",
        rumiga_api::MACHINE_RESET_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "POST",
        rumiga_api::MACHINE_PAUSE_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "POST",
        rumiga_api::MACHINE_RESUME_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "POST",
        rumiga_api::MACHINE_START_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "POST",
        rumiga_api::MACHINE_STOP_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "POST",
        rumiga_api::MACHINE_FLOPPY_INSERT_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "POST",
        rumiga_api::MACHINE_FLOPPY_EJECT_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "POST",
        rumiga_api::MACHINE_AUDIO_SEPARATION_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "GET",
        rumiga_api::MACHINE_SCREENSHOT_PATH,
        rumiga_api::ApiResponseFormat::Png,
    ),
    rumiga_api::ApiEndpoint::new(
        "GET",
        rumiga_api::FILES_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "POST",
        rumiga_api::FILES_UPLOAD_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "DELETE",
        rumiga_api::FILES_DELETE_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "POST",
        rumiga_api::FILES_FORMAT_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "GET",
        rumiga_api::WIFI_STATUS_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "POST",
        rumiga_api::WIFI_SCAN_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
    rumiga_api::ApiEndpoint::new(
        "POST",
        rumiga_api::WIFI_CONNECT_PATH,
        rumiga_api::ApiResponseFormat::Json,
    ),
];

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
            Self::A500Plus => MemoryConfig::a500_plus(),
            Self::A600 => MemoryConfig::a600(),
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
    NativeFullBorder,
    VisibleArea,
    Overscan,
    AutoCenter,
}

impl ViewportMode {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "raw" => Some(Self::Raw),
            "native-full-border" | "native_full_border" | "full-border" => {
                Some(Self::NativeFullBorder)
            }
            "visible-area" | "visible_area" | "visible" => Some(Self::VisibleArea),
            "overscan" => Some(Self::Overscan),
            "auto-center" | "auto_center" | "center" => Some(Self::AutoCenter),
            _ => None,
        }
    }

    const fn api_mode(self) -> rumiga_api::ViewportMode {
        match self {
            Self::Raw | Self::NativeFullBorder | Self::Overscan => rumiga_api::ViewportMode::Raw,
            Self::Auto | Self::VisibleArea | Self::AutoCenter => rumiga_api::ViewportMode::Auto,
        }
    }

    const fn api_preset(self) -> rumiga_api::ViewportPreset {
        match self {
            Self::Raw | Self::NativeFullBorder => rumiga_api::ViewportPreset::NativeFullBorder,
            Self::VisibleArea => rumiga_api::ViewportPreset::VisibleArea,
            Self::Overscan => rumiga_api::ViewportPreset::Overscan,
            Self::Auto | Self::AutoCenter => rumiga_api::ViewportPreset::AutoCenter,
        }
    }
}

#[derive(Debug, PartialEq)]
struct LaunchArgs {
    model: Option<MachineModel>,
    scale: usize,
    scaling_mode: rumiga_api::ScalingMode,
    viewport_mode: ViewportMode,
    vertical_stretch: bool,
    floppy_speed_percent: u16,
    rom_path: String,
    adf_paths: Vec<String>,
    hdf_path: Option<String>,
    hdf_write_policy: rumiga_api::HdfWritePolicy,
    hdf_snapshot_path: Option<String>,
    storage_root: Option<PathBuf>,
    upload_limit_bytes: u64,
    network: rumiga_api::NetworkConfig,
    network_pcap_path: Option<String>,
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
    capture_kind: rumiga_api::ScreenshotKind,
    mouse_scale_x: f32,
    mouse_scale_y: f32,
    audio_separation: u8,
}

#[derive(Clone, Debug)]
struct FileEvidence {
    path: String,
    bytes: usize,
    sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HdfSnapshotEvidence {
    path: String,
    bytes: usize,
    sha256: String,
    source_sha256: String,
    dirty: bool,
    changed_bytes: usize,
    changed_sectors: usize,
    sector_size: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HdfDiffStats {
    changed_bytes: usize,
    changed_sectors: usize,
    sector_size: usize,
}

struct CaptureFrame {
    pixels: Vec<u16>,
    width: usize,
    height: usize,
    source_x_start: usize,
    source_x_end: usize,
    source_y_start: usize,
    source_y_end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EdgeInspection {
    first_lines: usize,
    edge_width: usize,
    sampled_lines: usize,
    background_rgb565: u16,
    left_non_background_pixels: usize,
    right_non_background_pixels: usize,
    mirrored_non_background_pixels: usize,
    right_edge_wrapped_to_left_pixels: usize,
    left_edge_wrapped_to_right_pixels: usize,
    content_line_count: usize,
    min_content_width: usize,
    max_content_width: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ViewportRect {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl ViewportRect {
    const fn full_frame() -> Self {
        Self {
            x: 0,
            y: 0,
            width: WIDTH,
            height: HEIGHT,
        }
    }

    const fn x_end(self) -> usize {
        self.x + self.width
    }

    const fn y_end(self) -> usize {
        self.y + self.height
    }
}

struct CaptureEvidenceContext<'a> {
    args: &'a LaunchArgs,
    display: &'a rumiga_api::DisplayConfig,
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
    display: &'a rumiga_api::DisplayConfig,
    model: MachineModel,
    config: &'a MemoryConfig,
    emulator: &'a Emulator,
    rom: &'a FileEvidence,
    floppies: &'a [Option<FileEvidence>; 4],
    hdf: Option<&'a FileEvidence>,
    hdf_snapshot: Option<&'a HdfSnapshotEvidence>,
}

#[allow(
    clippy::cognitive_complexity,
    clippy::too_many_lines,
    clippy::similar_names
)]
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
    eprintln!("  Mouse Scale X:  {}", launch_args.mouse_scale_x);
    eprintln!("  Mouse Scale Y:  {}", launch_args.mouse_scale_y);
    eprintln!(
        "  Network:        {} ({}, MAC {})",
        if launch_args.network.enabled() {
            "enabled"
        } else {
            "disabled"
        },
        launch_args.network.backend.as_str(),
        launch_args.network.mac_address
    );
    if let Some(ref pcap_path) = launch_args.network_pcap_path {
        eprintln!("  Network PCAP:   {pcap_path}");
    }
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
    let mut network_backend = DesktopNetworkBackend::new();
    if let Some(ref pcap_path) = launch_args.network_pcap_path {
        if let Err(e) = network_backend.enable_pcap(Path::new(pcap_path)) {
            eprintln!("{e}");
            process::exit(1);
        }
    }
    if let Err(e) = network_backend.configure(&launch_args.network, &mut emulator) {
        eprintln!("Failed to initialize network backend: {e}");
        process::exit(1);
    }
    if let Some(ref trace_path) = launch_args.trace_cpu {
        match FileTraceSink::create(trace_path) {
            Ok(sink) => emulator.set_trace_sink(Box::new(sink), launch_args.trace_limit),
            Err(e) => {
                eprintln!("Failed to enable CPU tracing to '{trace_path}': {e}");
                process::exit(1);
            }
        }
    }
    emulator.set_floppy_speed_percent(launch_args.floppy_speed_percent);
    emulator
        .audio
        .apply_separation(launch_args.audio_separation);
    emulator.load_rom(&rom_data);
    let display_config = display_config_from_launch_args(&launch_args);

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
            "Mounted Gayle IDE HDF: {hdf_path} ({} bytes, {} policy)",
            hdf_data.len(),
            launch_args.hdf_write_policy.as_str()
        );
        emulator.insert_hdf(hdf_data);
    }

    if let Some(ref capture_path) = launch_args.capture_path {
        if let Err(e) = capture_evidence(
            &mut emulator,
            &mut network_backend,
            &CaptureEvidenceContext {
                args: &launch_args,
                model,
                config: &config_summary,
                rom: &rom_evidence,
                floppies: &floppy_evidence,
                hdf: hdf_evidence.as_ref(),
                capture_path,
                display: &display_config,
            },
        ) {
            eprintln!("Capture failed: {e}");
            process::exit(1);
        }
        return;
    }

    let storage_root = launch_args
        .storage_root
        .clone()
        .or_else(|| std::env::var_os("RUMIGA_STORAGE_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_STORAGE_ROOT));
    let media_store = MediaStore::new(&storage_root, launch_args.upload_limit_bytes)
        .unwrap_or_else(|error| {
            eprintln!(
                "Failed to initialize media storage at '{}': {error}",
                storage_root.display()
            );
            process::exit(1);
        });
    eprintln!(
        "Media storage root: {} (upload limit: {} bytes)",
        media_store.root().display(),
        media_store.upload_limit_bytes()
    );

    let initial_rect = resolve_viewport_rect(&display_config, None);
    let initial_height = presented_height(initial_rect, display_config.viewport.vertical_stretch);
    let initial_network_status = network_status_from_emulator(&launch_args.network, &emulator);

    let shared_state = Arc::new(Mutex::new(SharedState {
        running: true,
        fps: 50.0,
        model: model.name().to_string(),
        chip_ram_kb: config_summary.chip_ram_size / 1024,
        slow_ram_kb: config_summary.slow_ram_size / 1024,
        fast_ram_kb: config_summary.fast_ram_size / 1024,
        rom_file: launch_args.rom_path.clone(),
        floppy: floppy_paths.clone(),
        floppy_speed_percent: launch_args.floppy_speed_percent,
        hdf_path: launch_args.hdf_path.clone(),
        hdf_write_policy: launch_args.hdf_write_policy,
        network: launch_args.network.clone(),
        network_status: initial_network_status,
        stereo_separation: launch_args.audio_separation,
        display: display_config.clone(),
        screenshot: vec![0; initial_rect.width * initial_height],
        screenshot_width: u32::try_from(initial_rect.width).unwrap(),
        screenshot_height: u32::try_from(initial_height).unwrap(),
        native_screenshot: vec![0; WIDTH * HEIGHT],
        native_screenshot_width: u32::try_from(WIDTH).unwrap(),
        native_screenshot_height: u32::try_from(HEIGHT).unwrap(),
        pending_commands: Vec::new(),
    }));

    let server_state = AppState {
        machine: MachineState(Arc::clone(&shared_state)),
        media_store,
    };
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let app = Router::new()
                .route(rumiga_api::MACHINE_STATUS_PATH, get(get_status))
                .route(
                    rumiga_api::MACHINE_CONFIG_PATH,
                    get(get_config).put(put_config),
                )
                .route(
                    rumiga_api::MACHINE_SUPPORT_BUNDLE_PATH,
                    get(get_support_bundle),
                )
                .route(rumiga_api::MACHINE_RESET_PATH, post(post_reset))
                .route(rumiga_api::MACHINE_PAUSE_PATH, post(post_pause))
                .route(rumiga_api::MACHINE_RESUME_PATH, post(post_resume))
                .route(rumiga_api::MACHINE_START_PATH, post(post_resume))
                .route(rumiga_api::MACHINE_STOP_PATH, post(post_pause))
                .route(
                    rumiga_api::MACHINE_FLOPPY_INSERT_PATH,
                    post(post_floppy_insert),
                )
                .route(
                    rumiga_api::MACHINE_FLOPPY_EJECT_PATH,
                    post(post_floppy_eject),
                )
                .route(
                    rumiga_api::MACHINE_AUDIO_SEPARATION_PATH,
                    post(post_audio_separation),
                )
                .route(rumiga_api::MACHINE_SCREENSHOT_PATH, get(get_screenshot))
                .route(rumiga_api::FILES_PATH, get(get_files))
                .route(
                    rumiga_api::FILES_UPLOAD_PATH,
                    post(post_upload).layer(DefaultBodyLimit::disable()),
                )
                .route(rumiga_api::FILES_FORMAT_PATH, post(post_format))
                .route(rumiga_api::FILES_DELETE_PATH, delete(delete_file_handler))
                .route(rumiga_api::WIFI_STATUS_PATH, get(get_wifi_status))
                .route(rumiga_api::WIFI_SCAN_PATH, post(post_wifi_scan))
                .route(rumiga_api::WIFI_CONNECT_PATH, post(post_wifi_connect))
                .fallback(static_handler)
                .with_state(server_state);

            let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
                .await
                .unwrap();
            eprintln!("REST API server listening at http://127.0.0.1:8080");
            axum::serve(listener, app).await.unwrap();
        });
    });

    let mut video = DesktopVideo::new(
        "Rumiga",
        initial_rect.width,
        initial_height,
        launch_args.scale,
    )
    .unwrap_or_else(|| {
        eprintln!("Failed to create video window");
        process::exit(1);
    });

    let window_handle = video.window_handle();

    let mut last_mouse: Option<(f32, f32)> = None;
    let mut mouse_accum_x = 0.0f32;
    let mut mouse_accum_y = 0.0f32;
    let mut last_y_start = 0;
    let mut last_y_end = initial_rect.height;
    let mut last_presented_height = initial_height;

    // Pacing and frame-rate measurement own host time here, not in the core.
    let mut clock = DesktopClock::new();
    let frame_period = emulator.frame_period();
    let mut frames_since_sample = 0_u32;
    let mut sample_started = clock.now();

    while video.is_open() {
        let frame_started = clock.now();

        // Check ESC to quit
        if window_handle.borrow().is_key_down(Key::Escape) {
            break;
        }

        // Drain and execute pending API commands
        let commands = {
            let mut s = shared_state.lock().unwrap();
            std::mem::take(&mut s.pending_commands)
        };

        for cmd in commands {
            match cmd {
                ApiCommand::Reset => {
                    eprintln!("API: Resetting machine...");
                    emulator.cpu.reset(&mut emulator.memory);
                }
                ApiCommand::Pause => {
                    eprintln!("API: Pausing emulation...");
                    let mut s = shared_state.lock().unwrap();
                    s.running = false;
                }
                ApiCommand::Resume => {
                    eprintln!("API: Resuming emulation...");
                    let mut s = shared_state.lock().unwrap();
                    s.running = true;
                }
                ApiCommand::InsertFloppy {
                    drive_idx,
                    path,
                    data,
                } => {
                    eprintln!("API: Inserting floppy {path} into DF{drive_idx}");
                    emulator.insert_floppy(drive_idx, data);
                    shared_state.lock().unwrap().floppy[drive_idx] = Some(path.clone());
                    floppy_paths[drive_idx] = Some(path);
                }
                ApiCommand::EjectFloppy { drive_idx } => {
                    eprintln!("API: Ejecting floppy from DF{drive_idx}");
                    let _ = emulator.extract_floppy(drive_idx);
                    shared_state.lock().unwrap().floppy[drive_idx] = None;
                    floppy_paths[drive_idx] = None;
                }
                ApiCommand::UpdateAudioSeparation { separation } => {
                    eprintln!("API: Updating audio separation to {separation}%");
                    emulator.audio.apply_separation(separation);
                    let mut s = shared_state.lock().unwrap();
                    s.stereo_separation = separation;
                }
                ApiCommand::UpdateFloppySpeed { percent } => {
                    eprintln!("API: Updating floppy speed to {percent}%");
                    emulator.set_floppy_speed_percent(percent);
                    let mut s = shared_state.lock().unwrap();
                    s.floppy_speed_percent = percent;
                }
                ApiCommand::UpdateNetwork { config } => {
                    eprintln!(
                        "API: Updating network backend to {}",
                        config.backend.as_str()
                    );
                    if let Err(e) = network_backend.configure(&config, &mut emulator) {
                        eprintln!("API Error: Failed to update network backend: {e}");
                    }
                    let network_status = network_status_from_emulator(&config, &emulator);
                    let mut s = shared_state.lock().unwrap();
                    s.network = config;
                    s.network_status = network_status;
                }
            }
        }

        let is_running = {
            let s = shared_state.lock().unwrap();
            s.running
        };

        if is_running {
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

            // Pass mouse events to emulator
            {
                let win = window_handle.borrow();
                if let Some((mx, my)) = win.get_mouse_pos(minifb::MouseMode::Discard) {
                    if let Some((lmx, lmy)) = last_mouse {
                        let dx_f = (mx - lmx) * launch_args.mouse_scale_x;

                        let mut dy_f = my - lmy;
                        #[allow(clippy::cast_precision_loss)]
                        let active_height = (last_y_end - last_y_start).max(1) as f32;
                        #[allow(clippy::cast_precision_loss)]
                        let p_height = last_presented_height.max(1) as f32;
                        dy_f *= active_height / p_height;
                        dy_f *= launch_args.mouse_scale_y;

                        mouse_accum_x += dx_f;
                        mouse_accum_y += dy_f;

                        let dx = mouse_accum_x.trunc();
                        let dy = mouse_accum_y.trunc();

                        #[allow(clippy::cast_possible_truncation)]
                        let dx_i = dx as i16;
                        #[allow(clippy::cast_possible_truncation)]
                        let dy_i = dy as i16;

                        mouse_accum_x -= dx;
                        mouse_accum_y -= dy;

                        if dx_i != 0 || dy_i != 0 {
                            emulator.mouse_move(dx_i, dy_i);
                        }
                    }
                    last_mouse = Some((mx, my));
                } else {
                    last_mouse = None;
                }

                let left = win.get_mouse_down(minifb::MouseButton::Left);
                let right = win.get_mouse_down(minifb::MouseButton::Right);
                emulator.mouse_button(left, right);
            }

            emulator.run_frame();
            if let Err(e) = network_backend.pump(&emulator) {
                eprintln!("Network backend error: {e}");
            }
            let network_status = {
                let network = shared_state.lock().unwrap().network.clone();
                network_status_from_emulator(&network, &emulator)
            };
            let framebuffer = emulator.framebuffer();
            let display_config = {
                let s = shared_state.lock().unwrap();
                s.display.clone()
            };
            let frame = match prepare_capture_frame(
                framebuffer,
                &display_config,
                Some(&emulator.playfield),
            ) {
                Ok(frame) => frame,
                Err(e) => {
                    eprintln!("Failed to prepare video frame: {e}");
                    break;
                }
            };
            last_y_start = frame.source_y_start;
            last_y_end = frame.source_y_end;
            last_presented_height = frame.height;
            video.present_frame(
                &frame.pixels,
                u32::try_from(frame.width).unwrap_or(u32::MAX),
                u32::try_from(frame.height).unwrap_or(u32::MAX),
            );

            {
                let mut s = shared_state.lock().unwrap();
                s.screenshot_width = u32::try_from(frame.width).unwrap_or(u32::MAX);
                s.screenshot_height = u32::try_from(frame.height).unwrap_or(u32::MAX);
                copy_rgb565_to_argb(&frame.pixels, &mut s.screenshot);
                s.native_screenshot_width = u32::try_from(WIDTH).unwrap_or(u32::MAX);
                s.native_screenshot_height = u32::try_from(HEIGHT).unwrap_or(u32::MAX);
                copy_rgb565_to_argb(framebuffer, &mut s.native_screenshot);
                s.network_status = network_status;
            }
            emulator.clear_frame_ready();
        }

        // Pace to the emulated frame period. `pace` reports what the host actually
        // spent, which is what the frame-rate measurement below uses.
        let spent = clock.now().saturating_sub(frame_started);
        clock.pace(frame_period.saturating_sub(spent));

        frames_since_sample += 1;
        let sample_elapsed = clock.now().saturating_sub(sample_started);
        if sample_elapsed >= FPS_SAMPLE_WINDOW {
            #[allow(
                clippy::cast_precision_loss,
                reason = "a frame count and a sample window are far below f32 precision limits"
            )]
            let measured = frames_since_sample as f32 / sample_elapsed.as_secs_f32();
            shared_state.lock().unwrap().fps = measured;
            frames_since_sample = 0;
            sample_started = clock.now();
        }
    }

    // Durability of the trace file is explicit, not a side effect of drop order.
    emulator.flush_trace();

    if let Err(e) = write_hdf_snapshot_if_requested(&emulator, &launch_args) {
        eprintln!("{e}");
        process::exit(1);
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
            match launch_args.hdf_write_policy {
                rumiga_api::HdfWritePolicy::ReadOnly => {
                    eprintln!(
                        "Discarding dirty HDF session buffer for {hdf_path}; source file is protected by read-only policy."
                    );
                    emulator.clear_hdf_dirty();
                }
                rumiga_api::HdfWritePolicy::Writeback => {
                    if let Some(data) = emulator.extract_hdf() {
                        eprintln!("Writing dirty HDF sectors back to {hdf_path}...");
                        if let Err(e) = atomic_write_file(Path::new(hdf_path), &data) {
                            eprintln!("{e}");
                        } else {
                            emulator.clear_hdf_dirty();
                        }
                    }
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

fn atomic_write_file(path: &Path, data: &[u8]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("Invalid output path '{}'", path.display()))?;
    let tmp_path = parent.join(format!(".{file_name}.rumiga-tmp-{}", process::id()));

    fs::write(&tmp_path, data).map_err(|e| {
        format!(
            "Failed to write temporary file '{}': {e}",
            tmp_path.display()
        )
    })?;
    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("Failed to replace file '{}': {e}", path.display()));
    }
    Ok(())
}

fn write_hdf_snapshot_if_requested(
    emulator: &Emulator,
    launch_args: &LaunchArgs,
) -> Result<Option<HdfSnapshotEvidence>, String> {
    let Some(snapshot_path) = &launch_args.hdf_snapshot_path else {
        return Ok(None);
    };
    let Some(source_path) = &launch_args.hdf_path else {
        return Err("--hdf-snapshot requires --hdf".to_owned());
    };
    if source_path == snapshot_path
        || existing_paths_match(Path::new(source_path), Path::new(snapshot_path))
    {
        return Err("--hdf-snapshot must not point at the source HDF".to_owned());
    }

    let Some(snapshot_data) = emulator.extract_hdf() else {
        return Err("Cannot write HDF snapshot because no HDF is mounted".to_owned());
    };
    let source_data = fs::read(source_path)
        .map_err(|e| format!("Failed to read source HDF '{source_path}' for diff: {e}"))?;
    let diff = hdf_diff_stats(&source_data, &snapshot_data, HDF_DIFF_SECTOR_SIZE);
    let snapshot_file = Path::new(snapshot_path);
    create_parent_dirs(snapshot_file)?;
    atomic_write_file(snapshot_file, &snapshot_data)?;

    let evidence = HdfSnapshotEvidence {
        path: snapshot_path.clone(),
        bytes: snapshot_data.len(),
        sha256: sha256_hex(&snapshot_data),
        source_sha256: sha256_hex(&source_data),
        dirty: emulator.hdf_dirty(),
        changed_bytes: diff.changed_bytes,
        changed_sectors: diff.changed_sectors,
        sector_size: diff.sector_size,
    };
    eprintln!(
        "Wrote HDF session snapshot: {} ({} bytes changed across {} sectors, dirty={})",
        snapshot_file.display(),
        evidence.changed_bytes,
        evidence.changed_sectors,
        evidence.dirty
    );
    Ok(Some(evidence))
}

const HDF_DIFF_SECTOR_SIZE: usize = 512;

fn hdf_diff_stats(source: &[u8], snapshot: &[u8], sector_size: usize) -> HdfDiffStats {
    let max_len = source.len().max(snapshot.len());
    let changed_bytes = (0..max_len)
        .filter(|&index| source.get(index) != snapshot.get(index))
        .count();
    let changed_sectors = if sector_size == 0 {
        0
    } else {
        max_len.div_ceil(sector_size).saturating_sub(
            (0..max_len.div_ceil(sector_size))
                .filter(|&sector| {
                    let start = sector * sector_size;
                    let end = (start + sector_size).min(max_len);
                    source.get(start..end) == snapshot.get(start..end)
                })
                .count(),
        )
    };

    HdfDiffStats {
        changed_bytes,
        changed_sectors,
        sector_size,
    }
}

fn existing_paths_match(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
fn parse_args(args: &[String]) -> Result<LaunchArgs, String> {
    let mut model = None;
    let mut scale = DEFAULT_SCALE;
    let mut scaling_mode = rumiga_api::ScalingMode::default();
    let mut viewport_mode = ViewportMode::Auto;
    let mut vertical_stretch = true;
    let mut floppy_speed_percent = FLOPPY_SPEED_COMPATIBLE_PERCENT;
    let mut hdf_path = None;
    let mut hdf_write_policy = rumiga_api::HdfWritePolicy::default();
    let mut hdf_snapshot_path = None;
    let mut storage_root = None;
    let mut upload_limit_bytes = DEFAULT_UPLOAD_LIMIT_BYTES;
    let mut network = rumiga_api::NetworkConfig::default();
    let mut network_pcap_path = None;
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
    let mut capture_kind = rumiga_api::ScreenshotKind::default();
    let mut mouse_scale_x = 0.5f32;
    let mut mouse_scale_y = 1.0f32;
    let mut audio_separation = 100u8;
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
            "--scaling-mode" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--scaling-mode requires a value".to_owned());
                };
                scaling_mode = parse_scaling_mode(value)?;
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
            "--hdf-writeback" => {
                hdf_write_policy = rumiga_api::HdfWritePolicy::Writeback;
                index += 1;
            }
            "--hdf-write-policy" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--hdf-write-policy requires a value".to_owned());
                };
                hdf_write_policy = parse_hdf_write_policy(value)?;
                index += 2;
            }
            "--hdf-snapshot" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--hdf-snapshot requires a value".to_owned());
                };
                hdf_snapshot_path = Some(value.clone());
                index += 2;
            }
            "--storage-root" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--storage-root requires a value".to_owned());
                };
                if value.is_empty() {
                    return Err("--storage-root must not be empty".to_owned());
                }
                storage_root = Some(PathBuf::from(value));
                index += 2;
            }
            "--upload-limit-mib" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--upload-limit-mib requires a value".to_owned());
                };
                let limit_mib = value
                    .parse::<u64>()
                    .map_err(|_| format!("Unsupported upload limit '{value}'"))?;
                upload_limit_bytes = limit_mib
                    .checked_mul(1024 * 1024)
                    .filter(|bytes| *bytes > 0 && *bytes <= MAX_UPLOAD_LIMIT_BYTES)
                    .ok_or_else(|| {
                        format!(
                            "upload-limit-mib must be between 1 and {}",
                            MAX_UPLOAD_LIMIT_BYTES / (1024 * 1024)
                        )
                    })?;
                index += 2;
            }
            "--network" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--network requires a value".to_owned());
                };
                network.backend = parse_network_backend(value)?;
                index += 2;
            }
            "--network-slirp" => {
                network.backend = rumiga_api::NetworkBackend::Slirp;
                index += 1;
            }
            "--network-off" => {
                network.backend = rumiga_api::NetworkBackend::Disabled;
                index += 1;
            }
            "--network-mac" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--network-mac requires a value".to_owned());
                };
                network.mac_address = parse_network_mac_address(value)?;
                index += 2;
            }
            "--network-pcap" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--network-pcap requires a value".to_owned());
                };
                network_pcap_path = Some(value.clone());
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
            "--capture-kind" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--capture-kind requires a value".to_owned());
                };
                capture_kind = parse_screenshot_kind(value)?;
                index += 2;
            }
            "--mouse-scale-x" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--mouse-scale-x requires a value".to_owned());
                };
                mouse_scale_x = value
                    .parse::<f32>()
                    .map_err(|_| format!("Unsupported mouse scale X '{value}'"))?;
                if mouse_scale_x <= 0.0 {
                    return Err("mouse-scale-x must be positive".to_owned());
                }
                index += 2;
            }
            "--mouse-scale-y" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--mouse-scale-y requires a value".to_owned());
                };
                mouse_scale_y = value
                    .parse::<f32>()
                    .map_err(|_| format!("Unsupported mouse scale Y '{value}'"))?;
                if mouse_scale_y <= 0.0 {
                    return Err("mouse-scale-y must be positive".to_owned());
                }
                index += 2;
            }
            "--audio-separation" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--audio-separation requires a value".to_owned());
                };
                audio_separation = value
                    .parse::<u8>()
                    .map_err(|_| format!("Unsupported audio separation '{value}'"))?;
                if audio_separation > 100 {
                    return Err("audio-separation must be between 0 and 100".to_owned());
                }
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
    if hdf_snapshot_path.is_some() && hdf_path.is_none() {
        return Err("--hdf-snapshot requires --hdf".to_owned());
    }
    if let (Some(hdf), Some(snapshot)) = (&hdf_path, &hdf_snapshot_path) {
        if hdf == snapshot || existing_paths_match(Path::new(hdf), Path::new(snapshot)) {
            return Err("--hdf-snapshot must not point at the source HDF".to_owned());
        }
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
        scaling_mode,
        viewport_mode,
        vertical_stretch,
        floppy_speed_percent,
        rom_path: rom_path.clone(),
        adf_paths: positional.iter().skip(1).cloned().collect(),
        hdf_path,
        hdf_write_policy,
        hdf_snapshot_path,
        storage_root,
        upload_limit_bytes,
        network,
        network_pcap_path,
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
        capture_kind,
        mouse_scale_x,
        mouse_scale_y,
        audio_separation,
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

fn parse_scaling_mode(value: &str) -> Result<rumiga_api::ScalingMode, String> {
    match value.to_ascii_lowercase().as_str() {
        "integer" | "int" => Ok(rumiga_api::ScalingMode::Integer),
        "aspect-fit" | "aspect_fit" | "aspect" | "fit" => Ok(rumiga_api::ScalingMode::AspectFit),
        "stretch" => Ok(rumiga_api::ScalingMode::Stretch),
        _ => Err(format!("Unsupported scaling mode '{value}'")),
    }
}

fn parse_screenshot_kind(value: &str) -> Result<rumiga_api::ScreenshotKind, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "native-framebuffer" | "native_framebuffer" | "native" | "nativeframebuffer" => {
            Ok(rumiga_api::ScreenshotKind::NativeFramebuffer)
        }
        "viewport-presentation"
        | "viewport_presentation"
        | "presentation"
        | "viewport"
        | "viewportpresentation" => Ok(rumiga_api::ScreenshotKind::ViewportPresentation),
        _ => Err(format!("Unsupported screenshot kind '{value}'")),
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

fn parse_hdf_write_policy(value: &str) -> Result<rumiga_api::HdfWritePolicy, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "read-only" | "readonly" | "snapshot" | "discard" | "discard-writes" => {
            Ok(rumiga_api::HdfWritePolicy::ReadOnly)
        }
        "writeback" | "write-back" | "rw" | "read-write" => {
            Ok(rumiga_api::HdfWritePolicy::Writeback)
        }
        _ => Err(format!(
            "Unsupported HDF write policy '{value}'. Supported: read-only, writeback"
        )),
    }
}

fn parse_network_backend(value: &str) -> Result<rumiga_api::NetworkBackend, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "disabled" | "off" | "none" => Ok(rumiga_api::NetworkBackend::Disabled),
        "slirp" | "nat" => Ok(rumiga_api::NetworkBackend::Slirp),
        _ => Err(format!(
            "Unsupported network backend '{value}'. Supported: disabled, slirp"
        )),
    }
}

fn parse_network_mac_address(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if rumiga_api::is_valid_unicast_mac_address(&normalized) {
        Ok(normalized)
    } else {
        Err(format!(
            "Invalid network MAC address '{value}'. Expected a unicast address like 00:80:10:4d:49:47"
        ))
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
    let msg = r"Usage: rumiga-desktop [options] <kickstart.rom> [floppy1.adf] [floppy2.adf] ...

Options:
  -m, --model <model>     Machine profile: a500, a500-plus, a600, a1200
  -s, --scale <factor>    Window scale: 1, 2, 4, 8, 16, 32 [default: 1]
      --scaling-mode <mode>
                          Host presentation scaling: integer, aspect-fit, stretch
      --viewport <mode>   Viewport mode: auto, raw, native-full-border,
                          visible-area, overscan, auto-center [default: auto]
      --no-vertical-stretch  Disable vertical line doubling
      --mouse-scale-x <f> Scaling factor for horizontal mouse [default: 0.5]
      --mouse-scale-y <f> Scaling factor for vertical mouse [default: 1.0]
      --floppy-speed <%>  Floppy read speed: 100%, 200%, 400%, 800%, turbo
      --hdf <file.hdf>    Mount Gayle IDE virtual hardfile (.hdf)
      --hdf-write-policy <policy>
                          HDF persistence: read-only, writeback [default: read-only]
      --hdf-writeback     Persist dirty HDF sectors back to the source file on exit
      --hdf-snapshot <file.hdf>
                          Write the current in-memory HDF buffer to a separate file
      --storage-root <dir>
                          REST media root [env: RUMIGA_STORAGE_ROOT; default: ./rumiga-media]
      --upload-limit-mib <MiB>
                          Maximum streamed REST upload [default: 2048; max: 8192]
      --network <backend> Amiga network backend: disabled, slirp [default: disabled]
      --network-slirp     Enable A2065-compatible Ethernet via SLIRP/NAT
      --network-mac <mac> MAC for the emulated A2065 card
      --network-pcap <file.pcap>  Capture raw A2065/SLIRP Ethernet frames
      --cpu <type>        Override CPU: 68000, 68010, 68020, 68030, 68040
      --chip-ram <size>   Override Chip RAM size: e.g. 512K, 1M, 2M
      --slow-ram <size>   Override Slow RAM size: e.g. 512K, 1M
      --fast-ram <size>   Override Fast RAM size: e.g. 1M, 2M, 4M, 8M
      --pal               Force PAL video timing
      --ntsc              Force NTSC video timing
      --df0 <file.adf>    Explicitly mount floppy in DF0
      --df1 <file.adf>    Explicitly mount floppy in DF1
      --df2 <file.adf>    Explicitly mount floppy in DF2
      --df3 <file.adf>    Explicitly mount floppy in DF3
      --trace-cpu <file>  Save assembly instruction trace to a file
      --trace-limit <n>   Stop tracing after N instructions
      --capture <file.png>  Run headless and save a PNG screenshot
      --capture-frames <n>  Frames to run before capture [default: 300]
      --capture-kind <kind>  Capture kind: viewport-presentation, native-framebuffer
      --capture-manifest <file.json>  Save capture evidence manifest";
    if to_stdout {
        println!("{msg}");
    } else {
        eprintln!("{msg}");
    }
}

fn capture_evidence(
    emulator: &mut Emulator,
    network_backend: &mut DesktopNetworkBackend,
    context: &CaptureEvidenceContext<'_>,
) -> Result<(), String> {
    for _ in 0..context.args.capture_frames {
        emulator.run_frame();
        network_backend.pump(emulator)?;
    }
    // Durability of the trace file is explicit, not a side effect of drop order.
    emulator.flush_trace();

    let hdf_snapshot = write_hdf_snapshot_if_requested(emulator, context.args)?;
    let frame = prepare_capture_frame_for_kind(
        emulator.framebuffer(),
        context.display,
        Some(&emulator.playfield),
        &context.args.capture_kind,
    )?;
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
        display: context.display,
        model: context.model,
        config: context.config,
        emulator,
        rom: context.rom,
        floppies: context.floppies,
        hdf: context.hdf,
        hdf_snapshot: hdf_snapshot.as_ref(),
    };
    write_capture_manifest(&manifest_path, &manifest_context)?;

    eprintln!(
        "Captured {} {}x{} after {} frames: {}",
        screenshot_kind_label(&context.args.capture_kind),
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
    display: &rumiga_api::DisplayConfig,
    playfield: Option<&PlayfieldState>,
) -> Result<CaptureFrame, String> {
    let rect = resolve_viewport_rect(display, playfield);
    let output_height = presented_height(rect, display.viewport.vertical_stretch);
    let output_len = rect
        .width
        .checked_mul(output_height)
        .ok_or_else(|| "Capture dimensions overflow".to_owned())?;
    let mut pixels = vec![0u16; output_len];
    if !copy_presented_viewport(framebuffer, WIDTH, HEIGHT, rect, output_height, &mut pixels) {
        return Err("Failed to prepare capture frame".to_owned());
    }

    Ok(CaptureFrame {
        pixels,
        width: rect.width,
        height: output_height,
        source_x_start: rect.x,
        source_x_end: rect.x_end(),
        source_y_start: rect.y,
        source_y_end: rect.y_end(),
    })
}

fn prepare_capture_frame_for_kind(
    framebuffer: &[u16],
    display: &rumiga_api::DisplayConfig,
    playfield: Option<&PlayfieldState>,
    kind: &rumiga_api::ScreenshotKind,
) -> Result<CaptureFrame, String> {
    match kind {
        rumiga_api::ScreenshotKind::NativeFramebuffer => prepare_native_capture_frame(framebuffer),
        rumiga_api::ScreenshotKind::ViewportPresentation => {
            prepare_capture_frame(framebuffer, display, playfield)
        }
    }
}

fn prepare_native_capture_frame(framebuffer: &[u16]) -> Result<CaptureFrame, String> {
    let expected_len = WIDTH
        .checked_mul(HEIGHT)
        .ok_or_else(|| "Native framebuffer dimensions overflow".to_owned())?;
    if framebuffer.len() != expected_len {
        return Err(format!(
            "Native framebuffer length mismatch: expected {expected_len}, got {}",
            framebuffer.len()
        ));
    }

    Ok(CaptureFrame {
        pixels: framebuffer.to_vec(),
        width: WIDTH,
        height: HEIGHT,
        source_x_start: 0,
        source_x_end: WIDTH,
        source_y_start: 0,
        source_y_end: HEIGHT,
    })
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
    push_manifest_schema_json(&mut json);
    push_manifest_producer_json(&mut json);
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
        context.emulator.trace_count()
    );
    push_viewport_json(&mut json, context);
    push_presentation_json(&mut json, context);
    push_native_framebuffer_json(&mut json);
    push_boot_workarounds_json(&mut json, context.emulator);
    push_cia_state_json(&mut json, context.emulator);
    let _ = writeln!(
        json,
        "  \"framebuffer\": {{ \"background_rgb565\": {}, \"pixels_different_from_background\": {}, \"non_zero_rgb565_pixels\": {}, \"distinct_colors\": {} }},",
        json_string(&rgb565_hex(first_pixel(&context.frame.pixels))),
        count_pixels_different_from_first(&context.frame.pixels),
        count_non_zero_pixels(&context.frame.pixels),
        count_distinct_colors(&context.frame.pixels)
    );
    push_edge_integrity_json(&mut json, context.emulator.framebuffer());
    push_video_state_json(&mut json, context.emulator);
    push_floppy_state_json(&mut json, &context.emulator.floppy);
    push_gayle_ide_state_json(
        &mut json,
        context.emulator,
        context.args.hdf_write_policy,
        context.hdf_snapshot,
    );
    push_network_state_json(
        &mut json,
        &context.args.network,
        context.emulator,
        context.args.network_pcap_path.as_deref(),
    );
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

fn push_viewport_json(json: &mut String, context: &CaptureManifestContext<'_>) {
    let _ = writeln!(
        json,
        "  \"viewport\": {{ \"mode\": {}, \"preset\": {}, \"vertical_stretch\": {}, \"source_width\": {}, \"source_height\": {}, \"source_x_start\": {}, \"source_x_end\": {}, \"source_y_start\": {}, \"source_y_end\": {}, \"output_width\": {}, \"output_height\": {} }},",
        json_string(&format!("{:?}", context.display.viewport.mode)),
        json_string(&format!("{:?}", context.display.viewport.preset)),
        context.display.viewport.vertical_stretch,
        WIDTH,
        HEIGHT,
        context.frame.source_x_start,
        context.frame.source_x_end,
        context.frame.source_y_start,
        context.frame.source_y_end,
        context.frame.width,
        context.frame.height
    );
}

fn push_presentation_json(json: &mut String, context: &CaptureManifestContext<'_>) {
    let _ = writeln!(
        json,
        "  \"presentation\": {{ \"capture_kind\": {}, \"scaling\": {}, \"window_scale\": {}, \"orientation_landscape\": {} }},",
        json_string(screenshot_kind_label(&context.args.capture_kind)),
        json_string(&format!("{:?}", context.display.scaling)),
        context.args.scale,
        context.display.orientation_landscape
    );
}

fn push_manifest_schema_json(json: &mut String) {
    let _ = writeln!(
        json,
        "  \"schema\": {{ \"id\": {}, \"version\": {} }},",
        json_string(CAPTURE_MANIFEST_SCHEMA_ID),
        CAPTURE_MANIFEST_SCHEMA_VERSION
    );
}

fn push_manifest_producer_json(json: &mut String) {
    let git = git_evidence();
    let _ = writeln!(
        json,
        "  \"producer\": {{ \"name\": {}, \"version\": {}, \"git_sha\": {}, \"git_dirty\": {}, \"target_os\": {}, \"target_arch\": {} }},",
        json_string("rumiga-desktop"),
        json_string(env!("CARGO_PKG_VERSION")),
        json_string(&git.sha),
        git.dirty,
        json_string(std::env::consts::OS),
        json_string(std::env::consts::ARCH)
    );
}

fn push_native_framebuffer_json(json: &mut String) {
    let _ = writeln!(
        json,
        "  \"native_framebuffer\": {{ \"pixel_format\": {}, \"width\": {}, \"height\": {} }},",
        json_string("rgb565"),
        WIDTH,
        HEIGHT
    );
}

fn push_boot_workarounds_json(json: &mut String, emulator: &Emulator) {
    let _ = writeln!(
        json,
        "  \"boot_workarounds\": {{ \"forced_cia_timer_start\": false, \"forced_cia_timer_start_count\": 0, \"rom_drive_step_patch\": {} }},",
        emulator.memory.rom_drive_step_patch_applied
    );
}

fn push_cia_state_json(json: &mut String, emulator: &Emulator) {
    let cia = emulator.memory.cia.borrow();
    json.push_str("  \"cia\": {\n");
    push_single_cia_json(json, "a", &cia.cia_a, "    ", true);
    push_single_cia_json(json, "b", &cia.cia_b, "    ", false);
    json.push_str("  },\n");
}

fn push_single_cia_json(json: &mut String, name: &str, cia: &CiaState, indent: &str, comma: bool) {
    let suffix = if comma { "," } else { "" };
    let _ = writeln!(json, "{indent}\"{name}\": {{");
    let _ = writeln!(
        json,
        "{indent}  \"icr\": {{ \"pending\": {}, \"mask\": {}, \"ir\": {} }},",
        json_string(&format!("0x{:02X}", cia.icr_data)),
        json_string(&format!("0x{:02X}", cia.icr_mask)),
        cia.icr_ir
    );
    let _ = writeln!(
        json,
        "{indent}  \"timer_a\": {{ \"counter\": {}, \"latch\": {}, \"control\": {}, \"start_writes\": {}, \"stop_writes\": {}, \"force_load_writes\": {}, \"auto_start_writes\": {}, \"underflows\": {} }},",
        json_string(&format!("0x{:04X}", cia.timer_a)),
        json_string(&format!("0x{:04X}", cia.timer_a_latch)),
        json_string(&format!("0x{:02X}", cia.cra)),
        cia.timer_a_stats.start_writes,
        cia.timer_a_stats.stop_writes,
        cia.timer_a_stats.force_load_writes,
        cia.timer_a_stats.auto_start_writes,
        cia.timer_a_stats.underflows
    );
    let _ = writeln!(
        json,
        "{indent}  \"timer_b\": {{ \"counter\": {}, \"latch\": {}, \"control\": {}, \"start_writes\": {}, \"stop_writes\": {}, \"force_load_writes\": {}, \"auto_start_writes\": {}, \"underflows\": {} }},",
        json_string(&format!("0x{:04X}", cia.timer_b)),
        json_string(&format!("0x{:04X}", cia.timer_b_latch)),
        json_string(&format!("0x{:02X}", cia.crb)),
        cia.timer_b_stats.start_writes,
        cia.timer_b_stats.stop_writes,
        cia.timer_b_stats.force_load_writes,
        cia.timer_b_stats.auto_start_writes,
        cia.timer_b_stats.underflows
    );
    push_cia_register_writes_json(json, cia, indent);
    let _ = writeln!(json, "{indent}}}{suffix}");
}

fn push_cia_register_writes_json(json: &mut String, cia: &CiaState, indent: &str) {
    const REGISTER_NAMES: [&str; 16] = [
        "pra", "prb", "ddra", "ddrb", "talo", "tahi", "tblo", "tbhi", "tod_lo", "tod_mid",
        "tod_hi", "unused_b", "sdr", "icr", "cra", "crb",
    ];

    let _ = writeln!(json, "{indent}  \"register_writes\": {{");
    for (index, name) in REGISTER_NAMES.iter().enumerate() {
        let comma = if index + 1 == REGISTER_NAMES.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(
            json,
            "{indent}    \"{name}\": {{ \"count\": {}, \"last\": {} }}{comma}",
            cia.register_write_counts[index],
            json_string(&format!("0x{:02X}", cia.last_register_writes[index]))
        );
    }
    let _ = writeln!(json, "{indent}  }}");
}

fn push_edge_integrity_json(json: &mut String, framebuffer: &[u16]) {
    if let Some(edge) = inspect_frame_edges(
        framebuffer,
        WIDTH,
        HEIGHT,
        EDGE_INSPECTION_LINES,
        EDGE_INSPECTION_WIDTH,
    ) {
        let _ = writeln!(
            json,
            "  \"edge_integrity\": {{ \"first_lines\": {}, \"edge_width\": {}, \"sampled_lines\": {}, \"background_rgb565\": {}, \"left_non_background_pixels\": {}, \"right_non_background_pixels\": {}, \"mirrored_non_background_pixels\": {}, \"right_edge_wrapped_to_left_pixels\": {}, \"left_edge_wrapped_to_right_pixels\": {}, \"content_line_count\": {}, \"min_content_width\": {}, \"max_content_width\": {} }},",
            edge.first_lines,
            edge.edge_width,
            edge.sampled_lines,
            json_string(&rgb565_hex(edge.background_rgb565)),
            edge.left_non_background_pixels,
            edge.right_non_background_pixels,
            edge.mirrored_non_background_pixels,
            edge.right_edge_wrapped_to_left_pixels,
            edge.left_edge_wrapped_to_right_pixels,
            edge.content_line_count,
            edge.min_content_width,
            edge.max_content_width
        );
    } else {
        json.push_str("  \"edge_integrity\": null,\n");
    }
}

fn push_video_state_json(json: &mut String, emulator: &Emulator) {
    let regs = &emulator.memory.custom_regs;
    let playfield = &emulator.playfield;
    let (hstart, hstop, vstart, vstop) = playfield.display_window();
    let active_x_start = active_x_for_hpos(hstart);
    let active_x_end = active_x_for_hpos(hstop);

    let _ = writeln!(json, "  \"video\": {{");
    let _ = writeln!(
        json,
        "    \"display_window\": {{ \"hstart\": {hstart}, \"hstop\": {hstop}, \"vstart\": {vstart}, \"vstop\": {vstop} }},"
    );
    let _ = writeln!(
        json,
        "    \"active_frame_window\": {{ \"display_left_hpos\": {}, \"x_start\": {}, \"x_end\": {}, \"right_margin\": {} }},",
        DISPLAY_LEFT_HPOS,
        active_x_start,
        active_x_end,
        WIDTH.saturating_sub(active_x_end)
    );
    let _ = writeln!(
        json,
        "    \"diwstrt\": {},",
        json_string(&format!("0x{:04X}", regs[(custom::DIWSTRT / 2) as usize]))
    );
    let _ = writeln!(
        json,
        "    \"diwstop\": {},",
        json_string(&format!("0x{:04X}", regs[(custom::DIWSTOP / 2) as usize]))
    );
    let _ = writeln!(
        json,
        "    \"diwhigh\": {},",
        json_string(&format!("0x{:04X}", regs[(custom::DIWHIGH / 2) as usize]))
    );
    let _ = writeln!(
        json,
        "    \"ddfstrt\": {},",
        json_string(&format!("0x{:04X}", regs[(custom::DDFSTRT / 2) as usize]))
    );
    let _ = writeln!(
        json,
        "    \"ddfstop\": {},",
        json_string(&format!("0x{:04X}", regs[(custom::DDFSTOP / 2) as usize]))
    );
    let _ = writeln!(
        json,
        "    \"bplcon0\": {},",
        json_string(&format!("0x{:04X}", playfield.bplcon0))
    );
    let _ = writeln!(
        json,
        "    \"bplcon1\": {},",
        json_string(&format!("0x{:04X}", playfield.bplcon1))
    );
    let _ = writeln!(
        json,
        "    \"bplcon2\": {},",
        json_string(&format!("0x{:04X}", playfield.bplcon2))
    );
    let _ = writeln!(
        json,
        "    \"bplcon3\": {},",
        json_string(&format!("0x{:04X}", playfield.bplcon3))
    );
    let _ = writeln!(
        json,
        "    \"bplcon4\": {},",
        json_string(&format!("0x{:04X}", playfield.bplcon4))
    );
    let _ = writeln!(
        json,
        "    \"fmode\": {},",
        json_string(&format!("0x{:04X}", playfield.fmode))
    );
    let _ = writeln!(json, "    \"num_planes\": {},", playfield.num_planes());
    push_video_scanline_json(
        json,
        "first_scanline",
        emulator.first_video_scanline.as_ref(),
        true,
    );
    push_early_video_scanlines_json(json, emulator);
    push_video_scanline_json(
        json,
        "last_scanline",
        emulator.last_video_scanline.as_ref(),
        false,
    );
    json.push_str("  },\n");
}

fn push_video_scanline_json(
    json: &mut String,
    key: &str,
    scanline: Option<&VideoScanlineSnapshot>,
    trailing_comma: bool,
) {
    if let Some(scanline) = scanline {
        let active_x_start = active_x_for_hpos(scanline.hstart);
        let active_x_end = active_x_for_hpos(scanline.hstop);
        let _ = writeln!(json, "    {key:?}: {{");
        let _ = writeln!(json, "      \"vpos\": {},", scanline.vpos);
        let _ = writeln!(
            json,
            "      \"framebuffer_line\": {},",
            scanline.framebuffer_line
        );
        let _ = writeln!(
            json,
            "      \"display_window\": {{ \"hstart\": {}, \"hstop\": {}, \"vstart\": {}, \"vstop\": {} }},",
            scanline.hstart, scanline.hstop, scanline.vstart, scanline.vstop
        );
        let _ = writeln!(
            json,
            "      \"active_frame_window\": {{ \"display_left_hpos\": {}, \"x_start\": {}, \"x_end\": {}, \"right_margin\": {} }},",
            DISPLAY_LEFT_HPOS,
            active_x_start,
            active_x_end,
            WIDTH.saturating_sub(active_x_end)
        );
        let _ = writeln!(
            json,
            "      \"bplcon0\": {},",
            json_string(&format!("0x{:04X}", scanline.bplcon0))
        );
        let _ = writeln!(
            json,
            "      \"bplcon1\": {},",
            json_string(&format!("0x{:04X}", scanline.bplcon1))
        );
        let _ = writeln!(
            json,
            "      \"ddfstrt\": {},",
            json_string(&format!("0x{:04X}", scanline.ddfstrt))
        );
        let _ = writeln!(
            json,
            "      \"ddfstop\": {},",
            json_string(&format!("0x{:04X}", scanline.ddfstop))
        );
        let _ = writeln!(
            json,
            "      \"bpl1mod\": {},",
            json_string(&format!("0x{:04X}", scanline.bpl1mod))
        );
        let _ = writeln!(
            json,
            "      \"bpl2mod\": {},",
            json_string(&format!("0x{:04X}", scanline.bpl2mod))
        );
        push_scanline_bplpt_json(json, scanline);
        push_scanline_bitplane_words_json(json, scanline);
        let _ = writeln!(json, "      \"num_planes\": {}", scanline.num_planes);
        if trailing_comma {
            json.push_str("    },\n");
        } else {
            json.push_str("    }\n");
        }
    } else if trailing_comma {
        let _ = writeln!(json, "    {key:?}: null,");
    } else {
        let _ = writeln!(json, "    {key:?}: null");
    }
}

fn push_early_video_scanlines_json(json: &mut String, emulator: &Emulator) {
    let _ = writeln!(
        json,
        "    \"early_scanline_limit\": {EARLY_VIDEO_SCANLINE_DUMP},"
    );
    json.push_str("    \"early_scanlines\": [\n");
    for (index, scanline) in emulator.early_video_scanlines.iter().enumerate() {
        let _ = writeln!(json, "      {{");
        let _ = writeln!(json, "        \"vpos\": {},", scanline.vpos);
        let _ = writeln!(
            json,
            "        \"framebuffer_line\": {},",
            scanline.framebuffer_line
        );
        push_scanline_bplpt_json_with_indent(json, scanline, "        ");
        push_scanline_bitplane_words_json_with_indent(json, scanline, "        ");
        let _ = writeln!(json, "        \"num_planes\": {}", scanline.num_planes);
        if index + 1 == emulator.early_video_scanlines.len() {
            json.push_str("      }\n");
        } else {
            json.push_str("      },\n");
        }
    }
    json.push_str("    ],\n");
}

fn push_scanline_bplpt_json(json: &mut String, scanline: &VideoScanlineSnapshot) {
    push_scanline_bplpt_json_with_indent(json, scanline, "      ");
}

fn push_scanline_bplpt_json_with_indent(
    json: &mut String,
    scanline: &VideoScanlineSnapshot,
    indent: &str,
) {
    let planes = scanline.num_planes.min(scanline.bplpt.len());
    let _ = write!(json, "{indent}\"bplpt\": [");
    for plane in 0..planes {
        if plane > 0 {
            json.push_str(", ");
        }
        json.push_str(&json_string(&format!("0x{:06X}", scanline.bplpt[plane])));
    }
    json.push_str("],\n");
}

fn push_scanline_bitplane_words_json(json: &mut String, scanline: &VideoScanlineSnapshot) {
    push_scanline_bitplane_words_json_with_indent(json, scanline, "      ");
}

fn push_scanline_bitplane_words_json_with_indent(
    json: &mut String,
    scanline: &VideoScanlineSnapshot,
    indent: &str,
) {
    let planes = scanline.num_planes.min(scanline.bitplane_words.len());
    let _ = writeln!(json, "{indent}\"bitplane_words\": [");
    for plane in 0..planes {
        let _ = write!(json, "{indent}  [");
        for word_index in 0..VIDEO_SCANLINE_WORD_DUMP {
            if word_index > 0 {
                json.push_str(", ");
            }
            json.push_str(&json_string(&format!(
                "0x{:04X}",
                scanline.bitplane_words[plane][word_index]
            )));
        }
        if plane + 1 == planes {
            json.push_str("]\n");
        } else {
            json.push_str("],\n");
        }
    }
    let _ = writeln!(json, "{indent}],");
}

fn active_x_for_hpos(hpos: u16) -> usize {
    usize::from(hpos.saturating_sub(DISPLAY_LEFT_HPOS)).saturating_mul(2)
}

fn push_gayle_ide_state_json(
    json: &mut String,
    emulator: &Emulator,
    hdf_write_policy: rumiga_api::HdfWritePolicy,
    hdf_snapshot: Option<&HdfSnapshotEvidence>,
) {
    let ide = emulator.memory.ide.borrow();
    let disk_bytes = ide.disk_data.as_ref().map_or(0, Vec::len);
    let _ = writeln!(json, "  \"gayle_ide\": {{");
    let _ = writeln!(
        json,
        "    \"gayle_irq\": {},",
        json_string(&format!("0x{:02X}", emulator.memory.gayle_irq))
    );
    let _ = writeln!(
        json,
        "    \"gayle_intena\": {},",
        json_string(&format!("0x{:02X}", emulator.memory.gayle_intena))
    );
    let _ = writeln!(
        json,
        "    \"gayle_status\": {},",
        json_string(&format!("0x{:02X}", emulator.memory.gayle_status))
    );
    let _ = writeln!(
        json,
        "    \"gayle_config\": {},",
        json_string(&format!("0x{:02X}", emulator.memory.gayle_config))
    );
    let _ = writeln!(json, "    \"disk_inserted\": {},", ide.disk_data.is_some());
    let _ = writeln!(json, "    \"disk_bytes\": {disk_bytes},");
    let _ = writeln!(
        json,
        "    \"hdf_write_policy\": {},",
        json_string(hdf_write_policy.as_str())
    );
    let _ = writeln!(
        json,
        "    \"host_writeback_enabled\": {},",
        hdf_write_policy == rumiga_api::HdfWritePolicy::Writeback
    );
    let _ = writeln!(json, "    \"hdf_dirty\": {},", ide.hdf_dirty);
    push_hdf_snapshot_json(json, hdf_snapshot);
    let _ = writeln!(json, "    \"pending_irq\": {},", ide.pending_irq);
    let _ = writeln!(
        json,
        "    \"status\": {},",
        json_string(&format!("0x{:02X}", ide.status))
    );
    let _ = writeln!(
        json,
        "    \"error\": {},",
        json_string(&format!("0x{:02X}", ide.error))
    );
    let _ = writeln!(
        json,
        "    \"command\": {},",
        json_string(&format!("0x{:02X}", ide.command))
    );
    json.push_str("    \"command_log\": [");
    for (idx, command) in ide.command_log.iter().enumerate() {
        if idx > 0 {
            json.push_str(", ");
        }
        json.push_str(&json_string(&format!("0x{command:02X}")));
    }
    json.push_str("],\n");
    let _ = writeln!(
        json,
        "    \"select\": {},",
        json_string(&format!("0x{:02X}", ide.select))
    );
    let _ = writeln!(
        json,
        "    \"devcon\": {},",
        json_string(&format!("0x{:02X}", ide.devcon))
    );
    let _ = writeln!(json, "    \"nsector\": {},", ide.nsector);
    let _ = writeln!(json, "    \"sector\": {},", ide.sector);
    let _ = writeln!(json, "    \"lcyl\": {},", ide.lcyl);
    let _ = writeln!(json, "    \"hcyl\": {},", ide.hcyl);
    let _ = writeln!(json, "    \"current_lba\": {},", ide.current_lba());
    let _ = writeln!(json, "    \"total_sectors\": {},", ide.total_sectors());
    let _ = writeln!(json, "    \"sector_size\": 512,");
    let _ = writeln!(
        json,
        "    \"geometry_source\": {},",
        json_string(ide.geometry_source.as_str())
    );
    push_rdb_geometry_json(json, &ide.rdb_geometry);
    let _ = writeln!(json, "    \"cylinders\": {},", ide.cylinders);
    let _ = writeln!(json, "    \"heads\": {},", ide.heads);
    let _ = writeln!(
        json,
        "    \"sectors_per_track\": {},",
        ide.sectors_per_track
    );
    let _ = writeln!(
        json,
        "    \"data_direction\": {},",
        json_string(&format!("{:?}", ide.data_direction))
    );
    let _ = writeln!(json, "    \"data_index\": {},", ide.data_index);
    let _ = writeln!(json, "    \"data_buffer_len\": {}", ide.data_buffer.len());
    json.push_str("  },\n");
}

fn push_hdf_snapshot_json(json: &mut String, hdf_snapshot: Option<&HdfSnapshotEvidence>) {
    if let Some(snapshot) = hdf_snapshot {
        let _ = writeln!(json, "    \"hdf_snapshot\": {{");
        let _ = writeln!(json, "      \"path\": {},", json_string(&snapshot.path));
        let _ = writeln!(json, "      \"bytes\": {},", snapshot.bytes);
        let _ = writeln!(json, "      \"sha256\": {},", json_string(&snapshot.sha256));
        let _ = writeln!(
            json,
            "      \"source_sha256\": {},",
            json_string(&snapshot.source_sha256)
        );
        let _ = writeln!(json, "      \"dirty\": {},", snapshot.dirty);
        let _ = writeln!(json, "      \"changed_bytes\": {},", snapshot.changed_bytes);
        let _ = writeln!(
            json,
            "      \"changed_sectors\": {},",
            snapshot.changed_sectors
        );
        let _ = writeln!(json, "      \"sector_size\": {}", snapshot.sector_size);
        let _ = writeln!(json, "    }},");
    } else {
        let _ = writeln!(json, "    \"hdf_snapshot\": null,");
    }
}

fn push_rdb_geometry_json(json: &mut String, rdb: &rumiga_core::ide::RdbGeometry) {
    let _ = writeln!(json, "    \"rdb\": {{");
    let _ = writeln!(json, "      \"detected\": {},", rdb.detected);
    let _ = writeln!(json, "      \"usable\": {},", rdb.usable);
    let _ = writeln!(json, "      \"checksum_valid\": {},", rdb.checksum_valid);
    let _ = writeln!(json, "      \"block_index\": {},", rdb.block_index);
    let _ = writeln!(
        json,
        "      \"checksum_longwords\": {},",
        rdb.checksum_longwords
    );
    let _ = writeln!(
        json,
        "      \"block_size_bytes\": {},",
        rdb.block_size_bytes
    );
    let _ = writeln!(json, "      \"cylinders\": {},", rdb.cylinders);
    let _ = writeln!(json, "      \"heads\": {},", rdb.heads);
    let _ = writeln!(
        json,
        "      \"sectors_per_track\": {},",
        rdb.sectors_per_track
    );
    let _ = writeln!(json, "      \"declared_bytes\": {},", rdb.declared_bytes);
    let _ = writeln!(json, "      \"fits_in_image\": {}", rdb.fits_in_image);
    let _ = writeln!(json, "    }},");
}

fn network_status_from_emulator(
    network: &rumiga_api::NetworkConfig,
    emulator: &Emulator,
) -> rumiga_api::NetworkStatus {
    let a2065 = emulator.memory.a2065.borrow().status();
    rumiga_api::NetworkStatus {
        enabled: network.enabled(),
        device: network.device,
        backend: network.backend,
        mac_address: network.mac_address.clone(),
        a2065_present: a2065.enabled,
        a2065_configured: a2065.configured,
        a2065_shut_up: a2065.shut_up,
        a2065_base_address: a2065
            .base_address
            .map(|base_address| format!("0x{base_address:06X}")),
        a2065_card_mac_address: a2065.mac_address.to_colon_string(),
        link_up: a2065.link_up,
        counters: rumiga_api::NetworkPacketCounters {
            tx_packets: a2065.counters.tx_packets,
            rx_packets: a2065.counters.rx_packets,
            dropped_packets: a2065.counters.dropped_packets,
        },
    }
}

fn push_network_state_json(
    json: &mut String,
    network: &rumiga_api::NetworkConfig,
    emulator: &Emulator,
    pcap_path: Option<&str>,
) {
    let status = network_status_from_emulator(network, emulator);
    let _ = writeln!(json, "  \"network\": {{");
    let _ = writeln!(json, "    \"enabled\": {},", status.enabled);
    let _ = writeln!(
        json,
        "    \"device\": {},",
        json_string(status.device.as_str())
    );
    let _ = writeln!(
        json,
        "    \"backend\": {},",
        json_string(status.backend.as_str())
    );
    let _ = writeln!(
        json,
        "    \"mac_address\": {},",
        json_string(&status.mac_address)
    );
    if let Some(pcap_path) = pcap_path {
        let _ = writeln!(json, "    \"pcap\": {},", json_string(pcap_path));
    } else {
        let _ = writeln!(json, "    \"pcap\": null,");
    }
    let _ = writeln!(json, "    \"a2065_present\": {},", status.a2065_present);
    let _ = writeln!(
        json,
        "    \"a2065_configured\": {},",
        status.a2065_configured
    );
    let _ = writeln!(json, "    \"a2065_shut_up\": {},", status.a2065_shut_up);
    if let Some(base_address) = &status.a2065_base_address {
        let _ = writeln!(
            json,
            "    \"a2065_base_address\": {},",
            json_string(base_address)
        );
    } else {
        let _ = writeln!(json, "    \"a2065_base_address\": null,");
    }
    let _ = writeln!(
        json,
        "    \"a2065_card_mac_address\": {},",
        json_string(&status.a2065_card_mac_address)
    );
    let _ = writeln!(json, "    \"link_up\": {},", status.link_up);
    let _ = writeln!(json, "    \"tx_packets\": {},", status.counters.tx_packets);
    let _ = writeln!(json, "    \"rx_packets\": {},", status.counters.rx_packets);
    let _ = writeln!(
        json,
        "    \"dropped_packets\": {}",
        status.counters.dropped_packets
    );
    json.push_str("  },\n");
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct GitEvidence {
    sha: String,
    dirty: bool,
}

fn git_evidence() -> GitEvidence {
    GitEvidence {
        sha: git_output(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".into()),
        dirty: git_dirty(),
    }
}

fn git_dirty() -> bool {
    std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| !output.stdout.is_empty())
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    if value.is_empty() { None } else { Some(value) }
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

fn dominant_pixel(pixels: &[u16]) -> u16 {
    let mut counts = std::collections::HashMap::<u16, usize>::new();
    for &pixel in pixels {
        *counts.entry(pixel).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map_or(0, |(pixel, _)| pixel)
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

fn inspect_frame_edges(
    framebuffer: &[u16],
    width: usize,
    height: usize,
    first_lines: usize,
    edge_width: usize,
) -> Option<EdgeInspection> {
    let pixel_count = width.checked_mul(height)?;
    if width == 0 || height == 0 || first_lines == 0 || framebuffer.len() < pixel_count {
        return None;
    }

    let sampled_lines = first_lines.min(height);
    let edge_width = edge_width.min(width / 2);
    if edge_width == 0 {
        return None;
    }

    let background = dominant_pixel(&framebuffer[..pixel_count]);
    let mut left_non_background = 0usize;
    let mut right_non_background = 0usize;
    let mut mirrored_non_background = 0usize;
    let mut right_edge_wrapped_to_left = 0usize;
    let mut left_edge_wrapped_to_right = 0usize;
    let mut content_line_count = 0usize;
    let mut min_content_width = usize::MAX;
    let mut max_content_width = 0usize;

    for line in 0..sampled_lines {
        let line_start = line * width;
        let right_start = line_start + width - edge_width;
        let line_pixels = &framebuffer[line_start..line_start + width];
        let left_edge = &line_pixels[..edge_width];
        let right_edge = &line_pixels[width - edge_width..];
        for x in 0..edge_width {
            let left = framebuffer[line_start + x];
            let right = framebuffer[right_start + x];
            if left != background {
                left_non_background += 1;
            }
            if right != background {
                right_non_background += 1;
            }
            if left == right && left != background {
                mirrored_non_background += 1;
            }
        }
        right_edge_wrapped_to_left +=
            wrapped_suffix_to_prefix_pixels(left_edge, right_edge, background);
        left_edge_wrapped_to_right +=
            wrapped_suffix_to_prefix_pixels(right_edge, left_edge, background);

        if let Some(content_width) = line_content_width(line_pixels, background) {
            content_line_count += 1;
            min_content_width = min_content_width.min(content_width);
            max_content_width = max_content_width.max(content_width);
        }
    }

    if content_line_count == 0 {
        min_content_width = 0;
    }

    Some(EdgeInspection {
        first_lines,
        edge_width,
        sampled_lines,
        background_rgb565: background,
        left_non_background_pixels: left_non_background,
        right_non_background_pixels: right_non_background,
        mirrored_non_background_pixels: mirrored_non_background,
        right_edge_wrapped_to_left_pixels: right_edge_wrapped_to_left,
        left_edge_wrapped_to_right_pixels: left_edge_wrapped_to_right,
        content_line_count,
        min_content_width,
        max_content_width,
    })
}

fn wrapped_suffix_to_prefix_pixels(
    destination_edge: &[u16],
    source_edge: &[u16],
    background: u16,
) -> usize {
    let max_len = destination_edge.len().min(source_edge.len());
    for len in (1..=max_len).rev() {
        let destination_prefix = &destination_edge[..len];
        let source_suffix = &source_edge[source_edge.len() - len..];
        if destination_prefix == source_suffix
            && destination_prefix.iter().any(|&p| p != background)
        {
            return len;
        }
    }
    0
}

fn line_content_width(line_pixels: &[u16], background: u16) -> Option<usize> {
    let first = line_pixels.iter().position(|&pixel| pixel != background)?;
    let last = line_pixels.iter().rposition(|&pixel| pixel != background)?;
    Some(last - first + 1)
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

fn display_config_from_launch_args(args: &LaunchArgs) -> rumiga_api::DisplayConfig {
    let mut display = rumiga_api::DisplayConfig {
        scaling: args.scaling_mode.clone(),
        ..rumiga_api::DisplayConfig::default()
    };
    display.viewport.mode = args.viewport_mode.api_mode();
    display.viewport.preset = args.viewport_mode.api_preset();
    display.viewport.vertical_stretch = args.vertical_stretch;
    display
}

const fn presented_height(rect: ViewportRect, vertical_stretch: bool) -> usize {
    if vertical_stretch {
        rect.height * VERTICAL_STRETCH_FACTOR
    } else {
        rect.height
    }
}

fn resolve_viewport_rect(
    display: &rumiga_api::DisplayConfig,
    playfield: Option<&PlayfieldState>,
) -> ViewportRect {
    match &display.viewport.mode {
        rumiga_api::ViewportMode::Raw => ViewportRect::full_frame(),
        rumiga_api::ViewportMode::Auto => automatic_viewport_rect(&display.viewport, playfield),
        rumiga_api::ViewportMode::Manual => manual_viewport_rect(&display.viewport),
    }
}

fn automatic_viewport_rect(
    viewport: &rumiga_api::ViewportConfig,
    playfield: Option<&PlayfieldState>,
) -> ViewportRect {
    match viewport.preset {
        rumiga_api::ViewportPreset::NativeFullBorder | rumiga_api::ViewportPreset::Overscan => {
            ViewportRect::full_frame()
        }
        rumiga_api::ViewportPreset::VisibleArea => playfield
            .and_then(visible_area_viewport_rect)
            .unwrap_or_else(ViewportRect::full_frame),
        rumiga_api::ViewportPreset::AutoCenter => playfield
            .and_then(chipset_display_window_rect)
            .unwrap_or_else(ViewportRect::full_frame),
    }
}

fn chipset_display_window_rect(playfield: &PlayfieldState) -> Option<ViewportRect> {
    let (_, _, vstart, vstop) = playfield.display_window();
    let y_height = usize::from(vstop.saturating_sub(vstart)).min(HEIGHT);
    if y_height == 0 {
        return None;
    }

    // Keep the native horizontal span by default. WinUAE treats side-border
    // removal as a filter/viewport choice, while autoscale can keep aspect.
    Some(ViewportRect {
        x: 0,
        y: 0,
        width: WIDTH,
        height: y_height,
    })
}

fn visible_area_viewport_rect(playfield: &PlayfieldState) -> Option<ViewportRect> {
    let (hstart, hstop, vstart, vstop) = playfield.display_window();
    let x_start = active_x_for_hpos(hstart).min(WIDTH - 1);
    let x_end = active_x_for_hpos(hstop).min(WIDTH).max(x_start + 1);
    let y_height = usize::from(vstop.saturating_sub(vstart)).min(HEIGHT);
    if y_height == 0 || x_end <= x_start {
        return None;
    }

    Some(ViewportRect {
        x: x_start,
        y: 0,
        width: x_end - x_start,
        height: y_height,
    })
}

fn manual_viewport_rect(viewport: &rumiga_api::ViewportConfig) -> ViewportRect {
    let x = usize::try_from(viewport.x.max(0))
        .unwrap_or(0)
        .min(WIDTH - 1);
    let y = usize::try_from(viewport.y.max(0))
        .unwrap_or(0)
        .min(HEIGHT - 1);
    let width = usize::from(viewport.width).min(WIDTH - x).max(1);
    let height = usize::from(viewport.height).min(HEIGHT - y).max(1);
    ViewportRect {
        x,
        y,
        width,
        height,
    }
}

fn copy_presented_viewport(
    framebuffer: &[u16],
    width: usize,
    height: usize,
    rect: ViewportRect,
    output_height: usize,
    output: &mut [u16],
) -> bool {
    let Some(input_pixel_count) = width.checked_mul(height) else {
        return false;
    };
    let Some(output_pixel_count) = rect.width.checked_mul(output_height) else {
        return false;
    };
    if width == 0
        || height == 0
        || output_height == 0
        || rect.width == 0
        || rect.height == 0
        || rect.x_end() > width
        || rect.y_end() > height
        || framebuffer.len() < input_pixel_count
        || output.len() < output_pixel_count
    {
        return false;
    }

    for dest_y in 0..output_height {
        let source_y = rect.y + (dest_y * rect.height / output_height);
        let source_start = source_y * width + rect.x;
        let dest_start = dest_y * rect.width;
        output[dest_start..dest_start + rect.width]
            .copy_from_slice(&framebuffer[source_start..source_start + rect.width]);
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

    fn unique_temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rumiga-{label}-{}.hdf", std::process::id()))
    }

    fn unique_temp_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rumiga-{label}-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temporary test directory should be created");
        path
    }

    fn default_test_args() -> LaunchArgs {
        LaunchArgs {
            model: None,
            scale: DEFAULT_SCALE,
            scaling_mode: rumiga_api::ScalingMode::Integer,
            viewport_mode: ViewportMode::Auto,
            vertical_stretch: true,
            floppy_speed_percent: FLOPPY_SPEED_COMPATIBLE_PERCENT,
            rom_path: "kick.rom".to_owned(),
            adf_paths: Vec::new(),
            hdf_path: None,
            hdf_write_policy: rumiga_api::HdfWritePolicy::ReadOnly,
            hdf_snapshot_path: None,
            storage_root: None,
            upload_limit_bytes: DEFAULT_UPLOAD_LIMIT_BYTES,
            network: rumiga_api::NetworkConfig::default(),
            network_pcap_path: None,
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
            capture_kind: rumiga_api::ScreenshotKind::ViewportPresentation,
            mouse_scale_x: 0.5,
            mouse_scale_y: 1.0,
            audio_separation: 100,
        }
    }

    fn default_shared_state() -> SharedState {
        SharedState {
            running: true,
            fps: 50.0,
            model: "a1200".to_owned(),
            chip_ram_kb: 2048,
            slow_ram_kb: 0,
            fast_ram_kb: 0,
            rom_file: "kick.rom".to_owned(),
            floppy: [None, None, None, None],
            floppy_speed_percent: FLOPPY_SPEED_COMPATIBLE_PERCENT,
            hdf_path: None,
            hdf_write_policy: rumiga_api::HdfWritePolicy::ReadOnly,
            network: rumiga_api::NetworkConfig::default(),
            network_status: rumiga_api::NetworkStatus::default(),
            stereo_separation: 100,
            display: rumiga_api::DisplayConfig::default(),
            screenshot: vec![0xFF00_0000; 4],
            screenshot_width: 2,
            screenshot_height: 2,
            native_screenshot: vec![0xFF00_0000; WIDTH * HEIGHT],
            native_screenshot_width: u32::try_from(WIDTH).unwrap(),
            native_screenshot_height: u32::try_from(HEIGHT).unwrap(),
            pending_commands: Vec::new(),
        }
    }

    #[test]
    fn support_file_name_redacts_parent_paths() {
        assert_eq!(
            support_file_name("/Users/fabian/roms/kick.rom"),
            Some("kick.rom".to_owned())
        );
        assert_eq!(
            support_file_name("C:\\Amiga\\Workbench.adf"),
            Some("Workbench.adf".to_owned())
        );
        assert_eq!(support_file_name(""), None);
    }

    #[test]
    fn support_bundle_redacts_media_paths() {
        let mut state = default_shared_state();
        state.rom_file = "/Users/fabian/roms/kick.a1200.rom".to_owned();
        state.hdf_path = Some("/Users/fabian/disks/workbench.hdf".to_owned());
        state.floppy[0] = Some("/Users/fabian/disks/install.adf".to_owned());
        state.network_status = rumiga_api::NetworkStatus::from_config(&rumiga_api::NetworkConfig {
            backend: rumiga_api::NetworkBackend::Slirp,
            ..rumiga_api::NetworkConfig::default()
        });

        let bundle = support_bundle_from_state(&state);
        let json = serde_json::to_string(&bundle).expect("support bundle should serialize");

        assert_eq!(bundle.schema, SUPPORT_BUNDLE_SCHEMA_ID);
        assert_eq!(bundle.media.rom_name, Some("kick.a1200.rom".to_owned()));
        assert_eq!(bundle.media.hdf_name, Some("workbench.hdf".to_owned()));
        assert_eq!(bundle.media.floppies[0], Some("install.adf".to_owned()));
        assert!(bundle.screenshot.available);
        assert_eq!(
            bundle.screenshot.kind,
            rumiga_api::ScreenshotKind::ViewportPresentation
        );
        assert_eq!(bundle.screenshot.width, 2);
        assert_eq!(
            bundle.screenshot.native_width,
            u32::try_from(WIDTH).unwrap()
        );
        assert_eq!(
            bundle.screenshot.native_height,
            u32::try_from(HEIGHT).unwrap()
        );
        assert_eq!(bundle.screenshot.presentation_width, 2);
        assert_eq!(bundle.screenshot.presentation_height, 2);
        assert_eq!(
            bundle.screenshot.available_kinds,
            vec![
                rumiga_api::ScreenshotKind::ViewportPresentation,
                rumiga_api::ScreenshotKind::NativeFramebuffer,
            ]
        );
        assert!(!json.contains("/Users/fabian"));
    }

    #[test]
    fn desktop_api_endpoint_contract_matches_shared_contract() {
        assert_eq!(DESKTOP_API_ENDPOINTS, rumiga_api::API_ENDPOINTS);
    }

    #[tokio::test]
    async fn post_format_returns_versioned_unsupported_error_on_desktop() {
        let response = post_format(axum::Json(rumiga_api::FormatRequest {
            confirm_token: "CONFIRM".to_owned(),
        }))
        .await;

        assert_eq!(
            response.0["schema"],
            serde_json::Value::String(rumiga_api::API_RESPONSE_SCHEMA_ID.to_owned())
        );
        assert_eq!(
            response.0["version"],
            serde_json::Value::from(rumiga_api::API_RESPONSE_SCHEMA_VERSION)
        );
        assert_eq!(response.0["success"], false);
        assert_eq!(response.0["error_code"], "unsupported_on_desktop");
    }

    #[test]
    fn storage_errors_map_to_stable_http_responses() {
        for (error, expected_status, expected_code) in [
            (
                StorageError::InvalidPath,
                StatusCode::BAD_REQUEST,
                "invalid_storage_path",
            ),
            (
                StorageError::AccessDenied,
                StatusCode::FORBIDDEN,
                "storage_access_denied",
            ),
            (
                StorageError::NotFound,
                StatusCode::NOT_FOUND,
                "storage_entry_not_found",
            ),
            (
                StorageError::UnsupportedMediaType,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
            ),
            (
                StorageError::AlreadyExists,
                StatusCode::CONFLICT,
                "storage_entry_exists",
            ),
            (
                StorageError::UploadTooLarge { limit_bytes: 7 },
                StatusCode::PAYLOAD_TOO_LARGE,
                "upload_too_large",
            ),
        ] {
            let (status, Json(body)) = storage_error_response(&error);
            assert_eq!(status, expected_status);
            assert_eq!(body["schema"], rumiga_api::API_RESPONSE_SCHEMA_ID);
            assert_eq!(body["success"], false);
            assert_eq!(body["error_code"], expected_code);
        }
    }

    #[tokio::test]
    async fn rest_floppy_insert_is_confined_to_storage_root() {
        let root = unique_temp_directory("rest-storage");
        let disk = root.join("Workbench.adf");
        fs::write(&disk, [0_u8; 16]).unwrap();
        let state = AppState {
            machine: MachineState(Arc::new(Mutex::new(default_shared_state()))),
            media_store: MediaStore::new(&root, 1024).unwrap(),
        };

        let (status, Json(body)) = post_floppy_insert(
            State(state.clone()),
            Json(rumiga_api::FloppyInsertRequest {
                drive_idx: 0,
                path: "Workbench.adf".to_owned(),
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["success"], true);
        {
            let machine = state.machine.lock().unwrap();
            match machine.pending_commands.last().unwrap() {
                ApiCommand::InsertFloppy {
                    drive_idx,
                    path,
                    data,
                } => {
                    assert_eq!(*drive_idx, 0);
                    assert_eq!(Path::new(path), fs::canonicalize(&disk).unwrap());
                    assert_eq!(data, &[0_u8; 16]);
                }
                _ => panic!("expected an insert-floppy command"),
            }
        }

        let (status, Json(body)) = post_floppy_insert(
            State(state),
            Json(rumiga_api::FloppyInsertRequest {
                drive_idx: 0,
                path: "../outside.adf".to_owned(),
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error_code"], "invalid_storage_path");
        fs::remove_dir_all(root).unwrap();
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
    fn parse_args_accepts_scaling_mode() {
        let args = vec![
            "--scaling-mode".to_owned(),
            "aspect-fit".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                scaling_mode: rumiga_api::ScalingMode::AspectFit,
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_rejects_unsupported_scaling_mode() {
        let args = vec![
            "--scaling-mode".to_owned(),
            "nearest".to_owned(),
            "kick.rom".to_owned(),
        ];

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
    fn parse_args_accepts_named_viewport_preset() {
        let args = vec![
            "--viewport".to_owned(),
            "visible-area".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                viewport_mode: ViewportMode::VisibleArea,
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
    fn parse_args_defaults_hdf_to_read_only_policy() {
        let args = vec!["kick.rom".to_owned()];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                hdf_write_policy: rumiga_api::HdfWritePolicy::ReadOnly,
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_accepts_hdf_writeback_shortcut() {
        let args = vec![
            "--hdf".to_owned(),
            "system.hdf".to_owned(),
            "--hdf-writeback".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                hdf_path: Some("system.hdf".to_owned()),
                hdf_write_policy: rumiga_api::HdfWritePolicy::Writeback,
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_accepts_hdf_write_policy() {
        let args = vec![
            "--hdf-write-policy".to_owned(),
            "snapshot".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                hdf_write_policy: rumiga_api::HdfWritePolicy::ReadOnly,
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_accepts_hdf_snapshot_path() {
        let args = vec![
            "--hdf".to_owned(),
            "system.hdf".to_owned(),
            "--hdf-snapshot".to_owned(),
            "target/evidence/system-session.hdf".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                hdf_path: Some("system.hdf".to_owned()),
                hdf_snapshot_path: Some("target/evidence/system-session.hdf".to_owned()),
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_accepts_storage_policy() {
        let args = vec![
            "--storage-root".to_owned(),
            "media".to_owned(),
            "--upload-limit-mib".to_owned(),
            "512".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                storage_root: Some(PathBuf::from("media")),
                upload_limit_bytes: 512 * 1024 * 1024,
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_rejects_invalid_upload_limits() {
        for value in ["0", "8193", "invalid"] {
            let args = vec![
                "--upload-limit-mib".to_owned(),
                value.to_owned(),
                "kick.rom".to_owned(),
            ];
            assert!(parse_args(&args).is_err(), "limit {value} must be rejected");
        }
    }

    #[test]
    fn parse_args_rejects_hdf_snapshot_without_hdf() {
        let args = vec![
            "--hdf-snapshot".to_owned(),
            "target/evidence/system-session.hdf".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn parse_args_rejects_hdf_snapshot_over_source() {
        let args = vec![
            "--hdf".to_owned(),
            "system.hdf".to_owned(),
            "--hdf-snapshot".to_owned(),
            "system.hdf".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn parse_args_defaults_network_to_disabled() {
        let args = vec!["kick.rom".to_owned()];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                network: rumiga_api::NetworkConfig::default(),
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_accepts_slirp_network_with_explicit_mac() {
        let args = vec![
            "--network".to_owned(),
            "slirp".to_owned(),
            "--network-mac".to_owned(),
            "02:52:55:4D:49:48".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                network: rumiga_api::NetworkConfig {
                    backend: rumiga_api::NetworkBackend::Slirp,
                    mac_address: "02:52:55:4d:49:48".to_owned(),
                    ..rumiga_api::NetworkConfig::default()
                },
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_accepts_network_pcap_path() {
        let args = vec![
            "--network-slirp".to_owned(),
            "--network-pcap".to_owned(),
            "target/evidence/net/rumiga.pcap".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                network: rumiga_api::NetworkConfig {
                    backend: rumiga_api::NetworkBackend::Slirp,
                    ..rumiga_api::NetworkConfig::default()
                },
                network_pcap_path: Some("target/evidence/net/rumiga.pcap".to_owned()),
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_rejects_invalid_network_mac() {
        let args = vec![
            "--network-slirp".to_owned(),
            "--network-mac".to_owned(),
            "01:52:55:4d:49:48".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert!(parse_args(&args).is_err());
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
    fn parse_args_accepts_capture_kind() {
        let args = vec![
            "--capture".to_owned(),
            "evidence/native.png".to_owned(),
            "--capture-kind".to_owned(),
            "native-framebuffer".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                capture_path: Some("evidence/native.png".to_owned()),
                capture_kind: rumiga_api::ScreenshotKind::NativeFramebuffer,
                ..default_test_args()
            })
        );
    }

    #[test]
    fn parse_args_rejects_unsupported_capture_kind() {
        let args = vec![
            "--capture-kind".to_owned(),
            "host-window".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert!(parse_args(&args).is_err());
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
        let rect = ViewportRect {
            x: 0,
            y: 0,
            width: 320,
            height: 256,
        };
        assert_eq!(presented_height(rect, true), 512);
        assert_eq!(presented_height(rect, false), 256);
    }

    #[test]
    fn auto_viewport_uses_chipset_display_window_not_pixel_content() {
        let mut playfield = PlayfieldState::new();
        playfield.diwstrt = 0x1D81;
        playfield.diwstop = 0x38C1;
        let display = rumiga_api::DisplayConfig::default();

        let rect = resolve_viewport_rect(&display, Some(&playfield));

        assert_eq!(
            rect,
            ViewportRect {
                x: 0,
                y: 0,
                width: WIDTH,
                height: 283,
            }
        );
    }

    #[test]
    fn visible_area_preset_uses_active_horizontal_window() {
        let mut playfield = PlayfieldState::new();
        playfield.diwstrt = 0x1D81;
        playfield.diwstop = 0x38C1;
        let mut display = rumiga_api::DisplayConfig::default();
        display.viewport.preset = rumiga_api::ViewportPreset::VisibleArea;
        let (hstart, hstop, vstart, vstop) = playfield.display_window();

        let rect = resolve_viewport_rect(&display, Some(&playfield));

        assert_eq!(
            rect,
            ViewportRect {
                x: active_x_for_hpos(hstart),
                y: 0,
                width: active_x_for_hpos(hstop) - active_x_for_hpos(hstart),
                height: usize::from(vstop.saturating_sub(vstart)),
            }
        );
    }

    #[test]
    fn copy_presented_viewport_crops_and_doubles_lines() {
        let width = 2usize;
        let height = 4usize;
        let framebuffer = [
            10u16, 10, //
            20, 20, //
            30, 30, //
            40, 40,
        ];
        let mut output = [0u16; 8];

        assert!(copy_presented_viewport(
            &framebuffer,
            width,
            height,
            ViewportRect {
                x: 0,
                y: 1,
                width,
                height: 2,
            },
            height,
            &mut output
        ));

        assert_eq!(output, [20, 20, 20, 20, 30, 30, 30, 30]);
    }

    #[test]
    fn copy_presented_viewport_crops_horizontally() {
        let width = 4usize;
        let height = 2usize;
        let framebuffer = [
            10u16, 11, 12, 13, //
            20, 21, 22, 23,
        ];
        let mut output = [0u16; 4];

        assert!(copy_presented_viewport(
            &framebuffer,
            width,
            height,
            ViewportRect {
                x: 1,
                y: 0,
                width: 2,
                height: 2,
            },
            height,
            &mut output
        ));

        assert_eq!(output, [11, 12, 21, 22]);
    }

    #[test]
    fn copy_presented_viewport_line_doubles_full_frame() {
        let width = 2usize;
        let height = 2usize;
        let framebuffer = [
            10u16, 11, //
            20, 21,
        ];
        let mut output = [0u16; 8];

        assert!(copy_presented_viewport(
            &framebuffer,
            width,
            height,
            ViewportRect {
                x: 0,
                y: 0,
                width,
                height,
            },
            height * 2,
            &mut output
        ));

        assert_eq!(output, [10, 11, 10, 11, 20, 21, 20, 21]);
    }

    #[test]
    fn prepare_capture_frame_uses_presented_height() {
        let framebuffer = vec![0xFFFFu16; WIDTH * HEIGHT];
        let display = display_config_from_launch_args(&default_test_args());
        let frame =
            prepare_capture_frame(&framebuffer, &display, None).expect("valid frame buffer");

        assert_eq!(frame.width, WIDTH);
        assert_eq!(frame.height, HEIGHT * 2);
        assert_eq!(frame.pixels.len(), WIDTH * HEIGHT * 2);
    }

    #[test]
    fn prepare_capture_frame_for_native_kind_uses_full_framebuffer() {
        let mut framebuffer = vec![0x0000u16; WIDTH * HEIGHT];
        framebuffer[0] = 0x1234;
        framebuffer[WIDTH * HEIGHT - 1] = 0xABCD;
        let display = display_config_from_launch_args(&default_test_args());

        let frame = prepare_capture_frame_for_kind(
            &framebuffer,
            &display,
            None,
            &rumiga_api::ScreenshotKind::NativeFramebuffer,
        )
        .expect("native framebuffer should capture");

        assert_eq!(frame.width, WIDTH);
        assert_eq!(frame.height, HEIGHT);
        assert_eq!(frame.source_x_start, 0);
        assert_eq!(frame.source_x_end, WIDTH);
        assert_eq!(frame.source_y_start, 0);
        assert_eq!(frame.source_y_end, HEIGHT);
        assert_eq!(frame.pixels[0], 0x1234);
        assert_eq!(frame.pixels[WIDTH * HEIGHT - 1], 0xABCD);
    }

    #[test]
    #[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
    fn capture_manifest_contains_stable_schema_fields() {
        let config = MemoryConfig::a500();
        let emulator = Emulator::new(config.clone());
        let args = LaunchArgs {
            capture_frames: 42,
            ..default_test_args()
        };
        let display = display_config_from_launch_args(&args);
        let frame = CaptureFrame {
            pixels: vec![0x0000, 0xFFFF, 0x07E0, 0xF800],
            width: 2,
            height: 2,
            source_x_start: 0,
            source_x_end: 2,
            source_y_start: 0,
            source_y_end: 2,
        };
        let rom = FileEvidence {
            path: "kick.rom".to_owned(),
            bytes: 512 * 1024,
            sha256: "rom-hash".to_owned(),
        };
        let floppies: [Option<FileEvidence>; 4] = std::array::from_fn(|_| None);
        let image_path = Path::new("rumiga.png");
        let manifest_path =
            std::env::temp_dir().join(format!("rumiga-manifest-test-{}.json", std::process::id()));
        let context = CaptureManifestContext {
            image_path,
            frame: &frame,
            args: &args,
            display: &display,
            model: MachineModel::A500,
            config: &config,
            emulator: &emulator,
            rom: &rom,
            floppies: &floppies,
            hdf: None,
            hdf_snapshot: None,
        };

        write_capture_manifest(&manifest_path, &context).expect("manifest should write");
        let data = fs::read_to_string(&manifest_path).expect("manifest should be readable");
        let manifest: serde_json::Value =
            serde_json::from_str(&data).expect("manifest should be valid JSON");
        let _ = fs::remove_file(&manifest_path);

        assert_eq!(
            manifest["schema"]["id"],
            serde_json::Value::String(CAPTURE_MANIFEST_SCHEMA_ID.to_owned())
        );
        assert_eq!(
            manifest["schema"]["version"],
            serde_json::Value::from(CAPTURE_MANIFEST_SCHEMA_VERSION)
        );
        assert_eq!(manifest["producer"]["name"], "rumiga-desktop");
        assert_eq!(manifest["native_framebuffer"]["width"], WIDTH);
        assert_eq!(manifest["native_framebuffer"]["height"], HEIGHT);
        assert_eq!(
            manifest["boot_workarounds"]["forced_cia_timer_start"],
            false
        );
        assert_eq!(
            manifest["boot_workarounds"]["forced_cia_timer_start_count"],
            0
        );
        assert_eq!(manifest["boot_workarounds"]["rom_drive_step_patch"], false);
        assert_eq!(manifest["cia"]["a"]["timer_a"]["start_writes"], 0);
        assert_eq!(manifest["cia"]["a"]["timer_a"]["auto_start_writes"], 0);
        assert_eq!(manifest["cia"]["a"]["timer_a"]["underflows"], 0);
        assert_eq!(manifest["cia"]["a"]["register_writes"]["cra"]["count"], 0);
        assert_eq!(
            manifest["cia"]["a"]["register_writes"]["cra"]["last"],
            "0x00"
        );
        assert_eq!(manifest["cia"]["b"]["timer_b"]["start_writes"], 0);
        assert_eq!(manifest["cia"]["b"]["timer_b"]["auto_start_writes"], 0);
        assert_eq!(manifest["cia"]["b"]["timer_b"]["underflows"], 0);
        assert_eq!(manifest["cia"]["b"]["register_writes"]["crb"]["count"], 0);
        assert_eq!(manifest["gayle_ide"]["hdf_write_policy"], "read-only");
        assert_eq!(manifest["gayle_ide"]["host_writeback_enabled"], false);
        assert!(manifest["gayle_ide"]["hdf_snapshot"].is_null());
        assert_eq!(manifest["gayle_ide"]["geometry_source"], "none");
        assert_eq!(manifest["gayle_ide"]["sector_size"], 512);
        assert_eq!(manifest["gayle_ide"]["rdb"]["detected"], false);
        assert_eq!(manifest["gayle_ide"]["rdb"]["usable"], false);
        assert_eq!(manifest["gayle_ide"]["rdb"]["checksum_valid"], false);
        assert_manifest_network_defaults(&manifest);
        assert_eq!(manifest["viewport"]["source_width"], WIDTH);
        assert_eq!(manifest["viewport"]["preset"], "AutoCenter");
        assert_eq!(manifest["viewport"]["output_width"], 2);
        assert_eq!(
            manifest["presentation"]["capture_kind"],
            "viewport-presentation"
        );
        assert_eq!(manifest["presentation"]["scaling"], "Integer");
        assert_eq!(manifest["presentation"]["window_scale"], DEFAULT_SCALE);
        assert_eq!(manifest["presentation"]["orientation_landscape"], true);
        assert_eq!(
            manifest["edge_integrity"]["first_lines"],
            EDGE_INSPECTION_LINES
        );
        assert_eq!(
            manifest["edge_integrity"]["edge_width"],
            EDGE_INSPECTION_WIDTH
        );
        assert!(manifest["edge_integrity"]["right_edge_wrapped_to_left_pixels"].is_number());
        assert!(manifest["edge_integrity"]["left_edge_wrapped_to_right_pixels"].is_number());
        assert!(manifest["edge_integrity"]["content_line_count"].is_number());
        assert!(manifest["edge_integrity"]["min_content_width"].is_number());
        assert!(manifest["edge_integrity"]["max_content_width"].is_number());
        assert_eq!(manifest["run"]["frames"], 42);
        assert!(manifest["media"]["rom"]["sha256"].is_string());
    }

    fn assert_manifest_network_defaults(manifest: &serde_json::Value) {
        assert_eq!(manifest["network"]["enabled"], false);
        assert_eq!(manifest["network"]["device"], "a2065");
        assert_eq!(manifest["network"]["backend"], "disabled");
        assert_eq!(
            manifest["network"]["mac_address"],
            rumiga_api::DEFAULT_NETWORK_MAC_ADDRESS
        );
    }

    #[test]
    fn flush_dirty_hdf_keeps_source_file_read_only_by_default() {
        let path = unique_temp_path("hdf-readonly");
        fs::write(&path, [0x11u8; 512]).expect("temp HDF should be writable");

        let mut emulator = Emulator::new(MemoryConfig::a1200());
        emulator.insert_hdf(vec![0x22u8; 512]);
        emulator.memory.ide.borrow_mut().hdf_dirty = true;
        let args = LaunchArgs {
            hdf_path: Some(path.display().to_string()),
            hdf_write_policy: rumiga_api::HdfWritePolicy::ReadOnly,
            ..default_test_args()
        };
        let floppies = [None, None, None, None];

        flush_dirty_media(&mut emulator, &args, &floppies);

        assert_eq!(
            fs::read(&path).expect("temp HDF should remain"),
            [0x11u8; 512]
        );
        assert!(!emulator.hdf_dirty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn flush_dirty_hdf_persists_when_writeback_is_explicit() {
        let path = unique_temp_path("hdf-writeback");
        fs::write(&path, [0x11u8; 512]).expect("temp HDF should be writable");

        let mut emulator = Emulator::new(MemoryConfig::a1200());
        emulator.insert_hdf(vec![0x22u8; 512]);
        emulator.memory.ide.borrow_mut().hdf_dirty = true;
        let args = LaunchArgs {
            hdf_path: Some(path.display().to_string()),
            hdf_write_policy: rumiga_api::HdfWritePolicy::Writeback,
            ..default_test_args()
        };
        let floppies = [None, None, None, None];

        flush_dirty_media(&mut emulator, &args, &floppies);

        assert_eq!(
            fs::read(&path).expect("temp HDF should remain"),
            [0x22u8; 512]
        );
        assert!(!emulator.hdf_dirty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn hdf_diff_stats_counts_changed_bytes_and_sectors() {
        let source = [0u8; 1024];
        let mut snapshot = source;
        snapshot[10] = 1;
        snapshot[700] = 2;

        assert_eq!(
            hdf_diff_stats(&source, &snapshot, HDF_DIFF_SECTOR_SIZE),
            HdfDiffStats {
                changed_bytes: 2,
                changed_sectors: 2,
                sector_size: HDF_DIFF_SECTOR_SIZE,
            }
        );
    }

    #[test]
    fn hdf_snapshot_writes_copy_and_reports_diff_without_touching_source() {
        let source_path = unique_temp_path("hdf-snapshot-source");
        let snapshot_path = unique_temp_path("hdf-snapshot-copy");
        let source_data = [0x11u8; 1024];
        fs::write(&source_path, source_data).expect("source HDF should be writable");

        let mut session_data = source_data.to_vec();
        session_data[10] = 0x22;
        session_data[513] = 0x33;
        let mut emulator = Emulator::new(MemoryConfig::a1200());
        emulator.insert_hdf(session_data.clone());
        emulator.memory.ide.borrow_mut().hdf_dirty = true;
        let args = LaunchArgs {
            hdf_path: Some(source_path.display().to_string()),
            hdf_snapshot_path: Some(snapshot_path.display().to_string()),
            ..default_test_args()
        };

        let evidence = write_hdf_snapshot_if_requested(&emulator, &args)
            .expect("snapshot should write")
            .expect("snapshot evidence should exist");

        assert_eq!(
            fs::read(&source_path).expect("source should remain"),
            source_data
        );
        assert_eq!(
            fs::read(&snapshot_path).expect("snapshot should exist"),
            session_data
        );
        assert_eq!(evidence.bytes, session_data.len());
        assert_eq!(evidence.sha256, sha256_hex(&session_data));
        assert_eq!(evidence.source_sha256, sha256_hex(&source_data));
        assert!(evidence.dirty);
        assert_eq!(evidence.changed_bytes, 2);
        assert_eq!(evidence.changed_sectors, 2);
        assert_eq!(evidence.sector_size, HDF_DIFF_SECTOR_SIZE);

        let _ = fs::remove_file(source_path);
        let _ = fs::remove_file(snapshot_path);
    }

    #[test]
    fn edge_inspection_reports_clean_right_edge_without_left_mirror() {
        let width = 8usize;
        let height = 4usize;
        let mut framebuffer = vec![0u16; width * height];
        for line in 0..height {
            let start = line * width;
            framebuffer[start + 6] = 0x1234;
            framebuffer[start + 7] = 0x5678;
        }

        let report =
            inspect_frame_edges(&framebuffer, width, height, 20, 2).expect("valid edge report");

        assert_eq!(report.sampled_lines, height);
        assert_eq!(report.left_non_background_pixels, 0);
        assert_eq!(report.right_non_background_pixels, height * 2);
        assert_eq!(report.mirrored_non_background_pixels, 0);
        assert_eq!(report.right_edge_wrapped_to_left_pixels, 0);
        assert_eq!(report.left_edge_wrapped_to_right_pixels, 0);
        assert_eq!(report.content_line_count, height);
        assert_eq!(report.min_content_width, 2);
        assert_eq!(report.max_content_width, 2);
    }

    #[test]
    fn edge_inspection_detects_right_edge_pattern_mirrored_on_left() {
        let width = 8usize;
        let height = 4usize;
        let mut framebuffer = vec![0u16; width * height];
        for line in 0..3 {
            let start = line * width;
            framebuffer[start] = 0x1234;
            framebuffer[start + 1] = 0x5678;
            framebuffer[start + 6] = 0x1234;
            framebuffer[start + 7] = 0x5678;
        }

        let report =
            inspect_frame_edges(&framebuffer, width, height, 20, 2).expect("valid edge report");

        assert_eq!(report.left_non_background_pixels, 6);
        assert_eq!(report.right_non_background_pixels, 6);
        assert_eq!(report.mirrored_non_background_pixels, 6);
        assert_eq!(report.right_edge_wrapped_to_left_pixels, 6);
        assert_eq!(report.left_edge_wrapped_to_right_pixels, 6);
    }

    #[test]
    fn edge_inspection_detects_right_edge_suffix_wrapped_to_left_prefix() {
        let width = 10usize;
        let height = 3usize;
        let mut framebuffer = vec![0u16; width * height];
        for line in 0..height {
            let start = line * width;
            framebuffer[start] = 0x2222;
            framebuffer[start + 1] = 0x3333;
            framebuffer[start + 6] = 0x9999;
            framebuffer[start + 7] = 0x8888;
            framebuffer[start + 8] = 0x2222;
            framebuffer[start + 9] = 0x3333;
        }

        let report =
            inspect_frame_edges(&framebuffer, width, height, 20, 4).expect("valid edge report");

        assert_eq!(report.mirrored_non_background_pixels, 0);
        assert_eq!(report.right_edge_wrapped_to_left_pixels, height * 2);
        assert_eq!(report.left_edge_wrapped_to_right_pixels, 0);
    }

    #[test]
    fn edge_inspection_reports_stable_content_widths_across_first_lines() {
        let width = 12usize;
        let height = 4usize;
        let mut framebuffer = vec![0u16; width * height];
        for line in 0..height {
            let start = line * width;
            for x in 4..8 {
                framebuffer[start + x] = 0x7777;
            }
        }

        let report =
            inspect_frame_edges(&framebuffer, width, height, 20, 2).expect("valid edge report");

        assert_eq!(report.content_line_count, height);
        assert_eq!(report.min_content_width, 4);
        assert_eq!(report.max_content_width, 4);
    }

    #[test]
    fn prepare_capture_frame_keeps_bottom_line_inside_auto_viewport() {
        let mut framebuffer = vec![0u16; WIDTH * HEIGHT];
        let mut playfield = PlayfieldState::new();
        playfield.diwstrt = 0x1D81;
        playfield.diwstop = 0x38C1;
        let display = rumiga_api::DisplayConfig::default();
        let rect = resolve_viewport_rect(&display, Some(&playfield));
        framebuffer[(rect.height - 1) * WIDTH + 3] = 0x7BEF;

        let frame = prepare_capture_frame(&framebuffer, &display, Some(&playfield))
            .expect("valid frame buffer");
        let bottom_start = (frame.height - 1) * frame.width;

        assert_eq!(frame.pixels[bottom_start + 3], 0x7BEF);
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

    #[test]
    fn parse_args_accepts_mouse_scales() {
        let args = vec![
            "--mouse-scale-x".to_owned(),
            "0.25".to_owned(),
            "--mouse-scale-y".to_owned(),
            "1.5".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                mouse_scale_x: 0.25,
                mouse_scale_y: 1.5,
                ..default_test_args()
            })
        );

        // Reject negative scales
        let args2 = vec![
            "--mouse-scale-x".to_owned(),
            "-0.5".to_owned(),
            "kick.rom".to_owned(),
        ];
        assert!(parse_args(&args2).is_err());

        // Reject zero scales
        let args3 = vec![
            "--mouse-scale-y".to_owned(),
            "0.0".to_owned(),
            "kick.rom".to_owned(),
        ];
        assert!(parse_args(&args3).is_err());
    }

    #[test]
    fn parse_args_accepts_audio_separation() {
        let args = vec![
            "--audio-separation".to_owned(),
            "70".to_owned(),
            "kick.rom".to_owned(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(LaunchArgs {
                audio_separation: 70,
                ..default_test_args()
            })
        );

        // Reject > 100
        let args2 = vec![
            "--audio-separation".to_owned(),
            "101".to_owned(),
            "kick.rom".to_owned(),
        ];
        assert!(parse_args(&args2).is_err());
    }
}
