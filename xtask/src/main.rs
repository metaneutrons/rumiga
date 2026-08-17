// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod ci;
mod commit_policy;
mod compatibility;
mod governance;
mod supply_chain;

const BUILD_DIRECTORY: &str = "m0-008-firmware-build";
const EVIDENCE_DIRECTORY: &str = "m0-008-firmware-evidence";

/// Bootloader flash offset, pinned against `flasher_args.json` in
/// [`verify_flash_layout`]. The partition-table offset is configuration and is
/// read from the resolved `sdkconfig`.
const BOOTLOADER_OFFSET: usize = 0x2000;

/// Size of one partition-table entry in the binary layout.
const PARTITION_ENTRY_BYTES: usize = 32;

/// Magic prefix of a populated partition-table entry.
const PARTITION_ENTRY_MAGIC: [u8; 2] = [0xAA, 0x50];

/// Repository-owned product partition layout applied to the flashable image.
const PARTITION_LAYOUT_PATH: &str = "firmware/partitions.csv";

#[derive(Debug, Deserialize)]
struct ToolchainManifest {
    target: TargetConfiguration,
    portable_rust: PortableRustConfiguration,
    host: HostConfiguration,
    embedded_rust: EmbeddedRustConfiguration,
    esp_idf: EspIdfConfiguration,
    tools: ToolConfiguration,
    build: BuildConfiguration,
}

#[derive(Debug, Deserialize)]
struct PortableRustConfiguration {
    target: String,
    profiles: Vec<PortableRustProfile>,
}

#[derive(Debug, Deserialize)]
struct PortableRustProfile {
    name: String,
    packages: Vec<String>,
    release: bool,
    default_features: bool,
    features: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TargetConfiguration {
    board: String,
    soc: String,
    rust: String,
    physical_flash: String,
    configured_flash: String,
    physical_psram: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddedRustConfiguration {
    channel: String,
}

#[derive(Debug, Deserialize)]
struct HostConfiguration {
    rust: String,
    rust_msrv: String,
    node: String,
    npm: String,
}

#[derive(Debug, Deserialize)]
struct EspIdfConfiguration {
    version: String,
    commit: String,
}

#[derive(Debug, Deserialize)]
struct ToolConfiguration {
    ldproxy: String,
    espflash: String,
    cargo_audit: String,
    cargo_deny: String,
}

#[derive(Debug, Deserialize)]
struct BuildConfiguration {
    evidence_schema: String,
    profile: String,
}

#[derive(Debug, Serialize)]
struct EvidenceManifest {
    schema: String,
    source_revision: String,
    source_date_epoch: u64,
    source_dirty: bool,
    board: String,
    soc: String,
    target: String,
    profile: String,
    elf: ElfMetadata,
    board_configuration: BoardConfigurationEvidence,
    reversibility: ReversibilityEvidence,
    merged_image: MergedImageEvidence,
    esp_idf: EspIdfEvidence,
    tools: ToolEvidence,
    inputs: InputEvidence,
    artifacts: Vec<ArtifactEvidence>,
    claims: Vec<String>,
    exclusions: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ElfMetadata {
    class_bits: u8,
    endianness: String,
    machine: String,
    file_type: String,
    linkage: String,
    compressed_instructions: bool,
    float_abi: String,
}

#[derive(Debug, Serialize)]
struct BoardConfigurationEvidence {
    idf_target: String,
    runtime_flash_mode: String,
    bootloader_flash_mode: String,
    declared_physical_flash: String,
    configured_flash_geometry: String,
    flash_frequency: String,
    partition_table_offset: String,
    declared_physical_psram: String,
    psram_mode: String,
    psram_speed_mhz: u32,
    psram_allocation: String,
    psram_internal_threshold_bytes: u32,
    psram_reserved_internal_bytes: u32,
    cpu_frequency_mhz: u32,
    silicon_revision_min_encoded: u32,
    silicon_revision_max_encoded: u32,
    l2_cache_bytes: u32,
    l2_cache_line_bytes: u32,
    freertos_hz: u32,
    main_task_stack_bytes: u32,
    console: String,
}

/// Evidence that the merged flash image carries the ESP-IDF bootloader and the
/// repository-owned product partition layout rather than image-tool defaults.
#[derive(Debug, Serialize)]
struct MergedImageEvidence {
    bytes: u64,
    bootloader_offset: String,
    bootloader_bytes: u64,
    bootloader_window_bytes: u64,
    bootloader_region_sha256: String,
    partition_table_offset: String,
    partition_table_source: String,
    partition_table_region_sha256: String,
    app_partition: String,
    app_offset: String,
    app_image_bytes: u64,
    app_partition_bytes: u32,
    partitions: Vec<PartitionEvidence>,
}

/// Evidence that the configuration cannot make a device permanently different.
///
/// Every eFuse-burning feature must be paired with virtual eFuses, and release
/// mode is rejected outright, so no build produced from this repository can
/// consume a board.
#[derive(Debug, Serialize)]
struct ReversibilityEvidence {
    flash_encryption: String,
    secure_boot: String,
    nvs_encryption: String,
    efuse_virtual: bool,
    burns_efuses: bool,
}

/// One decoded partition-table entry.
#[derive(Debug, Serialize)]
struct PartitionEvidence {
    label: String,
    kind: String,
    subtype: String,
    offset: String,
    bytes: u32,
}

#[derive(Debug, Serialize)]
struct EspIdfEvidence {
    version: String,
    expected_commit: String,
    checkout_commit: String,
    checkout_clean: bool,
    submodule_count: usize,
    submodule_status_sha256: String,
}

#[derive(Debug, Serialize)]
struct ToolEvidence {
    rust_channel: String,
    rustc: String,
    cargo: String,
    gcc: String,
    ldproxy: String,
    espflash: String,
}

#[derive(Debug, Serialize)]
struct InputEvidence {
    cargo_lock_sha256: String,
    sdkconfig_defaults_sha256: String,
}

#[derive(Debug, Serialize)]
struct ArtifactEvidence {
    name: String,
    role: String,
    bytes: u64,
    sha256: String,
}

struct BuildOutputs {
    elf: PathBuf,
    map: PathBuf,
    sdkconfig: PathBuf,
    flasher_args: PathBuf,
    bootloader: PathBuf,
    partition_table: PathBuf,
    cmake_cache: PathBuf,
}

fn main() -> Result<()> {
    let mut arguments = env::args().skip(1);
    match arguments.next().as_deref() {
        Some("ci") => {
            let arguments = arguments.collect::<Vec<_>>();
            ci::run(&arguments)
        }
        Some("commit-policy") => {
            let arguments = arguments.collect::<Vec<_>>();
            commit_policy::run(&arguments)
        }
        Some("firmware-evidence") if arguments.next().is_none() => build_firmware_evidence(),
        Some("compatibility-evidence") if arguments.next().is_none() => {
            compatibility::build_evidence()
        }
        Some("governance-evidence") if arguments.next().is_none() => governance::build_evidence(),
        Some("supply-chain-evidence") if arguments.next().is_none() => {
            supply_chain::build_evidence()
        }
        _ => bail!(
            "usage: cargo xtask <ci|commit-policy|compatibility-evidence|firmware-evidence|governance-evidence|supply-chain-evidence> [options]"
        ),
    }
}

fn build_firmware_evidence() -> Result<()> {
    let root = workspace_root()?;
    let manifest = read_toolchain_manifest(&root)?;
    ensure!(
        manifest.build.profile == "release",
        "M0-008 requires release"
    );

    let target_root = root.join("target");
    let build_root = target_root.join(BUILD_DIRECTORY);
    let evidence_root = target_root.join(EVIDENCE_DIRECTORY);
    reset_generated_directory(&target_root, &build_root)?;
    reset_generated_directory(&target_root, &evidence_root)?;

    let source_revision = capture_git(&root, &["rev-parse", "HEAD"])?;
    let source_date_epoch = capture_git(&root, &["show", "-s", "--format=%ct", "HEAD"])?
        .parse::<u64>()
        .context("git commit timestamp must be an unsigned integer")?;
    let source_dirty = !capture_git(&root, &["status", "--porcelain"])?.is_empty();
    verify_ci_source(&source_revision, source_dirty)?;

    let tool_evidence = verify_declared_tools(&root, &manifest)?;
    run_firmware_build(&root, &build_root, &manifest, source_date_epoch)?;
    let outputs = locate_build_outputs(&build_root, &manifest.target.rust)?;
    let elf_metadata = inspect_elf(&outputs.elf)?;
    verify_final_link_map(&outputs.map)?;
    let board_configuration =
        verify_board_configuration(&outputs.sdkconfig, &outputs.flasher_args, &manifest)?;
    let reversibility = verify_reversible_security_posture(&read_sdkconfig(&outputs.sdkconfig)?)?;

    let (idf_evidence, gcc_version, size_tool) =
        verify_native_build_inputs(&outputs.cmake_cache, &manifest)?;
    let tool_evidence = ToolEvidence {
        gcc: gcc_version,
        ..tool_evidence
    };

    let mut artifacts = package_outputs(
        &root,
        &evidence_root,
        &outputs,
        &manifest,
        &size_tool,
        &board_configuration,
    )?;
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));
    let merged_image = verify_merged_flash_image(
        &root,
        &evidence_root.join("rumiga-firmware.flash.bin"),
        &outputs,
        parse_partition_number(&board_configuration.partition_table_offset)? as usize,
    )?;

    let evidence = EvidenceManifest {
        schema: manifest.build.evidence_schema,
        source_revision,
        source_date_epoch,
        source_dirty,
        board: manifest.target.board,
        soc: manifest.target.soc,
        target: manifest.target.rust,
        profile: manifest.build.profile,
        elf: elf_metadata,
        board_configuration,
        reversibility,
        merged_image,
        esp_idf: idf_evidence,
        tools: tool_evidence,
        inputs: InputEvidence {
            cargo_lock_sha256: sha256_file(&root.join("Cargo.lock"))?,
            sdkconfig_defaults_sha256: sha256_file(&root.join("firmware/sdkconfig.defaults"))?,
        },
        artifacts,
        claims: vec![
            "compile-and-link".to_owned(),
            "esp32p4-image-generation".to_owned(),
            "pinned-idf-source".to_owned(),
        ],
        exclusions: vec![
            "not-flashed".to_owned(),
            "not-boot-tested".to_owned(),
            "no-peripheral-hil".to_owned(),
            "no-performance-claim".to_owned(),
            "no-efuse-burn".to_owned(),
            "encryption-not-enforced".to_owned(),
        ],
    };
    write_manifest_and_checksums(&evidence_root, &evidence)?;

    println!("firmware evidence: {}", evidence_root.display());
    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .context("xtask must be a direct workspace child")
}

fn read_toolchain_manifest(root: &Path) -> Result<ToolchainManifest> {
    let path = root.join("toolchain/manifest.toml");
    let contents =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
}

fn reset_generated_directory(target_root: &Path, path: &Path) -> Result<()> {
    ensure!(
        path.starts_with(target_root),
        "generated path escaped target"
    );
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))
}

