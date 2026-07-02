#!/usr/bin/env bash
set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace_root"

target="crates/finam-gateway/src/real_order_endpoint.rs"

if [[ ! -f "$target" ]]; then
  echo "order-endpoint-scanner-transition-spec: missing $target" >&2
  exit 1
fi

failures=0

report_failure() {
  echo "order-endpoint-scanner-transition-spec: $*" >&2
  failures=$((failures + 1))
}

if rg -n '\.post\(|\.delete\(|\.request\(|\.send\(|Method::POST|Method::DELETE|reqwest|HttpClient|\bTransport\b|\bAdapter\b|\bBackend\b' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "design-only API shape must not contain HTTP send surfaces"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'pub struct GatewayRealOrderEndpointInternalRouteShape' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "internal route shape must not be public"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'pub fn consume_approved_request_parts_for_future_endpoint' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "approved request-parts consumer must not be public"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'pub fn classify_future_send_attempt_result' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "future send result classifier must not be public"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'pub fn classify_accepted_result_after_future_send' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "accepted-result classifier must not be public"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'pub fn create_(place|cancel)_checkpoint_marker_after_sqlite_transition' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "durable checkpoint marker constructors must not be public"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'pub fn captured_response_error_envelope_diagnostic' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "captured response/error envelope diagnostic constructor must not be public"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'pub fn bind_(place|cancel)_endpoint_attempt_journal' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "endpoint attempt journal binding functions must not be public"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'pub fn append_(place|cancel)_durable_endpoint_attempt_journal' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "durable endpoint attempt journal append functions must not be public"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'consume_approved_request_parts_for_future_endpoint\([^)]*GatewayRealOrderEndpoint.*Diagnostic' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "diagnostic DTOs must not feed the approved request-parts consumer"
fi
rm -f /tmp/moex_transition_forbidden.$$

if rg -n 'classify_future_send_attempt_result\([^)]*GatewayRealOrderEndpoint.*Diagnostic' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "diagnostic DTOs must not feed the future send result classifier"
fi
rm -f /tmp/moex_transition_forbidden.$$

for internal_type in \
  GatewayRealOrderEndpointInternalRouteShape \
  RenderedOrderEndpointPath \
  ApprovedOrderEndpointRequestParts \
  GatewayRealOrderEndpointAcceptedResponseShape \
  GatewayRealOrderEndpointRequestSnapshotFingerprint \
  GatewayRealOrderEndpointAttemptFingerprint \
  GatewayRealOrderEndpointSqliteTransitionCommitProof \
  GatewayRealOrderEndpointCapturedEnvelopeRecord \
  GatewayRealOrderEndpointAttemptJournalBinding \
  GatewayRealOrderEndpointCheckpointProofFingerprint \
  GatewayRealOrderEndpointCapturedEnvelopeFingerprint \
  GatewayRealOrderEndpointOutcomeClassifierFingerprint \
  GatewayRealOrderEndpointStateTransitionResultRecord \
  GatewayRealOrderEndpointAckDiagnosticFingerprint \
  GatewayRealOrderEndpointDurableAttemptJournalAppendInput \
  GatewayRealOrderEndpointDurableAttemptJournalRecord \
  PlaceEndpointDurableCheckpointApproved \
  CancelEndpointDurableCheckpointApproved
do
  if rg -n "pub struct ${internal_type}" "$target" >/tmp/moex_transition_forbidden.$$; then
    cat /tmp/moex_transition_forbidden.$$ >&2
    report_failure "${internal_type} must not be public"
  fi
  rm -f /tmp/moex_transition_forbidden.$$

  if rg -n "impl std::fmt::Debug for ${internal_type}" "$target" >/tmp/moex_transition_forbidden.$$; then
    cat /tmp/moex_transition_forbidden.$$ >&2
    report_failure "${internal_type} must not implement Debug"
  fi
  rm -f /tmp/moex_transition_forbidden.$$

  if rg -nU "#\\[derive\\([^\\]]*(Debug|Serialize|Deserialize)[^\\]]*\\)\\]\\nstruct ${internal_type}" "$target" >/tmp/moex_transition_forbidden.$$; then
    cat /tmp/moex_transition_forbidden.$$ >&2
    report_failure "${internal_type} must not be Debug/Serialize/Deserialize"
  fi
  rm -f /tmp/moex_transition_forbidden.$$
