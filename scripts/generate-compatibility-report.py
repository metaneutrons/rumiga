#!/usr/bin/env python3
"""Generate a markdown compatibility report from Rumiga evidence manifests."""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


EXPECTED_SCHEMA_ID = "rumiga.capture.v1"
EXPECTED_SCHEMA_VERSION = 1


@dataclass(frozen=True)
class ScenarioResult:
    path: Path
    scenario: str
    status: str
    profile: str
    frames: str
    git: str
    viewport: str
    edge: str
    hdf: str
    network: str
    media: str
    evidence: str
    notes: tuple[str, ...]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate a Rumiga compatibility report from rumiga.json manifests.",
    )
    parser.add_argument(
        "--evidence-root",
        default="target/evidence",
        help="Directory tree containing scenario rumiga.json manifests.",
    )
    parser.add_argument(
        "--output",
        default="target/evidence/compatibility-report.md",
        help="Markdown report path to write.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit non-zero if any scenario is classified as fail.",
    )
    return parser.parse_args()


def load_json(path: Path) -> tuple[dict[str, Any] | None, str | None]:
    try:
        return json.loads(path.read_text()), None
    except OSError as exc:
        return None, f"could not read manifest: {exc}"
    except json.JSONDecodeError as exc:
        return None, f"invalid JSON: {exc}"


def scalar(value: Any, default: str = "n/a") -> str:
    if value is None:
        return default
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)


def bool_value(value: Any) -> bool:
    return bool(value)


def int_value(value: Any) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return 0


def short_sha(value: Any) -> str:
    text = scalar(value)
    return text[:12] if text != "n/a" else text


def manifest_paths(root: Path) -> list[Path]:
    if root.is_file():
        return [root]
    if not root.exists():
        return []
    return sorted(root.glob("**/rumiga.json"))


