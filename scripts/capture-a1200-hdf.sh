#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

rom="${RUMIGA_A1200_ROM:-assets/kick.a1200.47.102.rom}"
hdf="${RUMIGA_A1200_HDF:-assets/workbench-39.hdf}"
hdf_snapshot="${RUMIGA_HDF_SNAPSHOT:-}"
cpu="${RUMIGA_A1200_CPU:-68020}"
frames="${RUMIGA_CAPTURE_FRAMES:-4000}"
out_dir="${RUMIGA_EVIDENCE_DIR:-target/evidence/a1200-hdf}"
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

if [[ ! -f "$hdf" ]]; then
  echo "Missing A1200 HDF: $hdf" >&2
  echo "Set RUMIGA_A1200_HDF=/path/to/workbench.hdf" >&2
  exit 2
fi

mkdir -p "$out_dir"

run_capture() {
  local kind="$1"
  local png_path="$2"
  local manifest_path="$3"
  local include_hdf_snapshot="$4"

  local cmd=(
    cargo run --release -p rumiga-desktop --bin rumiga-desktop --
    --model a1200
    --cpu "$cpu"
    --hdf "$hdf"
    --capture "$png_path"
    --capture-manifest "$manifest_path"
    --capture-frames "$frames"
    --capture-kind "$kind"
  )
  if [[ "$include_hdf_snapshot" == "1" && -n "$hdf_snapshot" ]]; then
    cmd+=(--hdf-snapshot "$hdf_snapshot")
  fi
  cmd+=("$rom")
  "${cmd[@]}"
}

run_capture viewport-presentation "$presentation_png" "$presentation_manifest" 1

native_manifest_arg=""
capture_native_normalized="$(printf '%s' "$capture_native" | tr '[:upper:]' '[:lower:]')"
case "$capture_native_normalized" in
  0|false|no|off)
    ;;
  *)
    run_capture native-framebuffer "$native_png" "$native_manifest" 0
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
edge = data.get("edge_integrity", {})
viewport = data.get("viewport", {})
presentation = data.get("presentation", {})
framebuffer = data.get("framebuffer", {})
native_viewport = native_data.get("viewport", {}) if isinstance(native_data, dict) else {}
native_presentation = native_data.get("presentation", {}) if isinstance(native_data, dict) else {}
native_framebuffer = native_data.get("framebuffer", {}) if isinstance(native_data, dict) else {}
producer = data.get("producer", {})
run = data.get("run", {})
schema = data.get("schema", {})
boot_workarounds = data.get("boot_workarounds", {})
cia = data.get("cia", {})
gayle = data.get("gayle_ide", {})
hdf_snapshot = gayle.get("hdf_snapshot")
network = data.get("network", {})
edge_regression_pixels = sum(
    int(edge.get(name) or 0)
    for name in (
        "mirrored_non_background_pixels",
        "right_edge_wrapped_to_left_pixels",
        "left_edge_wrapped_to_right_pixels",
    )
)
classification = (
    "viewport-edge-clean"
    if edge_regression_pixels == 0
    else "viewport-edge-regression-candidate"
)

print(f"manifest={manifest_path}")
if native_manifest_path is not None:
    print(f"native_manifest={native_manifest_path}")
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
print(
    "hdf_policy="
    f"{gayle.get('hdf_write_policy')}"
    f" host_writeback={gayle.get('host_writeback_enabled')}"
    f" dirty={gayle.get('hdf_dirty')}"
)
if isinstance(hdf_snapshot, dict):
    print(
        "hdf_snapshot="
        f"path={hdf_snapshot.get('path')}"
        f" dirty={hdf_snapshot.get('dirty')}"
        f" changed_bytes={hdf_snapshot.get('changed_bytes')}"
        f" changed_sectors={hdf_snapshot.get('changed_sectors')}"
    )
print(
    "hdf_geometry="
    f"source={gayle.get('geometry_source')}"
    f" chs={gayle.get('cylinders')}/{gayle.get('heads')}/{gayle.get('sectors_per_track')}"
    f" rdb_detected={gayle.get('rdb', {}).get('detected')}"
    f" rdb_usable={gayle.get('rdb', {}).get('usable')}"
    f" rdb_checksum_valid={gayle.get('rdb', {}).get('checksum_valid')}"
)
print(
    "network="
    f"enabled={network.get('enabled')}"
    f" device={network.get('device')}"
    f" backend={network.get('backend')}"
    f" mac={network.get('mac_address')}"
)
for cia_name in ("a", "b"):
    timer_a = cia.get(cia_name, {}).get("timer_a", {})
    timer_b = cia.get(cia_name, {}).get("timer_b", {})
    register_writes = cia.get(cia_name, {}).get("register_writes", {})
    cra_writes = register_writes.get("cra", {})
    crb_writes = register_writes.get("crb", {})
    icr_writes = register_writes.get("icr", {})
    print(
        f"cia_{cia_name}="
        f"ta_ctrl={timer_a.get('control')}"
        f" ta_start={timer_a.get('start_writes')}"
        f" ta_auto_start={timer_a.get('auto_start_writes')}"
        f" ta_underflows={timer_a.get('underflows')}"
        f" tb_ctrl={timer_b.get('control')}"
        f" tb_start={timer_b.get('start_writes')}"
        f" tb_auto_start={timer_b.get('auto_start_writes')}"
        f" tb_underflows={timer_b.get('underflows')}"
        f" cra_writes={cra_writes.get('count')}"
        f" cra_last={cra_writes.get('last')}"
        f" crb_writes={crb_writes.get('count')}"
        f" crb_last={crb_writes.get('last')}"
        f" icr_writes={icr_writes.get('count')}"
        f" icr_last={icr_writes.get('last')}"
    )
