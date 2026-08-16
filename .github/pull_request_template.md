## Task And Scope

Task ID: <!-- Required, for example M1-003 -->
Change record: <!-- Required, for example governance/changes/M1-003.json -->

<!-- State the outcome and explicit non-goals. -->

## Behavior

<!-- Describe externally visible behavior and important internal contracts. -->

## Risk And Rollback

Risk: <!-- low, medium, high, or critical; explain the main failure mode -->
Rollback: <!-- Exact revert, feature-disable, or recovery procedure -->

## Verification

<!-- List exact commands and relevant results. -->

- [ ] Focused success, boundary, and failure tests pass.
- [ ] `cargo +1.97.1 xtask ci` passes, or remaining hosted-only gates are named.
- [ ] The final revision leaves tracked files unchanged after quality gates.

## Evidence

Evidence: <!-- Artifact/scenario path, CI run, or N/A with justification -->

- [ ] Claims match the available host or D1001 evidence level.
- [ ] Artifacts identify revision/dirty state and have complete checksums.
- [ ] Public artifacts contain no private media, credentials, or local paths.

## Architecture

ADR: <!-- docs/adr/NNNN-title.md or N/A with justification -->

- [ ] Public boundaries, safety policy, persistence, and target assumptions are
      unchanged or captured by an ADR.

## Release Note

Release note: <!-- docs/release-notes/unreleased/TASK-ID.md or N/A -->

- [ ] User, operator, compatibility, security, dependency, and contributor
      workflow impact is documented.

## Reviewer Checklist

- [ ] Scope is cohesive and unrelated user changes are preserved.
- [ ] Tests and evidence prove the stated behavior without overclaiming.
- [ ] Resource bounds, error paths, security, and rollback are credible.
- [ ] Task, change record, docs, release note, and ADR agree.
