# Rumiga Toolchain Baseline

`toolchain/manifest.toml` is the canonical, machine-readable build-input
manifest. The Rust, Cargo, npm, and target configuration files consume its
values; `firmware/tests/toolchain_manifest.rs` rejects drift between them.

This baseline pins inputs and drives the repository-owned M0-008 firmware,
M0-009 supply-chain, and M0-010 unified quality gates. The local and hosted
pipelines produce and verify a complete ESP32-P4 build bundle. GitHub Actions run
[`31890919057`](https://github.com/metaneutrons/rumiga/actions/runs/31890919057)
closes the M0-008 build-evidence gate; flashing, booting, and D1001 peripherals
remain M2 HIL claims. GitHub Actions run
[`31894500079`](https://github.com/metaneutrons/rumiga/actions/runs/31894500079)
closes the M0-009 supply-chain gate with independently verified scanner
evidence. GitHub Actions run
[`31899884533`](https://github.com/metaneutrons/rumiga/actions/runs/31899884533)
closes M0-010 by running the same five repository-owned gate implementations
used by the single local command.

## Rust-First Boundary

The emulator core, platform contracts, firmware composition, configuration,
tests, and product behavior remain Rust. The D1001 backend uses maintained Rust
interfaces first and admits ESP-IDF C only through generated bindings or a
narrow audited adapter. Vellum's working D1001 port is both hardware evidence
and an authorized implementation source. Code owned by the shared copyright
holder may be adapted and distributed in Rumiga under `GPL-3.0-only` according
to the [Vellum reuse policy](docs/provenance/VELLUM_REUSE.md). Every transfer
records exact provenance; third-party and generated inputs remain subject to
their own licenses.

## Pinned Baseline

| Input | Pin | Role |
| --- | --- | --- |
| Host Rust | `1.97.1` | Formatting, Clippy, tests, desktop, and web server |
| Declared Rust MSRV | `1.85.0` | Minimum workspace language/tooling contract |
| Embedded Rust | `nightly-2026-07-27` with `rust-src` | ESP-IDF `std` build for the tier-3 RISC-V target |
| Rust target | `riscv32imafc-esp-espidf` | ESP32-P4 application target |
| Portable Rust target | `riscv32imafc-unknown-none-elf` | Current genuine `no_std` package boundary |
| Node.js | `24.19.0` | Web build LTS runtime |
| npm | `11.17.0` | Locked web installer |
| ESP-IDF | `6.0.0` at `662a3be354759d9487bf4b1a629fadb766cb1800` | Cross-built D1001 firmware baseline |
| Seeed D1001 BSP | `5074d3b2f45626b261298e305aaf792036febc5a` | Hardware reference only |
| `esp-idf-svc` | `0.52.1` plus pinned upstream IDF 6 revision | Safe ESP-IDF services |
| `esp-idf-hal` | `0.46.2` plus pinned upstream IDF 6 revision | Safe ESP-IDF peripheral abstractions |
| `esp-idf-sys` | `0.37.2` plus pinned upstream IDF 6 revision | Generated ESP-IDF bindings and native builder |
| `embuild` | `0.33.3` | ESP-IDF build integration |
| `ldproxy` | `0.3.5` | ESP-IDF linker proxy |
| `espflash` | `4.5.0` | Image and serial tooling |
| `cargo-audit` | `0.22.2` | RustSec vulnerability and yanked-package evidence |
| `cargo-deny` | `0.20.2` | License, source, advisory, and dependency-ban policy |

`esp-idf-sys` receives the release tag `v6.0`, while the canonical manifest
records its expected commit. M0-008 verification rejects any other resolved
commit. A tag is required because `embuild 0.33.3` checks out raw commits after
its recursive clone without refreshing submodules. Native IDF tools are
installed under the ignored workspace `.embuild`
directory. Release and evidence builds must have `IDF_PATH` unset so a local
clone cannot override the repository selection.

The [official D1001 specification](https://wiki.seeedstudio.com/getting_started_with_reterminal_d1001/)
lists 32 MB QSPI flash and 32 MB PSRAM. The M0 build uses the conservative 16 MB
flash geometry selected by both the
[pinned Seeed BSP](https://github.com/Seeed-Studio/reTerminal-D1001/blob/5074d3b2f45626b261298e305aaf792036febc5a/examples/factory_firmware/sdkconfig.defaults)
and the hardware-proven Vellum configuration. ESP-IDF writes the QIO
bootloader image in DIO mode and switches to quad mode during initialization;
the evidence task validates both values. Using the upper physical flash region
requires a later on-device qualification.

## ESP-IDF 6

ESP-IDF `6.0.0` is the active baseline. Two independent local observations
support it:

- Vellum revision `15bff64d316c3751861d02fcf7ace6b47afab176` builds and has
  D1001 bring-up evidence for boot, display, touch, audio, USB, and Wi-Fi.
- Rumiga cross-builds and links its Rust firmware for
  `riscv32imafc-esp-espidf` with the locked IDF 6 and esp-rs revisions.

The official Seeed repository still describes IDF 5.4.2 and is retained as a
hardware reference. Rumiga will port only the required board services through
safe Rust APIs and small audited FFI surfaces instead of importing the
monolithic BSP.

ESP-IDF `6.0.2` at `7101770dc6db2667b3c477cc31365dd1acd6db4e` remains a tracked
patch candidate. Its DSI bus config adds a `flags` field that upstream
`esp-idf-hal` revision `c2dac82f5243b0b7036c392f8218e6a2b4f7e375`
does not yet initialize, so the locked Rust build fails before Rumiga code.
Promotion requires a compatible upstream revision plus the full compile and
D1001 HIL gates.

## Setup

Install the exact Rust toolchains:

```sh
rustup toolchain install 1.97.1 --component clippy,rustfmt
rustup toolchain install 1.85.0 --profile minimal
rustup toolchain install nightly-2026-07-27 --profile minimal --component rust-src
```

Use `.node-version` with the local Node version manager, then install the pinned
Cargo tools:

```sh
cargo +1.97.1 install --locked --version 0.3.5 ldproxy
cargo +1.97.1 install --locked --version 4.5.0 espflash
cargo +1.97.1 install --locked --version 0.22.2 cargo-audit
cargo +1.97.1 install --locked --version 0.20.2 cargo-deny
```

Install the current portable target as a one-time toolchain prerequisite:

```sh
rustup target add --toolchain 1.97.1 riscv32imafc-unknown-none-elf
```

The canonical local validation command is then:

```sh
cargo +1.97.1 xtask ci
```

It validates exact Rust, Node, npm, scanner, firmware-tool, and target pins
before their respective gates. It does not install or switch global tools.

Verify all cross-file pins without downloading ESP-IDF:

```sh
cargo test --locked -p rumiga-firmware --test toolchain_manifest
```

Compile the current real `no_std` boundary through its canonical gate with:

```sh
cargo +1.97.1 xtask ci --gate portable
```

The foundation profile intentionally checks the platform crates separately;
the `stock-amiga-core` profile then compiles `m68k` and `rumiga-core` together
as an optimized `no_std` release. The host gate additionally checks that the
stock core graph compiles with the declared Rust 1.85 MSRV. Build, package, and
verify the full ESP-IDF firmware with:

```sh
cargo +1.97.1 xtask ci --gate firmware
```

The task owns a clean target directory, unsets ambient linker and IDF overrides,
checks the actual IDF commit and GCC path from CMake, validates the static
RISC-V ELF and final Rust linker map, enforces the D1001 configuration, and
creates the merged flash image from the ESP-IDF bootloader, the product partition
layout in `firmware/partitions.csv`, and the flash geometry declared by the
resolved `sdkconfig`. It then verifies that the merged image embeds the bootloader
byte for byte, carries the declared layout entry by entry, keeps the bootloader
inside its window, and leaves the application within its slot, because the image
tool otherwise substitutes its own defaults and rewrites the bootloader image
header. Its JSON manifest records source revision, tool versions, input and
artifact hashes, target metadata, the merged-image regions with the decoded
partition table, and explicit negative claims. Local dirty-worktree evidence is
marked as such; CI rejects it.

`esp-idf-sys` documents that a custom partition table declared in
`sdkconfig.defaults` is ignored by its generated CMake project. The layout is
therefore applied when the flashable image is generated, and the table the
ESP-IDF build emits is a build artifact rather than the shipped layout.

Generate and verify the M0-009 policy artifact with the exact host Rust, Node,
npm, and Cargo scanner versions above:

```sh
cargo +1.97.1 xtask ci --gate supply-chain
```

The task rejects tool drift before scanning and records the actual versions in
its manifest. It also requires a clean worktree when `CI=true`.

## Update Rule

Host Rust, embedded nightly, Node/npm, ESP crates, ESP-IDF, BSP, and Cargo tools
move as one reviewed compatibility matrix when they interact. Every update must
change the canonical manifest and consuming files together, retain immutable
source revisions, pass the pin test and host gates, and add target/HIL evidence
for any IDF or BSP change.
