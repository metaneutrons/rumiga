# ADR-0019: Hardware Manifest Provenance

- Status: Accepted
- Date: 2026-08-18
- Owners: @metaneutrons
- Task: M2-001

## Context

M1 established the emulator core and its portability contract against a target board
nobody had documented. `ARCHITECTURE.md` already made hardware claims that later work
depends on: an 800×1280 MIPI-DSI panel, an ES8311 codec, an ESP32-C6 network
coprocessor, and a 32 MiB PSRAM budget that the whole memory plan rests on. None of them
named a source, so none could be checked.

M2 begins the device work. Every task in it will need part numbers, connector
identities, and bus addresses. Deriving those repeatedly from a product page is how a
marketing figure becomes a design assumption.

## Decision

Every value in a hardware manifest names its source, and the manifest distinguishes what
a schematic shows from what a product page asserts. The two are tagged separately rather
than merged, because they differ in kind: a schematic is a document of record for a
revision, a product page is a description of a family.

Where they disagree, or where a claim cannot be resolved to a component, the manifest
records the gap instead of choosing the more convenient reading. Those gaps are the
manifest's most valuable content, because the alternative is discovering them on a bench
with a board in hand.

The schematic PDF is checksummed on download and the checksum recorded. Seeed publishes
no checksum, so this is not a vendor attestation and the manifest says so; it exists so a
later reader can tell whether the document they have is the one the manifest was written
from.

The BSP is referenced by commit SHA, not by branch. The repository publishes no tags, so
a SHA is the only stable reference available.

Derived values stay out. Flash layout belongs to `firmware/partitions.csv`, memory
budgets to `ARCHITECTURE.md`, toolchain pins to `toolchain/manifest.toml`. A manifest
that repeated them would drift from them, and the drift would be invisible.

## Consequences

Three claims in the existing architecture now have a source, and one is weaker than it
read. The panel, the codec, and the coprocessor are confirmed by the schematic or the
wiki. The 32 MiB PSRAM figure is a wiki claim about the `ESP32-P4NRW32` variant; the
schematic symbol carries only the family name. Since the memory budget rests on that
figure, the manifest records where it comes from rather than treating it as established.

Reading the schematic produced two findings that the vendor's own overview does not
support. The wiki advertises "flexible expansion interfaces (GPIO, I2C, UART)", but the
V01 schematic's only 2.54 mm header carries the ESP32-C6's programming signals. The
`EXP_GPO` net names belong to a `PCA9535RGER` expander at I2C 0x20 whose outputs drive
on-board functions, so despite the sheet label they are internal fan-out rather than a
connector. Separately, the schematic carries a `Wio-LR1121` sub-GHz module and an SMA
connector that the wiki never mentions.

Neither finding is resolved here, and neither should be. A schematic cannot say what a
shipped unit populates, and guessing would put a fabricated fact into the document the
rest of M2 will trust.

The published file stamp is recorded separately from the revision. The archive is named
`260715` while the schematic's own revision history dates V01 to 2025-10-15. Recording
the filename as the revision would have implied a 2026 revision the document does not
claim.

## Alternatives

Recording the product page's specification table as the manifest was rejected. It is the
fastest path and it is the one that turns marketing figures into design assumptions; it
also could not have produced connector designators, which are what M2's bring-up tasks
actually need.

Omitting the unresolved questions was rejected. A manifest that lists only what is known
reads as complete, and the next task would plan against an expansion header that may not
exist.

Resolving the questions by inference was rejected. The absence of a "do not populate"
marking is not evidence of population, and the absence of a header in one revision's
schematic is not evidence that no unit exposes user I/O.

Vendoring the schematic PDF into the repository was rejected. It is 2 MB of
CC BY-SA 4.0 material that Seeed hosts, and the checksum plus URL identifies it without
copying it.

Waiting for a physical board was rejected. Every document-derived value is usable now,
and the manifest states plainly that nothing in it has been verified against hardware.

## Evidence

The manifest under `docs/hardware/reterminal-d1001.md` records eleven connectors with
designators and manufacturer parts, twelve main-board parts, five module parts, the
schematic revision and its date from the revision history sheet, and the BSP commit.
Each entry carries a source tag.

The schematic was read with `pdftotext -layout` over the downloaded PDF, whose SHA-256 is
recorded in the manifest. The BSP commit was read through the GitHub API.

## Supersession

None. This establishes the manifest format for the boards this project targets.

The manifest covers the reTerminal D1001 main board V1.0. Four vendor documents were not
read: the SCH and PCB source archive, the SoC and peripheral datasheets, and the 3D model
that would answer the dimensions question.
