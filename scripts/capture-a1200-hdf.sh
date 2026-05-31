#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

rom="${RUMIGA_A1200_ROM:-assets/kick.a1200.47.102.rom}"
hdf="${RUMIGA_A1200_HDF:-assets/workbench-39.hdf}"
cpu="${RUMIGA_A1200_CPU:-68020}"
frames="${RUMIGA_CAPTURE_FRAMES:-4000}"
out_dir="${RUMIGA_EVIDENCE_DIR:-target/evidence/a1200-hdf}"
png="$out_dir/rumiga.png"
manifest="$out_dir/rumiga.json"
notes="$out_dir/notes.md"

if [[ ! -f "$rom" ]]; then
  echo "Missing A1200 ROM: $rom" >&2
  echo "Set RUMIGA_A1200_ROM=/path/to/kick.a1200.47.102.rom" >&2
  exit 2
fi

if [[ ! -f "$hdf" ]]; then
  echo "Missing A1200 HDF: $hdf" >&2
  echo "Set RUMIGA_A1200_HDF=/path/to/workbench.hdf" >&2
  exit 2
fi

mkdir -p "$out_dir"

cargo run --release -p rumiga-desktop --bin rumiga-desktop -- \
  --model a1200 \
  --cpu "$cpu" \
  --hdf "$hdf" \
  --capture "$png" \
  --capture-manifest "$manifest" \
  --capture-frames "$frames" \
  "$rom"

python3 - "$manifest" "$notes" <<'PY'
import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
notes_path = Path(sys.argv[2])
data = json.loads(manifest_path.read_text())
edge = data.get("edge_integrity", {})
viewport = data.get("viewport", {})
framebuffer = data.get("framebuffer", {})
producer = data.get("producer", {})
run = data.get("run", {})
schema = data.get("schema", {})
boot_workarounds = data.get("boot_workarounds", {})
cia = data.get("cia", {})
classification = (
    "viewport-edge-clean"
    if edge.get("mirrored_non_background_pixels", 1) == 0
    else "viewport-edge-regression-candidate"
)

print(f"manifest={manifest_path}")
print(f"notes={notes_path}")
print(f"schema={schema.get('id')}@{schema.get('version')}")
print(f"git={producer.get('git_sha')} dirty={producer.get('git_dirty')}")
print(f"frames={run.get('frames')} stopped={run.get('stopped')}")
print(
    "viewport="
    f"preset={viewport.get('preset')} "
    f"{viewport.get('source_width')}x{viewport.get('source_height')}"
    f" -> {viewport.get('output_width')}x{viewport.get('output_height')}"
    f" stretch={viewport.get('vertical_stretch')}"
)
print(
    "framebuffer="
    f"colors={framebuffer.get('distinct_colors')}"
    f" changed={framebuffer.get('pixels_different_from_background')}"
)
print(
    "edge_integrity="
    f"left={edge.get('left_non_background_pixels')}"
    f" right={edge.get('right_non_background_pixels')}"
    f" mirrored={edge.get('mirrored_non_background_pixels')}"
)
print(
    "boot_workarounds="
    f"forced_cia_timer_start={boot_workarounds.get('forced_cia_timer_start')}"
    f" forced_cia_timer_start_count={boot_workarounds.get('forced_cia_timer_start_count')}"
    f" rom_drive_step_patch={boot_workarounds.get('rom_drive_step_patch')}"
)
for cia_name in ("a", "b"):
    timer_a = cia.get(cia_name, {}).get("timer_a", {})
    timer_b = cia.get(cia_name, {}).get("timer_b", {})
    print(
        f"cia_{cia_name}="
        f"ta_ctrl={timer_a.get('control')}"
        f" ta_start={timer_a.get('start_writes')}"
        f" ta_underflows={timer_a.get('underflows')}"
        f" tb_ctrl={timer_b.get('control')}"
        f" tb_start={timer_b.get('start_writes')}"
        f" tb_underflows={timer_b.get('underflows')}"
    )
print(f"classification={classification}")

notes_path.write_text(
    "\n".join(
        [
            "# A1200 Workbench HDF Evidence",
            "",
            f"- Manifest: `{manifest_path.name}`",
            "- Screenshot: `rumiga.png`",
            f"- Schema: `{schema.get('id')}@{schema.get('version')}`",
            f"- Git: `{producer.get('git_sha')}` dirty=`{producer.get('git_dirty')}`",
            f"- Frames: `{run.get('frames')}` stopped=`{run.get('stopped')}`",
            (
                "- Viewport: "
                f"preset=`{viewport.get('preset')}` "
                f"`{viewport.get('source_width')}x{viewport.get('source_height')}`"
                f" -> `{viewport.get('output_width')}x{viewport.get('output_height')}`"
                f" stretch=`{viewport.get('vertical_stretch')}`"
            ),
            (
                "- Framebuffer: "
                f"colors=`{framebuffer.get('distinct_colors')}` "
                f"changed=`{framebuffer.get('pixels_different_from_background')}`"
            ),
            (
                "- Edge integrity: "
                f"left=`{edge.get('left_non_background_pixels')}` "
                f"right=`{edge.get('right_non_background_pixels')}` "
                f"mirrored=`{edge.get('mirrored_non_background_pixels')}`"
            ),
            (
                "- Boot workarounds: "
                f"forced_cia_timer_start=`{boot_workarounds.get('forced_cia_timer_start')}` "
                f"forced_cia_timer_start_count=`{boot_workarounds.get('forced_cia_timer_start_count')}` "
                f"rom_drive_step_patch=`{boot_workarounds.get('rom_drive_step_patch')}`"
            ),
            (
                "- CIA-A timers: "
                f"ta_start=`{cia.get('a', {}).get('timer_a', {}).get('start_writes')}` "
                f"ta_underflows=`{cia.get('a', {}).get('timer_a', {}).get('underflows')}` "
                f"tb_start=`{cia.get('a', {}).get('timer_b', {}).get('start_writes')}` "
                f"tb_underflows=`{cia.get('a', {}).get('timer_b', {}).get('underflows')}`"
            ),
            (
                "- CIA-B timers: "
                f"ta_start=`{cia.get('b', {}).get('timer_a', {}).get('start_writes')}` "
                f"ta_underflows=`{cia.get('b', {}).get('timer_a', {}).get('underflows')}` "
                f"tb_start=`{cia.get('b', {}).get('timer_b', {}).get('start_writes')}` "
                f"tb_underflows=`{cia.get('b', {}).get('timer_b', {}).get('underflows')}`"
            ),
            f"- Classification: `{classification}`",
            "",
            "A missing `LIBS/workbench.library` requester is treated as a media or",
            "install-state result for this HDF, not as a Gayle/IDE boot failure.",
            "",
        ]
    )
)
PY
