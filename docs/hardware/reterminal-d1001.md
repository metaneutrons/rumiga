# Seeed Studio reTerminal D1001

Hardware manifest for the product target. Every value names its source. Values taken
from the schematic are marked `[SCH]`, from the vendor wiki `[WIKI]`, from the BSP
repository `[BSP]`. Where a claim could not be resolved to a component, the manifest
says so rather than repeating it.

## Revision identity

| Field | Value | Source |
| --- | --- | --- |
| Board revision | Main Board **V1.0** | `[WIKI]` archive title `reTerminal D1001 Main Board V1.0 SCH & PCB` |
| Schematic revision | **V01** | `[SCH]` root sheet title block, `Rev: v01` |
| Schematic revision date | **2025-10-15** | `[SCH]` RevisionHistory sheet: `2025/10/15  V01  Initail release.` |
| Schematic sheet count | 13 | `[SCH]` sheet ids `1/13`–`13/13` |
| Schematic EDA tool | KiCad `10.0.0-19-g65df3ab11c` | `[SCH]` title block |
| Schematic licence | CC BY-SA 4.0 | `[SCH]` title block |
| Published file stamp | `260715` | `[WIKI]` filenames |

The file stamp is **not** the revision. `260715` is the publication stamp on the
downloadable archive and PDF, while the schematic's own revision history dates V01 to
2025-10-15. Recording only the filename would have implied a 2026 revision that the
document itself does not claim.

### Schematic sheets

`/` (root), `/Blockdiagram/`, `/PowerTreeDiagram/`, `/I2CTreeDiagram/`,
`/PowerManagement/`, `/MCU_ESP32-P4/`, `/WiFi_ESP32-C6/`, `/LCD&CAM/`, `/Audio/`,
`/USB&MicroSD/`, `/mPCIE&Lora/`, `/Misc/`, `/RevisionHistory/`. `[SCH]`

## BSP revision

| Field | Value |
| --- | --- |
| Repository | `Seeed-Studio/reTerminal-D1001` |
| Description | reTerminal D1001 esp32p4 bsp |
| Default branch | `main` |
| Commit | `5074d3b2f45626b261298e305aaf792036febc5a` |
| Commit date | 2026-04-17T07:35:48Z |
| Commit subject | `Merge pull request #2 from lwfl1111/main` |
| Tags | none published |

`[BSP]` read through the GitHub API. The repository publishes no tags, so a commit SHA
is the only stable reference available; a branch name would move.

## Silicon on the main board

Every part below appears in the V01 schematic. `[SCH]`

| Function | Part | Notes |
| --- | --- | --- |
| Application SoC | ESP32-P4 | The wiki names the exact variant `ESP32-P4NRW32` with 32 MB in-package PSRAM `[WIKI]`; the schematic symbol carries the family name only |
| External flash | `W25Q256JVEIQ` | 256 Mbit, so 32 MB, matching the wiki's "32MB QSPI Flash" `[WIKI]` |
| Wireless coprocessor | `ESP32-C6FH4` | Wi-Fi 6 2.4 GHz, Bluetooth 5 LE, 802.15.4 `[WIKI]` |
| Audio DAC and codec | `ES8311` | |
| Audio ADC | `ES7210` | Wiki adds echo cancellation `[WIKI]` |
| Speaker amplifier | `NS4150B` | 2 W into 8 Ω `[WIKI]` |
| I/O expander | `PCA9535RGER` | `U27`, I2C address **0x20** on `MISC_I2C`, provides `EXP_GPO0`–`EXP_GPO15` |
| Real-time clock | `PCF8563T` | |
| Inertial sensor | `LSM6DS3TR` | Six axes |
| Battery charger | `BQ25616` | 2500 mAh cell `[WIKI]` |
| Sub-GHz radio | `Wio-LR1121` module | On `/mPCIE&Lora/`, with SMA connector `J9`. Population on a shipped D1001 is **not** established: the wiki does not mention LoRa, and the schematic does not mark the footprint as unpopulated |

### Parts not on the main board

The panel, touch controller, and camera sensor are on modules reached through FPC
connectors, so they do not appear in the main board schematic. Their identities come
from the wiki and the module datasheets it links `[WIKI]`.

| Function | Part | Reached through |
| --- | --- | --- |
| Display panel | `GJX080C13-31BY`, 8 in, 800×1280, 250 cd/m² | `J8` (MIPI-DSI) |
| Display driver | `9365DA-H3` | `J8` |
| Touch controller | `GSL3670`, capacitive | `J3` |
| Camera sensor | `SC2356`, 1608×1208 active array, up to 30 fps at 1600×1200 | `J2` (MIPI-CSI) |
| Camera module | `ZD2481-D1001-V2.0` | `J2` |