fn verify_ci_source(source_revision: &str, source_dirty: bool) -> Result<()> {
    if let Ok(expected_revision) = env::var("GITHUB_SHA") {
        ensure!(
            expected_revision == source_revision,
            "GITHUB_SHA does not match checked-out HEAD"
        );
    }
    if env::var("CI").is_ok_and(|value| value == "true") {
        ensure!(
            !source_dirty,
            "CI evidence requires a clean tracked worktree"
        );
    }
    Ok(())
}

fn verify_declared_tools(root: &Path, manifest: &ToolchainManifest) -> Result<ToolEvidence> {
    ensure!(
        manifest.embedded_rust.channel.starts_with("nightly-"),
        "embedded Rust channel must be date-pinned"
    );
    let rustc = capture(
        root,
        "rustup",
        &[
            "run",
            &manifest.embedded_rust.channel,
            "rustc",
            "--version",
            "--verbose",
        ],
    )?;
    let release = rustc
        .lines()
        .find_map(|line| line.strip_prefix("release: "))
        .context("rustc evidence does not contain a release")?;
    ensure!(
        release.ends_with("-nightly"),
        "embedded rustc is not nightly"
    );
    let commit_hash = rustc
        .lines()
        .find_map(|line| line.strip_prefix("commit-hash: "))
        .context("rustc evidence does not contain a commit hash")?;
    ensure!(
        commit_hash.len() == 40 && commit_hash.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "embedded rustc commit hash is invalid"
    );
    let cargo = capture(
        root,
        "rustup",
        &["run", &manifest.embedded_rust.channel, "cargo", "--version"],
    )?;
    ensure!(cargo.contains("-nightly"), "embedded cargo is not nightly");
    let espflash = capture(root, "espflash", &["--version"])?;
    ensure!(
        espflash == format!("espflash {}", manifest.tools.espflash),
        "espflash does not match toolchain manifest"
    );
    let installed_tools = capture(root, "cargo", &["install", "--list"])?;
    let ldproxy_pin = format!("ldproxy v{}:", manifest.tools.ldproxy);
    ensure!(
        installed_tools.lines().any(|line| line == ldproxy_pin),
        "ldproxy does not match toolchain manifest"
    );

    Ok(ToolEvidence {
        rust_channel: manifest.embedded_rust.channel.clone(),
        rustc,
        cargo,
        gcc: String::new(),
        ldproxy: manifest.tools.ldproxy.clone(),
        espflash,
    })
}

fn run_firmware_build(
    root: &Path,
    build_root: &Path,
    manifest: &ToolchainManifest,
    source_date_epoch: u64,
) -> Result<()> {
    let mut command = Command::new("rustup");
    command
        .current_dir(root.join("firmware"))
        .args(["run", &manifest.embedded_rust.channel, "cargo", "build"])
        .args([
            "--locked",
            "--release",
            "--target",
            &manifest.target.rust,
            "--bin",
            "rumiga-firmware",
        ])
        .env("CARGO_TARGET_DIR", build_root)
        .env("SOURCE_DATE_EPOCH", source_date_epoch.to_string())
        .env_remove("CARGO_BUILD_RUSTC_WRAPPER")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("IDF_PATH")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("RUSTFLAGS")
        .stdin(Stdio::null());
    run_checked(&mut command, "ESP32-P4 firmware build")
}

