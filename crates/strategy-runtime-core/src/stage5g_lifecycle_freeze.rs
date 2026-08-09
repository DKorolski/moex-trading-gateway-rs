//! Stage 5G-g aggregate lifecycle evidence.
//!
//! This feature-gated module freezes accepted paper/mock lifecycle witnesses.
//! It owns no trading semantics and has no Redis, FINAM, dispatch, clock or
//! runtime-live attachment. Protective rows are projected from the accepted
//! source-produced Stage 5G-f artifact. Earlier-family rows identify the exact
//! executable witness that the Stage 5G-g gate runs in debug and release.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::stage5g_protective_completion::Stage5gProtectiveGprtArtifactRow;

pub const STAGE5G_G_LIFECYCLE_ARTIFACT_SCHEMA_VERSION: u16 = 1;
pub const STAGE5G_G_ACCEPTED_PREDECESSOR: &str = "12af52d23218c67bc15b7b79835790e40834dfbb";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage5gLifecycleEvidenceKind {
    ExecutableAcceptedWitness,
    SourceProducedLifecycleArtifact,
    SourceProducedRuntimeArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5gLifecycleClosedSurfaces {
    pub finam_transport_attached: bool,
    pub http_post_delete_attached: bool,
    pub redis_live_consumer_attached: bool,
    pub broker_dispatch_attached: bool,
    pub runtime_live_attached: bool,
    pub real_orders_attached: bool,
    pub native_stop_sltp_bracket_attached: bool,
    pub stage6_attached: bool,
}

impl Stage5gLifecycleClosedSurfaces {
    pub(crate) fn closed() -> Self {
        Self {
            finam_transport_attached: false,
            http_post_delete_attached: false,
            redis_live_consumer_attached: false,
            broker_dispatch_attached: false,
            runtime_live_attached: false,
            real_orders_attached: false,
            native_stop_sltp_bracket_attached: false,
            stage6_attached: false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn stage5g_g_source_row(
    scenario_id: &str,
    family: &str,
    owner_stage: &str,
    witnesses: Vec<String>,
    disposition: &str,
    pre_runtime_fingerprint_sha256: Option<String>,
    post_runtime_fingerprint_sha256: Option<String>,
    lifecycle_checkpoint_fingerprint_sha256: String,
    restart_package_fingerprint_sha256: Option<String>,
    callback_count: usize,
    generated_intent_fingerprint_sha256: Option<String>,
    cleanup_fingerprint_sha256: Option<String>,
    final_owner: Option<String>,
    final_cycle_id: Option<String>,
    final_position_qty: Option<String>,
) -> Stage5gLifecycleArtifactRow {
    let mut row = Stage5gLifecycleArtifactRow {
        schema_version: STAGE5G_G_LIFECYCLE_ARTIFACT_SCHEMA_VERSION,
        accepted_predecessor: STAGE5G_G_ACCEPTED_PREDECESSOR.to_string(),
        scenario_id: scenario_id.to_string(),
        family: family.to_string(),
        owner_stage: owner_stage.to_string(),
        evidence_kind: Stage5gLifecycleEvidenceKind::SourceProducedLifecycleArtifact,
        executable_witnesses: witnesses,
        disposition: disposition.to_string(),
        pre_runtime_fingerprint_sha256,
        post_runtime_fingerprint_sha256,
        lifecycle_checkpoint_fingerprint_sha256,
        restart_package_fingerprint_sha256,
        callback_count,
        generated_intent_fingerprint_sha256,
        cleanup_fingerprint_sha256,
        final_owner,
        final_cycle_id,
        final_position_qty,
        closed_surfaces: Stage5gLifecycleClosedSurfaces::closed(),
        canonical_row_fingerprint_sha256: String::new(),
    };
    finalize_row(&mut row);
    row
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage5gLifecycleArtifactRow {
    pub schema_version: u16,
    pub accepted_predecessor: String,
    pub scenario_id: String,
    pub family: String,
    pub owner_stage: String,
    pub evidence_kind: Stage5gLifecycleEvidenceKind,
    pub executable_witnesses: Vec<String>,
    pub disposition: String,
    pub pre_runtime_fingerprint_sha256: Option<String>,
    pub post_runtime_fingerprint_sha256: Option<String>,
    pub lifecycle_checkpoint_fingerprint_sha256: String,
    pub restart_package_fingerprint_sha256: Option<String>,
    pub callback_count: usize,
    pub generated_intent_fingerprint_sha256: Option<String>,
    pub cleanup_fingerprint_sha256: Option<String>,
    pub final_owner: Option<String>,
    pub final_cycle_id: Option<String>,
    pub final_position_qty: Option<String>,
    pub closed_surfaces: Stage5gLifecycleClosedSurfaces,
    pub canonical_row_fingerprint_sha256: String,
}

#[derive(Clone, Copy)]
struct AcceptedWitness {
    scenario_id: &'static str,
    family: &'static str,
    owner_stage: &'static str,
    witness: &'static str,
    disposition: &'static str,
    callback_count: usize,
}

#[allow(dead_code)]
const ACK: [AcceptedWitness; 10] = [
    witness(
        "GACK01_PLACE_ACCEPTED_EXACT_IDS",
        "ACK",
        "5G-b",
        "gack01_place_accepted_exact_ids_resolves_without_broker_truth",
        "resolved",
        1,
    ),
    witness(
        "GACK02_SUBMITTED_MISSING_BROKER_ID_KEEPS_PENDING",
        "ACK",
        "5G-b",
        "gack02_and_gack03_missing_broker_id_waits_then_recovered_resolves",
        "awaiting_broker_order_id",
        0,
    ),
    witness(
        "GACK03_RECOVERED_EXACT_BROKER_ID",
        "ACK",
        "5G-b",
        "gack02_and_gack03_missing_broker_id_waits_then_recovered_resolves",
        "resolved_after_reconciliation",
        1,
    ),
    witness(
        "GACK04_REJECTED_EXACT_REQUEST_CLEARS_PENDING",
        "ACK",
        "5G-b",
        "gack04_rejected_exact_request_clears_pending",
        "resolved_rejected",
        1,
    ),
    witness(
        "GACK05_TIMEOUT_KEEPS_PENDING",
        "ACK",
        "5G-b",
        "gack05_and_gack06_ambiguous_statuses_keep_pending",
        "reconciliation_pending",
        0,
    ),
    witness(
        "GACK06_UNKNOWN_PENDING_KEEPS_PENDING",
        "ACK",
        "5G-b",
        "gack05_and_gack06_ambiguous_statuses_keep_pending",
        "reconciliation_pending",
        0,
    ),
    witness(
        "GACK07_DUPLICATE_REQUIRES_PRIOR_OUTCOME",
        "ACK",
        "5G-b",
        "gack07_duplicate_requires_prior_outcome_and_exact_duplicate_is_noop",
        "prior_outcome_required_or_idempotent",
        1,
    ),
    witness(
        "GACK08_EXPIRED_REQUIRES_EXACT_NO_SEND_PROOF",
        "ACK",
        "5G-b",
        "gack08_expired_requires_exact_no_send_proof",
        "resolved_after_no_send_proof",
        1,
    ),
    witness(
        "GACK09_REQUEST_OR_CLIENT_ID_MISMATCH_BLOCKS",
        "ACK",
        "5G-b",
        "gack09_wrong_request_and_client_ids_block_atomically",
        "blocked_identity_mismatch",
        0,
    ),
    witness(
        "GACK10_BROKER_ORDER_ID_CONFLICT_BLOCKS",
        "ACK",
        "5G-b",
        "gack10_conflicting_broker_order_id_blocks",
        "blocked_broker_order_id_conflict",
        0,
    ),
];

#[allow(dead_code)]
const ORDER_POSITION: [AcceptedWitness; 16] = [
    witness(
        "GOP01_WORKING_ORDER_REMAINS_ACTIVE",
        "ORDER_POSITION",
        "5G-c",
        "gop01_working_order_remains_active",
        "awaiting_working",
        0,
    ),
    witness(
        "GOP02_PARTIAL_FILL_ADVANCES_MONOTONICALLY",
        "ORDER_POSITION",
        "5G-c",
        "gop02_partial_fill_advances_monotonically",
        "awaiting_partial_fill",
        0,
    ),
    witness(
        "GOP03_PARTIAL_FILL_REGRESSION_BLOCKS",
        "ORDER_POSITION",
        "5G-c",
        "gop03_partial_fill_regression_blocks",
        "blocked_fill_regression",
        0,
    ),
    witness(
        "GOP04_FILLED_REQUIRES_TARGET_POSITION_CONFIRMATION",
        "ORDER_POSITION",
        "5G-c",
        "gop04_filled_requires_target_position_confirmation",
        "blocked_position_confirmation",
        0,
    ),
    witness(
        "GOP05_CANCELED_TERMINATES_WITHOUT_POSITION_CHANGE",
        "ORDER_POSITION",
        "5G-c",
        "gop05_canceled_terminates_without_position_change",
        "terminal_canceled",
        1,
    ),
    witness(
        "GOP06_REJECTED_TERMINATES_WITHOUT_POSITION_CHANGE",
        "ORDER_POSITION",
        "5G-c",
        "gop06_rejected_terminates_without_position_change",
        "terminal_rejected",
        1,
    ),
    witness(
        "GOP07_EXPIRED_TERMINATES_WITHOUT_POSITION_CHANGE",
        "ORDER_POSITION",
        "5G-c",
        "gop07_expired_terminates_without_position_change",
        "terminal_expired",
        1,
    ),
    witness(
        "GOP08_UNKNOWN_ORDER_STATUS_BLOCKS",
        "ORDER_POSITION",
        "5G-c",
        "gop08_unknown_order_status_blocks",
        "blocked_unknown_status",
        0,
    ),
    witness(
        "GOP09_IDENTICAL_EVENT_REPLAY_IS_IDEMPOTENT",
        "ORDER_POSITION",
        "5G-c",
        "gop09_identical_event_replay_is_idempotent",
        "exact_replay_noop",
        0,
    ),
    witness(
        "GOP10_CONFLICTING_DUPLICATE_EVENT_BLOCKS",
        "ORDER_POSITION",
        "5G-c",
        "gop10_conflicting_duplicate_event_is_detectable",
        "blocked_conflicting_duplicate",
        0,
    ),
    witness(
        "GOP11_NON_TARGET_EVENT_CANNOT_SETTLE_TARGET",
        "ORDER_POSITION",
        "5G-c",
        "gop11_non_target_event_cannot_settle_target",
        "blocked_missing_target_order",
        0,
    ),
    witness(
        "GOP12_ACCOUNT_WIDE_ACTIVE_ORDER_IS_SAFETY_GUARD",
        "ORDER_POSITION",
        "5G-c",
        "gop12_account_wide_active_order_is_safety_guard",
        "blocked_account_safety_guard",
        0,
    ),
    witness(
        "GOP13_TARGET_POSITION_SIDE_MISMATCH_BLOCKS",
        "ORDER_POSITION",
        "5G-c",
        "gop13_target_position_side_mismatch_blocks",
        "blocked_position_side",
        0,
    ),
    witness(
        "GOP14_TARGET_POSITION_OVERFILL_BLOCKS",
        "ORDER_POSITION",
        "5G-c",
        "gop14_target_position_overfill_blocks",
        "blocked_position_overfill",
        0,
    ),
    witness(
        "GOP15_CORRELATED_TRADE_SUPPORTS_FILL_TRUTH",
        "ORDER_POSITION",
        "5G-c",
        "gop15_correlated_trade_supports_fill_truth",
        "correlated_trade_accepted",
        0,
    ),
    witness(
        "GOP16_TRADE_IDENTITY_OR_QUANTITY_MISMATCH_BLOCKS",
        "ORDER_POSITION",
        "5G-c",
        "gop16_trade_identity_or_quantity_mismatch_blocks",
        "blocked_trade_mismatch",
        0,
    ),
];

const TIMER: [AcceptedWitness; 8] = [
    witness(
        "GTMR01_MONOTONIC_ZERO_INTENT_TIMER_CONTINUES",
        "TIMER",
        "5G-d",
        "stage5gd_public_convergence_timer_is_linear_and_monotonic",
        "timer_ready",
        1,
    ),
    witness(
        "GTMR02_EQUAL_OR_REVERSED_TIMER_BLOCKS",
        "TIMER",
        "5G-d",
        "stage5gd_reversed_initial_timer_preserves_exact_checkpoint",
        "blocked_non_monotonic_timer",
        0,
    ),
    witness(
        "GTMR03_TIMER_INTENT_REENTERS_ACK_LIFECYCLE",
        "TIMER",
        "5G-d",
        "stage5gd_timer_generated_cleanup_roundtrips_through_ack_truth_and_next_session",
        "generated_intent_settled",
        2,
    ),
    witness(
        "GTMR04_TIMER_CLEANUP_PRESERVES_ATTRIBUTION",
        "TIMER",
        "5G-d",
        "stage5ck_partial_entry_cleanup_uses_pending_entry_attribution",
        "cleanup_attribution_preserved",
        1,
    ),
    witness(
        "GTMR05_CHECKPOINT_IS_SINGLE_CONSUME",
        "TIMER",
        "5G-d",
        "stage5cm_ready_checkpoint_can_continue_to_timer_or_bar_once",
        "single_consume",
        1,
    ),
    witness(
        "GTMR06_BAR_TIMER_RACE_HAS_ONE_DETERMINISTIC_WINNER",
        "TIMER",
        "5G-d",
        "stage5gd_bar_preflight_failure_returns_exact_incoming_checkpoint",
        "deterministic_single_winner",
        1,
    ),
    witness(
        "GTMR07_GENERATED_BATCH_BLOCKS_UNRELATED_CONTINUATION",
        "TIMER",
        "5G-d",
        "stage5cm_generated_timer_batch_blocks_continuation_until_lifecycle",
        "generated_batch_escrow",
        1,
    ),
    witness(
        "GTMR08_NO_AUTONOMOUS_LOOP_OR_CLOCK_READ",
        "TIMER",
        "5G-d",
        "scripts/stage5g_d_check.py",
        "static_surface_closed",
        0,
    ),
];

const RESTART: [AcceptedWitness; 12] = [
    witness(
        "GRST01_RESTART_BEFORE_ACK",
        "RESTART",
        "5G-e",
        "stage5g_edc_grst01_before_ack_is_blocked_without_mutation",
        "await_fresh_broker_truth",
        0,
    ),
    witness(
        "GRST02_RESTART_AFTER_ACK_BEFORE_ORDER",
        "RESTART",
        "5G-e",
        "stage5g_edc_grst02_after_ack_candidate_is_applied",
        "apply_owned_candidate",
        0,
    ),
    witness(
        "GRST03_RESTART_WITH_WORKING_ORDER",
        "RESTART",
        "5G-e",
        "stage5g_edc_grst03_working_candidate_is_applied",
        "working_candidate_applied",
        0,
    ),
    witness(
        "GRST04_RESTART_AFTER_PARTIAL_FILL",
        "RESTART",
        "5G-e",
        "stage5g_edc_grst04_partial_fill_candidate_is_applied",
        "partial_candidate_applied",
        0,
    ),
    witness(
        "GRST05_RESTART_FILLED_BEFORE_POSITION",
        "RESTART",
        "5G-e",
        "stage5g_edc_grst05_filled_before_position_awaits_truth",
        "await_position_truth",
        0,
    ),
    witness(
        "GRST06_RESTART_AFTER_TERMINAL_POSITION_APPLIED",
        "RESTART",
        "5G-e",
        "stage5g_edc_grst06_terminal_checkpoint_continues",
        "continue_committed_checkpoint",
        0,
    ),
    witness(
        "GRST07_RESTART_AT_TIMER_CHECKPOINT",
        "RESTART",
        "5G-e",
        "stage5g_edc_grst07_timer_checkpoint_continues_without_mutation",
        "continue_timer_checkpoint",
        0,
    ),
    witness(
        "GRST08_RESTART_WITH_GENERATED_INTENT_ESCROW",
        "RESTART",
        "5G-e",
        "stage5g_edc_grst08_generated_intent_escrow_is_blocked",
        "retain_generated_intent_escrow",
        0,
    ),
    witness(
        "GRST09_EXACT_REPLAY_IS_IDEMPOTENT",
        "RESTART",
        "5G-e",
        "stage5g_edc_grst09_policy_b_replay_is_disabled_without_mutation",
        "exact_replay_noop",
        0,
    ),
    witness(
        "GRST10_CONFLICTING_REPLAY_BLOCKS",
        "RESTART",
        "5G-e",
        "stage5g_edc_grst10_conflict_requires_manual_intervention",
        "blocked_conflicting_replay",
        0,
    ),
    witness(
        "GRST11_FRESH_BROKER_TRUTH_OVERRIDES_STALE_HINT",
        "RESTART",
        "5G-e",
        "stage5g_edc_grst11_fresh_terminal_candidate_is_applied",
        "fresh_truth_applied",
        0,
    ),
    witness(
        "GRST12_MISSING_OR_AMBIGUOUS_TRUTH_REQUIRES_RECONCILIATION",
        "RESTART",
        "5G-e",
        "stage5g_edc_grst12_missing_truth_is_blocked_without_mutation",
        "reconciliation_required",
        0,
    ),
];

const fn witness(
    scenario_id: &'static str,
    family: &'static str,
    owner_stage: &'static str,
    witness: &'static str,
    disposition: &'static str,
    callback_count: usize,
) -> AcceptedWitness {
    AcceptedWitness {
        scenario_id,
        family,
        owner_stage,
        witness,
        disposition,
        callback_count,
    }
}

pub fn stage5g_g_lifecycle_artifact_rows() -> Vec<Stage5gLifecycleArtifactRow> {
    let mut rows = crate::stage5g_mock_ack::tests::stage5g_g_ack_artifact_rows();
    rows.extend(crate::stage5g_order_position::tests::stage5g_g_order_position_artifact_rows());
    rows.extend(TIMER.into_iter().chain(RESTART).map(accepted_witness_row));
    rows.extend(
        crate::stage5g_f_gprt_artifact_rows_parallel_verified()
            .into_iter()
            .map(protective_row),
    );
    assert_eq!(rows.len(), 54, "Stage 5G-g matrix must contain 54 rows");
    rows
}

pub fn stage5g_g_lifecycle_artifact_rows_parallel_verified() -> Vec<Stage5gLifecycleArtifactRow> {
    let sequential = stage5g_g_lifecycle_artifact_rows();
    let ack = std::thread::spawn(crate::stage5g_mock_ack::tests::stage5g_g_ack_artifact_rows);
    let order_position = std::thread::spawn(|| {
        crate::stage5g_order_position::tests::stage5g_g_order_position_artifact_rows()
    });
    let protective = std::thread::spawn(|| {
        crate::stage5g_f_gprt_artifact_rows_parallel_verified()
            .into_iter()
            .map(protective_row)
            .collect::<Vec<_>>()
    });

    let mut parallel = ack.join().expect("Stage 5G-h ACK source worker joins");
    parallel.extend(
        order_position
            .join()
            .expect("Stage 5G-h order/position source worker joins"),
    );
    parallel.extend(TIMER.into_iter().chain(RESTART).map(accepted_witness_row));
    parallel.extend(
        protective
            .join()
            .expect("Stage 5G-h protective source worker joins"),
    );
    assert_eq!(
        serde_json::to_vec(&sequential).expect("sequential artifact serializes"),
        serde_json::to_vec(&parallel).expect("parallel artifact serializes"),
        "Stage 5G-h true-parallel source production must preserve accepted bytes"
    );
    parallel
}

pub fn stage5g_g_lifecycle_artifact_json_pretty() -> String {
    serde_json::to_string_pretty(&stage5g_g_lifecycle_artifact_rows_parallel_verified())
        .expect("Stage 5G-g artifact serializes")
}

pub fn stage5g_h_sequential_lifecycle_artifact_json_pretty() -> String {
    serde_json::to_string_pretty(&stage5g_g_lifecycle_artifact_rows())
        .expect("Stage 5G-h sequential artifact serializes")
}

fn accepted_witness_row(witness: AcceptedWitness) -> Stage5gLifecycleArtifactRow {
    let lifecycle = semantic_sha256(&(
        "stage5g-g-accepted-executable-witness-v1",
        STAGE5G_G_ACCEPTED_PREDECESSOR,
        witness.scenario_id,
        witness.witness,
        witness.disposition,
    ));
    let mut row = Stage5gLifecycleArtifactRow {
        schema_version: STAGE5G_G_LIFECYCLE_ARTIFACT_SCHEMA_VERSION,
        accepted_predecessor: STAGE5G_G_ACCEPTED_PREDECESSOR.to_string(),
        scenario_id: witness.scenario_id.to_string(),
        family: witness.family.to_string(),
        owner_stage: witness.owner_stage.to_string(),
        evidence_kind: Stage5gLifecycleEvidenceKind::ExecutableAcceptedWitness,
        executable_witnesses: vec![witness.witness.to_string()],
        disposition: witness.disposition.to_string(),
        pre_runtime_fingerprint_sha256: None,
        post_runtime_fingerprint_sha256: None,
        lifecycle_checkpoint_fingerprint_sha256: lifecycle,
        restart_package_fingerprint_sha256: None,
        callback_count: witness.callback_count,
        generated_intent_fingerprint_sha256: None,
        cleanup_fingerprint_sha256: None,
        final_owner: None,
        final_cycle_id: None,
        final_position_qty: None,
        closed_surfaces: Stage5gLifecycleClosedSurfaces::closed(),
        canonical_row_fingerprint_sha256: String::new(),
    };
    finalize_row(&mut row);
    row
}

fn protective_row(source: Stage5gProtectiveGprtArtifactRow) -> Stage5gLifecycleArtifactRow {
    let post_runtime = source
        .phase_b_final_runtime_fingerprint_sha256
        .clone()
        .or(source.phase_a_runtime_semantic_fingerprint_sha256.clone());
    let restart = source
        .phase_b_completed_canonical_restart_package_fingerprint_sha256
        .clone()
        .or(source
            .phase_a_canonical_restart_package_fingerprint_sha256
            .clone())
        .or(Some(
            source
                .pre_authenticated_restart_package_fingerprint_sha256
                .clone(),
        ));
    let cleanup = source
        .phase_b_final_cleanup_ledger_fingerprint_sha256
        .clone()
        .or(source.phase_a_cleanup_ledger_fingerprint_sha256.clone());
    let disposition = source
        .phase_b_disposition
        .clone()
        .unwrap_or_else(|| source.phase_a_disposition.clone());
    let lifecycle = source.canonical_semantic_fingerprint_sha256.clone();
    let mut row = Stage5gLifecycleArtifactRow {
        schema_version: STAGE5G_G_LIFECYCLE_ARTIFACT_SCHEMA_VERSION,
        accepted_predecessor: STAGE5G_G_ACCEPTED_PREDECESSOR.to_string(),
        scenario_id: source.scenario.clone(),
        family: "PROTECTIVE".to_string(),
        owner_stage: "5G-f".to_string(),
        evidence_kind: Stage5gLifecycleEvidenceKind::SourceProducedRuntimeArtifact,
        executable_witnesses: vec!["stage5g_f_gprt_artifact_rows".to_string()],
        disposition,
        pre_runtime_fingerprint_sha256: Some(
            source.pre_runtime_semantic_fingerprint_sha256.clone(),
        ),
        post_runtime_fingerprint_sha256: post_runtime,
        lifecycle_checkpoint_fingerprint_sha256: lifecycle,
        restart_package_fingerprint_sha256: restart,
        callback_count: source.callback_count,
        generated_intent_fingerprint_sha256: source
            .phase_a_cleanup_batch_fingerprint_sha256
            .clone(),
        cleanup_fingerprint_sha256: cleanup,
        final_owner: source.phase_b_final_owner.map(|owner| format!("{owner:?}")),
        final_cycle_id: source.phase_b_final_cycle_id.clone(),
        final_position_qty: source.phase_b_final_position_qty.map(|qty| qty.to_string()),
        closed_surfaces: Stage5gLifecycleClosedSurfaces::closed(),
        canonical_row_fingerprint_sha256: String::new(),
    };
    finalize_row(&mut row);
    row
}

fn finalize_row(row: &mut Stage5gLifecycleArtifactRow) {
    row.canonical_row_fingerprint_sha256 = String::new();
    row.canonical_row_fingerprint_sha256 = semantic_sha256(&("stage5g-g-lifecycle-row-v1", &row));
}

fn semantic_sha256<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("Stage 5G-g evidence serializes");
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn stage5g_g_matrix_is_exact_complete_and_ordered() {
        let rows = stage5g_g_lifecycle_artifact_rows();
        assert_eq!(rows.len(), 54);
        assert_eq!(rows[0].scenario_id, "GACK01_PLACE_ACCEPTED_EXACT_IDS");
        assert_eq!(
            rows[53].scenario_id,
            "GPRT08_NON_EXECUTION_TERMINAL_CANNOT_INVENT_EXIT"
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.scenario_id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            54
        );
    }

    #[test]
    fn stage5g_g_protective_rows_reuse_source_produced_runtime_artifact() {
        let rows = stage5g_g_lifecycle_artifact_rows();
        for row in rows.iter().filter(|row| row.family == "PROTECTIVE") {
            assert_eq!(
                row.evidence_kind,
                Stage5gLifecycleEvidenceKind::SourceProducedRuntimeArtifact
            );
            assert!(row.pre_runtime_fingerprint_sha256.is_some());
            assert!(row.post_runtime_fingerprint_sha256.is_some());
            assert!(row.restart_package_fingerprint_sha256.is_some());
        }
    }

    #[test]
    fn stage5g_g_parallel_artifact_is_byte_identical() {
        assert_eq!(
            serde_json::to_vec(&stage5g_g_lifecycle_artifact_rows()).unwrap(),
            serde_json::to_vec(&stage5g_g_lifecycle_artifact_rows_parallel_verified()).unwrap()
        );
    }

    #[test]
    fn stage5g_g_closed_surfaces_are_false_for_every_row() {
        for row in stage5g_g_lifecycle_artifact_rows() {
            assert_eq!(
                row.closed_surfaces,
                Stage5gLifecycleClosedSurfaces::closed()
            );
        }
    }
}