## Connector inventory

Every connector in the V01 schematic, with its designator and manufacturer part. `[SCH]`

| Designator | Part | Function | Sheet |
| --- | --- | --- | --- |
| `USB1` | `USB-31C-F-16P-L7.35` | USB Type-C, 16 pin. Labelled "USB C JTAG/DEBUG" and also the 5 V power input | `/USB&MicroSD/` |
| `J1` | `ST-TF-003J` | micro-SD socket with card-detect switch (`SD_DETECT`) | `/USB&MicroSD/` |
| `J2` | `FPC0309-31RL-TAG` | 31 pin, 0.3 mm FPC, MIPI-CSI to the camera module | `/LCD&CAM/` |
| `J8` | `FPC0309-31RL-TAG` | 31 pin, 0.3 mm FPC, MIPI-DSI to the display module | `/LCD&CAM/` |
| `J3` | `ST-FPC-W052006-2H` | 6 pin FPC, touch panel (`TP_RST`, `INT_TP`) | `/LCD&CAM/` |
| `J4` | `ST-SIM-SP132` | SIM card socket for the cellular module | `/USB&MicroSD/` |
| `U21` | `ST-PC-002` | mini-PCIe socket, `PCIE_3V3`, `PCIE_WAKE_HOSTn`. The wiki states the slot carries 4G LTE over USB 2.0 `[WIKI]` | `/USB&MicroSD/` |
| `J5` | `Header 1x5 2.54 mm` | ESP32-C6 programming and boot header (`C6_CHIP_PU`, `C6_BOOT`) | `/MCU_ESP32-P4/` |
| `J6` | `1.25-2A-WT` | 2 pin speaker (`PA_OUTP`, `PA_OUTN`) | `/Audio/` |
| `J7` | `PH-3A-WT` | 3 pin battery, including `BAT_NTC` | `/PowerManagement/` |
| `J9` | `SMA-90D` | SMA antenna for the sub-GHz radio | `/mPCIE&Lora/` |

## Open questions

These are recorded because they would otherwise be discovered on a bench.

**No user expansion header appears in the schematic.** The wiki describes "flexible
expansion interfaces (GPIO, I2C, UART)" `[WIKI]`, but the V01 schematic's only 2.54 mm
header is `J5`, which carries the C6's programming and boot signals. The `EXP_GPO0`–
`EXP_GPO15` net names belong to the `PCA9535RGER` expander and drive on-board functions
such as `LCD_PWR_EN`, `LCD_RST`, `TP_RST`, and `EN_PA`; despite the sheet label "GPO
Expanssion" they are internal fan-out, not a connector. Whether a shipped unit exposes
user I/O, and through which physical part, is unresolved.

**LoRa population is unknown.** The `/mPCIE&Lora/` sheet carries a `Wio-LR1121` module
and `J9`. The wiki does not mention LoRa for the D1001, and the schematic marks no
footprint as unpopulated, so the schematic alone cannot say whether a shipped board has
it.

**Board dimensions are not published.** Neither the wiki nor the product page states
main board or enclosure dimensions in text. The 3D model `D1001_asm.stp` `[WIKI]` would
answer this and has not been opened.

**The ESP32-P4 variant is a wiki claim, not a schematic one.** The schematic symbol
carries the family name; `ESP32-P4NRW32` and its 32 MB in-package PSRAM come from the
wiki. The distinction matters because the PSRAM size is the constraint the memory budget
in `ARCHITECTURE.md` rests on.

**Nothing here is verified against a physical board.** Every value is from a document.
No unit has been powered, and no connector has been probed.

## Sources

| Tag | Source |
| --- | --- |
| `[SCH]` | `reTerminal_D1001_260715.pdf`, sha256 `c488b1aeac3e8fd72d08f65aeb23e8c1143f0b36e2997024ddee252daa4a292c`, from `https://files.seeedstudio.com/wiki/reTerminal_d10xx/res/reTerminal_D1001_260715.pdf` |
| `[WIKI]` | `https://wiki.seeedstudio.com/getting_started_with_reterminal_d1001/` and `https://wiki.seeedstudio.com/reterminal_d10xx_main_page/` |
| `[BSP]` | `https://github.com/Seeed-Studio/reTerminal-D1001` at commit `5074d3b2f45626b261298e305aaf792036febc5a` |

The schematic checksum is recorded so a later reader can tell whether the document they
have is the one this manifest was written from. Seeed publishes the file without a
checksum, so this one was computed on download and is not a vendor attestation.

Further vendor documents exist and were not read for this manifest: the SCH and PCB
source archive, the ESP32-P4NRW32 and ESP32-C6 datasheets, the panel, IMU, and camera
datasheets, and the 3D model.