def classify_manifest(path: Path, root: Path, data: dict[str, Any] | None, error: str | None) -> ScenarioResult:
    scenario = scenario_name(path, root)
    if data is None:
        return ScenarioResult(
            path=path,
            scenario=scenario,
            status="fail",
            profile="n/a",
            frames="n/a",
            git="n/a",
            viewport="n/a",
            edge="n/a",
            hdf="n/a",
            network="n/a",
            media="n/a",
            evidence=evidence_links(path),
            notes=(error or "manifest could not be loaded",),
        )

    schema = data.get("schema", {})
    producer = data.get("producer", {})
    run = data.get("run", {})
    viewport = data.get("viewport", {})
    edge = data.get("edge_integrity", {})
    boot = data.get("boot_workarounds", {})
    hdf = data.get("gayle_ide", {})
    network = data.get("network", {})
    media = data.get("media", {})

    notes: list[str] = []
    hard_fail = False
    partial = False

    if schema.get("id") != EXPECTED_SCHEMA_ID or schema.get("version") != EXPECTED_SCHEMA_VERSION:
        hard_fail = True
        notes.append(
            f"schema is {schema.get('id')}@{schema.get('version')}, expected "
            f"{EXPECTED_SCHEMA_ID}@{EXPECTED_SCHEMA_VERSION}"
        )

    if not bool_value(run.get("stopped")):
        partial = True
        notes.append("capture did not stop cleanly")

    mirrored = int_value(edge.get("mirrored_non_background_pixels"))
    left_edge = int_value(edge.get("left_non_background_pixels"))
    right_edge = int_value(edge.get("right_non_background_pixels"))
    if mirrored > 0:
        hard_fail = True
        notes.append(f"edge mirror regression candidate: {mirrored} mirrored pixels")

    if bool_value(boot.get("forced_cia_timer_start")) or int_value(boot.get("forced_cia_timer_start_count")) > 0:
        hard_fail = True
        notes.append("forced CIA timer workaround is active")
    if bool_value(boot.get("rom_drive_step_patch")):
        partial = True
        notes.append("ROM drive step patch is active")

    has_hdf = media.get("hdf") is not None or bool_value(hdf.get("disk_inserted"))
    if has_hdf:
        if hdf.get("geometry_source") == "rdb" and not bool_value(hdf.get("rdb", {}).get("usable")):
            hard_fail = True
            notes.append("RDB geometry was detected but is not usable")
        if bool_value(hdf.get("hdf_dirty")):
            snapshot = hdf.get("hdf_snapshot")
            if isinstance(snapshot, dict):
                notes.append(
                    "HDF was dirty after evidence run; snapshot captured "
                    f"{scalar(snapshot.get('changed_sectors'))} changed sectors"
                )
            else:
                partial = True
                notes.append("HDF was dirty after evidence run without a snapshot")

    network_enabled = bool_value(network.get("enabled"))
    if network_enabled:
        device = network.get("device")
        backend = network.get("backend")
        present = bool_value(network.get("a2065_present"))
        configured = bool_value(network.get("a2065_configured"))
        link_up = bool_value(network.get("link_up"))
        tx_packets = int_value(network.get("tx_packets"))
        rx_packets = int_value(network.get("rx_packets"))
        if device != "a2065" or backend != "slirp" or not present or not link_up:
            hard_fail = True
            notes.append("A2065/SLIRP contract is incomplete")
        elif not configured:
            partial = True
            notes.append("A2065 link is ready but guest did not configure the card")
        elif tx_packets == 0 and rx_packets == 0:
            partial = True
            notes.append("A2065 configured with link up, but no guest packet exchange was observed")

    if hard_fail:
        status = "fail"
    elif partial:
        status = "partial"
    else:
        status = "pass"

    if not notes:
        notes.append("all configured evidence gates passed")

    model = scalar(data.get("model"))
    cpu = scalar(data.get("cpu"))
    video = scalar(data.get("video_standard"))
    profile = f"{model}/{cpu}/{video}"
    frames = scalar(run.get("frames"))
    git = f"{short_sha(producer.get('git_sha'))} dirty={scalar(producer.get('git_dirty'))}"
    viewport_summary = (
        f"{scalar(viewport.get('preset'))} "
        f"{scalar(viewport.get('source_width'))}x{scalar(viewport.get('source_height'))}"
        f"->{scalar(viewport.get('output_width'))}x{scalar(viewport.get('output_height'))}"
        f" stretch={scalar(viewport.get('vertical_stretch'))}"
    )
    edge_summary = f"L{left_edge}/R{right_edge}/M{mirrored}"
    hdf_summary = hdf_status(hdf, has_hdf)
    network_summary = network_status(network, network_enabled)
    media_summary = media_status(media)

    return ScenarioResult(
        path=path,
        scenario=scenario,
        status=status,
        profile=profile,
        frames=frames,
        git=git,
        viewport=viewport_summary,
        edge=edge_summary,
        hdf=hdf_summary,
        network=network_summary,
        media=media_summary,
        evidence=evidence_links(path),
        notes=tuple(notes),
    )


def scenario_name(path: Path, root: Path) -> str:
    try:
        parent = path.parent.relative_to(root)
    except ValueError:
        parent = path.parent
    text = str(parent)
    return text if text else path.parent.name


def hdf_status(hdf: dict[str, Any], has_hdf: bool) -> str:
    if not has_hdf:
        return "none"
    rdb = hdf.get("rdb", {})
    snapshot = hdf.get("hdf_snapshot")
    snapshot_summary = "none"
    if isinstance(snapshot, dict):
        snapshot_summary = (
            f"{scalar(snapshot.get('changed_sectors'))} sectors/"
            f"{scalar(snapshot.get('changed_bytes'))} bytes"
        )
    return (
        f"{scalar(hdf.get('geometry_source'))} "
        f"{scalar(hdf.get('cylinders'))}/{scalar(hdf.get('heads'))}/{scalar(hdf.get('sectors_per_track'))} "
        f"rdb={scalar(rdb.get('usable'))} dirty={scalar(hdf.get('hdf_dirty'))} "
        f"snapshot={snapshot_summary}"
    )


def network_status(network: dict[str, Any], enabled: bool) -> str:
    if not enabled:
        return "disabled"
    return (
        f"{scalar(network.get('device'))}/{scalar(network.get('backend'))} "
        f"link={scalar(network.get('link_up'))} "
        f"cfg={scalar(network.get('a2065_configured'))} "
        f"tx={scalar(network.get('tx_packets'))} rx={scalar(network.get('rx_packets'))}"
    )


