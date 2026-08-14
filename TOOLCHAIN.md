# Rumiga Toolchain Baseline

`toolchain/manifest.toml` is the canonical, machine-readable build-input
manifest. The Rust, Cargo, npm, and target configuration files consume its
values; `firmware/tests/toolchain_manifest.rs` rejects drift between them.

This baseline pins inputs. It does not prove that the firmware cross-builds,
flashes, boots, or drives D1001 hardware. Those claims require M0-008 and M2
evidence.

## Rust-First Boundary

The emulator core, platform contracts, firmware composition, configuration,
tests, and product behavior remain Rust. The D1001 backend uses safe Rust
wrappers first. Vendor ESP-IDF and Seeed C components are admitted only behind
the single audited BSP FFI boundary described in `ARCHITECTURE.md`; Rumiga does
not add C application logic when a maintained Rust interface exists.

## Pinned Baseline

| Input | Pin | Role |
| --- | --- | --- |
| Host Rust | `1.97.1` | Formatting, Clippy, tests, desktop, and web server |
| Declared Rust MSRV | `1.85.0` | Minimum workspace language/tooling contract |
| Embedded Rust | `nightly-2026-07-27` with `rust-src` | ESP-IDF `std` build for the tier-3 RISC-V target |
| Rust target | `riscv32imafc-esp-espidf` | ESP32-P4 application target |
| Node.js | `24.19.0` | Web build LTS runtime |
| npm | `11.17.0` | Locked web installer |
| ESP-IDF | `5.4.2` at `f5c3654a1c2d2a01f7f67def7a0dc48e691f63c0` | Seeed-compatible firmware baseline |
| Seeed D1001 BSP | `5074d3b2f45626b261298e305aaf792036febc5a` | Board component source baseline |
| `esp-idf-svc` | `0.52.1` | Safe ESP-IDF services |
| `esp-idf-hal` | `0.46.2` | Safe ESP-IDF peripheral abstractions |
| `esp-idf-sys` | `0.37.2` | Generated ESP-IDF bindings and native builder |
| `embuild` | `0.33.3` | ESP-IDF build integration |
| `ldproxy` | `0.3.5` | ESP-IDF linker proxy |
| `espflash` | `4.5.0` | Image and serial tooling |

The exact IDF commit is passed to `esp-idf-sys`, not merely its mutable release
branch. Native IDF tools are installed under the ignored workspace `.embuild`
directory. Release and evidence builds must have `IDF_PATH` unset so a local
clone cannot override the repository pin.

## ESP-IDF 6

ESP-IDF `6.0.2` at `7101770dc6db2667b3c477cc31365dd1acd6db4e` is recorded as
the next upgrade candidate, not the D1001 baseline. At the audited Seeed BSP
revision:

- Seeed documents ESP-IDF 5.4.2;
- bundled audio component manifests constrain ESP-IDF to `<6.0`;
- the audited `esp-rs/esp-idf-template` revision does not offer an IDF 6 profile;
- no D1001 BSP compile or hardware-in-the-loop result exists for IDF 6.

Promotion to IDF 6 therefore requires an isolated compatibility change that
updates or replaces incompatible BSP components and passes firmware compile,
display, touch, audio, SD/MMC, Wi-Fi, USB, soak, and rollback gates. This is a
compatibility decision, not a preference for an older SDK.

## Setup

Install the exact Rust toolchains:

```sh
rustup toolchain install 1.97.1 --component clippy,rustfmt
rustup toolchain install nightly-2026-07-27 --profile minimal --component rust-src
```

Use `.node-version` with the local Node version manager, then install the pinned
Cargo tools:

```sh
cargo +1.97.1 install --locked --version 0.3.5 ldproxy
cargo +1.97.1 install --locked --version 4.5.0 espflash
```

Verify all cross-file pins without downloading ESP-IDF:

```sh
cargo test --locked -p rumiga-firmware --test toolchain_manifest
```

M0-008 will turn the following intended build shape into a CI-proven command
and publish ELF, map, binary, size, and checksum artifacts:

```sh
cd firmware
env -u IDF_PATH cargo build --locked --release \
  --target riscv32imafc-esp-espidf
```

## Update Rule

Host Rust, embedded nightly, Node/npm, ESP crates, ESP-IDF, BSP, and Cargo tools
move as one reviewed compatibility matrix when they interact. Every update must
change the canonical manifest and consuming files together, retain immutable
source revisions, pass the pin test and host gates, and add target/HIL evidence
for any IDF or BSP change.
