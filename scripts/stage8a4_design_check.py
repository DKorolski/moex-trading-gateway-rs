#!/usr/bin/env python3
"""Fail-closed Stage 8A-4 design R1 semantic checker."""

from __future__ import annotations

import csv
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d"
BRANCH = "stage8a4-reconciliation-design"
AUTHORITY = Path("docs/stage-8/stage8a4-design-authority.json")
CONTRACT = Path("docs/stage-8/stage8a4-design-contract.md")
INVENTORY = Path("docs/stage-8/stage8a4-source-inventory.md")
DESCRIPTOR = Path("docs/stage-8/stage8a4-design-descriptor.json")
MATRIX = Path("docs/stage-8/STAGE8A_4_DESIGN_ACCEPTANCE_MATRIX_2026-08-15.csv")
NEGATIVE = Path("docs/stage-8/STAGE8A_4_DESIGN_NEGATIVE_INVENTORY_2026-08-15.md")
CURRENT_STATUS = Path("docs/current-status.md")
ROADMAP = Path("docs/roadmap.md")

ALLOWED_CHANGED_PATHS = {
    str(AUTHORITY),
    str(CONTRACT),
    str(INVENTORY),
    str(DESCRIPTOR),
    str(MATRIX),
    str(NEGATIVE),
    str(CURRENT_STATUS),
    str(ROADMAP),
    "scripts/stage8a4_design_check.py",
    "scripts/stage8a4_design_gate.sh",
    "scripts/stage8a4_design_negative_harness.py",
    "scripts/stage8a4_design_proof_map.py",
    "scripts/stage8a4_design_handoff_safety_check.py",
    "scripts/make_stage8a4_design_handoff_archive.py",
}

FORBIDDEN_CONTRACT_MARKERS = (
    "empty truth proves no match",
    "stale truth proves no match",
    "incomplete truth proves no match",
    "missing position means flat",
    "empty orders mean broker rejection",
    "position alone proves this request filled",
    "trade alone selects an order",
    "select the first plausible candidate",
    "select the latest plausible candidate",
    "select by broker status priority",
    "fall back to broker-neutral instrument.symbol",
    "same-request retry is allowed",
    "automatic resend after ambiguity",
    "HTTP response is broker truth",
    "historical cancel reconciler is authoritative",
    "M3d2 lifecycle is authoritative",
    "real FINAM POST enabled",
    "real FINAM DELETE enabled",
    "reqwest order transport enabled",
    "Redis live command consumer enabled",
    "broker dispatch enabled",
    "runtime-live enabled",
    "real strategy orders enabled",
    "STOP SLTP bracket replace multi-leg enabled",
    "Stage 8B is open",
    "shape matching precedes exact ClientOrderId",
    "known BrokerOrderId follows shape matching",
    "unknown broker status is terminal",
    "caller-selected unbounded freshness event policy",
    "raw broker truth identities and bodies are public diagnostics",
)


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def changed_paths() -> set[str]:
    tracked = subprocess.check_output(
        ["git", "diff", "--name-only", BASE, "--"], cwd=ROOT, text=True
    ).splitlines()
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT, text=True
    ).splitlines()
    return {value for value in tracked + untracked if value}


def markdown_section(source: str, heading: str) -> str:
    marker = f"## {heading}\n"
    require(marker in source, f"missing markdown section: {heading}")
    return source.split(marker, 1)[1].split("\n## ", 1)[0]


