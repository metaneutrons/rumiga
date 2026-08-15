// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    ToolchainManifest, capture, capture_git, read_toolchain_manifest, reset_generated_directory,
    sha256_file, verify_ci_source, workspace_root, write_manifest_and_checksums,
};

pub const EVIDENCE_DIRECTORY: &str = "target/m0-011-compatibility-evidence";

const CATALOG_PATH: &str = "evidence/scenarios.json";
const IGNORE_POLICY_PATH: &str = "evidence/ignored-tests.json";
const RUST_API_PATH: &str = "crates/rumiga-api/src/lib.rs";
const TYPESCRIPT_API_PATH: &str = "web/src/lib/api.ts";

const DTO_STRUCTS: [&str; 25] = [
    "FileEntry",
    "FileListResponse",
    "FormatRequest",
    "FloppyInsertRequest",
    "FloppyEjectRequest",
    "AudioSeparationRequest",
    "WifiNetwork",
    "WifiStatus",
    "WifiConnectRequest",
    "WifiScanResponse",
    "ChannelMixConfig",
    "AudioConfig",
    "ViewportConfig",
    "DisplayConfig",
    "NetworkConfig",
    "NetworkPacketCounters",
    "NetworkStatus",
    "MachineConfig",
    "MachineStatus",
    "SupportBundle",
    "SupportMachineSummary",
    "SupportMediaSummary",
    "SupportScreenshotSummary",
    "ApiEndpoint",
    "ApiResponse",
];

