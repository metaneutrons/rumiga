# ADR-0007: Product Flash Partition Layout

- Status: Accepted
- Date: 2026-08-17
- Owners: @metaneutrons
- Task: M2-013

## Context

The firmware bundle used the stock ESP-IDF single-application table: 24 KiB
`nvs`, 4 KiB `phy_init`, and a 1 MiB `factory` slot. It has no OTA slots, no
`otadata`, and no data partition, so it cannot support update, rollback, or
device-side storage.

Two constraints make the layout expensive to change later. An application slot
size is effectively permanent once devices ship, because an OTA image must fit
the table already flashed on the device; enlarging a slot afterwards requires a
cable and physical access. The partition-table offset is equally sticky for the
same reason.

The bootloader window was also too small to notice. `CONFIG_PARTITION_TABLE_OFFSET`
was `0x8000`, which leaves 24,576 bytes from the `0x2000` bootloader offset. The
built bootloader is 24,096 bytes, so 480 bytes remained. A Secure Boot V2
signature block is 4 KiB and does not fit, and any ESP-IDF patchlevel could have
broken the build without warning.

## Decision

`firmware/partitions.csv` owns the product layout:

| Partition | Type | Subtype | Offset | Size |
| --- | --- | --- | --- | --- |
| nvs | data | nvs | `0x011000` | 320 KiB |
| nvs_keys | data | nvs_keys | `0x061000` | 4 KiB |
| otadata | data | ota | `0x062000` | 8 KiB |
| phy_init | data | phy | `0x064000` | 4 KiB |
| coredump | data | coredump | `0x065000` | 108 KiB |
| ota_0 | app | ota_0 | `0x080000` | 6 MiB |
| ota_1 | app | ota_1 | `0x680000` | 6 MiB |
| storage | data | fat | `0xC80000` | remainder |

`CONFIG_PARTITION_TABLE_OFFSET` moves to `0x10000`, giving the bootloader a
57,344-byte window.

Three properties drive the arrangement.

The variable-size data partition is last, so both application slots keep
identical offsets on the configured 16 MB geometry and on the board's full 32 MB
part. Qualifying the upper half extends `storage` from 3.5 MiB to 19.5 MiB and
does not invalidate an image already deployed against the 16 MB table.

Slots are 6 MiB. A bare-metal RISC-V probe that forces full instantiation of the
emulator measures 540 KiB of code and read-only data for `rumiga-core`, `m68k`,
and `rumiga-platform`. Adding ESP-IDF with Wi-Fi, display, audio, storage, USB,
an HTTP stack, and the 195 KiB gzipped web bundle forecasts a feature-complete
image between 1.8 MiB and 3.8 MiB. A 4 MiB slot would leave the upper estimate at
95 percent occupancy, which is not a defensible ceiling for a slot that cannot be
enlarged.

Sizes leave no unallocated alignment gap. `coredump` is 108 KiB precisely so that
`ota_0` starts on a 64 KiB boundary, which application partitions require.

`esp-idf-sys` documents that a custom table declared in `sdkconfig.defaults` is
ignored by its generated CMake project. The layout is therefore applied when the
flashable image is generated, and the table the ESP-IDF build emits is a build
artifact rather than the shipped layout. `CONFIG_PARTITION_TABLE_SINGLE_APP_LARGE`
keeps that internal table larger than the current application so its own size
check does not fail before the real slot is full.

Secure Boot is reserved, not enabled. Building signed binaries requires a private
key, which must not enter the repository or the evidence bundle. The intended
production shape is an unsigned build plus offline signing against a protected
key, which belongs to the release process in M10.

## Consequences

The device gains update, rollback, crash-dump, and storage capability. Wi-Fi
credentials and configuration live in `nvs` with 80 sectors of headroom, and
`nvs_keys` is reserved so NVS encryption can be enabled without a layout change.

The evidence task verifies the merged image against `firmware/partitions.csv`
entry by entry rather than against the ESP-IDF table, checks that the bootloader
fits its window, and records the decoded layout in the build manifest. A unit
test asserts that the shipped layout is contiguous, aligned, fills the configured
geometry exactly, and declares two equal slots. A layout change is therefore
visible in the manifest diff and a regression fails a gate rather than a device.

`storage` is declared as FAT. The wear-levelling FAT stack is already in ESP-IDF
and is needed for the MicroSD, so the device keeps one filesystem stack and no new
third-party component. The cost is that FAT metadata is not crash consistent.

This is not a claim that any of it boots. Nothing has been flashed, and the
57,344-byte window is a reasoned choice rather than a measurement against a
signed bootloader.

## Alternatives

A 4 MiB slot was rejected because the upper forecast would occupy 95 percent of a
partition that cannot be enlarged after shipping.

Keeping the stock table and deferring the layout to board bring-up was rejected
because the bootloader window had 480 bytes of headroom, which is a live build
risk independent of OTA.

A third immutable `factory` slot as a recovery path was rejected for the 16 MB
geometry, where it would cost another 6 MiB and leave no room for `storage`. It
remains possible on the 32 MB variant.

LittleFS for `storage` was rejected for now because it is an external component
subject to the dependency and license review in `DEPENDENCY_POLICY.md`, while the
store is write-rarely and read-often and a damaged store is recoverable by
re-upload. It stays the better choice if crash consistency becomes a requirement.

Placing emulator media in flash was rejected. ADF and HDF images belong on the
MicroSD under the bounded block-device contract in M3. `storage` is intended for
device-side data such as Kickstart images the owner uploads, and the firmware
must never ship those.

## Evidence

`cargo +1.97.1 xtask ci --gate firmware` builds the image from
`firmware/partitions.csv`, verifies the embedded layout entry by entry, and
reports an application occupying 175,040 of 6,291,456 slot bytes with a
24,096-byte bootloader in a 57,344-byte window. `cargo +1.97.1 test --locked -p
rumiga-xtask` covers the layout parser, the subtype mapping, and the shipped
layout invariants.

## Supersession

None. This replaces the stock ESP-IDF table that ADR-0001's M0-008 evidence
recorded and complements the merged-image contract in M0-014.