def check(root: Path = ROOT, *, git_scope: bool = True) -> None:
    authority = json.loads((root / AUTHORITY).read_text())
    require(authority["stage"] == "8A-4-design-R1", "authority stage drift")
    require(
        authority["status"] == "design_candidate_independent_acceptance_pending",
        "authority status drift",
    )
    require(authority["accepted_predecessor"] == BASE, "predecessor drift")
    require(
        authority["accepted_predecessor_review_sha256"]
        == "2e969db40bd847230f4df426ce3ee235f2f2273b87a778297b4588bf1f127232",
        "accepted review drift",
    )
    require(authority["design_only"] is True, "design-only boundary opened")
    require(
        authority["production_reconciliation_implemented"] is False,
        "implementation predeclared",
    )
    require(
        authority["canonical_truth_type"] == "broker_core::BrokerTruthSnapshot",
        "canonical truth drift",
    )
    require(
        authority["required_truth_sources"]
        == ["orders", "trades", "positions", "instrument_registry"],
        "required truth source drift",
    )
    require(all(authority["required_truth_properties"].values()), "truth property disabled")
    require(
        authority["correlation_precedence"]
        == [
            "exact_client_order_id_or_native_correlation",
            "known_broker_order_id",
            "account_instrument_side_quantity_bounded_event_time",
        ],
        "correlation precedence drift",
    )
    require(
        authority["supporting_evidence_only"] == ["trades", "target_instrument_position"],
        "supporting evidence gained authority",
    )
    require(
        authority["outcomes"]
        == [
            "ExactWorking",
            "ExactPartiallyFilled",
            "ExactFullyFilled",
            "ExactTerminalRejected",
            "ExactTerminalCancelled",
            "ExactTerminalExpired",
            "Conflict",
            "StillUnknown",
        ],
        "outcome algebra drift",
    )
    require(all(authority["closed"].values()), "closed surface opened")
    require(
        authority["next_after_acceptance"] == "Stage 8A-4 implementation R1 only",
        "post-acceptance authority drift",
    )

    descriptor = json.loads((root / DESCRIPTOR).read_text())
    require(descriptor["stage"] == "8A-4-design-R1", "descriptor stage drift")
    require(descriptor["accepted_predecessor"] == BASE, "descriptor predecessor drift")
    require(descriptor["acceptance_rows"] == 72, "acceptance count drift")
    require(descriptor["negative_cases"] == 48, "negative count drift")
    require(descriptor["production_files_changed"] is False, "production change declared")
    require(descriptor["reconciliation_implemented"] is False, "implementation declared")
    require(descriptor["proven_no_match_available"] is False, "ProvenNoMatch opened")
    require(descriptor["network_send_authorized"] is False, "network send opened")
    require(descriptor["redis_live_authorized"] is False, "Redis live opened")
    require(descriptor["runtime_live_authorized"] is False, "runtime-live opened")
    require(descriptor["real_orders_authorized"] is False, "real orders opened")
    require(
        descriptor["next_after_acceptance"] == "Stage 8A-4 implementation R1 only",
        "descriptor next-stage drift",
    )

    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 72, "acceptance matrix must contain 72 rows")
    require(
        [row["id"] for row in rows] == [f"S8A4D-{index:03d}" for index in range(1, 73)],
        "acceptance row IDs drift",
    )
    require(all(row["mandatory"] == "YES" for row in rows), "optional row introduced")
    negative = (root / NEGATIVE).read_text()
    require(len(re.findall(r"^\d+\. ", negative, re.M)) == 48, "negative inventory drift")

    contract = (root / CONTRACT).read_text()
    required_contract_terms = (
        "broker_core::BrokerTruthSnapshot",
        "exact stable `ClientOrderId`",
        "known exact `BrokerOrderId(String)`",
        "account + instrument + side + quantity + bounded event time",
        "The first tier containing evidence owns the decision",
        "Trades and target-instrument position support a selected order",
        "`ProvenNoMatch` remains unconstructible throughout Stage 8A",
        "pure reducer cannot mutate a journal",
        "Stage 8A-4 implementation R1",
    )
    for term in required_contract_terms:
        require(term in contract, f"required contract term missing: {term}")
    for marker in FORBIDDEN_CONTRACT_MARKERS:
        require(marker not in contract, f"forbidden design marker: {marker}")

    source_inventory = (root / INVENTORY).read_text()
    require("Historical implementations that are oracle-only" in source_inventory, "oracle boundary missing")
    require("position evidence as terminal" in source_inventory, "historical gap not recorded")
    require("No production source is changed" in source_inventory, "design-only inventory drift")

    status = markdown_section((root / CURRENT_STATUS).read_text(), "Current accepted boundary")
    require("Stage 8A-3 R2 is independently accepted and closed at" in status and BASE in status, "status predecessor drift")
    require("Stage 8A-4 design R1 is the only active candidate" in status, "status active stage drift")
    require("Stage 8A-4 implementation" in status and "remain closed" in status, "status closed boundary drift")

    roadmap = markdown_section((root / ROADMAP).read_text(), "Current active stage")
    require("Stage 8A-3 R2 is independently accepted and closed at" in roadmap and BASE in roadmap, "roadmap predecessor drift")
    require("Stage 8A-4 design R1 is the only active candidate" in roadmap, "roadmap active stage drift")
    require("Stage 8A-4 implementation" in roadmap and "remain closed" in roadmap, "roadmap closed boundary drift")

    if git_scope:
        branch = subprocess.check_output(
            ["git", "branch", "--show-current"], cwd=ROOT, text=True
        ).strip()
        require(branch == BRANCH, f"branch drift: {branch}")
        require(changed_paths() == ALLOWED_CHANGED_PATHS, "changed-path allowlist drift")


def main() -> int:
    try:
        check()
    except (CheckFailure, KeyError, OSError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"stage8a4-design-r1-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage8a4-design-r1-check: PASS rows=72 design-only=true next=8A-4-implementation-r1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
