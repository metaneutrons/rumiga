#!/usr/bin/env python3
"""Check that Rust API DTO fields and TypeScript DTO fields stay in sync."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import subprocess
import sys
from pathlib import Path


STRUCTS = (
    "FileEntry",
    "FileListResponse",
    "FormatRequest",
    "FloppyInsertRequest",
    "FloppyEjectRequest",
    "AudioSeparationRequest",
    "WifiNetwork",
    "WifiStatus",
    "WifiConnectRequest",
    "WifiScanResponse",
    "ChannelMixConfig",
    "AudioConfig",
    "ViewportConfig",
    "DisplayConfig",
    "NetworkConfig",
    "NetworkPacketCounters",
    "NetworkStatus",
    "MachineConfig",
    "MachineStatus",
    "SupportBundle",
    "SupportMachineSummary",
    "SupportMediaSummary",
    "SupportScreenshotSummary",
    "ApiEndpoint",
    "ApiResponse",
)

ENUMS = (
    "WifiMode",
    "AmigaModel",
    "ScalingMode",
    "ViewportMode",
    "ViewportPreset",
    "ScreenshotKind",
    "HdfWritePolicy",
    "NetworkDevice",
    "NetworkBackend",
    "ApiResponseFormat",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare rumiga-api Rust DTOs with web TypeScript DTOs.",
    )
    parser.add_argument(
        "--rust",
        default="crates/rumiga-api/src/lib.rs",
        help="Rust API source file.",
    )
    parser.add_argument(
        "--typescript",
        default="web/src/lib/api.ts",
        help="TypeScript API source file.",
    )
    parser.add_argument(
        "--evidence-output",
        default=None,
        help="Optional rumiga.json path for REST/Web control-plane evidence.",
    )
    parser.add_argument(
        "--notes-output",
        default=None,
        help="Optional notes.md path for REST/Web control-plane evidence.",
    )
    return parser.parse_args()


def extract_block(source: str, declaration: str, name: str, opener: str = "{", closer: str = "}") -> str:
    match = re.search(rf"{declaration}\s+{name}\b[^\{{]*\{opener}", source)
    if not match:
        raise ValueError(f"missing {declaration} {name}")
    start = match.end()
    depth = 1
    index = start
    while index < len(source):
        char = source[index]
        if char == opener:
            depth += 1
        elif char == closer:
            depth -= 1
            if depth == 0:
                return source[start:index]
        index += 1
    raise ValueError(f"unterminated {declaration} {name}")


def rust_struct_fields(source: str, name: str) -> list[str]:
    body = extract_block(source, r"pub\s+struct", name)
    fields: list[str] = []
    for line in body.splitlines():
        stripped = line.strip()
        if stripped.startswith("pub "):
            field = stripped.removeprefix("pub ").split(":", 1)[0].strip()
            fields.append(field)
    return fields


def ts_interface_fields(source: str, name: str) -> list[str]:
    body = extract_block(source, r"export\s+interface", name)
    fields: list[str] = []
    for line in body.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("//"):
            continue
        field = stripped.split(":", 1)[0].strip()
        if field:
            fields.append(field)
    return fields


def rust_enum_variants(source: str, name: str) -> list[str]:
    body = extract_block(source, r"pub\s+enum", name)
    variants: list[str] = []
    for line in body.splitlines():
        stripped = line.strip().rstrip(",")
        if not stripped or stripped.startswith("#") or stripped.startswith("///"):
            continue
        if re.match(r"^[A-Z][A-Za-z0-9_]*$", stripped):
            variants.append(stripped)
    return variants


def ts_union_variants(source: str, name: str) -> list[str]:
    match = re.search(rf"export\s+type\s+{name}\s*=\s*(.*?);", source, flags=re.S)
    if not match:
        raise ValueError(f"missing TypeScript type {name}")
    return re.findall(r"'([^']+)'", match.group(1))


def rust_api_endpoints(source: str) -> list[str]:
    path_constants = dict(
        re.findall(r'pub\s+const\s+([A-Z0-9_]+_PATH)\s*:\s*&str\s*=\s*"([^"]+)";', source)
    )
    endpoints: list[str] = []
    for method, path_symbol, response_format in re.findall(
        r'ApiEndpoint::new\(\s*"([^"]+)",\s*([A-Z0-9_]+_PATH),\s*ApiResponseFormat::([A-Za-z]+)\s*,?\s*\)',
        source,
    ):
        path = path_constants.get(path_symbol)
        if path is None:
            raise ValueError(f"missing path constant {path_symbol}")
        endpoints.append(f"{method} {path} {response_format}")
    return endpoints


def ts_api_endpoints(source: str) -> list[str]:
    match = re.search(r"export\s+const\s+API_ENDPOINTS\s*=\s*\[(.*?)\]\s+as\s+const", source, flags=re.S)
    if not match:
        raise ValueError("missing TypeScript API_ENDPOINTS")
    return [
        f"{method} {path} {response_format}"
        for method, path, response_format in re.findall(
            r"\{\s*method:\s*'([^']+)',\s*path:\s*'([^']+)',\s*response_format:\s*'([^']+)'\s*\}",
            match.group(1),
        )
    ]


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git_output(*args: str) -> str | None:
    try:
        result = subprocess.run(
            ["git", *args],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return None
    return result.stdout.strip()


def git_sha() -> str:
    return git_output("rev-parse", "HEAD") or "unknown"


def git_dirty() -> bool:
    status = git_output("status", "--short")
    return bool(status)


def compare_lists(kind: str, name: str, rust_values: list[str], ts_values: list[str]) -> list[str]:
    if rust_values == ts_values:
        return []
    return [
        f"{kind} {name} differs",
        f"  rust: {', '.join(rust_values) if rust_values else '(none)'}",
        f"  ts:   {', '.join(ts_values) if ts_values else '(none)'}",
    ]


def write_control_plane_evidence(
    manifest_path: Path,
    notes_path: Path | None,
    rust_path: Path,
    ts_path: Path,
    endpoint_contracts: list[str],
    failures: list[str],
) -> None:
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    parity_ok = not failures
    manifest = {
        "schema": {"id": "rumiga.capture.v1", "version": 1},
        "producer": {
            "name": "check-api-dto-parity",
            "version": "1",
            "git_sha": git_sha(),
            "git_dirty": git_dirty(),
            "target_os": platform.system().lower() or "unknown",
            "target_arch": platform.machine() or "unknown",
        },
        "model": "desktop",
        "cpu": "host",
        "video_standard": "n/a",
        "run": {"frames": 0, "stopped": True},
        "control_plane": {
            "scenario": "rest-web-control-roundtrip",
            "parity_ok": parity_ok,
            "struct_count": len(STRUCTS),
            "enum_count": len(ENUMS),
            "endpoint_count": len(endpoint_contracts),
            "structs": list(STRUCTS),
            "enums": list(ENUMS),
            "endpoints": endpoint_contracts,
            "failures": failures,
            "sources": {
                "rust": {
                    "path": str(rust_path),
                    "sha256": file_sha256(rust_path),
                },
                "typescript": {
                    "path": str(ts_path),
                    "sha256": file_sha256(ts_path),
                },
            },
        },
    }
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")

    if notes_path is None:
        return

    notes_path.parent.mkdir(parents=True, exist_ok=True)
    result = "pass" if parity_ok else "fail"
    notes = [
        "# REST/Web Control-Plane Evidence",
        "",
        f"- Result: `{result}`",
        f"- Rust DTO source: `{rust_path}`",
        f"- TypeScript DTO source: `{ts_path}`",
        f"- Structs compared: `{len(STRUCTS)}`",
        f"- Enums compared: `{len(ENUMS)}`",
        f"- Endpoint contracts compared: `{len(endpoint_contracts)}`",
    ]
    if failures:
        notes.append("- Failures:")
        notes.extend(f"  - {failure}" for failure in failures)
    else:
        notes.append("- DTO structs, enums, and endpoint contracts match.")
    notes_path.write_text("\n".join(notes) + "\n")


def main() -> int:
    args = parse_args()
    rust_path = Path(args.rust)
    ts_path = Path(args.typescript)
    rust_source = rust_path.read_text()
    ts_source = ts_path.read_text()
    failures: list[str] = []
    endpoint_contracts: list[str] = []

    for name in STRUCTS:
        failures.extend(
            compare_lists(
                "struct",
                name,
                rust_struct_fields(rust_source, name),
                ts_interface_fields(ts_source, name),
            )
        )

    for name in ENUMS:
        failures.extend(
            compare_lists(
                "enum",
                name,
                rust_enum_variants(rust_source, name),
                ts_union_variants(ts_source, name),
            )
        )

    endpoint_contracts = rust_api_endpoints(rust_source)
    failures.extend(
        compare_lists(
            "contract",
            "API_ENDPOINTS",
            endpoint_contracts,
            ts_api_endpoints(ts_source),
        )
    )

    if args.evidence_output:
        write_control_plane_evidence(
            Path(args.evidence_output),
            Path(args.notes_output) if args.notes_output else None,
            rust_path,
            ts_path,
            endpoint_contracts,
            failures,
        )

    if failures:
        print("API DTO parity check failed:", file=sys.stderr)
        print("\n".join(failures), file=sys.stderr)
        return 3

    print(
        f"API DTO parity ok: {len(STRUCTS)} structs, {len(ENUMS)} enums, and "
        f"{len(endpoint_contracts)} endpoints match "
        f"{rust_path} <-> {ts_path}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
