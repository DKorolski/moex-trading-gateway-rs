#!/usr/bin/env python3
"""Fail-closed GOV-CI-1B authority for the current source tree."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import stat
from pathlib import Path
from typing import Any, Optional


AUTHORITY = Path("docs/stage-8/gov-ci-1-authority.json")
CONTRACT = Path("docs/stage-8/GOV_CI_1_CURRENT_TREE_AUTHORITY_2026-08-21.md")
MATRIX = Path("docs/stage-8/GOV_CI_1_ACCEPTANCE_MATRIX_2026-08-21.csv")
NEGATIVE = Path("docs/stage-8/GOV_CI_1_NEGATIVE_INVENTORY_2026-08-21.md")
CURRENT_WORKFLOW = Path(".github/workflows/ci.yml")
HISTORICAL_WORKFLOW = Path(".github/workflows/stage5f-base-authority.yml")
GATE = Path("scripts/current_tree_ci_gate.sh")
NEGATIVE_HARNESS = Path("scripts/current_tree_authority_negative_harness.py")
HANDOFF_SAFETY = Path("scripts/gov_ci_1_handoff_safety_check.py")
HANDOFF_MAKER = Path("scripts/make_gov_ci_1_handoff.py")

ACCEPTED_PREDECESSOR = "1dea519cbf2affc3d99866fdae66bbddbafefa24"
ACCEPTED_STAGE8A5_REF = "bf58b47fdef8af774a4107455dfcc6204e594283"
ACCEPTED_STAGE8A5_GATE_SHA256 = (
    "1361ad49d41351484cf61c86822deb640818e755b7b35bda44592fd437ff69f8"
)
ALLOWED_WORKFLOWS = {CURRENT_WORKFLOW.as_posix(), HISTORICAL_WORKFLOW.as_posix()}
HISTORICAL_CURRENT_TREE_MARKERS = (
    "bash scripts/forbidden_surface_scan.sh",
    "bash scripts/forbidden_surface_negative_harness.sh",
    "python3 scripts/stage5d_additive_freeze_negative_harness.py",
    "python3 scripts/stage5f_ci_snapshot_inheritance_check.py",
    "bash scripts/stage5f_atomic_hybrid_semantics_gate.sh",
)


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git_mode(path: Path) -> str:
    return "100755" if path.stat().st_mode & stat.S_IXUSR else "100644"


def production_files(root: Path) -> list[Path]:
    paths = [root / "Cargo.toml", root / "Cargo.lock"]
    paths.extend(
        path
        for path in (root / "crates").rglob("*")
        if path.is_file() and (path.name == "Cargo.toml" or path.suffix == ".rs")
    )
    return sorted(set(paths), key=lambda path: path.relative_to(root).as_posix())


def file_inventory(root: Path, paths: list[Path]) -> dict[str, dict[str, Any]]:
    return {
        path.relative_to(root).as_posix(): {
            "mode": git_mode(path),
            "sha256": sha256(path.read_bytes()),
            "size": path.stat().st_size,
        }
        for path in paths
    }


def inventory_digest(entries: dict[str, dict[str, Any]]) -> str:
    digest = hashlib.sha256()
    for path, entry in sorted(entries.items()):
        digest.update(
            path.encode()
            + b"\0"
            + str(entry["mode"]).encode()
            + b"\0"
            + str(entry["sha256"]).encode()
            + b"\0"
            + str(entry["size"]).encode()
            + b"\n"
        )
    return digest.hexdigest()


def content_digest(entries: dict[str, dict[str, Any]]) -> str:
    """Retain the Stage 8B-D R1 path/content fingerprint for lineage comparison."""
    digest = hashlib.sha256()
    for path, entry in sorted(entries.items()):
        digest.update(path.encode() + b"\0" + str(entry["sha256"]).encode() + b"\n")
    return digest.hexdigest()


def validate_inventory(
    root: Path,
    expected: dict[str, Any],
    actual_paths: list[Path],
    label: str,
) -> None:
    actual = file_inventory(root, actual_paths)
    require(expected.get("file_count") == len(actual), f"{label} file-count drift")
    require(expected.get("entries") == actual, f"{label} entry drift")
    require(
        expected.get("aggregate_sha256") == inventory_digest(actual),
        f"{label} aggregate drift",
    )
    if label == "production":
        require(expected.get("content_sha256") == content_digest(actual), "production content digest drift")


def workflow_trigger_block(text: str) -> str:
    match = re.search(r"(?ms)^on:\s*\n(?P<body>.*?)(?=^jobs:\s*$)", text)
    require(match is not None, "workflow trigger block missing")
    return "\n".join(
        line.split("#", 1)[0]
        for line in match.group("body").splitlines()
        if line.split("#", 1)[0].strip()
    )


def feature_defaults(path: Path) -> tuple[list[str], set[str]]:
    section: Optional[str] = None
    defaults: Optional[list[str]] = None
    names: set[str] = set()
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        section_match = re.fullmatch(r"\[([^]]+)\]", line)
        if section_match:
            section = section_match.group(1)
            continue
        if section != "features" or "=" not in line:
            continue
        name, value = (part.strip() for part in line.split("=", 1))
        names.add(name)
        if name == "default":
            parsed = json.loads(value)
            require(
                isinstance(parsed, list) and all(isinstance(item, str) for item in parsed),
                f"invalid default feature list: {path}",
            )
            defaults = parsed
    require(defaults is not None, f"default feature declaration missing: {path}")
    return defaults, names


def require_false_accessor(root: Path, relative: str, name: str) -> None:
    text = (root / relative).read_text(encoding="utf-8")
    pattern = rf"pub fn {re.escape(name)}\(&self\) -> bool \{{\s*false\s*\}}"
    require(re.search(pattern, text) is not None, f"closed accessor opened: {name}")


def check(root: Path) -> None:
    required = (
        AUTHORITY, CONTRACT, MATRIX, NEGATIVE, CURRENT_WORKFLOW, HISTORICAL_WORKFLOW,
        GATE, NEGATIVE_HARNESS, HANDOFF_SAFETY, HANDOFF_MAKER,
    )
    for relative in required:
        require((root / relative).is_file(), f"missing current authority artifact: {relative}")

    authority = json.loads((root / AUTHORITY).read_text(encoding="utf-8"))
    require(authority.get("schema_version") == 2, "authority schema drift")
    require(authority.get("stage") == "GOV-CI-1B", "authority stage drift")
    require(authority.get("status") == "independent_review_required", "authority status drift")
    require(authority.get("accepted_predecessor") == ACCEPTED_PREDECESSOR, "predecessor drift")

    replay = authority.get("accepted_stage8a5_replay", {})
    require(replay.get("source_ref") == ACCEPTED_STAGE8A5_REF, "Stage 8A-5 replay ref drift")
    require(replay.get("gate_path") == "scripts/stage8a5_gate.sh", "Stage 8A-5 gate drift")
    require(replay.get("gate_sha256") == ACCEPTED_STAGE8A5_GATE_SHA256, "Stage 8A-5 gate digest drift")

    for key in (
        "current_tree_authority_required",
        "current_tree_negative_harness_required",
        "accepted_stage8a5_immutable_replay_required",
        "workspace_debug_release_doc_clippy_required",
        "redis_regression_smoke_required",
        "handoff_complete_logs_required",
    ):
        require(authority.get("requirements", {}).get(key) is True, f"requirement drift: {key}")
    expected_closed = {
        "stage8b_s_authorized", "finam_post_delete_enabled", "broker_execution_enabled",
        "redis_live_consumer_enabled", "redis_xadd_xack_enabled", "runtime_live_enabled",
        "real_orders_enabled",
    }
    require(set(authority.get("closed_surfaces", {})) == expected_closed, "closed-surface inventory drift")
    for key, value in authority["closed_surfaces"].items():
        require(value is False, f"closed governance surface opened: {key}")

    production = production_files(root)
    validate_inventory(root, authority["production_code_manifest"], production, "production")
    control_paths = [root / path for path in authority["governance_control_plane_manifest"]["entries"]]
    require(all(path.is_file() for path in control_paths), "governance control-plane file missing")
    validate_inventory(root, authority["governance_control_plane_manifest"], control_paths, "governance control-plane")

    workflow_paths = {
        path.relative_to(root).as_posix()
        for path in (root / ".github/workflows").iterdir()
        if path.is_file() and path.suffix in {".yml", ".yaml"}
    }
    require(workflow_paths == ALLOWED_WORKFLOWS, f"active workflow inventory drift: {workflow_paths}")
    require(
        authority.get("active_workflow_manifest", {}).get("paths") == sorted(ALLOWED_WORKFLOWS),
        "authority workflow inventory drift",
    )
    current_workflow = (root / CURRENT_WORKFLOW).read_text(encoding="utf-8")
    historical_workflow = (root / HISTORICAL_WORKFLOW).read_text(encoding="utf-8")
    current_triggers = workflow_trigger_block(current_workflow)
    historical_triggers = workflow_trigger_block(historical_workflow)
    require("pull_request:" in current_triggers, "canonical CI pull_request trigger missing")
    require("push:" in current_triggers and "- main" in current_triggers, "canonical CI main push missing")
    require("pull_request_target" not in current_triggers, "canonical CI pull_request_target forbidden")
    require("workflow_dispatch:" in historical_triggers, "historical manual trigger missing")
    for forbidden in ("pull_request_target", "pull_request:", "push:"):
        require(forbidden not in historical_triggers, f"historical authority reactivated: {forbidden}")

    gate = (root / GATE).read_text(encoding="utf-8")
    for marker in HISTORICAL_CURRENT_TREE_MARKERS:
        require(marker not in current_workflow and marker not in gate, f"historical authority restored: {marker}")
    for command in (
        "run: bash scripts/current_tree_ci_gate.sh",
        "run: bash scripts/test_m4_3x_evidence_no_redis.sh",
        "run: cargo fmt --all --check",
        "run: cargo test --workspace --all-targets -- --test-threads=1",
        "run: cargo test --workspace --release --all-targets -- --test-threads=1",
        "run: cargo test --workspace --doc",
        "run: cargo clippy --workspace --all-targets --all-features -- -D warnings",
        "run: scripts/redis_shadow_smoke.sh",
        "run: scripts/runtime_bridge_dry_smoke.sh",
    ):
        require(current_workflow.count(command) == 1, f"mandatory workflow command drift: {command}")
    for command in (
        "python3 scripts/current_tree_authority_check.py",
        "python3 scripts/current_tree_authority_negative_harness.py",
        f'accepted_stage8a5_ref="{ACCEPTED_STAGE8A5_REF}"',
        'bash "$replay_root/repo/scripts/stage8a5_gate.sh"',
    ):
        require(gate.count(command) == 1, f"mandatory gate command drift: {command}")

    for crate in ("crates/broker-cli/Cargo.toml", "crates/finam-gateway/Cargo.toml"):
        defaults, names = feature_defaults(root / crate)
        require(defaults == [], f"default execution feature opened: {crate}")
        require("m3j16-actual-one-shot" in names, f"legacy feature declaration drift: {crate}")
    for accessor in (
        "redis_command_consumer_attached",
        "finam_transport_attached",
        "broker_network_dispatch_attached",
        "runtime_live_attached",
        "real_orders_enabled",
    ):
        require_false_accessor(root, "crates/strategy-runtime-core/src/stage6d_live_core.rs", accessor)

    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    expected_rows = authority.get("acceptance_rows")
    require(len(rows) == expected_rows, "GOV-CI-1B matrix row-count drift")
    require(
        [row["id"] for row in rows] == [f"GOV-CI-{index:03d}" for index in range(1, expected_rows + 1)],
        "acceptance matrix ID drift",
    )
    negative_text = (root / NEGATIVE).read_text(encoding="utf-8")
    negative_count = len(re.findall(r"(?m)^\d+\. GOV-CI-N\d+", negative_text))
    require(negative_count == authority.get("negative_cases"), "negative inventory count drift")
    contract = (root / CONTRACT).read_text(encoding="utf-8")
    for marker in (
        "GOV-CI-1A", "GOV-CI-1B", ACCEPTED_PREDECESSOR, ACCEPTED_STAGE8A5_REF,
        "Stage 8B-D R2", "FINAM POST/DELETE", "runtime-live",
    ):
        require(marker in contract, f"governance contract marker missing: {marker}")

    print(
        "current-tree-authority-check: PASS "
        f"production_files={len(production)} "
        f"production_fingerprint={authority['production_code_manifest']['aggregate_sha256']} "
        f"control_files={len(control_paths)} workflows={len(workflow_paths)} "
        f"negative_cases={negative_count} stage8b_s=false finam=false redis_live=false runtime_live=false"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    check(args.root.resolve())


if __name__ == "__main__":
    try:
        main()
    except (KeyError, OSError, RuntimeError, json.JSONDecodeError) as error:
        raise SystemExit(f"current-tree-authority-check: FAIL {error}") from error
