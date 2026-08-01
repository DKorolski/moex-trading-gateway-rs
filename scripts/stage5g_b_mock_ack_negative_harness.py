#!/usr/bin/env python3
"""Negative mutation harness for the Stage 5G-b fail-closed checker."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5g_b_mock_ack_check.py"
CONTRACT = "docs/stage-5/stage5g-b-mock-ack-contract.json"
ENTRY = "docs/stage-5/stage5g-lifecycle-entry-inventory.json"
MODULE = "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
LIB = "crates/strategy-runtime-core/src/lib.rs"
STATUS = "docs/current-status.md"
DESIGN = "docs/stage-5/5g-b-mock-ack-attachment.md"


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def required_paths() -> set[str]:
    paths = {CONTRACT, ENTRY, MODULE, LIB, STATUS, DESIGN}
    entry = load_json(ROOT / ENTRY)
    for authority in entry["reuse_authorities"]:
        if authority["mutability"] in {"frozen", "frozen_for_5g_entry"}:
            paths.add(authority["path"])
    return paths


def copy_baseline(destination: Path) -> None:
    for relative in required_paths():
        source = ROOT / relative
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def replace_once(path: Path, before: str, after: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(before) < 1:
        raise RuntimeError(f"mutation anchor missing in {path}: {before}")
    path.write_text(text.replace(before, after, 1), encoding="utf-8")


def insert_before_tests(root: Path, snippet: str) -> None:
    replace_once(root / MODULE, "#[cfg(test)]", snippet + "\n#[cfg(test)]")


def mutate_required_case(root: Path) -> None:
    value = load_json(root / CONTRACT)
    value["required_case_ids"].pop()
    write_json(root / CONTRACT, value)


def mutate_review_negative(root: Path) -> None:
    value = load_json(root / CONTRACT)
    value["review_negative_cases"].pop()
    write_json(root / CONTRACT, value)


def mutate_open_surface(root: Path) -> None:
    value = load_json(root / CONTRACT)
    value["closed_surfaces"]["finam_transport"] = True
    write_json(root / CONTRACT, value)


def mutate_predecessor(root: Path) -> None:
    value = load_json(root / CONTRACT)
    value["accepted_predecessors"]["stage5g_a_design_ref"] = "0" * 40
    write_json(root / CONTRACT, value)


def mutate_transition(root: Path) -> None:
    value = load_json(root / CONTRACT)
    value["next_transition"]["stage5g_c_open"] = True
    write_json(root / CONTRACT, value)


def mutate_frozen_authority(root: Path) -> None:
    path = root / "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    path.write_text(path.read_text(encoding="utf-8") + "\n// drift\n", encoding="utf-8")


def mutate_reqwest(root: Path) -> None:
    insert_before_tests(root, "fn forbidden_transport() { let _ = reqwest::Client::new(); }")


def mutate_synthetic_broker_id(root: Path) -> None:
    insert_before_tests(
        root,
        'fn forbidden_id() { let _ = BrokerOrderId::new("SYNTHETIC"); }',
    )


def mutate_order_truth(root: Path) -> None:
    insert_before_tests(root, "fn forbidden_truth(_: BrokerOrderSnapshot) {}")


def mutate_policy_delegation(root: Path) -> None:
    replace_once(root / MODULE, ".evaluate_ack(ack)", ".evaluate_ack_bypassed(ack)")


def mutate_stage5c_callsite(root: Path) -> None:
    insert_before_tests(
        root,
        "fn extra_callback(x: Stage5cSettledPaperStrategy, i: Stage5cPaperIntentLifecycleInput) { let _ = resolve_stage5c_paper_intent_lifecycle(x, i); }",
    )


def mutate_test_witness(root: Path) -> None:
    replace_once(
        root / MODULE,
        "gack10_conflicting_broker_order_id_blocks",
        "gack10_witness_removed",
    )


def mutate_compile_fail_witness(root: Path) -> None:
    replace_once(
        root / LIB,
        "Stage 5G-b ACK feedback cannot be attached before ownership",
        "ownership witness removed",
    )


def mutate_status(root: Path) -> None:
    replace_once(
        root / STATUS,
        "Stage 5G-c remains blocked",
        "Stage 5G-c is open",
    )


def mutate_status_enum(root: Path) -> None:
    insert_before_tests(root, "enum CommandAckStatus { Accepted }")


CASES: list[tuple[str, Callable[[Path], None]]] = [
    ("required-case-removal", mutate_required_case),
    ("review-negative-removal", mutate_review_negative),
    ("closed-surface-open", mutate_open_surface),
    ("predecessor-rebind", mutate_predecessor),
    ("premature-stage5g-c-open", mutate_transition),
    ("frozen-authority-drift", mutate_frozen_authority),
    ("reqwest-transport", mutate_reqwest),
    ("synthetic-broker-id", mutate_synthetic_broker_id),
    ("order-truth-surface", mutate_order_truth),
    ("broker-core-policy-bypass", mutate_policy_delegation),
    ("extra-stage5c-callback", mutate_stage5c_callsite),
    ("test-witness-removal", mutate_test_witness),
    ("compile-fail-witness-removal", mutate_compile_fail_witness),
    ("current-status-open", mutate_status),
    ("second-ack-status-enum", mutate_status_enum),
]


def checker_exit(root: Path) -> int:
    result = subprocess.run(
        [sys.executable, str(CHECKER), "--root", str(root)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.returncode


def main() -> int:
    if checker_exit(ROOT) != 0:
        print("stage5g-b-negative-harness: FAIL: positive baseline rejected", file=sys.stderr)
        return 1
    passed = 0
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-b-negative-") as directory:
            root = Path(directory)
            copy_baseline(root)
            mutation(root)
            if checker_exit(root) == 0:
                print(f"FAIL {name}: mutation was accepted", file=sys.stderr)
                return 1
            print(f"PASS {name}")
            passed += 1
    print(f"stage5g-b-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
