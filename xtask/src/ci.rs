// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use yaml_rust2::{Yaml, YamlLoader};

use super::{
    ToolchainManifest, capture, capture_git, commit_policy, compatibility, read_toolchain_manifest,
    run_checked, sha256_bytes, sha256_file, supply_chain, workspace_root,
};

const FIRMWARE_EVIDENCE_DIRECTORY: &str = "target/m0-008-firmware-evidence";
const SUPPLY_CHAIN_EVIDENCE_DIRECTORY: &str = "target/m0-009-supply-chain-evidence";

const ALL_GATES: [Gate; 8] = [
    Gate::Commits,
    Gate::Lockfiles,
    Gate::Governance,
    Gate::Host,
    Gate::Compatibility,
    Gate::SupplyChain,
    Gate::Portable,
    Gate::Firmware,
];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Gate {
    Commits,
    Lockfiles,
    Governance,
    Host,
    Compatibility,
    SupplyChain,
    Portable,
    Firmware,
}

impl Gate {
    const fn name(self) -> &'static str {
        match self {
            Self::Commits => "commits",
            Self::Lockfiles => "lockfiles",
            Self::Governance => "governance",
            Self::Host => "host",
            Self::Compatibility => "compatibility",
            Self::SupplyChain => "supply-chain",
            Self::Portable => "portable",
            Self::Firmware => "firmware",
        }
    }

    const fn job(self) -> &'static str {
        match self {
            Self::Commits => "commits",
            Self::Lockfiles => "lockfiles",
            Self::Governance => "governance",
            Self::Host => "host",
            Self::Compatibility => "compatibility",
            Self::SupplyChain => "supply-chain",
            Self::Portable => "portable",
            Self::Firmware => "firmware",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Commits => "Conventional commit policy",
            Self::Lockfiles => "Lockfile integrity",
            Self::Governance => "Engineering governance evidence",
            Self::Host => "Host validation",
            Self::Compatibility => "Public compatibility evidence",
            Self::SupplyChain => "Supply-chain policy",
            Self::Portable => "Portable Rust boundary",
            Self::Firmware => "ESP32-P4 firmware evidence",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        ALL_GATES
            .into_iter()
            .find(|gate| gate.name() == value)
            .with_context(|| {
                format!(
                    "unknown quality gate {value:?}; expected one of {}",
                    gate_names().join(", ")
                )
            })
    }
}

#[derive(Debug, Eq, PartialEq)]
enum CliAction {
    Run(Vec<Gate>),
    List,
    Help,
}

#[derive(Debug, Eq, PartialEq)]
struct TrackedState {
    status: String,
    worktree_diff_sha256: String,
    index_diff_sha256: String,
}

pub fn run(arguments: &[String]) -> Result<()> {
    match parse_arguments(arguments)? {
        CliAction::Help => {
            print_help();
            Ok(())
        }
        CliAction::List => {
            for gate in ALL_GATES {
                println!("{:<13} {}", gate.name(), gate.title());
            }
            Ok(())
        }
        CliAction::Run(gates) => run_gates(&gates),
    }
}

fn parse_arguments(arguments: &[String]) -> Result<CliAction> {
    if arguments == ["--help"] || arguments == ["-h"] {
        return Ok(CliAction::Help);
    }
    if arguments == ["--list"] {
        return Ok(CliAction::List);
    }

    let mut requested = BTreeSet::new();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        let name = if argument == "--gate" {
            index += 1;
            arguments
                .get(index)
                .context("--gate requires a gate name")?
                .as_str()
        } else if let Some(name) = argument.strip_prefix("--gate=") {
            ensure!(!name.is_empty(), "--gate requires a gate name");
            name
        } else {
            bail!("unknown ci option {argument:?}; use --help for usage");
        };
        let gate = Gate::parse(name)?;
        ensure!(
            requested.insert(gate),
            "quality gate {name:?} was selected twice"
        );
        index += 1;
    }

    let gates = if requested.is_empty() {
        ALL_GATES.to_vec()
    } else {
        ALL_GATES
            .into_iter()
            .filter(|gate| requested.contains(gate))
            .collect()
    };
    Ok(CliAction::Run(gates))
}