const DTO_ENUMS: [&str; 10] = [
    "WifiMode",
    "AmigaModel",
    "ScalingMode",
    "ViewportMode",
    "ViewportPreset",
    "ScreenshotKind",
    "HdfWritePolicy",
    "NetworkDevice",
    "NetworkBackend",
    "ApiResponseFormat",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SchemaVersion {
    id: String,
    version: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioCatalog {
    schema: SchemaVersion,
    scenarios: Vec<CatalogScenario>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogScenario {
    id: String,
    tier: String,
    machine: String,
    cpu: String,
    video_standard: String,
    #[serde(default)]
    target_count: Option<u32>,
    milestone: String,
    command: String,
    required_assets: Vec<String>,
    status_when_missing: String,
    #[serde(default)]
    public_ci: Option<PublicCiRunner>,
    notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicCiRunner {
    runner: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IgnorePolicy {
    schema: SchemaVersion,
    entries: Vec<IgnorePolicyEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct IgnorePolicyEntry {
    id: String,
    reason_code: String,
    reason: String,
    tracking: String,
}

#[derive(Clone, Debug, Serialize)]
struct SourceRevision {
    revision: String,
    date_epoch: u64,
    dirty: bool,
}

#[derive(Debug, Serialize)]
struct PublicScope {
    kind: String,
    private_media_included: bool,
    local_evidence_read: bool,
}

#[derive(Debug, Default, Serialize)]
struct CompatibilitySummary {
    total: usize,
    pass: usize,
    skipped: usize,
    unsupported: usize,
    partial: usize,
    fail: usize,
}

#[derive(Debug, Serialize)]
struct ScenarioReason {
    code: String,
    detail: String,
}

#[derive(Debug, Serialize)]
struct PublicScenarioResult {
    id: String,
    tier: String,
    machine: String,
    cpu: String,
    video_standard: String,
    milestone: String,
    status: String,
    reason: ScenarioReason,
    reproduction_command: String,
    required_assets: Vec<String>,
    required_asset_count: usize,
}

#[derive(Debug, Serialize)]
struct CompatibilityReport {
    schema: SchemaVersion,
    source: SourceRevision,
    scope: PublicScope,
    summary: CompatibilitySummary,
    scenarios: Vec<PublicScenarioResult>,
}

#[derive(Debug, Serialize)]
struct SourceFileEvidence {
    path: String,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct ControlPlaneSummary {
    structs: usize,
    enums: usize,
    endpoints: usize,
}

#[derive(Debug, Serialize)]
struct ControlPlaneSources {
    rust: SourceFileEvidence,
    typescript: SourceFileEvidence,
}

#[derive(Debug, Serialize)]
struct ControlPlaneEvidence {
    schema: SchemaVersion,
    source: SourceRevision,
    scenario: String,
    result: String,
    summary: ControlPlaneSummary,
    structs: Vec<String>,
    enums: Vec<String>,
    endpoints: Vec<String>,
    failures: Vec<String>,
    sources: ControlPlaneSources,
}

#[derive(Debug, Default, Serialize)]
struct TestCounts {
    discovered: usize,
    ignored: usize,
    runnable: usize,
}

#[derive(Debug, Serialize)]
struct TestInventorySummary {
    harness: TestCounts,
    documentation: TestCounts,
    total: TestCounts,
}

#[derive(Debug, Serialize)]
struct TestCase {
    id: String,
    kind: String,
    suite: String,
    name: String,
    ignored: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignored_reason: Option<IgnoreReason>,
}

#[derive(Debug, Serialize)]
struct IgnoreReason {
    code: String,
    detail: String,
    tracking: String,
}

#[derive(Debug, Serialize)]
struct TestInventoryCommands {
    compile_harnesses: String,
    list_documentation: String,
    list_ignored_documentation: String,
    execute_required_tests: String,
}

#[derive(Debug, Serialize)]
struct TestInventory {
    schema: SchemaVersion,
    source: SourceRevision,
    summary: TestInventorySummary,
    commands: TestInventoryCommands,
    definitions: BTreeMap<String, String>,
    tests: Vec<TestCase>,
}

#[derive(Debug, Serialize)]
struct ArtifactEvidence {
    name: String,
    role: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct BundleCommands {
    generate: String,
    quality_gate: String,
    execute_required_tests: String,
}

#[derive(Debug, Serialize)]
struct BundleSummary {
    compatibility: CompatibilitySummary,
    tests: TestCounts,
}

#[derive(Debug, Serialize)]
struct BundleManifest {
    schema: SchemaVersion,
    source: SourceRevision,
    scope: PublicScope,
    inputs: BTreeMap<String, String>,
    commands: BundleCommands,
    summary: BundleSummary,
    artifacts: Vec<ArtifactEvidence>,
    claims: Vec<String>,
    exclusions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    id: String,
    name: String,
}

#[derive(Debug)]
struct HarnessTarget {
    package: String,
    target: String,
    kind: String,
    executable: PathBuf,
}

pub fn build_evidence() -> Result<()> {
    let root = workspace_root()?;
    let toolchain = read_toolchain_manifest(&root)?;
    verify_rust_toolchain(&root, &toolchain)?;

    let target_root = root.join("target");
    let evidence_root = root.join(EVIDENCE_DIRECTORY);
    reset_generated_directory(&target_root, &evidence_root)?;

    let source = source_revision(&root)?;
    verify_ci_source(&source.revision, source.dirty)?;

    let catalog: ScenarioCatalog = read_json(&root.join(CATALOG_PATH))?;
    validate_catalog(&catalog)?;
    let ignore_policy: IgnorePolicy = read_json(&root.join(IGNORE_POLICY_PATH))?;
    validate_ignore_policy(&ignore_policy)?;

    let control_plane = build_control_plane_evidence(&root, &source)?;
    let test_inventory = build_test_inventory(&root, &toolchain, &source, &ignore_policy)?;
    let compatibility = build_compatibility_report(&catalog, &control_plane, &source);

    write_json(&evidence_root.join("control-plane.json"), &control_plane)?;
    write_json(&evidence_root.join("test-inventory.json"), &test_inventory)?;
    write_json(&evidence_root.join("compatibility.json"), &compatibility)?;
    fs::write(
        evidence_root.join("compatibility.md"),
        render_markdown(&compatibility, &test_inventory, &toolchain)?,
    )?;

    let artifacts = artifact_evidence(&evidence_root)?;
    let inputs = input_hashes(&root)?;
    let manifest = BundleManifest {
        schema: schema("rumiga.public-evidence.bundle.v1"),
        source,
        scope: public_scope(),
        inputs,
        commands: BundleCommands {
            generate: format!(
                "cargo +{} xtask compatibility-evidence",
                toolchain.host.rust
            ),
            quality_gate: format!(
                "cargo +{} xtask ci --gate compatibility",
                toolchain.host.rust
            ),
            execute_required_tests: format!(
                "cargo +{} test --locked --workspace",
                toolchain.host.rust
            ),
        },
        summary: BundleSummary {
            compatibility: clone_compatibility_summary(&compatibility.summary),
            tests: clone_test_counts(&test_inventory.summary.total),
        },
        artifacts,
        claims: vec![
            "scenario-catalog-classified".to_owned(),
            "asset-free-rest-web-contract-verified".to_owned(),
            "cargo-test-inventory-discovered".to_owned(),
            "ignored-tests-match-reviewed-policy".to_owned(),
            "private-media-free".to_owned(),
        ],
        exclusions: vec![
            "no-rom".to_owned(),
            "no-adf".to_owned(),
            "no-hdf".to_owned(),
            "no-screenshot".to_owned(),
            "no-packet-capture".to_owned(),
            "no-local-target-evidence".to_owned(),
            "runtime-media-conditioned-test-returns-are-not-libtest-skips".to_owned(),
        ],
    };
    write_manifest_and_checksums(&evidence_root, &manifest)?;
    verify_public_bundle(&root, &evidence_root)?;

    ensure!(
        control_plane.failures.is_empty(),
        "REST/Web control-plane parity failed; inspect control-plane.json"
    );
    ensure!(
        compatibility.summary.fail == 0,
        "public compatibility report contains failed scenarios"
    );

    println!("compatibility evidence: {}", evidence_root.display());
    Ok(())
}

fn schema(id: &str) -> SchemaVersion {
    SchemaVersion {
        id: id.to_owned(),
        version: 1,
    }
}

fn public_scope() -> PublicScope {
    PublicScope {
        kind: "public-ci".to_owned(),
        private_media_included: false,
        local_evidence_read: false,
    }
}

fn source_revision(root: &Path) -> Result<SourceRevision> {
    Ok(SourceRevision {
        revision: capture_git(root, &["rev-parse", "HEAD"])?,
        date_epoch: capture_git(root, &["show", "-s", "--format=%ct", "HEAD"])?
            .parse::<u64>()
            .context("git commit timestamp must be an unsigned integer")?,
        dirty: !capture_git(root, &["status", "--porcelain"])?.is_empty(),
    })
}

fn verify_rust_toolchain(root: &Path, manifest: &ToolchainManifest) -> Result<()> {
    let rustc = capture(
        root,
        "rustup",
        &["run", &manifest.host.rust, "rustc", "--version"],
    )?;
    ensure!(
        rustc.split_whitespace().nth(1) == Some(manifest.host.rust.as_str()),
        "rustc does not match toolchain/manifest.toml: {rustc}"
    );
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut contents = serde_json::to_vec_pretty(value)?;
    contents.push(b'\n');
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn validate_catalog(catalog: &ScenarioCatalog) -> Result<()> {
    ensure!(
        catalog.schema.id == "rumiga.evidence.scenario-catalog.v1" && catalog.schema.version == 1,
        "unsupported compatibility scenario catalog schema"
    );
    ensure!(!catalog.scenarios.is_empty(), "scenario catalog is empty");

    let mut ids = BTreeSet::new();
    let mut public_runner_count = 0;
    for scenario in &catalog.scenarios {
        validate_catalog_scenario(scenario)?;
        ensure!(
            ids.insert(&scenario.id),
            "duplicate scenario id {:?}",
            scenario.id
        );
        if let Some(public_ci) = &scenario.public_ci {
            public_runner_count += 1;
            ensure!(
                public_ci.runner == "api-dto-parity",
                "scenario {} names an unknown public CI runner",
                scenario.id
            );
            ensure!(
                scenario.required_assets.is_empty(),
                "public CI scenario {} requires private assets",
                scenario.id
            );
        }
    }
    ensure!(
        public_runner_count == 1,
        "scenario catalog must contain exactly one asset-free public CI runner"
    );
    Ok(())
}

fn validate_catalog_scenario(scenario: &CatalogScenario) -> Result<()> {
    ensure!(
        valid_identifier(&scenario.id),
        "invalid scenario id {:?}",
        scenario.id
    );
    for (field, value) in [
        ("tier", scenario.tier.as_str()),
        ("machine", scenario.machine.as_str()),
        ("cpu", scenario.cpu.as_str()),
        ("video_standard", scenario.video_standard.as_str()),
        ("milestone", scenario.milestone.as_str()),
        ("command", scenario.command.as_str()),
    ] {
        ensure!(
            !value.trim().is_empty(),
            "scenario {} has an empty {field}",
            scenario.id
        );
    }
    ensure!(
        !scenario.notes.is_empty(),
        "scenario {} has no review notes",
        scenario.id
    );
    ensure!(
        scenario.target_count.is_none_or(|count| count > 0),
        "scenario {} has an invalid target count",
        scenario.id
    );
    ensure!(
        matches!(
            scenario.status_when_missing.as_str(),
            "skipped-missing-assets" | "unsupported-out-of-scope"
        ),
        "scenario {} has unsupported missing status",
        scenario.id
    );
    ensure_public_text(
        &scenario.command,
        &format!("scenario {} command", scenario.id),
    )?;
    for asset in &scenario.required_assets {
        ensure!(
            !asset.trim().is_empty(),
            "scenario {} has an empty asset label",
            scenario.id
        );
        ensure_public_text(asset, &format!("scenario {} asset label", scenario.id))?;
    }

    if scenario.tier == "out-of-scope" {
        ensure!(
            scenario.status_when_missing == "unsupported-out-of-scope"
                && scenario.command == "n/a"
                && scenario.required_assets.is_empty()
                && scenario.public_ci.is_none(),
            "out-of-scope scenario {} has an inconsistent contract",
            scenario.id
        );
    } else if scenario.public_ci.is_some() {
        ensure!(
            scenario.required_assets.is_empty(),
            "public CI scenario {} requires private assets",
            scenario.id
        );
    } else {
        ensure!(
            !scenario.required_assets.is_empty()
                && scenario.status_when_missing == "skipped-missing-assets",
            "in-scope scenario {} has an inconsistent private-asset contract",
            scenario.id
        );
    }
    Ok(())
}

fn validate_ignore_policy(policy: &IgnorePolicy) -> Result<()> {
    ensure!(
        policy.schema.id == "rumiga.test-ignore-policy.v1" && policy.schema.version == 1,
        "unsupported ignored-test policy schema"
    );
    let mut ids = BTreeSet::new();
    for entry in &policy.entries {
        ensure!(
            !entry.id.trim().is_empty(),
            "ignored-test policy has an empty id"
        );
        ensure!(
            ids.insert(&entry.id),
            "duplicate ignored-test policy id {:?}",
            entry.id
        );
        ensure!(
            valid_reason_code(&entry.reason_code),
            "invalid reason code for {}",
            entry.id
        );
        ensure!(
            !entry.reason.trim().is_empty(),
            "ignored test {} has no reason",
            entry.id
        );
        ensure!(
            !entry.tracking.trim().is_empty(),
            "ignored test {} has no tracking id",
            entry.id
        );
        ensure_public_text(&entry.reason, &format!("ignored test {} reason", entry.id))?;
    }
    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_reason_code(value: &str) -> bool {
    valid_identifier(value)
}

fn build_control_plane_evidence(
    root: &Path,
    source: &SourceRevision,
) -> Result<ControlPlaneEvidence> {
    let rust_path = root.join(RUST_API_PATH);
    let typescript_path = root.join(TYPESCRIPT_API_PATH);
    let rust = fs::read_to_string(&rust_path)?;
    let typescript = fs::read_to_string(&typescript_path)?;
    let mut failures = Vec::new();

    for name in DTO_STRUCTS {
        compare_contract(
            "struct",
            name,
            &rust_struct_fields(&rust, name)?,
            &typescript_interface_fields(&typescript, name)?,
            &mut failures,
        );
    }
    for name in DTO_ENUMS {
        compare_contract(
            "enum",
            name,
            &rust_enum_variants(&rust, name)?,
            &typescript_union_variants(&typescript, name)?,
            &mut failures,
        );
    }
    let rust_endpoints = rust_api_endpoints(&rust)?;
    let typescript_endpoints = typescript_api_endpoints(&typescript)?;
    compare_contract(
        "endpoint",
        "API_ENDPOINTS",
        &rust_endpoints,
        &typescript_endpoints,
        &mut failures,
    );

    Ok(ControlPlaneEvidence {
        schema: schema("rumiga.control-plane-evidence.v1"),
        source: source.clone(),
        scenario: "rest-web-control-roundtrip".to_owned(),
        result: if failures.is_empty() { "pass" } else { "fail" }.to_owned(),
        summary: ControlPlaneSummary {
            structs: DTO_STRUCTS.len(),
            enums: DTO_ENUMS.len(),
            endpoints: rust_endpoints.len(),
        },
        structs: DTO_STRUCTS.iter().map(ToString::to_string).collect(),
        enums: DTO_ENUMS.iter().map(ToString::to_string).collect(),
        endpoints: rust_endpoints,
        failures,
        sources: ControlPlaneSources {
            rust: SourceFileEvidence {
                path: RUST_API_PATH.to_owned(),
                sha256: sha256_file(&rust_path)?,
            },
            typescript: SourceFileEvidence {
                path: TYPESCRIPT_API_PATH.to_owned(),
                sha256: sha256_file(&typescript_path)?,
            },
        },
    })
}

fn compare_contract(
    kind: &str,
    name: &str,
    rust_values: &[String],
    typescript_values: &[String],
    failures: &mut Vec<String>,
) {
    if rust_values != typescript_values {
        failures.push(format!(
            "{kind} {name} differs: rust=[{}], typescript=[{}]",
            rust_values.join(", "),
            typescript_values.join(", ")
        ));
    }
}

fn rust_struct_fields(source: &str, name: &str) -> Result<Vec<String>> {
    let marker = format!("pub struct {name}");
    let body = braced_declaration(source, &marker)?;
    Ok(body
        .lines()
        .filter_map(|line| {
            line.trim().strip_prefix("pub ").and_then(|field| {
                field
                    .split_once(':')
                    .map(|(name, _)| name.trim().to_owned())
            })
        })
        .collect())
}

fn typescript_interface_fields(source: &str, name: &str) -> Result<Vec<String>> {
    let marker = format!("export interface {name}");
    let body = braced_declaration(source, &marker)?;
    Ok(body
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                None
            } else {
                line.split_once(':')
                    .map(|(field, _)| field.trim().trim_end_matches('?').to_owned())
            }
        })
        .collect())
}

fn rust_enum_variants(source: &str, name: &str) -> Result<Vec<String>> {
    let marker = format!("pub enum {name}");
    let body = braced_declaration(source, &marker)?;
    Ok(body
        .lines()
        .map(str::trim)
        .map(|line| line.trim_end_matches(',').trim())
        .filter(|line| {
            !line.is_empty()
                && line
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_uppercase())
                && line
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        .map(ToString::to_string)
        .collect())
}

fn typescript_union_variants(source: &str, name: &str) -> Result<Vec<String>> {
    let marker = format!("export type {name}");
    let start = source
        .find(&marker)
        .with_context(|| format!("missing TypeScript type {name}"))?;
    let declaration = &source[start + marker.len()..];
    let equals = declaration
        .find('=')
        .with_context(|| format!("TypeScript type {name} has no equals sign"))?;
    let declaration = &declaration[equals + 1..];
    let semicolon = declaration
        .find(';')
        .with_context(|| format!("TypeScript type {name} is unterminated"))?;
    let variants = declaration[..semicolon]
        .split('|')
        .map(str::trim)
        .map(|value| value.trim_matches('\''))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    ensure!(
        !variants.is_empty() && variants.iter().all(|value| !value.is_empty()),
        "TypeScript type {name} has no variants"
    );
    Ok(variants)
}

fn rust_api_endpoints(source: &str) -> Result<Vec<String>> {
    let mut paths = BTreeMap::new();
    for line in source.lines().map(str::trim) {
        let Some(declaration) = line.strip_prefix("pub const ") else {
            continue;
        };
        let Some((name, value)) = declaration.split_once(": &str = ") else {
            continue;
        };
        if name.ends_with("_PATH") {
            let value = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix("\";"))
                .with_context(|| format!("invalid path constant {name}"))?;
            paths.insert(name.to_owned(), value.to_owned());
        }
    }

    let body = assigned_array(source, "pub const API_ENDPOINTS")?;
    let mut endpoints = Vec::new();
    let mut remaining = body;
    while let Some(offset) = remaining.find("ApiEndpoint::new") {
        remaining = &remaining[offset + "ApiEndpoint::new".len()..];
        let open = remaining
            .find('(')
            .context("ApiEndpoint::new has no argument list")?;
        let (arguments, end) = delimited_at(remaining, open, '(', ')')?;
        let arguments = arguments
            .split(',')
            .map(str::trim)
            .filter(|argument| !argument.is_empty())
            .collect::<Vec<_>>();
        ensure!(
            arguments.len() == 3,
            "ApiEndpoint::new must have three arguments"
        );
        let method = arguments[0]
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .context("API endpoint method must be a string literal")?;
        let path = paths
            .get(arguments[1])
            .with_context(|| format!("unknown API endpoint path constant {}", arguments[1]))?;
        let response_format = arguments[2]
            .strip_prefix("ApiResponseFormat::")
            .context("API endpoint has an invalid response format")?;
        endpoints.push(format!("{method} {path} {response_format}"));
        remaining = &remaining[end..];
    }
    ensure!(!endpoints.is_empty(), "Rust API_ENDPOINTS is empty");
    Ok(endpoints)
}

fn typescript_api_endpoints(source: &str) -> Result<Vec<String>> {
    let body = assigned_array(source, "export const API_ENDPOINTS")?;
    let mut endpoints = Vec::new();
    let mut remaining = body;
    while let Some(open) = remaining.find('{') {
        let (object, end) = delimited_at(remaining, open, '{', '}')?;
        let fields = object
            .split(',')
            .filter_map(|field| field.split_once(':'))
            .map(|(name, value)| {
                (
                    name.trim(),
                    value.trim().trim_matches('\'').trim_matches('"'),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let method = fields.get("method").context("endpoint has no method")?;
        let path = fields.get("path").context("endpoint has no path")?;
        let response_format = fields
            .get("response_format")
            .context("endpoint has no response format")?;
        endpoints.push(format!("{method} {path} {response_format}"));
        remaining = &remaining[end..];
    }
    ensure!(!endpoints.is_empty(), "TypeScript API_ENDPOINTS is empty");
    Ok(endpoints)
}

fn braced_declaration<'a>(source: &'a str, marker: &str) -> Result<&'a str> {
    let start = source
        .find(marker)
        .with_context(|| format!("missing declaration {marker}"))?;
    let declaration = &source[start + marker.len()..];
    let open = declaration
        .find('{')
        .with_context(|| format!("declaration {marker} has no body"))?;
    delimited_at(declaration, open, '{', '}').map(|(body, _)| body)
}

fn assigned_array<'a>(source: &'a str, marker: &str) -> Result<&'a str> {
    let start = source
        .find(marker)
        .with_context(|| format!("missing declaration {marker}"))?;
    let declaration = &source[start + marker.len()..];
    let equals = declaration
        .find('=')
        .with_context(|| format!("declaration {marker} has no equals sign"))?;
    let assigned = &declaration[equals + 1..];
    let open = assigned
        .find('[')
        .with_context(|| format!("declaration {marker} has no array"))?;
    delimited_at(assigned, open, '[', ']').map(|(body, _)| body)
}

fn delimited_at(source: &str, open_index: usize, open: char, close: char) -> Result<(&str, usize)> {
    ensure!(
        source[open_index..].starts_with(open),
        "delimiter start does not point at {open}"
    );
    let mut depth = 0_u32;
    let mut quote = None;
    let mut escaped = false;
    for (relative, character) in source[open_index..].char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }
        if character == '"'
            || (character == '\''
                && source[open_index + relative + character.len_utf8()..]
                    .split('\n')
                    .next()
                    .is_some_and(|line| line.contains('\'')))
        {
            quote = Some(character);
        } else if character == open {
            depth += 1;
        } else if character == close {
            depth = depth.checked_sub(1).context("delimiter depth underflow")?;
            if depth == 0 {
                let close_index = open_index + relative;
                return Ok((
                    &source[open_index + open.len_utf8()..close_index],
                    close_index + close.len_utf8(),
                ));
            }
        }
    }
    bail!("unterminated {open}{close} block")
}

fn build_test_inventory(
    root: &Path,
    toolchain: &ToolchainManifest,
    source: &SourceRevision,
    policy: &IgnorePolicy,
) -> Result<TestInventory> {
    let rust = &toolchain.host.rust;
    let compile_command = format!(
        "cargo +{rust} test --locked --workspace --no-run --message-format=json-render-diagnostics"
    );
    let doc_command =
        format!("cargo +{rust} test --locked --workspace --doc -- --list --format terse");
    let ignored_doc_command =
        format!("cargo +{rust} test --locked --workspace --doc -- --list --format terse --ignored");

    let mut tests = discover_harness_tests(root, rust)?;
    tests.extend(discover_documentation_tests(root, rust)?);
    tests.sort_by(|left, right| left.id.cmp(&right.id));
    ensure_unique_test_ids(&tests)?;
    apply_ignore_policy(&mut tests, policy)?;

    let harness = count_tests(tests.iter().filter(|test| test.kind == "harness"))?;
    let documentation = count_tests(tests.iter().filter(|test| test.kind == "documentation"))?;
    let total = count_tests(tests.iter())?;
    ensure!(
        total.discovered == harness.discovered + documentation.discovered
            && total.ignored == harness.ignored + documentation.ignored
            && total.runnable == harness.runnable + documentation.runnable,
        "test inventory category totals are inconsistent"
    );

    let mut definitions = BTreeMap::new();
    definitions.insert(
        "discovered".to_owned(),
        "A test name emitted by a Cargo-built libtest harness or rustdoc --list.".to_owned(),
    );
    definitions.insert(
        "ignored".to_owned(),
        "A framework-level ignored test whose stable id exactly matches the reviewed policy."
            .to_owned(),
    );
    definitions.insert(
        "runnable".to_owned(),
        "A discovered test not marked ignored; the separate required host gate executes it."
            .to_owned(),
    );

    Ok(TestInventory {
        schema: schema("rumiga.test-inventory.v1"),
        source: source.clone(),
        summary: TestInventorySummary {
            harness,
            documentation,
            total,
        },
        commands: TestInventoryCommands {
            compile_harnesses: compile_command,
            list_documentation: doc_command,
            list_ignored_documentation: ignored_doc_command,
            execute_required_tests: format!("cargo +{rust} test --locked --workspace"),
        },
        definitions,
        tests,
    })
}

fn discover_harness_tests(root: &Path, rust: &str) -> Result<Vec<TestCase>> {
    let packages = cargo_package_names(root, rust)?;
    let targets = cargo_harness_targets(root, rust, &packages)?;
    let mut tests = Vec::new();
    for target in targets.into_values() {
        let all = list_harness_tests(root, &target.executable, false)?;
        let ignored = list_harness_tests(root, &target.executable, true)?;
        ensure!(
            ignored.is_subset(&all),
            "ignored tests are not a subset for {}:{}",
            target.package,
            target.target
        );
        let suite = format!("{}:{}:{}", target.package, target.kind, target.target);
        for name in all {
            let id = format!("harness:{suite}:{name}");
            tests.push(TestCase {
                id,
                kind: "harness".to_owned(),
                suite: suite.clone(),
                ignored: ignored.contains(&name),
                name,
                ignored_reason: None,
            });
        }
    }
    Ok(tests)
}

fn cargo_package_names(root: &Path, rust: &str) -> Result<BTreeMap<String, String>> {
    let metadata_output = command_output(
        Command::new("cargo")
            .current_dir(root)
            .args([
                format!("+{rust}"),
                "metadata".to_owned(),
                "--locked".to_owned(),
                "--no-deps".to_owned(),
                "--format-version".to_owned(),
                "1".to_owned(),
            ])
            .stdin(Stdio::null()),
        "Cargo package metadata",
    )?;
    let metadata: CargoMetadata = serde_json::from_slice(&metadata_output.stdout)?;
    Ok(metadata
        .packages
        .into_iter()
        .map(|package| (package.id, package.name))
        .collect())
}

fn cargo_harness_targets(
    root: &Path,
    rust: &str,
    packages: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, HarnessTarget>> {
    let compile_output = command_output(
        Command::new("cargo")
            .current_dir(root)
            .args([
                format!("+{rust}"),
                "test".to_owned(),
                "--locked".to_owned(),
                "--workspace".to_owned(),
                "--no-run".to_owned(),
                "--message-format=json-render-diagnostics".to_owned(),
            ])
            .stdin(Stdio::null()),
        "Cargo test-harness discovery build",
    )?;
    let stdout = String::from_utf8(compile_output.stdout)
        .context("Cargo test-harness discovery output is not UTF-8")?;
    let mut targets = BTreeMap::new();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let message: Value = serde_json::from_str(line)
            .with_context(|| format!("Cargo emitted a non-JSON message: {line}"))?;
        if message.get("reason").and_then(Value::as_str) != Some("compiler-artifact")
            || message.pointer("/profile/test").and_then(Value::as_bool) != Some(true)
        {
            continue;
        }
        let Some(executable) = message.get("executable").and_then(Value::as_str) else {
            continue;
        };
        let package_id = message
            .get("package_id")
            .and_then(Value::as_str)
            .context("test artifact has no package id")?;
        let package = packages
            .get(package_id)
            .with_context(|| format!("test artifact references unknown package {package_id}"))?;
        let target = message
            .pointer("/target/name")
            .and_then(Value::as_str)
            .context("test artifact has no target name")?;
        let kind = message
            .pointer("/target/kind")
            .and_then(Value::as_array)
            .context("test artifact has no target kind")?
            .iter()
            .map(|value| value.as_str().context("target kind is not a string"))
            .collect::<Result<Vec<_>>>()?
            .join("+");
        let key = format!("{package}:{kind}:{target}");
        ensure!(
            targets
                .insert(
                    key.clone(),
                    HarnessTarget {
                        package: package.clone(),
                        target: target.to_owned(),
                        kind,
                        executable: PathBuf::from(executable),
                    },
                )
                .is_none(),
            "Cargo emitted duplicate test target {key}"
        );
    }
    ensure!(!targets.is_empty(), "Cargo discovered no test harnesses");
    Ok(targets)
}

fn list_harness_tests(root: &Path, executable: &Path, ignored: bool) -> Result<BTreeSet<String>> {
    let mut command = Command::new(executable);
    command
        .current_dir(root)
        .args(["--list", "--format", "terse"])
        .stdin(Stdio::null());
    if ignored {
        command.arg("--ignored");
    }
    let description = format!("test listing for {}", executable.display());
    let output = command_output(&mut command, &description)?;
    parse_libtest_names(&output.stdout, &description)
}

fn discover_documentation_tests(root: &Path, rust: &str) -> Result<Vec<TestCase>> {
    let all = list_documentation_tests(root, rust, false)?;
    let ignored = list_documentation_tests(root, rust, true)?;
    ensure!(
        ignored.is_subset(&all),
        "ignored documentation tests are not a subset of discovered documentation tests"
    );
    Ok(all
        .into_iter()
        .map(|name| TestCase {
            id: format!("documentation:{name}"),
            kind: "documentation".to_owned(),
            suite: "rustdoc".to_owned(),
            ignored: ignored.contains(&name),
            name,
            ignored_reason: None,
        })
        .collect())
}

fn list_documentation_tests(root: &Path, rust: &str, ignored: bool) -> Result<BTreeSet<String>> {
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .args([
            format!("+{rust}"),
            "test".to_owned(),
            "--locked".to_owned(),
            "--workspace".to_owned(),
            "--doc".to_owned(),
            "--".to_owned(),
            "--list".to_owned(),
            "--format".to_owned(),
            "terse".to_owned(),
        ])
        .stdin(Stdio::null());
    if ignored {
        command.arg("--ignored");
    }
    let description = if ignored {
        "ignored documentation-test listing"
    } else {
        "documentation-test listing"
    };
    let output = command_output(&mut command, description)?;
    parse_libtest_names(&output.stdout, description).map(|names| {
        names
            .into_iter()
            .map(|name| normalize_documentation_test_name(&name))
            .collect()
    })
}

fn parse_libtest_names(output: &[u8], description: &str) -> Result<BTreeSet<String>> {
    let output = std::str::from_utf8(output)
        .with_context(|| format!("{description} output is not UTF-8"))?;
    let mut names = BTreeSet::new();
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.starts_with("all doctests ran in ")
            && line.contains("merged doctests compilation took")
        {
            continue;
        }
        let name = line
            .strip_suffix(": test")
            .with_context(|| format!("unexpected {description} output line: {line}"))?;
        ensure!(
            names.insert(name.to_owned()),
            "duplicate test name {name:?}"
        );
    }
    Ok(names)
}

fn normalize_documentation_test_name(name: &str) -> String {
    let stable = name
        .rfind(" (line ")
        .filter(|_| name.ends_with(')'))
        .map_or(name, |index| &name[..index]);
    stable.trim_end_matches(" -").to_owned()
}

fn command_output(command: &mut Command, description: &str) -> Result<Output> {
    let output = command
        .output()
        .with_context(|| format!("failed to start {description}"))?;
    ensure!(
        output.status.success(),
        "{description} failed with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output)
}

fn ensure_unique_test_ids(tests: &[TestCase]) -> Result<()> {
    let mut ids = BTreeSet::new();
    for test in tests {
        ensure!(
            ids.insert(&test.id),
            "duplicate stable test id {:?}",
            test.id
        );
    }
    Ok(())
}

fn apply_ignore_policy(tests: &mut [TestCase], policy: &IgnorePolicy) -> Result<()> {
    let entries = policy
        .entries
        .iter()
        .map(|entry| (entry.id.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let actual = tests
        .iter()
        .filter(|test| test.ignored)
        .map(|test| test.id.as_str())
        .collect::<BTreeSet<_>>();
    let reviewed = entries.keys().copied().collect::<BTreeSet<_>>();
    ensure!(
        actual == reviewed,
        "ignored-test policy drift: actual-only={:?}, policy-only={:?}",
        actual.difference(&reviewed).collect::<Vec<_>>(),
        reviewed.difference(&actual).collect::<Vec<_>>()
    );

    for test in tests.iter_mut().filter(|test| test.ignored) {
        let entry = entries
            .get(test.id.as_str())
            .context("ignored test has no reviewed policy entry")?;
        test.ignored_reason = Some(IgnoreReason {
            code: entry.reason_code.clone(),
            detail: entry.reason.clone(),
            tracking: entry.tracking.clone(),
        });
    }
    Ok(())
}

fn count_tests<'a>(tests: impl Iterator<Item = &'a TestCase>) -> Result<TestCounts> {
    let mut counts = TestCounts::default();
    for test in tests {
        counts.discovered += 1;
        if test.ignored {
            counts.ignored += 1;
        }
    }
    counts.runnable = counts
        .discovered
        .checked_sub(counts.ignored)
        .context("ignored test count exceeds discovered test count")?;
    Ok(counts)
}

fn build_compatibility_report(
    catalog: &ScenarioCatalog,
    control_plane: &ControlPlaneEvidence,
    source: &SourceRevision,
) -> CompatibilityReport {
    let mut summary = CompatibilitySummary::default();
    let scenarios = catalog
        .scenarios
        .iter()
        .map(|scenario| {
            let (status, reason) = classify_public_scenario(scenario, control_plane);
            summary.total += 1;
            match status {
                "pass" => summary.pass += 1,
                "skipped" => summary.skipped += 1,
                "unsupported" => summary.unsupported += 1,
                "partial" => summary.partial += 1,
                "fail" => summary.fail += 1,
                _ => unreachable!("classification returned an unknown status"),
            }
            PublicScenarioResult {
                id: scenario.id.clone(),
                tier: scenario.tier.clone(),
                machine: scenario.machine.clone(),
                cpu: scenario.cpu.clone(),
                video_standard: scenario.video_standard.clone(),
                milestone: scenario.milestone.clone(),
                status: status.to_owned(),
                reason,
                reproduction_command: scenario.command.clone(),
                required_asset_count: scenario.required_assets.len(),
                required_assets: scenario.required_assets.clone(),
            }
        })
        .collect();

    CompatibilityReport {
        schema: schema("rumiga.compatibility.report.v1"),
        source: source.clone(),
        scope: public_scope(),
        summary,
        scenarios,
    }
}

fn classify_public_scenario(
    scenario: &CatalogScenario,
    control_plane: &ControlPlaneEvidence,
) -> (&'static str, ScenarioReason) {
    if scenario.tier == "out-of-scope" {
        return (
            "unsupported",
            ScenarioReason {
                code: "roadmap-exclusion".to_owned(),
                detail: "The feature is explicitly excluded from the current product scope."
                    .to_owned(),
            },
        );
    }
    if scenario.public_ci.is_some() {
        if control_plane.failures.is_empty() {
            return (
                "pass",
                ScenarioReason {
                    code: "asset-free-contract-passed".to_owned(),
                    detail: "Rust DTOs, TypeScript DTOs, and REST endpoint contracts match."
                        .to_owned(),
                },
            );
        }
        return (
            "fail",
            ScenarioReason {
                code: "asset-free-contract-failed".to_owned(),
                detail: "Rust and TypeScript control-plane contracts differ; inspect control-plane.json."
                    .to_owned(),
            },
        );
    }
    (
        "skipped",
        ScenarioReason {
            code: "private-assets-unavailable-in-public-ci".to_owned(),
            detail: format!(
                "Requires {} local licensed/private asset input(s); public CI receives no ROM, ADF, HDF, corpus media, screenshot, or packet capture.",
                scenario.required_assets.len()
            ),
        },
    )
}

fn render_markdown(
    compatibility: &CompatibilityReport,
    inventory: &TestInventory,
    toolchain: &ToolchainManifest,
) -> Result<String> {
    let mut markdown = String::new();
    markdown.push_str("# Rumiga Public Compatibility Evidence\n\n");
    writeln!(
        markdown,
        "- Schema: {} version {}\n- Source revision: {}\n- Source date epoch: {}\n- Dirty source: {}\n- Scope: public CI; private media and local target/evidence are excluded.",
        compatibility.schema.id,
        compatibility.schema.version,
        compatibility.source.revision,
        compatibility.source.date_epoch,
        compatibility.source.dirty
    )?;
    markdown.push('\n');
    markdown.push_str("## Compatibility Summary\n\n");
    markdown.push_str("| Total | Pass | Skipped | Unsupported | Partial | Fail |\n");
    markdown.push_str("| ---: | ---: | ---: | ---: | ---: | ---: |\n");
    writeln!(
        markdown,
        "| {} | {} | {} | {} | {} | {} |",
        compatibility.summary.total,
        compatibility.summary.pass,
        compatibility.summary.skipped,
        compatibility.summary.unsupported,
        compatibility.summary.partial,
        compatibility.summary.fail
    )?;
    markdown.push('\n');
    markdown.push_str("| Scenario | Tier | Status | Reason code |\n");
    markdown.push_str("| --- | --- | --- | --- |\n");
    for scenario in &compatibility.scenarios {
        writeln!(
            markdown,
            "| {} | {} | {} | {} |",
            markdown_cell(&scenario.id),
            markdown_cell(&scenario.tier),
            markdown_cell(&scenario.status),
            markdown_cell(&scenario.reason.code)
        )?;
    }

    markdown.push_str("\n## Test Inventory\n\n");
    markdown.push_str("| Kind | Discovered | Ignored | Runnable |\n");
    markdown.push_str("| --- | ---: | ---: | ---: |\n");
    writeln!(
        markdown,
        "| Harness | {} | {} | {} |\n| Documentation | {} | {} | {} |\n| **Total** | **{}** | **{}** | **{}** |",
        inventory.summary.harness.discovered,
        inventory.summary.harness.ignored,
        inventory.summary.harness.runnable,
        inventory.summary.documentation.discovered,
        inventory.summary.documentation.ignored,
        inventory.summary.documentation.runnable,
        inventory.summary.total.discovered,
        inventory.summary.total.ignored,
        inventory.summary.total.runnable
    )?;
    markdown.push('\n');
    markdown.push_str(
        "Ignored tests are permitted only when their stable IDs exactly match evidence/ignored-tests.json.\n\n",
    );
    markdown.push_str("| Ignored test | Reason code | Tracking |\n");
    markdown.push_str("| --- | --- | --- |\n");
    for test in inventory.tests.iter().filter(|test| test.ignored) {
        if let Some(reason) = &test.ignored_reason {
            writeln!(
                markdown,
                "| {} | {} | {} |",
                markdown_cell(&test.id),
                markdown_cell(&reason.code),
                markdown_cell(&reason.tracking)
            )?;
        }
    }

    markdown.push_str("\n## Required Commands\n\nGenerate this bundle:\n\n");
    writeln!(
        markdown,
        "    cargo +{} xtask compatibility-evidence",
        toolchain.host.rust
    )?;
    markdown.push('\n');
    markdown.push_str("Run its required quality gate:\n\n");
    writeln!(
        markdown,
        "    cargo +{} xtask ci --gate compatibility",
        toolchain.host.rust
    )?;
    markdown.push('\n');
    markdown
        .push_str("Execute the runnable workspace tests (the separate host gate owns this):\n\n");
    writeln!(
        markdown,
        "    cargo +{} test --locked --workspace",
        toolchain.host.rust
    )?;
    markdown.push('\n');
    append_scenario_commands(&mut markdown, &compatibility.scenarios)?;
    markdown.push_str("\n## Evidence Boundary\n\n");
    markdown.push_str(
        "This public bundle does not read target/evidence and contains no ROM, ADF, HDF, screenshot, packet capture, local media hash, or local media path. A skipped private-media scenario is not a compatibility pass. Framework-runnable counts do not detect tests that return early at runtime because an optional private fixture is absent.\n",
    );
    Ok(markdown)
}

fn append_scenario_commands(
    markdown: &mut String,
    scenarios: &[PublicScenarioResult],
) -> Result<()> {
    markdown.push_str("## Scenario Reproduction Commands\n");
    for scenario in scenarios {
        writeln!(
            markdown,
            "\n### {}\n\nStatus: {}. Reason: {}.\n\n    {}",
            scenario.id, scenario.status, scenario.reason.code, scenario.reproduction_command
        )?;
    }
    Ok(())
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|")
}

fn artifact_evidence(root: &Path) -> Result<Vec<ArtifactEvidence>> {
    let roles = [
        (
            "compatibility.json",
            "machine-readable scenario classification",
        ),
        ("compatibility.md", "human-readable public report"),
        (
            "control-plane.json",
            "asset-free REST/Web contract evidence",
        ),
        ("test-inventory.json", "Cargo-discovered test inventory"),
    ];
    roles
        .into_iter()
        .map(|(name, role)| {
            let path = root.join(name);
            let metadata = fs::metadata(&path)
                .with_context(|| format!("failed to inspect {}", path.display()))?;
            Ok(ArtifactEvidence {
                name: name.to_owned(),
                role: role.to_owned(),
                bytes: metadata.len(),
                sha256: sha256_file(&path)?,
            })
        })
        .collect()
}

fn input_hashes(root: &Path) -> Result<BTreeMap<String, String>> {
    [
        "Cargo.lock",
        "toolchain/manifest.toml",
        CATALOG_PATH,
        IGNORE_POLICY_PATH,
        RUST_API_PATH,
        TYPESCRIPT_API_PATH,
    ]
    .into_iter()
    .map(|path| Ok((path.to_owned(), sha256_file(&root.join(path))?)))
    .collect()
}

fn verify_public_bundle(workspace: &Path, evidence_root: &Path) -> Result<()> {
    let expected = [
        "SHA256SUMS",
        "compatibility.json",
        "compatibility.md",
        "control-plane.json",
        "manifest.json",
        "test-inventory.json",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect::<BTreeSet<_>>();
    let actual = fs::read_dir(evidence_root)?
        .map(|entry| {
            let entry = entry?;
            ensure!(
                entry.file_type()?.is_file(),
                "public evidence contains a non-file entry"
            );
            entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("public evidence filename is not UTF-8"))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        actual == expected,
        "public evidence bundle has unexpected contents"
    );

    let workspace = workspace.to_string_lossy();
    let home = env::var("HOME").unwrap_or_default();
    for name in actual {
        let path = evidence_root.join(&name);
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("public evidence {} is not UTF-8", path.display()))?;
        ensure_public_text(&contents, &format!("public evidence file {name}"))?;
        ensure!(
            workspace.is_empty() || !contents.contains(workspace.as_ref()),
            "public evidence file {name} leaks the workspace path"
        );
        ensure!(
            home.is_empty() || home == "/" || !contents.contains(&home),
            "public evidence file {name} leaks the home directory"
        );
    }
    Ok(())
}

fn ensure_public_text(value: &str, description: &str) -> Result<()> {
    let normalized = value.replace('\\', "/").to_ascii_lowercase();
    for marker in ["/users/", "/home/", "c:/users/"] {
        ensure!(
            !normalized.contains(marker),
            "{description} contains a private path marker {marker:?}"
        );
    }
    Ok(())
}

fn clone_compatibility_summary(summary: &CompatibilitySummary) -> CompatibilitySummary {
    CompatibilitySummary {
        total: summary.total,
        pass: summary.pass,
        skipped: summary.skipped,
        unsupported: summary.unsupported,
        partial: summary.partial,
        fail: summary.fail,
    }
}

fn clone_test_counts(counts: &TestCounts) -> TestCounts {
    TestCounts {
        discovered: counts.discovered,
        ignored: counts.ignored,
        runnable: counts.runnable,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CATALOG_PATH, IGNORE_POLICY_PATH, IgnorePolicy, ScenarioCatalog, SourceRevision,
        assigned_array, build_control_plane_evidence, delimited_at, ensure_public_text,
        normalize_documentation_test_name, read_json, rust_api_endpoints, rust_struct_fields,
        typescript_api_endpoints, typescript_interface_fields, validate_catalog,
        validate_ignore_policy,
    };
    use crate::workspace_root;

    #[test]
    fn extracts_nested_and_quoted_delimiters() {
        let source = "prefix [{ path: '/api/files/{name}' }, [1]] suffix";
        let open = source.find('[').expect("fixture has an array");
        let (body, end) = delimited_at(source, open, '[', ']').expect("array must parse");
        assert_eq!(body, "{ path: '/api/files/{name}' }, [1]");
        assert_eq!(&source[end..], " suffix");
    }

    #[test]
    fn extracts_generic_rust_and_typescript_struct_fields() {
        let rust = "pub struct ApiResponse<T> {\n pub schema: String,\n pub data: Option<T>,\n}";
        let typescript =
            "export interface ApiResponse<T> {\n schema: string;\n data?: T | null;\n}";
        assert_eq!(
            rust_struct_fields(rust, "ApiResponse").expect("Rust fields"),
            ["schema", "data"]
        );
        assert_eq!(
            typescript_interface_fields(typescript, "ApiResponse").expect("TypeScript fields"),
            ["schema", "data"]
        );
    }

    #[test]
    fn endpoint_parsers_preserve_order_and_paths_with_braces() {
        let rust = r#"
pub const FILES_DELETE_PATH: &str = "/api/files/{name}";
pub const API_ENDPOINTS: &[ApiEndpoint] = &[
    ApiEndpoint::new("DELETE", FILES_DELETE_PATH, ApiResponseFormat::Json),
];
"#;
        let typescript = r"
export const API_ENDPOINTS = [
  { method: 'DELETE', path: '/api/files/{name}', response_format: 'Json' },
] as const;
";
        assert_eq!(
            rust_api_endpoints(rust).expect("Rust endpoints"),
            ["DELETE /api/files/{name} Json"]
        );
        assert_eq!(
            typescript_api_endpoints(typescript).expect("TypeScript endpoints"),
            ["DELETE /api/files/{name} Json"]
        );
        assert!(assigned_array(typescript, "export const API_ENDPOINTS").is_ok());
    }

    #[test]
    fn documentation_ids_do_not_depend_on_source_line_numbers() {
        assert_eq!(
            normalize_documentation_test_name(
                "crates/m68000/src/decoder.rs - decoder::DECODER (line 15)"
            ),
            "crates/m68000/src/decoder.rs - decoder::DECODER"
        );
        assert_eq!(
            normalize_documentation_test_name("crates/m68000/src/lib.rs - (line 49)"),
            "crates/m68000/src/lib.rs"
        );
    }

    #[test]
    fn repository_public_evidence_policies_are_valid() {
        let root = workspace_root().expect("workspace root");
        let catalog: ScenarioCatalog =
            read_json(&root.join(CATALOG_PATH)).expect("scenario catalog");
        validate_catalog(&catalog).expect("scenario catalog must be valid");
        let policy: IgnorePolicy =
            read_json(&root.join(IGNORE_POLICY_PATH)).expect("ignored-test policy");
        validate_ignore_policy(&policy).expect("ignored-test policy must be valid");
    }

    #[test]
    fn repository_control_plane_contracts_match() {
        let root = workspace_root().expect("workspace root");
        let source = SourceRevision {
            revision: "test".to_owned(),
            date_epoch: 0,
            dirty: false,
        };
        let evidence =
            build_control_plane_evidence(&root, &source).expect("control-plane evidence");
        assert!(evidence.failures.is_empty(), "{:?}", evidence.failures);
        assert_eq!(evidence.summary.structs, 25);
        assert_eq!(evidence.summary.enums, 10);
        assert_eq!(evidence.summary.endpoints, 20);
    }

    #[test]
    fn public_evidence_rejects_private_path_markers() {
        assert!(ensure_public_text("evidence/scenarios.json", "fixture").is_ok());
        assert!(ensure_public_text("/Users/example/private.rom", "fixture").is_err());
        assert!(ensure_public_text(r"C:\Users\example\private.adf", "fixture").is_err());
        assert!(ensure_public_text("/home/example/private.hdf", "fixture").is_err());
    }
}
