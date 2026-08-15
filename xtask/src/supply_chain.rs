// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use spdx::Expression;
use yaml_rust2::{Yaml, YamlLoader};

use super::{
    capture, capture_git, read_toolchain_manifest, reset_generated_directory, sha256_file,
    verify_ci_source, workspace_root,
};

const EVIDENCE_DIRECTORY: &str = "m0-009-supply-chain-evidence";
const EVIDENCE_SCHEMA: &str = "rumiga.supply-chain.evidence.v1";
const POLICY_SCHEMA: &str = "rumiga.supply-chain-policy.v1";

#[derive(Debug, Deserialize)]
struct SupplyChainPolicy {
    schema: String,
    reviewed_on: String,
    rust: RustPolicy,
    npm: NpmPolicy,
    github_actions: GitHubActionsPolicy,
}

#[derive(Debug, Deserialize)]
struct RustPolicy {
    registry_source: String,
    max_advisory_database_age_days: u32,
    allowed_licenses: Vec<String>,
    git_sources: Vec<GitSource>,
    advisory_exceptions: Vec<AdvisoryException>,
    duplicate_approvals: Vec<DuplicateApproval>,
}

#[derive(Debug, Deserialize)]
struct GitSource {
    package: String,
    repository: String,
    revision: String,
}

#[derive(Debug, Deserialize)]
struct AdvisoryException {
    id: String,
    advisory: String,
    package: String,
    version: String,
    owner: String,
    reason: String,
    compensating_control: String,
    expires_on: String,
}

#[derive(Debug, Deserialize)]
struct DuplicateApproval {
    id: String,
    owner: String,
    reason: String,
    compensating_control: String,
    expires_on: String,
    packages: Vec<DuplicatePackage>,
}

