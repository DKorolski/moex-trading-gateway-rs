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


def refresh_group_frontier_hash(target: dict[str, Any]) -> None:
    frontier = target["redis"]["group_frontier_v1"]
    source_id = frontier["source_m10_redis_id"]
    target["redis"]["group_frontier"] = (
        f"before:last_delivered_id={source_id};pending={frontier['before']['pending']};"
        f"after:last_delivered_id={source_id};pending={frontier['post_restart']['pending']};"
        f"final:last_delivered_id={source_id};pending={frontier['final']['pending']}"
    )
    target["redis"]["group_frontier_sha256"] = hashlib.sha256(
        check.canonical_bytes(frontier)
    ).hexdigest()


def refresh_marker_hashes(target: dict[str, Any]) -> None:
    marker = target["filesystem"]["crash_marker_v1"]
    target["filesystem"]["raw_marker_sha256"] = hashlib.sha256(
        check.canonical_bytes(marker)
    ).hexdigest()
    normalized = copy.deepcopy(marker)
    normalized["child_pid"] = 0
    target["filesystem"]["normalized_marker_sha256"] = check.framed_sha256(
        check.MARKER_NORMALIZED_DOMAIN, check.canonical_bytes(normalized)
    )


def refresh_witness_hash(target: dict[str, Any]) -> None:
    witness = target["filesystem"]["pre_kill_xack_reply_witness_v1"]
    target["filesystem"]["pre_kill_xack_reply_witness_sha256"] = (
        check.framed_sha256(check.WITNESS_DIGEST_DOMAIN, check.canonical_bytes(witness))
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

    def f14_callback_erased(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-038")
        target["callback_before"] = 0
        target["callback_after"] = 0
        for phase in ("pre-kill", "post-restart", "final"):
            sync_runtime_audit(target, phase)

    def f14_callback_replayed(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-038")
        target["callback_after"] = 2
        sync_runtime_audit(target, "final")

    def f14_publication_missing(run: dict[str, Any]) -> None:
        cell(run, "P1D4C-038")["command_publications"] = 0

    def f14_publication_duplicated(run: dict[str, Any]) -> None:
        cell(run, "P1D4C-038")["command_publications"] = 2

    def f16_xack_disposition_inverted(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-010")
        target["redis"]["xack_reply"] = "integer:1"
        target["redis"]["xack_disposition"] = "AcknowledgedPending"

    def f16_xack_reply_incorrect(run: dict[str, Any]) -> None:
        cell(run, "P1D4C-010")["redis"]["xack_reply"] = "integer:9"

    def f16_pre_kill_xack_reply_missing(run: dict[str, Any]) -> None:
        cell(run, "P1D4C-010")["redis"]["pre_kill_xack_reply"] = "not_observed"

    def unrelated_group_frontier(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-038")
        frontier = target["redis"]["group_frontier_v1"]
        frontier["source_m10_redis_id"] = "1-0"
        for phase in ("before", "post_restart", "final"):
            frontier[phase]["last_delivered_id"] = "1-0"
        refresh_group_frontier_hash(target)

    def premature_immediate_xack(run: dict[str, Any]) -> None:
        cell(run, "P1D4C-038")["immediate_xack_attempts"] = 1

    def arbitrary_raw_marker_hash(run: dict[str, Any]) -> None:
        cell(run, "P1D4C-001")["filesystem"]["raw_marker_sha256"] = "e" * 64

    def arbitrary_normalized_marker_hash(run: dict[str, Any]) -> None:
        cell(run, "P1D4C-001")["filesystem"]["normalized_marker_sha256"] = "f" * 64

    def changed_pid_without_marker(run: dict[str, Any]) -> None:
        cell(run, "P1D4C-001")["process"]["child_pid"] += 1000

    def changed_marker_schema(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        target["filesystem"]["crash_marker_v1"]["schema_version"] = 2
        refresh_marker_hashes(target)

    def changed_marker_domain(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        target["filesystem"]["crash_marker_v1"]["domain"] = (
            "moex.stage8b.p1d4.crash-marker.v2"
        )
        refresh_marker_hashes(target)

    def changed_marker_cell(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        target["filesystem"]["crash_marker_v1"]["cell_id"] = "P1D4C-999"
        refresh_marker_hashes(target)

    def changed_marker_frontier(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        target["filesystem"]["crash_marker_v1"]["frontier_id"] = "F99"
        refresh_marker_hashes(target)

    def changed_marker_hook(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        target["filesystem"]["crash_marker_v1"]["kill_hook_name"] = "p1d4-forged"
        refresh_marker_hashes(target)

    def changed_marker_audit_binding(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        target["filesystem"]["crash_marker_v1"]["pre_kill_audit_sha256"] = "a" * 64
        refresh_marker_hashes(target)

    def normalized_extra_field(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-001")
        normalized = copy.deepcopy(target["filesystem"]["crash_marker_v1"])
        normalized["child_pid"] = 0
        normalized["scenario_id"] = "S99"
        target["filesystem"]["normalized_marker_sha256"] = check.framed_sha256(
            check.MARKER_NORMALIZED_DOMAIN, check.canonical_bytes(normalized)
        )

    def missing_f16_witness(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-010")
        target["filesystem"]["pre_kill_xack_reply_witness_v1"] = None
        target["filesystem"]["pre_kill_xack_reply_witness_sha256"] = None

    def arbitrary_f16_witness_hash(run: dict[str, Any]) -> None:
        cell(run, "P1D4C-010")["filesystem"][
            "pre_kill_xack_reply_witness_sha256"
        ] = "d" * 64

    def f16_witness_reply_zero(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-010")
        target["filesystem"]["pre_kill_xack_reply_witness_v1"]["xack_reply"] = (
            "integer:0"
        )
        refresh_witness_hash(target)

    def f16_witness_other_stream(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-010")
        target["filesystem"]["pre_kill_xack_reply_witness_v1"]["source_stream"] += (
            ":forged"
        )
        refresh_witness_hash(target)

    def f16_witness_other_group(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-010")
        target["filesystem"]["pre_kill_xack_reply_witness_v1"]["source_group"] += (
            "-forged"
        )
        refresh_witness_hash(target)

    def f16_witness_other_source_id(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-010")
        target["filesystem"]["pre_kill_xack_reply_witness_v1"][
            "source_m10_redis_id"
        ] = "1-0"
        refresh_witness_hash(target)

    def f16_witness_other_pid(run: dict[str, Any]) -> None:
        target = cell(run, "P1D4C-010")
        target["filesystem"]["pre_kill_xack_reply_witness_v1"]["child_pid"] += 1
        refresh_witness_hash(target)

    def f16_witness_before_reply(run: dict[str, Any]) -> None:
        cell(run, "P1D4C-010")["redis"]["pre_kill_xack_witness_order"] = (
            "before_integer_1"
        )

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
        ("f14-callback-erased", f14_callback_erased),
        ("f14-callback-replayed", f14_callback_replayed),
        ("f14-publication-missing", f14_publication_missing),
        ("f14-publication-duplicated", f14_publication_duplicated),
        ("f16-xack-disposition-inverted", f16_xack_disposition_inverted),
        ("f16-xack-reply-incorrect", f16_xack_reply_incorrect),
        ("f16-pre-kill-xack-reply-missing", f16_pre_kill_xack_reply_missing),
        ("unrelated-group-frontier", unrelated_group_frontier),
        ("premature-immediate-xack", premature_immediate_xack),
        ("arbitrary-raw-marker-hash", arbitrary_raw_marker_hash),
        ("arbitrary-normalized-marker-hash", arbitrary_normalized_marker_hash),
        ("changed-pid-without-marker", changed_pid_without_marker),
        ("changed-marker-schema", changed_marker_schema),
        ("changed-marker-domain", changed_marker_domain),
        ("changed-marker-cell", changed_marker_cell),
        ("changed-marker-frontier", changed_marker_frontier),
        ("changed-marker-hook", changed_marker_hook),
        ("changed-marker-audit-binding", changed_marker_audit_binding),
        ("normalized-extra-field", normalized_extra_field),
        ("missing-f16-witness", missing_f16_witness),
        ("arbitrary-f16-witness-hash", arbitrary_f16_witness_hash),
        ("f16-witness-reply-zero", f16_witness_reply_zero),
        ("f16-witness-other-stream", f16_witness_other_stream),
        ("f16-witness-other-group", f16_witness_other_group),
        ("f16-witness-other-source-id", f16_witness_other_source_id),
        ("f16-witness-other-pid", f16_witness_other_pid),
        ("f16-witness-before-reply", f16_witness_before_reply),
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
