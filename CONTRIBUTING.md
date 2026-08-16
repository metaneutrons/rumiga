# Contributing to Rumiga

Rumiga is an embedded-first Rust implementation of an Amiga emulator. Host
builds are development and reference environments; the Seeed reTerminal D1001
is the product target. Contributions must preserve that boundary and make each
claim traceable to a stable task, test, evidence record, and revision.

## Scope And Task

Before changing code, select a task from `IMPLEMENTATION_PLAN.md` or add a
reviewed task there. A pull request must name at least one stable task ID and
must keep unrelated cleanup out of the same functional commit.

Create or update `governance/changes/<TASK-ID>.json` for material work. The
record is the machine-readable link between scope, tests, evidence, affected
documents, release notes, and architecture decisions. Run the governance gate
before requesting review.

## Development Setup

Use the versions pinned by `toolchain/manifest.toml`, `rust-toolchain.toml`,
`.node-version`, and `web/package.json`. Install repository hooks once:

```sh
git config core.hooksPath .githooks
```

The complete local promotion command is:

```sh
cargo +1.97.1 xtask ci
```

Individual `--gate` runs are diagnostic subsets and are not a promotion result.

## Change Workflow

1. Start from a clean, current `main` and create a focused branch.
2. Add or update the task and machine-readable change record.
3. Implement the smallest complete behavioral change.
4. Add tests at the lowest useful level and broader evidence for shared or
   user-visible behavior.
5. Update public contracts, status, release notes, and ADRs when affected.
6. Run focused checks, then the complete quality command.
7. Open a pull request using `.github/pull_request_template.md` and keep it in
   draft until every required field is accurate.

Do not rewrite, discard, or reformat unrelated user changes in a dirty worktree.

## Tests And Evidence

Tests must cover success, boundary, and failure behavior appropriate to the
change. Compatibility claims require a versioned scenario or differential
reference artifact. D1001 claims require target-level HIL evidence; a desktop
run or cross-build alone cannot promote device behavior.

Evidence must identify its source revision and dirty state, use checksums, and
exclude copyrighted media, credentials, home paths, and private endpoints.
Generated evidence belongs under `target/` unless a synthetic fixture has been
explicitly approved for version control.

## Architecture Decisions

Add an ADR under `docs/adr/` before merging a decision that changes component
boundaries, public contracts, persistence, safety policy, target assumptions,
dependency strategy, or a cross-cutting quality rule. Accepted ADRs are not
silently rewritten; supersede them with a new numbered ADR.

## Release Notes

Material user, operator, compatibility, security, dependency, or contributor
workflow changes require one file under `docs/release-notes/unreleased/` based
on `docs/release-notes/TEMPLATE.md`. Purely internal changes may use `N/A` in
the pull request only when the reviewer agrees that behavior and operations are
unchanged.

## Commit And Pull Request

Every commit and pull-request title uses:

```text
<type>(<optional-scope>)!: <description>
```

Allowed types are `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`,
`refactor`, `revert`, `style`, and `test`. A scope starts and ends with a
lowercase ASCII letter or digit and may contain lowercase letters, digits,
`-`, `_`, `.`, or `/`. The header is at most 120 characters. Use `!` or a
`BREAKING CHANGE:` footer for an intentional breaking change; separate any
body or footer from the header with a blank line.

Use imperative functional subjects such as `feat(core): add bounded events`,
`fix(display): correct viewport origin`, `ci(evidence): publish manifest`, or
`docs(project): close M0-013 evidence`. WIP, `fixup!`, `squash!`, `amend!`,
and merge commits must not enter a promoted range. Each commit must build toward
one reviewable result.

The local `commit-msg` hook provides immediate feedback. The authoritative
hosted `commits` gate validates the pull-request range and title, then validates
the resulting `main` push again. Diagnose the current branch with:

```sh
cargo +1.97.1 xtask ci --gate commits
```

Do not commit ROMs, ADFs, HDFs, private screenshots, packet captures,
credentials, generated build trees, or machine-specific paths.

Complete every applicable pull-request checkbox. A checked box is an assertion
to the reviewer, not decoration. Link the change record and paste the exact
commands that were run.

## Review And Merge

Review follows `REVIEWING.md`. All required CI jobs and the fail-closed
`Required Quality Gate` must pass on the final revision. Resolve every blocking
finding, keep the branch current when required by protection rules, and use an
allowed merge strategy that preserves the intended functional history.

## Security And Private Media

Do not disclose a vulnerability in a public issue when exploitation details or
secrets would create risk. Contact the repository owner privately first. Never
attach licensed Amiga media or derived private evidence to issues, pull
requests, CI artifacts, or support bundles.