fn locate_build_outputs(build_root: &Path, target: &str) -> Result<BuildOutputs> {
    let release_root = build_root.join(target).join("release");
    let build_scripts = release_root.join("build");
    let mut candidates = Vec::new();
    for entry in fs::read_dir(&build_scripts)
        .with_context(|| format!("failed to read {}", build_scripts.display()))?
    {
        let path = entry?.path();
        let is_esp_idf_sys = path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with("esp-idf-sys-"));
        if is_esp_idf_sys && path.join("out/build/libespidf.map").is_file() {
            candidates.push(path.join("out"));
        }
    }
    ensure!(
        candidates.len() == 1,
        "expected one esp-idf-sys build output, found {}",
        candidates.len()
    );
    let out = candidates.pop().context("esp-idf-sys output disappeared")?;
    let outputs = BuildOutputs {
        elf: release_root.join("rumiga-firmware"),
        map: out.join("build/libespidf.map"),
        sdkconfig: out.join("sdkconfig"),
        flasher_args: out.join("build/flasher_args.json"),
        bootloader: release_root.join("bootloader.bin"),
        partition_table: release_root.join("partition-table.bin"),
        cmake_cache: out.join("build/CMakeCache.txt"),
    };
    for path in [
        &outputs.elf,
        &outputs.map,
        &outputs.sdkconfig,
        &outputs.flasher_args,
        &outputs.bootloader,
        &outputs.partition_table,
        &outputs.cmake_cache,
    ] {
        ensure!(path.is_file(), "missing build output {}", path.display());
    }
    Ok(outputs)
}

fn inspect_elf(path: &Path) -> Result<ElfMetadata> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    inspect_elf_data(&bytes)
}

