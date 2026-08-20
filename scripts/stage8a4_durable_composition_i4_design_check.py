#!/usr/bin/env python3
"""Validate the docs/checker-only Stage 8A-4 I4 Design R3 contract."""

from __future__ import annotations

import csv
import json
import os
import subprocess
from pathlib import Path

ROOT = Path(os.environ.get("STAGE8A4_I4_ROOT", Path(__file__).resolve().parents[1]))
DOC = ROOT / "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_DESIGN_2026-08-20.md"
MATRIX = ROOT / "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_DESIGN_ACCEPTANCE_MATRIX_2026-08-20.csv"
NEGATIVE = ROOT / "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_DESIGN_NEGATIVE_INVENTORY_2026-08-20.md"
AUTHORITY = ROOT / "docs/stage-8/stage8a4-durable-composition-i4-design-authority.json"
PREDECESSOR = "593ff255ef7826a22e66c9aff6f7ea47acf47644"
REVIEW_SHA = "1da167c3e7f1266473133d2d8a1412906a26d7f83b5dc026ce84dc7969090257"
REJECTED_R1 = "06bb09fa13431d0ae34039f37497d4f37914f022"
REJECTED_R2 = "d1a050a53d95a3d53874bf0866e3598b948dde68"


def fail(message: str) -> None:
    raise SystemExit(f"stage8a4-durable-composition-i4-design-check: FAIL {message}")


def require(text: str, *needles: str) -> None:
    for needle in needles:
        if needle not in text:
            fail(f"missing design contract: {needle}")


def require_true(section: dict[str, object], *keys: str) -> None:
    for key in keys:
        if section.get(key) is not True:
            fail(f"authority weakened: {key}")


def require_false(section: dict[str, object], *keys: str) -> None:
    for key in keys:
        if section.get(key) is not False:
            fail(f"authority opened: {key}")


