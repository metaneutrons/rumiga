# Rumiga Continuous Integration Contract

This document defines the required host, supply-chain, and target-build checks
implemented by M0-007 through M0-009. The workflow source is
`.github/workflows/ci.yml`; tool versions remain canonical in
`toolchain/manifest.toml` and its consuming files.

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

## Supply-Chain Contract

The supply-chain job installs exact Node.js, npm, `cargo-audit`, and
`cargo-deny` versions, then runs:

```sh
cargo +1.97.1 xtask supply-chain-evidence
(cd target/m0-009-supply-chain-evidence && sha256sum --check SHA256SUMS)
git diff --exit-code
```

The repository task first validates `supply-chain-policy.toml`, `deny.toml`,
both lockfiles, every workspace package, and every workflow. It then requires
zero Rust vulnerabilities or yanked packages, a RustSec database at most seven
days old, and zero high or critical npm advisories. License and informational
exceptions are exact-scope, owner-assigned, justified, compensated, expiring,
and fail when unused. CI uploads `supply-chain-<commit>` for 30 days; its
manifest and every raw scanner report are covered by `SHA256SUMS`.

## Target Build Contract

The portable gate compiles only the packages that are genuinely `no_std` today:

```sh
cargo +1.97.1 check --locked \
  --target riscv32imafc-unknown-none-elf \
  -p m68000 -p rumiga-api -p rumiga-platform
```

It deliberately does not claim that `rumiga-core` or `m68k` is `no_std`; that
conversion and its deterministic replay gate are M1.

The firmware gate installs the exact nightly, `ldproxy`, and `espflash` pins,
then runs:

```sh
cargo +1.97.1 xtask firmware-evidence
```

The repository-owned task builds a clean release target directory, verifies the
resolved ESP-IDF checkout commit and native compiler, rejects a dynamic or
non-RISC-V ELF, checks the D1001 SDK configuration and flash layout, creates an
unpadded merged image, and emits `rumiga.firmware.build.v1` evidence under
`target/m0-008-firmware-evidence`. `SHA256SUMS` covers the ELF, final linker map,
merged image, bootloader, partition table, resolved `sdkconfig`, flash arguments,
size report, and JSON manifest. CI validates those hashes before uploading
`firmware-esp32p4-<commit>` for 30 days.

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

CI intentionally uses only synthetic and repository-owned fixtures. It does not
access private Kickstart, Workbench, ADF, HDF, packet capture, or D1001 assets.
Firmware evidence proves compile, link, layout, and image generation only. Its
manifest explicitly excludes flashing, boot, peripherals, and performance.

The following remain separate milestones:

- M0-011: machine-readable compatibility and evidence artifacts.
- M2 and later: flash, boot, peripheral, browser, and hardware-in-loop proof.

## Local Validation

Before changing CI, run the host commands above and validate workflow syntax:

```sh
actionlint .github/workflows/ci.yml
cargo +1.97.1 xtask supply-chain-evidence
(cd target/m0-009-supply-chain-evidence && shasum -a 256 -c SHA256SUMS)
cargo +1.97.1 check --locked --target riscv32imafc-unknown-none-elf \
  -p m68000 -p rumiga-api -p rumiga-platform
cargo +1.97.1 xtask firmware-evidence
(cd target/m0-008-firmware-evidence && shasum -a 256 -c SHA256SUMS)
git diff --check
```

`actionlint` is a maintainer tool for workflow development; it is not yet a
repository-pinned release input. M0-010 will provide one repository-owned entry
point for local and CI quality gates.
