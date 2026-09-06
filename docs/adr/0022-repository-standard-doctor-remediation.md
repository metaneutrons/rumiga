# ADR-0022: Repository-Standard Doctor Remediation

- Status: Proposed
- Date: 2026-09-06
- Owners: @metaneutrons
- Task: M0-016

## Context

The repository-standard Doctor audit identified drift in the host toolchain,
web quality gates, local hooks, dependency lockfile workflow, security
documentation, and selected GitHub repository settings. The existing Node 26
pin was a current-line exception even though the repository standard requires
an LTS build tool. Web linting and type checking were not strict enough to be a
reliable local contract, and the npm lockfile workflow did not match the
standard pnpm baseline.

The audit also identified a timing constraint for branch protection: the
classic protection currently secures `main`, while the standard ruleset
transition must not be performed while foreign pull requests are open. There
are currently open Dependabot pull requests, so changing that control now would
create an avoidable review and enforcement transition.

## Decision

Adopt Node.js `24.20.0` LTS and pnpm `12.3.4` as the web build baseline. Track
`web/pnpm-lock.yaml`, enforce frozen installs, deny dependency lifecycle scripts
by default, and use Lefthook for the repository-owned commit, staged-file,
secret, formatting, and pre-push checks.

Make the web contract explicit with strict type-aware ESLint, TypeScript
typechecking, Vitest tests, and Rust and web coverage checks. Keep the same
repository-owned quality gates in local execution and GitHub Actions.

Apply the safe Class A GitHub settings: disable unused wiki and project
features, use squash-only merges with auto-merge enabled, enable the available
security controls and private vulnerability reporting, and retain automatic
branch deletion. Defer the Class B ruleset migration until no foreign pull
requests remain; the existing classic `main` protection stays in force.

Do not create releases, tags, publication workflows, release environments, or
release credentials as part of this remediation. Release planning is explicitly
out of scope for the current task.

## Consequences

Contributors must use the pinned Node and pnpm versions. The web package-lock is
removed in favor of pnpm's YAML lockfile, and installing the web package also
installs Lefthook. The stricter checks may expose existing TypeScript or
promise-handling defects, which is intentional and is covered by the current
code fixes and tests.

GitHub retains its current classic branch protection until the safe migration
window. This leaves an explicit operational follow-up rather than weakening
`main` protection during active foreign review.

## Alternatives

- Keep npm, the Node 26 exception, and the existing hooks: rejected because it
  preserves the audited standard drift.
- Replace classic protection with a ruleset immediately: rejected because
  foreign pull requests are open and the standard requires a clear transition
  window.
- Add release automation now: rejected because no release is currently
  planned.

## Evidence

The local web and Rust quality commands pass with the declared Node.js and pnpm
versions. The static supply-chain policy test and the repository-standard Doctor
check are also rerun. GitHub repository settings are read back after mutation;
private reporting and automated security fixes report enabled.

The Class B ruleset transition is deliberately not represented as a completed
change: foreign Dependabot pull requests #58 and #60 are open, so the existing
classic protection on `main` remains active. No release, tag, publication, or
release-environment action was executed.

## Supersession

This ADR supersedes ADR-0009's Node 26 current-line exception. ADR-0009 remains
in the repository as historical decision evidence.
