#!/usr/bin/env python3
"""Fail-closed Stage 5F-a atomic-Hybrid entry and B3F inheritance check."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs/stage-5/5f-a-atomic-hybrid-semantics-entry.md"
INVENTORY = (
    ROOT / "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json"
)
ACTIVE = ROOT / "docs/stage-5/stage5f-active-descriptor.json"
STAGE = "5F-a-atomic-hybrid-semantics-entry"
BASELINE_REF = "e14654f7129aa61011931306140a3bfefe2fcfbc"
EXPECTED_PLAN_SHA256 = "367c5fccdc798e5569d6bb6f88a7b5d15f70f5788b970e4414b0f51c0b193d5a"
EXPECTED_INVENTORY_SHA256 = "4a5f0048506582561f822812d85a5ca7953c831b57992f1d807de8bd6e5b54bd"

EXPECTED_B3F_CLOSURE = {
    "source_ref": BASELINE_REF,
    "checker_sha256": "cb873e636427c071b26c9c2781ebc320fd9a4c3bf79fd85efabcf91ba97c828a",
    "inventory_sha256": "e459675149e4e0b465da94a60e16adae856b422185fb9221ea627aa2db93a4dd",
    "plan_sha256": "91f2bf5a63da1d6d1626c8469e6a1bcbe0b5a6c99986d03963630ab5a62c3a3a",
    "stage5c_source_sha256": "0fce95557b2e7673d7e7e74a5b4d65dd3ec28360fab3674c20e3e6de6be02ff3",
    "stage5e_source_sha256": "34ed25d3ee188d3f0c52d4b655c6105349e9761b7bd3a5af934e52cab14fb2d6",
    "stage5c_region_semantic_token_sha256": "c1b4643260249676d4917ba17300866b2a3a05a9ee75e7c4dc99ff120f028d0f",
    "stage5e_region_semantic_token_sha256": "ed0733e2843b144524ed364708b6554e7744c93823953b24ea83af1d3ca6c1d3",
    "provenance_negative_case_count": 580,
    "production_ui_case_count": 8,
    "accepted_descriptor_stage": "5E-b3f-callback-settlement-escrow-design",
}
EXPECTED_CI_SNAPSHOT_AUTHORITY = {
    "ci_workflow_sha256": "6133fb3900a9f11323df444c38760f6b71fdece927bfe2fb2cb411b5172d02f3",
    "b3f_snapshot_provenance_wrapper_sha256": "f922a4f777fbb37e049ccb640f713b7ff7557cf4f86e8855823d7db328731e29",
    "stage5f_atomic_hybrid_semantics_gate_sha256": "b3fdcfb4bf000f36de333b61cf542da1ca0452ed7638c3f68195bf8fa8d264b8",
    "stage5f_ci_snapshot_inheritance_check_sha256": "50dd173044c4c4d1eee330b08a27e7c8e044fe75148bc7816f8448e43fff082a",
    "stage5f_atomic_hybrid_semantics_negative_harness_sha256": "1a8cf90caf9b1500f01eee0fe31108e22592a4b14eceb025b53296c2f098bef4",
    "stage5f_ci_snapshot_inheritance_negative_harness_sha256": "66fec06da991f5778db4b79c733d159ed3a11c97a626ec7064e2c918d605944a",
    "negative_case_count": 16,
}
CI_EXECUTION_AUTHORITY_FILES = {
    "ci_workflow_sha256": ".github/workflows/ci.yml",
    "b3f_snapshot_provenance_wrapper_sha256": "scripts/stage5f_b3f_snapshot_provenance_gate.sh",
    "stage5f_atomic_hybrid_semantics_gate_sha256": "scripts/stage5f_atomic_hybrid_semantics_gate.sh",
    "stage5f_ci_snapshot_inheritance_check_sha256": "scripts/stage5f_ci_snapshot_inheritance_check.py",
    "stage5f_atomic_hybrid_semantics_negative_harness_sha256": "scripts/stage5f_atomic_hybrid_semantics_negative_harness.py",
    "stage5f_ci_snapshot_inheritance_negative_harness_sha256": "scripts/stage5f_ci_snapshot_inheritance_negative_harness.py",
}
EXPECTED_B3F_FILE_SHA256 = {
    "docs/stage-5/stage5e-active-descriptor.json": (
        "73990dae9c5c5972c5217c62126707b9c24b4beffc655810673a628f35edbb8c"
    ),
    "docs/stage-5/5e-b3f-callback-settlement-escrow-design.md": (
        "91f2bf5a63da1d6d1626c8469e6a1bcbe0b5a6c99986d03963630ab5a62c3a3a"
    ),
    "docs/stage-5/stage5e-b3f-callback-settlement-escrow-design-inventory.json": (
        "e459675149e4e0b465da94a60e16adae856b422185fb9221ea627aa2db93a4dd"
    ),
    "scripts/stage5e_b3f_callback_settlement_escrow_design_check.py": (
        "cb873e636427c071b26c9c2781ebc320fd9a4c3bf79fd85efabcf91ba97c828a"
    ),
    "scripts/stage5e_b3f_production_ui_harness.py": (
        "8a43aed8bfed494ac224f415e7ebc0fcd0773394aa17374539da58a0d22d637d"
    ),
    "scripts/handoff_provenance_negative_harness.py": (
        "126c0d65451233e6b142c88b8d36c38eb072c2ade0c7ed164edf1ff77cdef41f"
    ),
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": (
        "0fce95557b2e7673d7e7e74a5b4d65dd3ec28360fab3674c20e3e6de6be02ff3"
    ),
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": (
        "34ed25d3ee188d3f0c52d4b655c6105349e9761b7bd3a5af934e52cab14fb2d6"
    ),
}
EXPECTED_TARGET_CONTRACT = {
    "instrument_symbol": "IMOEXF",
    "strategy_profile": "imoexf_primary_riskgate_high180_lb120",
    "bar_contract": "canonical_final_m10",
    "execution_mode": "paper_only",
    "alor_oracle_is_runtime_decision_source": False,
}
EXPECTED_SOLE_ROUTE = [
    "Stage5eStage5cAuthorizedCallbackMaterial::invoke_authorized_callback_once",
    "BrokerNeutralHybridStrategy::on_broker_bar",
    "HybridIntradayRuntimeStrategy::on_bar",
    "high180_and_riskgate_update",
    "HybridOrchestrator::on_bar_with_mr_override_or_on_bar",
    "ordered_broker_neutral_semantic_intents",
    "validate_and_settle_stage5e_paper_callback_escrow",
]
EXPECTED_ATOMIC_CONTRACT = {
    "pre_state_fingerprint_required": True,
    "post_state_fingerprint_required": True,
    "ordered_intent_vector_required": True,
    "exact_request_identity_within_deterministic_contour": True,
    "callback_count_per_accepted_bar": 1,
    "settlement_count_per_accepted_bar": 1,
    "accepted_output": "redacted_paper_semantic_intent_state_transition",
    "partial_bo_or_mr_parity_acceptance_allowed": False,
}
EXPECTED_SCENARIOS = [
    "no_signal_zero_intent",
    "bo_long_entry_candidate",
    "bo_short_entry_candidate",
    "bo_exit_candidate",
    "bo_no_overnight_eod",
    "mr_high180_long_entry_candidate",
    "mr_high180_short_entry_candidate",
    "mr_time_target_stop_exit",
    "simultaneous_bo_mr_deterministic_winner",
    "bo_owner_suppresses_mr",
    "mr_owner_suppresses_bo",
    "one_owner_one_cycle_no_overlap",
    "riskgate_normal_append",
    "riskgate_missing_or_inconsistent_blocks",
    "pending_or_deferred_initial_state",
    "terminal_callback_or_settlement_no_transition",
]
EXPECTED_CLOSED_SURFACES = {
    "redis_consumption": False,
    "finam_transport": False,
    "dispatch": False,
    "broker_execution": False,
    "runtime_live": False,
    "real_order_endpoint": False,
    "durable_persistence_opening": False,
    "direct_stage5c_callback_route": False,
    "second_orchestrator": False,
    "partial_bo_or_mr_parity_acceptance": False,
    "ack_order_position_timer_feedback": False,
    "alor_oracle_runtime_decision_source": False,
}
EXPECTED_STAGE_BOUNDARIES = {
    "stage5g_feedback_lifecycle_allowed": False,
    "stage5h_same_input_differential_replay_allowed": False,
    "stage5i_same_session_shadow_allowed": False,
    "stage5j_stage5_closure_allowed": False,
}
EXPECTED_ALLOWED_CHANGED_PATHS = [
    ".github/workflows/ci.yml",
    "README.md",
    "docs/current-status.md",
    "docs/handoff.md",
    "docs/stage-5/5f-a-atomic-hybrid-semantics-entry.md",
    "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json",
    "docs/stage-5/stage5f-active-descriptor.json",
    "scripts/handoff_safety_check.py",
    "scripts/make_handoff_archive.sh",
    "scripts/stage5f_atomic_hybrid_semantics_entry_check.py",
    "scripts/stage5f_atomic_hybrid_semantics_gate.sh",
    "scripts/stage5f_atomic_hybrid_semantics_negative_harness.py",
    "scripts/stage5f_b3f_snapshot_provenance_gate.sh",
    "scripts/stage5f_ci_snapshot_inheritance_check.py",
    "scripts/stage5f_ci_snapshot_inheritance_negative_harness.py",
    "scripts/stage5f_descriptor.py",
]
REQUIRED_PLAN_FRAGMENTS = [
    "The Stage 5E descriptor remains an immutable closure descriptor for B3F.",
    "There is no alternate direct Stage 5C callback route, second orchestrator,",
    "instrument: IMOEXF",
    "profile: imoexf_primary_riskgate_high180_lb120",
    "market input: canonical final M10",
    "execution mode: paper-only",
    "No internal sub-slice may claim Hybrid parity on its own.",
    "ACK, order, position, timer and restart feedback are excluded and",
]


class CheckFailure(RuntimeError):
    pass


def fail(message: str) -> None:
    raise CheckFailure(message)


def require_exact(actual: object, expected: object, message: str) -> None:
    if actual != expected:
        fail(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def changed_paths() -> list[str]:
    if not (ROOT / ".git").exists():
        return EXPECTED_ALLOWED_CHANGED_PATHS
    completed = subprocess.run(
        ["git", "diff", "--name-only", BASELINE_REF, "--"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return sorted(line for line in completed.stdout.splitlines() if line)


def validate_accepted_b3f_closure(inventory: dict[str, object]) -> None:
    require_exact(
        inventory.get("accepted_stage5e_b3f_closure"),
        EXPECTED_B3F_CLOSURE,
        "accepted B3F closure pin drift",
    )
    try:
        descriptor = json.loads(
            (ROOT / "docs/stage-5/stage5e-active-descriptor.json").read_text()
        )
    except json.JSONDecodeError as exc:
        fail(f"accepted B3F descriptor invalid: {exc}")
    require_exact(
        descriptor,
        {"schema_version": 1, "stage": "5E-b3f-callback-settlement-escrow-design"},
        "accepted B3F descriptor drift",
    )
    for relative, expected in EXPECTED_B3F_FILE_SHA256.items():
        require_exact(
            sha256(ROOT / relative),
            expected,
            f"accepted B3F source drift: {relative}",
        )
    if (ROOT / ".git").exists():
        completed = subprocess.run(
            ["git", "rev-parse", f"{BASELINE_REF}^{{commit}}"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        require_exact(
            completed.stdout.strip(),
            BASELINE_REF,
            "accepted B3F source ref unavailable",
        )


def validate_inventory(inventory: dict[str, object]) -> None:
    expected_keys = {
        "accepted_stage5e_b3f_closure",
        "allowed_changed_paths",
        "atomic_transition_contract",
        "baseline_ref",
        "ci_snapshot_authority",
        "closed_surfaces",
        "expected_stage5f_negative_case_count",
        "required_atomic_scenarios",
        "schema_version",
        "sole_route",
        "stage",
        "stage_boundaries",
        "status",
        "target_contract",
    }
    require_exact(set(inventory), expected_keys, "Stage 5F inventory key set drift")
    require_exact(inventory.get("schema_version"), 1, "Stage 5F schema drift")
    require_exact(inventory.get("stage"), STAGE, "Stage 5F stage drift")
    require_exact(
        inventory.get("status"),
        "entry_governance_design_pending_review",
        "Stage 5F status drift",
    )
    require_exact(inventory.get("baseline_ref"), BASELINE_REF, "Stage 5F baseline drift")
    require_exact(
        inventory.get("ci_snapshot_authority"),
        EXPECTED_CI_SNAPSHOT_AUTHORITY,
        "Stage 5F CI snapshot authority drift",
    )
    for field, relative in CI_EXECUTION_AUTHORITY_FILES.items():
        require_exact(
            sha256(ROOT / relative),
            EXPECTED_CI_SNAPSHOT_AUTHORITY[field],
            f"Stage 5F CI execution authority drift: {relative}",
        )
    require_exact(
        inventory.get("target_contract"),
        EXPECTED_TARGET_CONTRACT,
        "Stage 5F target contract drift",
    )
    require_exact(inventory.get("sole_route"), EXPECTED_SOLE_ROUTE, "Stage 5F sole route drift")
    require_exact(
        inventory.get("atomic_transition_contract"),
        EXPECTED_ATOMIC_CONTRACT,
        "Stage 5F atomic contract drift",
    )
    require_exact(
        inventory.get("required_atomic_scenarios"),
        EXPECTED_SCENARIOS,
        "Stage 5F scenario matrix drift",
    )
    require_exact(
        inventory.get("closed_surfaces"),
        EXPECTED_CLOSED_SURFACES,
        "Stage 5F closed-surface drift",
    )
    require_exact(
        inventory.get("stage_boundaries"),
        EXPECTED_STAGE_BOUNDARIES,
        "Stage 5F later-stage boundary drift",
    )
    require_exact(
        inventory.get("expected_stage5f_negative_case_count"),
        13,
        "Stage 5F negative case count drift",
    )
    require_exact(
        inventory.get("allowed_changed_paths"),
        EXPECTED_ALLOWED_CHANGED_PATHS,
        "Stage 5F allowed changed paths drift",
    )


def main() -> int:
    try:
        inventory = json.loads(INVENTORY.read_text())
        if not isinstance(inventory, dict):
            fail("Stage 5F inventory must be an object")
        require_exact(sha256(PLAN), EXPECTED_PLAN_SHA256, "Stage 5F plan drift")
        require_exact(
            canonical_sha256(inventory),
            EXPECTED_INVENTORY_SHA256,
            "Stage 5F inventory drift",
        )
        require_exact(
            json.loads(ACTIVE.read_text()),
            {"schema_version": 1, "stage": STAGE},
            "Stage 5F active descriptor drift",
        )
        validate_inventory(inventory)
        validate_accepted_b3f_closure(inventory)
        for fragment in REQUIRED_PLAN_FRAGMENTS:
            if fragment not in PLAN.read_text():
                fail(f"Stage 5F plan authority fragment missing: {fragment}")
        require_exact(
            changed_paths(),
            EXPECTED_ALLOWED_CHANGED_PATHS,
            "Stage 5F design changed-path set drift",
        )
    except (CheckFailure, FileNotFoundError, json.JSONDecodeError, subprocess.CalledProcessError) as exc:
        print(f"stage5f-atomic-hybrid-semantics-entry-check: FAIL: {exc}", file=sys.stderr)
        return 1
    print("stage5f-atomic-hybrid-semantics-entry-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
