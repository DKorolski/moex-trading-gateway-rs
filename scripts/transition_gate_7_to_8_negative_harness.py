#!/usr/bin/env python3
"""Negative mutations for the Transition Gate 7->8 specification."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path
from typing import Callable

import transition_gate_7_to_8_check as checker

ROOT = Path(__file__).resolve().parents[1]
FILES = {
    checker.DESCRIPTOR,
    checker.SPEC,
    checker.MATRIX,
    checker.SLICE_PLAN,
    checker.CLOSURE_DESCRIPTOR,
    checker.ACCEPTANCE_RECORD,
    Path("docs/current-status.md"),
    Path("docs/roadmap.md"),
}


def copy_contracts(destination: Path) -> None:
    for relative in FILES:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def edit_json(root: Path, mutate: Callable[[dict], None]) -> None:
    path = root / checker.DESCRIPTOR
    value = json.loads(path.read_text())
    mutate(value)
    path.write_text(json.dumps(value, indent=2) + "\n")


def replace(root: Path, path: Path, old: str, new: str) -> None:
    target = root / path
    text = target.read_text()
    if old not in text:
        raise RuntimeError(f"mutation source missing: {path}: {old}")
    target.write_text(text.replace(old, new, 1))


def drop_matrix_row(root: Path) -> None:
    path = root / checker.MATRIX
    lines = path.read_text().splitlines()
    path.write_text("\n".join(lines[:-1]) + "\n")


def duplicate_matrix_id(root: Path) -> None:
    path = root / checker.MATRIX
    text = path.read_text().replace("G78-045,governance", "G78-044,governance", 1)
    path.write_text(text)


def optional_matrix_row(root: Path) -> None:
    replace(root, checker.MATRIX, "G78-020,ambiguity,\"429 or 5xx after possible send is ambiguous\",\"No transport or operator retry bypass exists\",YES", "G78-020,ambiguity,\"429 or 5xx after possible send is ambiguous\",\"No transport or operator retry bypass exists\",NO")


CASES: list[tuple[str, Callable[[Path], None]]] = [
    ("self-accept-gate", lambda root: edit_json(root, lambda value: value.__setitem__("status", "ACCEPTED"))),
    ("open-stage8a-network", lambda root: edit_json(root, lambda value: value["decision_after_independent_acceptance"].__setitem__("stage8a_protected_adapter_and_reconciliation", "real_send_authorized"))),
    ("open-stage8b", lambda root: edit_json(root, lambda value: value["decision_after_independent_acceptance"].__setitem__("stage8b_bounded_real_execution", "open"))),
    ("open-finam-post", lambda root: edit_json(root, lambda value: value["currently_open_surfaces"].__setitem__("finam_http_post", True))),
    ("allow-stop", lambda root: edit_json(root, lambda value: value["allowed_initial_commands"].append("STOP"))),
    ("remove-stop-prohibition", lambda root: edit_json(root, lambda value: value["forbidden_initial_commands"].remove("STOP"))),
    ("allow-blind-retry", lambda root: edit_json(root, lambda value: value["safety_invariants"].__setitem__("blind_retry_after_ambiguous_outcome", True))),
    ("increase-micro-budget", lambda root: edit_json(root, lambda value: value["safety_invariants"].__setitem__("max_live_engineering_micro_commands", 2))),
    ("enable-autonomous-runtime", lambda root: edit_json(root, lambda value: value["safety_invariants"].__setitem__("autonomous_strategy_live_attachment", True))),
    ("allow-dual-broker-live", lambda root: edit_json(root, lambda value: value["safety_invariants"].__setitem__("simultaneous_alor_finam_live_for_same_strategy", True))),
    ("disable-one-shot-arm", lambda root: edit_json(root, lambda value: value["safety_invariants"].__setitem__("operator_arm_one_shot", False))),
    ("forge-review-binding", lambda root: edit_json(root, lambda value: value["accepted_stage7b"].__setitem__("independent_review_sha256", "0" * 64))),
    ("forge-closure-binding", lambda root: edit_json(root, lambda value: value["accepted_stage7b"].__setitem__("closure_descriptor_sha256", "0" * 64))),
    ("drop-acceptance-row", drop_matrix_row),
    ("make-row-optional", optional_matrix_row),
    ("duplicate-row-id", duplicate_matrix_id),
    ("remove-no-blind-retry-rule", lambda root: replace(root, checker.SPEC, "No outcome after a possible send is automatically retried.", "Retries may be automatic.")),
    ("remove-real-endpoint-prohibition", lambda root: replace(root, checker.SPEC, "It does not authorize real FINAM POST/DELETE", "It authorizes real FINAM POST/DELETE")),
    ("open-later-stage", lambda root: replace(root, checker.SLICE_PLAN, "No later stage is opened by this plan.", "Later stages are open.")),
    ("open-current-status", lambda root: replace(root, Path("docs/current-status.md"), "Stage 8 implementation remains CLOSED", "Stage 8 implementation is OPEN")),
]


def main() -> None:
    checker.check(ROOT, check_git_scope=False)
    passed = 0
    for name, mutate in CASES:
        with tempfile.TemporaryDirectory(prefix="gate7-to-8-negative-") as raw:
            root = Path(raw)
            copy_contracts(root)
            mutate(root)
            try:
                checker.check(root, check_git_scope=False)
            except checker.GateFailure:
                passed += 1
                print(f"PASS {name}")
            else:
                raise SystemExit(f"transition-gate-7-to-8-negative: FAIL accepted mutation {name}")
    if passed != 20:
        raise SystemExit(f"transition-gate-7-to-8-negative: FAIL cases={passed}/20")
    print("transition-gate-7-to-8-negative: PASS cases=20/20")


if __name__ == "__main__":
    main()
