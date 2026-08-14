#!/usr/bin/env python3
"""Fail-closed specification checks for Transition Gate 7->8."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import subprocess
from pathlib import Path

PREDECESSOR = "7c3ffffcfec012f3c96c65a3fcaf366c1740b88e"
ACCEPTED_STAGE7B = "a1044e0dbe324c722b637498ca80ffafd9f0cbee"
BRANCH = "gate7-to-8-spec"
REVIEW_SHA = "e66d87ae88cf1c1a8f2ec12ac5d4338374c26bfcc417624b0fa1f007a5c81bf2"
CLOSURE_DESCRIPTOR_SHA = "ee3e3555def13b5da6f699c4be62c2fff2c9bca1b82487036f7d9816b0a2c003"
ACCEPTANCE_RECORD_SHA = "f50b45318132124af0516d32df4f4fa4d358719a07e5cbe6603bead9caa52b1d"

DESCRIPTOR = Path("docs/stage-8/transition-gate-7-to-8-descriptor.json")
SPEC = Path("docs/stage-8/transition-gate-7-to-8-specification.md")
MATRIX = Path("docs/stage-8/TRANSITION_GATE_7_TO_8_ACCEPTANCE_MATRIX_2026-08-14.csv")
SLICE_PLAN = Path("docs/stage-8/stage8-slice-plan.md")
CLOSURE_DESCRIPTOR = Path("docs/stage-7/stage7b-closure-descriptor.json")
ACCEPTANCE_RECORD = Path("docs/stage-7/stage7b-final-acceptance-record.md")

ALLOWED_DELTA = {
    "docs/current-status.md",
    "docs/roadmap.md",
    str(DESCRIPTOR),
    str(SPEC),
    str(MATRIX),
    str(SLICE_PLAN),
    "scripts/transition_gate_7_to_8_check.py",
    "scripts/transition_gate_7_to_8_negative_harness.py",
    "scripts/transition_gate_7_to_8.sh",
    "scripts/make_transition_gate_7_to_8_handoff_archive.py",
}


class GateFailure(ValueError):
    pass


def require(value: bool, message: str) -> None:
    if not value:
        raise GateFailure(message)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_descriptor(value: dict) -> None:
    require(value.get("schema_version") == 1, "descriptor schema drift")
    require(value.get("gate") == "Transition Gate 7->8", "gate identity drift")
    require(value.get("status") == "specification_candidate_pending_independent_review", "gate self-accepted or status drift")
    binding = value.get("source_ref_binding", {})
    require(binding.get("required_branch") == BRANCH, "required branch drift")
    require(binding.get("required_predecessor") == PREDECESSOR, "predecessor drift")
    accepted = value.get("accepted_stage7b", {})
    require(accepted.get("accepted_source_ref") == ACCEPTED_STAGE7B, "accepted Stage 7B ref drift")
    require(accepted.get("closure_record_ref") == PREDECESSOR, "closure record ref drift")
    require(accepted.get("closure_descriptor_sha256") == CLOSURE_DESCRIPTOR_SHA, "closure descriptor binding drift")
    require(accepted.get("acceptance_record_sha256") == ACCEPTANCE_RECORD_SHA, "acceptance record binding drift")
    require(accepted.get("independent_review_sha256") == REVIEW_SHA, "independent review binding drift")
    decision = value.get("decision_after_independent_acceptance", {})
    require(decision.get("stage8a_protected_adapter_and_reconciliation") == "implementation_authorized_network_send_closed", "Stage 8A network boundary opened")
    require(decision.get("stage8b_bounded_real_execution") == "closed_pending_separate_acceptance_and_operator_authorization", "Stage 8B opened")
    require(decision.get("stage9_continuous_reconciliation") == "closed", "Stage 9 opened")
    require(decision.get("stage10_runtime_live") == "closed", "Stage 10 opened")
    require(value.get("allowed_initial_commands") == ["PLACE_MARKET", "PLACE_LIMIT", "CANCEL"], "initial command allowlist drift")
    require(value.get("forbidden_initial_commands") == ["REPLACE", "STOP", "SLTP", "BRACKET", "MULTI_LEG"], "forbidden command list drift")
    safety = value.get("safety_invariants", {})
    expected_safety = {
        "stable_client_order_id_required": True,
        "durable_attempt_before_possible_send": True,
        "blind_retry_after_ambiguous_outcome": False,
        "fresh_broker_truth_required_for_reconciliation": True,
        "max_nonfinal_lifecycles_per_strategy": 1,
        "max_live_engineering_micro_commands": 1,
        "autonomous_strategy_live_attachment": False,
        "simultaneous_alor_finam_live_for_same_strategy": False,
        "operator_arm_one_shot": True,
        "kill_switch_fail_closed": True,
    }
    require(safety == expected_safety, "safety invariant drift")
    require(value.get("acceptance_row_count") == 45, "acceptance row count drift")
    require(value.get("negative_case_count") == 20, "negative case count drift")
    require(value.get("independent_acceptance_required") is True, "independent acceptance removed")
    require(value.get("currently_open_surfaces") and not any(value["currently_open_surfaces"].values()), "execution surface opened")


SPEC_TOKENS = [
    "Status: specification candidate pending independent review.",
    "Stage8ExecutionCapability",
    "not implement `Clone`, `Copy`, `Serialize` or `Deserialize`",
    "DispatchAttemptRecorded",
    "one-shot operator arm",
    "PLACE MARKET",
    "PLACE LIMIT",
    "CANCEL",
    "AmbiguousAfterPossibleSend",
    "ReconciliationRequired",
    "No outcome after a possible send is automatically retried.",
    "fresh broker truth",
    "max-one engineering-micro budget",
    "one broker ownership lease",
    "Stage 8 implementation  CLOSED",
    "FINAM POST/DELETE       CLOSED",
    "native protective orders CLOSED",
]


def validate_spec(text: str) -> None:
    for token in SPEC_TOKENS:
        require(token in text, f"specification token missing: {token}")
    require("Independent acceptance of this specification authorizes only Stage 8A" in text, "Stage 8A authorization rule missing")
    require("It does not authorize real FINAM POST/DELETE" in text, "real endpoint prohibition missing")
    require("ALOR and FINAM must not both have live execution authority" in text, "single broker ownership rule missing")
    require("LimitCancel exercise" in text and "two-action scenario" in text, "LimitCancel budget rule missing")


def validate_matrix(path: Path) -> None:
    with path.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 45, "acceptance matrix is not 45 rows")
    require(list(rows[0]) == ["id", "category", "requirement", "expected", "mandatory", "evidence"], "matrix header drift")
    expected_ids = [f"G78-{index:03d}" for index in range(1, 46)]
    require([row["id"] for row in rows] == expected_ids, "matrix IDs missing reordered or duplicated")
    require(all(row["mandatory"] == "YES" for row in rows), "optional acceptance row introduced")
    require(all(all(row[field].strip() for field in rows[0]) for row in rows), "empty acceptance field")
    categories = {row["category"] for row in rows}
    required = {"predecessor", "capability", "operator", "mapping", "ambiguity", "reconciliation", "micro_budget", "scope", "limits", "kill_switch", "durability", "ownership", "evidence", "governance"}
    require(categories == required, "acceptance category drift")


def validate_slice_plan(text: str) -> None:
    normalized = " ".join(text.split())
    required = [
        "Stage 8A — protected adapter and reconciliation",
        "Stage 8A does not authorize a real FINAM POST/DELETE",
        "Stage 8B — bounded real engineering micro",
        "at most one explicitly armed engineering command",
        "It does not attach an autonomous strategy runtime",
        "No later stage is opened by this plan.",
    ]
    for token in required:
        require(" ".join(token.split()) in normalized, f"slice plan token missing: {token}")


def git_output(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def validate_git_scope(root: Path) -> None:
    require(git_output(root, "merge-base", "--is-ancestor", PREDECESSOR, "HEAD") == "", "predecessor is not ancestor")
    tracked = set(filter(None, git_output(root, "diff", "--name-only", PREDECESSOR).splitlines()))
    untracked = set(filter(None, git_output(root, "ls-files", "--others", "--exclude-standard").splitlines()))
    changed = tracked | untracked
    require(changed, "empty gate specification delta")
    unexpected = changed - ALLOWED_DELTA
    require(not unexpected, f"out-of-scope path changed: {sorted(unexpected)}")
    require(not any(path.startswith(("crates/", ".github/")) or path in {"Cargo.toml", "Cargo.lock"} for path in changed), "production or CI delta present")


def check(root: Path, *, check_git_scope: bool = True) -> None:
    require(sha(root / CLOSURE_DESCRIPTOR) == CLOSURE_DESCRIPTOR_SHA, "accepted closure descriptor changed")
    require(sha(root / ACCEPTANCE_RECORD) == ACCEPTANCE_RECORD_SHA, "accepted acceptance record changed")
    validate_descriptor(json.loads((root / DESCRIPTOR).read_text()))
    validate_spec((root / SPEC).read_text())
    validate_matrix(root / MATRIX)
    validate_slice_plan((root / SLICE_PLAN).read_text())
    status = (root / "docs/current-status.md").read_text()
    roadmap = (root / "docs/roadmap.md").read_text()
    status_words = " ".join(status.split())
    roadmap_words = " ".join(roadmap.split())
    require("Transition Gate 7→8 specification" in status_words, "current status not moved to gate candidate")
    require("Stage 8 implementation remains CLOSED" in status_words, "current status opened Stage 8")
    require("Transition Gate 7→8 specification" in roadmap_words, "roadmap gate target missing")
    require("real FINAM POST/DELETE remains closed" in roadmap_words, "roadmap real endpoint boundary missing")
    if check_git_scope:
        validate_git_scope(root)
    print("transition-gate-7-to-8-check: PASS rows=45 stage8a=no-send stage8b=closed")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--skip-git-scope", action="store_true")
    args = parser.parse_args()
    try:
        check(args.root.resolve(), check_git_scope=not args.skip_git_scope)
    except (GateFailure, FileNotFoundError, json.JSONDecodeError, csv.Error) as error:
        raise SystemExit(f"transition-gate-7-to-8-check: FAIL: {error}") from error


if __name__ == "__main__":
    main()
