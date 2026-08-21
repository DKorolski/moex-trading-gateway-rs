#!/usr/bin/env python3
"""Validate the docs/checker-only Stage 8B Design R1 contract."""

from __future__ import annotations

import argparse
import csv
import json
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(os.environ.get("STAGE8B_ROOT", Path(__file__).resolve().parents[1]))
DOC = ROOT / "docs/stage-8/STAGE8B_DESIGN_2026-08-21.md"
MATRIX = ROOT / "docs/stage-8/STAGE8B_DESIGN_ACCEPTANCE_MATRIX_2026-08-21.csv"
NEGATIVE = ROOT / "docs/stage-8/STAGE8B_DESIGN_NEGATIVE_INVENTORY_2026-08-21.md"
AUTHORITY = ROOT / "docs/stage-8/stage8b-design-authority.json"
BASE = "0ce76a334f12bf7b13e682ca976c9a4cde6be137"
ACCEPTED = "bf58b47fdef8af774a4107455dfcc6204e594283"
REVIEW_SHA = "72fa3c350dd34aef2d98230dec5547ba25bd7bc752b5b74eedf046e8502b13fc"
BRANCH = "stage8b-design"


def fail(message: str) -> None:
    raise SystemExit(f"stage8b-design-check: FAIL {message}")


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def require_markers(text: str, markers: tuple[str, ...]) -> None:
    for marker in markers:
        require(marker in text, f"missing contract marker: {marker}")


def require_true(section: dict[str, object], *keys: str) -> None:
    for key in keys:
        require(section.get(key) is True, f"required authority weakened: {key}")


