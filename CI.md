# Rumiga Continuous Integration Contract

This document defines the required commit-history, host, governance,
compatibility, supply-chain, and target-build checks implemented by M0-007
through M0-013.
Gate behavior is owned by
`xtask/src/ci.rs`, orchestration is defined in `.github/workflows/ci.yml`, and
tool versions remain canonical in `toolchain/manifest.toml` and its consuming
files.

## Trigger And Concurrency Policy

CI runs for every pull request targeting `main`, every push to `main`, and
manual `workflow_dispatch` requests. A newer run for the same pull request or
Git ref cancels the obsolete run. No path filter may bypass required checks.

The workflow grants read-only repository access. Checkout does not persist
credentials. Third-party and GitHub-authored actions are referenced by
immutable commit SHA and annotated with the reviewed release. Dependabot
proposes action updates monthly. `cargo-audit` and `cargo-deny` are built with
the pinned host Rust, installed with `--locked`, and checked against the exact
versions recorded in `toolchain/manifest.toml`.

## Required Jobs

| Job | Required behavior |
| --- | --- |
| `Commit Policy` | Validate every selected raw Git commit message and the pull-request title against the repository-owned Conventional Commit policy; reject merge, WIP, and autosquash commits |
| `Lockfile Integrity` | Verify locked Cargo metadata, install npm dependencies from the lockfile without lifecycle scripts, and reject lockfile mutation |
| `Engineering Governance Evidence` | Validate contribution, review, issue, PR, ADR, release-note, and change-record contracts; upload checksummed task-to-evidence traceability |
| `Host / Linux x86_64` | Run the complete Rust, core feature-matrix, and web host command set on `ubuntu-24.04` |
| `Host / macOS arm64` | Run the complete Rust, core feature-matrix, and web host command set on `macos-15` |
| `Public Compatibility Evidence` | Classify every scenario, verify the asset-free REST/web contract, inventory Cargo tests and reviewed ignores, and upload a private-media-free checksummed bundle |
| `Supply Chain Policy` | Enforce Cargo/npm source, checksum, license, duplicate, advisory, lifecycle-script, and immutable-Action policy; upload checksummed scanner evidence |
| `Portable Rust / RISC-V no_std` | Compile the current `no_std` package boundary for bare-metal 32-bit RISC-V |
| `Firmware / ESP32-P4 release evidence` | Cross-build, inspect, package, checksum, and upload the pinned D1001 firmware evidence |
| `Required Quality Gate` | Run unconditionally, summarize every prerequisite, and fail unless all required jobs succeeded |

The protected `main` branch requires the stable `Required Quality Gate` check
from the GitHub Actions app (`app_id 15368`). Requiring the aggregate instead
of every matrix-generated name keeps branch protection stable while the matrix
evolves; the aggregate fails when any required job fails, is cancelled, or is
skipped.

## Canonical Gate Orchestrator

The complete local entry point is:

```sh
cargo +1.97.1 xtask ci
```

It runs `commits`, `lockfiles`, `governance`, `host`, `compatibility`,
`supply-chain`, `portable`, and `firmware` in that order. GitHub jobs preserve matrix
parallelism by calling the same implementation with `--gate <name>`. The
repository test suite structurally parses the workflow and rejects missing,
extra, relocated, or version-drifted gate invocations as well as aggregate
dependency drift.

Every gate validates the relevant pinned tools. A gate snapshots tracked Git
status plus staged and unstaged diff hashes before execution and rejects any
mutation afterward. Local work may start dirty as long as the gate leaves it
byte-for-byte unchanged; `CI=true` requires a clean tracked checkout before a
gate starts. Evidence gates also verify that `SHA256SUMS` covers exactly the
regular files in each artifact directory using safe basenames and matching
SHA-256 values.

`cargo +1.97.1 xtask ci --list` displays the stable names. Repeated `--gate`
options select a diagnostic subset while retaining canonical order. A subset is
not a promotion result; only the command without `--gate` is the complete local
baseline, and only the hosted matrix proves both supported host platforms.

## Commit Policy Contract

The commit-policy job checks out complete Git history and runs:

```sh
cargo +1.97.1 xtask ci --gate commits
```

For pull requests, the workflow supplies immutable base and head object IDs;
the Rust validator computes their merge base and validates every commit unique
to the head. It also validates the pull-request title because that title can
become the retained header under a squash merge. Pushes to `main` validate the
event's before/head range again. A manual run without a base validates its head
commit; local execution selects commits after the `origin/main` merge base, or
`HEAD` when no unique commit exists.

The parser reads bounded raw commit objects rather than formatted log output.
Messages must be UTF-8 with LF endings and use
`<type>(<optional-scope>)!: <description>` with a maximum 120-character header.
The accepted types are `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`,
`refactor`, `revert`, `style`, and `test`. Optional scopes are lowercase ASCII;
bodies require a blank separator. Breaking markers and `BREAKING CHANGE:`
footers are supported. Merge, WIP, `fixup!`, `squash!`, and `amend!` commits
fail closed.

