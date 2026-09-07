#!/usr/bin/env python3
"""Mutation proof for exact Stage 8B-P1-d4 retained crash evidence."""

from __future__ import annotations

import copy
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
        cell(run, "P1D4GM-013")["durable_truths"] = 999

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

    cases: tuple[tuple[str, Mutation], ...] = (
        ("exact-base-sequence", exact_base_sequence),
        ("pair-equality-not-adjacency-only", adjacency_only),
        ("independent-truth-count", truth_count),
        ("observed-provider-count", provider_count),
        ("observed-schedule-count", schedule_count),
        ("authenticated-s-ack-count", s_ack_count),
        ("authenticated-s-truth-count", s_truth_count),
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
