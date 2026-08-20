#!/usr/bin/env python3
"""Validate the docs/checker-only Stage 8A-4 I4 design contract."""

from __future__ import annotations

import csv
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(os.environ.get("STAGE8A4_I4_ROOT", Path(__file__).resolve().parents[1]))
DOC = ROOT / "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_DESIGN_2026-08-20.md"
MATRIX = ROOT / "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_DESIGN_ACCEPTANCE_MATRIX_2026-08-20.csv"
NEGATIVE = ROOT / "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_DESIGN_NEGATIVE_INVENTORY_2026-08-20.md"
AUTHORITY = ROOT / "docs/stage-8/stage8a4-durable-composition-i4-design-authority.json"
PREDECESSOR = "593ff255ef7826a22e66c9aff6f7ea47acf47644"
REVIEW_SHA = "1da167c3e7f1266473133d2d8a1412906a26d7f83b5dc026ce84dc7969090257"


def fail(message: str) -> None:
    raise SystemExit(f"stage8a4-durable-composition-i4-design-check: FAIL {message}")


def require(text: str, *needles: str) -> None:
    for needle in needles:
        if needle not in text:
            fail(f"missing design contract: {needle}")


def main() -> None:
    authority = json.loads(AUTHORITY.read_text(encoding="utf-8"))
    expected = {
        "stage": "8A-4-durable-composition-I4-design-R1",
        "status": "design_candidate",
        "accepted_predecessor_ref": PREDECESSOR,
        "accepted_predecessor_review_sha256": REVIEW_SHA,
        "acceptance_rows": 40,
        "negative_cases": 24,
        "scope": "derived_ack_and_current_readiness_facade_no_io",
        "next_after_acceptance": "I4 controlled no-I/O implementation",
    }
    for key, value in expected.items():
        if authority.get(key) != value:
            fail(f"authority {key} drift")
    terminal = authority.get("terminal_authority", {})
    for key in (
        "complete_v2_exact_suffix_required",
        "request_finalized_required",
        "covering_s1_required",
        "restart_reconstruction_required",
    ):
        if terminal.get(key) is not True:
            fail(f"terminal authority weakened: {key}")
    for key in ("pending_or_hold_authorized", "receipt_alone_is_authority"):
        if terminal.get(key) is not False:
            fail(f"terminal authority opened: {key}")
    readiness = authority.get("current_readiness", {})
    for key in (
        "independent_from_terminal_ack",
        "fresh_run_allowed_required",
        "fresh_composite_and_broker_truth_required",
        "stop_stale_unreadable_unknown_or_orphan_block",
    ):
        if readiness.get(key) is not True:
            fail(f"readiness weakened: {key}")
    if readiness.get("i3_post_effect_snapshot_reusable") is not False:
        fail("I3 post-effect snapshot became readiness authority")
    closed = authority.get("closed", {})
    if set(closed) != {
        "redis_ack_xack", "redis_live", "finam_post_delete", "broker_dispatch",
        "retry_resend_rearm", "runtime_live", "real_orders", "stage8a5", "stage8b",
    } or not all(value is True for value in closed.values()):
        fail("closed surface opened")

    with MATRIX.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    expected_ids = [f"I4D-{number:03d}" for number in range(1, 41)]
    if [row.get("id") for row in rows] != expected_ids:
        fail("acceptance matrix must be exact I4D-001..I4D-040")
    matrix_contract = "\n".join(row.get("requirement", "") for row in rows)
    require(
        matrix_contract,
        "unknown or orphan account safety blocks readiness",
        "duplicate derivation appends no journal record",
        "duplicate and restart preserve canonical ACK identity",
        "facade and authorities are nonserializable opaque types",
        "no caller-built seal checkpoint or digest-only constructor",
    )

    design = DOC.read_text(encoding="utf-8")
    require(
        design,
        PREDECESSOR,
        REVIEW_SHA,
        "Stage7bStage8a4TerminalAuthority",
        "Stage8a4I4CurrentReadinessEvidence",
        "Stage8a4I4DerivedAckReadinessFacade",
        "complete Stage8A4 V2 transition",
        "authenticated covering S1",
        "ReconciliationConflictHold",
        "ReconciliationStillUnknownHold",
        "ExactWorking | none | none | unresolved",
        "ExactTerminalFilled | ExecutionObserved | Recovered | RecoveredByBrokerTruth",
        "ExactTerminalRejected | AlreadyTerminalNonExecution | Recovered | RecoveredByBrokerTruth",
        "ExactTerminalCancelled | Canceled | Recovered | RecoveredByBrokerTruth",
        "ExactTerminalExpired | AlreadyTerminalNonExecution | Recovered | RecoveredByBrokerTruth",
        "RecoveredByBrokerTruth",
        "BrokerRejected",
        "StopRequested",
        "No I3 post-effect control snapshot",
        "stable terminal ACK identity excludes unrelated later seal generations",
        "Actual ACK publication or Redis XACK requires a later",
    )
    negative = NEGATIVE.read_text(encoding="utf-8")
    if sum(1 for line in negative.splitlines() if line[:1].isdigit() and ". " in line) != 24:
        fail("negative inventory must contain 24 numbered cases")

    # This slice is design-only. A real checkout must remain source-identical
    # to the independently accepted I3 R6 predecessor.
    if (ROOT / ".git").exists():
        changed = subprocess.check_output(
            ["git", "diff", "--name-only", PREDECESSOR, "--", "Cargo.toml", "Cargo.lock", "crates", "tests"],
            cwd=ROOT,
            text=True,
        ).strip()
        if changed:
            fail(f"production/test source changed in design slice: {changed}")

    source = "\n".join(
        path.read_text(encoding="utf-8", errors="ignore")
        for base in (ROOT / "crates", ROOT / "tests")
        if base.exists()
        for path in base.rglob("*.rs")
    )
    if "Stage8a4I4DerivedAckReadinessFacade" in source:
        fail("I4 implementation opened in design slice")
    print("stage8a4-durable-composition-i4-design-check: PASS rows=40 negatives=24 implementation=false ack_publish=false redis=false finam=false live=false")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, json.JSONDecodeError) as error:
        fail(str(error))
