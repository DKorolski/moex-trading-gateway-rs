#!/usr/bin/env python3
"""Mutation proof for exact Stage 8B-P1-d4 retained crash evidence."""

from __future__ import annotations

import copy
import hashlib
import json
import pathlib
import sys
import tempfile
from typing import Any, Callable

import stage8b_p1d4_crash_evidence_check as check


Mutation = Callable[[dict[str, Any]], None]


def cell(run: dict[str, Any], cell_id: str) -> dict[str, Any]:
    matches = [candidate for candidate in run["cells"] if candidate["cell_id"] == cell_id]
    if len(matches) != 1:
        raise RuntimeError(f"cell lookup is not exact: {cell_id}")
    return matches[0]


def write_mutation(source: pathlib.Path, mutation: Mutation, destination: pathlib.Path) -> None:
    runs = [check.load_json(source / name) for name in check.RUN_FILES]
    for run in runs:
        mutation(run)
    destination.mkdir()
    for name, run in zip(check.RUN_FILES, runs):
        (destination / name).write_text(
            json.dumps(run, sort_keys=True, separators=(",", ":")),
            encoding="utf-8",
        )
    # Deliberately redigest the forged two-run evidence.  The semantic checker
    # must reject the incorrect fact itself, not merely a stale digest.
    (destination / check.DIGEST_FILE).write_text(
        check.semantic_digest(runs[0]) + "\n", encoding="utf-8"
    )


def refresh_audit_hash(target: dict[str, Any], phase: str) -> None:
    payload_field = f"{phase.replace('-', '_')}_audit_payload"
    digest_field = f"{phase.replace('-', '_')}_audit_sha256"
    target[digest_field] = hashlib.sha256(
        check.canonical_bytes(target[payload_field])
    ).hexdigest()


