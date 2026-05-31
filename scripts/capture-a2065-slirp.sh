#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

rom="${RUMIGA_NETWORK_ROM:-${RUMIGA_A1200_ROM:-assets/kick.a1200.47.102.rom}}"
hdf="${RUMIGA_NETWORK_HDF:-${RUMIGA_A1200_HDF:-assets/workbench-39.hdf}}"
hdf_snapshot="${RUMIGA_HDF_SNAPSHOT:-}"
cpu="${RUMIGA_NETWORK_CPU:-68020}"
mac="${RUMIGA_NETWORK_MAC:-00:80:10:4d:49:47}"
frames="${RUMIGA_CAPTURE_FRAMES:-4000}"
mode="${RUMIGA_NETWORK_EVIDENCE_MODE:-link}"
expect_configured="${RUMIGA_NETWORK_EXPECT_A2065_CONFIGURED:-0}"
expect_tx_min="${RUMIGA_NETWORK_EXPECT_TX_MIN:-0}"
expect_rx_min="${RUMIGA_NETWORK_EXPECT_RX_MIN:-0}"
out_dir="${RUMIGA_EVIDENCE_DIR:-target/evidence/a2065-slirp}"
png="$out_dir/rumiga.png"
manifest="$out_dir/rumiga.json"
notes="$out_dir/notes.md"
pcap="${RUMIGA_NETWORK_PCAP:-$out_dir/rumiga.pcap}"

if [[ ! -f "$rom" ]]; then
  echo "Missing A1200 ROM: $rom" >&2
  echo "Set RUMIGA_NETWORK_ROM=/path/to/kick.a1200.47.102.rom" >&2
  exit 2
fi

if [[ ! -f "$hdf" ]]; then
  echo "Missing network evidence HDF: $hdf" >&2
  echo "Set RUMIGA_NETWORK_HDF=/path/to/workbench-with-network-stack.hdf" >&2
  exit 2
fi

mkdir -p "$out_dir"

cmd=(
  cargo run --release -p rumiga-desktop --bin rumiga-desktop --
  --model a1200
  --cpu "$cpu"
  --network-slirp
  --network-mac "$mac"
  --network-pcap "$pcap"
  --hdf "$hdf"
  --capture "$png"
  --capture-manifest "$manifest"
  --capture-frames "$frames"
)
if [[ -n "$hdf_snapshot" ]]; then
  cmd+=(--hdf-snapshot "$hdf_snapshot")
fi
cmd+=("$rom")
"${cmd[@]}"

python3 - "$manifest" "$notes" "$mode" "$expect_configured" "$expect_tx_min" "$expect_rx_min" <<'PY'
import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
notes_path = Path(sys.argv[2])
mode = sys.argv[3]
expect_configured = sys.argv[4] not in ("0", "false", "False", "no", "No", "")
expect_tx_min = int(sys.argv[5])
expect_rx_min = int(sys.argv[6])

data = json.loads(manifest_path.read_text())
schema = data.get("schema", {})
producer = data.get("producer", {})
run = data.get("run", {})
network = data.get("network", {})
hdf = data.get("gayle_ide", {})
hdf_snapshot = hdf.get("hdf_snapshot")
pcap = network.get("pcap")

enabled = bool(network.get("enabled"))
backend = network.get("backend")
device = network.get("device")
link_up = bool(network.get("link_up"))
present = bool(network.get("a2065_present"))
configured = bool(network.get("a2065_configured"))
base_address = network.get("a2065_base_address")
tx_packets = int(network.get("tx_packets") or 0)
rx_packets = int(network.get("rx_packets") or 0)
dropped_packets = int(network.get("dropped_packets") or 0)

failures = []
if not enabled:
    failures.append("network backend is disabled")
if backend != "slirp":
    failures.append(f"backend is {backend!r}, expected 'slirp'")
if device != "a2065":
    failures.append(f"device is {device!r}, expected 'a2065'")
if not present:
    failures.append("A2065 device is not present")
if not link_up:
    failures.append("SLIRP link is down")
if expect_configured and not configured:
    failures.append("A2065 was not autoconfigured by the guest")
if tx_packets < expect_tx_min:
    failures.append(f"TX packets {tx_packets} < expected {expect_tx_min}")
if rx_packets < expect_rx_min:
    failures.append(f"RX packets {rx_packets} < expected {expect_rx_min}")

