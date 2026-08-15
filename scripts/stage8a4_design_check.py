#!/usr/bin/env python3
"""Fail-closed Stage 8A-4 design R2 semantic checker."""

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
    require(authority["schema_version"] == 2, "authority schema drift")
    require(authority["stage"] == "8A-4-design-R2", "authority stage drift")
    require(
        authority["status"] == "design_r2_candidate_independent_acceptance_pending",
        "authority status drift",
    )
    require(authority["accepted_predecessor"] == BASE, "predecessor drift")
    require(
        authority["accepted_predecessor_review_sha256"]
        == "2e969db40bd847230f4df426ce3ee235f2f2273b87a778297b4588bf1f127232",
        "accepted review drift",
    )
    require(
        authority["design_r1_baseline"] == "0aa9ff19708409879bcf82f8e87613c53a917218",
        "design R1 baseline drift",
    )
    require(
        authority["design_r1_review_sha256"]
        == "39cf2a55836200d1951ee2a370b7a0b126a057fa99b377b89259cf133a04e8f2",
        "design R1 review drift",
    )
    require(
        authority["design_r2_correction_spec_sha256"]
        == "dc09d8db5851451a3004a60bc1c426af78ecddbd4727cdd9024a2d66550e7c9a",
        "design R2 correction specification drift",
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
    require(
        authority["required_truth_properties"]
        == {
            "fresh": True,
            "complete_under_source_specific_proof": True,
            "account_scoped": True,
            "target_instrument_resolved": True,
            "trusted_time_checked": True,
            "post_attempt_acquisition_checked": True,
            "policy_fingerprint_bound": True,
        },
        "truth property drift",
    )
    completeness = authority["source_completeness"]
    require(
        completeness["orders"]
        == {
            "mode": "non_paginated_snapshot",
            "account_binding_required": True,
            "request_and_response_trusted_timestamps_required": True,
            "full_body_decode_required": True,
            "local_truncation_forbidden": True,
            "synthetic_cursor_or_page_evidence_forbidden": True,
            "absence_proves_no_match": False,
        },
        "orders completeness drift",
    )
    trades = completeness["trades"]
    require(
        trades["mode"] == "bounded_interval_limit_with_fail_closed_saturation",
        "trades completeness mode drift",
    )
    require(trades["interval_semantics"] == "start_inclusive_end_exclusive", "trade interval semantics drift")
    require(trades["query_envelope_policy_fingerprint_bound"] is True, "trade query policy unbound")
    require(trades["caller_selected_limit_or_window_allowed"] is False, "caller controls trade query")
    require(trades["event_window_start_coverage_required"] is True, "trade start coverage disabled")
    require(trades["event_window_end_coverage_required"] is True, "trade end coverage disabled")
    require(trades["gap_free_union_required"] is True, "trade interval gaps allowed")
    require(trades["interval_coverage_fingerprint_required"] is True, "trade coverage unbound")
    require(
        trades["returned_count_greater_or_equal_limit_is_complete"] is False,
        "saturated trade interval accepted",
    )
    require(trades["deterministic_subdivision_or_still_unknown"] is True, "trade saturation not fail closed")
    require(trades["subdivision_bounded"] is True, "trade subdivision unbounded")
    require(trades["subdivision_policy_fingerprint_bound"] is True, "trade subdivision policy unbound")
    require(
        completeness["instrument_registry"]
        == {
            "exact_target_resolution_mode": "exact_asset_params_schedule_reads",
            "full_registry_mode": "cursor_exhausted_only_when_assets_all_used",
            "fictitious_pagination_for_exact_target_forbidden": True,
        },
        "instrument completeness drift",
    )
    require(
        completeness["known_broker_order_id_lookup"]
        == {
            "mode": "conditional_readonly_source",
            "requires_durable_broker_order_id": True,
            "strengthens_tier2": True,
            "replaces_account_wide_orders_safety_snapshot": False,
            "not_found_or_unavailable_proves_no_match": False,
            "exact_source_disagreement": "Conflict",
        },
        "exact-order lookup semantics drift",
    )
    require(
        authority["correlation_precedence"]
        == [
            "exact_client_order_id_or_native_correlation",
            "known_broker_order_id",
            "exact_bound_order_shape_and_bounded_event_time",
        ],
        "correlation precedence drift",
    )
    require(
        authority["tier3_shape"]
        == {
            "binds_account": True,
            "binds_exact_instrument_and_finam_venue_symbol": True,
            "binds_side": True,
            "binds_original_quantity": True,
            "binds_order_type": True,
            "binds_time_in_force_day": True,
            "binds_exact_normalized_limit_price": True,
            "market_requires_absent_limit_price": True,
            "binds_bounded_trusted_event_window": True,
            "market_limit_cross_match_allowed": False,
            "caller_selected_price_tolerance_allowed": False,
            "missing_required_shape_field_outcome": "StillUnknown",
            "cancel_uses_original_target_order_shape": True,
        },
        "tier3 shape drift",
    )
    require(
        authority["supporting_evidence_only"] == ["trades", "target_instrument_position"],
        "supporting evidence gained authority",
    )
    exact = authority["exact_outcome"]
    require(exact["type"] == "ExactOrderState", "exact outcome type drift")
    require(exact["orthogonal_lifecycle_and_fill"] is True, "lifecycle/fill collapsed")
    require(
        exact["lifecycles"]
        == ["Working", "TerminalFilled", "TerminalRejected", "TerminalCancelled", "TerminalExpired"],
        "lifecycle algebra drift",
    )
    require(exact["fill_effects"] == ["Zero", "Partial", "Full"], "fill algebra drift")
    require(exact["selected_order_binding_required"] is True, "selected order binding disabled")
    require(exact["trade_summary_binding_required"] is True, "trade summary binding disabled")
    require(exact["terminal_cancel_partial_fill_preserved"] is True, "cancel partial fill lost")
    require(exact["terminal_expired_partial_fill_preserved"] is True, "expired partial fill lost")
    require(
        authority["quantity_invariants"]
        == {
            "qty_positive": True,
            "filled_between_zero_and_qty": True,
            "remaining_equals_qty_minus_filled": True,
            "filled_status_requires_full_quantity": True,
            "active_partial_requires_strict_partial_quantity": True,
            "rejected_nonzero_fill_is_conflict": True,
            "status_quantity_inconsistency_is_conflict": True,
            "unique_matching_trade_quantity_must_agree": True,
            "incomplete_trade_truth_cannot_create_exact_fill": True,
        },
        "quantity invariant drift",
    )
    require(
        authority["trade_identity"]
        == {
            "primary": "BrokerTradeId",
            "equal_duplicate_counted_once": True,
            "conflicting_duplicate_outcome": "Conflict",
            "matching_requires_exact_order_or_client_identity": True,
            "matching_also_requires_account_instrument_side": True,
            "position_cannot_supply_trade_identity": True,
        },
        "trade identity/dedup drift",
    )
    require(authority["outcomes"] == ["ExactOrderState", "Conflict", "StillUnknown"], "outcome algebra drift")
    require(all(authority["closed"].values()), "closed surface opened")
    require(
        authority["next_after_acceptance"] == "Stage 8A-4 implementation R1 only",
        "post-acceptance authority drift",
    )

    descriptor = json.loads((root / DESCRIPTOR).read_text())
    require(descriptor["schema_version"] == 2, "descriptor schema drift")
    require(descriptor["stage"] == "8A-4-design-R2", "descriptor stage drift")
    require(
        descriptor["status"] == "design_r2_candidate_independent_acceptance_pending",
        "descriptor status drift",
    )
    require(descriptor["accepted_predecessor"] == BASE, "descriptor predecessor drift")
    require(descriptor["acceptance_rows"] == 92, "acceptance count drift")
    require(descriptor["negative_cases"] == 68, "negative count drift")
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
    require(len(rows) == 92, "acceptance matrix must contain 92 rows")
    require(
        [row["id"] for row in rows] == [f"S8A4D-{index:03d}" for index in range(1, 93)],
        "acceptance row IDs drift",
    )
    require(all(row["mandatory"] == "YES" for row in rows), "optional row introduced")
    negative = (root / NEGATIVE).read_text()
    require(len(re.findall(r"^\d+\. ", negative, re.M)) == 68, "negative inventory drift")

    contract = (root / CONTRACT).read_text()
    required_contract_terms = (
        "broker_core::BrokerTruthSnapshot",
        "exact stable `ClientOrderId`",
        "known exact `BrokerOrderId(String)`",
        "account + exact canonical instrument/FINAM venue symbol + side + original",
        "`returned_count >= requested_limit` is incomplete",
        "`BrokerTradeId` is the primary trade identity",
        "exact result is `ExactOrderState` with two independent dimensions",
        "exact normalized LIMIT price",
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
    require(
        "Design" in status and "R2 is the only active candidate" in status,
        "status active stage drift",
    )
    require("Stage 8A-4 implementation" in status and "remain closed" in status, "status closed boundary drift")

    roadmap = markdown_section((root / ROADMAP).read_text(), "Current active stage")
    require("Stage 8A-3 R2 is independently accepted and closed at" in roadmap and BASE in roadmap, "roadmap predecessor drift")
    require(
        "Design R2" in roadmap and "only active candidate" in roadmap,
        "roadmap active stage drift",
    )
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
        print(f"stage8a4-design-r2-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage8a4-design-r2-check: PASS rows=92 design-only=true next=8A-4-implementation-r1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