#[derive(Debug, Deserialize)]
struct DuplicatePackage {
    name: String,
    versions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct NpmPolicy {
    registry_prefix: String,
    allowed_licenses: Vec<String>,
    license_exceptions: Vec<NpmLicenseException>,
}

#[derive(Debug, Deserialize)]
struct NpmLicenseException {
    id: String,
    package: String,
    version: String,
    license: String,
    owner: String,
    reason: String,
    compensating_control: String,
    expires_on: String,
}

#[derive(Debug, Deserialize)]
struct GitHubActionsPolicy {
    allowed_repositories: Vec<String>,
    require_full_commit: bool,
    require_release_annotation: bool,
    allow_local: bool,
}

#[derive(Debug, Deserialize)]
struct DenyConfiguration {
    advisories: DenyAdvisories,
    licenses: DenyLicenses,
    sources: DenySources,
}

#[derive(Debug, Deserialize)]
struct DenyAdvisories {
    ignore: Vec<DenyAdvisoryIgnore>,
}

#[derive(Debug, Deserialize)]
struct DenyAdvisoryIgnore {
    id: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct DenyLicenses {
    allow: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DenySources {
    #[serde(rename = "allow-registry")]
    allow_registry: Vec<String>,
    #[serde(rename = "allow-git")]
    allow_git: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CargoLock {
    package: Vec<CargoLockPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoLockPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoMetadataPackage>,
    workspace_root: String,
}

#[derive(Debug, Deserialize)]
struct CargoMetadataPackage {
    name: String,
    version: String,
    source: Option<String>,
    manifest_path: String,
    publish: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct NpmLock {
    #[serde(rename = "lockfileVersion")]
    lockfile_version: u32,
    packages: BTreeMap<String, NpmLockPackage>,
}

#[derive(Debug, Default, Deserialize)]
struct NpmLockPackage {
    name: Option<String>,
    version: Option<String>,
    resolved: Option<String>,
    integrity: Option<String>,
    license: Option<String>,
    #[serde(default, rename = "inBundle")]
    in_bundle: bool,
    #[serde(default, rename = "hasInstallScript")]
    has_install_script: bool,
    #[serde(default)]
    link: bool,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
    #[serde(default, rename = "devDependencies")]
    dev_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    engines: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct NpmManifest {
    name: String,
    version: String,
    #[serde(rename = "packageManager")]
    package_manager: String,
    engines: BTreeMap<String, String>,
    dependencies: BTreeMap<String, String>,
    #[serde(rename = "devDependencies")]
    dev_dependencies: BTreeMap<String, String>,
    #[serde(rename = "allowScripts")]
    allow_scripts: BTreeMap<String, bool>,
}

#[derive(Debug, Serialize)]
struct SupplyChainEvidence {
    schema: &'static str,
    source_revision: String,
    source_date_epoch: u64,
    source_dirty: bool,
    generated_at: String,
    inputs: InputEvidence,
    tools: ToolEvidence,
    rust: RustEvidence,
    npm: NpmEvidence,
    github_actions: ActionEvidence,
    active_exception_ids: Vec<String>,
    claims: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct InputEvidence {
    policy_sha256: String,
    deny_config_sha256: String,
    cargo_lock_sha256: String,
    npm_lock_sha256: String,
    workflows: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ToolEvidence {
    cargo_deny: String,
    cargo_audit: String,
    node: String,
    npm: String,
}

#[derive(Debug, Serialize)]
struct RustEvidence {
    packages: usize,
    workspace_packages: usize,
    registry_packages: usize,
    git_packages: usize,
    duplicate_names: usize,
    advisory_database_commit: String,
    advisory_database_updated_at: String,
    advisory_database_max_age_days: u32,
    advisory_count: u64,
    vulnerabilities: u64,
    informational_warnings: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize)]
struct NpmEvidence {
    packages: usize,
    integrity_protected_packages: usize,
    bundled_packages: usize,
    license_exceptions: usize,
    vulnerabilities: BTreeMap<String, u64>,
}

#[derive(Debug, Serialize)]
struct ActionEvidence {
    workflows: usize,
    references: usize,
    repositories: Vec<String>,
}

struct StaticEvidence {
    cargo: CargoStaticEvidence,
    npm: NpmStaticEvidence,
    actions: ActionStaticEvidence,
    max_advisory_database_age_days: u32,
    active_exception_ids: Vec<String>,
    input: InputEvidence,
}

struct CargoStaticEvidence {
    packages: usize,
    workspace_packages: usize,
    registry_packages: usize,
    git_packages: usize,
    duplicate_names: usize,
}

struct NpmStaticEvidence {
    packages: usize,
    integrity_protected_packages: usize,
    bundled_packages: usize,
    license_exceptions: usize,
}

struct ActionStaticEvidence {
    workflows: usize,
    references: usize,
    repositories: Vec<String>,
}

pub fn build_evidence() -> Result<()> {
    let root = workspace_root()?;
    let toolchain = read_toolchain_manifest(&root)?;
    let source_revision = capture_git(&root, &["rev-parse", "HEAD"])?;
    let source_date_epoch = capture_git(&root, &["show", "-s", "--format=%ct", "HEAD"])?
        .parse::<u64>()
        .context("git commit timestamp must be an unsigned integer")?;
    let source_dirty = !capture_git(&root, &["status", "--porcelain"])?.is_empty();
    verify_ci_source(&source_revision, source_dirty)?;

    let evidence_root = root.join("target").join(EVIDENCE_DIRECTORY);
    reset_generated_directory(&root.join("target"), &evidence_root)?;
    let static_evidence = verify_static_policy(&root)?;
    let tools = verify_tools(&root, &toolchain)?;

    let deny_output = run_cargo_deny(&root)?;
    write_output(&evidence_root, "cargo-deny", &deny_output)?;
    ensure_command_success("cargo-deny", &deny_output)?;

    let cargo_audit_output = run_cargo_audit(&root)?;
    write_output(&evidence_root, "cargo-audit", &cargo_audit_output)?;
    ensure_command_success("cargo-audit", &cargo_audit_output)?;
    let cargo_audit = parse_json(&cargo_audit_output.stdout, "cargo-audit")?;
    let advisory =
        verify_cargo_audit(&cargo_audit, static_evidence.max_advisory_database_age_days)?;

    let npm_audit_output = run_npm_audit(&root)?;
    write_output(&evidence_root, "npm-audit", &npm_audit_output)?;
    ensure_command_success("npm audit", &npm_audit_output)?;
    let npm_audit = parse_json(&npm_audit_output.stdout, "npm audit")?;
    let vulnerabilities = verify_npm_audit(&npm_audit)?;

    let manifest = SupplyChainEvidence {
        schema: EVIDENCE_SCHEMA,
        source_revision,
        source_date_epoch,
        source_dirty,
        generated_at: Utc::now().to_rfc3339(),
        inputs: static_evidence.input,
        tools,
        rust: RustEvidence {
            packages: static_evidence.cargo.packages,
            workspace_packages: static_evidence.cargo.workspace_packages,
            registry_packages: static_evidence.cargo.registry_packages,
            git_packages: static_evidence.cargo.git_packages,
            duplicate_names: static_evidence.cargo.duplicate_names,
            advisory_database_commit: advisory.commit,
            advisory_database_updated_at: advisory.updated_at,
            advisory_database_max_age_days: static_evidence.max_advisory_database_age_days,
            advisory_count: advisory.advisory_count,
            vulnerabilities: advisory.vulnerabilities,
            informational_warnings: advisory.warnings,
        },
        npm: NpmEvidence {
            packages: static_evidence.npm.packages,
            integrity_protected_packages: static_evidence.npm.integrity_protected_packages,
            bundled_packages: static_evidence.npm.bundled_packages,
            license_exceptions: static_evidence.npm.license_exceptions,
            vulnerabilities,
        },
        github_actions: ActionEvidence {
            workflows: static_evidence.actions.workflows,
            references: static_evidence.actions.references,
            repositories: static_evidence.actions.repositories,
        },
        active_exception_ids: static_evidence.active_exception_ids,
        claims: vec![
            "locked-source-policy",
            "spdx-license-policy",
            "rustsec-advisory-policy",
            "npm-high-critical-advisory-policy",
            "immutable-action-policy",
        ],
    };
    write_evidence(&evidence_root, &manifest)?;
    println!("supply-chain evidence: {}", evidence_root.display());
    Ok(())
}

fn verify_static_policy(root: &Path) -> Result<StaticEvidence> {
    let policy: SupplyChainPolicy = read_toml(&root.join("supply-chain-policy.toml"))?;
    let deny: DenyConfiguration = read_toml(&root.join("deny.toml"))?;
    let toolchain = read_toolchain_manifest(root)?;
    let today = Utc::now().date_naive();
    let active_exception_ids = validate_policy(&policy, &deny, today)?;
    let cargo = verify_cargo_policy(root, &policy, &toolchain.host.rust)?;
    let npm = verify_npm_policy(root, &policy, &toolchain)?;
    let actions = verify_action_policy(root, &policy.github_actions)?;

    Ok(StaticEvidence {
        cargo,
        npm,
        actions,
        max_advisory_database_age_days: policy.rust.max_advisory_database_age_days,
        active_exception_ids,
        input: InputEvidence {
            policy_sha256: sha256_file(&root.join("supply-chain-policy.toml"))?,
            deny_config_sha256: sha256_file(&root.join("deny.toml"))?,
            cargo_lock_sha256: sha256_file(&root.join("Cargo.lock"))?,
            npm_lock_sha256: sha256_file(&root.join("web/package-lock.json"))?,
            workflows: workflow_hashes(root)?,
        },
    })
}

fn validate_policy(
    policy: &SupplyChainPolicy,
    deny: &DenyConfiguration,
    today: NaiveDate,
) -> Result<Vec<String>> {
    ensure!(
        policy.schema == POLICY_SCHEMA,
        "unknown supply-chain policy schema"
    );
    let reviewed_on = parse_date(&policy.reviewed_on, "policy review date")?;
    ensure!(reviewed_on <= today, "policy review date is in the future");
    ensure!(
        today.signed_duration_since(reviewed_on) <= Duration::days(365),
        "supply-chain policy review is older than 365 days"
    );
    ensure!(
        (1..=30).contains(&policy.rust.max_advisory_database_age_days),
        "Rust advisory database age must be between 1 and 30 days"
    );

    let mut ids = BTreeSet::new();
    for exception in &policy.rust.advisory_exceptions {
        validate_exception(
            &exception.id,
            &exception.owner,
            &exception.reason,
            &exception.compensating_control,
            &exception.expires_on,
            reviewed_on,
            today,
            &mut ids,
        )?;
        ensure!(
            exception.advisory.starts_with("RUSTSEC-")
                && !exception.package.is_empty()
                && !exception.version.is_empty(),
            "{} has an invalid advisory scope",
            exception.id
        );
    }
    for approval in &policy.rust.duplicate_approvals {
        validate_exception(
            &approval.id,
            &approval.owner,
            &approval.reason,
            &approval.compensating_control,
            &approval.expires_on,
            reviewed_on,
            today,
            &mut ids,
        )?;
        ensure!(
            !approval.packages.is_empty(),
            "{} has no packages",
            approval.id
        );
    }
    for exception in &policy.npm.license_exceptions {
        validate_exception(
            &exception.id,
            &exception.owner,
            &exception.reason,
            &exception.compensating_control,
            &exception.expires_on,
            reviewed_on,
            today,
            &mut ids,
        )?;
        ensure!(
            !exception.package.is_empty()
                && !exception.version.is_empty()
                && !exception.license.is_empty(),
            "{} has an invalid npm license scope",
            exception.id
        );
    }

    validate_allowlists(policy, deny)?;
    Ok(ids.into_iter().collect())
}

#[allow(clippy::too_many_arguments)]
fn validate_exception(
    id: &str,
    owner: &str,
    reason: &str,
    compensating_control: &str,
    expires_on: &str,
    reviewed_on: NaiveDate,
    today: NaiveDate,
    ids: &mut BTreeSet<String>,
) -> Result<()> {
    ensure!(
        !id.is_empty() && ids.insert(id.to_owned()),
        "duplicate exception ID {id}"
    );
    ensure!(
        !owner.trim().is_empty()
            && !reason.trim().is_empty()
            && !compensating_control.trim().is_empty(),
        "{id} has incomplete exception metadata"
    );
    let expiry = parse_date(expires_on, id)?;
    ensure!(expiry > reviewed_on, "{id} did not expire after its review");
    ensure!(expiry >= today, "{id} expired on {expiry}");
    Ok(())
}

fn validate_allowlists(policy: &SupplyChainPolicy, deny: &DenyConfiguration) -> Result<()> {
    let rust_licenses = unique_set(&policy.rust.allowed_licenses, "Rust license")?;
    let deny_licenses = unique_set(&deny.licenses.allow, "cargo-deny license")?;
    ensure!(
        rust_licenses == deny_licenses,
        "Rust license allowlists drifted"
    );
    for license in &rust_licenses {
        Expression::parse(license)
            .with_context(|| format!("invalid SPDX license in Rust allowlist: {license}"))?;
    }

    let expected_registry = policy
        .rust
        .registry_source
        .strip_prefix("registry+")
        .context("Rust registry source must start with registry+")?;
    ensure!(
        deny.sources.allow_registry == [expected_registry],
        "cargo-deny registry allowlist drifted"
    );
    let policy_git = policy
        .rust
        .git_sources
        .iter()
        .map(|source| normalize_git_url(&source.repository))
        .collect::<BTreeSet<_>>();
    let deny_git = deny
        .sources
        .allow_git
        .iter()
        .map(|source| normalize_git_url(source))
        .collect::<BTreeSet<_>>();
    ensure!(policy_git == deny_git, "cargo-deny Git allowlist drifted");

    let policy_advisories = policy
        .rust
        .advisory_exceptions
        .iter()
        .map(|exception| exception.advisory.as_str())
        .collect::<BTreeSet<_>>();
    let deny_advisories = deny
        .advisories
        .ignore
        .iter()
        .map(|exception| {
            ensure!(
                !exception.reason.trim().is_empty(),
                "cargo-deny ignore has no reason"
            );
            Ok(exception.id.as_str())
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        policy_advisories == deny_advisories,
        "cargo-deny advisory exceptions drifted"
    );

    let npm_licenses = unique_set(&policy.npm.allowed_licenses, "npm license")?;
    for license in npm_licenses {
        Expression::parse(&license)
            .with_context(|| format!("invalid SPDX license in npm allowlist: {license}"))?;
    }
    Ok(())
}

fn verify_cargo_policy(
    root: &Path,
    policy: &SupplyChainPolicy,
    rust_version: &str,
) -> Result<CargoStaticEvidence> {
    let lock: CargoLock = read_toml(&root.join("Cargo.lock"))?;
    let metadata_output = capture(
        root,
        "cargo",
        &[
            &format!("+{rust_version}"),
            "metadata",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
        ],
    )?;
    let metadata: CargoMetadata = serde_json::from_str(&metadata_output)?;
    let canonical_root = root.canonicalize()?;
    ensure!(
        Path::new(&metadata.workspace_root).canonicalize()? == canonical_root,
        "Cargo metadata resolved a different workspace root"
    );
    let workspace_packages = validate_workspace_packages(&metadata, &canonical_root)?;
    let git_sources = expected_git_sources(policy)?;
    let mut used_git_sources = BTreeSet::new();
    let mut registry_count = 0;
    let mut git_count = 0;
    let mut path_count = 0;
    let mut packages_by_name = BTreeMap::<String, Vec<&CargoLockPackage>>::new();

    for package in &lock.package {
        packages_by_name
            .entry(package.name.clone())
            .or_default()
            .push(package);
        match package.source.as_deref() {
            Some(source) if source == policy.rust.registry_source => {
                let checksum = package
                    .checksum
                    .as_deref()
                    .context("registry package has no checksum")?;
                ensure!(
                    is_sha256(checksum),
                    "{} has an invalid checksum",
                    package.name
                );
                registry_count += 1;
            }
            Some(source) if source.starts_with("git+") => {
                let expected = git_sources
                    .get(&package.name)
                    .with_context(|| format!("unapproved Git package {}", package.name))?;
                ensure!(
                    source == expected,
                    "Git source drifted for {}",
                    package.name
                );
                ensure!(
                    package.checksum.is_none(),
                    "Git package unexpectedly has a checksum"
                );
                ensure!(
                    used_git_sources.insert(package.name.clone()),
                    "duplicate Git package"
                );
                git_count += 1;
            }
            Some(source) => bail!("unapproved Cargo source for {}: {source}", package.name),
            None => {
                ensure!(
                    package.checksum.is_none(),
                    "path package unexpectedly has a checksum"
                );
                ensure!(
                    workspace_packages.contains(&(package.name.clone(), package.version.clone())),
                    "path package {} {} is not a workspace member",
                    package.name,
                    package.version
                );
                path_count += 1;
            }
        }
    }
    ensure!(
        used_git_sources == git_sources.keys().cloned().collect(),
        "approved Cargo Git source is unused"
    );
    ensure!(
        path_count == workspace_packages.len(),
        "Cargo path-package count does not match the workspace"
    );
    let duplicate_names = validate_duplicate_approvals(policy, &packages_by_name)?;
    validate_advisory_packages(policy, &lock.package)?;

    Ok(CargoStaticEvidence {
        packages: lock.package.len(),
        workspace_packages: path_count,
        registry_packages: registry_count,
        git_packages: git_count,
        duplicate_names,
    })
}

fn validate_workspace_packages(
    metadata: &CargoMetadata,
    canonical_root: &Path,
) -> Result<BTreeSet<(String, String)>> {
    let mut packages = BTreeSet::new();
    for package in &metadata.packages {
        ensure!(
            package.source.is_none(),
            "workspace metadata contains an external package"
        );
        ensure!(
            package.publish.as_ref().is_some_and(Vec::is_empty),
            "workspace package {} must set publish = false",
            package.name
        );
        let manifest = Path::new(&package.manifest_path).canonicalize()?;
        ensure!(
            manifest.starts_with(canonical_root),
            "workspace manifest escaped the repository: {}",
            manifest.display()
        );
        ensure!(
            packages.insert((package.name.clone(), package.version.clone())),
            "duplicate workspace package identity"
        );
    }
    Ok(packages)
}

fn expected_git_sources(policy: &SupplyChainPolicy) -> Result<BTreeMap<String, String>> {
    let mut sources = BTreeMap::new();
    for source in &policy.rust.git_sources {
        ensure!(
            is_git_revision(&source.revision),
            "invalid Git revision for {}",
            source.package
        );
        ensure!(
            source.repository.starts_with("https://github.com/")
                && Path::new(&source.repository)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("git")),
            "Git source must be an HTTPS GitHub repository"
        );
        let lock_source = format!(
            "git+{}?rev={}#{}",
            source.repository, source.revision, source.revision
        );
        ensure!(
            sources
                .insert(source.package.clone(), lock_source)
                .is_none(),
            "duplicate Git source approval for {}",
            source.package
        );
    }
    Ok(sources)
}

fn validate_duplicate_approvals(
    policy: &SupplyChainPolicy,
    packages_by_name: &BTreeMap<String, Vec<&CargoLockPackage>>,
) -> Result<usize> {
    let mut approved = BTreeMap::<String, BTreeSet<String>>::new();
    for approval in &policy.rust.duplicate_approvals {
        for package in &approval.packages {
            let versions = unique_set(&package.versions, "duplicate version")?;
            ensure!(
                versions.len() > 1,
                "{} is not a duplicate approval",
                package.name
            );
            ensure!(
                approved.insert(package.name.clone(), versions).is_none(),
                "duplicate approval for {}",
                package.name
            );
        }
    }

    let actual = packages_by_name
        .iter()
        .filter(|(_, packages)| packages.len() > 1)
        .map(|(name, packages)| {
            let versions = packages
                .iter()
                .map(|package| package.version.clone())
                .collect::<BTreeSet<_>>();
            ensure!(
                versions.len() == packages.len(),
                "{name} has duplicate source entries for one version"
            );
            Ok((name.clone(), versions))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    ensure!(
        actual == approved,
        "Cargo duplicate-version approvals drifted"
    );
    Ok(actual.len())
}

fn validate_advisory_packages(
    policy: &SupplyChainPolicy,
    packages: &[CargoLockPackage],
) -> Result<()> {
    for exception in &policy.rust.advisory_exceptions {
        let matches = packages
            .iter()
            .filter(|package| {
                let name_matches = package.name == exception.package;
                let version_matches = package.version == exception.version;
                name_matches && version_matches
            })
            .count();
        ensure!(
            matches == 1,
            "{} no longer matches exactly one package",
            exception.id
        );
    }
    Ok(())
}

fn verify_npm_policy(
    root: &Path,
    policy: &SupplyChainPolicy,
    toolchain: &super::ToolchainManifest,
) -> Result<NpmStaticEvidence> {
    let lock: NpmLock = read_json(&root.join("web/package-lock.json"))?;
    let manifest: NpmManifest = read_json(&root.join("web/package.json"))?;
    ensure!(
        lock.lockfile_version == 3,
        "npm lockfile must use version 3"
    );
    let lock_root = lock
        .packages
        .get("")
        .context("npm lockfile has no root package")?;
    verify_npm_root(lock_root, &manifest, toolchain, root)?;

    let allowed = unique_set(&policy.npm.allowed_licenses, "npm license")?;
    let mut exceptions = BTreeMap::new();
    for exception in &policy.npm.license_exceptions {
        let key = (
            exception.package.clone(),
            exception.version.clone(),
            exception.license.clone(),
        );
        ensure!(
            exceptions.insert(key, exception.id.clone()).is_none(),
            "duplicate npm license exception"
        );
    }
    let mut used_exceptions = BTreeSet::new();
    let mut install_script_packages = BTreeSet::new();
    let mut integrity_count = 0;
    let mut bundled_count = 0;
    for (path, package) in lock.packages.iter().filter(|(path, _)| !path.is_empty()) {
        ensure!(!package.link, "linked npm package is not allowed: {path}");
        let name = npm_package_name(path)?;
        let version = package
            .version
            .as_deref()
            .with_context(|| format!("npm package {path} has no version"))?;
        let license = package
            .license
            .as_deref()
            .with_context(|| format!("npm package {path} has no license"))?;
        let expression = Expression::parse(license)
            .with_context(|| format!("npm package {name}@{version} has invalid SPDX"))?;
        if package.has_install_script {
            install_script_packages.insert(name.to_owned());
        }
        let allowed_expression =
            expression.evaluate(|requirement| allowed.contains(&requirement.to_string()));
        if !allowed_expression {
            let key = (name.to_owned(), version.to_owned(), license.to_owned());
            let exception = exceptions.get(&key).with_context(|| {
                format!("npm package {name}@{version} has unapproved license {license}")
            })?;
            ensure!(
                used_exceptions.insert(exception.clone()),
                "npm exception matched twice"
            );
        }

        match (&package.resolved, &package.integrity) {
            (Some(resolved), Some(integrity)) => {
                ensure!(
                    resolved.starts_with(&policy.npm.registry_prefix),
                    "npm package {name}@{version} uses unapproved source {resolved}"
                );
                ensure!(
                    valid_npm_integrity(integrity),
                    "invalid npm integrity for {name}@{version}"
                );
                integrity_count += 1;
            }
            (None, None) if package.in_bundle => {
                ensure!(
                    has_integrity_protected_bundle_parent(path, &lock.packages),
                    "bundled npm package {path} has no integrity-protected parent"
                );
                bundled_count += 1;
            }
            _ => bail!("npm package {name}@{version} has incomplete source integrity"),
        }
    }
    ensure!(
        used_exceptions == exceptions.values().cloned().collect(),
        "unused npm license exception"
    );
    ensure!(
        install_script_packages == manifest.allow_scripts.keys().cloned().collect(),
        "npm install-script denylist drifted"
    );
    Ok(NpmStaticEvidence {
        packages: lock.packages.len() - 1,
        integrity_protected_packages: integrity_count,
        bundled_packages: bundled_count,
        license_exceptions: used_exceptions.len(),
    })
}

fn verify_npm_root(
    lock_root: &NpmLockPackage,
    manifest: &NpmManifest,
    toolchain: &super::ToolchainManifest,
    root: &Path,
) -> Result<()> {
    ensure!(
        lock_root.name.as_deref() == Some(&manifest.name),
        "npm root name drifted"
    );
    ensure!(
        lock_root.version.as_deref() == Some(&manifest.version),
        "npm root version drifted"
    );
    ensure!(
        lock_root.dependencies == manifest.dependencies
            && lock_root.dev_dependencies == manifest.dev_dependencies,
        "npm manifest and lockfile root dependencies drifted"
    );
    ensure!(
        manifest.package_manager == format!("npm@{}", toolchain.host.npm),
        "npm package-manager pin drifted"
    );
    ensure!(
        manifest.engines.get("node") == Some(&toolchain.host.node)
            && manifest.engines.get("npm") == Some(&toolchain.host.npm)
            && lock_root.engines == manifest.engines,
        "npm engine pins drifted"
    );
    let node_version = fs::read_to_string(root.join(".node-version"))?;
    ensure!(
        node_version.trim() == toolchain.host.node,
        "Node pin drifted"
    );
    ensure!(
        manifest.allow_scripts.values().all(|value| !value),
        "npm lifecycle scripts must be explicitly denied"
    );
    Ok(())
}

fn has_integrity_protected_bundle_parent(
    package_path: &str,
    packages: &BTreeMap<String, NpmLockPackage>,
) -> bool {
    let mut cursor = package_path;
    while let Some((parent, _)) = cursor.rsplit_once("/node_modules/") {
        if packages.get(parent).is_some_and(|package| {
            package.resolved.is_some()
                && package
                    .integrity
                    .as_deref()
                    .is_some_and(valid_npm_integrity)
        }) {
            return true;
        }
        cursor = parent;
    }
    false
}

fn npm_package_name(path: &str) -> Result<&str> {
    path.rsplit_once("node_modules/")
        .map(|(_, name)| name)
        .filter(|name| !name.is_empty() && !name.contains("/node_modules/"))
        .context("invalid npm package path")
}

fn valid_npm_integrity(value: &str) -> bool {
    value.strip_prefix("sha512-").is_some_and(|digest| {
        let bytes = digest.as_bytes();
        bytes.len() == 88
            && &bytes[86..] == b"=="
            && bytes[..86]
                .iter()
                .copied()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
    })
}

fn verify_action_policy(root: &Path, policy: &GitHubActionsPolicy) -> Result<ActionStaticEvidence> {
    let allowed = unique_set(&policy.allowed_repositories, "GitHub Actions repository")?;
    let paths = workflow_paths(root)?;
    ensure!(
        !paths.is_empty(),
        "repository has no GitHub Actions workflows"
    );
    let mut references = 0;
    let mut used_repositories = BTreeSet::new();
    for path in &paths {
        let contents = fs::read_to_string(path)?;
        let documents = YamlLoader::load_from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        ensure!(
            documents.len() == 1,
            "workflow must contain one YAML document"
        );
        let mut structured = Vec::new();
        collect_action_uses(&documents[0], &mut structured)?;
        let annotated = annotated_action_uses(&contents, policy.require_release_annotation)?;
        ensure!(
            structured == annotated,
            "workflow action annotations drifted"
        );
        for reference in structured {
            let repository = validate_action_reference(&reference, policy, &allowed)?;
            if let Some(repository) = repository {
                used_repositories.insert(repository);
            }
            references += 1;
        }
    }
    ensure!(
        used_repositories == allowed,
        "GitHub Actions repository allowlist contains an unused or missing entry"
    );
    Ok(ActionStaticEvidence {
        workflows: paths.len(),
        references,
        repositories: used_repositories.into_iter().collect(),
    })
}

fn collect_action_uses(node: &Yaml, references: &mut Vec<String>) -> Result<()> {
    match node {
        Yaml::Hash(mapping) => {
            for (key, value) in mapping {
                if key.as_str() == Some("uses") {
                    references.push(
                        value
                            .as_str()
                            .context("workflow uses value must be a string")?
                            .to_owned(),
                    );
                }
                collect_action_uses(value, references)?;
            }
        }
        Yaml::Array(values) => {
            for value in values {
                collect_action_uses(value, references)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn annotated_action_uses(contents: &str, require_annotation: bool) -> Result<Vec<String>> {
    contents
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix("uses:")
                .or_else(|| trimmed.strip_prefix("- uses:"))
        })
        .map(|value| {
            let (reference, annotation) =
                if let Some((reference, annotation)) = value.split_once('#') {
                    (reference.trim(), Some(annotation.trim()))
                } else {
                    (value.trim(), None)
                };
            let reference = reference.trim_matches(['\'', '"']);
            if reference.starts_with("./") {
                ensure!(
                    !require_annotation || annotation.is_none(),
                    "local action has a release annotation"
                );
            } else if require_annotation {
                let annotation = annotation.context("external action has no release annotation")?;
                ensure!(
                    valid_release_annotation(annotation),
                    "invalid action release annotation"
                );
            }
            Ok(reference.to_owned())
        })
        .collect()
}

fn validate_action_reference(
    reference: &str,
    policy: &GitHubActionsPolicy,
    allowed: &BTreeSet<String>,
) -> Result<Option<String>> {
    if reference.starts_with("./") {
        ensure!(policy.allow_local, "local GitHub Action is not allowed");
        return Ok(None);
    }
    let (action, revision) = reference
        .rsplit_once('@')
        .context("external action reference has no revision")?;
    ensure!(
        !policy.require_full_commit || is_git_revision(revision),
        "GitHub Action is not pinned to a full commit: {reference}"
    );
    let mut components = action.split('/');
    let owner = components.next().context("action owner is missing")?;
    let repository = components.next().context("action repository is missing")?;
    ensure!(
        !owner.is_empty() && !repository.is_empty(),
        "invalid action repository"
    );
    let repository = format!("{owner}/{repository}");
    ensure!(
        allowed.contains(&repository),
        "unapproved GitHub Action {repository}"
    );
    Ok(Some(repository))
}

fn valid_release_annotation(annotation: &str) -> bool {
    annotation.strip_prefix('v').is_some_and(|version| {
        let parts = version.split('.').collect::<Vec<_>>();
        (2..=3).contains(&parts.len())
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    })
}

fn workflow_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let directory = root.join(".github/workflows");
    let mut paths = fs::read_dir(&directory)?
        .map(|entry| entry.map(|value| value.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|path| {
        path.extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| matches!(extension, "yml" | "yaml"))
    });
    paths.sort();
    Ok(paths)
}

fn workflow_hashes(root: &Path) -> Result<BTreeMap<String, String>> {
    workflow_paths(root)?
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .and_then(OsStr::to_str)
                .context("workflow name is not UTF-8")?
                .to_owned();
            Ok((name, sha256_file(&path)?))
        })
        .collect()
}

fn verify_tools(root: &Path, toolchain: &super::ToolchainManifest) -> Result<ToolEvidence> {
    let cargo_deny = capture(root, "cargo-deny", &["--version"])?;
    ensure!(
        cargo_deny == format!("cargo-deny {}", toolchain.tools.cargo_deny),
        "cargo-deny version drifted"
    );
    let cargo_audit = capture(root, "cargo-audit", &["--version"])?;
    ensure!(
        cargo_audit == format!("cargo-audit {}", toolchain.tools.cargo_audit),
        "cargo-audit version drifted"
    );
    let node = capture(root, "node", &["--version"])?;
    ensure!(
        node == format!("v{}", toolchain.host.node),
        "Node version drifted"
    );
    let npm = capture(root, "npm", &["--version"])?;
    ensure!(npm == toolchain.host.npm, "npm version drifted");
    Ok(ToolEvidence {
        cargo_deny,
        cargo_audit,
        node,
        npm,
    })
}

fn run_cargo_deny(root: &Path) -> Result<Output> {
    run_output(
        root,
        "cargo-deny",
        &["--locked", "--format", "json", "check", "all"],
    )
}

fn run_cargo_audit(root: &Path) -> Result<Output> {
    run_output(
        root,
        "cargo-audit",
        &["audit", "--file", "Cargo.lock", "--json"],
    )
}

fn run_npm_audit(root: &Path) -> Result<Output> {
    run_output(
        &root.join("web"),
        "npm",
        &["audit", "--audit-level=high", "--json"],
    )
}

fn run_output(directory: &Path, program: &str, arguments: &[&str]) -> Result<Output> {
    Command::new(program)
        .current_dir(directory)
        .args(arguments)
        .env("CARGO_TERM_COLOR", "never")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to execute {program}"))
}

fn write_output(directory: &Path, name: &str, output: &Output) -> Result<()> {
    fs::write(
        directory.join(format!("{name}.stdout.json")),
        &output.stdout,
    )?;
    fs::write(
        directory.join(format!("{name}.stderr.jsonl")),
        &output.stderr,
    )?;
    Ok(())
}

fn ensure_command_success(description: &str, output: &Output) -> Result<()> {
    let diagnostics = scanner_diagnostic_tail(output, 6_000);
    ensure!(
        output.status.success(),
        "{description} failed with {}; full scanner logs are in target/{EVIDENCE_DIRECTORY}\n{diagnostics}",
        output.status,
    );
    Ok(())
}

fn scanner_diagnostic_tail(output: &Output, max_chars: usize) -> String {
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    diagnostic_tail(&diagnostics, max_chars)
}

fn diagnostic_tail(diagnostics: &str, max_chars: usize) -> String {
    let char_count = diagnostics.chars().count();
    if char_count <= max_chars {
        return diagnostics.to_owned();
    }
    let start = diagnostics
        .char_indices()
        .nth(char_count - max_chars)
        .map_or(0, |(index, _)| index);
    format!("[scanner output truncated]\n{}", &diagnostics[start..])
}

struct AdvisoryEvidence {
    commit: String,
    updated_at: String,
    advisory_count: u64,
    vulnerabilities: u64,
    warnings: BTreeMap<String, usize>,
}

fn verify_cargo_audit(report: &Value, max_database_age_days: u32) -> Result<AdvisoryEvidence> {
    let vulnerabilities = json_u64(report, "/vulnerabilities/count")?;
    ensure!(vulnerabilities == 0, "cargo-audit reported vulnerabilities");
    ensure!(
        report
            .pointer("/vulnerabilities/found")
            .and_then(Value::as_bool)
            == Some(false),
        "cargo-audit vulnerability result is inconsistent"
    );
    let warnings = report
        .get("warnings")
        .and_then(Value::as_object)
        .context("cargo-audit report has no warnings object")?
        .iter()
        .map(|(kind, values)| {
            let count = values
                .as_array()
                .with_context(|| format!("cargo-audit warning {kind} is not an array"))?
                .len();
            Ok((kind.clone(), count))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    ensure!(
        warnings.get("yanked").copied().unwrap_or_default() == 0,
        "cargo-audit reported a yanked package"
    );
    let updated_at = json_string(report, "/database/last-updated")?;
    let updated = DateTime::parse_from_rfc3339(updated_at)
        .context("cargo-audit database timestamp is invalid")?
        .with_timezone(&Utc);
    let database_age = Utc::now().signed_duration_since(updated);
    ensure!(
        database_age >= Duration::days(-1),
        "cargo-audit database timestamp is in the future"
    );
    ensure!(
        database_age <= Duration::days(i64::from(max_database_age_days)),
        "cargo-audit database is older than {max_database_age_days} days"
    );
    Ok(AdvisoryEvidence {
        commit: json_string(report, "/database/last-commit")?.to_owned(),
        updated_at: updated_at.to_owned(),
        advisory_count: json_u64(report, "/database/advisory-count")?,
        vulnerabilities,
        warnings,
    })
}

fn verify_npm_audit(report: &Value) -> Result<BTreeMap<String, u64>> {
    let mut vulnerabilities = BTreeMap::new();
    for severity in ["info", "low", "moderate", "high", "critical", "total"] {
        let count = json_u64(report, &format!("/metadata/vulnerabilities/{severity}"))?;
        vulnerabilities.insert(severity.to_owned(), count);
    }
    ensure!(
        vulnerabilities["high"] == 0 && vulnerabilities["critical"] == 0,
        "npm audit reported high or critical vulnerabilities"
    );
    Ok(vulnerabilities)
}

fn write_evidence(directory: &Path, evidence: &SupplyChainEvidence) -> Result<()> {
    let mut manifest = serde_json::to_vec_pretty(evidence)?;
    manifest.push(b'\n');
    fs::write(directory.join("manifest.json"), manifest)?;
    let mut files = fs::read_dir(directory)?
        .map(|entry| entry.map(|value| value.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    files.retain(|path| path.is_file() && path.file_name() != Some(OsStr::new("SHA256SUMS")));
    files.sort();
    let mut checksums = File::create(directory.join("SHA256SUMS"))?;
    for path in files {
        writeln!(
            checksums,
            "{}  {}",
            sha256_file(&path)?,
            path.file_name()
                .and_then(OsStr::to_str)
                .context("evidence filename is not UTF-8")?
        )?;
    }
    Ok(())
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let contents = fs::read_to_string(path)?;
    toml::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let contents = fs::read_to_string(path)?;
    serde_json::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
}

fn parse_json(bytes: &[u8], description: &str) -> Result<Value> {
    serde_json::from_slice(bytes).with_context(|| format!("{description} did not emit valid JSON"))
}

fn parse_date(value: &str, description: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("{description} has an invalid date"))
}

fn unique_set(values: &[String], description: &str) -> Result<BTreeSet<String>> {
    let set = values.iter().cloned().collect::<BTreeSet<_>>();
    ensure!(set.len() == values.len(), "duplicate {description} entry");
    ensure!(!set.is_empty(), "empty {description} allowlist");
    Ok(set)
}

fn normalize_git_url(value: &str) -> &str {
    value.strip_suffix(".git").unwrap_or(value)
}

fn is_git_revision(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn json_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("JSON report has no string at {pointer}"))
}

fn json_u64(value: &Value, pointer: &str) -> Result<u64> {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .with_context(|| format!("JSON report has no unsigned integer at {pointer}"))
}

#[cfg(test)]
mod tests {
    use super::{
        annotated_action_uses, diagnostic_tail, has_integrity_protected_bundle_parent,
        valid_npm_integrity, verify_static_policy,
    };
    use crate::workspace_root;
    use std::collections::BTreeMap;

    #[test]
    fn repository_static_supply_chain_policy_is_valid() {
        verify_static_policy(&workspace_root().expect("workspace root"))
            .expect("repository policy must pass");
    }

    #[test]
    fn action_annotation_requires_an_immutable_review_label() {
        let action = "owner/repo@0123456789012345678901234567890123456789";
        let workflow = format!("steps:\n  - uses: {action} # v1.2.3\n");
        assert_eq!(
            annotated_action_uses(&workflow, true).expect("valid action"),
            [action]
        );
        assert!(annotated_action_uses(&format!("uses: {action}\n"), true).is_err());
    }

    #[test]
    fn bundled_package_requires_an_integrity_protected_parent() {
        let parent = super::NpmLockPackage {
            resolved: Some("https://registry.npmjs.org/example/-/example-1.0.0.tgz".to_owned()),
            integrity: Some(format!("sha512-{}==", "A".repeat(86))),
            ..super::NpmLockPackage::default()
        };
        let mut packages = BTreeMap::new();
        packages.insert("node_modules/example".to_owned(), parent);
        assert!(has_integrity_protected_bundle_parent(
            "node_modules/example/node_modules/child",
            &packages
        ));
        assert!(valid_npm_integrity(&format!("sha512-{}==", "A".repeat(86))));
    }

    #[test]
    fn scanner_diagnostics_keep_a_bounded_unicode_safe_tail() {
        assert_eq!(
            diagnostic_tail("prefix-\u{00e4}bcd", 4),
            "[scanner output truncated]\n\u{00e4}bcd"
        );
    }
}
