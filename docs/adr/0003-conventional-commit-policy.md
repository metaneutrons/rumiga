# ADR-0003: Repository-Owned Conventional Commit Policy

- Status: Accepted
- Date: 2026-08-16
- Owners: @metaneutrons
- Task: M0-013

## Context

Rumiga's local `commit-msg` hook used one shell regular expression. It could be
bypassed with `--no-verify`, was not evaluated again by GitHub Actions, and did
not cover breaking-change markers, revert commits, complete branch ranges, or
squash-merge titles. A local convention therefore could not protect the linear
history retained on `main`.

The repository already owns its quality orchestration in Rust. Adding a second
JavaScript policy stack solely for commit messages would duplicate toolchain,
lockfile, update, and supply-chain responsibilities.

## Decision

Rumiga owns Conventional Commit validation in `rumiga-xtask`. One parser is
used by the local `commit-msg` hook and the canonical `commits` quality gate.
The allowed types are `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`,
`refactor`, `revert`, `style`, and `test`. Scopes are optional, lowercase ASCII
identifiers; `!` and `BREAKING CHANGE:` are supported. Headers are limited to
120 characters and bodies require a blank separator.

The CI gate reads complete raw Git commit objects, rejects merge, autosquash,
WIP, non-UTF-8, unsafe-control-character, and malformed messages, and validates
every commit after the event merge base. Pull requests also validate their
title so a future squash merge cannot create a nonconforming `main` commit.
Pushes to `main` validate the event range again. The fail-closed aggregate
requires the commit-policy job.

Local hooks provide immediate feedback but are not evidence. GitHub Actions is
the authoritative enforcement boundary because it runs from a clean checkout
with explicit full object IDs and cannot be bypassed by `git --no-verify`.

## Consequences

Every retained commit and merge-capable pull-request title has one predictable,
machine-checked form. Dependabot continues to work through its existing
`chore(deps)`, `chore(deps-web)`, and `chore(ci-deps)` prefixes. The complete
local promotion gains one fast Git-history gate, while hosted CI gains one
small Rust job and an additional required aggregate dependency.

Contributors must fix or reword invalid commits before promotion. Historical
commits before the selected merge base are deliberately not retroactively
rewritten. Semantic accuracy still requires review; syntax cannot prove that a
commit is well scoped or that its declared breaking change is justified.

## Alternatives

Keeping the shell regex was rejected because it is local-only and cannot safely
validate full messages or Git ranges. Adding `commitlint` was rejected because
it would add a second package ecosystem and policy implementation for behavior
already suited to the repository's Rust quality owner. Validating only pull-
request titles was rejected because rebase merges preserve the individual
commit messages.

## Evidence

`cargo +1.97.1 test --locked -p rumiga-xtask commit_policy::tests` covers valid,
breaking, revert, Dependabot, malformed, WIP, autosquash, size, encoding, and
scope cases. `cargo +1.97.1 xtask ci --gate commits` validates the selected Git
range and the structural CI test requires the job in `Required Quality Gate`.
The complete local eight-gate baseline passes in 91.516 seconds. GitHub Actions
pull-request run
[`31952285487`](https://github.com/metaneutrons/rumiga/actions/runs/31952285487)
validates all three commits and the PR title; final `main` run
[`31952671051`](https://github.com/metaneutrons/rumiga/actions/runs/31952671051)
validates the exact promoted three-commit range. Both strict aggregates pass,
and both downloaded governance bundles pass archive and payload checksum
verification with clean source revisions.

## Supersession

None.