fn print_help() {
    println!(
        "Rumiga quality gates\n\n\
         Usage:\n  cargo +1.97.1 xtask ci [--gate <name>]...\n\n\
         With no --gate option, all gates run in canonical order. Repeating\n\
         --gate selects a subset while retaining that order. Use --list to\n\
         show the available gate names."
    );
}

fn gate_names() -> Vec<&'static str> {
    ALL_GATES.into_iter().map(Gate::name).collect()
}

fn run_gates(gates: &[Gate]) -> Result<()> {
    ensure!(!gates.is_empty(), "at least one quality gate is required");
    let root = workspace_root()?;
    let manifest = read_toolchain_manifest(&root)?;
    verify_manifest(&manifest)?;
    verify_workflow_contract(&root, &manifest)?;

    let started = Instant::now();
    let mut completed = Vec::new();
    println!("Rumiga quality gates: {}", display_gate_list(gates));

    for &gate in gates {
        let gate_started = Instant::now();
        println!("\n==> {} [{}]", gate.title(), gate.name());
        run_guarded_gate(&root, &manifest, gate)
            .with_context(|| format!("quality gate {:?} failed", gate.name()))?;
        let elapsed = gate_started.elapsed();
        println!("<== {} passed ({})", gate.title(), format_duration(elapsed));
        completed.push((gate, elapsed));
    }

    println!("\nQuality gate summary");
    for (gate, elapsed) in completed {
        println!("  PASS  {:<13} {}", gate.name(), format_duration(elapsed));
    }
    println!(
        "  PASS  aggregate     {}",
        format_duration(started.elapsed())
    );
    Ok(())
}

fn display_gate_list(gates: &[Gate]) -> String {
    gates
        .iter()
        .map(|gate| gate.name())
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let milliseconds = duration.subsec_millis();
    if seconds == 0 {
        format!("{milliseconds} ms")
    } else {
        format!("{seconds}.{milliseconds:03} s")
    }
}

fn run_guarded_gate(root: &Path, manifest: &ToolchainManifest, gate: Gate) -> Result<()> {
    let before = tracked_state(root)?;
    if is_ci() {
        ensure!(
            before.status.is_empty(),
            "CI quality gates require a clean tracked worktree"
        );
    }

    let result = match gate {
        Gate::Commits => run_commit_gate(root, manifest),
        Gate::Lockfiles => run_lockfile_gate(root, manifest),
        Gate::Governance => run_governance_gate(root, manifest),
        Gate::Host => run_host_gate(root, manifest),
        Gate::Compatibility => run_compatibility_gate(root, manifest),
        Gate::SupplyChain => run_supply_chain_gate(root, manifest),
        Gate::Portable => run_portable_gate(root, manifest),
        Gate::Firmware => run_firmware_gate(root, manifest),
    };

    let after = tracked_state(root)?;
    ensure!(
        after == before,
        "quality gate modified tracked repository files; inspect git status and restore intentionally"
    );
    result
}

fn run_commit_gate(root: &Path, manifest: &ToolchainManifest) -> Result<()> {
    verify_host_tools(root, manifest, false)?;
    commit_policy::validate_repository(root)
}

fn is_ci() -> bool {
    env::var("CI").is_ok_and(|value| value == "true")
}

fn tracked_state(root: &Path) -> Result<TrackedState> {
    let status = capture_git(root, &["status", "--porcelain=v1", "--untracked-files=no"])?;
    let worktree_diff = capture_git(root, &["diff", "--binary", "--full-index", "--no-ext-diff"])?;
    let index_diff = capture_git(
        root,
        &[
            "diff",
            "--cached",
            "--binary",
            "--full-index",
            "--no-ext-diff",
        ],
    )?;
    Ok(TrackedState {
        status,
        worktree_diff_sha256: sha256_bytes(worktree_diff.as_bytes()),
        index_diff_sha256: sha256_bytes(index_diff.as_bytes()),
    })
}

