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
  GatewayRealOrderEndpointSqliteTransitionCommitProof \
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