def sync_runtime_audit(target: dict[str, Any], phase: str) -> None:
    before = phase in {"pre-kill", "post-restart"}
    sequence = target["sequence_audit_before" if before else "sequence_audit_after"]
    runtime = target[f"{phase.replace('-', '_')}_audit_payload"]["runtime_audit"]
    runtime["lifecycle_sequence"] = sequence["lifecycle_sequence"]
    runtime["journal_lifecycle_sequences"] = copy.deepcopy(
        sequence["journal_lifecycle_sequences"]
    )
    runtime["sequence_pair"] = copy.deepcopy(sequence["durable_sequence_pair"])
    runtime["sequence_allocations"] = copy.deepcopy(sequence["allocations"])
    runtime["package"] = copy.deepcopy(
        target["package_before" if before else "package_after"]
    )
    runtime["callback_count"] = target["callback_before" if before else "callback_after"]
    runtime["durable_outcomes"] = len(sequence["allocations"])
    runtime["truth_bearing_outcomes"] = sum(
        allocation["seq_truth"] is not None for allocation in sequence["allocations"]
    )
    refresh_audit_hash(target, phase)


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(
            "usage: stage8b_p1d4_crash_evidence_negative_harness.py EVIDENCE_DIRECTORY"
        )
    source = pathlib.Path(sys.argv[1]).resolve()

    def exact_base_sequence(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-004")
        for field in ("sequence_audit_before", "sequence_audit_after"):
            allocation = target[field]["allocations"][0]
            allocation["sequence_allocation_frontier"] = 998
            allocation["seq_ack"] = 999
            allocation["seq_truth"] = 1000
        target["sequence_before"] = "seq_ack=999;seq_truth=1000"
        target["sequence_after"] = "seq_ack=999;seq_truth=1000"

    def adjacency_only(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4GM-009")
        target["sequence_audit_before"]["pre_kill_sequence_pair"] = [41, 42]
        target["sequence_before"] = "seq_ack=41;seq_truth=42"

    def truth_count(run: dict[str, Any]) -> None:
        cell(run, "P1D4GM-013")["truth_bearing_outcomes"] = 999

    def provider_count(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4GM-004")
        target["provider_attempts"] = 0
        target["observed_effect_events"].remove("generated_provider")

    def schedule_count(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4GM-002")
        target["schedule_issue_attempts"] = 0
        target["observed_effect_events"].remove("generated_schedule")

    def s_ack_count(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4GM-008")
        target["s_ack_commits"] = 0
        target["s_ack_generations"] = []
        target["observed_effect_events"].remove("generated_s_ack")

    def s_truth_count(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4GM-011")
        target["s_truth_commits"] = 0
        target["s_truth_generations"] = []
        target["observed_effect_events"].remove("generated_s_truth")

    def f00_allocation_before_wal(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        target["sequence_audit_before"] = copy.deepcopy(target["sequence_audit_after"])
        target["sequence_before"] = check.sequence_label(
            target["sequence_audit_before"], False
        )
        target["callback_before"] = target["callback_after"]
        for phase in ("pre-kill", "post-restart"):
            sync_runtime_audit(target, phase)

    def f03_allocation_missing_after_wal(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-004")
        target["sequence_audit_before"]["allocations"] = []
        target["sequence_audit_before"]["durable_sequence_pair"] = None
        target["sequence_audit_before"]["pre_kill_sequence_pair"] = None
        target["sequence_before"] = check.sequence_label(
            target["sequence_audit_before"], False
        )
        for phase in ("pre-kill", "post-restart"):
            sync_runtime_audit(target, phase)

    def reverse_base_effect_order(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        target["observed_effect_events"] = list(
            reversed(target["observed_effect_events"])
        )

    def wrong_final_package_phase(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        target["package_after"]["p1d3_phase"] = "Terminal"
        target["package_commit_history"][-1]["p1d3_phase"] = "Terminal"
        sync_runtime_audit(target, "final")

    def wrong_absolute_generation(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        target["package_before"]["write_generation"] += 10
        target["package_after"]["write_generation"] += 10
        for commit in target["package_commit_history"]:
            commit["write_generation"] += 10
        for phase in ("pre-kill", "post-restart", "final"):
            sync_runtime_audit(target, phase)

    def truth_without_final_package_commit(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        target["package_after"]["p1d3_phase"] = "Migrated"
        for commit in target["package_commit_history"]:
            commit["p1d3_phase"] = "Migrated"
        target["truth_replacement_commits"] = 0
        sync_runtime_audit(target, "final")

    def retained_audit_hash_mismatch(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        target["final_audit_payload"]["runtime_audit"]["callback_count"] += 1

    cases: tuple[tuple[str, Mutation], ...] = (
        ("exact-base-sequence", exact_base_sequence),
        ("pair-equality-not-adjacency-only", adjacency_only),
        ("independent-truth-count", truth_count),
        ("observed-provider-count", provider_count),
        ("observed-schedule-count", schedule_count),
        ("authenticated-s-ack-count", s_ack_count),
        ("authenticated-s-truth-count", s_truth_count),
        ("f00-allocation-before-wal", f00_allocation_before_wal),
        ("f03-allocation-missing-after-wal", f03_allocation_missing_after_wal),
        ("reverse-base-effect-order", reverse_base_effect_order),
        ("wrong-final-package-phase", wrong_final_package_phase),
        ("wrong-absolute-generation", wrong_absolute_generation),
        ("truth-without-final-package-commit", truth_without_final_package_commit),
        ("retained-audit-hash-mismatch", retained_audit_hash_mismatch),
    )
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-p1d4-evidence-negative-") as root:
        root_path = pathlib.Path(root)
        for index, (name, mutation) in enumerate(cases):
            mutated = root_path / f"{index:02}-{name}"
            write_mutation(source, mutation, mutated)
            try:
                check.check(mutated)
            except check.EvidenceFailure:
                passed += 1
                continue
            raise SystemExit(
                f"FAIL stage8b-p1d4-crash-evidence-negative-harness accepted {name}"
            )
    print(
        "PASS stage8b-p1d4-crash-evidence-negative-harness "
        f"{passed}/{len(cases)}"
    )


if __name__ == "__main__":
    main()