fn inspect_elf_data(bytes: &[u8]) -> Result<ElfMetadata> {
    ensure!(bytes.len() >= 52, "firmware ELF header is truncated");
    ensure!(&bytes[..4] == b"\x7fELF", "firmware output is not ELF");
    ensure!(bytes[4] == 1, "firmware ELF is not 32-bit");
    ensure!(bytes[5] == 1, "firmware ELF is not little-endian");
    ensure!(read_u16(bytes, 16)? == 2, "firmware ELF is not executable");
    ensure!(read_u16(bytes, 18)? == 243, "firmware ELF is not RISC-V");

    let flags = read_u32(bytes, 36)?;
    let program_offset = usize::try_from(read_u32(bytes, 28)?)?;
    let program_entry_size = usize::from(read_u16(bytes, 42)?);
    let program_count = usize::from(read_u16(bytes, 44)?);
    ensure!(program_entry_size >= 32, "invalid ELF program header size");
    let mut dynamically_linked = false;
    for index in 0..program_count {
        let offset = program_offset
            .checked_add(
                index
                    .checked_mul(program_entry_size)
                    .context("ELF overflow")?,
            )
            .context("ELF overflow")?;
        dynamically_linked |= matches!(read_u32(bytes, offset)?, 2 | 3);
    }
    ensure!(!dynamically_linked, "firmware ELF is dynamically linked");
    ensure!(
        flags & 0x6 == 0x2,
        "firmware ELF does not use single-float ABI"
    );

    Ok(ElfMetadata {
        class_bits: 32,
        endianness: "little".to_owned(),
        machine: "RISC-V".to_owned(),
        file_type: "executable".to_owned(),
        linkage: "static".to_owned(),
        compressed_instructions: flags & 0x1 != 0,
        float_abi: "single".to_owned(),
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset.checked_add(2).context("ELF offset overflow")?;
    let value = bytes.get(offset..end).context("ELF read exceeds file")?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset.checked_add(4).context("ELF offset overflow")?;
    let value = bytes.get(offset..end).context("ELF read exceeds file")?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn verify_final_link_map(path: &Path) -> Result<()> {
    let contents = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    ensure!(
        contents
            .windows(b"rumiga_firmware-".len())
            .any(|window| window == b"rumiga_firmware-"),
        "linker map does not describe the final Rust firmware link"
    );
    Ok(())
}

fn verify_native_build_inputs(
    cmake_cache: &Path,
    manifest: &ToolchainManifest,
) -> Result<(EspIdfEvidence, String, PathBuf)> {
    let target = cmake_value(cmake_cache, "IDF_TARGET")?;
    ensure!(target == manifest.target.soc, "CMake IDF target drifted");

    let idf_path = PathBuf::from(cmake_value(cmake_cache, "esp-idf_SOURCE_DIR")?);
    let checkout_commit = capture_git(&idf_path, &["rev-parse", "HEAD"])?;
    ensure!(
        checkout_commit == manifest.esp_idf.commit,
        "ESP-IDF checkout does not match the declared commit"
    );
    let checkout_status = capture_git(
        &idf_path,
        &["status", "--porcelain", "--untracked-files=no"],
    )?;
    ensure!(checkout_status.is_empty(), "ESP-IDF checkout is dirty");
    let submodule_status = capture_git(&idf_path, &["submodule", "status", "--recursive"])?;
    let submodule_count = submodule_status.lines().count();
    ensure!(submodule_count > 0, "ESP-IDF submodule graph is empty");
    for line in submodule_status.lines() {
        ensure!(
            line.starts_with(' '),
            "ESP-IDF submodule is missing, modified, or conflicted: {line}"
        );
    }

    let compiler_ar = PathBuf::from(cmake_value(cmake_cache, "CMAKE_C_COMPILER_AR")?);
    let compiler_name = compiler_ar
        .file_name()
        .and_then(OsStr::to_str)
        .and_then(|name| name.strip_suffix("-ar"))
        .context("failed to derive ESP-IDF compiler from CMake cache")?;
    let compiler = compiler_ar.with_file_name(compiler_name);
    let size_tool = compiler_ar.with_file_name("riscv32-esp-elf-size");
    ensure!(
        compiler.is_file(),
        "missing compiler {}",
        compiler.display()
    );
    ensure!(
        size_tool.is_file(),
        "missing size tool {}",
        size_tool.display()
    );
    let gcc_version = capture_path(
        cmake_cache.parent().context("CMake cache has no parent")?,
        &compiler,
        &["--version"],
    )?;

    Ok((
        EspIdfEvidence {
            version: manifest.esp_idf.version.clone(),
            expected_commit: manifest.esp_idf.commit.clone(),
            checkout_commit,
            checkout_clean: true,
            submodule_count,
            submodule_status_sha256: sha256_bytes(submodule_status.as_bytes()),
        },
        gcc_version,
        size_tool,
    ))
}

fn cmake_value(path: &Path, key: &str) -> Result<String> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let prefix = format!("{key}:");
    contents
        .lines()
        .find(|line| line.starts_with(&prefix))
        .and_then(|line| line.split_once('='))
        .map(|(_, value)| value.to_owned())
        .with_context(|| format!("CMake cache does not define {key}"))
}

fn verify_board_configuration(
    sdkconfig_path: &Path,
    flasher_args_path: &Path,
    manifest: &ToolchainManifest,
) -> Result<BoardConfigurationEvidence> {
    let config = read_sdkconfig(sdkconfig_path)?;
    let idf_target = config_string(&config, "CONFIG_IDF_TARGET")?;
    ensure!(
        idf_target == manifest.target.soc,
        "sdkconfig target does not match the board manifest"
    );

    for key in [
        "CONFIG_ESPTOOLPY_FLASHMODE_QIO",
        "CONFIG_ESPTOOLPY_FLASHFREQ_80M",
        "CONFIG_ESPTOOLPY_FLASHSIZE_16MB",
        "CONFIG_COMPILER_OPTIMIZATION_PERF",
        "CONFIG_ESP32P4_SELECTS_REV_LESS_V3",
        "CONFIG_ESP32P4_REV_MIN_100",
        "CONFIG_SPIRAM",
        "CONFIG_SPIRAM_BOOT_INIT",
        "CONFIG_SPIRAM_MODE_HEX",
        "CONFIG_SPIRAM_SPEED_200M",
        "CONFIG_SPIRAM_USE_MALLOC",
        "CONFIG_CACHE_L2_CACHE_256KB",
        "CONFIG_CACHE_L2_CACHE_LINE_128B",
        "CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ_400",
        "CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG",
    ] {
        ensure_config(&config, key, "y")?;
    }

    let bootloader_flash_mode = config_string(&config, "CONFIG_ESPTOOLPY_FLASHMODE")?;
    let flash_size = config_string(&config, "CONFIG_ESPTOOLPY_FLASHSIZE")?;
    let flash_frequency = config_string(&config, "CONFIG_ESPTOOLPY_FLASHFREQ")?;
    let partition_table_offset = config_offset(&config, "CONFIG_PARTITION_TABLE_OFFSET")?;
    ensure!(
        partition_table_offset > BOOTLOADER_OFFSET,
        "partition table must follow the bootloader"
    );
    ensure!(
        bootloader_flash_mode == "dio",
        "ESP-IDF must flash the QIO bootloader in DIO mode"
    );
    ensure!(
        flash_size == manifest.target.configured_flash,
        "configured flash geometry does not match the board manifest"
    );
    ensure!(
        flash_frequency == "80m",
        "D1001 flash frequency must be 80MHz"
    );

    verify_flash_layout(
        flasher_args_path,
        &bootloader_flash_mode,
        &flash_size,
        &flash_frequency,
        partition_table_offset,
    )?;

    ensure_config(&config, "CONFIG_SPIRAM_SPEED", "200")?;
    ensure_config(&config, "CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL", "8192")?;
    ensure_config(&config, "CONFIG_SPIRAM_MALLOC_RESERVE_INTERNAL", "65536")?;
    ensure_config(&config, "CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ", "400")?;
    ensure_config(&config, "CONFIG_ESP32P4_REV_MIN_FULL", "100")?;
    ensure_config(&config, "CONFIG_ESP32P4_REV_MAX_FULL", "199")?;
    ensure_config(&config, "CONFIG_CACHE_L2_CACHE_SIZE", "0x40000")?;
    ensure_config(&config, "CONFIG_CACHE_L2_CACHE_LINE_SIZE", "128")?;
    ensure_config(&config, "CONFIG_FREERTOS_HZ", "1000")?;
    ensure_config(&config, "CONFIG_MAIN_TASK_STACK_SIZE", "16384")?;
    ensure_config(&config, "CONFIG_ESP_MAIN_TASK_STACK_SIZE", "16384")?;

    Ok(BoardConfigurationEvidence {
        idf_target,
        runtime_flash_mode: "qio".to_owned(),
        bootloader_flash_mode,
        declared_physical_flash: manifest.target.physical_flash.clone(),
        configured_flash_geometry: flash_size,
        flash_frequency,
        partition_table_offset: format!("{partition_table_offset:#x}"),
        declared_physical_psram: manifest.target.physical_psram.clone(),
        psram_mode: "hex".to_owned(),
        psram_speed_mhz: config_u32(&config, "CONFIG_SPIRAM_SPEED")?,
        psram_allocation: "malloc".to_owned(),
        psram_internal_threshold_bytes: config_u32(&config, "CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL")?,
        psram_reserved_internal_bytes: config_u32(
            &config,
            "CONFIG_SPIRAM_MALLOC_RESERVE_INTERNAL",
        )?,
        cpu_frequency_mhz: config_u32(&config, "CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ")?,
        silicon_revision_min_encoded: config_u32(&config, "CONFIG_ESP32P4_REV_MIN_FULL")?,
        silicon_revision_max_encoded: config_u32(&config, "CONFIG_ESP32P4_REV_MAX_FULL")?,
        l2_cache_bytes: 0x40000,
        l2_cache_line_bytes: config_u32(&config, "CONFIG_CACHE_L2_CACHE_LINE_SIZE")?,
        freertos_hz: config_u32(&config, "CONFIG_FREERTOS_HZ")?,
        main_task_stack_bytes: config_u32(&config, "CONFIG_MAIN_TASK_STACK_SIZE")?,
        console: "usb-serial-jtag".to_owned(),
    })
}

fn verify_flash_layout(
    path: &Path,
    mode: &str,
    size: &str,
    frequency: &str,
    partition_table_offset: usize,
) -> Result<()> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let layout: serde_json::Value = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    for (pointer, expected) in [
        ("/flash_settings/flash_mode", mode),
        ("/flash_settings/flash_size", size),
        ("/flash_settings/flash_freq", frequency),
    ] {
        ensure!(
            json_string(&layout, pointer)? == expected,
            "flash layout value at {pointer} does not match sdkconfig"
        );
    }
    for (offset, expected_file) in [
        (BOOTLOADER_OFFSET, "bootloader/bootloader.bin"),
        (
            partition_table_offset,
            "partition_table/partition-table.bin",
        ),
    ] {
        let pointer = format!("/flash_files/{offset:#x}");
        ensure!(
            json_string(&layout, &pointer)? == expected_file,
            "unexpected flash layout entry at {offset:#x}"
        );
    }
    // The application offset belongs to the ESP-IDF build's own table, which is
    // a build artifact; the shipped layout comes from PARTITION_LAYOUT_PATH.
    let files = layout
        .pointer("/flash_files")
        .and_then(serde_json::Value::as_object)
        .context("flash layout has no flash_files object")?;
    ensure!(
        files
            .values()
            .filter_map(serde_json::Value::as_str)
            .any(|file| file == "libespidf.bin"),
        "flash layout declares no application image"
    );
    Ok(())
}

/// Translate the ESP-IDF flash mode spelling to the espflash CLI value.
fn espflash_flash_mode(value: &str) -> Result<&'static str> {
    match value {
        "dio" => Ok("dio"),
        "dout" => Ok("dout"),
        "qio" => Ok("qio"),
        "qout" => Ok("qout"),
        other => bail!("unsupported ESP-IDF flash mode {other}"),
    }
}

/// Translate the ESP-IDF flash size spelling to the espflash CLI value.
fn espflash_flash_size(value: &str) -> Result<&'static str> {
    match value {
        "2MB" => Ok("2mb"),
        "4MB" => Ok("4mb"),
        "8MB" => Ok("8mb"),
        "16MB" => Ok("16mb"),
        "32MB" => Ok("32mb"),
        other => bail!("unsupported ESP-IDF flash size {other}"),
    }
}

