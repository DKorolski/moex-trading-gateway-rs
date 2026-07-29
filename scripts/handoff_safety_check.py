#!/usr/bin/env python3
"""Fail-closed source/archive safety checks for review handoffs."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import stat
import sys
import zipfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath

sys.path.insert(0, str(Path(__file__).resolve().parent))
from stage5e_descriptor import descriptor_for_stage
from stage5f_descriptor import descriptor_for_stage as stage5f_descriptor_for_stage
from stage5e_b_no_io_lifecycle_check import (
    EXPECTED_ALLOWED_CHANGED_PATHS as STAGE5E_B_ALLOWED_CHANGED_PATHS,
    EXPECTED_TOP_LEVEL_KEYS as STAGE5E_B_TOP_LEVEL_KEYS,
)


EXCLUDED_PARTS = {".git", "target", "tmp", "reports", "__pycache__", "__MACOSX"}
FORBIDDEN_NAME_PATTERNS = (
    re.compile(r"^\.env$"),
    re.compile(r"^\.env\.(?!example$).+"),
    re.compile(r".*\.log$"),
    re.compile(r".*\.local\..*$"),
)
FORBIDDEN_CONTENT = re.compile(
    rb"(75" rb"02[A-Z0-9]*|190" rb"9892|63" rb"170[A-Z0-9/]*|"
    rb"tapi_[sa]k_[A-Za-z0-9_-]+|"
    rb"eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{10,})"
)
HEX64 = re.compile(r"[0-9a-f]{64}")
HEX40 = re.compile(r"[0-9a-f]{40}")
ISO_UTC = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z")
JSON_SHA256_VALUE = re.compile(rb'("(?:sha256|[a-z_]+_sha256)"\s*:\s*)"[0-9a-f]{64}"')
JSON_GIT_OR_DIGEST_VALUE = re.compile(rb'"[0-9a-f]{40}(?:[0-9a-f]{24})?"')


def path_is_excluded(path: PurePosixPath) -> bool:
    return any(part in EXCLUDED_PARTS for part in path.parts) or any(
        pattern.fullmatch(path.name) for pattern in FORBIDDEN_NAME_PATTERNS
    ) or path.name == ".DS_Store"


def check_payload(name: str, payload: bytes) -> None:
    if b"\x00" in payload:
        return
    # Generated provenance JSON contains many independently verified digest
    # values. A random hexadecimal digest can accidentally contain an
    # account-like substring; scan its structure, not its cryptographic noise.
    if name.startswith("handoff-") and name.endswith(".json"):
        payload = JSON_SHA256_VALUE.sub(rb'\1"<sha256>"', payload)
        payload = JSON_GIT_OR_DIGEST_VALUE.sub(b'"<verified-digest>"', payload)
    match = FORBIDDEN_CONTENT.search(payload)
    if match:
        raise SystemExit(f"handoff safety: forbidden live-like literal in {name}")


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def parse_utc_timestamp(value: object, field: str) -> datetime:
    if not isinstance(value, str) or not ISO_UTC.fullmatch(value):
        raise SystemExit(f"handoff safety: invalid Stage 5E gate timestamp: {field}")
    return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(timezone.utc)


def require_hex64(value: object, field: str) -> None:
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        raise SystemExit(f"handoff safety: missing or invalid {field}")


def git_blob_sha1(payload: bytes) -> bytes:
    return hashlib.sha1(b"blob " + str(len(payload)).encode() + b"\0" + payload).digest()


def git_tree_sha1(entries: dict[str, tuple[str, bytes]]) -> str:
    tree: dict[str, object] = {}
    for path, (mode, object_hash) in entries.items():
        parts = path.split("/")
        cursor = tree
        for part in parts[:-1]:
            cursor = cursor.setdefault(part, {})  # type: ignore[assignment]
            if not isinstance(cursor, dict):
                raise SystemExit("handoff safety: invalid source-tree path nesting")
        cursor[parts[-1]] = (mode, object_hash)

    def digest_node(node: dict[str, object]) -> bytes:
        body = bytearray()
        def sort_key(name: str) -> str:
            return f"{name}/" if isinstance(node[name], dict) else name

        for name in sorted(node, key=sort_key):
            value = node[name]
            if isinstance(value, dict):
                mode = "40000"
                digest = digest_node(value)
            else:
                mode, digest = value  # type: ignore[misc]
            body.extend(mode.encode())
            body.extend(b" ")
            body.extend(name.encode())
            body.extend(b"\0")
            body.extend(digest)
        payload = bytes(body)
        return hashlib.sha1(b"tree " + str(len(payload)).encode() + b"\0" + payload).digest()

    return digest_node(tree).hex()


STAGE5F_ENTRY_STAGE = "5F-a-atomic-hybrid-semantics-entry"
STAGE5F_BASELINE_REF = "e14654f7129aa61011931306140a3bfefe2fcfbc"
STAGE5F_B3F_CLOSURE = {
    "source_ref": STAGE5F_BASELINE_REF,
    "checker_sha256": "cb873e636427c071b26c9c2781ebc320fd9a4c3bf79fd85efabcf91ba97c828a",
    "inventory_sha256": "e459675149e4e0b465da94a60e16adae856b422185fb9221ea627aa2db93a4dd",
    "plan_sha256": "91f2bf5a63da1d6d1626c8469e6a1bcbe0b5a6c99986d03963630ab5a62c3a3a",
    "stage5c_source_sha256": "0fce95557b2e7673d7e7e74a5b4d65dd3ec28360fab3674c20e3e6de6be02ff3",
    "stage5e_source_sha256": "34ed25d3ee188d3f0c52d4b655c6105349e9761b7bd3a5af934e52cab14fb2d6",
    "stage5c_region_semantic_token_sha256": "c1b4643260249676d4917ba17300866b2a3a05a9ee75e7c4dc99ff120f028d0f",
    "stage5e_region_semantic_token_sha256": "ed0733e2843b144524ed364708b6554e7744c93823953b24ea83af1d3ca6c1d3",
    "provenance_negative_case_count": 580,
    "production_ui_case_count": 8,
    "accepted_descriptor_stage": "5E-b3f-callback-settlement-escrow-design",
}
STAGE5F_GENERATED_MEMBERS = {
    "handoff-commit.txt",
    "handoff-cargo-gate-result.json",
    "handoff-cargo-gate-stderr.txt",
    "handoff-cargo-gate-stdout.txt",
    "handoff-forbidden-negative-result.json",
    "handoff-forbidden-negative-stderr.txt",
    "handoff-forbidden-negative-stdout.txt",
    "handoff-manifest.json",
    "handoff-provenance-negative-result.json",
    "handoff-provenance-negative-stderr.txt",
    "handoff-provenance-negative-stdout.txt",
    "handoff-stage5d-negative-result.json",
    "handoff-stage5d-negative-stderr.txt",
    "handoff-stage5d-negative-stdout.txt",
    "handoff-stage5f-gate-result.json",
    "handoff-stage5f-gate-stderr.txt",
    "handoff-stage5f-gate-stdout.txt",
    "handoff-stage5f-negative-result.json",
    "handoff-stage5f-negative-stderr.txt",
    "handoff-stage5f-negative-stdout.txt",
    "handoff-source-tree-manifest.json",
}


def check_stage5f_negative_result(
    archive: zipfile.ZipFile, manifest: dict[str, object]
) -> dict[str, object]:
    result_name = "handoff-stage5f-negative-result.json"
    stdout_name = "handoff-stage5f-negative-stdout.txt"
    stderr_name = "handoff-stage5f-negative-stderr.txt"
    result = json.loads(archive.read(result_name))
    expected_keys = {
        "command",
        "exit_code",
        "finished_at_utc",
        "gate_id",
        "passed_cases",
        "schema_version",
        "source_ref",
        "source_tree_manifest_sha256",
        "source_tree_member_count",
        "started_at_utc",
        "stderr_member",
        "stderr_sha256",
        "stdout_member",
        "stdout_sha256",
    }
    if not isinstance(result, dict) or set(result) != expected_keys:
        raise SystemExit("handoff safety: Stage 5F negative result key set drift")
    if (
        result.get("schema_version") != 1
        or result.get("gate_id") != "stage5f_atomic_hybrid_semantics_negative"
        or result.get("command")
        != ["python3", "scripts/stage5f_atomic_hybrid_semantics_negative_harness.py"]
        or result.get("exit_code") != 0
        or result.get("passed_cases") != 13
    ):
        raise SystemExit("handoff safety: Stage 5F negative gate did not pass")
    parse_utc_timestamp(result.get("started_at_utc"), "Stage 5F negative started_at_utc")
    parse_utc_timestamp(result.get("finished_at_utc"), "Stage 5F negative finished_at_utc")
    if result.get("stdout_member") != stdout_name or result.get("stderr_member") != stderr_name:
        raise SystemExit("handoff safety: Stage 5F negative log member mismatch")
    for field, member in [("stdout_sha256", stdout_name), ("stderr_sha256", stderr_name)]:
        require_hex64(result.get(field), f"Stage 5F negative {field}")
        if hashlib.sha256(archive.read(member)).hexdigest() != result[field]:
            raise SystemExit(f"handoff safety: Stage 5F negative {field} mismatch")
    if archive.read(stdout_name).count(b"PASS ") != 13:
        raise SystemExit("handoff safety: Stage 5F negative passed case count mismatch")
    require_hex64(manifest.get("stage5f_negative_result_sha256"), "stage5f_negative_result_sha256")
    if hashlib.sha256(archive.read(result_name)).hexdigest() != manifest.get(
        "stage5f_negative_result_sha256"
    ):
        raise SystemExit("handoff safety: Stage 5F negative result hash mismatch")
    return result


def check_stage5f_source_tree_binding(
    archive: zipfile.ZipFile,
    manifest: dict[str, object],
    gate_result: dict[str, object],
    cargo_result: dict[str, object],
    provenance_result: dict[str, object],
    negative_results: dict[str, dict[str, object]],
) -> None:
    source_tree_manifest_name = "handoff-source-tree-manifest.json"
    source_tree_manifest = json.loads(archive.read(source_tree_manifest_name))
    expected_source_tree_keys = {
        "baseline_ref",
        "changed_paths",
        "excluded_generated_members",
        "head_tree",
        "members",
        "schema_version",
        "source_ref",
    }
    if not isinstance(source_tree_manifest, dict) or set(source_tree_manifest) != expected_source_tree_keys:
        raise SystemExit("handoff safety: Stage 5F source-tree manifest key set drift")
    if source_tree_manifest.get("schema_version") != 1:
        raise SystemExit("handoff safety: unsupported Stage 5F source-tree manifest schema_version")
    design_scope = gate_result.get("design_scope")
    if not isinstance(design_scope, dict):
        raise SystemExit("handoff safety: Stage 5F design scope must be an object")
    if source_tree_manifest.get("source_ref") != gate_result.get("source_ref"):
        raise SystemExit("handoff safety: Stage 5F source-tree source_ref mismatch")
    if source_tree_manifest.get("head_tree") != design_scope.get("head_tree"):
        raise SystemExit("handoff safety: Stage 5F source-tree head_tree mismatch")
    if source_tree_manifest.get("baseline_ref") != design_scope.get("baseline_ref"):
        raise SystemExit("handoff safety: Stage 5F source-tree baseline mismatch")
    if source_tree_manifest.get("changed_paths") != design_scope.get("changed_paths"):
        raise SystemExit("handoff safety: Stage 5F source-tree changed_paths mismatch")
    if gate_result.get("source_tree_manifest_sha256") != manifest.get(
        "source_tree_manifest_sha256"
    ):
        raise SystemExit("handoff safety: Stage 5F gate/source-tree manifest mismatch")
    if gate_result.get("source_tree_manifest_sha256") != hashlib.sha256(
        archive.read(source_tree_manifest_name)
    ).hexdigest():
        raise SystemExit("handoff safety: Stage 5F source-tree manifest hash mismatch")
    generated = source_tree_manifest.get("excluded_generated_members")
    if not isinstance(generated, list) or not all(isinstance(item, str) for item in generated):
        raise SystemExit("handoff safety: Stage 5F generated member list invalid")
    if set(generated) != STAGE5F_GENERATED_MEMBERS:
        raise SystemExit("handoff safety: Stage 5F generated member set drift")
    source_members = source_tree_manifest.get("members")
    if not isinstance(source_members, list):
        raise SystemExit("handoff safety: Stage 5F source-tree members must be a list")
    source_member_map: dict[str, tuple[str, str]] = {}
    for row in source_members:
        if not isinstance(row, dict) or set(row) != {"git_mode", "path", "sha256"}:
            raise SystemExit("handoff safety: Stage 5F source-tree member row key set drift")
        member_path = row["path"]
        member_sha = row["sha256"]
        git_mode = row["git_mode"]
        if not isinstance(member_path, str) or not member_path:
            raise SystemExit("handoff safety: Stage 5F source-tree member path invalid")
        if git_mode not in {"100644", "100755"}:
            raise SystemExit("handoff safety: Stage 5F source-tree member git_mode invalid")
        require_hex64(member_sha, f"Stage 5F source-tree member sha256 {member_path}")
        if member_path in source_member_map:
            raise SystemExit("handoff safety: Stage 5F duplicate source-tree member")
        source_member_map[member_path] = (git_mode, member_sha)
    if gate_result.get("source_tree_member_count") != len(source_member_map):
        raise SystemExit("handoff safety: Stage 5F source-tree member count mismatch")
    if cargo_result.get("source_tree_manifest_sha256") != manifest.get("source_tree_manifest_sha256"):
        raise SystemExit("handoff safety: cargo gate/Stage 5F source-tree mismatch")
    if cargo_result.get("source_tree_member_count") != len(source_member_map):
        raise SystemExit("handoff safety: cargo gate/Stage 5F member count mismatch")
    if provenance_result.get("source_ref") != gate_result.get("source_ref"):
        raise SystemExit("handoff safety: provenance-negative/Stage 5F source_ref mismatch")
    if provenance_result.get("source_tree_manifest_sha256") != manifest.get(
        "source_tree_manifest_sha256"
    ):
        raise SystemExit("handoff safety: provenance-negative/Stage 5F source-tree mismatch")
    if provenance_result.get("source_tree_member_count") != len(source_member_map):
        raise SystemExit("handoff safety: provenance-negative/Stage 5F member count mismatch")
    for prefix, result in negative_results.items():
        if result.get("source_ref") != gate_result.get("source_ref"):
            raise SystemExit(f"handoff safety: {prefix}-negative/Stage 5F source_ref mismatch")
        if result.get("source_tree_manifest_sha256") != manifest.get(
            "source_tree_manifest_sha256"
        ):
            raise SystemExit(f"handoff safety: {prefix}-negative/Stage 5F source-tree mismatch")
        if result.get("source_tree_member_count") != len(source_member_map):
            raise SystemExit(f"handoff safety: {prefix}-negative/Stage 5F member count mismatch")
    archive_files = {info.filename for info in archive.infolist() if not info.is_dir()}
    expected_archive_files = set(source_member_map) | set(generated)
    if archive_files != expected_archive_files:
        raise SystemExit("handoff safety: Stage 5F source-tree/archive member set mismatch")
    git_entries: dict[str, tuple[str, bytes]] = {}
    for member_path, (git_mode, expected_sha) in source_member_map.items():
        payload = archive.read(member_path)
        if hashlib.sha256(payload).hexdigest() != expected_sha:
            raise SystemExit(f"handoff safety: Stage 5F source-tree member hash mismatch: {member_path}")
        git_entries[member_path] = (git_mode, git_blob_sha1(payload))
    if git_tree_sha1(git_entries) != design_scope.get("head_tree"):
        raise SystemExit("handoff safety: Stage 5F source-tree head_tree mismatch")


def check_stage5f_archive(
    archive: zipfile.ZipFile,
    names: list[str],
    manifest: dict[str, object],
    cargo_result: dict[str, object],
    provenance_result: dict[str, object],
    heavy_negative_results: dict[str, dict[str, object]],
    stage5d_manifest_name: str,
) -> dict[str, object]:
    current_review_stage = manifest.get("current_review_stage")
    if current_review_stage != STAGE5F_ENTRY_STAGE:
        raise SystemExit("handoff safety: current_review_stage/Stage 5F inventory mismatch")
    expected_manifest_keys = {
        "archive_name",
        "cargo_gate_result_sha256",
        "created_at_utc",
        "current_review_stage",
        "forbidden_negative_result_sha256",
        "provenance_negative_result_sha256",
        "required_gate_names",
        "review_stage",
        "schema_version",
        "source_commit",
        "source_ref",
        "source_tree_manifest_sha256",
        "stage5c_checker_sha256",
        "stage5d_checker_sha256",
        "stage5d_manifest_sha256",
        "stage5d_negative_result_sha256",
        "stage5f_active_descriptor_sha256",
        "stage5f_checker_sha256",
        "stage5f_descriptor_registry_sha256",
        "stage5f_design_scope_sha256",
        "stage5f_gate_result_sha256",
        "stage5f_inventory_sha256",
        "stage5f_negative_result_sha256",
        "stage5f_plan_sha256",
    }
    if set(manifest) != expected_manifest_keys:
        raise SystemExit("handoff safety: Stage 5F manifest key set drift")
    if manifest.get("required_gate_names") != [
        "stage5f_atomic_hybrid_semantics",
        "stage5f_atomic_hybrid_semantics_negative",
        "stage5e_b3f_snapshot_inheritance",
        "stage5c_api_freeze",
        "stage5d_additive_freeze",
        "forbidden_surface",
        "forbidden_surface_negative",
        "stage5d_negative",
        "handoff_provenance_negative",
        "no_redis_smoke",
        "python_syntax",
        "fixture_parse",
        "handoff_source_safety",
        "handoff_archive_safety",
        "checker_input_completeness",
        "cargo_fmt",
        "cargo_test_all_targets",
        "cargo_test_docs",
        "cargo_clippy",
    ]:
        raise SystemExit("handoff safety: Stage 5F required gate list drift")
    active_descriptor_name = "docs/stage-5/stage5f-active-descriptor.json"
    descriptor_registry_name = "scripts/stage5f_descriptor.py"
    gate_result_name = "handoff-stage5f-gate-result.json"
    gate_stdout_name = "handoff-stage5f-gate-stdout.txt"
    gate_stderr_name = "handoff-stage5f-gate-stderr.txt"
    negative_result_name = "handoff-stage5f-negative-result.json"
    negative_stdout_name = "handoff-stage5f-negative-stdout.txt"
    negative_stderr_name = "handoff-stage5f-negative-stderr.txt"
    source_tree_manifest_name = "handoff-source-tree-manifest.json"
    required_members = {
        active_descriptor_name,
        descriptor_registry_name,
        "docs/stage-5/5e-b3f-callback-settlement-escrow-design.md",
        "docs/stage-5/stage5e-active-descriptor.json",
        "docs/stage-5/stage5e-b3f-callback-settlement-escrow-design-inventory.json",
        "scripts/handoff_provenance_negative_harness.py",
        "scripts/stage5e_b3f_callback_settlement_escrow_design_check.py",
        "scripts/stage5e_b3f_production_ui_harness.py",
        "scripts/stage5f_atomic_hybrid_semantics_gate.sh",
        "scripts/stage5f_atomic_hybrid_semantics_negative_harness.py",
        gate_result_name,
        gate_stdout_name,
        gate_stderr_name,
        negative_result_name,
        negative_stdout_name,
        negative_stderr_name,
        source_tree_manifest_name,
    }
    missing = sorted(required_members - set(names))
    if missing:
        raise SystemExit(f"handoff safety: missing Stage 5F member: {missing[0]}")
    active_descriptor = json.loads(archive.read(active_descriptor_name))
    if active_descriptor != {"schema_version": 1, "stage": STAGE5F_ENTRY_STAGE}:
        raise SystemExit("handoff safety: active Stage 5F descriptor drift")
    try:
        selected = stage5f_descriptor_for_stage(active_descriptor["stage"])
    except ValueError as exc:
        raise SystemExit(f"handoff safety: {exc}") from exc
    inventory_name = selected["inventory"]
    plan_name = selected["plan"]
    checker_name = selected["checker"]
    for member in [inventory_name, plan_name, checker_name]:
        if member not in names:
            raise SystemExit(f"handoff safety: missing Stage 5F member: {member}")
    for field, member in [
        ("stage5f_active_descriptor_sha256", active_descriptor_name),
        ("stage5f_descriptor_registry_sha256", descriptor_registry_name),
        ("stage5f_checker_sha256", checker_name),
        ("stage5f_inventory_sha256", inventory_name),
        ("stage5f_plan_sha256", plan_name),
        ("stage5f_gate_result_sha256", gate_result_name),
        ("stage5f_negative_result_sha256", negative_result_name),
        ("source_tree_manifest_sha256", source_tree_manifest_name),
    ]:
        expected = manifest.get(field)
        require_hex64(expected, field)
        if hashlib.sha256(archive.read(member)).hexdigest() != expected:
            raise SystemExit(f"handoff safety: {field} mismatch")
    inventory = json.loads(archive.read(inventory_name))
    expected_inventory_keys = {
        "accepted_stage5e_b3f_closure",
        "allowed_changed_paths",
        "atomic_transition_contract",
        "baseline_ref",
        "closed_surfaces",
        "expected_stage5f_negative_case_count",
        "required_atomic_scenarios",
        "schema_version",
        "sole_route",
        "stage",
        "stage_boundaries",
        "status",
        "target_contract",
    }
    if not isinstance(inventory, dict) or set(inventory) != expected_inventory_keys:
        raise SystemExit("handoff safety: Stage 5F inventory key set drift")
    if (
        inventory.get("schema_version") != 1
        or inventory.get("stage") != STAGE5F_ENTRY_STAGE
        or inventory.get("status") != "entry_governance_design_pending_review"
        or inventory.get("baseline_ref") != STAGE5F_BASELINE_REF
        or inventory.get("accepted_stage5e_b3f_closure") != STAGE5F_B3F_CLOSURE
        or inventory.get("expected_stage5f_negative_case_count") != 13
    ):
        raise SystemExit("handoff safety: Stage 5F inventory authority drift")
    target_contract = inventory.get("target_contract")
    if target_contract != {
        "instrument_symbol": "IMOEXF",
        "strategy_profile": "imoexf_primary_riskgate_high180_lb120",
        "bar_contract": "canonical_final_m10",
        "execution_mode": "paper_only",
        "alor_oracle_is_runtime_decision_source": False,
    }:
        raise SystemExit("handoff safety: Stage 5F target contract drift")
    closed = inventory.get("closed_surfaces")
    boundaries = inventory.get("stage_boundaries")
    if (
        not isinstance(closed, dict)
        or any(value is not False for value in closed.values())
        or not isinstance(boundaries, dict)
        or any(value is not False for value in boundaries.values())
    ):
        raise SystemExit("handoff safety: Stage 5F closed-surface drift")
    expected_allowed_paths = [
        "README.md",
        "docs/current-status.md",
        "docs/handoff.md",
        "docs/stage-5/5f-a-atomic-hybrid-semantics-entry.md",
        "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json",
        "docs/stage-5/stage5f-active-descriptor.json",
        "scripts/handoff_safety_check.py",
        "scripts/make_handoff_archive.sh",
        "scripts/stage5f_atomic_hybrid_semantics_entry_check.py",
        "scripts/stage5f_atomic_hybrid_semantics_gate.sh",
        "scripts/stage5f_atomic_hybrid_semantics_negative_harness.py",
        "scripts/stage5f_descriptor.py",
    ]
    if inventory.get("allowed_changed_paths") != expected_allowed_paths:
        raise SystemExit("handoff safety: Stage 5F changed-path allowlist drift")

    gate_result = json.loads(archive.read(gate_result_name))
    expected_gate_keys = {
        "accepted_stage5e_b3f_source_ref",
        "command",
        "design_scope",
        "exit_code",
        "finished_at_utc",
        "gate_id",
        "input_sha256",
        "schema_version",
        "source_ref",
        "source_tree_manifest_sha256",
        "source_tree_member_count",
        "started_at_utc",
        "stderr_member",
        "stderr_line_count",
        "stderr_sha256",
        "stdout_member",
        "stdout_line_count",
        "stdout_sha256",
    }
    if not isinstance(gate_result, dict) or set(gate_result) != expected_gate_keys:
        raise SystemExit("handoff safety: Stage 5F gate result key set drift")
    if (
        gate_result.get("schema_version") != 1
        or gate_result.get("gate_id") != "stage5f_atomic_hybrid_semantics"
        or gate_result.get("command")
        != ["bash", "scripts/stage5f_atomic_hybrid_semantics_gate.sh"]
        or gate_result.get("accepted_stage5e_b3f_source_ref") != STAGE5F_BASELINE_REF
        or gate_result.get("exit_code") != 0
    ):
        raise SystemExit("handoff safety: Stage 5F gate identity drift")
    started_at = parse_utc_timestamp(gate_result.get("started_at_utc"), "Stage 5F started_at_utc")
    finished_at = parse_utc_timestamp(gate_result.get("finished_at_utc"), "Stage 5F finished_at_utc")
    if finished_at < started_at:
        raise SystemExit("handoff safety: Stage 5F gate timestamp order invalid")
    if gate_result.get("stdout_member") != gate_stdout_name or gate_result.get("stderr_member") != gate_stderr_name:
        raise SystemExit("handoff safety: Stage 5F gate log member mismatch")
    for field, member in [("stdout_sha256", gate_stdout_name), ("stderr_sha256", gate_stderr_name)]:
        require_hex64(gate_result.get(field), f"Stage 5F gate {field}")
        if hashlib.sha256(archive.read(member)).hexdigest() != gate_result[field]:
            raise SystemExit(f"handoff safety: Stage 5F gate {field} mismatch")
    for field in ["stdout_line_count", "stderr_line_count"]:
        if not isinstance(gate_result.get(field), int) or gate_result[field] < 0:
            raise SystemExit(f"handoff safety: invalid Stage 5F gate {field}")
    input_sha256 = gate_result.get("input_sha256")
    expected_input_members = {
        "stage5c_checker": "scripts/stage5c_api_freeze_check.py",
        "stage5d_checker": "scripts/stage5d_additive_freeze_check.py",
        "stage5d_manifest": stage5d_manifest_name,
        "stage5e_b3f_active_descriptor": "docs/stage-5/stage5e-active-descriptor.json",
        "stage5e_b3f_checker": "scripts/stage5e_b3f_callback_settlement_escrow_design_check.py",
        "stage5e_b3f_inventory": "docs/stage-5/stage5e-b3f-callback-settlement-escrow-design-inventory.json",
        "stage5e_b3f_plan": "docs/stage-5/5e-b3f-callback-settlement-escrow-design.md",
        "stage5e_b3f_production_ui_harness": "scripts/stage5e_b3f_production_ui_harness.py",
        "stage5e_b3f_provenance_negative_harness": "scripts/handoff_provenance_negative_harness.py",
        "stage5f_active_descriptor": active_descriptor_name,
        "stage5f_checker": checker_name,
        "stage5f_descriptor_registry": descriptor_registry_name,
        "stage5f_inventory": inventory_name,
        "stage5f_plan": plan_name,
    }
    if not isinstance(input_sha256, dict) or set(input_sha256) != set(expected_input_members):
        raise SystemExit("handoff safety: Stage 5F gate input hash key set drift")
    for key, member in expected_input_members.items():
        value = input_sha256.get(key)
        require_hex64(value, f"Stage 5F gate input hash {key}")
        if value != hashlib.sha256(archive.read(member)).hexdigest():
            raise SystemExit(f"handoff safety: Stage 5F gate input/archive mismatch: {key}")
    expected_b3f_input_hashes = {
        "stage5e_b3f_active_descriptor": "73990dae9c5c5972c5217c62126707b9c24b4beffc655810673a628f35edbb8c",
        "stage5e_b3f_checker": STAGE5F_B3F_CLOSURE["checker_sha256"],
        "stage5e_b3f_inventory": STAGE5F_B3F_CLOSURE["inventory_sha256"],
        "stage5e_b3f_plan": STAGE5F_B3F_CLOSURE["plan_sha256"],
        "stage5e_b3f_production_ui_harness": "8a43aed8bfed494ac224f415e7ebc0fcd0773394aa17374539da58a0d22d637d",
        "stage5e_b3f_provenance_negative_harness": "126c0d65451233e6b142c88b8d36c38eb072c2ade0c7ed164edf1ff77cdef41f",
    }
    for key, expected in expected_b3f_input_hashes.items():
        if input_sha256.get(key) != expected:
            raise SystemExit(f"handoff safety: accepted B3F input pin drift: {key}")
    for member, expected in [
        ("crates/strategy-runtime-core/src/stage5c_paper_host.rs", STAGE5F_B3F_CLOSURE["stage5c_source_sha256"]),
        ("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", STAGE5F_B3F_CLOSURE["stage5e_source_sha256"]),
    ]:
        if hashlib.sha256(archive.read(member)).hexdigest() != expected:
            raise SystemExit(f"handoff safety: accepted B3F source pin drift: {member}")
    design_scope = gate_result.get("design_scope")
    expected_design_keys = {
        "baseline_ref",
        "changed_paths",
        "changed_paths_sha256",
        "head_tree",
        "source_ref",
    }
    if not isinstance(design_scope, dict) or set(design_scope) != expected_design_keys:
        raise SystemExit("handoff safety: Stage 5F design scope key set drift")
    require_hex64(manifest.get("stage5f_design_scope_sha256"), "stage5f_design_scope_sha256")
    if canonical_sha256(design_scope) != manifest.get("stage5f_design_scope_sha256"):
        raise SystemExit("handoff safety: Stage 5F design scope hash mismatch")
    if design_scope.get("baseline_ref") != STAGE5F_BASELINE_REF:
        raise SystemExit("handoff safety: Stage 5F design scope baseline mismatch")
    if not isinstance(design_scope.get("source_ref"), str) or not HEX40.fullmatch(design_scope["source_ref"]):
        raise SystemExit("handoff safety: Stage 5F design scope source_ref invalid")
    if not isinstance(design_scope.get("head_tree"), str) or not HEX40.fullmatch(design_scope["head_tree"]):
        raise SystemExit("handoff safety: Stage 5F design scope head_tree invalid")
    changed_paths = design_scope.get("changed_paths")
    if not isinstance(changed_paths, list) or not all(isinstance(item, str) for item in changed_paths):
        raise SystemExit("handoff safety: Stage 5F changed_paths must be a string list")
    if len(changed_paths) != len(set(changed_paths)):
        raise SystemExit("handoff safety: Stage 5F changed_paths contains duplicates")
    if hashlib.sha256(
        json.dumps(changed_paths, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest() != design_scope.get("changed_paths_sha256"):
        raise SystemExit("handoff safety: Stage 5F changed_paths hash mismatch")
    if changed_paths != expected_allowed_paths:
        raise SystemExit("handoff safety: Stage 5F changed-path set mismatch")
    stage5f_negative = check_stage5f_negative_result(archive, manifest)
    all_negative_results = {**heavy_negative_results, "stage5f": stage5f_negative}
    check_stage5f_source_tree_binding(
        archive,
        manifest,
        gate_result,
        cargo_result,
        provenance_result,
        all_negative_results,
    )
    return gate_result


def check_source_tree(root: Path) -> None:
    for path in root.rglob("*"):
        relative = PurePosixPath(path.relative_to(root).as_posix())
        if path_is_excluded(relative):
            continue
        if path.is_symlink():
            raise SystemExit(f"handoff safety: included symlink in source tree: {relative}")
        if path.is_file():
            check_payload(str(relative), path.read_bytes())
    print("handoff-source-safety: ok")


def check_archive(path: Path) -> None:
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise SystemExit("handoff safety: duplicate ZIP entries")
        for info in archive.infolist():
            pure = PurePosixPath(info.filename)
            if pure.is_absolute() or ".." in pure.parts:
                raise SystemExit(f"handoff safety: unsafe ZIP path: {info.filename}")
            if path_is_excluded(pure):
                raise SystemExit(f"handoff safety: excluded artifact in ZIP: {info.filename}")
            mode = info.external_attr >> 16
            if stat.S_ISLNK(mode):
                raise SystemExit(f"handoff safety: symlink in ZIP: {info.filename}")
            if not info.is_dir():
                check_payload(info.filename, archive.read(info))

        required = {
            "handoff-commit.txt",
            "handoff-cargo-gate-result.json",
            "handoff-cargo-gate-stderr.txt",
            "handoff-cargo-gate-stdout.txt",
            "handoff-forbidden-negative-result.json",
            "handoff-forbidden-negative-stderr.txt",
            "handoff-forbidden-negative-stdout.txt",
            "handoff-manifest.json",
            "handoff-provenance-negative-result.json",
            "handoff-provenance-negative-stderr.txt",
            "handoff-provenance-negative-stdout.txt",
            "handoff-stage5d-negative-result.json",
            "handoff-stage5d-negative-stderr.txt",
            "handoff-stage5d-negative-stdout.txt",
            "handoff-source-tree-manifest.json",
        }
        missing = sorted(required - set(names))
        if missing:
            raise SystemExit(f"handoff safety: missing generated markers: {missing}")
        try:
            manifest = json.loads(archive.read("handoff-manifest.json"))
        except json.JSONDecodeError as exc:
            raise SystemExit(f"handoff safety: malformed handoff manifest JSON: {exc}") from exc
        if not isinstance(manifest, dict):
            raise SystemExit("handoff safety: handoff manifest must be a JSON object")
        if manifest.get("schema_version") != 1:
            raise SystemExit("handoff safety: unsupported handoff manifest schema_version")
        cargo_result_name = "handoff-cargo-gate-result.json"
        cargo_stdout_name = "handoff-cargo-gate-stdout.txt"
        cargo_stderr_name = "handoff-cargo-gate-stderr.txt"
        cargo_result = json.loads(archive.read(cargo_result_name))
        if set(cargo_result) != {
            "cargo_version", "commands", "exit_code", "finished_at_utc", "schema_version",
            "source_ref", "started_at_utc", "stderr_member", "stderr_sha256", "stdout_member",
            "stdout_sha256", "source_tree_manifest_sha256", "source_tree_member_count",
        }:
            raise SystemExit("handoff safety: cargo gate result key set drift")
        if cargo_result.get("schema_version") != 1 or cargo_result.get("exit_code") != 0:
            raise SystemExit("handoff safety: cargo gate did not pass")
        if cargo_result.get("commands") != [
            ["cargo", "fmt", "--check"],
            ["cargo", "test", "--workspace", "--all-targets"],
            ["cargo", "test", "--workspace", "--doc"],
            ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        ]:
            raise SystemExit("handoff safety: cargo gate commands mismatch")
        if not isinstance(cargo_result.get("cargo_version"), str) or not cargo_result["cargo_version"]:
            raise SystemExit("handoff safety: cargo gate version missing")
        parse_utc_timestamp(cargo_result.get("started_at_utc"), "cargo started_at_utc")
        parse_utc_timestamp(cargo_result.get("finished_at_utc"), "cargo finished_at_utc")
        if cargo_result.get("stdout_member") != cargo_stdout_name or cargo_result.get("stderr_member") != cargo_stderr_name:
            raise SystemExit("handoff safety: cargo gate log member mismatch")
        for field, member in [("stdout_sha256", cargo_stdout_name), ("stderr_sha256", cargo_stderr_name)]:
            require_hex64(cargo_result.get(field), f"cargo gate {field}")
            if hashlib.sha256(archive.read(member)).hexdigest() != cargo_result[field]:
                raise SystemExit(f"handoff safety: cargo gate {field} mismatch")
        require_hex64(manifest.get("cargo_gate_result_sha256"), "cargo_gate_result_sha256")
        if hashlib.sha256(archive.read(cargo_result_name)).hexdigest() != manifest.get("cargo_gate_result_sha256"):
            raise SystemExit("handoff safety: cargo gate result hash mismatch")
        provenance_result_name = "handoff-provenance-negative-result.json"
        provenance_stdout_name = "handoff-provenance-negative-stdout.txt"
        provenance_stderr_name = "handoff-provenance-negative-stderr.txt"
        provenance_result = json.loads(archive.read(provenance_result_name))
        stage5f_provenance_snapshot = (
            manifest.get("current_review_stage") == STAGE5F_ENTRY_STAGE
        )
        expected_provenance_keys = {
            "command", "exit_code", "finished_at_utc", "gate_id", "passed_cases",
            "schema_version", "source_ref", "source_tree_manifest_sha256",
            "source_tree_member_count", "started_at_utc", "stderr_member", "stderr_sha256",
            "stdout_member", "stdout_sha256",
        }
        if stage5f_provenance_snapshot:
            expected_provenance_keys.add("tested_source_ref")
        if not isinstance(provenance_result, dict) or set(provenance_result) != expected_provenance_keys:
            raise SystemExit("handoff safety: provenance-negative result key set drift")
        if provenance_result.get("schema_version") != 1 or provenance_result.get("gate_id") != "handoff_provenance_negative":
            raise SystemExit("handoff safety: provenance-negative result identity mismatch")
        if provenance_result.get("command") != ["python3", "scripts/handoff_provenance_negative_harness.py"]:
            raise SystemExit("handoff safety: provenance-negative command mismatch")
        if (
            provenance_result.get("exit_code") != 0
            or not isinstance(provenance_result.get("passed_cases"), int)
            or provenance_result["passed_cases"] <= 0
        ):
            raise SystemExit("handoff safety: provenance-negative gate did not pass")
        if stage5f_provenance_snapshot and (
            provenance_result.get("tested_source_ref") != STAGE5F_BASELINE_REF
            or provenance_result.get("passed_cases") != 580
        ):
            raise SystemExit("handoff safety: accepted B3F provenance snapshot mismatch")
        parse_utc_timestamp(provenance_result.get("started_at_utc"), "provenance-negative started_at_utc")
        parse_utc_timestamp(provenance_result.get("finished_at_utc"), "provenance-negative finished_at_utc")
        if provenance_result.get("stdout_member") != provenance_stdout_name or provenance_result.get("stderr_member") != provenance_stderr_name:
            raise SystemExit("handoff safety: provenance-negative log member mismatch")
        for field, member in [("stdout_sha256", provenance_stdout_name), ("stderr_sha256", provenance_stderr_name)]:
            require_hex64(provenance_result.get(field), f"provenance-negative {field}")
            if hashlib.sha256(archive.read(member)).hexdigest() != provenance_result[field]:
                raise SystemExit(f"handoff safety: provenance-negative {field} mismatch")
        if archive.read(provenance_stdout_name).count(b"PASS ") != provenance_result["passed_cases"]:
            raise SystemExit("handoff safety: provenance-negative passed case count mismatch")
        require_hex64(manifest.get("provenance_negative_result_sha256"), "provenance_negative_result_sha256")
        if hashlib.sha256(archive.read(provenance_result_name)).hexdigest() != manifest.get("provenance_negative_result_sha256"):
            raise SystemExit("handoff safety: provenance-negative result hash mismatch")
        heavy_negative_results = {}
        for prefix, gate_id, command, expected_cases, manifest_field in [
            (
                "stage5d",
                "stage5d_additive_freeze_negative",
                ["python3", "scripts/stage5d_additive_freeze_negative_harness.py"],
                303,
                "stage5d_negative_result_sha256",
            ),
            (
                "forbidden",
                "forbidden_surface_negative",
                ["bash", "scripts/forbidden_surface_negative_harness.sh"],
                87,
                "forbidden_negative_result_sha256",
            ),
        ]:
            result_name = f"handoff-{prefix}-negative-result.json"
            stdout_name = f"handoff-{prefix}-negative-stdout.txt"
            stderr_name = f"handoff-{prefix}-negative-stderr.txt"
            result = json.loads(archive.read(result_name))
            if set(result) != {
                "command", "exit_code", "finished_at_utc", "gate_id", "passed_cases",
                "schema_version", "source_ref", "source_tree_manifest_sha256",
                "source_tree_member_count", "started_at_utc", "stderr_member", "stderr_sha256",
                "stdout_member", "stdout_sha256",
            }:
                raise SystemExit(f"handoff safety: {prefix}-negative result key set drift")
            if (
                result.get("schema_version") != 1
                or result.get("gate_id") != gate_id
                or result.get("command") != command
                or result.get("exit_code") != 0
                or result.get("passed_cases") != expected_cases
            ):
                raise SystemExit(f"handoff safety: {prefix}-negative gate did not pass")
            parse_utc_timestamp(result.get("started_at_utc"), f"{prefix}-negative started_at_utc")
            parse_utc_timestamp(result.get("finished_at_utc"), f"{prefix}-negative finished_at_utc")
            if (
                result.get("stdout_member") != stdout_name
                or result.get("stderr_member") != stderr_name
            ):
                raise SystemExit(f"handoff safety: {prefix}-negative log member mismatch")
            for field, member in [
                ("stdout_sha256", stdout_name),
                ("stderr_sha256", stderr_name),
            ]:
                require_hex64(result.get(field), f"{prefix}-negative {field}")
                if hashlib.sha256(archive.read(member)).hexdigest() != result[field]:
                    raise SystemExit(f"handoff safety: {prefix}-negative {field} mismatch")
            if archive.read(stdout_name).count(b"PASS ") != expected_cases:
                raise SystemExit(f"handoff safety: {prefix}-negative passed case count mismatch")
            require_hex64(manifest.get(manifest_field), manifest_field)
            if hashlib.sha256(archive.read(result_name)).hexdigest() != manifest[manifest_field]:
                raise SystemExit(f"handoff safety: {prefix}-negative result hash mismatch")
            heavy_negative_results[prefix] = result
        review_stage = manifest.get("review_stage")
        if not isinstance(review_stage, str) or not review_stage:
            raise SystemExit("handoff safety: missing review_stage")
        archive_name = manifest.get("archive_name")
        if not isinstance(archive_name, str) or not archive_name:
            raise SystemExit("handoff safety: missing archive_name")
        stage5d_manifest_name = "docs/stage-5/stage-5d-additive-freeze-manifest.json"
        stage5d_manifest = json.loads(archive.read(stage5d_manifest_name))
        if review_stage != stage5d_manifest.get("stage"):
            raise SystemExit("handoff safety: review_stage/freeze-stage mismatch")
        for field, member in [
            ("stage5c_checker_sha256", "scripts/stage5c_api_freeze_check.py"),
            ("stage5d_checker_sha256", "scripts/stage5d_additive_freeze_check.py"),
            ("stage5d_manifest_sha256", stage5d_manifest_name),
        ]:
            expected = manifest.get(field)
            if not isinstance(expected, str) or not re.fullmatch(r"[0-9a-f]{64}", expected):
                raise SystemExit(f"handoff safety: missing or invalid {field}")
            actual = hashlib.sha256(archive.read(member)).hexdigest()
            if actual != expected:
                raise SystemExit(f"handoff safety: {field} mismatch")
        current_review_stage = manifest.get("current_review_stage")
        stage5e_declared = any(
            key in manifest
            for key in [
                "stage5e_checker_sha256",
                "stage5e_inventory_sha256",
                "stage5e_plan_sha256",
                "stage5e_gate_result_sha256",
                "stage5e_design_scope_sha256",
            ]
        )
        stage5f_declared = any(
            key in manifest
            for key in [
                "stage5f_checker_sha256",
                "stage5f_inventory_sha256",
                "stage5f_plan_sha256",
                "stage5f_gate_result_sha256",
                "stage5f_negative_result_sha256",
                "stage5f_design_scope_sha256",
            ]
        )
        if stage5e_declared and stage5f_declared:
            raise SystemExit("handoff safety: mixed Stage 5E/Stage 5F provenance")
        if stage5e_declared and (
            not isinstance(current_review_stage, str)
            or not current_review_stage.startswith("5E-")
        ):
            raise SystemExit("handoff safety: current_review_stage/Stage 5E inventory mismatch")
        if stage5f_declared and current_review_stage != STAGE5F_ENTRY_STAGE:
            raise SystemExit("handoff safety: current_review_stage/Stage 5F inventory mismatch")
        stage5f_gate_result: dict[str, object] | None = None
        if isinstance(current_review_stage, str) and current_review_stage.startswith("5E-"):
            active_descriptor_name = "docs/stage-5/stage5e-active-descriptor.json"
            if active_descriptor_name not in names:
                raise SystemExit("handoff safety: missing active Stage 5E descriptor")
            active_descriptor = json.loads(archive.read(active_descriptor_name))
            if set(active_descriptor) != {"schema_version", "stage"} or active_descriptor.get("schema_version") != 1:
                raise SystemExit("handoff safety: active Stage 5E descriptor schema mismatch")
            try:
                selected = descriptor_for_stage(active_descriptor.get("stage"))
            except ValueError as exc:
                raise SystemExit(f"handoff safety: {exc}") from exc
            if selected["stage"] != current_review_stage:
                raise SystemExit("handoff safety: current_review_stage/Stage 5E inventory mismatch")
            stage5e_inventory_name = selected["inventory"]
            stage5e_plan_name = selected["plan"]
            stage5e_checker_name = selected["checker"]
            if current_review_stage == "5E-b-no-io-lifecycle-capability":
                expected_stage5e_baseline_ref = "0ffeb6aefe790efeaa6d99157104bd5aef8ff35e"
                expected_stage5e_a_freeze_ref = "eb03695dc407b02bb8327de57fde6acea077d96b"
            elif current_review_stage == "5E-b3-schedule-window-evidence":
                # b3 retains the Stage 5D aggregate closure as its lineage root,
                # but its scoped review diff begins at the accepted b2.1 seal.
                expected_stage5e_baseline_ref = "04431096e269daaf9715e253b2354b1ac8fcc3e8"
                expected_stage5e_a_freeze_ref = None
            elif current_review_stage == "5E-b3c-private-eligibility-seam":
                expected_stage5e_baseline_ref = "95861577ce3acc11963104bb5a313a82f6f82bdb"
                expected_stage5e_a_freeze_ref = None
            elif current_review_stage == "5E-b3c-source-authority-freeze-extension":
                # R6 was reviewed from this exact authority-freeze baseline.
                expected_stage5e_baseline_ref = "2b2c57d7bacb8e3f1de572b7c35790be906b82a9"
                expected_stage5e_a_freeze_ref = None
            elif current_review_stage == "5E-b3d-callback-authority-design":
                expected_stage5e_baseline_ref = (
                    "ff1344f170b8457df91a6038d670087eef3cc1dc"
                )
                expected_stage5e_a_freeze_ref = None
            elif current_review_stage == "5E-b3e-callback-invocation-design":
                expected_stage5e_baseline_ref = (
                    "529d8e42946bb8bebad3cbf5e8fca2727dd95a07"
                )
                expected_stage5e_a_freeze_ref = None
            elif current_review_stage == "5E-b3f-callback-settlement-escrow-design":
                expected_stage5e_baseline_ref = (
                    "a5ccea08bc64a66e768340f7121e9b94a09ff884"
                )
                expected_stage5e_a_freeze_ref = None
            else:
                expected_stage5e_baseline_ref = "9ebbfd29d0346be5149dac746225866f0c8d0257"
                expected_stage5e_a_freeze_ref = None
            stage5e_gate_result_name = "handoff-stage5e-gate-result.json"
            stage5e_stdout_name = "handoff-stage5e-gate-stdout.txt"
            stage5e_stderr_name = "handoff-stage5e-gate-stderr.txt"
            source_tree_manifest_name = "handoff-source-tree-manifest.json"
            for member in [
                stage5e_inventory_name,
                stage5e_plan_name,
                stage5e_checker_name,
                active_descriptor_name,
                stage5e_gate_result_name,
                stage5e_stdout_name,
                stage5e_stderr_name,
                source_tree_manifest_name,
            ]:
                if member not in names:
                    raise SystemExit(f"handoff safety: missing Stage 5E member: {member}")
            for field, member in [
                ("stage5e_checker_sha256", stage5e_checker_name),
                ("stage5e_inventory_sha256", stage5e_inventory_name),
                ("stage5e_plan_sha256", stage5e_plan_name),
                ("stage5e_gate_result_sha256", stage5e_gate_result_name),
                ("source_tree_manifest_sha256", source_tree_manifest_name),
            ]:
                expected = manifest.get(field)
                if not isinstance(expected, str) or not HEX64.fullmatch(expected):
                    raise SystemExit(f"handoff safety: missing or invalid {field}")
                actual = hashlib.sha256(archive.read(member)).hexdigest()
                if actual != expected:
                    raise SystemExit(f"handoff safety: {field} mismatch")
            stage5e_inventory = json.loads(archive.read(stage5e_inventory_name))
            if not isinstance(stage5e_inventory, dict):
                raise SystemExit("handoff safety: Stage 5E inventory must be a JSON object")
            if current_review_stage != stage5e_inventory.get("stage"):
                raise SystemExit("handoff safety: current_review_stage/Stage 5E inventory mismatch")
            if current_review_stage == "5E-b-no-io-lifecycle-capability":
                if set(stage5e_inventory) != STAGE5E_B_TOP_LEVEL_KEYS:
                    raise SystemExit("handoff safety: Stage 5E-b inventory key set drift")
                if stage5e_inventory.get("allowed_changed_paths") != STAGE5E_B_ALLOWED_CHANGED_PATHS:
                    raise SystemExit("handoff safety: Stage 5E-b allowed_changed_paths drift")
            if (
                current_review_stage != "5E-b3c-source-authority-freeze-extension"
                and stage5e_inventory.get("source_stage5d_aggregate_closure_r2_ref")
                != "9ebbfd29d0346be5149dac746225866f0c8d0257"
            ):
                raise SystemExit("handoff safety: Stage 5E source baseline ref mismatch")
            if stage5e_inventory.get("baseline_ref") != expected_stage5e_baseline_ref:
                raise SystemExit("handoff safety: Stage 5E baseline_ref mismatch")
            expected_provenance_case_count = stage5e_inventory.get(
                "expected_provenance_case_count"
            )
            if (
                expected_provenance_case_count is not None
                and provenance_result.get("passed_cases")
                != expected_provenance_case_count
            ):
                raise SystemExit(
                    "handoff safety: Stage 5E provenance-negative case count mismatch"
                )
            if expected_stage5e_a_freeze_ref is not None and stage5e_inventory.get("stage5e_a_freeze_ref") != expected_stage5e_a_freeze_ref:
                raise SystemExit("handoff safety: Stage 5E-a freeze ref mismatch")
            closed = stage5e_inventory.get("closed_surfaces")
            if current_review_stage == "5E-b3c-source-authority-freeze-extension":
                expected_closed = [
                    "strategy_callback", "strategy_state_mutation", "executable_intents",
                    "strategy_intent_sink", "redis", "finam_io", "transport", "dispatch",
                    "runtime_live", "broker_execution", "autonomous_event_loop",
                ]
                if closed != expected_closed:
                    raise SystemExit("handoff safety: Stage 5E extension closed-surface mismatch")
            elif current_review_stage in {
                "5E-b3e-callback-invocation-design",
                "5E-b3f-callback-settlement-escrow-design",
            }:
                opened_private_surfaces = {
                    "actual_callback_invocation",
                    "strategy_state_mutation",
                    "in_memory_intent_construction",
                }
                expected_surface_names = {
                        "actual_callback_invocation",
                        "strategy_state_mutation",
                        "in_memory_intent_construction",
                        "escrow_validation_or_settlement",
                        "intent_extraction_or_sink",
                        "executable_intents",
                        "redis",
                        "finam_io",
                        "transport",
                        "dispatch",
                        "runtime_live",
                        "broker_execution",
                        "autonomous_event_loop",
                        "schedule_provider_attachment",
                        "venue_calendar_inference",
                    }
                if current_review_stage == "5E-b3f-callback-settlement-escrow-design":
                    expected_surface_names.update(
                        {"durable_persistence", "crash_restart_recovery"}
                    )
                    transition = stage5e_inventory.get("transition_contract")
                    if (
                        stage5e_inventory.get("accepted_b3f_r4_ref")
                        != "148377f71b4afe6ce20f4e42433f58c812fb4917"
                        or not isinstance(transition, dict)
                        or transition.get("implementation_status")
                        != "implemented_private_process_local_pending_review"
                        or transition.get(
                            "settlement_implementation_allowed_in_this_stage"
                        )
                        is not True
                    ):
                        raise SystemExit(
                            "handoff safety: Stage 5E B3F implementation authority mismatch"
                        )
                    opened_private_surfaces.add("escrow_validation_or_settlement")
                if (
                    not isinstance(closed, dict)
                    or set(closed) != expected_surface_names
                    or any(closed[key] is not True for key in opened_private_surfaces)
                    or any(
                        value is not False
                        for key, value in closed.items()
                        if key not in opened_private_surfaces
                    )
                ):
                    raise SystemExit("handoff safety: Stage 5E B3E surface mismatch")
            elif not isinstance(closed, dict) or any(value is not False for value in closed.values()):
                raise SystemExit("handoff safety: Stage 5E closed-surface mismatch")
            gate_result = json.loads(archive.read(stage5e_gate_result_name))
            if not isinstance(gate_result, dict):
                raise SystemExit("handoff safety: Stage 5E gate result must be a JSON object")
            expected_gate_keys = {
                "command",
                "design_scope",
                "exit_code",
                "finished_at_utc",
                "gate_id",
                "input_sha256",
                "schema_version",
                "source_ref",
                "source_tree_manifest_sha256",
                "source_tree_member_count",
                "started_at_utc",
                "stderr_member",
                "stderr_line_count",
                "stderr_sha256",
                "stdout_member",
                "stdout_line_count",
                "stdout_sha256",
            }
            if set(gate_result) != expected_gate_keys:
                raise SystemExit("handoff safety: Stage 5E gate result key set drift")
            if gate_result.get("schema_version") != 1:
                raise SystemExit("handoff safety: unsupported Stage 5E gate result schema_version")
            if gate_result.get("gate_id") != "stage5e_lifecycle_event_time":
                raise SystemExit("handoff safety: Stage 5E gate result id mismatch")
            if gate_result.get("command") != ["bash", "scripts/stage5e_lifecycle_event_time_gate.sh"]:
                raise SystemExit("handoff safety: Stage 5E gate command mismatch")
            if gate_result.get("exit_code") != 0:
                raise SystemExit("handoff safety: Stage 5E gate did not pass")
            started_at = parse_utc_timestamp(gate_result.get("started_at_utc"), "started_at_utc")
            finished_at = parse_utc_timestamp(gate_result.get("finished_at_utc"), "finished_at_utc")
            if finished_at < started_at:
                raise SystemExit("handoff safety: Stage 5E gate timestamp order invalid")
            require_hex64(gate_result.get("stdout_sha256"), "Stage 5E gate stdout_sha256")
            require_hex64(gate_result.get("stderr_sha256"), "Stage 5E gate stderr_sha256")
            if gate_result.get("stdout_member") != stage5e_stdout_name:
                raise SystemExit("handoff safety: Stage 5E gate stdout member mismatch")
            if gate_result.get("stderr_member") != stage5e_stderr_name:
                raise SystemExit("handoff safety: Stage 5E gate stderr member mismatch")
            if hashlib.sha256(archive.read(stage5e_stdout_name)).hexdigest() != gate_result.get(
                "stdout_sha256"
            ):
                raise SystemExit("handoff safety: Stage 5E gate stdout hash mismatch")
            if hashlib.sha256(archive.read(stage5e_stderr_name)).hexdigest() != gate_result.get(
                "stderr_sha256"
            ):
                raise SystemExit("handoff safety: Stage 5E gate stderr hash mismatch")
            for field in ["stdout_line_count", "stderr_line_count"]:
                if not isinstance(gate_result.get(field), int) or gate_result[field] < 0:
                    raise SystemExit(f"handoff safety: invalid Stage 5E gate {field}")
            input_sha256 = gate_result.get("input_sha256")
            if not isinstance(input_sha256, dict):
                raise SystemExit("handoff safety: Stage 5E gate input hashes must be an object")
            expected_input_keys = {
                "stage5c_checker",
                "stage5d_checker",
                "stage5d_manifest",
                "stage5e_active_descriptor",
                "stage5e_checker",
                "stage5e_descriptor_registry",
                "stage5e_inventory",
                "stage5e_plan",
            }
            if set(input_sha256) != expected_input_keys:
                raise SystemExit("handoff safety: Stage 5E gate input hash key set drift")
            for key, manifest_field, member in [
                ("stage5c_checker", "stage5c_checker_sha256", "scripts/stage5c_api_freeze_check.py"),
                ("stage5d_checker", "stage5d_checker_sha256", "scripts/stage5d_additive_freeze_check.py"),
                ("stage5d_manifest", "stage5d_manifest_sha256", stage5d_manifest_name),
                ("stage5e_active_descriptor", None, active_descriptor_name),
                ("stage5e_checker", "stage5e_checker_sha256", stage5e_checker_name),
                ("stage5e_descriptor_registry", None, "scripts/stage5e_descriptor.py"),
                ("stage5e_inventory", "stage5e_inventory_sha256", stage5e_inventory_name),
                ("stage5e_plan", "stage5e_plan_sha256", stage5e_plan_name),
            ]:
                value = input_sha256.get(key)
                require_hex64(value, f"Stage 5E gate input hash {key}")
                if manifest_field is not None and value != manifest.get(manifest_field):
                    raise SystemExit(f"handoff safety: Stage 5E gate input/manifest mismatch: {key}")
                actual = hashlib.sha256(archive.read(member)).hexdigest()
                if value != actual:
                    raise SystemExit(f"handoff safety: Stage 5E gate input/archive mismatch: {key}")
            design_scope = gate_result.get("design_scope")
            if not isinstance(design_scope, dict):
                raise SystemExit("handoff safety: Stage 5E design scope must be an object")
            expected_design_keys = {
                "baseline_ref",
                "changed_paths",
                "changed_paths_sha256",
                "head_tree",
                "source_ref",
            }
            if set(design_scope) != expected_design_keys:
                raise SystemExit("handoff safety: Stage 5E design scope key set drift")
            require_hex64(manifest.get("stage5e_design_scope_sha256"), "stage5e_design_scope_sha256")
            if canonical_sha256(design_scope) != manifest.get("stage5e_design_scope_sha256"):
                raise SystemExit("handoff safety: Stage 5E design scope hash mismatch")
            if design_scope.get("baseline_ref") != stage5e_inventory.get("baseline_ref"):
                raise SystemExit("handoff safety: Stage 5E design scope baseline mismatch")
            if not isinstance(design_scope.get("source_ref"), str) or not HEX40.fullmatch(design_scope["source_ref"]):
                raise SystemExit("handoff safety: Stage 5E design scope source_ref invalid")
            if not isinstance(design_scope.get("head_tree"), str) or not HEX40.fullmatch(design_scope["head_tree"]):
                raise SystemExit("handoff safety: Stage 5E design scope head_tree invalid")
            changed_paths = design_scope.get("changed_paths")
            if not isinstance(changed_paths, list) or not all(isinstance(item, str) for item in changed_paths):
                raise SystemExit("handoff safety: Stage 5E changed_paths must be a string list")
            if len(changed_paths) != len(set(changed_paths)):
                raise SystemExit("handoff safety: Stage 5E changed_paths contains duplicates")
            if hashlib.sha256(
                json.dumps(changed_paths, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest() != design_scope.get("changed_paths_sha256"):
                raise SystemExit("handoff safety: Stage 5E changed_paths hash mismatch")
            allowed = stage5e_inventory.get("allowed_changed_paths")
            if not isinstance(allowed, list) or not set(changed_paths).issubset(set(allowed)):
                raise SystemExit("handoff safety: Stage 5E design scope allowlist mismatch")
            if changed_paths != allowed:
                raise SystemExit("handoff safety: Stage 5E design scope changed-path set mismatch")
            source_tree_manifest = json.loads(archive.read(source_tree_manifest_name))
            if not isinstance(source_tree_manifest, dict):
                raise SystemExit("handoff safety: source-tree manifest must be a JSON object")
            expected_source_tree_keys = {
                "baseline_ref",
                "changed_paths",
                "excluded_generated_members",
                "head_tree",
                "members",
                "schema_version",
                "source_ref",
            }
            if set(source_tree_manifest) != expected_source_tree_keys:
                raise SystemExit("handoff safety: source-tree manifest key set drift")
            if source_tree_manifest.get("schema_version") != 1:
                raise SystemExit("handoff safety: unsupported source-tree manifest schema_version")
            if source_tree_manifest.get("source_ref") != gate_result.get("source_ref"):
                raise SystemExit("handoff safety: source-tree manifest source_ref mismatch")
            if source_tree_manifest.get("head_tree") != design_scope.get("head_tree"):
                raise SystemExit("handoff safety: source-tree manifest head_tree mismatch")
            if source_tree_manifest.get("baseline_ref") != design_scope.get("baseline_ref"):
                raise SystemExit("handoff safety: source-tree manifest baseline_ref mismatch")
            if source_tree_manifest.get("changed_paths") != changed_paths:
                raise SystemExit("handoff safety: source-tree manifest changed_paths mismatch")
            if gate_result.get("source_tree_manifest_sha256") != manifest.get(
                "source_tree_manifest_sha256"
            ):
                raise SystemExit("handoff safety: gate/source-tree manifest hash mismatch")
            if gate_result.get("source_tree_manifest_sha256") != hashlib.sha256(
                archive.read(source_tree_manifest_name)
            ).hexdigest():
                raise SystemExit("handoff safety: source-tree manifest hash mismatch")
            generated = source_tree_manifest.get("excluded_generated_members")
            if not isinstance(generated, list) or not all(isinstance(item, str) for item in generated):
                raise SystemExit("handoff safety: source-tree generated member list invalid")
            if set(generated) != {
                "handoff-commit.txt",
                "handoff-cargo-gate-result.json",
                "handoff-cargo-gate-stderr.txt",
                "handoff-cargo-gate-stdout.txt",
                "handoff-forbidden-negative-result.json",
                "handoff-forbidden-negative-stderr.txt",
                "handoff-forbidden-negative-stdout.txt",
                "handoff-manifest.json",
                "handoff-provenance-negative-result.json",
                "handoff-provenance-negative-stderr.txt",
                "handoff-provenance-negative-stdout.txt",
                "handoff-stage5d-negative-result.json",
                "handoff-stage5d-negative-stderr.txt",
                "handoff-stage5d-negative-stdout.txt",
                "handoff-stage5e-gate-result.json",
                "handoff-stage5e-gate-stderr.txt",
                "handoff-stage5e-gate-stdout.txt",
                "handoff-source-tree-manifest.json",
            }:
                raise SystemExit("handoff safety: source-tree generated member set drift")
            source_members = source_tree_manifest.get("members")
            if not isinstance(source_members, list):
                raise SystemExit("handoff safety: source-tree members must be a list")
            source_member_map: dict[str, tuple[str, str]] = {}
            for row in source_members:
                if not isinstance(row, dict) or set(row) != {"git_mode", "path", "sha256"}:
                    raise SystemExit("handoff safety: source-tree member row key set drift")
                member_path = row["path"]
                member_sha = row["sha256"]
                git_mode = row["git_mode"]
                if not isinstance(member_path, str) or not member_path:
                    raise SystemExit("handoff safety: source-tree member path invalid")
                if git_mode not in {"100644", "100755"}:
                    raise SystemExit("handoff safety: source-tree member git_mode invalid")
                require_hex64(member_sha, f"source-tree member sha256 {member_path}")
                if member_path in source_member_map:
                    raise SystemExit("handoff safety: duplicate source-tree member")
                source_member_map[member_path] = (git_mode, member_sha)
            if gate_result.get("source_tree_member_count") != len(source_member_map):
                raise SystemExit("handoff safety: source-tree member count mismatch")
            if cargo_result.get("source_tree_manifest_sha256") != manifest.get("source_tree_manifest_sha256"):
                raise SystemExit("handoff safety: cargo gate/source-tree manifest mismatch")
            if cargo_result.get("source_tree_member_count") != len(source_member_map):
                raise SystemExit("handoff safety: cargo gate/source-tree member count mismatch")
            if provenance_result.get("source_ref") != gate_result.get("source_ref"):
                raise SystemExit("handoff safety: provenance-negative source_ref mismatch")
            if provenance_result.get("source_tree_manifest_sha256") != manifest.get("source_tree_manifest_sha256"):
                raise SystemExit("handoff safety: provenance-negative/source-tree manifest mismatch")
            if provenance_result.get("source_tree_member_count") != len(source_member_map):
                raise SystemExit("handoff safety: provenance-negative source-tree member count mismatch")
            for prefix, result in heavy_negative_results.items():
                if result.get("source_ref") != gate_result.get("source_ref"):
                    raise SystemExit(f"handoff safety: {prefix}-negative source_ref mismatch")
                if (
                    result.get("source_tree_manifest_sha256")
                    != manifest.get("source_tree_manifest_sha256")
                ):
                    raise SystemExit(
                        f"handoff safety: {prefix}-negative/source-tree manifest mismatch"
                    )
                if result.get("source_tree_member_count") != len(source_member_map):
                    raise SystemExit(
                        f"handoff safety: {prefix}-negative source-tree member count mismatch"
                    )
            archive_files = {
                info.filename
                for info in archive.infolist()
                if not info.is_dir()
            }
            expected_archive_files = set(source_member_map) | set(generated)
            if archive_files != expected_archive_files:
                raise SystemExit("handoff safety: source-tree/archive member set mismatch")
            git_entries: dict[str, tuple[str, bytes]] = {}
            for member_path, (git_mode, expected_sha) in source_member_map.items():
                payload = archive.read(member_path)
                if hashlib.sha256(payload).hexdigest() != expected_sha:
                    raise SystemExit(f"handoff safety: source-tree member hash mismatch: {member_path}")
                git_entries[member_path] = (git_mode, git_blob_sha1(payload))
            if git_tree_sha1(git_entries) != design_scope.get("head_tree"):
                raise SystemExit("handoff safety: source-tree head_tree mismatch")
        elif current_review_stage == STAGE5F_ENTRY_STAGE:
            stage5f_gate_result = check_stage5f_archive(
                archive,
                names,
                manifest,
                cargo_result,
                provenance_result,
                heavy_negative_results,
                stage5d_manifest_name,
            )
        source_commit = manifest.get("source_commit")
        source_ref = manifest.get("source_ref")
        if not isinstance(source_commit, str) or not re.fullmatch(
            r"[0-9a-f]{7,12}", source_commit
        ):
            raise SystemExit("handoff safety: missing or invalid source_commit")
        if not isinstance(source_ref, str) or not re.fullmatch(r"[0-9a-f]{40}", source_ref):
            raise SystemExit("handoff safety: missing or invalid source_ref")
        if cargo_result.get("source_ref") != source_ref:
            raise SystemExit("handoff safety: cargo gate source_ref mismatch")
        if not source_ref.startswith(source_commit):
            raise SystemExit("handoff safety: source short/full commit mismatch")
        marker = archive.read("handoff-commit.txt").decode().splitlines()
        expected_marker = [
            f"source_commit={source_commit}",
            f"source_ref={source_ref}",
            f"archive_name={archive_name}",
        ]
        if marker != expected_marker or archive_name != path.name:
            raise SystemExit("handoff safety: provenance marker/manifest mismatch")
        if isinstance(current_review_stage, str) and current_review_stage.startswith("5E-"):
            gate_result = json.loads(archive.read("handoff-stage5e-gate-result.json"))
            if gate_result.get("source_ref") != source_ref:
                raise SystemExit("handoff safety: Stage 5E gate source_ref mismatch")
            if gate_result.get("design_scope", {}).get("source_ref") != source_ref:
                raise SystemExit("handoff safety: Stage 5E design scope source_ref mismatch")
        elif current_review_stage == STAGE5F_ENTRY_STAGE:
            if stage5f_gate_result is None:
                raise SystemExit("handoff safety: Stage 5F gate result missing")
            if stage5f_gate_result.get("source_ref") != source_ref:
                raise SystemExit("handoff safety: Stage 5F gate source_ref mismatch")
            if stage5f_gate_result.get("design_scope", {}).get("source_ref") != source_ref:
                raise SystemExit("handoff safety: Stage 5F design scope source_ref mismatch")
    print("handoff-archive-safety: ok")


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--source-tree", type=Path)
    group.add_argument("--archive", type=Path)
    args = parser.parse_args()
    if args.source_tree:
        check_source_tree(args.source_tree.resolve())
    else:
        check_archive(args.archive.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