done

if rg -nU '#\[derive\([^\]]*Serialize[^\]]*\)\]\nstruct GatewayRealOrderEndpointInternalRouteShape' "$target" >/tmp/moex_transition_forbidden.$$; then
  cat /tmp/moex_transition_forbidden.$$ >&2
  report_failure "internal route shape must not be serializable"
fi
rm -f /tmp/moex_transition_forbidden.$$

required_patterns=(
  "EndpointGateApproved"
  "FinamPlaceOrderRequestSpec"
  "FinamCancelOrderRequestSpec"
  "DesignOnlyNoHttpSend"
  "api_shape_contains_route_templates: false"
  "struct GatewayRealOrderEndpointInternalRouteShape"
  "struct RenderedOrderEndpointPath"
  "struct ApprovedOrderEndpointRequestParts"
  "fn consume_approved_request_parts_for_future_endpoint"
  "diagnostic_can_construct_request_parts: false"
  "constructors_require_endpoint_gate: true"
  "constructors_require_approved_request_spec: true"
  "constructors_require_account_instrument_allowlist: true"
  "constructors_require_operator_arm: true"
  "constructors_require_durable_state_checkpoint: true"
  "constructors_require_operation_specific_checkpoint: true"
  "consumer_internal_only: true"
  "consumer_requires_endpoint_gate: true"
  "consumer_accepts_approved_request_parts_only: true"
  "consumer_accepts_diagnostics: false"
  "consumer_network_enabled: false"
  "GatewayRealOrderEndpointFutureSendOutcome"
  "Accepted"
  "Rejected"
  "TimeoutUnknownPending"
  "RateLimited"
  "Maintenance"
  "Unauthorized"
  "DecodeError"
  "TransportError"
  "fn classify_future_send_attempt_result"
  "future_send_requires_endpoint_gate: true"
  "future_send_accepts_approved_request_parts_only: true"
  "future_send_accepts_diagnostics: false"
  "future_send_consumes_request_parts: true"
  "future_send_network_enabled: false"
  "operation_specific_durable_checkpoint_required: true"
  "retry_after_timeout_unknown_allowed: false"
  "request_parts_reuse_after_outcome_allowed: false"
  "result_diagnostic_can_bypass_state_machine: false"
  "state_machine_transition_required: true"
  "PlaceBeginSubmitPersistedBeforeEndpoint"
  "CancelRequestCancelPersistedBeforeEndpoint"
  "route_template_exported: false"
  "rendered_path_exported: false"
  "raw_body_exported: false"
  "GatewayRealOrderEndpointRedactedRouteDiagnostic"
  "GatewayRealOrderEndpointApprovedPartsDiagnostic"
  "GatewayRealOrderEndpointConsumerDiagnostic"
  "GatewayRealOrderEndpointFutureSendDiagnostic"
  "GatewayRealOrderEndpointTransportCategory"
  "GatewayRealOrderEndpointTransportStateSemantics"
  "GatewayRealOrderEndpointTransportCategoryPolicyEntry"
  "GatewayRealOrderEndpointTransportCategoryPolicyDesignShape"
  "transport_category_policy_matrix"
  "DnsOrConnectError"
  "TlsError"
  "HttpSendError"
  "BodyReadError"
  "Timeout"
  "NonTimeoutTransportFailure"
  "timeout_separated_from_non_timeout_transport: true"
  "non_timeout_transport_does_not_use_timeout_ack_reason: true"
  "non_timeout_transport_does_not_enter_timeout_unknown_state: true"
  "timeout_uses_unknown_pending_semantics: true"
  "GatewayRealOrderEndpointAcceptedResultKind"
  "GatewayRealOrderEndpointAcceptedResponseShape"
  "GatewayRealOrderEndpointAcceptedResultPolicyEntry"
  "GatewayRealOrderEndpointAcceptedResultClassifierDesignShape"
  "GatewayRealOrderEndpointAcceptedResultDiagnostic"
  "accepted_result_classifier_policy_matrix"
  "classify_accepted_result_after_future_send"
  "accepted_broker_id_policy_wired: true"
  "unconditional_submitted_allowed: false"
  "GatewayRealOrderEndpointCancelAcceptedIdPolicy"
  "GatewayRealOrderEndpointCancelAcceptedIdPolicyEntry"
  "GatewayRealOrderEndpointCancelAcceptedIdPolicyDesignShape"
  "cancel_accepted_id_policy_matrix"
  "MissingBrokerOrderIdAcceptedPendingReconciliation"
  "BrokerOrderIdMismatchManualIntervention"
  "response_body_optional_documented: true"
  "missing_id_requires_reconciliation: true"
  "mismatched_id_manual_conflict: true"
  "GatewayRealOrderEndpointCapturedEnvelopeKind"
  "GatewayRealOrderEndpointCapturedEnvelopeTransportCategoryEntry"
  "GatewayRealOrderEndpointCapturedResponseEnvelopeDiagnostic"
  "GatewayRealOrderEndpointCapturedEnvelopeDesignShape"
  "captured_response_error_envelope_diagnostic"
  "captured_envelope_transport_category_matrix"
  "envelope_diagnostic_redacted_only: true"
  "envelope_accepts_raw_path_body_or_error: false"
  "raw_path_exported: false"
  "raw_error_exported: false"
  "status_len_hash_presence_only: true"
  "transport_category_mapped: true"
  "diagnostic_can_feed_transport: false"
  "GatewayRealOrderEndpointAttemptFingerprint"
  "GatewayRealOrderEndpointCapturedEnvelopeRecord"
  "GatewayRealOrderEndpointAttemptJournalBinding"
  "GatewayRealOrderEndpointAttemptJournalDesignShape"
  "GatewayRealOrderEndpointAttemptDiagnostic"
  "bind_place_endpoint_attempt_journal"
  "bind_cancel_endpoint_attempt_journal"
  "endpoint_attempt_redacted_diagnostic"
  "journal_internal_only: true"
  "endpoint_attempt_id_hash_len: 64"
  "binds_approved_request_parts: true"
  "binds_request_snapshot_fingerprint: true"
  "binds_checkpoint_marker: true"
  "binds_captured_envelope: true"
  "binds_outcome_classifier: true"
  "diagnostic_redacted_only: true"
  "GatewayRealOrderEndpointCheckpointProofFingerprint"
  "GatewayRealOrderEndpointCapturedEnvelopeFingerprint"
  "GatewayRealOrderEndpointOutcomeClassifierFingerprint"
  "GatewayRealOrderEndpointStateTransitionResultRecord"
  "GatewayRealOrderEndpointAckDiagnosticFingerprint"
  "GatewayRealOrderEndpointDurableAttemptJournalAppendInput"
  "GatewayRealOrderEndpointDurableAttemptJournalRecord"
  "GatewayRealOrderEndpointDurableAttemptJournalContractDesignShape"
  "GatewayRealOrderEndpointDurableAttemptJournalDiagnostic"
  "append_place_durable_endpoint_attempt_journal"
  "append_cancel_durable_endpoint_attempt_journal"
  "durable_endpoint_attempt_journal_redacted_diagnostic"
  "durable_journal_schema_design_only: true"
  "journal_record_internal_only: true"
  "append_requires_endpoint_gate: true"
  "append_requires_approved_request_parts: true"
  "append_requires_operation_specific_checkpoint_marker: true"
  "binds_checkpoint_proof_fingerprint: true"
  "binds_captured_envelope_fingerprint: true"
  "binds_outcome_fingerprint: true"
  "binds_state_transition_result_fingerprint: true"
  "binds_ack_diagnostic_fingerprint: true"
  "append_committed_after_state_transition: true"
  "exact_once_attempt_id_unique_required: true"
  "replay_requires_same_fingerprint_set: true"
  "raw_endpoint_attempt_id_exported: false"
  "raw_request_values_exported: false"
  "GatewayRealOrderEndpointHttpBodyShape"
  "GatewayRealOrderEndpointHttpStatusOutcomeEntry"
  "GatewayRealOrderEndpointHttpStatusOutcomeMatrixDesignShape"
  "place_http_status_outcome_matrix"
  "cancel_http_status_outcome_matrix"
  "http_status_outcome_matrix"
  "AcceptedWithBrokerOrderId"
  "AcceptedWithoutBrokerOrderId"
  "AcceptedEmptyBrokerOrderId"
  "AcceptedBrokerOrderIdMismatch"
  "BrokerReject"
  "MalformedBody"
  "covers_2xx_accepted_body_variants: true"
  "covers_400_422_broker_reject: true"
  "covers_401_403_unauthorized: true"
  "covers_408_504_timeout: true"
  "covers_429_rate_limit: true"
  "covers_500_502_503_maintenance: true"
  "covers_malformed_body_decode_error: true"
  "covers_transport_category_failures: true"
  "place_cancel_specific_mapping: true"
  "GatewayRealOrderEndpointFinamStatusBodyPolicy"
  "GatewayRealOrderEndpointFinamStatusSemanticsEntry"
  "GatewayRealOrderEndpointFinamStatusSemanticsDesignShape"
  "documented_place_finam_rest_statuses"
  "documented_cancel_finam_rest_statuses"
  "place_finam_status_semantics_matrix"
  "cancel_finam_status_semantics_matrix"
  "finam_status_semantics_matrix"
  "PlaceSuccessBodyRequiredForSubmitted"
  "CancelSuccessBodyOptional"
  "Undocumented2xxRequiresEvidenceOrWaiver"
  "NotFoundRequiresReadOnlyReconciliation"
  "official_rest_docs_checked: true"
  "documented_success_status_only_200: true"
  "undocumented_success_201_202_204_require_evidence_or_waiver: true"
  "place_success_body_required_for_immediate_submitted: true"
  "place_empty_body_requires_reconciliation: true"
  "cancel_success_body_optional: true"
  "cancel_missing_id_requires_reconciliation: true"
  "cancel_404_documented_and_requires_reconciliation: true"
  "cancel_409_410_documented_by_finam_rest_docs: false"
  "cancel_409_410_policy_or_waiver_required: true"
  "defensive_422_502_not_documented_as_finam_order_status: true"
  "status_semantics_can_bypass_state_machine: false"
  "GatewayRealOrderEndpointEvidenceSlot"
  "GatewayRealOrderEndpointEvidenceClosureMethod"
  "GatewayRealOrderEndpointEvidenceClosureStatus"
  "GatewayRealOrderEndpointEvidenceSlotClosurePlanEntry"
  "GatewayRealOrderEndpointEvidenceClosurePlanDesignShape"
  "implementation_gate_evidence_closure_plan"
  "ReleaseProfileEvidenceOrWaiver"
  "PositiveGetOrderEvidenceOrWaiver"
  "RouteTemplateRecheck"
  "Undocumented2xxStatusSemantics"
  "Cancel409410StatusSemantics"
  "PendingBeforeImplementationGate"
  "ControlledEvidence"
  "ReviewerAcceptedWaiver"
  "OfficialDocsConfirmation"
  "plan_design_only: true"
  "release_profile_slot_present: true"
  "positive_get_order_slot_present: true"
  "route_template_recheck_slot_present: true"
  "undocumented_2xx_policy_slot_present: true"
  "cancel_409_410_policy_slot_present: true"
  "all_slots_pending_before_implementation_gate: true"
  "all_slots_require_reviewer_acceptance: true"
  "order_endpoint_calls_allowed_for_closure: false"
  "closure_artifacts_redacted_only: true"
  "source_archive_binding_required: true"
  "GatewayRealOrderEndpointDurableAttemptJournalColumnKind"
  "GatewayRealOrderEndpointDurableAttemptJournalColumn"
  "GatewayRealOrderEndpointDurableAttemptJournalIndex"
  "GatewayRealOrderEndpointDurableAttemptJournalReplayDecision"
  "GatewayRealOrderEndpointDurableAttemptJournalReplayPolicyEntry"
  "GatewayRealOrderEndpointDurableAttemptJournalSqliteSchemaDesignShape"
  "durable_attempt_journal_sqlite_columns"
  "durable_attempt_journal_sqlite_indexes"
  "durable_attempt_journal_replay_policy_matrix"
  "order_endpoint_attempts"
  "endpoint_attempt_id_sha256"
  "request_id_sha256"
  "client_order_id_sha256"
  "broker_order_id_sha256"
  "replay_fingerprint_set_sha256"
  "schema_design_only: true"
  "schema_version: 1"
  "endpoint_attempt_id_unique: true"
  "request_id_hash_indexed: true"
  "client_order_id_hash_indexed: true"
  "broker_order_id_hash_optional: true"
  "order_path_record_reference_required: true"
  "append_only: true"
  "begin_immediate_required: true"
  "wal_required: true"
  "synchronous_full_required: true"
  "writer_lock_required: true"
  "schema_version_guard_required: true"
  "idempotent_replay_requires_same_fingerprint_set: true"
  "conflict_replay_rejects_and_disarms: true"
  "SameFingerprintSetIdempotentReplay"
  "DifferentFingerprintSetRejectAndDisarm"
  "raw_values_compared: false"
  "operator_disarm_on_conflict: true"
  "GatewayRealOrderEndpointDurableJournalMigrationStepKind"
  "GatewayRealOrderEndpointDurableJournalMigrationStep"
  "GatewayRealOrderEndpointDurableJournalMigrationRunbookDesignShape"
  "durable_journal_migration_runbook_steps"
  "BackupBeforeMigration"
  "AcquireSingleWriterLock"
  "OpenSqliteWithWal"
  "SetSynchronousFull"
  "VerifySchemaVersion"
  "CreateOrderEndpointAttemptsTable"
  "CreateReplayIndexes"
  "RunIntegrityCheck"
  "RefuseAutoRepair"
  "migration_runbook_design_only: true"
  "begin_immediate_required_for_schema_change: true"
  "sqlite_integrity_check_required: true"
  "corruption_open_failure_disarms: true"
  "stale_or_unknown_lock_disarms: true"
  "auto_repair_allowed: false"
  "auto_stale_lock_delete_allowed: false"
  "operator_runbook_required: true"
  "redacted_operator_diagnostics_only: true"
  "GatewayRealOrderEndpointCanonicalReplayFingerprintField"
  "GatewayRealOrderEndpointCanonicalReplayEncoding"
  "GatewayRealOrderEndpointCanonicalReplayFingerprintFieldEntry"
  "GatewayRealOrderEndpointCanonicalReplayFingerprintDesignShape"
  "canonical_replay_fingerprint_fields"
  "Utf8JsonObjectSortedKeysNoWhitespace"
  "SchemaVersion"
  "Operation"
  "EndpointAttemptIdSha256"
  "RequestIdSha256"
  "ClientOrderIdSha256"
  "AccountSha256"
  "InstrumentSha256"
  "CheckpointLabel"
  "RequestFingerprintSha256"
  "CheckpointProofSha256"
  "CapturedEnvelopeSha256"
  "OutcomeSha256"
  "StateTransitionSha256"
  "AckDiagnosticSha256"
  "fingerprint_spec_design_only: true"
  "stable_field_order_required: true"
  "sorted_keys_required: true"
  "whitespace_forbidden: true"
  "refactor_changes_require_schema_bump: true"
  "GatewayRealOrderEndpointAttemptIdLifecyclePhase"
  "GatewayRealOrderEndpointAttemptIdLifecyclePolicyEntry"
  "GatewayRealOrderEndpointAttemptIdLifecycleDesignShape"
  "endpoint_attempt_id_lifecycle_policy"
  "GeneratedAfterApprovedRequestParts"
  "BoundBeforeEndpointSend"
  "PersistedWithAttemptJournal"
  "ReusedOnlyForIdempotentReplay"
  "NeverReusedForNewAttemptAfterTerminalOrManual"
  "lifecycle_design_only: true"
  "generated_after_approved_request_parts: true"
  "generated_before_future_endpoint_send: true"
  "same_attempt_id_replay_requires_same_fingerprint_set: true"
  "reuse_after_timeout_manual_or_terminal_allowed: false"
  "new_endpoint_attempt_requires_new_id: true"
  "GatewayRealOrderEndpointImplementationGateReadinessItem"
  "GatewayRealOrderEndpointImplementationGateReadinessStatus"
  "GatewayRealOrderEndpointImplementationGateReadinessChecklistEntry"
  "GatewayRealOrderEndpointImplementationGateReadinessDesignShape"
  "implementation_gate_readiness_checklist"
  "ForbiddenSurfaceScanners"
  "EndpointGateUnconstructible"
  "DurableStoreImplementedTested"
  "OperatorArmImplementedTested"
  "RateLimitBackoffImplementedTested"
  "NoBlindRetryImplementedTested"
  "RedactedAckImplementedTested"
  "ImplementedAndTested"
  "PendingEvidenceOrWaiver"
  "checklist_design_only: true"
  "release_profile_evidence_or_waiver_pending: true"
  "positive_get_order_evidence_or_waiver_pending: true"
  "route_template_recheck_pending: true"
  "endpoint_calls_allowed_for_readiness: false"
  "GatewayRealOrderEndpointCanonicalReplayGoldenVector"
  "GatewayRealOrderEndpointCanonicalReplayGoldenVectorDesignShape"
  "canonical_replay_golden_vectors"
  "place_order_schema_v1_sorted_keys_no_whitespace"
  "d467afd3b7d320c26966a1a400995e00664397ed47bb74320a418cfd2524abc6"
  "golden_vectors_design_only: true"
  "canonical_json_no_whitespace: true"
  "all_fields_hash_or_safe_label: true"
  "refactor_changes_require_vector_update_and_schema_bump: true"
  "GatewayRealOrderEndpointOperatorReplayRunbookCase"
  "GatewayRealOrderEndpointOperatorReplayRunbookEntry"
  "GatewayRealOrderEndpointOperatorReplayRunbookDesignShape"
  "operator_replay_runbook_entries"
  "IdempotentReplaySameFingerprint"
  "ConflictingReplayDisarm"
  "TimeoutUnknownPending"
  "ManualIntervention"
  "TerminalOutcomeNewAttempt"
  "operator_runbook_design_only: true"
  "idempotent_replay_case_present: true"
  "conflicting_replay_disarms: true"
  "timeout_requires_new_attempt_id: true"
  "manual_requires_new_attempt_id: true"
  "terminal_requires_new_attempt_id: true"
  "redacted_diagnostics_only: true"
  "GatewayRealOrderEndpointEvidenceClosurePackageStatus"
  "GatewayRealOrderEndpointEvidenceClosurePackageEntry"
  "GatewayRealOrderEndpointEvidenceClosurePackageDesignShape"
  "GatewayRealOrderEndpointRouteTemplateRecheckPlanDesignShape"
  "GatewayRealOrderEndpointEvidenceReportReadinessDesignShape"
  "evidence_closure_package_entries"
  "PendingEvidenceOrWaiver"
  "EvidenceProvided"
  "WaiverAccepted"
  "closure_package_design_only: true"
  "all_slots_require_evidence_or_waiver: true"
  "undocumented_2xx_slot_present: true"
  "cancel_409_410_slot_present: true"
  "route_template_recheck_design_only: true"
  "exact_two_route_allowlist_required: true"
  "official_docs_or_waiver_required: true"
  "recheck_before_implementation_gate: true"
  "route_templates_exported_as_design_data_only: true"
  "rendered_routes_exported: false"
  "raw_account_or_order_id_exported: false"
  "order_endpoint_calls_allowed_for_recheck: false"
  "evidence_report_readiness_design_only: true"
  "canonical_replay_golden_vector_sha256"
  "canonical_replay_vector_count"
  "readiness_implemented_tested_count"
  "readiness_pending_evidence_or_waiver_count"
  "operator_replay_runbook_case_count"
  "GatewayRealOrderEndpointOutcomeStatePolicyEntry"
  "GatewayRealOrderEndpointAcceptedBrokerIdPolicyEntry"
  "GatewayRealOrderEndpointOutcomeStatePolicyDesignShape"
  "GatewayRealOrderEndpointDurableCheckpointCapabilityDesignShape"
  "GatewayRealOrderEndpointCheckpointMarkerCreationDesignShape"
  "GatewayRealOrderEndpointRequestSnapshotFingerprint"
  "GatewayRealOrderEndpointSqliteTransitionCommitProof"
  "create_place_checkpoint_marker_after_sqlite_transition"
  "create_cancel_checkpoint_marker_after_sqlite_transition"
  "creation_requires_sqlite_transition_commit_proof: true"
  "proof_bound_to_request_snapshot_fingerprint: true"
  "proof_fingerprint_includes_request_client_account_instrument_hashes: true"
  "proof_raw_request_values_exported: false"
  "marker_single_use_required: true"
  "checkpoint_reuse_across_intents_allowed: false"
  "creation_rejects_diagnostic_or_report_source: true"
  "creation_requires_durable_commit_observed: true"
  "creation_requires_transition_event_match: true"
  "creation_requires_fingerprint_operation_match: true"
  "future_send_outcome_state_policy_matrix"
  "accepted_broker_id_policy_matrix"
  "matrix_serializable: true"
  "outcome_entry_count"
  "accepted_broker_id_policy_entry_count"
  "ack_reason_mapping_redacted: true"
  "operator_disarm_backoff_manual_matrix_present: true"
  "accepted_broker_id_policy_inherited: true"
  "timeout_no_blind_retry_invariant: true"
  "outcome_diagnostic_can_bypass_state_machine: false"
  "place_ack_reason_code"
  "cancel_ack_reason_code"
  "CommandAckReasonCode::CancelTimeoutUnknownPending"
  "backoff_required: true"
  "manual_intervention_required: true"
  "no_blind_retry: true"
  "raw_broker_order_id_exported: false"
  "PlaceEndpointDurableCheckpointApproved"
  "CancelEndpointDurableCheckpointApproved"
  "place_capability_type_internal: true"
  "cancel_capability_type_internal: true"
  "capability_not_debug_or_serializable: true"
  "created_after_sqlite_transition_only: true"
  "CurrentDenyAllOrderPostDelete"
  "FutureExactTwoRouteAllowlistAfterReview"
  "real_post_delete_calls_allowed_now: false"
)

for pattern in "${required_patterns[@]}"; do
  if ! rg -n "$pattern" "$target" >/dev/null; then
    report_failure "required API-shape marker missing: $pattern"
  fi
done

if (( failures > 0 )); then
  exit 1
fi

echo "order-endpoint-scanner-transition-spec: ok"
