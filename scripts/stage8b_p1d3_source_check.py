#!/usr/bin/env python3
"""Fail-closed source/scope checker for Stage 8B-P1-d3."""

from __future__ import annotations

import csv
import hashlib
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
ACCEPTED_DESIGN = "df330b2424199739ceb7c261321a5e5ee381c332"
ACCEPTED_P1D2 = "bcd8db546104968dd0e48ab041e02acf6869d224"
EXPECTED_CHANGED = {
    "crates/runtime-durable-service/src/lib.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "crates/strategy-runtime-core/src/stage5g_p1_semantic.rs",
    "crates/strategy-runtime-core/src/stage6_durable_identity.rs",
    "crates/strategy-runtime-core/src/stage6_reconciliation_v2.rs",
    "crates/strategy-runtime-core/src/stage6_replay.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs",
    "crates/strategy-runtime-core/src/stage8b_p1d3_working_limit.rs",
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d3-source-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d3-source-evidence.json",
    "docs/stage-8/stage8b-p1d3-working-limit-cancel-source.md",
    "fixtures/stage8a4-i1/canonical-golden-sha256.json",
    "fixtures/stage8b-p1d3/outcome-golden-v1.json",
    "fixtures/stage8b-p1d3/projection-golden-v1.json",
    "scripts/make_stage8b_p1d3_source_handoff.py",
    "scripts/stage8b_p1d3_source_check.py",
    "scripts/stage8b_p1d3_source_gate.sh",
    "scripts/stage8b_p1d3_source_handoff_safety_check.py",
    "scripts/stage8b_p1d3_source_negative_harness.py",
}
EXPECTED_SHAPES = {
    "initial_working",
    "initial_filled",
    "initial_expired",
    "later_filled",
    "later_expired",
    "cancel_canceled",
    "cancel_execution_observed",
    "cancel_already_terminal_non_execution",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", ACCEPTED_DESIGN],
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


def load_content(root: pathlib.Path = ROOT) -> dict[str, str]:
    paths = {
        "core": "crates/strategy-runtime-core/src/stage8b_p1d3_working_limit.rs",
        "p1d2": "crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs",
        "stage5c": "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
        "restart": "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
        "order_position": "crates/strategy-runtime-core/src/stage5g_order_position.rs",
        "semantic": "crates/strategy-runtime-core/src/stage5g_p1_semantic.rs",
        "stage6": "crates/strategy-runtime-core/src/stage6d_live_core.rs",
        "service": "crates/runtime-durable-service/src/recovery.rs",
        "redis": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
        "service_lib": "crates/runtime-durable-service/src/lib.rs",
        "doc": "docs/stage-8/stage8b-p1d3-working-limit-cancel-source.md",
        "matrix": "docs/stage-8/stage8b-p1d3-source-acceptance-matrix.csv",
        "evidence": "docs/stage-8/stage8b-p1d3-source-evidence.json",
        "golden": "fixtures/stage8b-p1d3/outcome-golden-v1.json",
        "projection_golden": "fixtures/stage8b-p1d3/projection-golden-v1.json",
        "status": "docs/current-status.md",
        "roadmap": "docs/roadmap.md",
    }
    return {key: (root / path).read_text(encoding="utf-8") for key, path in paths.items()}


def section(text: str, start: str, end: str) -> str:
    begin = text.find(start)
    require(begin >= 0, f"section start missing: {start}")
    finish = text.find(end, begin + len(start))
    require(finish >= 0, f"section end missing: {end}")
    return text[begin:finish]


def validate_content(content: dict[str, str]) -> None:
    core = content["core"]
    p1d2 = content["p1d2"]
    stage5c = content["stage5c"]
    stage6 = content["stage6"]
    service = content["service"]
    redis_source = content["redis"]
    service_lib = content["service_lib"]
    document = content["doc"]

    for token in (
        'STAGE8B_P1D3_WORKING_BOOK_DOMAIN: &str = "moex.stage8b.p1d3.working-book.v1"',
        'STAGE8B_P1D3_BOOK_TRANSITION_DOMAIN: &str = "moex.stage8b.p1d3.book-transition.v1"',
        'STAGE8B_P1D3_OUTCOME_EVIDENCE_DOMAIN: &str = "moex.stage8b.p1d3.outcome-evidence.v1"',
        'STAGE8B_P1D3_BOOK_GENESIS_DOMAIN: &str = "moex.stage8b.p1d3.book-genesis.v1"',
        "P1D3_MAX_ORDER_RECORDS_PER_GENERATION: usize = 1024",
        "pub(crate) struct Stage8bP1d3WorkingBookProjectionV1",
        "pub(crate) struct Stage8bP1d3OutcomeEvidenceV1",
        "records: Vec<Stage8bP1d3OrderRecordV1>",
        "account_id: BrokerAccountId",
        "qty_decimal_bytes: [u8; 16]",
        "limit_price_decimal_bytes: [u8; 16]",
        "fill_price_decimal_bytes: Option<[u8; 16]>",
        "migrate_stage8b_p1d3_from_p1d2",
        "recover_stage8b_p1d3_outcome_transition",
        "build_initial_limit_transition",
        "build_later_limit_transition",
        "build_cancel_transition",
        "build_cancel_after_target_transition",
        "Stage8bP1d3EvaluationStageResult::AlreadyEvaluated",
        "Stage8bP1d3BookPhase::CancelRecovered",
        "previous_order_id.is_some_and(|previous| previous >= order_id)",
        ".as_bytes()\n                .cmp(right.broker_order_id.as_str().as_bytes())",
        "self.records.len() >= P1D3_MAX_ORDER_RECORDS_PER_GENERATION",
        "Stage8bP1d3ConsumedWitnessKind::DayExpiry",
        "all_eight_fresh_and_recovery_paths_match_checked_in_canonical_goldens",
        "canonical_terminal_registry_is_independent_of_in_memory_insertion_order",
        "position_projection_reuses_the_complete_p1d2_arithmetic_matrix",
        "distinct_request_ids_with_the_same_truncated_client_id_fail_closed_for_cancel",
        "crate::stage8b_p1d2_market_feedback::resulting_position(",
        "self.durable_request_client_id.as_ref() == self.target_place_client_id.as_ref()",
        "input.target_place_client_id.as_ref() == Some(&input.durable_request_client_id)",
        '"ack": ack',
        '"orders": orders',
        '"trades": trades',
        '"positions": positions',
        '"truth": truth',
        '"orders": exact_order_rows(Some(truth))',
        '"unrealized_pnl": optional_decimal_bytes(row.unrealized_pnl)',
        '"snapshot": row',
        "fresh_and_recovery_each_checked_against_oracle: true",
        'join("../../fixtures/stage8b-p1d3/projection-golden-v1.json")',
        ".target_order_client_order_id()\n                .map_or(true, |supplied| {",
    ):
        require(token in core, f"core invariant missing: {token}")
    for token in (
        "pub(crate) fn resulting_position(",
        "q0 == Decimal::ZERO && a0.is_some()",
        "q0 != Decimal::ZERO && a0.is_none()",
        "else if q1.is_sign_positive() == q0.is_sign_positive()",
        "canonical.rescale(STAGE8B_P1D2_AVG_PRICE_SCALE);",
        "canonical.scale() != STAGE8B_P1D2_AVG_PRICE_SCALE",
        "RoundingStrategy::MidpointNearestEven",
    ):
        require(token in p1d2, f"canonical P1-d2 position reducer invariant missing: {token}")
    require(
        core.count('"snapshot": row') >= 4,
        "complete order/trade/position/instrument snapshot projection weakened",
    )
    for outcome in (
        "InitialWorking",
        "InitialFilled",
        "InitialExpired",
        "LaterFilled",
        "LaterExpired",
        "CancelCanceled",
        "CancelExecutionObserved",
        "CancelAlreadyTerminalNonExecution",
    ):
        require(outcome in core, f"outcome missing: {outcome}")
    for forbidden in ("redis::", "reqwest::", "Utc::now()", ": f32", ": f64"):
        require(forbidden not in core, f"pure reducer opened forbidden input: {forbidden}")
    require(
        "// The current digest is excluded from the bytes it authenticates." in core,
        "self-referential digest exclusion is undocumented",
    )
    require(
        "OrderSide::Buy if bar.low > limit_price" in core
        and "Some(if bar.open < limit_price" in core,
        "buy touch pricing drifted",
    )
    require(
        "OrderSide::Sell if bar.high < limit_price" in core
        and "Some(if bar.open > limit_price" in core,
        "sell touch pricing drifted",
    )

    for token in (
        "Stage6JournalRecordV3",
        "Stage6JournalRecordVersioned::V3(outcome_record.clone())",
        "Stage8bP1d3OutcomeEvidenceV1::decode_canonical",
        "outcome_record.outcome_evidence_bytes()",
        "Stage6Stage8bP1d3RestartPhase::ReadyForEvaluation",
        "Stage6Stage8bP1d3RestartPhase::AckCommitted",
        "Stage6Stage8bP1d3RestartPhase::TruthCommitted",
        "Stage6Stage8bP1d3RestartPhase::CancelContinuationPending",
        "Stage6Stage8bP1d3RestartPhase::SemanticCallbackPending",
        "Stage6Stage8bP1d3RestartPhase::SemanticCallbackCommitted",
        "continue_stage8b_p1d3_cancel_after_target_transition",
        "projected_checkpoint_after_append",
        "rebind_stage8b_p1d3_one_intent_request_checkpoint",
        "source.rebind_semantic_request_checkpoint(",
        "fn canonical_stage8b_p1d3_cancel_target_client_id(",
        "durable_cancel_client_order_id == authenticated_target_client_order_id",
        ".is_some_and(|supplied| supplied != authenticated_target_client_order_id)",
        "Ok(authenticated_target_client_order_id.clone())",
        "target_place_client_id: Some(canonical_target_place_client_id)",
        ".target_order_client_order_id()\n                    .is_some_and(|supplied| {",
    ):
        require(token in stage6, f"Stage6/restart invariant missing: {token}")
    for token in (
        "pub fn stage8b_p1d3_test_materialize_host_cancel_command(",
        "BrokerNeutralHybridIntent::Cancel {",
        "stage8b_p1_single_intent_command_material_from_parts(strategy, &batch)",
        "client_order_id: None",
    ):
        require(token in stage5c, f"actual Stage5C host CANCEL proof missing: {token}")

    order_position_authority = section(
        content["restart"],
        "OrderPositionAwaitingCommitted {",
        "ProtectiveLifecycleCommitted {",
    )
    require(
        "p1_semantic_commit: Option<Box<Stage5gP1SemanticCommitProjectionV1>>"
        in order_position_authority,
        "order-position restart dropped the pending P1 semantic commit",
    )
    fresh_truth_projection = section(
        content["restart"],
        "pub(crate) fn fresh_truth_reducer_projection(&self)",
        "pub fn export_stage5g_clean_restart(",
    )
    require(
        "and_then(crate::stage5g_p1_semantic::p1_prepublication_restart_slot)"
        in fresh_truth_projection,
        "order-position restart no longer overlays the pending P1 semantic slot",
    )
    for token in (
        "pub(crate) fn rebind_semantic_request_checkpoint(",
        "authenticated_post_checkpoint_sha256 == expected_pre_checkpoint_sha256",
    ):
        require(token in core, f"semantic checkpoint rebind invariant missing: {token}")

    for token in (
        "Stage8bP1d3PreAckPendingOwner",
        "Stage8bP1d3AckCommittedOwner",
        "Stage8bP1d3TruthCommittedOwner",
        "Stage8bP1d3CancelContinuationOwner",
        "Stage8bP1d3SemanticPendingOwner",
        "resume_stage8b_p1d3_journal_ahead_transition",
    ):
        require(token in service, f"durable lifecycle invariant missing: {token}")
    require(
        service.count("p1d3-after-recovered-cancel-before-s-cancel-recovered") == 2,
        "recovered-cancel pre-seal barrier coverage drifted",
    )
    require(
        service.count("p1d3-after-s-cancel-recovered-before-source-xack") == 2,
        "recovered-cancel post-seal barrier coverage drifted",
    )

    ack_impl = section(
        redis_source,
        "impl Stage8bP1RedisLimitAckCommitted {",
        "impl Stage8bP1RedisCancelContinuationPending {",
    )
    cancel_impl = section(
        redis_source,
        "impl Stage8bP1RedisCancelContinuationPending {",
        "impl Stage8bP1RedisLimitTruthCommitted {",
    )
    truth_impl = section(
        redis_source,
        "impl Stage8bP1RedisLimitTruthCommitted {",
        "impl Stage8bP1RedisLimitResolved {",
    )
    require("pub fn commit_truth(" in ack_impl, "S_ack truth-only continuation missing")
    require("acknowledge_source" not in ack_impl, "S_ack gained early XACK")
    require("pub fn commit_recovered_cancel(" in cancel_impl, "cancel seal continuation missing")
    require("acknowledge_source" not in cancel_impl, "cancel continuation gained early XACK")
    require("commit_truth" not in cancel_impl, "cancel continuation can skip recovered seal")
    require("pub async fn acknowledge_source(" in truth_impl, "truth source XACK missing")
    require("commit_truth" not in truth_impl, "truth owner gained duplicate truth")
    require("acknowledge_exact(&self.pending_m10)" in truth_impl, "exact source XACK drifted")
    for token in (
        "exact_first_successor_m10",
        "process_next_working_limit",
        "p1d3_read_only_successor_observation_retains_original_source_until_xack",
        "Stage8bP1d3LaterCommitOutcome::SemanticPending",
        "resume_stage8b_p1d3_cancel_continuation_with_redis",
        "resume_stage8b_p1d3_semantic_with_redis",
        "p1d3_subprocess_sigkill_brackets_s_cancel_recovered",
        "p1d3_cancel_recovery_crash_frontier_child",
        "P1d3CancelExpectedRestart::PreRecoveredSeal",
        "P1d3CancelExpectedRestart::Truth",
        "child.kill().unwrap()",
        "0xd301_0000_0000_4000_8000_0000_0000_0001",
        "0xd302_0000_0000_4000_8000_0000_0000_0001",
        "p1d3_actual_host_optional_tcid_completes_recovered_cancel_and_xacks_last",
        "p1d3_colliding_cancel_dcid_fails_before_dispatch_for_optional_or_exact_tcid",
        "p1d3_optional_tcid_target_first_restart_continues_without_duplicate_effects",
    ):
        require(token in redis_source, f"Redis composition invariant missing: {token}")

    for token in (
        "Stage8bP1RedisLimitAckCommitted",
        "ack.acknowledge_source()",
        "Stage8bP1RedisLimitTruthCommitted",
        "truth.commit_truth(",
        "Stage8bP1RedisCancelContinuationPending",
        "pending.acknowledge_source()",
        "Stage8bP1d3SemanticPendingOwner",
        "pending.commit_later_limit(",
    ):
        require(token in service_lib, f"compile-fail boundary missing: {token}")

    for token in (
        "R2 optional-TCID source-correction review candidate",
        "initial LIMIT Working/Filled/Expired",
        "CancelExecutionObserved",
        "S_cancel_recovered",
        "exact source XACK last",
        "read with `XRANGE`",
        "operational Redis DB 0",
        "FINAM POST/DELETE",
        "P1-d4",
        "projection-golden-v1.json",
        "global mapper still deliberately remains outside this narrow slice",
        "authenticated target registry row",
        "Before `DispatchAttemptRecorded`",
        "actual Stage 5C host",
    ):
        require(token in document, f"implementation document missing: {token}")

    rows = list(csv.DictReader(content["matrix"].splitlines()))
    require(len(rows) == 59, f"acceptance row count drifted: {len(rows)}")
    require(
        {row.get("id") for row in rows} == {f"P1D3S-{index:03d}" for index in range(1, 60)},
        "acceptance IDs drifted",
    )
    require(all(row.get("status") == "REQUIRED" for row in rows), "acceptance weakened")

    evidence = json.loads(content["evidence"])
    require(evidence.get("schema_version") == 1, "evidence schema drifted")
    require(
        evidence.get("stage") == "Stage 8B-P1-d3 working LIMIT/CANCEL/expiry source",
        "stage drifted",
    )
    require(evidence.get("status") == "SOURCE_IMPLEMENTATION_REVIEW_CANDIDATE", "status drifted")
    require(evidence.get("accepted_design_ref") == ACCEPTED_DESIGN, "design lineage drifted")
    require(evidence.get("accepted_p1d2_closure_ref") == ACCEPTED_P1D2, "P1-d2 lineage drifted")
    require(evidence.get("acceptance_rows") == 59, "evidence row count drifted")
    require(evidence.get("negative_cases") == 43, "negative count drifted")
    implementation = evidence.get("implementation", {})
    require(implementation.get("golden_shapes") == 8, "golden evidence count drifted")
    require(
        implementation.get("complete_projection_golden_shapes") == 8,
        "complete projection golden count drifted",
    )
    require(
        implementation.get("canonical_position_reducer") == "stage8b_p1d2_market_feedback::resulting_position"
        and implementation.get("cancel_dcid_tcid_inequality_enforced") is True,
        "P1-d3 R1 source hardening evidence weakened",
    )
    require(implementation.get("max_order_records_per_generation") == 1024, "capacity drifted")
    require(implementation.get("full_outcome_evidence_in_stage6_v3") is True, "V3 evidence weakened")
    require(implementation.get("source_xack_last") is True, "XACK-last weakened")
    require(
        implementation.get("subprocess_sigkill_cancel_recovery_frontiers") == 2,
        "cancel-recovery subprocess frontier proof drifted",
    )
    require(
        implementation.get("request_accepted_checkpoint_rebound_before_replacement_export")
        is True,
        "RequestAccepted checkpoint rebind weakened",
    )
    require(
        implementation.get("pending_semantic_slot_survives_order_position_restart") is True,
        "pending semantic restart slot weakened",
    )
    require(
        implementation.get("host_cancel_target_client_order_id_optional") is True
        and implementation.get("canonical_target_client_order_id_from_authenticated_registry")
        is True
        and implementation.get("cancel_collision_rejected_before_dispatch") is True
        and implementation.get("target_first_optional_tcid_restart_safe") is True
        and implementation.get("service_level_optional_tcid_tests") == 3,
        "optional-TCID composition evidence weakened",
    )
    outcome_matrix = evidence.get("outcome_matrix")
    require(isinstance(outcome_matrix, list) and len(outcome_matrix) == 8, "outcome matrix missing")
    outcome_by_kind = {row.get("stage6_outcome"): row for row in outcome_matrix}
    require(set(outcome_by_kind) == EXPECTED_SHAPES, "outcome matrix shape drifted")
    golden_by_kind = {
        shape.get("shape"): shape.get("fresh_canonical_bytes_sha256")
        for shape in json.loads(content["golden"]).get("shapes", [])
    }
    for kind, row in outcome_by_kind.items():
        require(row.get("fresh_recovery_byte_identical") is True, f"matrix parity weakened: {kind}")
        require(row.get("golden_sha256") == golden_by_kind.get(kind), f"matrix hash drifted: {kind}")
        require(
            row.get("source_xack")
            in {
                "last_after_authenticated_replacement_persist_reread",
                "only_and_last_effect_after_S_cancel_recovered_reread",
            },
            f"matrix XACK-last weakened: {kind}",
        )
    collision = evidence.get("client_order_id_collision_regression", {})
    require(
        collision.get("legacy_colliding_client_order_id") == "00000000000000000000",
        "legacy collision fixture drifted",
    )
    require(
        collision.get("fixed_place_client_order_id") == "000D608000000G00G000"
        and collision.get("fixed_cancel_client_order_id") == "000D60G000000G00G000"
        and collision.get("fixed_ids_are_distinct") is True,
        "fixed ClientOrderId separation drifted",
    )
    require(
        collision.get("deterministic_across_restart") is True
        and collision.get("finam_safe_ascii_alphanumeric") is True
        and collision.get("max_length") == 20
        and collision.get("global_mapper_truncation_collision_absent") is False
        and collision.get("p1d3_cancel_boundary_rejects_dcid_tcid_collision") is True
        and collision.get("wall_clock_dependency") is False
        and collision.get("binding_includes_original_request_and_order_fingerprint") is True,
        "ClientOrderId contract evidence weakened",
    )
    verification = evidence.get("verification")
    require(isinstance(verification, dict) and len(verification) == 9, "verification inventory drifted")
    require(
        all(isinstance(value, str) and value.startswith("PASS") for value in verification.values()),
        "verification result is not fully passing",
    )
    require(evidence.get("current_tree_authority_rebind_deferred") is True, "authority rebound early")
    require(evidence.get("next_stage_authorized") is False, "next stage opened early")
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 9, "closed surface inventory drifted")
    require(all(value is False for value in closed.values()), "closed surface opened")

    golden = json.loads(content["golden"])
    require(golden.get("schema_version") == 1, "golden schema drifted")
    require(golden.get("shape_count") == 8, "golden shape count drifted")
    require(golden.get("fresh_recovery_byte_identical") is True, "golden parity weakened")
    shapes = golden.get("shapes")
    require(isinstance(shapes, list) and len(shapes) == 8, "golden shapes missing")
    require({shape.get("shape") for shape in shapes} == EXPECTED_SHAPES, "golden shape names drifted")
    for shape in shapes:
        fresh = bytes.fromhex(shape["fresh_canonical_bytes_hex"])
        recovery = bytes.fromhex(shape["recovery_canonical_bytes_hex"])
        require(fresh == recovery, f"fresh/recovery bytes differ: {shape['shape']}")
        digest = hashlib.sha256(fresh).hexdigest()
        require(digest == shape["fresh_canonical_bytes_sha256"], f"fresh hash drift: {shape['shape']}")
        require(digest == shape["recovery_canonical_bytes_sha256"], f"recovery hash drift: {shape['shape']}")
        require(len(shape["fresh_domain_digest_sha256"]) == 64, "fresh domain digest malformed")
        require(
            shape["fresh_domain_digest_sha256"] == shape["recovery_domain_digest_sha256"],
            f"domain digest drift: {shape['shape']}",
        )

    projection_golden = json.loads(content["projection_golden"])
    require(projection_golden.get("schema_version") == 1, "projection golden schema drifted")
    require(
        projection_golden.get("domain") == "moex.stage8b.p1d3.projection-golden.v1",
        "projection golden domain drifted",
    )
    require(projection_golden.get("shape_count") == 8, "projection golden count drifted")
    require(
        projection_golden.get("fresh_and_recovery_each_checked_against_oracle") is True,
        "projection oracle independence weakened",
    )
    projection_shapes = projection_golden.get("shapes")
    require(
        isinstance(projection_shapes, list) and len(projection_shapes) == 8,
        "projection golden shapes missing",
    )
    require(
        {shape.get("shape") for shape in projection_shapes} == EXPECTED_SHAPES,
        "projection golden shape names drifted",
    )
    component_names = ("ack", "orders", "trades", "positions", "truth", "complete_projection")
    for shape in projection_shapes:
        shape_name = shape["shape"]
        require(len(shape.get("pre_book_sha256", "")) == 64, f"pre-book hash malformed: {shape_name}")
        require(len(shape.get("post_book_sha256", "")) == 64, f"post-book hash malformed: {shape_name}")
        require(
            isinstance(shape.get("sequence_allocation_frontier"), int),
            f"sequence frontier missing: {shape_name}",
        )
        require(
            shape.get("fresh_recovery_byte_identical") is True,
            f"projection parity weakened: {shape_name}",
        )
        decoded: dict[str, dict[str, object]] = {}
        for path in ("fresh", "recovery"):
            components = shape.get(path)
            require(isinstance(components, dict), f"projection component set missing: {shape_name}/{path}")
            decoded[path] = {}
            for component_name in component_names:
                component = components.get(component_name)
                require(isinstance(component, dict), f"projection component missing: {shape_name}/{path}/{component_name}")
                raw = bytes.fromhex(component["canonical_bytes_hex"])
                require(
                    hashlib.sha256(raw).hexdigest() == component.get("canonical_bytes_sha256"),
                    f"projection component hash drift: {shape_name}/{path}/{component_name}",
                )
                decoded[path][component_name] = json.loads(raw)
            full = decoded[path]["complete_projection"]
            require(isinstance(full, dict), f"complete projection malformed: {shape_name}/{path}")
            require(full.get("domain") == "moex.stage8b.p1d3.complete-projection.v1", f"complete projection domain drift: {shape_name}/{path}")
            require(full.get("outcome_kind") == shape_name, f"outcome binding drift: {shape_name}/{path}")
            require(full.get("pre_book_sha256") == shape["pre_book_sha256"], f"pre-book binding drift: {shape_name}/{path}")
            require(full.get("post_book_sha256") == shape["post_book_sha256"], f"post-book binding drift: {shape_name}/{path}")
            require(full.get("reserved_seq_ack") == shape.get("reserved_seq_ack"), f"ACK sequence drift: {shape_name}/{path}")
            require(full.get("reserved_seq_truth") == shape.get("reserved_seq_truth"), f"truth sequence drift: {shape_name}/{path}")
            require(full.get("sequence_allocation_frontier") == shape["sequence_allocation_frontier"], f"sequence frontier drift: {shape_name}/{path}")
            for component_name in ("ack", "orders", "trades", "positions", "truth"):
                require(
                    full.get(component_name) == decoded[path][component_name],
                    f"embedded projection mismatch: {shape_name}/{path}/{component_name}",
                )
        for component_name in component_names:
            require(
                decoded["fresh"][component_name] == decoded["recovery"][component_name],
                f"fresh/recovery projection differs: {shape_name}/{component_name}",
            )

    require(
        "R1 was independently accepted" in content["status"]
        and ACCEPTED_DESIGN in content["status"]
        and "The active source R2 correction review" in content["status"],
        "current status drifted",
    )
    require(
        "Two real subprocess/SIGKILL cases bracket" in content["status"]
        and "post-`RequestAccepted` Stage 6" in content["status"],
        "current status omitted the executed restart proof",
    )
    require(
        "P1-d3 R2 source review candidate" in content["roadmap"]
        and "Two subprocess/SIGKILL cases bracket" in content["roadmap"]
        and "P1-d4 exhaustive crash/replay closure remains closed" in content["roadmap"],
        "roadmap drifted",
    )


def main() -> None:
    try:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"source changed path drift: {sorted(actual)}")
        forbidden = subprocess.run(
            ["git", "diff", "--name-only", ACCEPTED_DESIGN, "--", "Cargo.toml", "Cargo.lock", ".github"],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()
        require(not forbidden, f"Cargo/workflow changed: {forbidden}")
        authority = subprocess.run(
            ["git", "diff", "--name-only", ACCEPTED_DESIGN, "--", "docs/stage-8/gov-ci-1-authority.json"],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()
        require(not authority, "current-tree authority rebound before source acceptance")
        validate_content(load_content())
    except (
        CheckFailure,
        OSError,
        ValueError,
        KeyError,
        json.JSONDecodeError,
        subprocess.CalledProcessError,
    ) as error:
        print(f"FAIL stage8b-p1d3-source-scope: {error}", file=sys.stderr)
        raise SystemExit(1)
    print(
        "PASS stage8b-p1d3-source-scope rows=59 negatives=43 golden=8 projection_golden=8 "
        "redis=isolated db0=false finam=false live=false p1d4=false"
    )


if __name__ == "__main__":
    main()