/// Translate the ESP-IDF flash frequency spelling to the espflash CLI value.
fn espflash_flash_frequency(value: &str) -> Result<&'static str> {
    match value {
        "20m" => Ok("20mhz"),
        "26m" => Ok("26mhz"),
        "40m" => Ok("40mhz"),
        "80m" => Ok("80mhz"),
        other => bail!("unsupported ESP-IDF flash frequency {other}"),
    }
}

/// Verify that the merged image embeds the ESP-IDF bootloader, partition table,
/// and application, and that the application fits its declared partition.
///
/// Comparing the bootloader region byte for byte also pins the image header, so
/// the merged image cannot silently carry a different flash geometry than the
/// resolved `sdkconfig`.
fn verify_merged_flash_image(
    root: &Path,
    image_path: &Path,
    outputs: &BuildOutputs,
    partition_table_offset: usize,
) -> Result<MergedImageEvidence> {
    let image =
        fs::read(image_path).with_context(|| format!("failed to read {}", image_path.display()))?;
    let bootloader = fs::read(&outputs.bootloader)
        .with_context(|| format!("failed to read {}", outputs.bootloader.display()))?;

    let window = partition_table_offset - BOOTLOADER_OFFSET;
    ensure!(
        bootloader.len() <= window,
        "bootloader of {} bytes does not fit its {window} byte window; raise CONFIG_PARTITION_TABLE_OFFSET",
        bootloader.len()
    );
    let bootloader_region = image_region(&image, BOOTLOADER_OFFSET, bootloader.len())?;
    ensure!(
        bootloader_region == bootloader.as_slice(),
        "merged image does not embed the ESP-IDF bootloader"
    );

    let expected = read_partition_layout(root)?;
    let table_region = image_region(
        &image,
        partition_table_offset,
        expected.len() * PARTITION_ENTRY_BYTES,
    )?;
    let partitions = decode_partition_table(table_region)?;
    ensure!(
        partitions.len() == expected.len(),
        "merged image declares {} partitions but {PARTITION_LAYOUT_PATH} declares {}",
        partitions.len(),
        expected.len()
    );
    for (actual, wanted) in partitions.iter().zip(&expected) {
        ensure!(
            actual.label == wanted.label
                && actual.kind == wanted.kind
                && actual.subtype == wanted.subtype
                && actual.raw_offset == wanted.raw_offset
                && actual.bytes == wanted.bytes,
            "merged image partition {} does not match {PARTITION_LAYOUT_PATH}",
            actual.label
        );
    }

    let app = partitions
        .iter()
        .find(|entry| entry.kind == "app")
        .context("partition layout declares no application partition")?;
    let app_offset = usize::try_from(app.raw_offset)?;
    ensure!(
        app_offset < image.len(),
        "merged image ends before the application partition"
    );
    let app_image_bytes = image.len() - app_offset;
    ensure!(
        u64::from(app.bytes) >= app_image_bytes as u64,
        "application image of {app_image_bytes} bytes exceeds its {} byte slot",
        app.bytes
    );

    Ok(MergedImageEvidence {
        bytes: image.len() as u64,
        bootloader_offset: format!("{BOOTLOADER_OFFSET:#x}"),
        bootloader_bytes: bootloader.len() as u64,
        bootloader_window_bytes: window as u64,
        bootloader_region_sha256: sha256_bytes(bootloader_region),
        partition_table_offset: format!("{partition_table_offset:#x}"),
        partition_table_source: PARTITION_LAYOUT_PATH.to_owned(),
        partition_table_region_sha256: sha256_bytes(table_region),
        app_partition: app.label.clone(),
        app_offset: format!("{app_offset:#x}"),
        app_image_bytes: app_image_bytes as u64,
        app_partition_bytes: app.bytes,
        partitions: partitions
            .into_iter()
            .map(DecodedPartition::into_evidence)
            .collect(),
    })
}

/// Parse the repository-owned product partition layout.
fn read_partition_layout(root: &Path) -> Result<Vec<DecodedPartition>> {
    let path = root.join(PARTITION_LAYOUT_PATH);
    let contents =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    parse_partition_layout(&contents)
}

/// Parse partition-layout CSV text.
fn parse_partition_layout(contents: &str) -> Result<Vec<DecodedPartition>> {
    let mut entries = Vec::new();
    for line in contents.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
        ensure!(
            fields.len() >= 5,
            "{PARTITION_LAYOUT_PATH} row {line:?} needs name, type, subtype, offset, and size"
        );
        let kind = match fields[1] {
            "app" | "data" => fields[1].to_owned(),
            other => bail!("{PARTITION_LAYOUT_PATH} has unsupported partition type {other}"),
        };
        entries.push(DecodedPartition {
            label: fields[0].to_owned(),
            subtype: format!("{:#04x}", partition_subtype(&kind, fields[2])?),
            kind,
            raw_offset: parse_partition_number(fields[3])?,
            bytes: parse_partition_number(fields[4])?,
        });
    }
    ensure!(
        !entries.is_empty(),
        "{PARTITION_LAYOUT_PATH} declares no partitions"
    );
    Ok(entries)
}

/// Map an ESP-IDF partition subtype name to its binary encoding.
fn partition_subtype(kind: &str, name: &str) -> Result<u8> {
    let subtype = match kind {
        "app" => match name {
            "factory" => 0x00,
            "ota_0" => 0x10,
            "ota_1" => 0x11,
            _ => bail!("unsupported app partition subtype {name}"),
        },
        "data" => match name {
            "ota" => 0x00,
            "phy" => 0x01,
            "nvs" => 0x02,
            "coredump" => 0x03,
            "nvs_keys" => 0x04,
            "fat" => 0x81,
            "spiffs" => 0x82,
            _ => bail!("unsupported data partition subtype {name}"),
        },
        _ => bail!("unsupported partition type {kind}"),
    };
    Ok(subtype)
}

/// Parse a hexadecimal or decimal partition field.
fn parse_partition_number(value: &str) -> Result<u32> {
    let parsed = value.strip_prefix("0x").map_or_else(
        || value.parse::<u32>().ok(),
        |hex| u32::from_str_radix(hex, 16).ok(),
    );
    parsed.with_context(|| format!("{PARTITION_LAYOUT_PATH} has invalid numeric field {value:?}"))
}

/// Borrow `length` bytes at `offset`, failing when the image is too short.
fn image_region(image: &[u8], offset: usize, length: usize) -> Result<&[u8]> {
    image
        .get(offset..offset + length)
        .with_context(|| format!("merged image is too short for the region at {offset:#x}"))
}

/// A partition-table entry with the raw offset retained for range checks.
struct DecodedPartition {
    label: String,
    kind: String,
    subtype: String,
    raw_offset: u32,
    bytes: u32,
}

impl DecodedPartition {
    fn into_evidence(self) -> PartitionEvidence {
        PartitionEvidence {
            label: self.label,
            kind: self.kind,
            subtype: self.subtype,
            offset: format!("{:#x}", self.raw_offset),
            bytes: self.bytes,
        }
    }
}

