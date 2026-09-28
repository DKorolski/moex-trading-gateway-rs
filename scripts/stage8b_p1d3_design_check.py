#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P1-d3 R1 design correction."""

from __future__ import annotations

import csv
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "74696d1eefc0453c41440f79b087cafebd0d7ab0"
ACCEPTED_P1D2 = "bcd8db546104968dd0e48ab041e02acf6869d224"
DESIGN = ROOT / "docs/stage-8/stage8b-p1d3-working-limit-cancel-lifecycle-design.md"
ANNEX = ROOT / "docs/stage-8/stage8b-p1d3-projection-recovery-annex-r1.md"
MATRIX = ROOT / "docs/stage-8/stage8b-p1d3-working-limit-cancel-acceptance-matrix.csv"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d3-working-limit-cancel-evidence.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"
EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d3-projection-recovery-annex-r1.md",
    "docs/stage-8/stage8b-p1d3-working-limit-cancel-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d3-working-limit-cancel-evidence.json",
    "docs/stage-8/stage8b-p1d3-working-limit-cancel-lifecycle-design.md",
    "scripts/make_stage8b_p1d3_design_handoff.py",
    "scripts/stage8b_p1d3_design_check.py",
    "scripts/stage8b_p1d3_design_gate.sh",
    "scripts/stage8b_p1d3_design_handoff_safety_check.py",
    "scripts/stage8b_p1d3_design_negative_harness.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", BASE],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    ).stdout.splitlines()
    return set(tracked) | set(untracked)


