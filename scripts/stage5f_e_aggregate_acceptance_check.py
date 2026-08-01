#!/usr/bin/env python3
"""Fail-closed authority and scope checker for Stage 5F-e aggregate closure."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


DEFAULT_ROOT = Path(__file__).resolve().parents[1]
STAGE = "5F-e-aggregate-acceptance"
ACCEPTED_D = "1a41b530419d39ddc84fff81a9dfdde6ede878ce"
PLAN = "docs/stage-5/5f-e-acceptance-report.md"
INVENTORY = "docs/stage-5/stage5f-final-scenario-inventory.json"
D_INVENTORY = "docs/stage-5/stage5f-d-scenario-inventory.json"
D_GOLDEN = "docs/stage-5/stage5f-d-golden-results.json"
D_CHECKER = "scripts/stage5f_d_atomic_matrix_check.py"
EXPECTED_PLAN_SHA256 = "7a73127488fcec1155114fe194ad0da07f7c8b5dd368dd7943ed0697895af15b"
EXPECTED_INVENTORY_SHA256 = "92330d1b54ff8a88ae437f6c43d894c35a0ea58f93195ded4edb63e2f5723136"
EXPECTED_ACCEPTED_D = {
    "source_ref": ACCEPTED_D,
    "archive_name": "moex-trading-project-1a41b53.zip",
    "archive_sha256": "18d7944264ade10ea2f0860b861a7176ba98fe5d82c9beaf1cbcd22b72e5b2b3",
    "review_record_sha256": "3ffeb72698a472f7857b2b430ead81560c886fb77f6a4d3a64e501253b271eec",
    "verdict": "ACCEPTED",
}
EXPECTED_TARGET = {
    "strategy_id": "hybrid_imoexf",
    "account_id": "ACC_TEST_0001",
    "symbol": "IMOEXF",
    "timeframe_sec": 600,
    "paper_only": True,
}
EXPECTED_ROWS = [f"F{ordinal:02}" for ordinal in range(1, 35)]
EXPECTED_GROUPS = [
    "G01_NO_SIGNAL",
    "G02_BO_LONG",
    "G03_BO_SHORT",
    "G04_BO_EXIT",
    "G05_BO_EOD",
    "G06_MR_LONG",
    "G07_MR_SHORT",
    "G08_MR_TIME",
    "G09_MR_TARGET",
    "G10_MR_STOP",
    "G11_ARBITRATION",
    "G12_OWNER_CYCLE",
    "G13_RISKGATE_NORMAL",
    "G14_RISKGATE_BLOCK",
    "G15_PENDING_DEFERRED",
    "G16_TERMINAL",
]
EXPECTED_SUMMARY = {
    "row_count": 34,
    "group_count": 16,
    "slice_counts": {"5F-d1": 15, "5F-d2": 7, "5F-d3": 12},
    "accepted": 26,
    "structural_invariant": 1,
    "blocked_before_callback": 3,
    "terminal_after_callback": 4,
}
EXPECTED_GATE_LABELS = [
    "aggregate-checker",
    "aggregate-negative",
    "fmt",
    "workspace-tests",
    "doctests",
    "clippy",
    "matrix-checker",
    "matrix-negative",
    "matrix-debug",
    "matrix-release",
    "matrix-default-parallel",
    "matrix-reproducibility",
    "r3-snapshot",
    "inherited-b1",
    "inherited-b3f",
    "inherited-b3f-ui",
    "stage5c-freeze",
    "stage5d-freeze",
    "stage5d-negative",
    "forbidden-no-rg",
    "redis-smoke",
    "functional",
]
EXPECTED_REPORTS = [
    "reports/stage5f/stage5f-acceptance-result.json",
    "reports/stage5f/stage5f-fingerprint-reproducibility.json",
    "reports/stage5f/stage5f-negative-result.json",
]
EXPECTED_CLOSED_SURFACES = {
    "redis_command_consumption": False,
    "finam_transport": False,
    "http_post_delete": False,
    "dispatch": False,
    "broker_execution": False,
    "runtime_live": False,
    "real_orders": False,
    "ack_order_trade_position_timer_restart_feedback": False,
    "protective_order_lifecycle": False,
    "stage5g_authorized": False,
}
EXPECTED_ALLOWED_CHANGES = [
    "docs/stage-5/5f-e-acceptance-report.md",
    "docs/stage-5/stage5f-final-scenario-inventory.json",
    "scripts/make_stage5f_e_handoff_archive.py",
    "scripts/stage5f_b3f_snapshot_ui_gate.sh",
    "scripts/stage5f_e_aggregate_acceptance_check.py",
    "scripts/stage5f_e_aggregate_acceptance_negative_harness.py",
    "scripts/stage5f_e_handoff_safety_check.py",
    "scripts/stage5f_e_redis_regression_gate.sh",
    "scripts/stage5f_e_reproducibility.py",
    "scripts/stage5f_forbidden_no_rg_gate.sh",
    "scripts/stage5f_stage5d_snapshot_gate.sh",
]
EXPECTED_RESULTS_ARRAY_SHA256 = (
    "e85f15912e3dd97e2a41a3d2617bc9b560769aa964e158b0129bb0d2c89e0f17"
)


class CheckFailure(RuntimeError):
    pass


def fail(message: str) -> None:
    raise CheckFailure(message)


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            fail(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(root: Path, relative: str) -> dict[str, Any]:
    try:
        value = json.loads(
            (root / relative).read_text(encoding="utf-8"),
            object_pairs_hook=strict_object,
        )
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot parse {relative}: {exc}")
    if not isinstance(value, dict):
        fail(f"{relative} must contain an object")
    return value


def sha256(root: Path, relative: str) -> str:
    try:
        return hashlib.sha256((root / relative).read_bytes()).hexdigest()
    except OSError as exc:
        fail(f"cannot hash {relative}: {exc}")


def require(actual: object, expected: object, label: str) -> None:
    if actual != expected:
        fail(f"{label}: expected {expected!r}, got {actual!r}")


def exact_keys(value: object, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        fail(f"{label} key-set drift")
    return value


def validate_lineage(root: Path) -> None:
    try:
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", ACCEPTED_D, "HEAD"],
            cwd=root,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        changed = set(
            subprocess.check_output(
                ["git", "diff", "--name-only", ACCEPTED_D, "--"],
                cwd=root,
                text=True,
            ).splitlines()
        )
        untracked = set(
            subprocess.check_output(
                ["git", "ls-files", "--others", "--exclude-standard"],
                cwd=root,
                text=True,
            ).splitlines()
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        fail(f"cannot verify accepted Stage 5F-d lineage: {exc}")
    require(sorted(changed | untracked), EXPECTED_ALLOWED_CHANGES, "5F-e changed paths")
    if any(path.endswith(".rs") for path in changed | untracked):
        fail("Stage 5F-e must not change Rust sources")


def validate_d_checker(root: Path) -> None:
    completed = subprocess.run(
        [
            sys.executable,
            str(root / D_CHECKER),
            "--root",
            str(root),
            "--skip-lineage",
        ],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        fail("accepted Stage 5F-d checker failed: " + completed.stderr.strip())
    if "stage5f-d-atomic-matrix-check: ok rows=34 groups=16 golden=true" not in completed.stdout:
        fail("accepted Stage 5F-d checker marker missing")


def validate_authority(root: Path, inventory: dict[str, Any]) -> None:
    authority = exact_keys(
        inventory["authority_freeze"],
        {"documents", "fixtures", "source_bindings", "gate_bindings"},
        "authority freeze",
    )
    for group in ("documents", "fixtures", "source_bindings", "gate_bindings"):
        bindings = authority[group]
        if not isinstance(bindings, dict) or not bindings:
            fail(f"authority group {group} must be non-empty")
        for path, expected in bindings.items():
            require(sha256(root, path), expected, f"frozen authority {path}")

    d_inventory = read_json(root, D_INVENTORY)
    d_golden = read_json(root, D_GOLDEN)
    require(d_inventory["target"], EXPECTED_TARGET, "Stage 5F-d target")
    require(d_inventory["summary"], EXPECTED_SUMMARY, "Stage 5F-d summary")
    require(d_inventory["groups"], EXPECTED_GROUPS, "Stage 5F-d groups")
    require(
        [row.get("row_id") for row in d_inventory["rows"]],
        EXPECTED_ROWS,
        "Stage 5F-d row order",
    )
    require(
        d_inventory["source_bindings"],
        authority["source_bindings"],
        "source-binding freeze",
    )
    require(
        d_inventory["authorities"]["golden_results"]["sha256"],
        authority["documents"][D_GOLDEN],
        "golden inventory binding",
    )
    require(
        d_golden["generation"]["results_array_sha256"],
        EXPECTED_RESULTS_ARRAY_SHA256,
        "golden results-array binding",
    )
    require(
        [row.get("row_id") for row in d_golden["results"]],
        EXPECTED_ROWS,
        "golden result order",
    )
    validate_d_checker(root)


def validate_plan(root: Path) -> None:
    require(sha256(root, PLAN), EXPECTED_PLAN_SHA256, "Stage 5F-e report SHA-256")
    text = (root / PLAN).read_text(encoding="utf-8")
    for fragment in (
        "Stage 5F-e does not rewrite the accepted Stage 5F-d candidate artifacts.",
        "accepted Stage 5F semantic evidence",
        "!= production/live strategy golden authorization",
        "three independent focused matrix executions",
        "580/580",
        "87/87 with `rg`",
        "Stage 5G is not authorized",
    ):
        if fragment not in text:
            fail(f"acceptance report fragment missing: {fragment}")


def validate(root: Path, *, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    validate_plan(root)
    require(
        sha256(root, INVENTORY),
        EXPECTED_INVENTORY_SHA256,
        "final inventory SHA-256",
    )
    inventory = exact_keys(
        read_json(root, INVENTORY),
        {
            "schema_version",
            "stage",
            "status",
            "accepted_stage5f_d",
            "target",
            "matrix",
            "authority_freeze",
            "closure_contract",
            "allowed_changes_since_accepted_stage5f_d",
        },
        "final inventory",
    )
    require(inventory["schema_version"], 1, "inventory schema")
    require(inventory["stage"], STAGE, "inventory stage")
    require(inventory["status"], "aggregate_review_candidate", "inventory status")
    require(inventory["accepted_stage5f_d"], EXPECTED_ACCEPTED_D, "accepted 5F-d")
    require(inventory["target"], EXPECTED_TARGET, "target")

    matrix = exact_keys(
        inventory["matrix"],
        {
            "row_count",
            "group_count",
            "row_ids",
            "group_ids",
            "slice_counts",
            "dispositions",
            "results_array_sha256",
        },
        "matrix",
    )
    require(matrix["row_count"], 34, "row count")
    require(matrix["group_count"], 16, "group count")
    require(matrix["row_ids"], EXPECTED_ROWS, "row ids")
    require(matrix["group_ids"], EXPECTED_GROUPS, "group ids")
    require(matrix["slice_counts"], EXPECTED_SUMMARY["slice_counts"], "slice counts")
    require(
        matrix["dispositions"],
        {key: EXPECTED_SUMMARY[key] for key in (
            "accepted",
            "structural_invariant",
            "blocked_before_callback",
            "terminal_after_callback",
        )},
        "disposition summary",
    )
    require(
        matrix["results_array_sha256"],
        EXPECTED_RESULTS_ARRAY_SHA256,
        "results array SHA-256",
    )

    closure = exact_keys(
        inventory["closure_contract"],
        {
            "minimum_reproducibility_runs",
            "required_reports",
            "required_gate_labels",
            "closed_surfaces",
        },
        "closure contract",
    )
    require(closure["minimum_reproducibility_runs"], 3, "reproducibility runs")
    require(closure["required_reports"], EXPECTED_REPORTS, "required reports")
    require(closure["required_gate_labels"], EXPECTED_GATE_LABELS, "required gates")
    require(closure["closed_surfaces"], EXPECTED_CLOSED_SURFACES, "closed surfaces")
    require(
        inventory["allowed_changes_since_accepted_stage5f_d"],
        EXPECTED_ALLOWED_CHANGES,
        "allowed Stage 5F-e paths",
    )
    validate_authority(root, inventory)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--skip-lineage", action="store_true")
    args = parser.parse_args()
    try:
        validate(args.root.resolve(), verify_lineage=not args.skip_lineage)
    except (
        CheckFailure,
        KeyError,
        TypeError,
        ValueError,
        OSError,
        subprocess.SubprocessError,
    ) as exc:
        print(f"stage5f-e-aggregate-acceptance-check: FAIL: {exc}", file=sys.stderr)
        return 1
    print("stage5f-e-aggregate-acceptance-check: ok rows=34 groups=16 frozen=true")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