`.githooks/commit-msg` invokes the same parser against the proposed message for
fast local feedback. Because local hooks can be bypassed, only the required
hosted job and its aggregate dependency are promotion evidence. No npm package
or external commit-policy service is required.

## Engineering Governance Contract

The governance job runs:

```sh
cargo +1.97.1 xtask ci --gate governance
```

The Rust validator requires the repository-owned contribution and review
policies, GitHub PR/issue templates, CODEOWNERS, ADR lifecycle and template,
release-note lifecycle and template, JSON change-record schema, and every
task-named record. It rejects missing or duplicate headings, unsafe or symlinked
paths, unknown JSON fields, invalid status/risk/impact values, stale task links,
missing documents, release notes or ADRs owned by another task, duplicate test
or evidence IDs, and pending evidence on a verified record.

The resulting `target/m0-012-governance-evidence` directory contains
`governance.json`, normalized `traceability.json`, `manifest.json`, and exact
`SHA256SUMS`. Its manifest records the source revision and dirty state, hashes
every contract input, and explicitly excludes human-review, branch-protection,
and release-versioning claims. Output scanning rejects home/workspace paths;
CI uploads `governance-<commit>` for 30 days only after exact checksum coverage
passes.

The gate proves that the versioned workflow and links are internally
consistent. It does not claim that a human approved a change, that GitHub
branch settings match policy, or that host evidence proves D1001 behavior.

## Host Matrix Contract

Both host legs use Rust `1.97.1`, the declared Rust `1.85.0` MSRV, Node.js
`24.19.0`, and npm `11.17.0`. The workflow validates the installed versions
against repository-owned files before building. Ubuntu installs `libglib2.0-dev`,
`libslirp-dev`, and `pkg-config`; macOS installs the equivalent Homebrew
`libslirp` and `pkg-config` formulae. Each leg executes the canonical host gate:

```sh
cargo +1.97.1 xtask ci --gate host
```

The gate expands to the locked npm install, web lint and production build,
Rust format, the `m68k` and `rumiga-core` runtime matrices, locked workspace
Clippy and tests, and warning-free Rustdoc. The runtime matrices explicitly
compile and lint `std`, lint and test `no_std`, and verify that invalid
selections fail with the required diagnostics. `rumiga-core` denies
standard-library primitives when `core` or `alloc` provides the same contract;
this is therefore checked in the desktop profile as well as the portable one.
The same gate compiles the stock `no_std` core graph with the declared MSRV, so
the workspace `rust-version` cannot silently fall behind the code. The CPU
matrix also rejects FPU under `no_std` and tests the stock Line-F fallback.
Default workspace commands continue to exercise the FPU-enabled
desktop graph. The web build runs before Rust compilation because
`rumiga-desktop` embeds the generated `web/out` directory in its binary.

GitHub's Rust and npm caches may improve runtime but are never build inputs:
every install and Cargo command remains lockfile-enforced.

## Public Compatibility Contract

The public compatibility job runs:

```sh
cargo +1.97.1 xtask ci --gate compatibility
```

The Rust task validates `evidence/scenarios.json`, compares 25 shared DTO
structs, 10 enums, and 20 REST endpoint contracts, and discovers tests from
Cargo-built libtest harnesses plus rustdoc. Every framework-level ignored test
must exactly match `evidence/ignored-tests.json`; unknown and stale entries
both fail the gate.

The resulting `target/m0-011-compatibility-evidence` directory contains
versioned JSON reports, a Markdown report, input and payload hashes, source
revision, stable skipped reasons, and exact reproduction/validation commands.
The builder never reads `target/evidence`, scans output for local workspace and
home paths, and explicitly excludes ROMs, ADFs, HDFs, screenshots, packet
captures, and local media hashes. CI uploads `compatibility-<commit>` for 30
days only after exact `SHA256SUMS` coverage passes.

The compatibility job inventories tests; the required host matrix executes
them. The aggregate requires both jobs, so an inventory is never presented as
test execution. Likewise, a private-media scenario skipped in public CI is not
a compatibility pass.

## Supply-Chain Contract

The supply-chain job installs exact Node.js, npm, `cargo-audit`, and
`cargo-deny` versions, then runs:

```sh
cargo +1.97.1 xtask ci --gate supply-chain
```

The repository task first validates `supply-chain-policy.toml`, `deny.toml`,
both lockfiles, every workspace package, and every workflow. It then requires
zero Rust vulnerabilities or yanked packages, a RustSec database at most seven
days old, and zero high or critical npm advisories. License and informational
exceptions are exact-scope, owner-assigned, justified, compensated, expiring,
and fail when unused. CI uploads `supply-chain-<commit>` for 30 days; its
manifest and every raw scanner report are covered by `SHA256SUMS`; the gate
validates the complete manifest before returning success.