/// Decode the binary partition table into its declared entries.
fn decode_partition_table(table: &[u8]) -> Result<Vec<DecodedPartition>> {
    let mut entries = Vec::new();
    for chunk in table.chunks(PARTITION_ENTRY_BYTES) {
        if chunk.len() < PARTITION_ENTRY_BYTES || chunk[..2] != PARTITION_ENTRY_MAGIC {
            break;
        }
        let kind = match chunk[2] {
            0 => "app",
            1 => "data",
            other => bail!("unsupported partition type {other}"),
        };
        let label = String::from_utf8(
            chunk[12..28]
                .iter()
                .copied()
                .take_while(|byte| *byte != 0)
                .collect(),
        )
        .context("partition label is not valid UTF-8")?;
        entries.push(DecodedPartition {
            label,
            kind: kind.to_owned(),
            subtype: format!("{:#04x}", chunk[3]),
            raw_offset: read_u32(chunk, 4)?,
            bytes: read_u32(chunk, 8)?,
        });
    }
    ensure!(
        !entries.is_empty(),
        "partition table declares no valid entries"
    );
    Ok(entries)
}

/// Reject any configuration that would permanently alter a device.
///
/// eFuse bits cannot be cleared, so enabling flash encryption or Secure Boot on a
/// real board is a one-way step. Both features are allowed here only alongside
/// virtual eFuses, and release-mode flash encryption is rejected outright because
/// it also removes plaintext recovery over cable for good.
fn verify_reversible_security_posture(
    config: &BTreeMap<String, String>,
) -> Result<ReversibilityEvidence> {
    let enabled = |key: &str| config.get(key).is_some_and(|value| value == "y");

    let flash_encryption = enabled("CONFIG_SECURE_FLASH_ENC_ENABLED");
    let secure_boot = enabled("CONFIG_SECURE_BOOT");
    let efuse_virtual = enabled("CONFIG_EFUSE_VIRTUAL");
    ensure!(
        !enabled("CONFIG_SECURE_FLASH_ENCRYPTION_MODE_RELEASE"),
        "release-mode flash encryption is irreversible and must not be configured here"
    );
    ensure!(
        !(flash_encryption || secure_boot) || efuse_virtual,
        "flash encryption or Secure Boot without CONFIG_EFUSE_VIRTUAL would burn eFuses on the first boot"
    );

    let mode = if !flash_encryption {
        "disabled"
    } else if enabled("CONFIG_SECURE_FLASH_ENCRYPTION_MODE_DEVELOPMENT") {
        "development"
    } else {
        bail!("flash encryption is enabled without a recognized usage mode");
    };
    let nvs_encryption = if enabled("CONFIG_NVS_ENCRYPTION") {
        ensure!(
            enabled("CONFIG_NVS_SEC_KEY_PROTECT_USING_FLASH_ENC"),
            "NVS encryption must use the flash-encryption scheme so no eFuse key block is consumed"
        );
        "flash-encryption-scheme"
    } else {
        "disabled"
    };

    Ok(ReversibilityEvidence {
        flash_encryption: mode.to_owned(),
        secure_boot: if secure_boot { "virtual" } else { "disabled" }.to_owned(),
        nvs_encryption: nvs_encryption.to_owned(),
        efuse_virtual,
        burns_efuses: false,
    })
}

fn read_sdkconfig(path: &Path) -> Result<BTreeMap<String, String>> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut config = BTreeMap::new();
    for line in contents.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .with_context(|| format!("invalid sdkconfig line: {line}"))?;
        ensure!(
            config.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate sdkconfig key {key}"
        );
    }
    Ok(config)
}

fn ensure_config(config: &BTreeMap<String, String>, key: &str, expected: &str) -> Result<()> {
    let actual = config
        .get(key)
        .with_context(|| format!("sdkconfig does not define {key}"))?;
    ensure!(actual == expected, "{key} must be {expected}, got {actual}");
    Ok(())
}

fn config_string(config: &BTreeMap<String, String>, key: &str) -> Result<String> {
    let value = config
        .get(key)
        .with_context(|| format!("sdkconfig does not define {key}"))?;
    value
        .strip_prefix('"')
        .and_then(|unquoted| unquoted.strip_suffix('"'))
        .map(str::to_owned)
        .with_context(|| format!("{key} is not a quoted string"))
}

/// Read a hexadecimal `sdkconfig` offset such as `CONFIG_PARTITION_TABLE_OFFSET`.
fn config_offset(config: &BTreeMap<String, String>, key: &str) -> Result<usize> {
    let value = config
        .get(key)
        .with_context(|| format!("sdkconfig does not define {key}"))?;
    let parsed = parse_partition_number(value)
        .with_context(|| format!("{key} is not a valid offset: {value}"))?;
    Ok(parsed as usize)
}

fn config_u32(config: &BTreeMap<String, String>, key: &str) -> Result<u32> {
    config
        .get(key)
        .with_context(|| format!("sdkconfig does not define {key}"))?
        .parse::<u32>()
        .with_context(|| format!("{key} is not an unsigned integer"))
}

fn json_string<'a>(value: &'a serde_json::Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .with_context(|| format!("flash layout does not define string {pointer}"))
}

fn package_outputs(
    root: &Path,
    evidence_root: &Path,
    outputs: &BuildOutputs,
    manifest: &ToolchainManifest,
    size_tool: &Path,
    board: &BoardConfigurationEvidence,
) -> Result<Vec<ArtifactEvidence>> {
    let flash_image = evidence_root.join("rumiga-firmware.flash.bin");
    // Without the explicit bootloader, partition table, and flash geometry,
    // espflash substitutes its own defaults and rewrites the bootloader image
    // header, so the merged image would contradict the resolved sdkconfig.
    let mut espflash = Command::new("espflash");
    espflash
        .current_dir(root)
        .args(["save-image", "--chip", &manifest.target.soc])
        .args(["--merge", "--skip-padding", "--skip-update-check"])
        .arg("--bootloader")
        .arg(&outputs.bootloader)
        // espflash 4.5.0 documents a CSV here but also accepts the binary table
        // that ESP-IDF generates. The pinned version keeps that behavior stable.
        .arg("--partition-table")
        .arg(root.join(PARTITION_LAYOUT_PATH))
        .args([
            "--partition-table-offset",
            &board.partition_table_offset.clone(),
        ])
        .args([
            "--flash-mode",
            espflash_flash_mode(&board.bootloader_flash_mode)?,
        ])
        .args([
            "--flash-size",
            espflash_flash_size(&board.configured_flash_geometry)?,
        ])
        .args([
            "--flash-freq",
            espflash_flash_frequency(&board.flash_frequency)?,
        ])
        .arg(&outputs.elf)
        .arg(&flash_image)
        .stdin(Stdio::null());
    run_checked(&mut espflash, "ESP32-P4 merged image generation")?;

    let size_report = create_size_report(size_tool, &outputs.elf)?;
    let size_path = evidence_root.join("rumiga-firmware.size.txt");
    fs::write(&size_path, size_report)
        .with_context(|| format!("failed to write {}", size_path.display()))?;

    let copies = [
        (&outputs.elf, "rumiga-firmware.elf", "linked firmware ELF"),
        (&outputs.map, "rumiga-firmware.map", "final linker map"),
        (&outputs.bootloader, "bootloader.bin", "ESP-IDF bootloader"),
        (
            &outputs.partition_table,
            "partition-table.bin",
            "ESP-IDF partition table",
        ),
        (
            &outputs.sdkconfig,
            "sdkconfig",
            "resolved ESP-IDF configuration",
        ),
        (
            &outputs.flasher_args,
            "flasher_args.json",
            "ESP-IDF flash layout",
        ),
    ];
    let mut artifacts = Vec::new();
    for (source, name, role) in copies {
        let destination = evidence_root.join(name);
        fs::copy(source, &destination).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source.display(),
                destination.display()
            )
        })?;
        artifacts.push(artifact_evidence(&destination, role)?);
    }
    artifacts.push(artifact_evidence(
        &flash_image,
        "merged flash image at offset 0x0",
    )?);
    artifacts.push(artifact_evidence(&size_path, "section size report")?);
    Ok(artifacts)
}