fn verify_manifest(manifest: &ToolchainManifest) -> Result<()> {
    ensure!(
        valid_version_pin(&manifest.host.rust),
        "host Rust pin is invalid"
    );
    ensure!(
        valid_version_pin(&manifest.host.node),
        "Node.js pin is invalid"
    );
    ensure!(valid_version_pin(&manifest.host.npm), "npm pin is invalid");
    ensure!(
        !manifest.portable_rust.target.trim().is_empty(),
        "portable Rust target is empty"
    );
    ensure!(
        !manifest.portable_rust.profiles.is_empty(),
        "portable Rust profile list is empty"
    );
    let profile_names = manifest
        .portable_rust
        .profiles
        .iter()
        .map(|profile| &profile.name)
        .collect::<BTreeSet<_>>();
    ensure!(
        profile_names.len() == manifest.portable_rust.profiles.len(),
        "portable Rust profile names contain duplicates"
    );
    for profile in &manifest.portable_rust.profiles {
        ensure!(
            !profile.name.trim().is_empty(),
            "portable Rust profile name is empty"
        );
        ensure!(
            !profile.packages.is_empty(),
            "portable Rust profile {} has no packages",
            profile.name
        );
        let packages = profile.packages.iter().collect::<BTreeSet<_>>();
        ensure!(
            packages.len() == profile.packages.len(),
            "portable Rust profile {} contains duplicate packages",
            profile.name
        );
        ensure!(
            profile
                .packages
                .iter()
                .all(|package| !package.trim().is_empty()),
            "portable Rust profile {} contains an empty package",
            profile.name
        );
        let features = profile.features.iter().collect::<BTreeSet<_>>();
        ensure!(
            features.len() == profile.features.len(),
            "portable Rust profile {} contains duplicate features",
            profile.name
        );
        ensure!(
            profile
                .features
                .iter()
                .all(|feature| !feature.trim().is_empty()),
            "portable Rust profile {} contains an empty feature",
            profile.name
        );
    }
    Ok(())
}