if failures:
    classification = "a2065-slirp-fail"
elif mode == "guest-tcp" and configured and tx_packets >= expect_tx_min and rx_packets >= expect_rx_min:
    classification = "guest-tcp-evidence"
elif configured:
    classification = "a2065-configured-link-ready"
else:
    classification = "a2065-link-ready-awaiting-guest-driver"

print(f"manifest={manifest_path}")
print(f"notes={notes_path}")
print(f"schema={schema.get('id')}@{schema.get('version')}")
print(f"git={producer.get('git_sha')} dirty={producer.get('git_dirty')}")
print(f"frames={run.get('frames')} stopped={run.get('stopped')}")
print(
    "network="
    f"enabled={enabled}"
    f" device={device}"
    f" backend={backend}"
    f" link_up={link_up}"
    f" present={present}"
    f" configured={configured}"
    f" base={base_address}"
    f" tx={tx_packets}"
    f" rx={rx_packets}"
    f" dropped={dropped_packets}"
    f" pcap={pcap}"
)
print(
    "hdf_geometry="
    f"source={hdf.get('geometry_source')}"
    f" rdb_detected={hdf.get('rdb', {}).get('detected')}"
    f" rdb_usable={hdf.get('rdb', {}).get('usable')}"
)
if isinstance(hdf_snapshot, dict):
    print(
        "hdf_snapshot="
        f"path={hdf_snapshot.get('path')}"
        f" dirty={hdf_snapshot.get('dirty')}"
        f" changed_bytes={hdf_snapshot.get('changed_bytes')}"
        f" changed_sectors={hdf_snapshot.get('changed_sectors')}"
    )
print(f"classification={classification}")

notes = [
    "# A2065 SLIRP Evidence",
    "",
    f"- Manifest: `{manifest_path.name}`",
    "- Screenshot: `rumiga.png`",
    f"- PCAP: `{Path(pcap).name if pcap else None}`",
    f"- Schema: `{schema.get('id')}@{schema.get('version')}`",
    f"- Git: `{producer.get('git_sha')}` dirty=`{producer.get('git_dirty')}`",
    f"- Frames: `{run.get('frames')}` stopped=`{run.get('stopped')}`",
    f"- Mode: `{mode}`",
    (
        "- Network: "
        f"enabled=`{enabled}` "
        f"device=`{device}` "
        f"backend=`{backend}` "
        f"link_up=`{link_up}` "
        f"present=`{present}` "
        f"configured=`{configured}` "
        f"base=`{base_address}` "
        f"pcap=`{pcap}`"
    ),
    (
        "- Packet counters: "
        f"tx=`{tx_packets}` "
        f"rx=`{rx_packets}` "
        f"dropped=`{dropped_packets}`"
    ),
    (
        "- HDF geometry: "
        f"source=`{hdf.get('geometry_source')}` "
        f"rdb_detected=`{hdf.get('rdb', {}).get('detected')}` "
        f"rdb_usable=`{hdf.get('rdb', {}).get('usable')}`"
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
    f"- Classification: `{classification}`",
    "",
    "Guest TCP proof requires a user-provided HDF with an A2065/SANA-II driver",
    "and TCP/IP stack configured to run the proof workload during boot.",
    "",
    "WinUAE/FS-UAE-style SLIRP defaults for static configuration:",
    "",
    "- IP: `10.0.2.15`",
    "- Gateway: `10.0.2.2`",
    "- DNS: `10.0.2.3`",
    "- Netmask: `255.255.255.0`",
    "",
    "For strict guest-side proof, run this scenario with:",
    "",
    "```sh",
    "RUMIGA_NETWORK_EVIDENCE_MODE=guest-tcp \\",
    "RUMIGA_NETWORK_EXPECT_A2065_CONFIGURED=1 \\",
    "RUMIGA_NETWORK_EXPECT_TX_MIN=1 \\",
    "RUMIGA_NETWORK_EXPECT_RX_MIN=1 \\",
    "scripts/capture-a2065-slirp.sh",
    "```",
    "",
]

if failures:
    notes.extend(["Failures:", ""])
    notes.extend(f"- {failure}" for failure in failures)
    notes.append("")

notes_path.write_text("\n".join(notes))

if failures:
    sys.exit(3)
PY
