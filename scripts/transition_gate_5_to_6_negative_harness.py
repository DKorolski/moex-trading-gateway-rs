#!/usr/bin/env python3
"""Named mutation matrix for Transition Gate 5->6."""

from __future__ import annotations

import copy
import json
from pathlib import Path

import transition_gate_5_to_6_check as checker

ROOT = Path(__file__).resolve().parents[1]


def rejected(name: str, action) -> None:
    try:
        action()
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        raise SystemExit(f"transition-gate-5-to-6-negative: FAIL: mutation accepted: {name}")


def main() -> None:
    count = 0
    descriptor = json.loads((ROOT / checker.DESCRIPTOR).read_text())
    top = [
        ("descriptor-schema", "schema_version", 2),
        ("descriptor-gate", "gate", "Stage 6"),
        ("descriptor-inventory-sha", "transition_authority_inventory_sha256", "0" * 64),
        ("descriptor-slices", "stage6_slices", ["6A"]),
        ("descriptor-stage6-open", "stage6_status", "open"),
    ]
    for name, field, value in top:
        candidate = copy.deepcopy(descriptor); candidate[field] = value
        rejected(name, lambda candidate=candidate: checker.validate_descriptor(candidate)); count += 1
    nested = [
        ("wrong-parent", "source_ref_binding", "required_parent", "0" * 40),
        ("wrong-branch", "source_ref_binding", "required_branch", "main"),
        ("wrong-stage5-commit", "accepted_stage5", "closure_commit", "0" * 40),
        ("wrong-closure-sha", "accepted_stage5", "closure_descriptor_sha256", "0" * 64),
        ("wrong-stage5-inventory", "accepted_stage5", "authority_inventory_sha256", "0" * 64),
        ("wrong-artifact-sha", "accepted_stage5", "lifecycle_artifact_sha256", "0" * 64),
        ("wrong-row-count", "accepted_stage5", "lifecycle_row_count", 53),
        ("wrong-acceptance-ref", "accepted_stage5", "acceptance_reference_sha256", "0" * 64),
    ]
    for name, section, field, value in nested:
        candidate = copy.deepcopy(descriptor); candidate[section][field] = value
        rejected(name, lambda candidate=candidate: checker.validate_descriptor(candidate)); count += 1
    for surface in descriptor["closed_surfaces"]:
        candidate = copy.deepcopy(descriptor); candidate["closed_surfaces"][surface] = True
        rejected(f"open-{surface}", lambda candidate=candidate: checker.validate_descriptor(candidate)); count += 1

    acceptance = json.loads((ROOT / checker.ACCEPTANCE).read_text())
    acceptance_mutations = [
        ("acceptance-verdict", "verdict", "REJECTED"),
        ("acceptance-stage5g", "stage5g_status", "OPEN"),
        ("acceptance-commit", "accepted_commit", "0" * 40),
        ("acceptance-archive", "accepted_archive_sha256", "0" * 64),
        ("acceptance-transition", "transition_gate_5_to_6", "CLOSED"),
        ("acceptance-stage6", "stage6", "OPEN"),
    ]
    for name, field, value in acceptance_mutations:
        candidate = copy.deepcopy(acceptance); candidate[field] = value
        rejected(name, lambda candidate=candidate: checker.validate_acceptance(candidate)); count += 1

    inventory = json.loads((ROOT / checker.INVENTORY).read_text())
    for index in range(4):
        candidate = copy.deepcopy(inventory); candidate["authorities"][index]["sha256"] = "0" * 64
        rejected(f"authority-hash-{index}", lambda candidate=candidate: checker.validate_inventory(ROOT, candidate)); count += 1
    for index in (0, 3):
        candidate = copy.deepcopy(inventory); candidate["authorities"][index]["path"] = f"missing-{index}"
        rejected(f"authority-missing-{index}", lambda candidate=candidate: checker.validate_inventory(ROOT, candidate)); count += 1
    for path in ("crates/strategy-runtime-core/src/stage5g_mock_ack.rs", "crates/strategy-runtime-core/src/stage5g_order_position.rs"):
        candidate = copy.deepcopy(inventory)
        entry = next(item for item in candidate["authorities"] if item["path"] == path)
        entry["classifications"] = ["artifact_fixture_adapter"]
        rejected(f"classification-downgrade-{Path(path).stem}", lambda candidate=candidate: checker.validate_inventory(ROOT, candidate)); count += 1

    contracts = {name: (ROOT / path).read_text() for name, path in checker.CONTRACTS.items()}
    for contract, tokens in checker.REQUIRED_TOKENS.items():
        for index, token in enumerate(tokens):
            candidate = dict(contracts); candidate[contract] = candidate[contract].replace(token, f"removed-{index}")
            rejected(f"{contract}-missing-{index:02d}", lambda candidate=candidate: checker.validate_contracts(candidate)); count += 1

    if count < 48:
        raise SystemExit(f"transition-gate-5-to-6-negative: FAIL: only {count} cases")
    print(f"transition-gate-5-to-6-negative: PASS {count}/{count}")


if __name__ == "__main__":
    main()
