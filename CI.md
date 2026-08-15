# Rumiga Continuous Integration Contract

This document defines the required host checks implemented by M0-007. The
workflow source is `.github/workflows/ci.yml`; tool versions remain canonical in
`toolchain/manifest.toml` and its consuming version files.

## Trigger And Concurrency Policy

CI runs for every pull request targeting `main`, every push to `main`, and
manual `workflow_dispatch` requests. A newer run for the same pull request or
Git ref cancels the obsolete run. No path filter may bypass required checks.

The workflow grants read-only repository access by default. Checkout does not
persist credentials. The Rust advisory job alone receives `checks: write`, as
required to publish its GitHub check. Third-party and GitHub-authored actions
are referenced by immutable commit SHA and annotated with the reviewed release.
Dependabot proposes action updates monthly.

## Required Jobs

| Job | Required behavior |
| --- | --- |
| `Lockfile Integrity` | Verify locked Cargo metadata, install npm dependencies from the lockfile without lifecycle scripts, reject high npm advisories, and reject lockfile mutation |
| `Host / Linux x86_64` | Run the complete Rust and web host command set on `ubuntu-24.04` |
| `Host / macOS arm64` | Run the complete Rust and web host command set on `macos-15` |
| `Rust Security Audit` | Check the locked Cargo graph against the RustSec advisory database |
| `Required Quality Gate` | Run unconditionally, summarize every prerequisite, and fail unless all required jobs succeeded |

Repository branch protection should require the stable
`CI / Required Quality Gate` check. Requiring the aggregate instead of every
matrix-generated name keeps branch protection stable while the matrix evolves;
the aggregate fails when any matrix leg fails, is cancelled, or is skipped.

## Host Matrix Contract

Both host legs use Rust `1.97.1`, Node.js `24.19.0`, and npm `11.17.0`. The
workflow validates the installed versions against repository-owned files before
building. Ubuntu installs `libglib2.0-dev`, `libslirp-dev`, and `pkg-config`;
macOS installs the equivalent Homebrew `libslirp` and `pkg-config` formulae.
Each leg then executes:

```sh
(
  cd web
  npm ci --ignore-scripts --no-audit --no-fund
  npm run lint
  npm run build
)

cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
```

The final cleanliness check rejects changes to tracked files produced by these
commands. GitHub's Rust and npm caches may improve runtime but are never build
inputs: every install and Cargo command remains lockfile-enforced. The web build
runs before Rust compilation because `rumiga-desktop` embeds the generated
`web/out` directory in its binary.

## Summaries And Evidence

Every job appends a concise Markdown result to `GITHUB_STEP_SUMMARY`. The
aggregate summary is the human-readable promotion record for the run. A local
commit cannot claim a GitHub-hosted result; hosted evidence must cite the run
URL and immutable commit after the commit reaches GitHub.

Host CI intentionally uses only synthetic and repository-owned fixtures. It
does not access private Kickstart, Workbench, ADF, HDF, packet capture, or D1001
assets.

The following remain separate milestones:

- M0-008: RISC-V `no_std` and ESP32-P4 firmware target builds.
- M0-009: complete advisory, license, source, and dependency policy.
- M0-011: machine-readable compatibility and evidence artifacts.
- M2 and later: flash, boot, peripheral, browser, and hardware-in-loop proof.

## Local Validation

Before changing CI, run the host commands above and validate workflow syntax:

```sh
actionlint .github/workflows/ci.yml
git diff --check
```

`actionlint` is a maintainer tool for workflow development; it is not yet a
repository-pinned release input. M0-010 will provide one repository-owned entry
point for local and CI quality gates.
