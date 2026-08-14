#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

rom="${RUMIGA_A1200_ROM:-assets/kick.a1200.47.102.rom}"
adf="${RUMIGA_A1200_WORKBENCH314_ADF:-assets/wb314/Workbench3_1_4.adf}"
cpu="${RUMIGA_A1200_CPU:-68020}"
frames="${RUMIGA_CAPTURE_FRAMES:-8000}"
out_dir="${RUMIGA_EVIDENCE_DIR:-target/evidence/a1200-workbench314-adf}"
capture_native="${RUMIGA_CAPTURE_NATIVE:-1}"
presentation_png="$out_dir/rumiga.png"
presentation_manifest="$out_dir/rumiga.json"
native_png="$out_dir/rumiga-native.png"
native_manifest="$out_dir/rumiga-native.json"
notes="$out_dir/notes.md"

if [[ ! -f "$rom" ]]; then
  echo "Missing A1200 ROM: $rom" >&2
  echo "Set RUMIGA_A1200_ROM=/path/to/kick.a1200.47.102.rom" >&2
  exit 2
fi

if [[ ! -f "$adf" ]]; then
  echo "Missing A1200 Workbench 3.1.4 ADF: $adf" >&2
  echo "Set RUMIGA_A1200_WORKBENCH314_ADF=/path/to/Workbench3_1_4.adf" >&2
  exit 2
fi

mkdir -p "$out_dir"

run_capture() {
  local kind="$1"
  local png_path="$2"
  local manifest_path="$3"

  cargo run --locked --release -p rumiga-desktop --bin rumiga-desktop -- \
    --model a1200 \
    --cpu "$cpu" \
    --capture "$png_path" \
    --capture-manifest "$manifest_path" \
    --capture-frames "$frames" \
    --capture-kind "$kind" \
    "$rom" \
    "$adf"
}

run_capture viewport-presentation "$presentation_png" "$presentation_manifest"

native_manifest_arg=""
capture_native_normalized="$(printf '%s' "$capture_native" | tr '[:upper:]' '[:lower:]')"
case "$capture_native_normalized" in
  0|false|no|off)
    ;;
  *)
    run_capture native-framebuffer "$native_png" "$native_manifest"
    native_manifest_arg="$native_manifest"
    ;;
esac

python3 - "$presentation_manifest" "$notes" "$native_manifest_arg" <<'PY'
import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
notes_path = Path(sys.argv[2])
native_manifest_path = Path(sys.argv[3]) if len(sys.argv) > 3 and sys.argv[3] else None
data = json.loads(manifest_path.read_text())
native_data = (
    json.loads(native_manifest_path.read_text())
    if native_manifest_path is not None and native_manifest_path.exists()
    else None
)

schema = data.get("schema", {})
producer = data.get("producer", {})
run = data.get("run", {})
media = data.get("media", {})
floppy = data.get("floppy", {})
edge = data.get("edge_integrity", {})
viewport = data.get("viewport", {})
presentation = data.get("presentation", {})
framebuffer = data.get("framebuffer", {})
boot_workarounds = data.get("boot_workarounds", {})
native_viewport = native_data.get("viewport", {}) if isinstance(native_data, dict) else {}
native_presentation = native_data.get("presentation", {}) if isinstance(native_data, dict) else {}
native_framebuffer = native_data.get("framebuffer", {}) if isinstance(native_data, dict) else {}

df0 = media.get("df0") if isinstance(media.get("df0"), dict) else {}
df0_drive = {}
for drive in floppy.get("drives", []):
    if isinstance(drive, dict) and drive.get("name") == "DF0":
        df0_drive = drive
        break

edge_regression_pixels = sum(
    int(edge.get(name) or 0)
    for name in (
        "mirrored_non_background_pixels",
        "right_edge_wrapped_to_left_pixels",
        "left_edge_wrapped_to_right_pixels",
    )
)
classification = (
    "workbench314-adf-edge-clean"
    if edge_regression_pixels == 0 and df0
    else "workbench314-adf-regression-candidate"
)

print(f"manifest={manifest_path}")
if native_manifest_path is not None:
    print(f"native_manifest={native_manifest_path}")