fn create_size_report(size_tool: &Path, elf: &Path) -> Result<String> {
    let directory = elf.parent().context("firmware ELF has no directory")?;
    let name = elf.file_name().context("firmware ELF has no name")?;
    let version = capture_path(directory, size_tool, &["--version"])?;
    let berkeley = capture_os(
        directory,
        size_tool,
        &[OsStr::new("--format=berkeley"), name],
    )?;
    let system_v = capture_os(directory, size_tool, &[OsStr::new("--format=sysv"), name])?;
    Ok(format!(
        "{version}\n\nBerkeley format\n{berkeley}\n\nSystem V format\n{system_v}\n"
    ))
}

fn artifact_evidence(path: &Path, role: &str) -> Result<ArtifactEvidence> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to inspect {}", path.display()))?;
    Ok(ArtifactEvidence {
        name: path
            .file_name()
            .and_then(OsStr::to_str)
            .context("artifact name is not UTF-8")?
            .to_owned(),
        role: role.to_owned(),
        bytes: metadata.len(),
        sha256: sha256_file(path)?,
    })
}

fn write_manifest_and_checksums<T: Serialize>(root: &Path, evidence: &T) -> Result<()> {
    let manifest_path = root.join("manifest.json");
    let mut manifest = serde_json::to_vec_pretty(evidence)?;
    manifest.push(b'\n');
    fs::write(&manifest_path, manifest)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let mut files = fs::read_dir(root)?
        .map(|entry| entry.map(|value| value.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    files.retain(|path| path.is_file() && path.file_name() != Some(OsStr::new("SHA256SUMS")));
    files.sort();
    let checksums_path = root.join("SHA256SUMS");
    let mut checksums = File::create(&checksums_path)
        .with_context(|| format!("failed to create {}", checksums_path.display()))?;
    for path in files {
        writeln!(
            checksums,
            "{}  {}",
            sha256_file(&path)?,
            path.file_name()
                .and_then(OsStr::to_str)
                .context("artifact name is not UTF-8")?
        )?;
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex_lowercase(&hasher.finalize()))
}

/// Render bytes as lowercase hexadecimal.
///
/// `digest` 0.11 returns an `Array` that does not implement `LowerHex`, so the
/// encoding is explicit rather than a formatting flag.
fn hex_lowercase(bytes: &[u8]) -> String {
    const DIGITS: [u8; 16] = *b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0F)]));
    }
    encoded
}

fn capture_git(directory: &Path, arguments: &[&str]) -> Result<String> {
    capture(directory, "git", arguments)
}

fn capture(directory: &Path, program: &str, arguments: &[&str]) -> Result<String> {
    capture_path(directory, Path::new(program), arguments)
}

fn capture_path(directory: &Path, program: &Path, arguments: &[&str]) -> Result<String> {
    let arguments = arguments.iter().map(OsStr::new).collect::<Vec<_>>();
    capture_os(directory, program, &arguments)
}

fn capture_os(directory: &Path, program: &Path, arguments: &[&OsStr]) -> Result<String> {
    let output = Command::new(program)
        .current_dir(directory)
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to execute {}", program.display()))?;
    ensure!(
        output.status.success(),
        "{} failed: {}",
        program.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout)
        .context("command output is not UTF-8")
        .map(|value| value.trim_end().to_owned())
}

