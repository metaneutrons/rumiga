// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use yaml_rust2::{Yaml, YamlLoader};

use super::{
    capture_git, reset_generated_directory, sha256_file, verify_ci_source, workspace_root,
    write_manifest_and_checksums,
};

pub const EVIDENCE_DIRECTORY: &str = "target/m0-012-governance-evidence";

const CHANGE_RECORD_SCHEMA: &str = "rumiga.change-record.v1";
const CHANGE_RECORD_SCHEMA_PATH: &str = "governance/change-record.schema.json";
const CHANGE_RECORD_DIRECTORY: &str = "governance/changes";
const ADR_DIRECTORY: &str = "docs/adr";
const RELEASE_NOTE_DIRECTORY: &str = "docs/release-notes/unreleased";
const MAX_CONTRACT_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
struct SchemaVersion {
    id: String,
    version: u16,
}

#[derive(Clone, Debug, Serialize)]
struct SourceRevision {
    revision: String,
    date_epoch: u64,
    dirty: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChangeRecord {
    schema: String,
    task_id: String,
    title: String,
    status: String,
    summary: String,
    scope: Vec<String>,
    risk: RiskRecord,
    compatibility: ImpactRecord,
    security: ImpactRecord,
    tests: Vec<TestReference>,
    evidence: Vec<EvidenceReference>,
    documentation: Vec<String>,
    release_note: String,
    architecture_decisions: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RiskRecord {
    level: String,
    failure_mode: String,
    rollback: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ImpactRecord {
    status: String,
    detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TestReference {
    id: String,
    command: String,
    proves: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EvidenceReference {
    id: String,
    kind: String,
    location: String,
    proves: String,
}

#[derive(Debug, Serialize)]
struct ContractEvidence {
    path: String,
    kind: String,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct AdrEvidence {
    path: String,
    number: u16,
    status: String,
    task_id: String,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct ReleaseNoteEvidence {
    path: String,
    task_id: String,
    change_type: String,
    sha256: String,
}

#[derive(Clone, Debug, Serialize)]
struct TraceabilityEntry {
    path: String,
    sha256: String,
    record: ChangeRecord,
}

#[derive(Debug, Serialize)]
struct GovernanceSummary {
    contracts: usize,
    architecture_decisions: usize,
    release_notes: usize,
    change_records: usize,
    test_references: usize,
    evidence_references: usize,
}

#[derive(Debug, Serialize)]
struct GovernanceReport {
    schema: SchemaVersion,
    source: SourceRevision,
    result: String,
    summary: GovernanceSummary,
    contracts: Vec<ContractEvidence>,
    architecture_decisions: Vec<AdrEvidence>,
    release_notes: Vec<ReleaseNoteEvidence>,
    change_records: Vec<ChangeRecordSummary>,
}

#[derive(Debug, Serialize)]
struct ChangeRecordSummary {
    path: String,
    task_id: String,
    status: String,
    tests: usize,
    evidence: usize,
    documentation: usize,
    architecture_decisions: usize,
    release_note: String,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct TraceabilityReport {
    schema: SchemaVersion,
    source: SourceRevision,
    records: Vec<TraceabilityEntry>,
}

#[derive(Debug, Serialize)]
struct PublicScope {
    kind: String,
    private_content_included: bool,
    external_project_system_required: bool,
}

#[derive(Debug, Serialize)]
struct Commands {
    generate: String,
    quality_gate: String,
}

#[derive(Debug, Serialize)]
struct ArtifactEvidence {
    name: String,
    role: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct GovernanceManifest {
    schema: SchemaVersion,
    source: SourceRevision,
    scope: PublicScope,
    commands: Commands,
    summary: GovernanceSummary,
    inputs: BTreeMap<String, String>,
    artifacts: Vec<ArtifactEvidence>,
    claims: Vec<String>,
    exclusions: Vec<String>,
}

struct ValidationOutput {
    contracts: Vec<ContractEvidence>,
    architecture_decisions: Vec<AdrEvidence>,
    release_notes: Vec<ReleaseNoteEvidence>,
    traceability: Vec<TraceabilityEntry>,
    inputs: BTreeMap<String, String>,
}

pub fn build_evidence() -> Result<()> {
    let root = workspace_root()?;
    let target_root = root.join("target");
    let evidence_root = root.join(EVIDENCE_DIRECTORY);
    reset_generated_directory(&target_root, &evidence_root)?;

    let source = source_revision(&root)?;
    verify_ci_source(&source.revision, source.dirty)?;
    let validation = validate_repository(&root)?;
    let summary = summary(&validation);

    let change_records = validation
        .traceability
        .iter()
        .map(|entry| ChangeRecordSummary {
            path: entry.path.clone(),
            task_id: entry.record.task_id.clone(),
            status: entry.record.status.clone(),
            tests: entry.record.tests.len(),
            evidence: entry.record.evidence.len(),
            documentation: entry.record.documentation.len(),
            architecture_decisions: entry.record.architecture_decisions.len(),
            release_note: entry.record.release_note.clone(),
            sha256: entry.sha256.clone(),
        })
        .collect();
    let report = GovernanceReport {
        schema: schema("rumiga.governance.report.v1"),
        source: source.clone(),
        result: "pass".to_owned(),
        summary: clone_summary(&summary),
        contracts: validation.contracts,
        architecture_decisions: validation.architecture_decisions,
        release_notes: validation.release_notes,
        change_records,
    };
    write_json(&evidence_root.join("governance.json"), &report)?;

    let traceability = TraceabilityReport {
        schema: schema("rumiga.traceability.report.v1"),
        source: source.clone(),
        records: validation.traceability,
    };
    write_json(&evidence_root.join("traceability.json"), &traceability)?;

    let artifacts = vec![
        artifact(
            &evidence_root.join("governance.json"),
            "validated governance contract report",
        )?,
        artifact(
            &evidence_root.join("traceability.json"),
            "normalized task-to-evidence traceability records",
        )?,
    ];
    let manifest = GovernanceManifest {
        schema: schema("rumiga.governance.bundle.v1"),
        source,
        scope: PublicScope {
            kind: "public-ci".to_owned(),
            private_content_included: false,
            external_project_system_required: false,
        },
        commands: Commands {
            generate: "cargo +1.97.1 xtask governance-evidence".to_owned(),
            quality_gate: "cargo +1.97.1 xtask ci --gate governance".to_owned(),
        },
        summary,
        inputs: validation.inputs,
        artifacts,
        claims: vec![
            "required-governance-contracts-validated".to_owned(),
            "task-test-evidence-traceability-validated".to_owned(),
            "adr-and-release-note-links-validated".to_owned(),
            "public-checksummed-evidence".to_owned(),
        ],
        exclusions: vec![
            "no-human-review-approval-claim".to_owned(),
            "no-branch-protection-configuration-claim".to_owned(),
            "no-release-versioning-claim".to_owned(),
        ],
    };
    write_manifest_and_checksums(&evidence_root, &manifest)?;
    verify_public_output(&evidence_root, &root)?;

    println!("governance evidence: {}", evidence_root.display());
    Ok(())
}

fn schema(id: &str) -> SchemaVersion {
    SchemaVersion {
        id: id.to_owned(),
        version: 1,
    }
}

fn clone_summary(summary: &GovernanceSummary) -> GovernanceSummary {
    GovernanceSummary {
        contracts: summary.contracts,
        architecture_decisions: summary.architecture_decisions,
        release_notes: summary.release_notes,
        change_records: summary.change_records,
        test_references: summary.test_references,
        evidence_references: summary.evidence_references,
    }
}

fn source_revision(root: &Path) -> Result<SourceRevision> {
    let revision = capture_git(root, &["rev-parse", "HEAD"])?;
    let date_epoch = capture_git(root, &["show", "-s", "--format=%ct", "HEAD"])?
        .parse::<u64>()
        .context("git commit timestamp must be an unsigned integer")?;
    let dirty = !capture_git(root, &["status", "--porcelain"])?.is_empty();
    Ok(SourceRevision {
        revision,
        date_epoch,
        dirty,
    })
}

fn validate_repository(root: &Path) -> Result<ValidationOutput> {
    let mut input_paths = BTreeSet::new();
    let mut contracts = Vec::new();

    validate_contributor_contracts(root, &mut contracts, &mut input_paths)?;
    validate_github_contracts(root, &mut contracts, &mut input_paths)?;
    validate_document_contracts(root, &mut contracts, &mut input_paths)?;

    let architecture_decisions = validate_adrs(root, &mut input_paths)?;
    let release_notes = validate_release_notes(root, &mut input_paths)?;
    let traceability = validate_change_records(
        root,
        &architecture_decisions,
        &release_notes,
        &mut input_paths,
    )?;

    let inputs = input_paths
        .into_iter()
        .map(|relative| {
            let checksum = sha256_file(&checked_file(root, &relative)?)?;
            Ok((relative, checksum))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;

    Ok(ValidationOutput {
        contracts,
        architecture_decisions,
        release_notes,
        traceability,
        inputs,
    })
}

fn validate_contributor_contracts(
    root: &Path,
    contracts: &mut Vec<ContractEvidence>,
    input_paths: &mut BTreeSet<String>,
) -> Result<()> {
    validate_markdown_contract(
        root,
        "CONTRIBUTING.md",
        &[
            "# Contributing to Rumiga",
            "## Scope And Task",
            "## Development Setup",
            "## Change Workflow",
            "## Tests And Evidence",
            "## Architecture Decisions",
            "## Release Notes",
            "## Commit And Pull Request",
            "## Review And Merge",
            "## Security And Private Media",
        ],
        contracts,
        input_paths,
        "contribution-policy",
    )?;
    validate_markdown_contract(
        root,
        "REVIEWING.md",
        &[
            "# Reviewing Rumiga Changes",
            "## Review Order",
            "## Correctness",
            "## Compatibility And Evidence",
            "## Embedded Constraints",
            "## Security And Supply Chain",
            "## Documentation And Traceability",
            "## Decision",
        ],
        contracts,
        input_paths,
        "review-policy",
    )
}

fn validate_github_contracts(
    root: &Path,
    contracts: &mut Vec<ContractEvidence>,
    input_paths: &mut BTreeSet<String>,
) -> Result<()> {
    validate_pull_request_template(root, contracts, input_paths)?;
    validate_codeowners(root, contracts, input_paths)?;
    validate_issue_form(
        root,
        ".github/ISSUE_TEMPLATE/bug_report.yml",
        &[
            "summary",
            "revision",
            "environment",
            "reproduce",
            "expected",
            "actual",
        ],
        contracts,
        input_paths,
    )?;
    validate_issue_form(
        root,
        ".github/ISSUE_TEMPLATE/feature_request.yml",
        &["problem", "roadmap", "outcome", "evidence"],
        contracts,
        input_paths,
    )?;
    validate_issue_config(root, contracts, input_paths)
}

fn validate_document_contracts(
    root: &Path,
    contracts: &mut Vec<ContractEvidence>,
    input_paths: &mut BTreeSet<String>,
) -> Result<()> {
    validate_markdown_contract(
        root,
        "docs/adr/README.md",
        &[
            "# Architecture Decision Records",
            "## When An ADR Is Required",
            "## Lifecycle",
            "## Index",
        ],
        contracts,
        input_paths,
        "adr-policy",
    )?;
    validate_markdown_contract(
        root,
        "docs/adr/0000-template.md",
        &[
            "# ADR-NNNN: Decision Title",
            "## Context",
            "## Decision",
            "## Consequences",
            "## Alternatives",
            "## Evidence",
            "## Supersession",
        ],
        contracts,
        input_paths,
        "adr-template",
    )?;
    validate_markdown_contract(
        root,
        "docs/release-notes/README.md",
        &["# Release Notes"],
        contracts,
        input_paths,
        "release-note-policy",
    )?;
    validate_markdown_contract(
        root,
        "docs/release-notes/TEMPLATE.md",
        &[
            "# TASK-ID: Concise Change Title",
            "## Summary",
            "## User And Operator Impact",
            "## Compatibility And Migration",
            "## Verification",
            "## Known Limitations",
        ],
        contracts,
        input_paths,
        "release-note-template",
    )?;
    validate_markdown_contract(
        root,
        "governance/README.md",
        &["# Governance Records"],
        contracts,
        input_paths,
        "change-record-policy",
    )?;
    validate_change_record_schema(root, contracts, input_paths)
}

fn summary(validation: &ValidationOutput) -> GovernanceSummary {
    GovernanceSummary {
        contracts: validation.contracts.len(),
        architecture_decisions: validation.architecture_decisions.len(),
        release_notes: validation.release_notes.len(),
        change_records: validation.traceability.len(),
        test_references: validation
            .traceability
            .iter()
            .map(|entry| entry.record.tests.len())
            .sum(),
        evidence_references: validation
            .traceability
            .iter()
            .map(|entry| entry.record.evidence.len())
            .sum(),
    }
}

fn validate_markdown_contract(
    root: &Path,
    relative: &str,
    headings: &[&str],
    contracts: &mut Vec<ContractEvidence>,
    input_paths: &mut BTreeSet<String>,
    kind: &str,
) -> Result<()> {
    let contents = read_regular_utf8(root, relative)?;
    require_unique_lines(&contents, headings, relative)?;
    add_contract(root, relative, kind, contracts, input_paths)
}

fn validate_pull_request_template(
    root: &Path,
    contracts: &mut Vec<ContractEvidence>,
    input_paths: &mut BTreeSet<String>,
) -> Result<()> {
    let relative = ".github/pull_request_template.md";
    let contents = read_regular_utf8(root, relative)?;
    require_unique_lines(
        &contents,
        &[
            "## Task And Scope",
            "## Behavior",
            "## Risk And Rollback",
            "## Verification",
            "## Evidence",
            "## Architecture",
            "## Release Note",
            "## Reviewer Checklist",
        ],
        relative,
    )?;
    for marker in [
        "Task ID:",
        "Change record:",
        "Risk:",
        "Rollback:",
        "Evidence:",
        "ADR:",
        "Release note:",
    ] {
        ensure!(
            contents.contains(marker),
            "{relative} is missing required marker {marker:?}"
        );
    }
    ensure!(
        contents.matches("- [ ]").count() >= 10,
        "{relative} must retain the review assertions"
    );
    add_contract(
        root,
        relative,
        "pull-request-template",
        contracts,
        input_paths,
    )
}

fn validate_codeowners(
    root: &Path,
    contracts: &mut Vec<ContractEvidence>,
    input_paths: &mut BTreeSet<String>,
) -> Result<()> {
    let relative = ".github/CODEOWNERS";
    let contents = read_regular_utf8(root, relative)?;
    for line in [
        "* @metaneutrons",
        "/.github/ @metaneutrons",
        "/governance/ @metaneutrons",
        "/docs/adr/ @metaneutrons",
        "/docs/release-notes/ @metaneutrons",
    ] {
        ensure!(
            contents.lines().any(|candidate| candidate == line),
            "{relative} is missing ownership rule {line:?}"
        );
    }
    add_contract(root, relative, "code-ownership", contracts, input_paths)
}

fn validate_issue_form(
    root: &Path,
    relative: &str,
    required_ids: &[&str],
    contracts: &mut Vec<ContractEvidence>,
    input_paths: &mut BTreeSet<String>,
) -> Result<()> {
    let contents = read_regular_utf8(root, relative)?;
    let documents = YamlLoader::load_from_str(&contents)
        .with_context(|| format!("failed to parse {relative}"))?;
    ensure!(
        documents.len() == 1,
        "{relative} must have one YAML document"
    );
    let document = &documents[0];
    for key in ["name", "description", "title"] {
        ensure!(
            yaml_get(document, key).and_then(Yaml::as_str).is_some(),
            "{relative} must define string {key}"
        );
    }
    ensure!(
        yaml_get(document, "labels")
            .and_then(Yaml::as_vec)
            .is_some_and(|labels| !labels.is_empty()),
        "{relative} must define at least one label"
    );
    let body = yaml_get(document, "body")
        .and_then(Yaml::as_vec)
        .with_context(|| format!("{relative} must define a body array"))?;
    let mut observed = BTreeMap::new();
    for item in body {
        let Some(id) = yaml_get(item, "id").and_then(Yaml::as_str) else {
            continue;
        };
        ensure!(
            observed.insert(id.to_owned(), item).is_none(),
            "{relative} contains duplicate body id {id:?}"
        );
    }
    for id in required_ids {
        let item = observed
            .get(*id)
            .with_context(|| format!("{relative} is missing required body id {id:?}"))?;
        ensure!(
            yaml_get(item, "validations")
                .and_then(|node| yaml_get(node, "required"))
                .and_then(Yaml::as_bool)
                == Some(true),
            "{relative} body id {id:?} must be required"
        );
    }
    add_contract(root, relative, "issue-form", contracts, input_paths)
}

fn validate_issue_config(
    root: &Path,
    contracts: &mut Vec<ContractEvidence>,
    input_paths: &mut BTreeSet<String>,
) -> Result<()> {
    let relative = ".github/ISSUE_TEMPLATE/config.yml";
    let contents = read_regular_utf8(root, relative)?;
    let documents = YamlLoader::load_from_str(&contents)
        .with_context(|| format!("failed to parse {relative}"))?;
    ensure!(
        documents.len() == 1,
        "{relative} must have one YAML document"
    );
    ensure!(
        yaml_get(&documents[0], "blank_issues_enabled").and_then(Yaml::as_bool) == Some(false),
        "{relative} must disable blank issues"
    );
    ensure!(
        yaml_get(&documents[0], "contact_links")
            .and_then(Yaml::as_vec)
            .is_some(),
        "{relative} must define contact_links"
    );
    add_contract(root, relative, "issue-config", contracts, input_paths)
}

fn validate_change_record_schema(
    root: &Path,
    contracts: &mut Vec<ContractEvidence>,
    input_paths: &mut BTreeSet<String>,
) -> Result<()> {
    let contents = read_regular_utf8(root, CHANGE_RECORD_SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse {CHANGE_RECORD_SCHEMA_PATH}"))?;
    ensure!(
        schema.pointer("/$schema").and_then(Value::as_str)
            == Some("https://json-schema.org/draft/2020-12/schema"),
        "change-record schema must use JSON Schema draft 2020-12"
    );
    ensure!(
        schema.pointer("/$id").and_then(Value::as_str)
            == Some("https://github.com/metaneutrons/rumiga/schemas/change-record-v1.json"),
        "change-record schema ID drifted"
    );
    ensure!(
        schema
            .pointer("/additionalProperties")
            .and_then(Value::as_bool)
            == Some(false),
        "change-record schema must reject unknown fields"
    );
    add_contract(
        root,
        CHANGE_RECORD_SCHEMA_PATH,
        "change-record-schema",
        contracts,
        input_paths,
    )
}

fn validate_adrs(root: &Path, input_paths: &mut BTreeSet<String>) -> Result<Vec<AdrEvidence>> {
    let files = directory_files(root, ADR_DIRECTORY, "md")?;
    let mut decisions = Vec::new();
    for relative in files {
        if relative.ends_with("/README.md") || relative.ends_with("/0000-template.md") {
            continue;
        }
        let name = Path::new(&relative)
            .file_name()
            .and_then(|value| value.to_str())
            .context("ADR filename is not UTF-8")?;
        let (number_text, slug) = name
            .split_once('-')
            .with_context(|| format!("ADR filename lacks numeric prefix: {name}"))?;
        ensure!(
            number_text.len() == 4
                && number_text.bytes().all(|byte| byte.is_ascii_digit())
                && Path::new(slug)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
                && slug.len() > 3,
            "invalid ADR filename {name:?}"
        );
        let number = number_text.parse::<u16>()?;
        ensure!(number > 0, "ADR 0000 is reserved for the template");
        let contents = read_regular_utf8(root, &relative)?;
        ensure!(
            contents
                .lines()
                .next()
                .is_some_and(|line| line.starts_with(&format!("# ADR-{number_text}: "))),
            "{relative} title does not match its ADR number"
        );
        require_unique_lines(
            &contents,
            &[
                "## Context",
                "## Decision",
                "## Consequences",
                "## Alternatives",
                "## Evidence",
                "## Supersession",
            ],
            &relative,
        )?;
        let status = metadata_value(&contents, "- Status: ", &relative)?;
        ensure!(
            ["Proposed", "Accepted", "Rejected", "Superseded"].contains(&status.as_str()),
            "{relative} has unsupported ADR status {status:?}"
        );
        let date = metadata_value(&contents, "- Date: ", &relative)?;
        ensure!(valid_date(&date), "{relative} has invalid date {date:?}");
        let owners = metadata_value(&contents, "- Owners: ", &relative)?;
        ensure!(owners.starts_with('@'), "{relative} must name an owner");
        let task_id = metadata_value(&contents, "- Task: ", &relative)?;
        ensure!(valid_task_id(&task_id), "{relative} has invalid task ID");
        input_paths.insert(relative.clone());
        decisions.push(AdrEvidence {
            path: relative.clone(),
            number,
            status,
            task_id,
            sha256: sha256_file(&checked_file(root, &relative)?)?,
        });
    }
    decisions.sort_by_key(|decision| decision.number);
    for (index, decision) in decisions.iter().enumerate() {
        ensure!(
            usize::from(decision.number) == index + 1,
            "ADR numbering must be contiguous from 0001"
        );
    }
    ensure!(
        !decisions.is_empty(),
        "at least one architecture decision is required"
    );
    Ok(decisions)
}

fn validate_release_notes(
    root: &Path,
    input_paths: &mut BTreeSet<String>,
) -> Result<Vec<ReleaseNoteEvidence>> {
    let files = directory_files(root, RELEASE_NOTE_DIRECTORY, "md")?;
    let mut notes = Vec::new();
    for relative in files {
        let task_id = Path::new(&relative)
            .file_stem()
            .and_then(|value| value.to_str())
            .context("release-note filename is not UTF-8")?
            .to_owned();
        ensure!(
            valid_task_id(&task_id),
            "invalid release-note task ID {task_id:?}"
        );
        let contents = read_regular_utf8(root, &relative)?;
        ensure!(
            contents
                .lines()
                .next()
                .is_some_and(|line| line.starts_with(&format!("# {task_id}: "))),
            "{relative} title does not match its task ID"
        );
        require_unique_lines(
            &contents,
            &[
                "## Summary",
                "## User And Operator Impact",
                "## Compatibility And Migration",
                "## Verification",
                "## Known Limitations",
            ],
            &relative,
        )?;
        ensure!(
            metadata_value(&contents, "- Status: ", &relative)? == "Unreleased",
            "{relative} must remain Unreleased in this directory"
        );
        let change_type = metadata_value(&contents, "- Change type: ", &relative)?;
        ensure!(
            [
                "Added",
                "Changed",
                "Fixed",
                "Security",
                "Deprecated",
                "Removed"
            ]
            .contains(&change_type.as_str()),
            "{relative} has unsupported change type {change_type:?}"
        );
        ensure!(
            metadata_value(&contents, "- Task: ", &relative)? == task_id,
            "{relative} metadata task does not match its filename"
        );
        let audience = metadata_value(&contents, "- Audience: ", &relative)?;
        ensure!(!audience.trim().is_empty(), "{relative} audience is empty");
        input_paths.insert(relative.clone());
        notes.push(ReleaseNoteEvidence {
            path: relative.clone(),
            task_id,
            change_type,
            sha256: sha256_file(&checked_file(root, &relative)?)?,
        });
    }
    ensure!(
        !notes.is_empty(),
        "at least one unreleased note is required"
    );
    Ok(notes)
}

fn validate_change_records(
    root: &Path,
    adrs: &[AdrEvidence],
    release_notes: &[ReleaseNoteEvidence],
    input_paths: &mut BTreeSet<String>,
) -> Result<Vec<TraceabilityEntry>> {
    let plan = read_regular_utf8(root, "IMPLEMENTATION_PLAN.md")?;
    input_paths.insert("IMPLEMENTATION_PLAN.md".to_owned());
    let adr_tasks = adrs
        .iter()
        .map(|adr| (adr.path.as_str(), adr.task_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let note_tasks = release_notes
        .iter()
        .map(|note| (note.path.as_str(), note.task_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let files = directory_files(root, CHANGE_RECORD_DIRECTORY, "json")?;
    let mut records = Vec::new();
    let mut tasks = BTreeSet::new();
    for relative in files {
        let record: ChangeRecord = read_json(root, &relative)?;
        ensure!(
            record.schema == CHANGE_RECORD_SCHEMA,
            "{relative} schema drifted"
        );
        ensure!(
            valid_task_id(&record.task_id),
            "{relative} has invalid task ID"
        );
        ensure!(
            Path::new(&relative)
                .file_stem()
                .and_then(|value| value.to_str())
                == Some(record.task_id.as_str()),
            "{relative} filename must match task ID {}",
            record.task_id
        );
        ensure!(
            tasks.insert(record.task_id.clone()),
            "duplicate change record for {}",
            record.task_id
        );
        ensure!(
            plan.lines()
                .any(|line| line.starts_with(&format!("| {} |", record.task_id))),
            "{} does not exist in IMPLEMENTATION_PLAN.md",
            record.task_id
        );
        validate_change_record(&relative, &record)?;
        for document in &record.documentation {
            read_regular_utf8(root, document)
                .with_context(|| format!("{} links invalid documentation", record.task_id))?;
            input_paths.insert(document.clone());
        }
        ensure!(
            record
                .documentation
                .iter()
                .any(|path| path == "IMPLEMENTATION_PLAN.md")
                && record
                    .documentation
                    .iter()
                    .any(|path| path == "PROJECT_STATUS.md"),
            "{} must link plan and project status",
            record.task_id
        );
        if let Some(justification) = record.release_note.strip_prefix("N/A: ") {
            ensure!(
                !justification.trim().is_empty(),
                "{} release-note N/A needs a justification",
                record.task_id
            );
        } else {
            ensure!(
                note_tasks.get(record.release_note.as_str()).copied()
                    == Some(record.task_id.as_str()),
                "{} release-note link is missing or belongs to another task",
                record.task_id
            );
        }
        for adr in &record.architecture_decisions {
            ensure!(
                adr_tasks.get(adr.as_str()).copied() == Some(record.task_id.as_str()),
                "{} ADR link is missing or belongs to another task",
                record.task_id
            );
        }
        input_paths.insert(relative.clone());
        records.push(TraceabilityEntry {
            path: relative.clone(),
            sha256: sha256_file(&checked_file(root, &relative)?)?,
            record,
        });
    }
    ensure!(
        !records.is_empty(),
        "at least one change record is required"
    );
    records.sort_by(|left, right| left.record.task_id.cmp(&right.record.task_id));
    Ok(records)
}

fn validate_change_record(relative: &str, record: &ChangeRecord) -> Result<()> {
    for (field, value) in [
        ("title", record.title.as_str()),
        ("summary", record.summary.as_str()),
        ("risk.failure_mode", record.risk.failure_mode.as_str()),
        ("risk.rollback", record.risk.rollback.as_str()),
        ("compatibility.detail", record.compatibility.detail.as_str()),
        ("security.detail", record.security.detail.as_str()),
    ] {
        ensure!(
            !value.trim().is_empty(),
            "{relative} field {field} is empty"
        );
    }
    ensure!(
        ["planned", "implemented", "verified", "released"].contains(&record.status.as_str()),
        "{relative} has unsupported status"
    );
    ensure!(
        ["low", "medium", "high", "critical"].contains(&record.risk.level.as_str()),
        "{relative} has unsupported risk level"
    );
    validate_impact(relative, "compatibility", &record.compatibility)?;
    validate_impact(relative, "security", &record.security)?;
    ensure!(!record.scope.is_empty(), "{relative} scope is empty");
    unique_nonempty(&record.scope, relative, "scope")?;
    unique_nonempty(&record.documentation, relative, "documentation")?;
    unique_nonempty(
        &record.architecture_decisions,
        relative,
        "architecture_decisions",
    )?;
    ensure!(!record.tests.is_empty(), "{relative} tests are empty");
    ensure!(!record.evidence.is_empty(), "{relative} evidence is empty");

    let mut test_ids = BTreeSet::new();
    for test in &record.tests {
        ensure!(
            test_ids.insert(&test.id),
            "{relative} has duplicate test ID"
        );
        for (field, value) in [
            ("id", &test.id),
            ("command", &test.command),
            ("proves", &test.proves),
        ] {
            ensure!(!value.trim().is_empty(), "{relative} test {field} is empty");
        }
    }
    let mut evidence_ids = BTreeSet::new();
    for evidence in &record.evidence {
        ensure!(
            evidence_ids.insert(&evidence.id),
            "{relative} has duplicate evidence ID"
        );
        ensure!(
            ["command", "ci-artifact", "host-scenario", "hil"].contains(&evidence.kind.as_str()),
            "{relative} has unsupported evidence kind"
        );
        for (field, value) in [
            ("id", &evidence.id),
            ("location", &evidence.location),
            ("proves", &evidence.proves),
        ] {
            ensure!(
                !value.trim().is_empty(),
                "{relative} evidence {field} is empty"
            );
        }
        if matches!(record.status.as_str(), "verified" | "released") {
            ensure!(
                !evidence.location.to_ascii_lowercase().contains("pending"),
                "{relative} verified evidence cannot remain pending"
            );
        }
    }
    Ok(())
}

fn validate_impact(relative: &str, field: &str, impact: &ImpactRecord) -> Result<()> {
    ensure!(
        ["none", "affected"].contains(&impact.status.as_str()),
        "{relative} has unsupported {field} impact"
    );
    Ok(())
}

fn unique_nonempty(values: &[String], relative: &str, field: &str) -> Result<()> {
    let mut unique = BTreeSet::new();
    for value in values {
        ensure!(
            !value.trim().is_empty(),
            "{relative} {field} contains an empty value"
        );
        ensure!(
            unique.insert(value),
            "{relative} {field} contains duplicate {value:?}"
        );
    }
    Ok(())
}

fn add_contract(
    root: &Path,
    relative: &str,
    kind: &str,
    contracts: &mut Vec<ContractEvidence>,
    input_paths: &mut BTreeSet<String>,
) -> Result<()> {
    input_paths.insert(relative.to_owned());
    contracts.push(ContractEvidence {
        path: relative.to_owned(),
        kind: kind.to_owned(),
        sha256: sha256_file(&checked_file(root, relative)?)?,
    });
    Ok(())
}

fn read_json<T: DeserializeOwned>(root: &Path, relative: &str) -> Result<T> {
    let contents = read_regular_utf8(root, relative)?;
    serde_json::from_str(&contents).with_context(|| format!("failed to parse {relative}"))
}

fn read_regular_utf8(root: &Path, relative: &str) -> Result<String> {
    let path = checked_file(root, relative)?;
    let bytes = fs::read(&path).with_context(|| format!("failed to read {relative}"))?;
    ensure!(
        bytes.len() as u64 <= MAX_CONTRACT_BYTES,
        "governance input {relative} exceeds {MAX_CONTRACT_BYTES} bytes"
    );
    let contents = String::from_utf8(bytes).with_context(|| format!("{relative} is not UTF-8"))?;
    reject_private_markers(&contents, relative)?;
    Ok(contents)
}

fn checked_file(root: &Path, relative: &str) -> Result<PathBuf> {
    ensure!(
        valid_repository_relative(relative),
        "governance path is not a safe repository-relative path: {relative:?}"
    );
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("missing governance input {relative}"))?;
    ensure!(
        metadata.file_type().is_file(),
        "governance input is not a regular file: {relative}"
    );
    Ok(path)
}

fn valid_repository_relative(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
        && !Path::new(value).is_absolute()
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn directory_files(root: &Path, relative: &str, extension: &str) -> Result<Vec<String>> {
    ensure!(
        valid_repository_relative(relative),
        "invalid directory path"
    );
    let directory = root.join(relative);
    let metadata = fs::symlink_metadata(&directory)
        .with_context(|| format!("missing governance directory {relative}"))?;
    ensure!(
        metadata.file_type().is_dir(),
        "governance directory is not a real directory: {relative}"
    );
    let mut files = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        ensure!(
            file_type.is_file(),
            "unexpected non-file governance entry {}",
            entry.path().display()
        );
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("governance filename is not UTF-8"))?;
        ensure!(
            Path::new(&name)
                .extension()
                .and_then(|value| value.to_str())
                == Some(extension),
            "unexpected governance file extension: {name}"
        );
        files.push(format!("{relative}/{name}"));
    }
    files.sort();
    Ok(files)
}

fn require_unique_lines(contents: &str, required: &[&str], relative: &str) -> Result<()> {
    for expected in required {
        let count = contents.lines().filter(|line| line == expected).count();
        ensure!(
            count == 1,
            "{relative} must contain {expected:?} exactly once, found {count}"
        );
    }
    Ok(())
}

fn metadata_value(contents: &str, prefix: &str, relative: &str) -> Result<String> {
    let values = contents
        .lines()
        .filter_map(|line| line.strip_prefix(prefix))
        .collect::<Vec<_>>();
    ensure!(
        values.len() == 1,
        "{relative} must contain metadata {prefix:?} exactly once"
    );
    let value = values[0].trim();
    ensure!(!value.is_empty(), "{relative} metadata {prefix:?} is empty");
    Ok(value.to_owned())
}

fn valid_task_id(value: &str) -> bool {
    let Some((milestone, task)) = value.split_once('-') else {
        return false;
    };
    let valid_milestone = milestone == "BASE"
        || milestone.strip_prefix('M').is_some_and(|digits| {
            !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
        });
    valid_milestone
        && task.len() == 3
        && task.bytes().all(|byte| byte.is_ascii_digit())
        && !value.contains(char::is_whitespace)
}

fn valid_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn reject_private_markers(contents: &str, relative: &str) -> Result<()> {
    for marker in ["/Users/", "/home/", "C:\\Users\\", "C:/Users/"] {
        ensure!(
            !contents.contains(marker),
            "{relative} contains private filesystem marker {marker:?}"
        );
    }
    Ok(())
}

fn yaml_get<'a>(node: &'a Yaml, key: &str) -> Option<&'a Yaml> {
    node.as_hash()?.get(&Yaml::String(key.to_owned()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
}

fn artifact(path: &Path, role: &str) -> Result<ArtifactEvidence> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to inspect {}", path.display()))?;
    Ok(ArtifactEvidence {
        name: path
            .file_name()
            .and_then(|value| value.to_str())
            .context("artifact filename is not UTF-8")?
            .to_owned(),
        role: role.to_owned(),
        bytes: metadata.len(),
        sha256: sha256_file(path)?,
    })
}

fn verify_public_output(directory: &Path, repository_root: &Path) -> Result<()> {
    let expected = [
        "SHA256SUMS",
        "governance.json",
        "manifest.json",
        "traceability.json",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();
    let mut observed = BTreeSet::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_file(),
            "governance evidence contains a non-file entry"
        );
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("evidence filename is not UTF-8"))?;
        let contents = fs::read_to_string(entry.path())
            .with_context(|| format!("evidence file {name} is not UTF-8"))?;
        reject_private_markers(&contents, &name)?;
        ensure!(
            !contents.contains(&repository_root.to_string_lossy().into_owned()),
            "evidence file {name} leaked the repository path"
        );
        observed.insert(name);
    }
    ensure!(
        observed == expected,
        "governance evidence directory has unexpected contents"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        reject_private_markers, require_unique_lines, valid_repository_relative, valid_task_id,
        validate_repository,
    };
    use crate::workspace_root;

    #[test]
    fn task_ids_are_strict_and_stable() {
        assert!(valid_task_id("M0-012"));
        assert!(valid_task_id("M10-001"));
        assert!(valid_task_id("BASE-009"));
        assert!(!valid_task_id("M-001"));
        assert!(!valid_task_id("M1-01"));
        assert!(!valid_task_id("M1-001-extra"));
        assert!(!valid_task_id(" M1-001"));
    }

    #[test]
    fn repository_paths_reject_escape_and_absolute_inputs() {
        assert!(valid_repository_relative("governance/changes/M0-012.json"));
        assert!(!valid_repository_relative("../M0-012.json"));
        assert!(!valid_repository_relative("/tmp/M0-012.json"));
        assert!(!valid_repository_relative("governance/./M0-012.json"));
    }

    #[test]
    fn markdown_contracts_require_exact_unique_headings() {
        require_unique_lines("# Title\n\n## Scope\n", &["# Title", "## Scope"], "fixture")
            .expect("valid headings");
        assert!(require_unique_lines("# Title\n# Title\n", &["# Title"], "fixture").is_err());
        assert!(require_unique_lines("# Different\n", &["# Title"], "fixture").is_err());
    }

    #[test]
    fn public_governance_text_rejects_private_paths() {
        reject_private_markers("target/evidence/report.json", "fixture").expect("public path");
        assert!(reject_private_markers("/Users/example/private.rom", "fixture").is_err());
        assert!(reject_private_markers("C:\\Users\\example\\private.adf", "fixture").is_err());
    }

    #[test]
    fn repository_governance_contracts_are_valid() {
        let root = workspace_root().expect("workspace root");
        let output = validate_repository(&root).expect("governance contracts must validate");
        assert!(output.contracts.len() >= 10);
        assert!(!output.architecture_decisions.is_empty());
        assert!(!output.release_notes.is_empty());
        assert!(!output.traceability.is_empty());
    }
}
