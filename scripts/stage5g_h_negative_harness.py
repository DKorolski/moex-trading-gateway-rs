#!/usr/bin/env python3
"""Rehash-aware aggregate mutation harness for Stage 5G-h."""

from __future__ import annotations

import copy
import json
from pathlib import Path

import stage5g_h_check as checker

ROOT = Path(__file__).resolve().parents[1]


def rehash(row: dict) -> None:
    row["canonical_row_fingerprint_sha256"] = checker.canonical_row_sha256(row)


def cases(accepted: list[dict]) -> list[tuple[str, list[dict]]]:
    result: list[tuple[str, list[dict]]] = []

    def mutated(name: str, index: int, field: str, value: object) -> None:
        rows = copy.deepcopy(accepted)
        rows[index][field] = value
        rehash(rows[index])
        result.append((name, rows))

    result.append(("missing-row", copy.deepcopy(accepted[:-1])))
    result.append(("duplicate-row", copy.deepcopy(accepted + [accepted[-1]])))
    swapped = copy.deepcopy(accepted); swapped[0], swapped[1] = swapped[1], swapped[0]
    result.append(("order-drift", swapped))
    for index in range(20):
        mutated(f"rehash-disposition-{index:02d}", index, "disposition", f"forged-{index}")
    for index in range(10):
        mutated(f"rehash-callback-{index:02d}", index, "callback_count", accepted[index]["callback_count"] + 1)
    fields = [
        "pre_runtime_fingerprint_sha256", "post_runtime_fingerprint_sha256",
        "lifecycle_checkpoint_fingerprint_sha256", "restart_package_fingerprint_sha256",
        "generated_intent_fingerprint_sha256", "cleanup_fingerprint_sha256",
        "final_owner", "final_cycle_id", "final_position_qty",
    ]
    for offset, field in enumerate(fields):
        mutated(f"rehash-{field}", 46 + (offset % 8), field, f"forged-{field}")
    evidence_values = ["executable_accepted_witness", "source_produced_runtime_artifact"]
    for index, value in zip((0, 26), evidence_values):
        mutated(f"rehash-evidence-kind-{index}", index, "evidence_kind", value)
    timer_indices = list(range(26, 34))
    restart_indices = list(range(34, 46))
    for number, index in enumerate(timer_indices[:4]):
        rows = copy.deepcopy(accepted)
        rows[index]["executable_witnesses"] = copy.deepcopy(accepted[timer_indices[(number + 1) % len(timer_indices)]]["executable_witnesses"])
        rehash(rows[index]); result.append((f"timer-witness-swap-{number}", rows))
    for number, index in enumerate(restart_indices[:4]):
        rows = copy.deepcopy(accepted)
        rows[index]["executable_witnesses"] = copy.deepcopy(accepted[restart_indices[(number + 1) % len(restart_indices)]]["executable_witnesses"])
        rehash(rows[index]); result.append((f"restart-witness-swap-{number}", rows))
    for index, surface in enumerate(accepted[0]["closed_surfaces"]):
        rows = copy.deepcopy(accepted)
        rows[index]["closed_surfaces"][surface] = True
        rehash(rows[index]); result.append((f"closed-surface-{surface}", rows))
    for index in range(5):
        mutated(f"rehash-predecessor-{index}", index, "accepted_predecessor", "0" * 40)
    for index in range(5):
        mutated(f"rehash-schema-{index}", index, "schema_version", 2)
    return result


def main() -> None:
    accepted = json.loads((ROOT / checker.ARTIFACT).read_text())
    all_cases = cases(accepted)
    for name, rows in all_cases:
        try:
            checker.validate_rows(rows, accepted)
        except checker.CheckFailure:
            print(f"PASS {name}")
        else:
            raise SystemExit(f"stage5g-h-negative: FAIL: mutation accepted: {name}")

    descriptor = json.loads((ROOT / checker.DESCRIPTOR).read_text())
    descriptor_cases = [
        ("descriptor-schema", "schema_version", 2),
        ("descriptor-stage-g-ref", "accepted_stage5g_g_predecessor", "0" * 40),
        ("descriptor-stage-f-ref", "accepted_stage5g_f_predecessor", "0" * 40),
        ("descriptor-next-transition", "next_transition", "Stage 6"),
        ("descriptor-stage6-open", "stage6_status", "open"),
        ("descriptor-inventory-sha", "authority_inventory_sha256", "0" * 64),
    ]
    for name, field, value in descriptor_cases:
        candidate = copy.deepcopy(descriptor); candidate[field] = value
        try:
            checker.validate_descriptor(candidate)
        except checker.CheckFailure:
            print(f"PASS {name}")
        else:
            raise SystemExit(f"stage5g-h-negative: FAIL: mutation accepted: {name}")
    nested_descriptor_cases = [
        ("descriptor-artifact-sha", "accepted_matrix", "sha256", "0" * 64),
        ("descriptor-artifact-path", "accepted_matrix", "path", "forged.json"),
        ("descriptor-row-count", "accepted_matrix", "row_count", 53),
        ("descriptor-parent", "source_ref_binding", "required_parent", "0" * 40),
        ("descriptor-branch", "source_ref_binding", "required_branch", "main"),
    ]
    for name, section, field, value in nested_descriptor_cases:
        candidate = copy.deepcopy(descriptor); candidate[section][field] = value
        try:
            checker.validate_descriptor(candidate)
        except checker.CheckFailure:
            print(f"PASS {name}")
        else:
            raise SystemExit(f"stage5g-h-negative: FAIL: mutation accepted: {name}")

    source = (ROOT / checker.SOURCE).read_text()
    source_cases = {
        "parallel-ack-omitted": source.replace("stage5g_g_ack_artifact_rows", "ack_adapter_removed"),
        "parallel-order-omitted": source.replace("stage5g_g_order_position_artifact_rows", "order_adapter_removed"),
        "parallel-protective-omitted": source.replace("stage5g_f_gprt_artifact_rows_parallel_verified", "protective_adapter_removed"),
        "parallel-comparison-removed": source.replace("Stage 5G-h true-parallel source production must preserve accepted bytes", "comparison removed"),
        "parallel-clone-return": source + "\n// spawn(move || row)\n",
    }
    for name, candidate in source_cases.items():
        try:
            checker.validate_parallel_source(candidate)
        except checker.CheckFailure:
            print(f"PASS {name}")
        else:
            raise SystemExit(f"stage5g-h-negative: FAIL: mutation accepted: {name}")

    total = len(all_cases) + len(descriptor_cases) + len(nested_descriptor_cases) + len(source_cases)
    if total < 64:
        raise SystemExit(f"stage5g-h-negative: FAIL: only {total} cases")
    print(f"stage5g-h-negative: PASS {total}/{total}")


if __name__ == "__main__":
    main()
