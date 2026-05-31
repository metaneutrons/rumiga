#!/usr/bin/env python3
"""Generate a markdown compatibility report from Rumiga evidence manifests."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


EXPECTED_SCHEMA_ID = "rumiga.capture.v1"
EXPECTED_SCHEMA_VERSION = 1
CATALOG_SCHEMA_ID = "rumiga.evidence.scenario-catalog.v1"
CATALOG_SCHEMA_VERSION = 1
REPORT_STATUSES = (
    "pass",
    "partial",
    "fail",
    "skipped-missing-assets",
    "unsupported-out-of-scope",
)


@dataclass(frozen=True)
class ScenarioResult:
    path: Path | None
    scenario: str
    status: str
    tier: str
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


@dataclass(frozen=True)
class ScenarioCatalogEntry:
    scenario: str
    tier: str
    status_when_missing: str
    profile: str
    milestone: str
    command: str
    required_assets: tuple[str, ...]
    notes: tuple[str, ...]


@dataclass(frozen=True)
class FilteredManifest:
    path: Path
    scenario: str
    reason: str


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
        "--scenario-catalog",
        default="evidence/scenarios.json",
        help="Optional versioned scenario catalog used to include skipped and out-of-scope rows.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit non-zero if any scenario is classified as fail.",
    )
    parser.add_argument(
        "--git-sha",
        default=None,
        help="Only include manifests produced by this git SHA or SHA prefix.",
    )
    parser.add_argument(
        "--current-git-only",
        action="store_true",
        help="Only include manifests produced by the current Rumiga HEAD.",
    )
    return parser.parse_args()


def load_json(path: Path) -> tuple[dict[str, Any] | None, str | None]:
    try:
        return json.loads(path.read_text()), None
    except OSError as exc:
        return None, f"could not read manifest: {exc}"
    except json.JSONDecodeError as exc:
        return None, f"invalid JSON: {exc}"


def load_catalog(path: Path) -> tuple[dict[str, ScenarioCatalogEntry], list[str]]:
    if not path.exists():
        return {}, []

    data, error = load_json(path)
    if data is None:
        return {}, [f"scenario catalog {path}: {error}"]

    errors: list[str] = []
    schema = data.get("schema", {})
    if schema.get("id") != CATALOG_SCHEMA_ID or schema.get("version") != CATALOG_SCHEMA_VERSION:
        errors.append(
            f"scenario catalog schema is {schema.get('id')}@{schema.get('version')}, "
            f"expected {CATALOG_SCHEMA_ID}@{CATALOG_SCHEMA_VERSION}"
        )

    entries: dict[str, ScenarioCatalogEntry] = {}
    scenarios = data.get("scenarios", [])
    if not isinstance(scenarios, list):
        return {}, errors + ["scenario catalog `scenarios` must be a list"]

    for index, item in enumerate(scenarios):
        if not isinstance(item, dict):
            errors.append(f"scenario catalog item {index} is not an object")
            continue

        scenario = scalar(item.get("id"), "").strip()
        if not scenario:
            errors.append(f"scenario catalog item {index} is missing id")
            continue
        if scenario in entries:
            errors.append(f"scenario catalog id {scenario} is duplicated")
            continue

        status = scalar(item.get("status_when_missing"), "skipped-missing-assets")
        if status not in ("skipped-missing-assets", "unsupported-out-of-scope"):
            errors.append(
                f"scenario catalog id {scenario} has invalid status_when_missing {status}"
            )
            status = "skipped-missing-assets"

        required_assets = tuple(
            str(asset)
            for asset in item.get("required_assets", [])
            if isinstance(asset, str)
        )
        notes = tuple(
            str(note)
            for note in item.get("notes", [])
            if isinstance(note, str)
        )
        profile = "/".join(
            part
            for part in (
                scalar(item.get("machine"), ""),
                scalar(item.get("cpu"), ""),
                scalar(item.get("video_standard"), ""),
            )
            if part
        )
        entries[scenario] = ScenarioCatalogEntry(
            scenario=scenario,
            tier=scalar(item.get("tier"), "uncatalogued"),
            status_when_missing=status,
            profile=profile or "n/a",
            milestone=scalar(item.get("milestone")),
            command=scalar(item.get("command")),
            required_assets=required_assets,
            notes=notes,
        )

    return entries, errors


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


def current_git_sha() -> str | None:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return None
    return result.stdout.strip() or None


def manifest_git_sha(data: dict[str, Any]) -> str | None:
    producer = data.get("producer", {})
    if not isinstance(producer, dict):
        return None
    value = producer.get("git_sha")
    if not isinstance(value, str):
        return None
    return value.strip() or None


def git_sha_matches(actual: str, required: str) -> bool:
    return actual.startswith(required) or required.startswith(actual)


def filter_reason(data: dict[str, Any], required_git_sha: str | None) -> str | None:
    if required_git_sha is None:
        return None

    actual_git_sha = manifest_git_sha(data)
    if actual_git_sha is None:
        return f"missing producer git SHA, required {short_sha(required_git_sha)}"
    if not git_sha_matches(actual_git_sha, required_git_sha):
        return f"git {short_sha(actual_git_sha)} does not match required {short_sha(required_git_sha)}"
    return None


def classify_manifest(
    path: Path,
    root: Path,
    catalog: dict[str, ScenarioCatalogEntry],
    data: dict[str, Any] | None,
    error: str | None,
) -> ScenarioResult:
    scenario = scenario_name(path, root)
    catalog_entry = catalog.get(scenario)
    if data is None:
        return ScenarioResult(
            path=path,
            scenario=scenario,
            status="fail",
            tier=catalog_entry.tier if catalog_entry else "uncatalogued",
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
    presentation = data.get("presentation", {})
    edge = data.get("edge_integrity", {})
    boot = data.get("boot_workarounds", {})
    hdf = data.get("gayle_ide", {})
    network = data.get("network", {})
    media = data.get("media", {})

    notes: list[str] = []
    hard_fail = False
    partial = False
    if catalog_entry:
        notes.append(f"catalog milestone: {catalog_entry.milestone}")

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
    right_to_left = int_value(edge.get("right_edge_wrapped_to_left_pixels"))
    left_to_right = int_value(edge.get("left_edge_wrapped_to_right_pixels"))
    left_edge = int_value(edge.get("left_non_background_pixels"))
    right_edge = int_value(edge.get("right_non_background_pixels"))
    if mirrored > 0:
        hard_fail = True
        notes.append(f"edge mirror regression candidate: {mirrored} mirrored pixels")
    if right_to_left > 0:
        hard_fail = True
        notes.append(
            "right-edge wrap regression candidate: "
            f"{right_to_left} pixels injected at the left edge"
        )
    if left_to_right > 0:
        hard_fail = True
        notes.append(
            "left-edge wrap regression candidate: "
            f"{left_to_right} pixels injected at the right edge"
        )

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
        f" scale={scalar(presentation.get('scaling'))}"
        f" kind={scalar(presentation.get('capture_kind'))}"
    )
    edge_summary = (
        f"L{left_edge}/R{right_edge}/M{mirrored}/"
        f"R2L{right_to_left}/L2R{left_to_right}"
    )
    hdf_summary = hdf_status(hdf, has_hdf)
    network_summary = network_status(network, network_enabled)
    media_summary = media_status(media)

    return ScenarioResult(
        path=path,
        scenario=scenario,
        status=status,
        tier=catalog_entry.tier if catalog_entry else "uncatalogued",
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


def skipped_catalog_result(entry: ScenarioCatalogEntry) -> ScenarioResult:
    notes = list(entry.notes)
    if entry.required_assets:
        notes.append("required assets: " + ", ".join(entry.required_assets))
    if entry.command != "n/a":
        notes.append(f"reproduction command: {entry.command}")
    notes.append(f"catalog milestone: {entry.milestone}")

    return ScenarioResult(
        path=None,
        scenario=entry.scenario,
        status=entry.status_when_missing,
        tier=entry.tier,
        profile=entry.profile,
        frames="n/a",
        git="n/a",
        viewport="n/a",
        edge="n/a",
        hdf="n/a",
        network="n/a",
        media="local assets required" if entry.required_assets else "n/a",
        evidence=entry.command,
        notes=tuple(notes),
    )


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
    native_manifest = manifest.with_name("rumiga-native.json")
    native_png = manifest.with_name("rumiga-native.png")
    notes = manifest.with_name("notes.md")
    pcap = manifest.with_name("rumiga.pcap")
    if png.exists():
        links.append("rumiga.png")
    if native_manifest.exists():
        links.append("rumiga-native.json")
    if native_png.exists():
        links.append("rumiga-native.png")
    if notes.exists():
        links.append("notes.md")
    if pcap.exists():
        links.append("rumiga.pcap")
    return ", ".join(links)


def markdown_table_row(values: tuple[str, ...]) -> str:
    escaped = [value.replace("|", "\\|").replace("\n", " ") for value in values]
    return "| " + " | ".join(escaped) + " |"


def render_report(
    root: Path,
    output: Path,
    catalog_path: Path,
    catalog_errors: list[str],
    results: list[ScenarioResult],
    filtered: list[FilteredManifest],
    git_filter: str | None,
) -> str:
    generated = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    counts = {
        status: sum(1 for result in results if result.status == status)
        for status in REPORT_STATUSES
    }

    lines = [
        "# Rumiga Compatibility Evidence Report",
        "",
        f"- Generated: `{generated}`",
        f"- Evidence root: `{root}`",
        f"- Scenario catalog: `{catalog_path}`",
        f"- Git filter: `{short_sha(git_filter) if git_filter else 'none'}`",
        f"- Filtered manifests: `{len(filtered)}`",
        f"- Output: `{output}`",
        (
            f"- Scenarios: `{len(results)}` pass=`{counts['pass']}` "
            f"partial=`{counts['partial']}` fail=`{counts['fail']}` "
            f"skipped=`{counts['skipped-missing-assets']}` "
            f"unsupported=`{counts['unsupported-out-of-scope']}`"
        ),
        "",
        "## Status Rules",
        "",
        "- `pass`: configured gates passed for the scenario manifest.",
        "- `partial`: emulator path is usable but an evidence gate is incomplete, such as guest TCP traffic not observed.",
        "- `fail`: schema, viewport edge, boot workaround, RDB, or required network contract failed.",
        "- `skipped-missing-assets`: cataloged scenario was not run because local ROM/media inputs were not supplied.",
        "- `unsupported-out-of-scope`: cataloged feature is explicitly outside the current WinUAE-parity target.",
        "",
        "## Scenario Matrix",
        "",
        markdown_table_row(
            (
                "Scenario",
                "Status",
                "Tier",
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
        markdown_table_row(("-" * 8, "-" * 6, "-" * 4, "-" * 7, "-" * 6, "-" * 3, "-" * 8, "-" * 4, "-" * 3, "-" * 7, "-" * 5, "-" * 8)),
    ]

    for result in results:
        lines.append(
            markdown_table_row(
                (
                    result.scenario,
                    result.status,
                    result.tier,
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

    if catalog_errors:
        lines.extend(["", "## Catalog Warnings", ""])
        for error in catalog_errors:
            lines.append(f"- {error}")

    if filtered:
        lines.extend(["", "## Filtered Manifests", ""])
        for item in filtered:
            lines.append(f"- `{item.scenario}` (`{item.path}`): {item.reason}")

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
    catalog_path = Path(args.scenario_catalog)
    git_filter = args.git_sha.strip() if isinstance(args.git_sha, str) else None
    if git_filter == "":
        git_filter = None
    if args.current_git_only:
        git_filter = current_git_sha()
        if git_filter is None:
            print("could not resolve current git SHA for --current-git-only", file=sys.stderr)
            return 6
    catalog, catalog_errors = load_catalog(catalog_path)
    paths = manifest_paths(root)

    results: list[ScenarioResult] = []
    filtered: list[FilteredManifest] = []
    for path in paths:
        data, error = load_json(path)
        if data is not None:
            reason = filter_reason(data, git_filter)
            if reason is not None:
                filtered.append(
                    FilteredManifest(
                        path=path,
                        scenario=scenario_name(path, root),
                        reason=reason,
                    )
                )
                continue
        results.append(classify_manifest(path, root, catalog, data, error))

    observed = {result.scenario for result in results}
    results.extend(
        skipped_catalog_result(entry)
        for scenario, entry in catalog.items()
        if scenario not in observed
    )

    report = render_report(root, output, catalog_path, catalog_errors, results, filtered, git_filter)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(report)

    fail_count = sum(1 for result in results if result.status == "fail")
    partial_count = sum(1 for result in results if result.status == "partial")
    pass_count = sum(1 for result in results if result.status == "pass")
    skipped_count = sum(1 for result in results if result.status == "skipped-missing-assets")
    unsupported_count = sum(1 for result in results if result.status == "unsupported-out-of-scope")
    print(
        f"wrote {output} from {len(results)} scenario entries "
        f"(pass={pass_count} partial={partial_count} fail={fail_count} "
        f"skipped={skipped_count} unsupported={unsupported_count} filtered={len(filtered)})"
    )

    if args.strict and catalog_errors:
        return 5
    if args.strict and fail_count:
        return 4
    return 0


if __name__ == "__main__":
    sys.exit(main())
