#!/usr/bin/env python3
"""Validate Stage 5E-a lifecycle/event-time attachment inventory.

Stage 5E-a is intentionally design/inventory-only. This checker makes the
first Stage 5E boundary explicit while preserving all closed execution
surfaces.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs/stage-5/5e-a-lifecycle-event-time-attachment-plan.md"
INVENTORY = ROOT / "docs/stage-5/stage5e-lifecycle-event-time-attachment-inventory.json"
EXPECTED_STAGE5D_REF = "9ebbfd29d0346be5149dac746225866f0c8d0257"
EXPECTED_STAGE5D_SHORT_REF = "9ebbfd2"
EXPECTED_TOP_LEVEL_KEYS = {
    "allowed_changed_paths",
    "baseline_ref",
    "binding_documents",
    "closed_surfaces",
    "lifecycle_chain",
    "required_future_executable_checks",
    "schema_version",
    "source_stage5d_aggregate_closure_r2_ref",
    "source_stage5d_aggregate_closure_r2_short_ref",
    "stage",
    "stage5e_a_claims",
    "status",
    "typed_watermark_domains",
}
EXPECTED_ALLOWED_CHANGED_PATHS = {
    ".github/workflows/ci.yml",
    "README.md",
    "docs/current-status.md",
    "docs/handoff.md",
    "docs/stage-5/5e-a-lifecycle-event-time-attachment-plan.md",
    "docs/stage-5/stage5e-lifecycle-event-time-attachment-inventory.json",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/make_handoff_archive.sh",
    "scripts/stage5e_lifecycle_event_time_freeze_check.py",
    "scripts/stage5e_lifecycle_event_time_gate.sh",
}
EXPECTED_CHAIN = [
    "validated_broker_truth",
    "runtime_state_restore",
    "bootstrap_notification",
    "restored_state_notification",
    "canonical_history_warmup",
    "pending_stream_recovery",
    "first_eligible_strategy_callback",
]
EXPECTED_CLOSED_SURFACES = {
    "redis",
    "finam",
    "transport",
    "dispatch",
    "runtime_live",
    "broker_execution",
    "strategy_intent_sink",
    "autonomous_event_loop",
}
EXPECTED_CHECKS = {
    "callback_after_validated_truth_and_stage5d_restore",
    "canonical_final_m10_only",
    "monotonic_event_time_watermarks",
    "warmup_sufficiency_source_compatible",
    "reconnect_gap_proof_before_first_fresh_bar",
    "session_day_rollover_and_weekend_policy",
    "blocked_report_zero_callbacks",
    "restart_replay_determinism",
    "exact_numeric_and_semantic_adr_entry_enforcement",
    "broker_truth_fresh_through_first_callback",
    "replay_bar_observation_only_and_zero_executable_intents",
    "first_live_bar_callback_exactly_once",
    "duplicate_bar_identity_rejected",
    "invalid_bar_rejected_before_state_mutation",
    "pending_recovery_complete_before_semantic_bar",
    "unknown_session_or_clearing_state_blocks_callback",
}
EXPECTED_WATERMARK_DOMAINS = {
    "lifecycle_wall_clock_boundary": {
        "comparison_domain": "utc_wall_clock",
        "ordered_fields": [
            "checked_at_utc",
            "issued_at_utc",
            "bootstrap_notified_at_utc",
            "runtime_state_restored_at_utc",
            "warmup_started_at_utc",
            "recovery_completed_at_utc",
            "callback_processed_at_utc",
        ],
    },
    "market_event_time_boundary": {
        "comparison_domain": "market_bar_close_time",
        "ordered_fields": [
            "last_history_bar_close_ts_utc",
            "first_fresh_live_bar_close_ts_utc",
            "callback_bar_close_ts_utc",
        ],
    },
    "recovery_event_boundary": {
        "comparison_domain": "broker_event_source_time",
        "ordered_fields": [
            "broker_event_source_ts_utc",
            "recovery_completed_at_utc",
        ],
    },
    "stream_position_boundary": {
        "comparison_domain": "opaque_stream_position",
        "ordered_fields": [
            "snapshot_boundary_stream_id",
            "replay_eligible_stream_id",
        ],
        "not_comparable_with": [
            "utc_wall_clock",
            "market_bar_close_time",
            "broker_event_source_time",
        ],
    },
}
REQUIRED_DOC_MARKERS = [
    "Stage 5E-a lifecycle/event-time attachment plan",
    "design/inventory-only",
    EXPECTED_STAGE5D_REF,
    "validated broker truth",
    "runtime state restore",
    "bootstrap notification",
    "restored-state notification",
    "canonical history warmup",
    "pending stream recovery",
    "first eligible strategy callback",
    "canonical final M10",
    "first fresh semantic bar",
    "four time/position domains",
    "stream IDs are opaque positions",
    "Stage 5E-a keeps these surfaces closed",
]
REQUIRED_BINDING_DOCS = {
    "docs/stage-5/5d-final-restart-r3-aggregate-closure-r2-review-summary.md",
    "docs/adr/adr-stage5d-exact-numeric-persistence.md",
    "docs/adr/adr-stage5d-semantic-compatibility-policy.md",
    "docs/stage-5-real-strategy-semantics-plan.md",
    "docs/stage-5/5e-a-lifecycle-event-time-attachment-plan.md",
}


def fail(message: str) -> None:
    print(f"stage5e-lifecycle-event-time-freeze-check: FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text())
    except FileNotFoundError:
        fail(f"missing file: {path.relative_to(ROOT)}")
    except json.JSONDecodeError as exc:
        fail(f"invalid JSON in {path.relative_to(ROOT)}: {exc}")
    if not isinstance(value, dict):
        fail(f"top-level JSON is not object: {path.relative_to(ROOT)}")
    return value


def require_file(path: Path) -> None:
    if not path.is_file():
        fail(f"missing file: {path.relative_to(ROOT)}")


def require_unique(name: str, values: list[object]) -> None:
    if len(values) != len(set(values)):
        fail(f"{name} contains duplicates")


def check_design_only_diff(allowed_paths: set[str]) -> None:
    if not (ROOT / ".git").exists():
        return
    result = subprocess.run(
        ["git", "diff", "--name-only", EXPECTED_STAGE5D_REF, "--"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(f"could not compute baseline diff: {result.stderr.strip()}")
    changed = {line.strip() for line in result.stdout.splitlines() if line.strip()}
    unexpected = sorted(changed - allowed_paths)
    if unexpected:
        fail(f"design-only allowlist violation: {unexpected}")


def main() -> int:
    require_file(DOC)
    inventory = load_json(INVENTORY)
    doc_text = DOC.read_text()

    for marker in REQUIRED_DOC_MARKERS:
        if marker not in doc_text:
            fail(f"document marker missing: {marker}")

    if set(inventory) != EXPECTED_TOP_LEVEL_KEYS:
        fail("top-level inventory key set drift")
    if inventory.get("schema_version") != 1:
        fail("schema_version must be 1")
    if inventory.get("stage") != "5E-a-lifecycle-event-time-attachment-plan":
        fail("unexpected stage")
    if inventory.get("status") != "review_candidate_design_inventory_only":
        fail("unexpected status")
    if inventory.get("baseline_ref") != EXPECTED_STAGE5D_REF:
        fail("baseline_ref mismatch")
    if inventory.get("source_stage5d_aggregate_closure_r2_ref") != EXPECTED_STAGE5D_REF:
        fail("Stage 5D aggregate closure r2 source ref mismatch")
    if inventory.get("source_stage5d_aggregate_closure_r2_short_ref") != EXPECTED_STAGE5D_SHORT_REF:
        fail("Stage 5D aggregate closure r2 short source ref mismatch")

    allowed_paths = inventory.get("allowed_changed_paths")
    if not isinstance(allowed_paths, list) or not all(isinstance(item, str) for item in allowed_paths):
        fail("allowed_changed_paths must be a string list")
    require_unique("allowed_changed_paths", allowed_paths)
    if set(allowed_paths) != EXPECTED_ALLOWED_CHANGED_PATHS:
        fail("allowed_changed_paths drift")
    check_design_only_diff(set(allowed_paths))

    if inventory.get("lifecycle_chain") != EXPECTED_CHAIN:
        fail("lifecycle chain drift")

    closed = inventory.get("closed_surfaces")
    if not isinstance(closed, dict):
        fail("closed_surfaces must be an object")
    if set(closed) != EXPECTED_CLOSED_SURFACES:
        fail("closed_surfaces key set drift")
    opened = [name for name, value in closed.items() if value is not False]
    if opened:
        fail(f"closed surfaces opened: {opened}")

    claims = inventory.get("stage5e_a_claims")
    if not isinstance(claims, dict):
        fail("stage5e_a_claims must be an object")
    if claims.get("design_inventory_only") is not True:
        fail("Stage 5E-a must remain design/inventory-only")
    false_claims = [
        "callback_implementation_added",
        "redis_opened",
        "finam_opened",
        "transport_opened",
        "dispatch_opened",
        "runtime_live_opened",
        "broker_execution_opened",
    ]
    for key in false_claims:
        if claims.get(key) is not False:
            fail(f"claim must be false: {key}")

    binding_docs = set(inventory.get("binding_documents", []))
    if isinstance(inventory.get("binding_documents"), list):
        require_unique("binding_documents", inventory["binding_documents"])
    if binding_docs != REQUIRED_BINDING_DOCS:
        fail("binding document set drift")
    for rel_path in sorted(binding_docs):
        require_file(ROOT / rel_path)

    checks = inventory.get("required_future_executable_checks")
    if not isinstance(checks, list):
        fail("required_future_executable_checks must be a list")
    observed = set()
    for row in checks:
        if not isinstance(row, dict):
            fail("required check row must be an object")
        if set(row) != {"id", "status"}:
            fail("required check row key set drift")
        observed.add(row.get("id"))
        if row.get("status") != "planned_no_io_check":
            fail(f"required check has unexpected status: {row.get('id')}")
    require_unique("required_future_executable_checks", [row.get("id") for row in checks])
    if observed != EXPECTED_CHECKS:
        fail("required future executable check set drift")

    domains = inventory.get("typed_watermark_domains")
    if not isinstance(domains, list):
        fail("typed_watermark_domains must be a list")
    require_unique("typed_watermark_domains", [row.get("id") for row in domains if isinstance(row, dict)])
    observed_domains: dict[str, Any] = {}
    for row in domains:
        if not isinstance(row, dict):
            fail("typed watermark domain row must be an object")
        if row.get("id") == "stream_position_boundary":
            expected_keys = {"comparison_domain", "id", "not_comparable_with", "ordered_fields"}
        else:
            expected_keys = {"comparison_domain", "id", "ordered_fields"}
        if set(row) != expected_keys:
            fail(f"typed watermark domain key set drift: {row.get('id')}")
        observed_domains[row["id"]] = {k: v for k, v in row.items() if k != "id"}
    if observed_domains != EXPECTED_WATERMARK_DOMAINS:
        fail("typed watermark domain drift")

    print("stage5e-lifecycle-event-time-freeze-check: ok")
    print(f"stage5e_a_source_ref={EXPECTED_STAGE5D_REF}")
    print("closed_surfaces=redis,finam,transport,dispatch,runtime_live,broker_execution,strategy_intent_sink,autonomous_event_loop")
    print("lifecycle_chain=validated_broker_truth->runtime_state_restore->bootstrap_notification->restored_state_notification->canonical_history_warmup->pending_stream_recovery->first_eligible_strategy_callback")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