fn valid_version_pin(value: &str) -> bool {
    let components = value.split('.').collect::<Vec<_>>();
    components.len() == 3
        && components.iter().all(|component| {
            !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn verify_host_tools(root: &Path, manifest: &ToolchainManifest, require_node: bool) -> Result<()> {
    let rustc = capture(
        root,
        "rustup",
        &["run", &manifest.host.rust, "rustc", "--version"],
    )?;
    ensure!(
        rustc.split_whitespace().nth(1) == Some(manifest.host.rust.as_str()),
        "rustc does not match toolchain/manifest.toml: {rustc}"
    );
    let cargo = capture(
        root,
        "rustup",
        &["run", &manifest.host.rust, "cargo", "--version"],
    )?;
    ensure!(
        cargo.split_whitespace().nth(1) == Some(manifest.host.rust.as_str()),
        "Cargo does not match toolchain/manifest.toml: {cargo}"
    );

    if require_node {
        let node = capture(root, "node", &["--version"])?;
        ensure!(
            node == format!("v{}", manifest.host.node),
            "Node.js does not match toolchain/manifest.toml: {node}"
        );
        let npm = capture(root, "npm", &["--version"])?;
        ensure!(
            npm == manifest.host.npm,
            "npm does not match toolchain/manifest.toml: {npm}"
        );
    }
    Ok(())
}

fn run_lockfile_gate(root: &Path, manifest: &ToolchainManifest) -> Result<()> {
    verify_host_tools(root, manifest, true)?;
    let mut metadata = Command::new("cargo");
    metadata
        .current_dir(root)
        .args([
            format!("+{}", manifest.host.rust),
            "metadata".to_owned(),
            "--locked".to_owned(),
            "--no-deps".to_owned(),
            "--format-version".to_owned(),
            "1".to_owned(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    run_checked(&mut metadata, "locked Cargo metadata")?;
    run_npm(
        root,
        &["ci", "--ignore-scripts", "--no-audit", "--no-fund"],
        "locked npm install",
    )
}

fn run_host_gate(root: &Path, manifest: &ToolchainManifest) -> Result<()> {
    verify_host_tools(root, manifest, true)?;
    run_npm(
        root,
        &["ci", "--ignore-scripts", "--no-audit", "--no-fund"],
        "web dependency install",
    )?;
    run_npm(root, &["run", "lint"], "web lint")?;
    run_npm(root, &["run", "build"], "web production build")?;

    run_cargo(
        root,
        manifest,
        &["fmt", "--all", "--", "--check"],
        "Rust formatting",
    )?;
    verify_m68k_feature_model(root, manifest)?;
    verify_core_feature_model(root, manifest)?;
    run_cargo(
        root,
        manifest,
        &[
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        "Rust Clippy",
    )?;
    run_cargo(
        root,
        manifest,
        &["test", "--locked", "--workspace"],
        "Rust workspace tests",
    )?;

    let mut docs = cargo_command(
        root,
        manifest,
        &["doc", "--locked", "--workspace", "--no-deps"],
    );
    docs.env("RUSTDOCFLAGS", "-D warnings");
    run_checked(&mut docs, "warning-free Rust documentation")
}

fn run_governance_gate(root: &Path, manifest: &ToolchainManifest) -> Result<()> {
    verify_host_tools(root, manifest, false)?;
    super::governance::build_evidence()?;
    verify_checksum_manifest(&root.join(super::governance::EVIDENCE_DIRECTORY))
}

fn run_supply_chain_gate(root: &Path, manifest: &ToolchainManifest) -> Result<()> {
    verify_host_tools(root, manifest, true)?;
    supply_chain::build_evidence()?;
    verify_checksum_manifest(&root.join(SUPPLY_CHAIN_EVIDENCE_DIRECTORY))
}

fn run_compatibility_gate(root: &Path, manifest: &ToolchainManifest) -> Result<()> {
    verify_host_tools(root, manifest, false)?;
    compatibility::build_evidence()?;
    verify_checksum_manifest(&root.join(compatibility::EVIDENCE_DIRECTORY))
}

fn run_portable_gate(root: &Path, manifest: &ToolchainManifest) -> Result<()> {
    verify_host_tools(root, manifest, false)?;
    let installed = capture(
        root,
        "rustup",
        &[
            "target",
            "list",
            "--installed",
            "--toolchain",
            &manifest.host.rust,
        ],
    )?;
    ensure!(
        installed
            .lines()
            .any(|line| line == manifest.portable_rust.target),
        "portable target {} is not installed for Rust {}; run `rustup target add --toolchain {} {}`",
        manifest.portable_rust.target,
        manifest.host.rust,
        manifest.host.rust,
        manifest.portable_rust.target
    );

    for profile in &manifest.portable_rust.profiles {
        let mut arguments = vec![
            "check".to_owned(),
            "--locked".to_owned(),
            "--target".to_owned(),
            manifest.portable_rust.target.clone(),
        ];
        if profile.release {
            arguments.push("--release".to_owned());
        }
        if !profile.default_features {
            arguments.push("--no-default-features".to_owned());
        }
        if !profile.features.is_empty() {
            arguments.push("--features".to_owned());
            arguments.push(profile.features.join(","));
        }
        for package in &profile.packages {
            arguments.push("-p".to_owned());
            arguments.push(package.clone());
        }
        let mut command = Command::new("cargo");
        command
            .current_dir(root)
            .arg(format!("+{}", manifest.host.rust))
            .args(arguments)
            .stdin(Stdio::null());
        run_checked(
            &mut command,
            &format!("portable Rust profile {}", profile.name),
        )?;
    }
    Ok(())
}

fn run_firmware_gate(root: &Path, manifest: &ToolchainManifest) -> Result<()> {
    verify_host_tools(root, manifest, false)?;
    super::build_firmware_evidence()?;
    verify_checksum_manifest(&root.join(FIRMWARE_EVIDENCE_DIRECTORY))
}

fn run_npm(root: &Path, arguments: &[&str], description: &str) -> Result<()> {
    let mut command = Command::new("npm");
    command
        .current_dir(root.join("web"))
        .args(arguments)
        .stdin(Stdio::null());
    run_checked(&mut command, description)
}

fn run_cargo(
    root: &Path,
    manifest: &ToolchainManifest,
    arguments: &[&str],
    description: &str,
) -> Result<()> {
    let mut command = cargo_command(root, manifest, arguments);
    run_checked(&mut command, description)
}

fn verify_m68k_feature_model(root: &Path, manifest: &ToolchainManifest) -> Result<()> {
    run_cargo(
        root,
        manifest,
        &[
            "check",
            "--locked",
            "-p",
            "m68k",
            "--no-default-features",
            "--features",
            "std",
        ],
        "m68k stock std profile",
    )?;
    run_cargo(
        root,
        manifest,
        &[
            "clippy",
            "--locked",
            "-p",
            "m68k",
            "--all-targets",
            "--no-default-features",
            "--features",
            "no_std",
            "--",
            "-D",
            "warnings",
        ],
        "m68k no_std Clippy",
    )?;
    run_cargo(
        root,
        manifest,
        &[
            "test",
            "--locked",
            "-p",
            "m68k",
            "--no-default-features",
            "--features",
            "no_std",
        ],
        "m68k no_std tests",
    )?;
    run_cargo_expect_failure(
        root,
        manifest,
        &["check", "--locked", "-p", "m68k", "--no-default-features"],
        "m68k missing runtime feature rejection",
        "select exactly one runtime feature: `std` or `no_std`",
    )?;
    run_cargo_expect_failure(
        root,
        manifest,
        &[
            "check",
            "--locked",
            "-p",
            "m68k",
            "--no-default-features",
            "--features",
            "std,no_std",
        ],
        "m68k conflicting runtime feature rejection",
        "features `std` and `no_std` are mutually exclusive",
    )?;
    run_cargo_expect_failure(
        root,
        manifest,
        &[
            "check",
            "--locked",
            "-p",
            "m68k",
            "--no-default-features",
            "--features",
            "no_std,fpu",
        ],
        "m68k no_std FPU rejection",
        "feature `fpu` requires the `std` runtime profile",
    )
}

fn verify_core_feature_model(root: &Path, manifest: &ToolchainManifest) -> Result<()> {
    run_cargo(
        root,
        manifest,
        &[
            "check",
            "--locked",
            "-p",
            "rumiga-core",
            "--no-default-features",
            "--features",
            "std",
        ],
        "rumiga-core explicit std profile",
    )?;
    run_cargo(
        root,
        manifest,
        &[
            "clippy",
            "--locked",
            "-p",
            "rumiga-core",
            "--all-targets",
            "--no-default-features",
            "--features",
            "no_std",
            "--",
            "-D",
            "warnings",
        ],
        "rumiga-core no_std Clippy",
    )?;
    run_cargo(
        root,
        manifest,
        &[
            "test",
            "--locked",
            "-p",
            "rumiga-core",
            "--no-default-features",
            "--features",
            "no_std",
        ],
        "rumiga-core no_std tests",
    )?;
    run_cargo_expect_failure(
        root,
        manifest,
        &[
            "check",
            "--locked",
            "-p",
            "rumiga-core",
            "--no-default-features",
        ],
        "rumiga-core missing runtime feature rejection",
        "select exactly one runtime feature: `std` or `no_std`",
    )?;
    run_cargo_expect_failure(
        root,
        manifest,
        &["check", "--locked", "-p", "rumiga-core", "--all-features"],
        "rumiga-core conflicting runtime feature rejection",
        "features `std` and `no_std` are mutually exclusive",
    )
}

fn run_cargo_expect_failure(
    root: &Path,
    manifest: &ToolchainManifest,
    arguments: &[&str],
    description: &str,
    expected_diagnostic: &str,
) -> Result<()> {
    let mut command = cargo_command(root, manifest, arguments);
    command.env("CARGO_TERM_COLOR", "never");
    let output = command
        .output()
        .with_context(|| format!("failed to start {description}"))?;
    ensure!(
        !output.status.success(),
        "{description} unexpectedly succeeded"
    );
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        diagnostic.contains(expected_diagnostic),
        "{description} failed without the required diagnostic {expected_diagnostic:?}: {}",
        diagnostic.trim()
    );
    Ok(())
}

fn cargo_command(root: &Path, manifest: &ToolchainManifest, arguments: &[&str]) -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .arg(format!("+{}", manifest.host.rust))
        .args(arguments)
        .stdin(Stdio::null());
    command
}

fn verify_checksum_manifest(directory: &Path) -> Result<()> {
    ensure!(
        directory.is_dir(),
        "missing evidence directory {}",
        directory.display()
    );
    let manifest_path = directory.join("SHA256SUMS");
    let contents = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    ensure!(!contents.is_empty(), "checksum manifest is empty");

    let mut listed = BTreeSet::new();
    for (index, line) in contents.lines().enumerate() {
        let (expected, name) = parse_checksum_line(line)
            .with_context(|| format!("invalid checksum line {}", index + 1))?;
        ensure!(
            listed.insert(name.to_owned()),
            "duplicate checksum entry {name:?}"
        );
        let path = directory.join(name);
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("missing checksummed file {}", path.display()))?;
        ensure!(
            metadata.file_type().is_file(),
            "checksummed path is not a regular file: {}",
            path.display()
        );
        ensure!(
            sha256_file(&path)? == expected,
            "checksum mismatch for {}",
            path.display()
        );
    }

    let actual = fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            let file_type = entry.file_type()?;
            ensure!(file_type.is_file(), "unexpected non-file evidence entry");
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("evidence filename is not UTF-8"))?;
            Ok(name)
        })
        .filter_map(|result: Result<String>| match result {
            Ok(name) if name == "SHA256SUMS" => None,
            other => Some(other),
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        listed == actual,
        "checksum manifest does not cover the exact evidence directory contents"
    );
    Ok(())
}

fn parse_checksum_line(line: &str) -> Result<(&str, &str)> {
    let (checksum, name) = line
        .split_once("  ")
        .context("checksum entry must use two-space separation")?;
    ensure!(
        checksum.len() == 64
            && checksum
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')),
        "checksum must be lowercase SHA-256"
    );
    ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains(['/', '\\'])
            && Path::new(name).file_name() == Some(OsStr::new(name)),
        "checksum filename must be a plain basename"
    );
    ensure!(
        name != "SHA256SUMS",
        "checksum manifest cannot cover itself"
    );
    Ok((checksum, name))
}

fn verify_workflow_contract(root: &Path, manifest: &ToolchainManifest) -> Result<()> {
    let path = root.join(".github/workflows/ci.yml");
    let contents =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let documents = YamlLoader::load_from_str(&contents)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    ensure!(
        documents.len() == 1,
        "CI workflow must have one YAML document"
    );
    let jobs = yaml_get(&documents[0], "jobs")
        .and_then(Yaml::as_hash)
        .context("CI workflow has no jobs mapping")?;

    let mut observed = BTreeSet::new();
    for (job_name, job) in jobs {
        let job_name = job_name.as_str().context("CI job name must be a string")?;
        let Some(steps) = yaml_get(job, "steps").and_then(Yaml::as_vec) else {
            continue;
        };
        for step in steps {
            let Some(run) = yaml_get(step, "run").and_then(Yaml::as_str) else {
                continue;
            };
            if run.contains("xtask ci") {
                observed.insert((job_name.to_owned(), run.trim().to_owned()));
            }
        }
    }

    let expected = ALL_GATES
        .into_iter()
        .map(|gate| {
            (
                gate.job().to_owned(),
                format!(
                    "cargo +{} xtask ci --gate {}",
                    manifest.host.rust,
                    gate.name()
                ),
            )
        })
        .collect::<BTreeSet<_>>();
    ensure!(
        observed == expected,
        "CI workflow quality-gate invocations drifted from xtask"
    );

    let quality_gate = jobs
        .get(&Yaml::String("quality-gate".to_owned()))
        .context("CI workflow has no quality-gate job")?;
    let needs = yaml_get(quality_gate, "needs")
        .and_then(Yaml::as_vec)
        .context("quality-gate job needs must be an array")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .context("quality-gate dependency must be a string")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let required = ALL_GATES
        .into_iter()
        .map(|gate| gate.job().to_owned())
        .collect::<BTreeSet<_>>();
    ensure!(
        needs == required,
        "Required Quality Gate dependencies drifted from canonical gates"
    );
    Ok(())
}

fn yaml_get<'a>(node: &'a Yaml, key: &str) -> Option<&'a Yaml> {
    node.as_hash()?.get(&Yaml::String(key.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{
        ALL_GATES, CliAction, Gate, gate_names, parse_arguments, parse_checksum_line,
        verify_workflow_contract,
    };
    use crate::{read_toolchain_manifest, workspace_root};

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn no_arguments_select_every_gate_in_canonical_order() {
        assert_eq!(
            parse_arguments(&[]).expect("default selection must parse"),
            CliAction::Run(ALL_GATES.to_vec())
        );
    }

    #[test]
    fn selected_gates_keep_canonical_order() {
        assert_eq!(
            parse_arguments(&strings(&[
                "--gate",
                "firmware",
                "--gate=lockfiles",
                "--gate",
                "host",
            ]))
            .expect("selection must parse"),
            CliAction::Run(vec![Gate::Lockfiles, Gate::Host, Gate::Firmware])
        );
    }

    #[test]
    fn invalid_and_duplicate_gate_selections_fail() {
        assert!(parse_arguments(&strings(&["--gate", "unknown"])).is_err());
        assert!(parse_arguments(&strings(&["--gate", "host", "--gate=host"])).is_err());
        assert!(parse_arguments(&strings(&["--gate"])).is_err());
    }

    #[test]
    fn gate_names_are_stable_and_unique() {
        let names = gate_names();
        assert_eq!(
            names,
            [
                "commits",
                "lockfiles",
                "governance",
                "host",
                "compatibility",
                "supply-chain",
                "portable",
                "firmware"
            ]
        );
        assert_eq!(
            names
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            names.len()
        );
    }

    #[test]
    fn checksum_lines_require_safe_lowercase_sha256_entries() {
        let checksum = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            parse_checksum_line(&format!("{checksum}  manifest.json"))
                .expect("valid checksum must parse"),
            (checksum, "manifest.json")
        );
        assert!(parse_checksum_line(&format!("{checksum} *manifest.json")).is_err());
        assert!(parse_checksum_line(&format!("{checksum}  ../manifest.json")).is_err());
        assert!(
            parse_checksum_line(&format!("{}  manifest.json", checksum.to_uppercase())).is_err()
        );
    }

    #[test]
    fn repository_workflow_uses_canonical_xtask_gates() {
        let root = workspace_root().expect("workspace root");
        let manifest = read_toolchain_manifest(&root).expect("toolchain manifest");
        verify_workflow_contract(&root, &manifest).expect("workflow contract must match");
    }
}
