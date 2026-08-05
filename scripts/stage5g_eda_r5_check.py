#!/usr/bin/env python3
"""Whole-source and execution-boundary seal for Stage 5G-e-d-a R5."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path

import stage5g_eda_r3_check as r3
import stage5g_eda_r4_check as r4


SOURCE = r4.SOURCE
LIB = r4.LIB
CONTRACT = r4.CONTRACT
DESIGN = r4.DESIGN
INVARIANTS = r4.INVARIANTS
LIFECYCLE_INVENTORY = r4.LIFECYCLE_INVENTORY
STATUS = r4.STATUS
ONBOARDING = r4.ONBOARDING
R4_FREEZE = r4.FREEZE
SOURCE_FREEZE = Path("docs/stage-5/stage5g-e-d-a-r5-runtime-core-source-freeze.json")
GATE = Path("scripts/stage5g_eda_r5_gate.sh")
NEGATIVE = Path("scripts/stage5g_eda_r5_negative_harness.py")
PRESEAL = Path("scripts/stage5g_eda_r5_preseal_check.py")
BUILDER = Path("scripts/make_stage5g_ed_handoff_archive.py")
R3_NEGATIVE = Path("scripts/stage5g_eda_r3_negative_harness.py")
R4_NEGATIVE = Path("scripts/stage5g_eda_r4_negative_harness.py")
R4_REF = "49357a2d49d45ab6f5f9cb8b3f0e11dfb6b97c30"
SOURCE_ROOT = Path("crates/strategy-runtime-core/src")
SOURCE_FILE_COUNT = 19
SOURCE_MANIFEST_SHA256 = "5d1896345d55a6603cf447fee196acfc44ea43eb5c0106ddbaf38dc412f684d1"
TARGET_FULL_LINES = 1698
TARGET_FULL_BYTES = 66717
TARGET_FULL_SHA256 = "286ba28553f3202f403e32f97a307e39f2c6694b7d566e3b84298c84ca63e44b"
GATE_SHA256 = "579404712c5bf392fc093655c9e40f2c62dce2dbea73a549093e7c31d15fe828"
BUILDER_SHA256 = "1a6cc0b9ec6292617cf2fea897729e50c5b53c6379db469a42ba6b2b5fef4bba"

EXPECTED_GATE_COMMANDS = [
    "python3 scripts/stage5g_eda_r5_check.py",
    "python3 scripts/stage5g_eda_r5_negative_harness.py",
    "python3 scripts/stage5g_eda_r5_preseal_check.py",
    "cargo fmt --all -- --check",
    "cargo test -p strategy-runtime-core --lib stage5g_fresh_broker_truth",
    "cargo test --release -p strategy-runtime-core --lib stage5g_fresh_broker_truth",
    "cargo test -p strategy-runtime-core --lib",
    "cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings",
]

R5_MUTATIONS = {
    "append-direct-after-test-reducer",
    "append-alias-after-test-reducer",
    "append-production-item-after-test",
    "add-sibling-reducer-module",
    "add-reducer-to-existing-sibling",
    "gate-drop-current-checker",
    "gate-drop-negative-harness",
    "gate-drop-preseal",
    "gate-drop-fmt",
    "gate-drop-focused-debug",
    "gate-drop-focused-release",
    "gate-drop-full-core",
    "gate-drop-clippy",
    "gate-drop-detached-r4",
    "builder-drop-clean-tree-check",
    "builder-drop-branch-check",
    "builder-drop-parent-check",
    "builder-drop-origin-check",
    "builder-ignore-gate-failure",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage5g-eda-r5-check: FAIL: {message}")


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage5g-eda-r5-check: FAIL: cannot load {path}: {error}") from error
    require(isinstance(value, dict), f"{path} must contain an object")
    return value


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def source_rows(root: Path) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    for path in sorted((root / SOURCE_ROOT).rglob("*.rs")):
        payload = path.read_bytes()
        rows.append({
            "path": path.relative_to(root).as_posix(),
            "size": len(payload),
            "sha256": sha256_bytes(payload),
        })
    return rows


def canonical_manifest_sha256(rows: list[dict[str, object]]) -> str:
    canonical = json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()
    return sha256_bytes(canonical)


def assert_source_freeze(root: Path) -> None:
    freeze = load_json(root / SOURCE_FREEZE)
    expected_keys = {
        "schema_version", "stage", "base_commit", "source_glob", "source_file_count",
        "canonicalization", "canonical_manifest_sha256", "target_source_path",
        "target_full_line_count", "target_full_byte_count", "target_full_sha256",
        "accepted_production_prefix_sha256", "runtime_core_rust_source_byte_frozen",
        "implemented_restart_case_ids", "stage5g_e_d_b_open", "files",
    }
    require(set(freeze) == expected_keys, "R5 source-freeze key set drifted")
    require(freeze["schema_version"] == 1, "R5 source-freeze schema drifted")
    require(freeze["stage"] == "5G-e-d-a-r5", "R5 source-freeze stage drifted")
    require(freeze["base_commit"] == R4_REF, "R5 source-freeze base drifted")
    require(freeze["source_glob"] == "crates/strategy-runtime-core/src/**/*.rs",
            "R5 source glob drifted")
    require(freeze["source_file_count"] == SOURCE_FILE_COUNT, "R5 source count drifted")
    require(freeze["canonicalization"] ==
            "json.dumps(files, sort_keys=True, separators=(',', ':')).encode('utf-8')",
            "R5 source manifest canonicalization drifted")
    require(freeze["canonical_manifest_sha256"] == SOURCE_MANIFEST_SHA256,
            "R5 source manifest commitment drifted")
    require(freeze["target_source_path"] == str(SOURCE), "R5 target path drifted")
    require(freeze["target_full_line_count"] == TARGET_FULL_LINES,
            "R5 target line count drifted")
    require(freeze["target_full_byte_count"] == TARGET_FULL_BYTES,
            "R5 target byte count drifted")
    require(freeze["target_full_sha256"] == TARGET_FULL_SHA256,
            "R5 target SHA-256 drifted")
    require(freeze["accepted_production_prefix_sha256"] == r4.PRODUCTION_PREFIX_SHA256,
            "R5 inherited production-prefix SHA-256 drifted")
    require(freeze["runtime_core_rust_source_byte_frozen"] is True,
            "R5 runtime-core source freeze disabled")
    require(freeze["implemented_restart_case_ids"] == [], "R5 claims GRST execution")
    require(freeze["stage5g_e_d_b_open"] is False, "R5 opens e-d-b")

    rows = freeze["files"]
    require(isinstance(rows, list), "R5 source manifest files must be a list")
    require(len(rows) == SOURCE_FILE_COUNT, "R5 source manifest must have 19 files")
    require(canonical_manifest_sha256(rows) == SOURCE_MANIFEST_SHA256,
            "R5 compact source manifest commitment mismatch")
    actual = source_rows(root)
    require(actual == rows, "runtime-core Rust source set/hash/size drifted")

    target = (root / SOURCE).read_bytes()
    require(len(target) == TARGET_FULL_BYTES, "target source byte count drifted")
    require(target.count(b"\n") == TARGET_FULL_LINES, "target source line count drifted")
    require(sha256_bytes(target) == TARGET_FULL_SHA256, "target full-file SHA-256 drifted")
    prefix = r4.production_prefix(target.decode())
    r4.assert_alias_aware_authority_closed(prefix)


def mutation_names(path: Path) -> set[str]:
    return set(re.findall(r'\("([^"]+)",\s*lambda root:', path.read_text()))


def assert_contract_and_inventory(root: Path) -> None:
    r4_freeze = load_json(root / R4_FREEZE)
    require(r4_freeze == r4.EXPECTED_FREEZE, "inherited R4 prefix freeze drifted")
    contract = load_json(root / CONTRACT)
    require(contract.get("status") == "r5_final_gate_review_candidate", "R5 status drifted")
    require(contract.get("rejected_r4_commit") == R4_REF, "R4 base binding drifted")
    require(contract.get("runtime_core_source_freeze_manifest") == SOURCE_FREEZE.name,
            "R5 source-freeze pointer drifted")
    require(contract.get("contract") == r4.EXPECTED_CONTRACT_SHAPE, "exact contract map drifted")
    require(contract.get("closed_surfaces") == r4.EXPECTED_CLOSED_SURFACES,
            "exact closed-surface map drifted")
    require(contract.get("implemented_restart_case_ids") == [], "e-d-b implementation claimed")
    require(contract.get("dispositions") == r3.EXPECTED_DISPOSITIONS,
            "contract dispositions drifted")
    require(contract.get("operational_identity_fields") == r3.EXPECTED_OPERATIONAL_FIELDS,
            "contract operational fields drifted")
    scenarios = contract.get("restart_scenarios")
    require(isinstance(scenarios, list), "restart scenarios missing")
    require([row.get("id") for row in scenarios] == r3.EXPECTED_GRST_IDS,
            "contract GRST IDs/order drifted")
    require(contract.get("restart_scenario_count") == 12, "restart scenario count drifted")

    lifecycle = load_json(root / LIFECYCLE_INVENTORY)
    restart = next((row for row in lifecycle.get("scenario_families", [])
                    if row.get("id") == "RESTART"), None)
    require(restart is not None, "RESTART lifecycle family missing")
    require(restart.get("case_ids") == r3.EXPECTED_GRST_IDS,
            "lifecycle GRST IDs/order drifted")

    inventory = load_json(root / INVARIANTS)
    rows = inventory.get("invariants")
    require(isinstance(rows, list), "invariant inventory must be a list")
    expected_ids = r3.EXPECTED_INVARIANT_IDS + r4.R4_INVARIANT_IDS
    require([row.get("invariant_id") for row in rows] == expected_ids,
            "R4 invariant inventory drifted")
    require(inventory.get("stage") == "5G-e-d-a-r4", "semantic inventory stage drifted")
    require(inventory.get("base_commit") == r4.R3_REF, "semantic inventory base drifted")
    source = (root / SOURCE).read_text()
    prefix = r4.production_prefix(source)
    inherited = mutation_names(root / R3_NEGATIVE)
    r4_names = mutation_names(root / R4_NEGATIVE)
    r5_names = mutation_names(root / NEGATIVE)
    all_names = inherited | r4_names | r5_names
    require(len(inherited) == 56, "R3 mutation inventory no longer has 56 cases")
    require(len(r4_names) == 23, "R4 mutation inventory no longer has 23 cases")
    require(R5_MUTATIONS <= r5_names, "mandatory R5 mutation missing")
    require(len(all_names) >= 98, "R5 negative matrix has fewer than 98 cases")
    require(len(all_names) == len(inherited) + len(r4_names) + len(r5_names),
            "duplicate mutation names across R3/R4/R5")
    for row in rows:
        invariant_id = row["invariant_id"]
        require(row.get("production_anchor") in prefix,
                f"production anchor missing for {invariant_id}")
        require(row.get("focused_rust_witness") in source,
                f"focused witness missing for {invariant_id}")
        require(row.get("negative_mutation_id") in all_names,
                f"negative mutation missing for {invariant_id}")


def assert_exact_gate(root: Path) -> None:
    payload = (root / GATE).read_bytes()
    require(sha256_bytes(payload) == GATE_SHA256, "reviewed R5 gate script hash drifted")
    lines = [line.strip() for line in payload.decode().splitlines()]
    positions: list[int] = []
    for command in EXPECTED_GATE_COMMANDS:
        require(lines.count(command) == 1, f"gate command must occur exactly once: {command}")
        positions.append(lines.index(command))
    require(positions == sorted(positions), "R5 gate command order drifted")
    require(lines.count("bash scripts/stage5g_eda_r4_gate.sh") == 1,
            "detached R4 gate command must occur exactly once")
    require(f'r4_ref="{R4_REF}"' in lines, "detached R4 reference drifted")
    require('git worktree add --detach "$snapshot_root" "$r4_ref" >/dev/null' in lines,
            "R4 gate must run from detached worktree")


def assert_exact_builder(root: Path) -> None:
    payload = (root / BUILDER).read_bytes()
    require(sha256_bytes(payload) == BUILDER_SHA256, "reviewed R5 builder hash drifted")
    text = payload.decode()
    required = {
        'BRANCH = "stage5g-lifecycle"': 1,
        'STAGE = "5G-e-d-a-r5"': 1,
        f'REQUIRED_PARENT = "{R4_REF}"': 1,
        '["git", "status", "--porcelain", "--untracked-files=all"]': 1,
        'if branch != BRANCH:': 1,
        'if parent_ref != REQUIRED_PARENT:': 1,
        'if origin_ref != source_ref:': 1,
        '["bash", "scripts/stage5g_eda_r5_gate.sh"]': 2,
        'if gate.returncode != 0:': 1,
        '["git", "archive", "--format=tar", source_ref]': 1,
        'archive_sha256 = hashlib.sha256(archive_path.read_bytes()).hexdigest()': 1,
    }
    for anchor, count in required.items():
        require(text.count(anchor) == count, f"builder guard drifted: {anchor}")


def assert_docs(root: Path) -> None:
    design = (root / DESIGN).read_text()
    status = (root / STATUS).read_text()
    onboarding = (root / ONBOARDING).read_text()
    required_design = (
        "Primary current-HEAD gate: `bash scripts/stage5g_eda_r5_gate.sh`",
        "R5 is final e-d-a acceptance/gate closure only.",
        "All strategy-runtime-core Rust source remains byte-frozen to R4.",
        "implemented_restart_case_ids remains empty",
        "Stage 5G-e-d-b remains closed pending independent R5 acceptance",
    )
    for value in required_design:
        require(value in design, f"R5 design statement missing: {value}")
    require("Stage 5G-e-d-a R5" in status, "status R5 target missing")
    require("Stage 5G-e-d-b remains closed pending independent R5 acceptance" in status,
            "status e-d-b boundary missing")
    require("Stage 5G-e-d-a R5" in onboarding, "onboarding R5 target missing")
    require("Stage 5G-e-d-b remains closed pending independent R5 acceptance" in onboarding,
            "onboarding e-d-b boundary missing")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    root = parser.parse_args().root.resolve()
    files = (
        SOURCE, LIB, CONTRACT, DESIGN, INVARIANTS, LIFECYCLE_INVENTORY, STATUS,
        ONBOARDING, R4_FREEZE, SOURCE_FREEZE, GATE, NEGATIVE, PRESEAL, BUILDER,
        R3_NEGATIVE, R4_NEGATIVE,
    )
    for relative in files:
        require((root / relative).is_file(), f"missing {relative}")
    assert_source_freeze(root)
    assert_contract_and_inventory(root)
    assert_exact_gate(root)
    assert_exact_builder(root)
    assert_docs(root)
    print("stage5g-eda-r5-check: PASS")


if __name__ == "__main__":
    main()
