#!/usr/bin/env python3
"""Fail-closed governance/lineage checker for Stage 8A-5 aggregate acceptance."""

from __future__ import annotations

import argparse
import csv
import json
import subprocess
from pathlib import Path

PREDECESSOR = "4a11688c941ee240e377b384042c4bca837b040f"
BRANCH = "stage8a5-aggregate-acceptance"
EXPECTED = {
    "accepted_predecessor": PREDECESSOR,
    "accepted_stage7b": "a1044e0dbe324c722b637498ca80ffafd9f0cbee",
    "accepted_stage8a0": "c949d7f83aa87cf990204a5b8ae66e5ca37c9f1d",
    "accepted_stage8a1": "1ff04154ba4b7a5ee060a73b853ce89bd7442f44",
    "accepted_stage8a2": "16180ac4f8eab761b3b055c1f5515f62cd94bfb9",
    "accepted_stage8a3": "012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d",
    "accepted_stage8a4_reducer": "4caf07c16ddad021add7cffe6e887165e49e1bf0",
    "accepted_stage8a4_durable_design": "6ddf54ef9d7f740dc59cd2450e78301be3d068cb",
    "accepted_stage8a4_implementation_spec": "dd01253596527d6cff1db11cc32ae3c3348c96a0",
    "accepted_stage8a4_i1": "113d2827ef255e8d2c2597a3acb38fe52dd7e52d",
    "accepted_stage8a4_i2": "90f46052cc31cea012437eddb59fb7c3ca5c2320",
    "accepted_stage8a4_i3": "593ff255ef7826a22e66c9aff6f7ea47acf47644",
    "accepted_stage8a4_i4_design": "81727aae1f648f17961177fc9541e2483cbf07f2",
    "accepted_stage8a4_i4": PREDECESSOR,
    "accepted_i4_review_sha256": "0377879b5b10c38ef0740af54e3d2b341d980b21824490664d828e8a6d4e0046",
}


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def check(root: Path, git_scope: bool = True) -> None:
    authority_path = root / "docs/stage-8/stage8a5-aggregate-acceptance-authority.json"
    matrix_path = root / "docs/stage-8/STAGE8A5_AGGREGATE_ACCEPTANCE_MATRIX_2026-08-21.csv"
    contract_path = root / "docs/stage-8/STAGE8A5_AGGREGATE_ACCEPTANCE_2026-08-21.md"
    negative_path = root / "docs/stage-8/STAGE8A5_AGGREGATE_NEGATIVE_INVENTORY_2026-08-21.md"
    gate_path = root / "scripts/stage8a5_gate.sh"
    scanner_path = root / "scripts/stage8a5_forbidden_surface_check.py"
    inherited_path = root / "scripts/stage8a5_inherited_stage8_check.py"
    safety_path = root / "scripts/stage8a5_handoff_safety_check.py"
    maker_path = root / "scripts/make_stage8a5_handoff.py"
    detached_cargo_path = root / "scripts/stage8a5_detached_cargo.sh"
    for path in (
        authority_path, matrix_path, contract_path, negative_path, gate_path,
        scanner_path, inherited_path, safety_path, maker_path, detached_cargo_path,
    ):
        require(path.is_file(), f"missing Stage8A5 artifact: {path}")

    authority = json.loads(authority_path.read_text(encoding="utf-8"))
    require(authority.get("schema_version") == 1, "authority schema drift")
    require(authority.get("stage") == "8A-5-aggregate-acceptance", "stage drift")
    require(authority.get("status") == "aggregate_acceptance_candidate", "candidate status drift")
    for key, expected in EXPECTED.items():
        require(authority.get(key) == expected, f"accepted lineage drift: {key}")
    for key in (
        "aggregate_only", "inherited_stage7b_gate_required",
        "inherited_stage8_semantic_negative_gates_required",
        "stage8_forbidden_surface_required", "debug_workspace_tests_required",
        "release_workspace_tests_required", "workspace_doctests_required",
        "workspace_clippy_required", "external_compile_boundaries_required",
        "immutable_source_evidence_binding_required",
    ):
        require(authority.get(key) is True, f"required aggregate property drift: {key}")
    for key in (
        "production_rust_changed", "cargo_or_lock_changed", "workflow_changed",
        "stage8a_closed", "stage8b_authorized", "ack_publication_enabled",
        "readiness_publication_enabled", "redis_xadd_xack_enabled",
        "redis_live_consumer_enabled", "finam_post_delete_enabled",
        "broker_dispatch_enabled", "retry_resend_rearm_enabled",
        "runtime_live_enabled", "real_orders_enabled",
    ):
        require(authority.get(key) is False, f"closed surface opened: {key}")

    with matrix_path.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 30, "Stage8A5 matrix must contain 30 rows")
    require([row["id"] for row in rows] == [f"8A5-{i:03d}" for i in range(1, 31)], "Stage8A5 matrix ID drift")
    require(all(row["requirement"].strip() and row["proof"].strip() for row in rows), "empty Stage8A5 matrix cell")

    contract = contract_path.read_text(encoding="utf-8")
    for marker in (
        "aggregate acceptance candidate", PREDECESSOR,
        "Stage8-specific forbidden-surface scan", "workspace debug and release tests",
        "ACK/readiness publication", "Redis XADD/XACK", "FINAM POST/DELETE",
        "runtime-live", "Stage 8B", "does not itself\nauthorize that micro",
    ):
        require(marker in contract, f"aggregate contract marker missing: {marker}")
    negative = negative_path.read_text(encoding="utf-8")
    require(sum(1 for line in negative.splitlines() if line[:1].isdigit() and ". " in line) == 20, "negative inventory must contain 20 cases")

    gate = gate_path.read_text(encoding="utf-8")
    for marker in (
        "stage8a5_inherited_stage8_check.py", "stage8a5_forbidden_surface_check.py",
        "stage8a5_forbidden_surface_negative_harness.py", "stage8a5_negative_harness.py",
        "stage8a5_detached_cargo.sh",
        "stage7b_e_gate.sh", "stage8a4_durable_composition_i4_external_compile_fail.sh",
        "stage8a4_durable_composition_i3_external_compile_fail.sh",
        "cargo test --workspace --all-targets", "cargo test --workspace --release --all-targets",
        "cargo test --workspace --doc", "cargo clippy --workspace --all-targets --all-features -- -D warnings",
        "cargo fmt --all -- --check", "acceptance_rows\": 30", "negative_cases\": 20",
    ):
        require(marker in gate, f"aggregate gate marker missing: {marker}")

    if git_scope:
        require(git(root, "branch", "--show-current") == BRANCH, "Stage8A5 branch drift")
        subprocess.run(["git", "merge-base", "--is-ancestor", PREDECESSOR, "HEAD"], cwd=root, check=True)
        for ref in EXPECTED.values():
            if len(ref) == 40:
                subprocess.run(["git", "cat-file", "-e", f"{ref}^{{commit}}"], cwd=root, check=True)
        changed = git(root, "diff", "--name-only", PREDECESSOR, "--").splitlines()
        allowed_exact = {"README.md", "docs/current-status.md", "docs/roadmap.md", "docs/stage-8/stage8-slice-plan.md"}
        for path in changed:
            require(
                path in allowed_exact
                or path.startswith("docs/stage-8/STAGE8A5_")
                or path == "docs/stage-8/stage8a5-aggregate-acceptance-authority.json"
                or path.startswith("scripts/stage8a5_")
                or path == "scripts/make_stage8a5_handoff.py",
                f"Stage8A5 aggregate scope widened: {path}",
            )
            require(not path.startswith(("crates/", ".github/")), f"production/workflow delta entered aggregate: {path}")
            require(path not in ("Cargo.toml", "Cargo.lock"), f"Cargo delta entered aggregate: {path}")
    print("stage8a5-check: PASS rows=30 aggregate_only=true stage8b=false")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()
    check(args.root.resolve(), git_scope=not args.no_git)


if __name__ == "__main__":
    main()