## Target Build Contract

The portable job runs:

```sh
cargo +1.97.1 xtask ci --gate portable
```

The target and build profiles come from `toolchain/manifest.toml`. They resolve
to `riscv32imafc-unknown-none-elf`. The `foundation` profile checks `m68000`,
`rumiga-api`, and `rumiga-platform`; the `stock-amiga-core` profile checks
`m68k` and `rumiga-core` together in release mode with defaults disabled and
`no_std` selected. This proves the complete stock CPU/core dependency graph
against a bare-metal 32-bit RISC-V standard library boundary. It does not prove
linking with a firmware allocator, execution, performance, or D1001 behavior.

The firmware gate installs the exact nightly, `ldproxy`, and `espflash` pins,
then runs:

```sh
cargo +1.97.1 xtask ci --gate firmware
```

The repository-owned task builds a clean release target directory, verifies the
resolved ESP-IDF checkout commit and native compiler, rejects a dynamic or
non-RISC-V ELF, checks the D1001 SDK configuration and flash layout, creates an
unpadded merged image from the ESP-IDF bootloader, the product partition layout in
`firmware/partitions.csv`, and the flash geometry declared by the resolved
`sdkconfig`, verifies that the merged image embeds the bootloader byte for byte,
carries the declared layout entry by entry, keeps the bootloader inside its
window, and leaves the application within its slot, and emits
rejects any configuration that could burn an eFuse, and emits
`rumiga.firmware.build.v1` evidence under
`target/m0-008-firmware-evidence`. `SHA256SUMS` covers the ELF, final linker map,
merged image, bootloader, partition table, resolved `sdkconfig`, flash arguments,
size report, and JSON manifest. CI validates those hashes before uploading
`firmware-esp32p4-<commit>` for 30 days. The gate verifies every generated hash
before upload.

The physical board has 32 MB flash and 32 MB PSRAM. M0-008 intentionally keeps
the 16 MB flash geometry used by the pinned Seeed BSP and the hardware-proven
Vellum baseline. The generated evidence records both values; expanding the
firmware geometry requires D1001 flash and boot evidence.

## Summaries And Evidence

Every job appends a concise Markdown result to `GITHUB_STEP_SUMMARY`. The
aggregate summary is the human-readable promotion record for the run. A local
commit cannot claim a GitHub-hosted result; hosted evidence must cite the run
URL and immutable commit after the commit reaches GitHub.