def main() -> None:
    authority = json.loads(AUTHORITY.read_text(encoding="utf-8"))
    expected = {
        "schema_version": 3,
        "stage": "8A-4-durable-composition-I4-design-R3",
        "status": "design_candidate",
        "accepted_predecessor_ref": PREDECESSOR,
        "accepted_predecessor_review_sha256": REVIEW_SHA,
        "rejected_r1_ref": REJECTED_R1,
        "rejected_r2_ref": REJECTED_R2,
        "acceptance_rows": 64,
        "negative_cases": 46,
        "scope": "derived_ack_and_current_readiness_read_only_no_effect",
        "next_after_acceptance": "I4 controlled read-only no-effect implementation",
    }
    for key, value in expected.items():
        if authority.get(key) != value:
            fail(f"authority {key} drift")

    terminal = authority.get("terminal_authority", {})
    require_true(
        terminal,
        "complete_v2_exact_suffix_required",
        "request_finalized_required",
        "covering_s1_required",
        "existing_s1_must_cover_current_frontier",
        "lagging_s1_fails_closed",
        "restart_reconstruction_required",
    )
    require_false(
        terminal,
        "seal_advance_or_repair_allowed",
        "pending_or_hold_authorized",
        "receipt_alone_is_authority",
    )

    ack = authority.get("ack_facts", {})
    exact_ack = {
        "request_id_source": "durable_stage6_request_identity",
        "client_order_id_source": "durable_request_client_order_id",
        "broker_order_id_source": "durable_mixed_replay_known_broker_order_id_only",
        "timestamp_model": "timestamp_free_model_a",
        "stable_identity": "reuse_exact_stage7b_terminal_request_ack_identity_sha256",
    }
    for key, value in exact_ack.items():
        if ack.get(key) != value:
            fail(f"ACK facts {key} drift")
    require_true(ack, "trade_established_broker_id_survives_idless_selected_order")
    require_false(
        ack,
        "cancel_target_client_id_can_replace_ack_client_id",
        "current_truth_can_fill_broker_order_id",
        "constructs_full_command_ack",
        "received_ts_in_stable_identity",
        "second_request_identity_domain_allowed",
        "current_seal_checkpoint_or_readiness_in_stable_identity",
    )

    readiness = authority.get("current_readiness", {})
    if readiness.get("issuer_owner") != "current_stage7b_recovery_ready_owner":
        fail("readiness issuer drift")
    require_true(
        readiness,
        "independent_from_terminal_ack",
        "accepted_stage8a1_root_and_config_required",
        "fresh_run_allowed_required",
        "fresh_composite_and_broker_truth_required",
        "exact_scope_binding_required",
        "account_active_orders_must_be_zero",
        "target_active_orders_must_be_zero",
        "stop_stale_unreadable_unknown_or_orphan_block",
        "observed_at_and_valid_until_required",
        "valid_until_is_minimum_source_expiry",
        "source_revision_change_invalidates",
    )
    require_false(
        readiness,
        "operator_arm_required_or_minted",
        "execution_capability_required_or_minted",
        "cached_ready_survives_restart",
        "i3_post_effect_snapshot_reusable",
    )

    boundary = authority.get("read_only_boundary", {})
    require_true(
        boundary,
        "seal_reread_authentication_allowed",
        "mixed_replay_refresh_allowed",
        "current_source_reads_allowed",
        "broker_truth_readiness_sampling_allowed",
    )
    require_false(
        boundary,
        "journal_or_suffix_append_allowed",
        "request_finalized_append_allowed",
        "seal_write_advance_repair_allowed",
        "ack_or_readiness_publication_allowed",
    )

    topology = authority.get("cross_crate_topology", {})
    topology_exact = {
        "terminal_authority_owner_crate": "runtime-durable-service",
        "sole_issuer": "Stage7bRecoveryReadyOwner",
        "downstream_consumer_crate": "finam-gateway",
    }
    for key, value in topology_exact.items():
        if topology.get(key) != value:
            fail(f"cross-crate topology {key} drift")
    require_true(
        topology,
        "terminal_authority_public_type",
        "finam_to_runtime_durable_dependency_allowed",
        "external_compile_fail_required",
    )
    require_false(
        topology,
        "terminal_authority_public_constructor",
        "terminal_authority_public_fields",
        "terminal_authority_clone",
        "terminal_authority_copy",
        "terminal_authority_debug",
        "terminal_authority_serialize",
        "terminal_authority_deserialize",
        "runtime_durable_to_finam_dependency_allowed",
        "raw_public_facts_can_replace_authority",
        "finam_can_authenticate_stage7b_seal",
        "ack_facts_readiness_facade_public",
    )

    closed = authority.get("closed", {})
    expected_closed = {
        "redis_ack_xack", "redis_live", "finam_post_delete", "broker_dispatch",
        "retry_resend_rearm", "runtime_live", "real_orders", "stage8a5", "stage8b",
    }
    if set(closed) != expected_closed or not all(value is True for value in closed.values()):
        fail("closed surface opened")

    with MATRIX.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    expected_ids = [f"I4D-{number:03d}" for number in range(1, 65)]
    if [row.get("id") for row in rows] != expected_ids:
        fail("acceptance matrix must be exact I4D-001..I4D-064")
    matrix_contract = "\n".join(row.get("requirement", "") for row in rows)
    require(
        matrix_contract,
        "unknown or orphan account safety blocks readiness",
        "duplicate derivation appends no journal record",
        "facade and authorities are nonserializable opaque types",
        "CANCEL target client ID cannot replace ACK client ID",
        "trade-established B1 survives an ID-less selected V2 order",
        "current broker truth cannot synthesize ACK broker ID",
        "ACK facts are timestamp-free and publication owns received_ts",
        "existing Stage7B terminal identity is reused exactly as sole ACK identity",
        "any account-wide or target active order blocks generic Ready",
        "read-side S1 replay and current-source checks are allowed while every mutation is forbidden",
        "lagging S1 fails closed and I4 never repairs or advances it",
        "Stage7B terminal authority is public-opaque solely for cross-crate use",
        "Stage7bRecoveryReadyOwner is the sole terminal-authority issuer",
        "terminal authority has no public constructor or fields",
        "terminal authority is non-Clone Copy Debug Serialize Deserialize",
        "finam-gateway consumes authority through the existing dependency direction",
        "runtime-durable-service remains free of finam-gateway dependency",
        "ACK facts readiness evidence and facade remain FINAM crate-private",
        "no raw journal seal checkpoint digest or receipt constructor can mint terminal authority",
    )

    design = DOC.read_text(encoding="utf-8")
    require(
        design,
        PREDECESSOR,
        REVIEW_SHA,
        REJECTED_R1,
        REJECTED_R2,
        "read-only / no-effect composition",
        "pub struct Stage7bStage8a4TerminalAuthority { /* private fields */ }",
        "public-opaque broker-neutral durable capability",
        "Only `Stage7bRecoveryReadyOwner` can issue it",
        "`finam-gateway -> runtime-durable-service` is allowed",
        "`runtime-durable-service -> finam-gateway` is forbidden",
        "caller-settable public facts struct",
        "serializable proof DTO accepted as authority",
        "duplicate Stage7B",
        "Stage7bStage8a4TerminalAuthority",
        "Stage8a4I4TerminalAckFacts",
        "Stage8a4I4CurrentReadinessEvidence",
        "Stage8a4I4DerivedAckReadinessFacade",
        "There is no public constructor",
        "strategy_request_id: StrategyRequestId",
        "durable_client_order_id: ClientOrderId",
        "broker_order_id: Option<BrokerOrderId>",
        "status: CommandAckStatus",
        "reason_code: Option<CommandAckReasonCode>",
        "cancel request client ID, never the target order's",
        "BrokerTradeObserved",
        "ACK broker ID is `Some(B1)`",
        "current broker truth cannot fill it",
        "Model A",
        "durable ACK facts are timestamp-free",
        "I4 does not construct a full `CommandAck`",
        "`received_ts` belongs to a future publication",
        "`Utc::now()` is forbidden",
        "reuses the existing Stage 7B",
        "`terminal_request_ack_identity_sha256` **exactly**",
        "introduces no second",
        "Stage8a1AuthorityRoot",
        "Stage8a1AcceptedExecutionConfigV1",
        "Stage7bCompositeReadinessSnapshot",
        "BrokerTruthSnapshot",
        "BrokerReadinessSnapshot",
        "not supplied as public caller\nsnapshots",
        "Stage8ExecutionCapability",
        "account_active_orders_count == 0",
        "target_active_orders_count == 0",
        "valid_until = min(all current trusted source expiries)",
        "now < valid_until",
        "No cached `Ready` survives restart",
        "advance_recovery_seal(...)`",
        "cannot be reused unchanged",
        "Stage 6 journal append",
        "Redis `XADD`/`XACK`",
        "Required external-crate compile-fail proof",
        "Stage7bStage8a4TerminalAuthority::new(...)",
        "value.clone()",
        "serde_json::to_vec(&value)",
        "format!(\"{value:?}\")",
        "ExactWorking | none | none | unresolved",
        "ExactTerminalRejected | Rejected | Rejected | BrokerRejected",
        "ExactTerminalFilled | ExecutionObserved | Recovered | RecoveredByBrokerTruth",
        "ExactTerminalRejected | AlreadyTerminalNonExecution | Recovered | RecoveredByBrokerTruth",
        "ExactTerminalCancelled | Canceled | Recovered | RecoveredByBrokerTruth",
        "ExactTerminalExpired | AlreadyTerminalNonExecution | Recovered | RecoveredByBrokerTruth",
    )
    if "This design opens only a no-I/O" in design or "Each step remains no-I/O" in design:
        fail("obsolete no-I/O boundary remains")

    negative = NEGATIVE.read_text(encoding="utf-8")
    count = sum(1 for line in negative.splitlines() if line[:1].isdigit() and ". " in line)
    if count != 46:
        fail("negative inventory must contain 46 numbered cases")

    # Design R2 must leave all production/test/Cargo/workflow files byte-equal
    # to the independently accepted I3 R6 predecessor.
    if (ROOT / ".git").exists():
        changed = subprocess.check_output(
            [
                "git", "diff", "--name-only", PREDECESSOR, "--",
                "Cargo.toml", "Cargo.lock", "crates", "tests", ".github/workflows",
            ],
            cwd=ROOT,
            text=True,
        ).strip()
        if changed:
            fail(f"production/test/Cargo/workflow source changed in design slice: {changed}")

    source = "\n".join(
        path.read_text(encoding="utf-8", errors="ignore")
        for base in (ROOT / "crates", ROOT / "tests")
        if base.exists()
        for path in base.rglob("*.rs")
    )
    if "Stage8a4I4DerivedAckReadinessFacade" in source:
        fail("I4 implementation opened in design slice")
    print(
        "stage8a4-durable-composition-i4-design-check: PASS "
        "revision=R3 rows=64 negatives=46 implementation=false "
        "ack_publish=false redis=false finam=false live=false"
    )


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, json.JSONDecodeError) as error:
        fail(str(error))
