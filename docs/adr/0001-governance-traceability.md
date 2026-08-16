# ADR-0001: Version Governance And Traceability Contracts

- Status: Accepted
- Date: 2026-08-16
- Owners: @metaneutrons
- Task: M0-012

## Context

Rumiga had stable roadmap tasks and strong build/evidence gates, but no
repository-owned contribution, review, release-note, or decision workflow. A
pull request could therefore pass CI while its scope, evidence claim, rollback,
or architectural impact remained implicit. Markdown templates alone would make
the desired process visible but could silently drift from one another.

## Decision

Rumiga versions contributor and reviewer contracts in the repository. Material
changes carry a JSON record under `governance/changes/` that links one stable
task to tests, evidence, documents, a release note, risk/rollback, and any ADR.
The Rust-owned governance gate validates those records and templates, emits a
checksummed public artifact, and is a required CI dependency.

The machine-readable record supports process verification; it does not replace
reviewer judgment, branch protection, compatibility evidence, or D1001 HIL.

## Consequences

Changes gain an explicit audit trail and stale template/schema links fail CI.
Contributors must maintain one small structured record for material work.
Governance changes themselves require tests because they can block every pull
request. Release versioning and changelog policy remain M10-001 work.

## Alternatives

Relying only on a pull-request template was rejected because unchecked fields
and renamed files are not detectable from a clean checkout. A hosted external
project-management system was rejected as the source of truth because a clone
would not contain the complete engineering contract.

## Evidence

`cargo +1.97.1 xtask ci --gate governance` validates the contracts and produces
`target/m0-012-governance-evidence`. M0-012's change record is the first
end-to-end traceability example.

## Supersession

None.
