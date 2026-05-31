#!/usr/bin/env python3
"""Check that Rust API DTO fields and TypeScript DTO fields stay in sync."""

from __future__ import annotations

import argparse
import re
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


def compare_lists(kind: str, name: str, rust_values: list[str], ts_values: list[str]) -> list[str]:
    if rust_values == ts_values:
        return []
    return [
        f"{kind} {name} differs",
        f"  rust: {', '.join(rust_values) if rust_values else '(none)'}",
        f"  ts:   {', '.join(ts_values) if ts_values else '(none)'}",
    ]


def main() -> int:
    args = parse_args()
    rust_path = Path(args.rust)
    ts_path = Path(args.typescript)
    rust_source = rust_path.read_text()
    ts_source = ts_path.read_text()
    failures: list[str] = []

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

    failures.extend(
        compare_lists(
            "contract",
            "API_ENDPOINTS",
            rust_api_endpoints(rust_source),
            ts_api_endpoints(ts_source),
        )
    )

    if failures:
        print("API DTO parity check failed:", file=sys.stderr)
        print("\n".join(failures), file=sys.stderr)
        return 3

    print(
        f"API DTO parity ok: {len(STRUCTS)} structs, {len(ENUMS)} enums, and "
        f"{len(rust_api_endpoints(rust_source))} endpoints match "
        f"{rust_path} <-> {ts_path}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
