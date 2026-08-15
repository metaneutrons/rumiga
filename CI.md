# Rumiga Continuous Integration Contract

This document defines the required host, supply-chain, and target-build checks
implemented by M0-007 through M0-010. Gate behavior is owned by
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
| `Lockfile Integrity` | Verify locked Cargo metadata, install npm dependencies from the lockfile without lifecycle scripts, and reject lockfile mutation |
| `Host / Linux x86_64` | Run the complete Rust and web host command set on `ubuntu-24.04` |
| `Host / macOS arm64` | Run the complete Rust and web host command set on `macos-15` |
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

It runs `lockfiles`, `host`, `supply-chain`, `portable`, and `firmware` in that
order. GitHub jobs preserve matrix parallelism by calling the same
implementation with `--gate <name>`. The repository test suite structurally
parses the workflow and rejects missing, extra, relocated, or version-drifted
gate invocations as well as aggregate dependency drift.

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

## Host Matrix Contract

Both host legs use Rust `1.97.1`, Node.js `24.19.0`, and npm `11.17.0`. The
workflow validates the installed versions against repository-owned files before
building. Ubuntu installs `libglib2.0-dev`, `libslirp-dev`, and `pkg-config`;
macOS installs the equivalent Homebrew `libslirp` and `pkg-config` formulae.
Each leg executes the canonical host gate:

```sh
cargo +1.97.1 xtask ci --gate host
```

The gate expands to the locked npm install, web lint and production build,
Rust format, locked workspace Clippy and tests, and warning-free Rustdoc. The
web build runs before Rust compilation because `rumiga-desktop` embeds the
generated `web/out` directory in its binary.

GitHub's Rust and npm caches may improve runtime but are never build inputs:
every install and Cargo command remains lockfile-enforced.

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

The target and package set come from `toolchain/manifest.toml`. They currently
resolve to `riscv32imafc-unknown-none-elf` and `m68000`, `rumiga-api`, and
`rumiga-platform`. The gate deliberately does not claim that `rumiga-core` or
`m68k` is `no_std`; that conversion and its deterministic replay gate are M1.

The firmware gate installs the exact nightly, `ldproxy`, and `espflash` pins,
then runs:

```sh
cargo +1.97.1 xtask ci --gate firmware
```

The repository-owned task builds a clean release target directory, verifies the
resolved ESP-IDF checkout commit and native compiler, rejects a dynamic or
non-RISC-V ELF, checks the D1001 SDK configuration and flash layout, creates an
unpadded merged image, and emits `rumiga.firmware.build.v1` evidence under
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

CI intentionally uses only synthetic and repository-owned fixtures. It does not
access private Kickstart, Workbench, ADF, HDF, packet capture, or D1001 assets.
Firmware evidence proves compile, link, layout, and image generation only. Its
manifest explicitly excludes flashing, boot, peripherals, and performance.

The following remain separate milestones:

- M0-011: machine-readable compatibility and evidence artifacts.
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