The first M0-008 hosted baseline is GitHub Actions run
[`31890919057`](https://github.com/metaneutrons/rumiga/actions/runs/31890919057)
for head commit `3cd47ddb3bb02eb9eecde59a651dcebe0badcf99`. Its pull-request
merge revision `fb273fca2fa8c52cb42c8e2738d11418a288ddbc` produced artifact
`firmware-esp32p4-fb273fca2fa8c52cb42c8e2738d11418a288ddbc` (artifact ID
`9248602076`, archive SHA-256
`a49535d56c0be4740ce6711a99e28829608044e99ada9be66e7b5cf593c5cc7e`).
All nine payload checksums were independently revalidated after download.

The first M0-009 hosted baseline is GitHub Actions run
[`31894500079`](https://github.com/metaneutrons/rumiga/actions/runs/31894500079)
for branch head `53e154d8cecc0d3f9359ba023be6e5803c251b87`. Pull-request merge
revision `055b0ae3ed36a44c44aa7314ac928545dc7262ae` produced artifact
`supply-chain-055b0ae3ed36a44c44aa7314ac928545dc7262ae` (artifact ID
`9249484883`, archive SHA-256
`2c477e759400e0d12e7139b3613fd7bd10f4f0dd07d20f4016c5edc48387f0c9`).
All seven payload checksums and the clean-revision claim were independently
revalidated after download.

The first M0-010 hosted baseline is GitHub Actions run
[`31899884533`](https://github.com/metaneutrons/rumiga/actions/runs/31899884533)
for branch head `e2f7d653df91ce53842d649ec85edc756d4b6f2f`. Every prerequisite
invokes its repository-owned `xtask ci --gate` implementation and passes on the
hosted Linux x86_64/macOS arm64 matrix. Pull-request merge revision
`20c280bddd2a28597534efb1bac053f6c5ea859b` produced supply-chain artifact
`9250843826` (archive SHA-256
`9bdc8283b6fbf8faaf1d766df658e4df927c07d1411b02da4cf0786595cb9440`)
and firmware artifact `9250846613` (archive SHA-256
`71b4fc0c6f05109b441dbd91eb8c5d3bee86c69e9c67da58a0037783ed7eea91`).
All 7 and 9 payload hashes and both clean-revision claims were independently
revalidated after download.

The first M0-011 hosted baseline is GitHub Actions run
[`31910408906`](https://github.com/metaneutrons/rumiga/actions/runs/31910408906)
for branch head `aff4a6e680ab71aeff94f7416823008319156582`. Pull-request merge
revision `c61242bd545fc4fd6bedc28f217bcd2695955529` produced artifact
`compatibility-c61242bd545fc4fd6bedc28f217bcd2695955529` (artifact ID
`9253512112`, archive SHA-256
`ee634d0f429c673e465776cb70de002adaf3867a539623374e57e3332444d00a`).
Independent download verification confirmed exact archive coverage, all five
payload checksums, the clean revision, scenario/test totals, reviewed ignores,
privacy flags, and no private filesystem paths.

The first M0-012 hosted baseline is GitHub Actions run
[`31933087138`](https://github.com/metaneutrons/rumiga/actions/runs/31933087138)
for branch head `ad461580287229366c6b0492e9cfedad2f6610fe`. Pull-request merge
revision `11e68bddf0f7739ed11711c97de0483f8381b6a6` produced artifact
`governance-11e68bddf0f7739ed11711c97de0483f8381b6a6` (artifact ID
`9259855560`, archive SHA-256
`249614ac364af890f92da3dcb8a1a3e3917f4be553fb54eade2d4c314ccbb480`).
Independent download verification confirmed exactly four regular files, all
three payload checksums, a clean source revision, the 13-contract and
task-link totals, public scope flags, and no private filesystem paths.

The M1-001 core runtime-profile baseline is GitHub Actions run
[`31934749529`](https://github.com/metaneutrons/rumiga/actions/runs/31934749529)
for branch head `f538f0ba811691703dd88b1c75d7cceaa5dc8676`. Linux host job
`95134810493` and macOS host job `95134810516` independently pass the explicit
`std`, `no_std`, invalid-selection, and default-workspace matrix before the
strict aggregate succeeds. Pull-request merge revision
`aab85a06bd8c893397b5e9ac719c77863628c5a1` produced artifact
`governance-aab85a06bd8c893397b5e9ac719c77863628c5a1` (artifact ID
`9260313104`, archive SHA-256
`e59760be76a9b1be3599fed1dc8300c08c64ed667c13506d224a122e7042c7b6`).
Independent download verification confirmed all three payload checksums, the
clean revision, and the complete two-task traceability set.

The M0-013 Conventional Commit promotion starts with a complete local
eight-gate pass in 91.516 seconds. Pull-request run
[`31952285487`](https://github.com/metaneutrons/rumiga/actions/runs/31952285487)
validated three branch commits plus the PR title in commit-policy job
`95177500999`; strict aggregate job `95178194590` passed. PR merge revision
`cb87993e1b5671b4bd33753a54eb80504bd6310a` produced governance artifact
`9264985708` with archive SHA-256
`beafa7e754e75df43be4a5ea9f5f0a852195a54c999798a1a265a7471c88085e`.
Independent download verification passed all three payload checksums and the
clean-source claim.

Final `main` push run
[`31952671051`](https://github.com/metaneutrons/rumiga/actions/runs/31952671051)
validated all three promoted commits in
`89be3955ecf77841f659d95354e7186af27d5847..f2505b34676929b0a2bc99ee3b4203db7a9ed80b`
through commit-policy job `95178459474`; aggregate job `95179110310` passed.
Governance artifact `9265088957` has archive SHA-256
`a88ff2e04d9e623094baf83e351a5a134ddda82a856bdcde14a0eed2f038b81d`.
Its source is clean revision `f2505b34676929b0a2bc99ee3b4203db7a9ed80b`,
its report result is `pass`, and every payload checksum was independently
revalidated after download.

CI intentionally uses only synthetic and repository-owned fixtures. It does not
access private Kickstart, Workbench, ADF, HDF, packet capture, or D1001 assets.
Firmware evidence proves compile, link, layout, and image generation only. Its
manifest explicitly excludes flashing, boot, peripherals, and performance.

The following remain separate milestones:

- Media-backed compatibility and differential reference evidence remain local
  or require a controlled private runner.
- M2 and later: flash, boot, peripheral, browser, and hardware-in-loop proof.

## Local Validation

Install every tool pinned by `toolchain/manifest.toml`, ensure `.node-version`
is first on `PATH`, install the declared portable Rust target, and run:

```sh
cargo +1.97.1 xtask ci
```

The command does not install, upgrade, or switch machine-global tools. Missing
or mismatched versions fail before the relevant build with the expected pin.
When changing workflow syntax, maintainers additionally run
`actionlint .github/workflows/ci.yml`; `actionlint` is not yet a
repository-pinned release input.