fn run_checked(command: &mut Command, description: &str) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("failed to start {description}"))?;
    ensure!(status.success(), "{description} failed with {status}");
    Ok(())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex_lowercase(&Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        decode_partition_table, espflash_flash_frequency, espflash_flash_mode, espflash_flash_size,
        image_region, inspect_elf_data, parse_partition_layout, read_partition_layout,
        sha256_bytes, verify_reversible_security_posture,
    };

    /// Build one 32-byte partition-table entry.
    fn partition_entry(kind: u8, subtype: u8, offset: u32, size: u32, label: &str) -> Vec<u8> {
        let mut entry = vec![0_u8; 32];
        entry[..2].copy_from_slice(&[0xAA, 0x50]);
        entry[2] = kind;
        entry[3] = subtype;
        entry[4..8].copy_from_slice(&offset.to_le_bytes());
        entry[8..12].copy_from_slice(&size.to_le_bytes());
        entry[12..12 + label.len()].copy_from_slice(label.as_bytes());
        entry
    }

    #[test]
    fn decodes_partition_entries_until_the_end_marker() {
        let mut table = partition_entry(1, 0x02, 0x9000, 24576, "nvs");
        table.extend(partition_entry(0, 0x00, 0x10000, 1_048_576, "factory"));
        // The real table is followed by an MD5 entry and unwritten flash.
        table.extend(vec![0xFF_u8; 64]);

        let entries = decode_partition_table(&table).expect("table must decode");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].label, "nvs");
        assert_eq!(entries[0].kind, "data");
        assert_eq!(entries[1].label, "factory");
        assert_eq!(entries[1].kind, "app");
        assert_eq!(entries[1].raw_offset, 0x10000);
        assert_eq!(entries[1].bytes, 1_048_576);
    }

    #[test]
    fn rejects_a_partition_table_without_entries() {
        assert!(decode_partition_table(&[0xFF_u8; 32]).is_err());
    }

    #[test]
    fn rejects_an_unsupported_partition_type() {
        let table = partition_entry(7, 0x00, 0x10000, 1024, "odd");
        assert!(decode_partition_table(&table).is_err());
    }

    #[test]
    fn rejects_a_region_beyond_the_image() {
        let image = [0_u8; 16];
        assert!(image_region(&image, 8, 4).is_ok());
        assert!(image_region(&image, 8, 16).is_err());
    }

    #[test]
    fn translates_declared_flash_settings_for_espflash() {
        assert_eq!(espflash_flash_mode("dio").expect("known mode"), "dio");
        assert_eq!(espflash_flash_size("16MB").expect("known size"), "16mb");
        assert_eq!(
            espflash_flash_frequency("80m").expect("known frequency"),
            "80mhz"
        );
    }

    #[test]
    fn rejects_unknown_flash_settings() {
        assert!(espflash_flash_mode("sio").is_err());
        assert!(espflash_flash_size("16mb").is_err());
        assert!(espflash_flash_frequency("80mhz").is_err());
    }

    /// Build a resolved-config map from `key=value` lines.
    fn config_from(lines: &[&str]) -> std::collections::BTreeMap<String, String> {
        lines
            .iter()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect()
    }

    #[test]
    fn accepts_a_reversible_security_posture() {
        let config = config_from(&[
            "CONFIG_SECURE_FLASH_ENC_ENABLED=y",
            "CONFIG_SECURE_FLASH_ENCRYPTION_MODE_DEVELOPMENT=y",
            "CONFIG_EFUSE_VIRTUAL=y",
            "CONFIG_NVS_ENCRYPTION=y",
            "CONFIG_NVS_SEC_KEY_PROTECT_USING_FLASH_ENC=y",
        ]);
        let evidence =
            verify_reversible_security_posture(&config).expect("virtual eFuses must be accepted");
        assert_eq!(evidence.flash_encryption, "development");
        assert_eq!(evidence.nvs_encryption, "flash-encryption-scheme");
        assert!(evidence.efuse_virtual);
        assert!(!evidence.burns_efuses);
    }

    #[test]
    fn rejects_configurations_that_would_burn_efuses() {
        // Flash encryption without virtual eFuses burns a key on the first boot.
        assert!(
            verify_reversible_security_posture(&config_from(&[
                "CONFIG_SECURE_FLASH_ENC_ENABLED=y",
                "CONFIG_SECURE_FLASH_ENCRYPTION_MODE_DEVELOPMENT=y",
            ]))
            .is_err()
        );
        // Secure Boot without virtual eFuses burns the key digest and enable bit.
        assert!(
            verify_reversible_security_posture(&config_from(&["CONFIG_SECURE_BOOT=y"])).is_err()
        );
        // Release mode also removes plaintext recovery over cable for good.
        assert!(
            verify_reversible_security_posture(&config_from(&[
                "CONFIG_SECURE_FLASH_ENC_ENABLED=y",
                "CONFIG_SECURE_FLASH_ENCRYPTION_MODE_RELEASE=y",
                "CONFIG_EFUSE_VIRTUAL=y",
            ]))
            .is_err()
        );
        // NVS encryption via HMAC would consume an eFuse key block.
        assert!(
            verify_reversible_security_posture(&config_from(&[
                "CONFIG_SECURE_FLASH_ENC_ENABLED=y",
                "CONFIG_SECURE_FLASH_ENCRYPTION_MODE_DEVELOPMENT=y",
                "CONFIG_EFUSE_VIRTUAL=y",
                "CONFIG_NVS_ENCRYPTION=y",
                "CONFIG_NVS_SEC_KEY_PROTECT_USING_HMAC=y",
            ]))
            .is_err()
        );
    }

    #[test]
    fn accepts_security_features_being_absent() {
        let evidence = verify_reversible_security_posture(&config_from(&["CONFIG_IDF_TARGET=x"]))
            .expect("a plain configuration must be accepted");
        assert_eq!(evidence.flash_encryption, "disabled");
        assert_eq!(evidence.secure_boot, "disabled");
        assert_eq!(evidence.nvs_encryption, "disabled");
    }

    #[test]
    fn parses_the_partition_layout_ignoring_comments() {
        let csv = "# header comment\n\
                   nvs,      data, nvs,   0x11000, 0x50000,\n\
                   \n\
                   ota_0,    app,  ota_0, 0x80000, 0x600000, # trailing\n";
        let entries = parse_partition_layout(csv).expect("layout must parse");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].label, "nvs");
        assert_eq!(entries[0].kind, "data");
        assert_eq!(entries[0].subtype, "0x02");
        assert_eq!(entries[0].raw_offset, 0x11000);
        assert_eq!(entries[0].bytes, 0x50000);
        assert_eq!(entries[1].kind, "app");
        assert_eq!(entries[1].subtype, "0x10");
    }

    #[test]
    fn rejects_malformed_partition_layouts() {
        assert!(parse_partition_layout("# only a comment\n").is_err());
        assert!(parse_partition_layout("nvs, data, nvs, 0x1000\n").is_err());
        assert!(parse_partition_layout("nvs, blob, nvs, 0x1000, 0x1000,\n").is_err());
        assert!(parse_partition_layout("nvs, data, magic, 0x1000, 0x1000,\n").is_err());
        assert!(parse_partition_layout("nvs, data, nvs, 0xzz, 0x1000,\n").is_err());
    }

    #[test]
    fn repository_partition_layout_is_contiguous_and_aligned() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask is a workspace child")
            .to_path_buf();
        let entries = read_partition_layout(&root).expect("shipped layout must parse");
        let mut cursor = entries[0].raw_offset;
        for entry in &entries {
            assert_eq!(
                entry.raw_offset, cursor,
                "partition {} leaves an unallocated gap",
                entry.label
            );
            if entry.kind == "app" {
                assert_eq!(
                    entry.raw_offset % 0x10000,
                    0,
                    "app partition {} must be 64 KiB aligned",
                    entry.label
                );
            }
            assert_eq!(
                entry.raw_offset % 0x1000,
                0,
                "partition {} must be 4 KiB aligned",
                entry.label
            );
            cursor = entry.raw_offset + entry.bytes;
        }
        assert_eq!(
            u64::from(cursor),
            16 * 1024 * 1024,
            "the layout must fill the configured 16 MB geometry exactly"
        );
        let slots = entries
            .iter()
            .filter(|entry| entry.kind == "app")
            .map(|entry| entry.bytes)
            .collect::<Vec<_>>();
        assert_eq!(slots.len(), 2, "the product layout needs two OTA slots");
        assert_eq!(slots[0], slots[1], "OTA slots must be the same size");
    }

    #[test]
    fn validates_static_riscv32_single_float_elf() {
        let mut elf = vec![0_u8; 84];
        elf[..6].copy_from_slice(b"\x7fELF\x01\x01");
        elf[16..18].copy_from_slice(&2_u16.to_le_bytes());
        elf[18..20].copy_from_slice(&243_u16.to_le_bytes());
        elf[28..32].copy_from_slice(&52_u32.to_le_bytes());
        elf[36..40].copy_from_slice(&3_u32.to_le_bytes());
        elf[42..44].copy_from_slice(&32_u16.to_le_bytes());
        elf[44..46].copy_from_slice(&1_u16.to_le_bytes());
        elf[52..56].copy_from_slice(&1_u32.to_le_bytes());

        let metadata = inspect_elf_data(&elf).expect("valid ELF must pass");
        assert!(metadata.compressed_instructions);
        assert_eq!(metadata.linkage, "static");
        assert_eq!(metadata.float_abi, "single");
    }

    #[test]
    fn rejects_dynamically_linked_firmware() {
        let mut elf = vec![0_u8; 84];
        elf[..6].copy_from_slice(b"\x7fELF\x01\x01");
        elf[16..18].copy_from_slice(&2_u16.to_le_bytes());
        elf[18..20].copy_from_slice(&243_u16.to_le_bytes());
        elf[28..32].copy_from_slice(&52_u32.to_le_bytes());
        elf[36..40].copy_from_slice(&2_u32.to_le_bytes());
        elf[42..44].copy_from_slice(&32_u16.to_le_bytes());
        elf[44..46].copy_from_slice(&1_u16.to_le_bytes());
        elf[52..56].copy_from_slice(&2_u32.to_le_bytes());

        assert!(inspect_elf_data(&elf).is_err());
    }

    #[test]
    fn hashes_bytes_with_sha256() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