def validate(
    design: str,
    annex: str,
    matrix_text: str,
    evidence: dict[str, object],
    status: str,
    roadmap: str,
) -> None:
    required_design = (
        "Status: R1 design-only review candidate",
        BASE,
        ACCEPTED_P1D2,
        "stage8b-p1d3-projection-recovery-annex-r1.md",
        "moex.stage8b.p1d3.working-book.v1",
        "moex.stage8b.p1d3.book-transition.v1",
        "moex.stage8b.p1d3.outcome-evidence.v1",
        "moex.stage8b.p1d3.book-genesis.v1",
        "1. **request lifecycle**",
        "2. **order lifecycle**",
        "replacement `S_working` is persisted and reread",
        "inside the existing authenticated replacement package, never as a sidecar",
        "P1D3_MAX_ORDER_RECORDS_PER_GENERATION = 1024",
        "Capacity exhaustion fails closed before a new dispatch.",
        "Stage8bP1d3ScheduleStepAuthority",
        "Stage8bP1d3DayExpiryAuthority",
        "UTC date comparison, wall clock, process time and the\nfirst bar of a new day are not expiry authorities.",
        "resolve the exact already-active order against B[k]",
        "The book evaluator has no M10 XACK method.",
        "records `B[n+1]` as already evaluated",
        "bar.low <= limit -> Filled at min(bar.open, limit)",
        "bar.high >= limit -> Filled at max(bar.open, limit)",
        "exact final execution-bar `close_ts`",
        "Stage6dPaperOutcome::LimitExpired { broker_order_id }",
        "ACK: `Accepted`, reason `None`, exact request-level durable client ID",
        "truth uses `seq_truth = seq_ack + 1`",
        "untouched: order remains Working; no Stage 5G event or total sequence is",
        "Partial fill, a second trade or a\nsecond terminal transition fails closed.",
        "same authenticated Stage 6 journal record as the outcome",
        "Persisting only\na digest is insufficient.",
        "This\nfreezes fill-before-cancel",
        "`CancelCanceled` | `Accepted` / no reason | target `Canceled`",
        "`CancelExecutionObserved` | `Recovered` / `RecoveredByBrokerTruth`",
        "`CancelAlreadyTerminalNonExecution` | `Recovered` / `RecoveredByBrokerTruth`",
        "`Inconclusive` hold | none | none",
        "A target\norder client ID never replaces it.",
        "later autonomous fill or expiry: one next truth sequence",
        "S_cancel_recovered  recovered cancel ACK plus unchanged terminal book committed",
        "An in-memory\nrecovered ACK never authorizes XACK.",
        "Restart authenticates the exact Stage 5G/6/7 package and P1-d3 book together.",
        "full authenticated\n`Stage8bP1d3OutcomeEvidenceV1` embedded in its Stage 6 record",
        "never\nreacquires a consumed capability",
        "Registry rows are serialized in ascending bytewise UTF-8 order",
        "one-time crate-private authenticated transition to an empty\nordinal-zero P1-d3 book",
        "P1-d4 remains responsible for the complete SIGKILL/frontier matrix",
        "P1-d3 production source implementation before design acceptance",
        "operational Redis DB 0 and VPS paper service activation",
        "FINAM HTTP POST/DELETE and broker network dispatch",
    )
    for token in required_design:
        require(token in design, f"missing design invariant: {token}")

    required_annex = (
        "Status: R1 design-only review candidate",
        BASE,
        ACCEPTED_P1D2,
        "T_CANDIDATE = exact schedule-approved final candidate M10 close_ts",
        "T_BOUNDARY  = exact consumed Day-expiry authority boundary timestamp",
        "Z       = Decimal::ZERO with positive sign and scale zero",
        "CancelExecutionObserved | cancel `T_CANDIDATE`; never old target clock",
        "`T_source = T_receipt = T_transition`",
        "`Utc::now()`, process time, Redis read\ntime",
        "`client_order_id` | `Some(DCID_P)` | `Some(DCID_C)` | `Some(DCID_C)`",
        "`Some(CommandAckReason { code: RecoveredByBrokerTruth })`",
        "`filled_qty` | exact `Z` | exact `Q` | exact `Z` | exact `Z`",
        "`broker_asset_id` | `None` | `None` | `None` | `None`",
        "`remaining_qty` | `Some(Q)` | `Some(Z)` | `Some(Q)` | `Some(Q)`",
        "`commission` | `Some(Z)`",
        "`gross_amount` | `None`",
        "P1-d2 scale-8 `MidpointNearestEven` result",
        "CancelExecutionObserved | no new `BrokerTruthSnapshot`",
        "initial Working | `[Working order]` | `[]` | `None` | `[]` | `[]`",
        "post-ACK seal\nis `S_cancel_recovered`",
        "S_cancel_recovered persisted+reread",
        "byte-identical recovered ACK replay is the only continuation",
        "Immediate XACK after an in-memory recovered ACK is forbidden.",
        "Recovered ACK\nreplay after the seal and duplicate target truth before or after the seal are\nforbidden.",
        "Stage8bP1d3OutcomeEvidenceV1",
        "full versioned write-ahead fact",
        "not only its digest",
        "same authenticated Stage6 journal\nrecord",
        "request ID, durable request client ID, canonical command hash, accepted\n   command payload hash and optional target place client ID/order ID",
        "Every Decimal is a `[u8; 16]` returned by\n`Decimal::serialize()`",
        "moex.stage8b.p1d3.outcome-evidence.v1\\0",
        "no provider, Hybrid callback, Redis\ncandidate selection",
        "It cannot select another currently valid boundary.",
        "bytewise UTF-8 bytes of exact\n`BrokerOrderId::as_str()`",
        "original `RID_P`, original `DCID_P`, and deterministic order\nfingerprint",
        "moex.stage8b.p1d3.book-genesis.v1\\0",
        "moex.stage8b.p1d3.book-transition.v1\\0",
        "u64_be(outcome_evidence_len)",
        "sole P1-d2 migration bridge",
        "no P1 request is pending and no active order exists",
        "checked-in golden canonical bytes and SHA-256\nfor all eight shapes",
        "immediate XACK after in-memory\nrecovered ACK and duplicate target truth",
    )
    for token in required_annex:
        require(token in annex, f"missing R1 annex invariant: {token}")
    require(annex.count("| `broker_asset_id` | `None`") == 2, "metadata rows drifted")
    require(annex.count("no new `BrokerTruthSnapshot`") == 2, "recovered truth cardinality drifted")
    require(annex.count("T_transition") >= 25, "clock coverage weakened")

    rows = list(csv.DictReader(matrix_text.splitlines()))
    expected_ids = {f"P1D3D-{index:03d}" for index in range(1, 93)}
    require(len(rows) == 92, f"acceptance row count drifted: {len(rows)}")
    require({row.get("id") for row in rows} == expected_ids, "acceptance IDs drifted")
    require(all(row.get("status") == "REQUIRED" for row in rows), "acceptance weakened")
    by_id = {row["id"]: row for row in rows}
    row_tokens = {
        "P1D3D-001": ACCEPTED_P1D2,
        "P1D3D-007": "S_working",
        "P1D3D-040": "target evaluation precedes cancellation",
        "P1D3D-061": "S_cancel_recovered",
        "P1D3D-064": "Immediate XACK",
        "P1D3D-071": "eight shape truth memberships",
        "P1D3D-080": "eight shapes",
        "P1D3D-081": "Full Stage8bP1d3OutcomeEvidenceV1 bytes",
        "P1D3D-089": "no provider callback dispatch",
        "P1D3D-091": "u64 big-endian lengths",
        "P1D3D-092": "without effects",
    }
    for row_id, token in row_tokens.items():
        require(token in by_id[row_id]["requirement"], f"acceptance semantics drifted: {row_id}")

    require(evidence.get("schema_version") == 1, "evidence schema drifted")
    require(
        evidence.get("stage")
        == "Stage 8B-P1-d3 working LIMIT/CANCEL/expiry lifecycle design",
        "stage drifted",
    )
    require(evidence.get("status") == "R1_DESIGN_REVIEW_CANDIDATE", "status drifted")
    require(evidence.get("accepted_p1d2_closure_ref") == ACCEPTED_P1D2, "lineage drifted")
    require(evidence.get("acceptance_rows") == 92, "evidence row count drifted")
    require(evidence.get("negative_cases") == 58, "negative count drifted")
    require(evidence.get("design_only") is True, "design-only marker opened")
    require(evidence.get("implementation_authorized") is False, "source opened early")
    require(evidence.get("current_tree_authority_remains_p1d2") is True, "authority rebound")
    contracts = {
        "canonical_book_contract": "moex.stage8b.p1d3.working-book.v1",
        "canonical_transition_contract": "moex.stage8b.p1d3.book-transition.v1",
        "canonical_outcome_evidence_contract": "moex.stage8b.p1d3.outcome-evidence.v1",
        "canonical_genesis_contract": "moex.stage8b.p1d3.book-genesis.v1",
    }
    for name, value in contracts.items():
        require(evidence.get(name) == value, f"contract drifted: {name}")
    r1 = evidence.get("r1_closure")
    require(isinstance(r1, dict) and len(r1) == 8, "R1 closure inventory drifted")
    require(r1.get("reviewed_r0_ref") == BASE, "R0 lineage drifted")
    require(r1.get("recovered_cancel_terminal_phase") == "S_cancel_recovered", "phase drifted")
    require(r1.get("exact_projection_shapes") == 8, "projection inventory drifted")
    require(r1.get("golden_byte_shapes_required") == 8, "golden inventory drifted")
    require(r1.get("full_outcome_evidence_persisted") is True, "digest-only recovery opened")
    require(r1.get("p1d2_quiescent_migration") is True, "migration weakened")
    require(r1.get("recovery_constructor_has_external_authority") is False, "recovery API opened")
    policy = evidence.get("policy")
    require(isinstance(policy, dict) and len(policy) == 8, "policy inventory drifted")
    require(policy.get("active_orders_per_operational_identity") == 1, "active bound drifted")
    require(policy.get("max_order_records_per_generation") == 1024, "registry bound drifted")
    require(policy.get("partial_fills_enabled") is False, "partial fills opened")
    require(policy.get("cancel_fill_order") == "target_evaluation_before_cancel", "cancel order drifted")
    require(policy.get("day_expiry_authority") == "opaque_stage5e_source_capability", "expiry drifted")
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 12, "closed inventory drifted")
    require(all(value is False for value in closed.values()), "closed surface opened")

    status_tokens = (BASE, "narrow R1 design correction", "S_cancel_recovered", "P1-d3 production source")
    require(all(token in status for token in status_tokens), "current status is not synchronized")
    roadmap_tokens = (BASE, "active R1\ndesign-only correction", "full Stage6-embedded", "P1-d4 exhaustive")
    require(all(token in roadmap for token in roadmap_tokens), "roadmap is not synchronized")


def main() -> None:
    try:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"R1 design-only changed path drift: {sorted(actual)}")
        forbidden_diff = subprocess.run(
            ["git", "diff", "--name-only", BASE, "--", "Cargo.toml", "Cargo.lock", "crates", ".github", "config"],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()
        require(not forbidden_diff, f"production/workflow/config changed: {forbidden_diff}")
        validate(
            DESIGN.read_text(encoding="utf-8"),
            ANNEX.read_text(encoding="utf-8"),
            MATRIX.read_text(encoding="utf-8"),
            json.loads(EVIDENCE.read_text(encoding="utf-8")),
            STATUS.read_text(encoding="utf-8"),
            ROADMAP.read_text(encoding="utf-8"),
        )
    except (CheckFailure, OSError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"FAIL stage8b-p1d3-r1-design-scope: {error}", file=sys.stderr)
        raise SystemExit(1)
    print(
        "PASS stage8b-p1d3-r1-design-scope rows=92 negatives=58 shapes=8 "
        "design_only=true source=false p1d4=false db0=false finam=false live=false"
    )


if __name__ == "__main__":
    main()