def media_status(media: dict[str, Any]) -> str:
    parts: list[str] = []
    rom = media.get("rom")
    if isinstance(rom, dict):
        parts.append(f"rom={short_sha(rom.get('sha256'))}")
    hdf = media.get("hdf")
    if isinstance(hdf, dict):
        parts.append(f"hdf={short_sha(hdf.get('sha256'))}")
    floppies = [
        name
        for name in ("df0", "df1", "df2", "df3")
        if isinstance(media.get(name), dict)
    ]
    if floppies:
        parts.append("adf=" + ",".join(floppies))
    return " ".join(parts) if parts else "n/a"


def evidence_links(manifest: Path) -> str:
    links = ["rumiga.json"]
    png = manifest.with_name("rumiga.png")
    notes = manifest.with_name("notes.md")
    pcap = manifest.with_name("rumiga.pcap")
    if png.exists():
        links.append("rumiga.png")
    if notes.exists():
        links.append("notes.md")
    if pcap.exists():
        links.append("rumiga.pcap")
    return ", ".join(links)


def markdown_table_row(values: tuple[str, ...]) -> str:
    escaped = [value.replace("|", "\\|").replace("\n", " ") for value in values]
    return "| " + " | ".join(escaped) + " |"


def render_report(root: Path, output: Path, results: list[ScenarioResult]) -> str:
    generated = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    counts = {status: sum(1 for result in results if result.status == status) for status in ("pass", "partial", "fail")}

    lines = [
        "# Rumiga Compatibility Evidence Report",
        "",
        f"- Generated: `{generated}`",
        f"- Evidence root: `{root}`",
        f"- Output: `{output}`",
        f"- Scenarios: `{len(results)}` pass=`{counts['pass']}` partial=`{counts['partial']}` fail=`{counts['fail']}`",
        "",
        "## Status Rules",
        "",
        "- `pass`: configured gates passed for the scenario manifest.",
        "- `partial`: emulator path is usable but an evidence gate is incomplete, such as guest TCP traffic not observed.",
        "- `fail`: schema, viewport edge, boot workaround, RDB, or required network contract failed.",
        "",
        "## Scenario Matrix",
        "",
        markdown_table_row(
            (
                "Scenario",
                "Status",
                "Profile",
                "Frames",
                "Git",
                "Viewport",
                "Edge",
                "HDF",
                "Network",
                "Media",
                "Evidence",
            )
        ),
        markdown_table_row(("-" * 8, "-" * 6, "-" * 7, "-" * 6, "-" * 3, "-" * 8, "-" * 4, "-" * 3, "-" * 7, "-" * 5, "-" * 8)),
    ]

    for result in results:
        lines.append(
            markdown_table_row(
                (
                    result.scenario,
                    result.status,
                    result.profile,
                    result.frames,
                    result.git,
                    result.viewport,
                    result.edge,
                    result.hdf,
                    result.network,
                    result.media,
                    result.evidence,
                )
            )
        )

    lines.extend(["", "## Notes", ""])
    for result in results:
        joined_notes = "; ".join(result.notes)
        lines.append(f"- `{result.scenario}`: {result.status} - {joined_notes}")

    lines.extend(
        [
            "",
            "## Release Caveats",
            "",
            "- Reports intentionally reference local artifact names only; ROMs, HDFs, ADFs, screenshots, and PCAPs remain outside git unless explicitly approved.",
            "- A2065 link/configuration evidence is not full guest TCP proof unless TX and RX packet counters are non-zero from a configured guest stack.",
            "- FS-UAE/WinUAE reference captures should be stored beside local evidence and cited in release notes, not committed with copyrighted inputs.",
            "",
        ]
    )
    return "\n".join(lines)


def main() -> int:
    args = parse_args()
    root = Path(args.evidence_root)
    output = Path(args.output)
    paths = manifest_paths(root)

    results = [
        classify_manifest(path, root, *load_json(path))
        for path in paths
    ]

    report = render_report(root, output, results)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(report)

    fail_count = sum(1 for result in results if result.status == "fail")
    partial_count = sum(1 for result in results if result.status == "partial")
    pass_count = sum(1 for result in results if result.status == "pass")
    print(
        f"wrote {output} from {len(results)} manifests "
        f"(pass={pass_count} partial={partial_count} fail={fail_count})"
    )

    if args.strict and fail_count:
        return 4
    return 0


if __name__ == "__main__":
    sys.exit(main())