def require_false(section: dict[str, object], *keys: str) -> None:
    for key in keys:
        require(section.get(key) is False, f"forbidden authority opened: {key}")


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def check(git_scope: bool) -> None:
    for path in (DOC, MATRIX, NEGATIVE, AUTHORITY):
        require(path.is_file(), f"missing file: {path.relative_to(ROOT)}")

    authority = json.loads(AUTHORITY.read_text(encoding="utf-8"))
    expected = {
        "schema_version": 1,
        "stage": "8B-design-R1",
        "status": "design_candidate",
        "design_base_ref": BASE,
        "accepted_stage8a5_ref": ACCEPTED,
        "accepted_stage8a5_review_sha256": REVIEW_SHA,
        "acceptance_rows": 48,
        "negative_cases": 36,
        "scope": "design_only_single_operator_armed_engineering_command",
        "next_after_acceptance": "Stage 8B implementation specification only",
    }
    for key, value in expected.items():
        require(authority.get(key) == value, f"authority drift: {key}")

    require(
        authority.get("phase_order") == [
            "8B-D design acceptance",
            "8B-S implementation specification acceptance",
            "8B-I no-send implementation and rehearsal acceptance",
            "8B-P read-only preflight and exact run authorization acceptance",
            "8B-X one-shot execution and post-run closure",
        ],
        "phase order drift",
    )

    run = authority.get("run_contract", {})
    require(run.get("allowed_action_domain") == ["PLACE", "CANCEL"], "action domain drift")
    require(run.get("place_order_type") == "LIMIT", "PLACE order type drift")
    require(run.get("place_time_in_force") == "DAY", "PLACE TIF drift")
    require(run.get("max_quantity_lots") == 1, "quantity bound drift")
    require(run.get("canonical_instrument") == "IMOEXF", "instrument drift")
    require(run.get("venue_symbol") == "IMOEXF@RTSX", "venue symbol drift")
    require_true(
        run,
        "exactly_one_command",
        "action_is_singleton_in_reviewed_run_contract",
        "account_bound_by_broker_account_id_hash",
        "side_price_and_notional_exactly_bound",
    )
    require_false(
        run,
        "automatic_followup_command_allowed",
        "limit_cancel_pair_allowed",
        "market_order_allowed",
        "protective_or_multi_leg_allowed",
    )

    arm = authority.get("operator_arm", {})
    require_true(
        arm,
        "durable_one_use",
        "expires_before_transport",
        "exact_command_identity_required",
        "exact_build_config_endpoint_and_body_hashes_required",
        "exact_account_instrument_action_side_qty_price_required",
    )
    require_false(arm, "reconstructible_after_restart", "second_arm_for_same_request_allowed")

    preflight = authority.get("preflight", {})
    require_true(
        preflight,
        "read_only_only",
        "fresh_broker_truth_required",
        "fresh_readiness_required",
        "fresh_schedule_required",
        "run_allowed_kill_switch_required",
        "single_broker_ownership_required",
        "zero_ambiguity_required",
        "zero_unresolved_lifecycle_required",
        "read_immediately_before_effect",
    )
    require_false(preflight, "caller_supplied_snapshot_allowed")

    durability = authority.get("durability", {})
    require_true(
        durability,
        "stage7b_i3_i4_lineage_required",
        "attempt_before_send_fsync_and_covering_seal_required",
        "outcome_unknown_requires_reconciliation",
        "redis_is_not_execution_authority",
    )
    require_false(
        durability,
        "transport_may_run_before_attempt_commit",
        "same_request_automatic_retry_after_transport_boundary",
    )

    network = authority.get("network", {})
    require_true(network, "exact_finam_host_required", "exact_method_and_route_required", "tls_required")
    require_false(network, "redirects_proxies_and_alternate_hosts_allowed", "generic_arbitrary_request_allowed")

    closed = authority.get("closed", {})
    expected_closed = {
        "stage8b_execution",
        "finam_post_delete",
        "redis_xadd_xack",
        "redis_live_consumer",
        "ack_readiness_publication",
        "broker_dispatch",
        "retry_resend_rearm",
        "runtime_live",
        "real_orders",
        "autonomous_strategy_attachment",
        "stop_sltp_bracket_replace_multi_leg",
        "unattended_or_repeated_send",
    }
    require(set(closed) == expected_closed, "closed-surface inventory drift")
    require(all(value is True for value in closed.values()), "closed surface opened")

    with MATRIX.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    require([row.get("id") for row in rows] == [f"8BD-{n:03d}" for n in range(1, 49)], "matrix IDs/count drift")
    require(all(row.get("area") and row.get("requirement") and row.get("evidence") for row in rows), "matrix row incomplete")

    negative_text = NEGATIVE.read_text(encoding="utf-8")
    numbers = [int(value) for value in re.findall(r"^(\d+)\.", negative_text, flags=re.MULTILINE)]
    require(numbers == list(range(1, 37)), "negative inventory must be exact 1..36")

    doc = DOC.read_text(encoding="utf-8")
    require_markers(
        doc,
        (
            "docs/checker-only design candidate",
            "does not authorize implementation, preflight, operator arming or a real\nrequest",
            "A LimitCancel pair\nis out of scope",
            "maximum one lot. MARKET is closed",
            "Caller-built snapshots, cached readiness, stale\nbroker truth",
            "append DispatchAttemptRecorded",
            "fsync journal",
            "write and authenticate covering Stage 7B seal",
            "outcome unknown; no retry; reconcile",
            "Timeout, disconnect, malformed/truncated success",
            "An ambiguous request is never automatically sent again",
            "Current broker truth cannot rewrite durable identity",
            "account-wide row counts cannot prove no order or flat",
            "Conflict or still unknown disarms Stage 8B",
            "Post-run closure requires exact target order truth",
            "Only FINAM may hold execution ownership",
            "Redirects, proxies, alternate hosts, arbitrary URLs",
            "Secrets and\nraw authorization headers never enter logs",
            "deterministic no-send positive rehearsal and complete fault matrix",
            "A real request remains forbidden until the later exact",
            "Independent acceptance may authorize only Stage 8B implementation\nspecification work",
        ),
    )

    if git_scope:
        require(git("branch", "--show-current") == BRANCH, "branch drift")
        subprocess.run(["git", "merge-base", "--is-ancestor", BASE, "HEAD"], cwd=ROOT, check=True)
        changed = git("diff", "--name-only", BASE, "--").splitlines()
        allowed_exact = {"README.md", "docs/current-status.md", "docs/roadmap.md", "docs/stage-8/stage8-slice-plan.md"}
        for path in changed:
            require(
                path in allowed_exact
                or path.startswith("docs/stage-8/STAGE8B_")
                or path == "docs/stage-8/stage8b-design-authority.json"
                or path.startswith("scripts/stage8b_")
                or path == "scripts/make_stage8b_design_handoff.py",
                f"design scope widened: {path}",
            )
            require(not path.startswith(("crates/", ".github/")), f"production/workflow delta: {path}")
            require(path not in ("Cargo.toml", "Cargo.lock"), f"Cargo delta: {path}")

    print("stage8b-design-check: PASS rows=48 negatives=36 design_only=true execution=false")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()
    check(git_scope=not args.no_git)


if __name__ == "__main__":
    main()
