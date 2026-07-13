#!/usr/bin/env python3
"""Validate the Stage 5C public API freeze manifest.

The checker is intentionally conservative and local. It does not try to be a
complete Rust parser; instead it validates the accepted Stage 5C seam that is
owned by this repository:

* source hashes listed in the manifest;
* the `pub use stage5c_paper_host::{...}` re-export surface in `lib.rs`;
* top-level public functions/constants/types in `stage5c_paper_host.rs`;
* public methods, public struct fields, and public enum variants for Stage 5C
  types;
* executable evidence test names referenced by the manifest.

Use `--update` only when intentionally refreshing the freeze candidate.
Normal scanner runs must execute without `--update`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "docs/stage-5/stage-5c-api-freeze-manifest.json"
LIB_PATH = ROOT / "crates/strategy-runtime-core/src/lib.rs"
STAGE5C_PATH = ROOT / "crates/strategy-runtime-core/src/stage5c_paper_host.rs"


DEFAULT_EVIDENCE_MAP = [
    {
        "transition": "admission_prepare_restore_bootstrap_restored",
        "tests": ["stage5cc_restores_same_strategy_and_opens_no_later_gate"],
    },
    {
        "transition": "clean_admission_prepare_bootstrap",
        "tests": ["stage5cb_uses_exact_snapshot_and_opens_no_later_lifecycle_step"],
    },
    {
        "transition": "history_warmup",
        "tests": ["stage5cd_warms_canonical_history_without_opening_later_gates"],
    },
    {
        "transition": "pending_recovery",
        "tests": ["stage5ce_recovers_complete_empty_pending_set_without_opening_later_gates"],
    },
    {
        "transition": "semantic_bar_settlement",
        "tests": ["stage5cg_settles_zero_intent_result_without_sink"],
    },
    {
        "transition": "controlled_next_bar",
        "tests": ["stage5ch_controlled_next_bar_requires_settled_input_and_accumulates_history"],
    },
    {
        "transition": "ack_lifecycle",
        "tests": ["stage5ci_resolves_nonzero_batch_by_exact_ack_without_sink_or_transport"],
    },
    {
        "transition": "terminal_complete_broker_batch",
        "tests": ["stage5cn_working_filled_position_batch_resolves_as_one_atomic_step"],
    },
    {
        "transition": "generated_intent_ack_lifecycle_timer_settlement",
        "tests": ["stage5cn_callback_generated_broker_intents_settle_and_reenter_ack_lifecycle"],
    },
    {
        "transition": "timer_continuation",
        "tests": ["stage5cm_ready_checkpoint_can_continue_to_timer_or_bar_once"],
    },
    {
        "transition": "blocked_invalid_transitions_preserve_state",
        "tests": [
            "stage5cn_invalid_transition_preserves_input_state",
            "stage5cn_working_only_batch_preserves_state_and_can_retry_full_batch",
        ],
    },
]


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def normalize_signature(text: str) -> str:
    text = re.sub(r"//.*", "", text)
    text = re.sub(r"\s+", " ", text).strip()
    text = text.removesuffix("{").removesuffix(";").strip()
    return text


def collect_signature(lines: list[str], start_index: int) -> tuple[str, int]:
    parts = []
    index = start_index
    while index < len(lines):
        line = lines[index].strip()
        parts.append(line)
        if "{" in line or line.endswith(";"):
            break
        index += 1
    signature = " ".join(parts)
    if "{" in signature:
        signature = signature.split("{", 1)[0]
    return normalize_signature(signature), index


def parse_reexports(lib_source: str) -> list[str]:
    match = re.search(r"pub use stage5c_paper_host::\{(?P<body>.*?)\};", lib_source, re.S)
    if not match:
        raise ValueError("cannot locate stage5c_paper_host re-export block")
    body = match.group("body")
    names = []
    for token in body.replace("\n", " ").split(","):
        name = token.strip()
        if name:
            names.append(name)
    return sorted(names)


def top_level_brace_delta(line: str) -> int:
    stripped = line.strip()
    if stripped.startswith("//"):
        return 0
    return stripped.count("{") - stripped.count("}")


def collect_block(lines: list[str], start_index: int) -> tuple[list[str], int]:
    block = [lines[start_index]]
    depth = top_level_brace_delta(lines[start_index])
    index = start_index + 1
    while index < len(lines) and depth > 0:
        block.append(lines[index])
        depth += top_level_brace_delta(lines[index])
        index += 1
    return block, index - 1


def parse_struct_fields(block: list[str]) -> list[dict[str, str]]:
    fields = []
    for line in block[1:-1]:
        stripped = line.strip().rstrip(",")
        if not stripped.startswith("pub "):
            continue
        declaration = stripped.removeprefix("pub ").strip()
        if ":" not in declaration:
            continue
        name, type_name = declaration.split(":", 1)
        fields.append({"name": name.strip(), "type": normalize_signature(type_name)})
    return fields


def parse_enum_variants(block: list[str]) -> list[str]:
    variants = []
    depth = 0
    for raw in block[1:-1]:
        stripped = raw.strip()
        if not stripped or stripped.startswith("#") or stripped.startswith("//"):
            continue
        if depth == 0:
            match = re.match(r"([A-Z][A-Za-z0-9_]*)", stripped)
            if match:
                variants.append(match.group(1))
        depth += stripped.count("{") + stripped.count("(") - stripped.count("}") - stripped.count(")")
        if depth < 0:
            depth = 0
    return variants


def parse_stage5c_source(source: str) -> dict[str, Any]:
    lines = source.splitlines()
    constants = []
    free_functions = []
    types: dict[str, dict[str, Any]] = {}
    methods = []

    index = 0
    while index < len(lines):
        line = lines[index]
        stripped = line.strip()

        const_match = re.match(r"^pub const ([A-Za-z0-9_]+)\s*:\s*([^=]+)=", stripped)
        if const_match:
            constants.append(
                {
                    "name": const_match.group(1),
                    "type": normalize_signature(const_match.group(2)),
                    "signature": normalize_signature(stripped),
                }
            )

        struct_match = re.match(r"^pub struct (Stage5c[A-Za-z0-9_]+)(?:<[^>]+>)?", stripped)
        if struct_match:
            name = struct_match.group(1)
            block, end_index = collect_block(lines, index) if "{" in stripped else ([line], index)
            fields = parse_struct_fields(block) if "{" in stripped else []
            types[name] = {
                "name": name,
                "kind": "struct",
                "opaque": len(fields) == 0,
                "public_fields": fields,
                "public_variants": [],
            }
            index = end_index

        enum_match = re.match(r"^pub enum (Stage5c[A-Za-z0-9_]+)", stripped)
        if enum_match:
            name = enum_match.group(1)
            block, end_index = collect_block(lines, index)
            types[name] = {
                "name": name,
                "kind": "enum",
                "opaque": False,
                "public_fields": [],
                "public_variants": parse_enum_variants(block),
            }
            index = end_index

        fn_match = re.match(r"^pub fn (stage5c_[A-Za-z0-9_]+|[a-z][A-Za-z0-9_]*stage5c[A-Za-z0-9_]*)", stripped)
        if fn_match:
            signature, end_index = collect_signature(lines, index)
            free_functions.append({"name": fn_match.group(1), "signature": signature})
            index = end_index

        impl_match = re.match(r"^impl (Stage5c[A-Za-z0-9_]+)", stripped)
        if impl_match and "{" in stripped:
            type_name = impl_match.group(1)
            block, end_index = collect_block(lines, index)
            for offset, block_line in enumerate(block):
                method_stripped = block_line.strip()
                method_match = re.match(r"^pub fn ([A-Za-z0-9_]+)", method_stripped)
                if not method_match:
                    continue
                signature, _ = collect_signature(block, offset)
                methods.append(
                    {
                        "type": type_name,
                        "name": method_match.group(1),
                        "signature": signature,
                    }
                )
            index = end_index

        index += 1

    return {
        "public_constants": sorted(constants, key=lambda item: item["name"]),
        "public_free_functions": sorted(free_functions, key=lambda item: item["name"]),
        "public_types": sorted(types.values(), key=lambda item: item["name"]),
        "public_methods": sorted(methods, key=lambda item: (item["type"], item["name"], item["signature"])),
    }


def public_api_hash(surface: dict[str, Any]) -> str:
    payload = json.dumps(surface, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(payload).hexdigest()


def derive_manifest_surface() -> dict[str, Any]:
    stage5c_source = STAGE5C_PATH.read_text()
    lib_source = LIB_PATH.read_text()
    surface = parse_stage5c_source(stage5c_source)
    surface["public_reexports"] = parse_reexports(lib_source)
    surface["opaque_capabilities"] = sorted(
        item["name"]
        for item in surface["public_types"]
        if item["kind"] == "struct" and item["opaque"]
    )
    surface["externally_constructible_enums"] = sorted(
        item["name"] for item in surface["public_types"] if item["kind"] == "enum"
    )
    surface["normalized_signature_hash"] = public_api_hash(
        {
            "public_constants": surface["public_constants"],
            "public_free_functions": surface["public_free_functions"],
            "public_methods": surface["public_methods"],
            "public_reexports": surface["public_reexports"],
            "public_types": surface["public_types"],
        }
    )
    return surface


def validate_evidence_tests(stage5c_source: str, evidence_map: list[dict[str, Any]]) -> list[str]:
    failures = []
    for entry in evidence_map:
        transition = entry.get("transition", "<missing transition>")
        tests = entry.get("tests", [])
        if not tests:
            failures.append(f"evidence transition {transition} has no tests")
            continue
        for test_name in tests:
            if not re.search(rf"\n\s*fn {re.escape(test_name)}\s*\(", stage5c_source):
                failures.append(f"evidence test {test_name} missing for {transition}")
    return failures


def build_updated_manifest(existing: dict[str, Any]) -> dict[str, Any]:
    manifest = dict(existing)
    manifest.pop("public_functions", None)
    manifest["schema_version"] = 2
    manifest["status"] = "api_freeze_candidate"
    manifest["manifest_checker"] = "scripts/stage5c_api_freeze_check.py"
    manifest["source_hashes"] = {
        path: sha256_file(ROOT / path) for path in sorted(existing.get("source_hashes", {}))
    }
    surface = derive_manifest_surface()
    manifest.update(surface)
    manifest["public_surface_counts"] = {
        "public_reexports": len(surface["public_reexports"]),
        "public_constants": len(surface["public_constants"]),
        "public_free_functions": len(surface["public_free_functions"]),
        "public_types": len(surface["public_types"]),
        "public_methods": len(surface["public_methods"]),
        "opaque_capabilities": len(surface["opaque_capabilities"]),
        "externally_constructible_enums": len(surface["externally_constructible_enums"]),
    }
    manifest["executable_evidence_map"] = existing.get(
        "executable_evidence_map", DEFAULT_EVIDENCE_MAP
    )
    return manifest


def compare(label: str, expected: Any, actual: Any, failures: list[str]) -> None:
    if expected != actual:
        failures.append(f"{label} mismatch")


def check_manifest(manifest: dict[str, Any]) -> list[str]:
    failures = []
    if manifest.get("schema_version") != 2:
        failures.append("schema_version must be 2")
    if manifest.get("accepted_implementation_baseline", {}).get("short_commit") != "69cc73b":
        failures.append("accepted implementation baseline short commit must remain 69cc73b")

    for path, expected in manifest.get("source_hashes", {}).items():
        actual_path = ROOT / path
        if not actual_path.is_file():
            failures.append(f"source hash path missing: {path}")
            continue
        actual = sha256_file(actual_path)
        if actual != expected:
            failures.append(f"source hash mismatch for {path}: actual={actual} expected={expected}")

    surface = derive_manifest_surface()
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
        compare(key, manifest.get(key), surface.get(key), failures)

    expected_counts = {
        "public_reexports": len(surface["public_reexports"]),
        "public_constants": len(surface["public_constants"]),
        "public_free_functions": len(surface["public_free_functions"]),
        "public_types": len(surface["public_types"]),
        "public_methods": len(surface["public_methods"]),
        "opaque_capabilities": len(surface["opaque_capabilities"]),
        "externally_constructible_enums": len(surface["externally_constructible_enums"]),
    }
    compare("public_surface_counts", manifest.get("public_surface_counts"), expected_counts, failures)
    failures.extend(
        validate_evidence_tests(
            STAGE5C_PATH.read_text(), manifest.get("executable_evidence_map", [])
        )
    )
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--update", action="store_true", help="rewrite the manifest from current source")
    args = parser.parse_args()

    manifest = json.loads(MANIFEST_PATH.read_text())

    if args.update:
        updated = build_updated_manifest(manifest)
        MANIFEST_PATH.write_text(json.dumps(updated, indent=2, ensure_ascii=False) + "\n")
        print(f"stage5c-api-freeze-check: updated {MANIFEST_PATH.relative_to(ROOT)}")
        return 0

    failures = check_manifest(manifest)
    if failures:
        for failure in failures:
            print(f"stage5c-api-freeze-check: {failure}", file=sys.stderr)
        return 1

    print("stage5c-api-freeze-check: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
