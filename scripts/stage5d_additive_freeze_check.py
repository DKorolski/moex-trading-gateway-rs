#!/usr/bin/env python3
"""Validate the Stage 5D additive freeze baseline.

The checker is intentionally local and conservative. Stage 5D has a dual
baseline:

* Stage 5C closure baseline: immutable historical public API/source evidence;
* Stage 5D additive baseline: reviewed bridge regions and Stage5d* API.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Any


DEFAULT_ROOT = Path(__file__).resolve().parents[1]
MANIFEST_REL = Path("docs/stage-5/stage-5d-additive-freeze-manifest.json")
STAGE5C_MANIFEST_REL = Path("docs/stage-5/stage-5c-api-freeze-manifest.json")
STAGE5C_CHECKER_REL = Path("scripts/stage5c_api_freeze_check.py")
LIB_REL = Path("crates/strategy-runtime-core/src/lib.rs")
STAGE5C_HOST_REL = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
WRAPPER_REL = Path("crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs")
STAGE5D_REL = Path("crates/strategy-runtime-core/src/stage5d_persistence.rs")

EXPECTED_STAGE5C_CLOSURE = {
    "short_commit": "69cc73b",
    "full_commit": "69cc73b7f33d8cb418c784ac993856d8a487693d",
    "handoff_archive": "moex-trading-project-69cc73b.zip",
    "handoff_sha256": "0b614ebe83b0a8af85cde0ca7a1ae481457813edad72626cd4bb5972c9c83f91",
    "manifest_sha256": "f8c555d11de1271f5041b4d3abf880ac7a406d6fb23f5e4d38ca25468a974323",
    "report_sha256": "1d15c992ce1658fea6d7ec8a25094b094400ba00b764ac23d32c525207d19b48",
    "original_checker_sha256": "e494e92ffb5f8d90b6a581c7b99e4e80f1906aeedfa1e7446d428eb31c757209",
}

APPROVED_BRIDGE_FILES = {
    str(LIB_REL): ["lib-stage5d-module", "lib-stage5d-exports"],
    str(STAGE5C_HOST_REL): ["type-state-transitions"],
    str(WRAPPER_REL): ["runtime-private-snapshot"],
}

FORBIDDEN_STAGE5D_PUBLIC_PATTERNS = [
    re.compile(r"pub\s+fn\s+.*(?:raw|inner|extract|into_parts|strategy)", re.I),
    re.compile(r"pub\s+struct\s+(?!Stage5d)[A-Za-z0-9_]+"),
    re.compile(r"pub\s+enum\s+(?!Stage5d)[A-Za-z0-9_]+"),
    re.compile(r"pub\s+const\s+(?!STAGE5D)[A-Za-z0-9_]+"),
]

LEGACY_RESTORE_CALLS = [
    "restore_stage5c_runtime_state(",
    "notify_stage5c_bootstrap(",
    "notify_stage5c_runtime_state_restored(",
]

ALLOWED_LEGACY_RESTORE_CALL_PATHS = {
    str(LIB_REL),
    str(STAGE5C_HOST_REL),
    str(STAGE5D_REL),
}


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def load_stage5c_checker(root: Path):
    checker_path = root / STAGE5C_CHECKER_REL
    spec = importlib.util.spec_from_file_location("stage5c_api_freeze_check_for_stage5d", checker_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {checker_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def additive_markers(region: str) -> tuple[bytes, bytes]:
    return (
        f"// STAGE5D-ADDITIVE-BRIDGE-BEGIN: {region}".encode(),
        f"// STAGE5D-ADDITIVE-BRIDGE-END: {region}".encode(),
    )


def strip_additive_regions(path: Path, regions: list[str]) -> tuple[bytes, list[str]]:
    payload = path.read_bytes()
    failures: list[str] = []
    stripped = payload
    previous_start = -1
    for region in regions:
        begin, end = additive_markers(region)
        begin_count = stripped.count(begin)
        end_count = stripped.count(end)
        if begin_count != 1 or end_count != 1:
            failures.append(
                f"{path}: additive region {region} markers must appear exactly once "
                f"(begin={begin_count}, end={end_count})"
            )
            continue
        begin_index = stripped.find(begin)
        end_index = stripped.find(end)
        if begin_index <= previous_start:
            failures.append(f"{path}: additive region {region} marker order drifted")
        if end_index <= begin_index:
            failures.append(f"{path}: additive region {region} closing marker precedes opening marker")
            continue
        line_end = stripped.find(b"\n", end_index)
        if line_end == -1:
            line_end = len(stripped)
        else:
            line_end += 1
        stripped = stripped[:begin_index] + stripped[line_end:]
        previous_start = begin_index
    return stripped, failures


def parse_stage5d_public_symbols(source: str) -> list[str]:
    symbols: set[str] = set()
    for pattern in [
        r"^pub\s+struct\s+(Stage5d[A-Za-z0-9_]+)",
        r"^pub\s+enum\s+(Stage5d[A-Za-z0-9_]+)",
        r"^pub\s+const\s+(STAGE5D[A-Za-z0-9_]+)",
    ]:
        for match in re.finditer(pattern, source, re.M):
            symbols.add(match.group(1))
    return sorted(symbols)


def parse_stage5d_reexports(lib_source: str) -> list[str]:
    match = re.search(r"pub use stage5d_persistence::\{(?P<body>.*?)\};", lib_source, re.S)
    if not match:
        return []
    body = match.group("body")
    return sorted(token.strip() for token in body.replace("\n", " ").split(",") if token.strip())


def validate_stage5c_public_shape(root: Path, manifest: dict[str, Any], failures: list[str]) -> None:
    stage5c_checker = load_stage5c_checker(root)
    stage5c_manifest = json.loads((root / STAGE5C_MANIFEST_REL).read_text())
    surface = stage5c_checker.derive_manifest_surface()
    for key in [
        "public_reexports",
        "public_constants",
        "public_free_functions",
        "public_types",
        "public_methods",
        "opaque_capabilities",
        "externally_constructible_enums",
        "normalized_signature_hash",
    ]:
        if surface.get(key) != stage5c_manifest.get(key):
            failures.append(f"Stage 5C public API shape drifted for {key}")
    declared_count = (
        len(surface["public_constants"])
        + len(surface["public_free_functions"])
        + len(surface["public_types"])
    )
    expected_count = manifest.get("stage5c_public_api", {}).get("public_symbol_count")
    if declared_count != expected_count:
        failures.append(
            f"Stage 5C public symbol count mismatch: actual={declared_count} expected={expected_count}"
        )
    expected_hash = manifest.get("stage5c_public_api", {}).get("normalized_signature_hash")
    if surface.get("normalized_signature_hash") != expected_hash:
        failures.append("Stage 5C normalized signature hash mismatch")


def validate_legacy_restore_call_sites(root: Path, failures: list[str]) -> None:
    for path in sorted((root / "crates").glob("**/*.rs")):
        rel = str(path.relative_to(root))
        if rel in ALLOWED_LEGACY_RESTORE_CALL_PATHS:
            continue
        source = path.read_text(errors="replace")
        for token in LEGACY_RESTORE_CALLS:
            if token in source:
                failures.append(f"legacy Stage 5C restore bypass call-site forbidden: {rel}: {token}")


def validate(root: Path, manifest_path: Path) -> list[str]:
    failures: list[str] = []
    manifest = json.loads(manifest_path.read_text())

    if manifest.get("schema_version") != 1:
        failures.append("schema_version must be 1")
    if manifest.get("stage") != "5D-b1":
        failures.append("stage must be 5D-b1")
    if manifest.get("status") != "additive_freeze_candidate":
        failures.append("status must be additive_freeze_candidate")
    if manifest.get("stage5c_closure_baseline") != EXPECTED_STAGE5C_CLOSURE:
        failures.append("Stage 5C closure baseline reference mismatch")

    stage5c_manifest_hash = sha256_file(root / STAGE5C_MANIFEST_REL)
    if stage5c_manifest_hash != EXPECTED_STAGE5C_CLOSURE["manifest_sha256"]:
        failures.append(
            f"Stage 5C closure manifest hash mismatch: actual={stage5c_manifest_hash}"
        )
    report_hash = sha256_file(root / "docs/stage-5/stage-5c-acceptance-api-freeze-report.md")
    if report_hash != EXPECTED_STAGE5C_CLOSURE["report_sha256"]:
        failures.append(f"Stage 5C closure report hash mismatch: actual={report_hash}")

    validate_stage5c_public_shape(root, manifest, failures)

    approved = manifest.get("approved_bridge_files", {})
    if set(approved) != set(APPROVED_BRIDGE_FILES):
        failures.append(
            f"approved bridge file set mismatch: actual={sorted(approved)} "
            f"expected={sorted(APPROVED_BRIDGE_FILES)}"
        )
    stage5c_manifest = json.loads((root / STAGE5C_MANIFEST_REL).read_text())
    closure_hashes = stage5c_manifest.get("source_hashes", {})
    for rel, regions in APPROVED_BRIDGE_FILES.items():
        path = root / rel
        record = approved.get(rel, {})
        if not path.is_file():
            failures.append(f"approved bridge file missing: {rel}")
            continue
        current_hash = sha256_file(path)
        if record.get("current_sha256") != current_hash:
            failures.append(f"{rel}: current hash mismatch actual={current_hash}")
        if record.get("closure_sha256") != closure_hashes.get(rel):
            failures.append(f"{rel}: closure hash reference mismatch")
        stripped, marker_failures = strip_additive_regions(path, regions)
        failures.extend(marker_failures)
        stripped_hash = sha256_bytes(stripped)
        if record.get("stripped_without_additive_regions_sha256") != stripped_hash:
            failures.append(f"{rel}: stripped hash mismatch actual={stripped_hash}")
        if stripped_hash != closure_hashes.get(rel):
            failures.append(f"{rel}: frozen region does not match Stage 5C closure source")

    stage5d_record = manifest.get("stage5d_persistence_file", {})
    stage5d_path = root / STAGE5D_REL
    if not stage5d_path.is_file():
        failures.append("stage5d_persistence.rs missing")
    else:
        stage5d_hash = sha256_file(stage5d_path)
        if stage5d_record.get("path") != str(STAGE5D_REL):
            failures.append("Stage 5D persistence file path mismatch")
        if stage5d_record.get("current_sha256") != stage5d_hash:
            failures.append(f"stage5d_persistence.rs hash mismatch actual={stage5d_hash}")
        stage5d_source = stage5d_path.read_text()
        for pattern in FORBIDDEN_STAGE5D_PUBLIC_PATTERNS:
            for match in pattern.finditer(stage5d_source):
                failures.append(f"forbidden Stage 5D public surface: {match.group(0)}")
        public_symbols = parse_stage5d_public_symbols(stage5d_source)
        if public_symbols != manifest.get("stage5d_public_symbols"):
            failures.append(
                f"Stage5d public symbol mismatch actual={public_symbols} "
                f"expected={manifest.get('stage5d_public_symbols')}"
            )
        reexports = parse_stage5d_reexports((root / LIB_REL).read_text())
        if reexports != public_symbols:
            failures.append(f"Stage5d re-export mismatch actual={reexports} expected={public_symbols}")

    validate_legacy_restore_call_sites(root, failures)
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=str(DEFAULT_ROOT), help="workspace root")
    parser.add_argument("--manifest", default=None, help="manifest path")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    manifest_path = Path(args.manifest).resolve() if args.manifest else root / MANIFEST_REL
    failures = validate(root, manifest_path)
    if failures:
        for failure in failures:
            print(f"stage5d-additive-freeze-check: {failure}", file=sys.stderr)
        return 1
    print("stage5d-additive-freeze-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