print(f"notes={notes_path}")
print(f"schema={schema.get('id')}@{schema.get('version')}")
print(f"git={producer.get('git_sha')} dirty={producer.get('git_dirty')}")
print(f"frames={run.get('frames')} stopped={run.get('stopped')}")
print(
    "df0="
    f"path={df0.get('path')}"
    f" bytes={df0.get('bytes')}"
    f" dirty={df0_drive.get('dirty')}"
    f" cylinder={df0_drive.get('cylinder')}"
)
print(
    "viewport="
    f"preset={viewport.get('preset')} "
    f"{viewport.get('source_width')}x{viewport.get('source_height')}"
    f" -> {viewport.get('output_width')}x{viewport.get('output_height')}"
    f" stretch={viewport.get('vertical_stretch')}"
    f" scaling={presentation.get('scaling')}"
    f" kind={presentation.get('capture_kind')}"
)
if isinstance(native_data, dict):
    print(
        "native_capture="
        f"{native_viewport.get('output_width')}x{native_viewport.get('output_height')}"
        f" kind={native_presentation.get('capture_kind')}"
        f" colors={native_framebuffer.get('distinct_colors')}"
        f" changed={native_framebuffer.get('pixels_different_from_background')}"
    )
else:
    print("native_capture=disabled")
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
    f" right_to_left={edge.get('right_edge_wrapped_to_left_pixels')}"
    f" left_to_right={edge.get('left_edge_wrapped_to_right_pixels')}"
    f" content_width={edge.get('min_content_width')}..{edge.get('max_content_width')}"
)
print(
    "boot_workarounds="
    f"forced_cia_timer_start={boot_workarounds.get('forced_cia_timer_start')}"
    f" forced_cia_timer_start_count={boot_workarounds.get('forced_cia_timer_start_count')}"
    f" rom_drive_step_patch={boot_workarounds.get('rom_drive_step_patch')}"
)
print(f"classification={classification}")

notes_path.write_text(
    "\n".join(
        [
            "# A1200 Workbench 3.1.4 ADF Evidence",
            "",
            f"- Presentation manifest: `{manifest_path.name}`",
            "- Presentation screenshot: `rumiga.png`",
            (
                f"- Native manifest: `{native_manifest_path.name}`"
                if native_manifest_path is not None
                else "- Native manifest: `disabled`"
            ),
            (
                "- Native screenshot: `rumiga-native.png`"
                if native_manifest_path is not None
                else "- Native screenshot: `disabled`"
            ),
            f"- Schema: `{schema.get('id')}@{schema.get('version')}`",
            f"- Git: `{producer.get('git_sha')}` dirty=`{producer.get('git_dirty')}`",
            f"- Frames: `{run.get('frames')}` stopped=`{run.get('stopped')}`",
            (
                "- DF0: "
                f"path=`{df0.get('path')}` "
                f"bytes=`{df0.get('bytes')}` "
                f"dirty=`{df0_drive.get('dirty')}` "
                f"cylinder=`{df0_drive.get('cylinder')}`"
            ),
            (
                "- Viewport: "
                f"preset=`{viewport.get('preset')}` "
                f"`{viewport.get('source_width')}x{viewport.get('source_height')}`"
                f" -> `{viewport.get('output_width')}x{viewport.get('output_height')}` "
                f"stretch=`{viewport.get('vertical_stretch')}` "
                f"scaling=`{presentation.get('scaling')}` "
                f"kind=`{presentation.get('capture_kind')}`"
            ),
            (
                "- Native capture: "
                f"`{native_viewport.get('output_width')}x{native_viewport.get('output_height')}` "
                f"kind=`{native_presentation.get('capture_kind')}` "
                f"colors=`{native_framebuffer.get('distinct_colors')}` "
                f"changed=`{native_framebuffer.get('pixels_different_from_background')}`"
                if isinstance(native_data, dict)
                else "- Native capture: `disabled`"
            ),
            (
                "- Edge integrity: "
                f"left=`{edge.get('left_non_background_pixels')}` "
                f"right=`{edge.get('right_non_background_pixels')}` "
                f"mirrored=`{edge.get('mirrored_non_background_pixels')}` "
                f"right_to_left=`{edge.get('right_edge_wrapped_to_left_pixels')}` "
                f"left_to_right=`{edge.get('left_edge_wrapped_to_right_pixels')}` "
                f"content_width=`{edge.get('min_content_width')}..{edge.get('max_content_width')}`"
            ),
            (
                "- Boot workarounds: "
                f"forced_cia_timer_start=`{boot_workarounds.get('forced_cia_timer_start')}` "
                f"forced_cia_timer_start_count=`{boot_workarounds.get('forced_cia_timer_start_count')}` "
                f"rom_drive_step_patch=`{boot_workarounds.get('rom_drive_step_patch')}`"
            ),
            f"- Classification: `{classification}`",
            "",
            "A requester for `LIBS/icon.library` or a similar Workbench library is",
            "treated as a media-set/install-state outcome for this ADF set, not as a",
            "trackdisk or viewport failure.",
            "",
        ]
    )
)
PY