print(f"classification={classification}")

notes_path.write_text(
    "\n".join(
        [
            "# A1200 Workbench HDF Evidence",
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
                "- Viewport: "
                f"preset=`{viewport.get('preset')}` "
                f"`{viewport.get('source_width')}x{viewport.get('source_height')}`"
                f" -> `{viewport.get('output_width')}x{viewport.get('output_height')}`"
                f" stretch=`{viewport.get('vertical_stretch')}`"
                f" scaling=`{presentation.get('scaling')}`"
                f" kind=`{presentation.get('capture_kind')}`"
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
                "- Framebuffer: "
                f"colors=`{framebuffer.get('distinct_colors')}` "
                f"changed=`{framebuffer.get('pixels_different_from_background')}`"
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
            (
                "- HDF policy: "
                f"policy=`{gayle.get('hdf_write_policy')}` "
                f"host_writeback=`{gayle.get('host_writeback_enabled')}` "
                f"dirty=`{gayle.get('hdf_dirty')}`"
            ),
            (
                "- HDF snapshot: "
                f"path=`{hdf_snapshot.get('path')}` "
                f"dirty=`{hdf_snapshot.get('dirty')}` "
                f"changed_bytes=`{hdf_snapshot.get('changed_bytes')}` "
                f"changed_sectors=`{hdf_snapshot.get('changed_sectors')}`"
                if isinstance(hdf_snapshot, dict)
                else "- HDF snapshot: `none`"
            ),
            (
                "- HDF geometry: "
                f"source=`{gayle.get('geometry_source')}` "
                f"chs=`{gayle.get('cylinders')}/{gayle.get('heads')}/{gayle.get('sectors_per_track')}` "
                f"rdb_detected=`{gayle.get('rdb', {}).get('detected')}` "
                f"rdb_usable=`{gayle.get('rdb', {}).get('usable')}` "
                f"rdb_checksum_valid=`{gayle.get('rdb', {}).get('checksum_valid')}` "
                f"rdb_declared_bytes=`{gayle.get('rdb', {}).get('declared_bytes')}` "
                f"rdb_fits_image=`{gayle.get('rdb', {}).get('fits_in_image')}`"
            ),
            (
                "- Network: "
                f"enabled=`{network.get('enabled')}` "
                f"device=`{network.get('device')}` "
                f"backend=`{network.get('backend')}` "
                f"mac=`{network.get('mac_address')}`"
            ),
            (
                "- CIA-A timers: "
                f"ta_start=`{cia.get('a', {}).get('timer_a', {}).get('start_writes')}` "
                f"ta_auto_start=`{cia.get('a', {}).get('timer_a', {}).get('auto_start_writes')}` "
                f"ta_underflows=`{cia.get('a', {}).get('timer_a', {}).get('underflows')}` "
                f"tb_start=`{cia.get('a', {}).get('timer_b', {}).get('start_writes')}` "
                f"tb_auto_start=`{cia.get('a', {}).get('timer_b', {}).get('auto_start_writes')}` "
                f"tb_underflows=`{cia.get('a', {}).get('timer_b', {}).get('underflows')}`"
            ),
            (
                "- CIA-A register writes: "
                f"cra=`{cia.get('a', {}).get('register_writes', {}).get('cra', {}).get('count')}` "
                f"cra_last=`{cia.get('a', {}).get('register_writes', {}).get('cra', {}).get('last')}` "
                f"crb=`{cia.get('a', {}).get('register_writes', {}).get('crb', {}).get('count')}` "
                f"crb_last=`{cia.get('a', {}).get('register_writes', {}).get('crb', {}).get('last')}` "
                f"icr=`{cia.get('a', {}).get('register_writes', {}).get('icr', {}).get('count')}` "
                f"icr_last=`{cia.get('a', {}).get('register_writes', {}).get('icr', {}).get('last')}`"
            ),
            (
                "- CIA-B timers: "
                f"ta_start=`{cia.get('b', {}).get('timer_a', {}).get('start_writes')}` "
                f"ta_auto_start=`{cia.get('b', {}).get('timer_a', {}).get('auto_start_writes')}` "
                f"ta_underflows=`{cia.get('b', {}).get('timer_a', {}).get('underflows')}` "
                f"tb_start=`{cia.get('b', {}).get('timer_b', {}).get('start_writes')}` "
                f"tb_auto_start=`{cia.get('b', {}).get('timer_b', {}).get('auto_start_writes')}` "
                f"tb_underflows=`{cia.get('b', {}).get('timer_b', {}).get('underflows')}`"
            ),
            (
                "- CIA-B register writes: "
                f"cra=`{cia.get('b', {}).get('register_writes', {}).get('cra', {}).get('count')}` "
                f"cra_last=`{cia.get('b', {}).get('register_writes', {}).get('cra', {}).get('last')}` "
                f"crb=`{cia.get('b', {}).get('register_writes', {}).get('crb', {}).get('count')}` "
                f"crb_last=`{cia.get('b', {}).get('register_writes', {}).get('crb', {}).get('last')}` "
                f"icr=`{cia.get('b', {}).get('register_writes', {}).get('icr', {}).get('count')}` "
                f"icr_last=`{cia.get('b', {}).get('register_writes', {}).get('icr', {}).get('last')}`"
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
