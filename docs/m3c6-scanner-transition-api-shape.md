# M3c-6 scanner transition spec and pre-implementation API shape

Status: design-only. This increment does not add or authorize real FINAM order
`POST` / `DELETE`, command consumption, real ACK lifecycle, runtime/live
attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## API shape module

The future gateway-owned order endpoint boundary is represented by:

```text
crates/finam-gateway/src/real_order_endpoint.rs
```

This module is deliberately API-shape only:

```text
mode = DesignOnlyNoHttpSend
api_shape_contains_route_templates = false
real_post_delete_calls_allowed_now = false
```

It contains no `reqwest` token, no real HTTP send, no `.post(`, no `.delete(`,
no `.request(`, no `.send(`, and no `Method::POST/DELETE`.

## Gate-marker signatures

M3c-7 separates the design/report shape from gated route-shape functions.
Future route-shape functions require `EndpointGateApproved` in their signatures:

```text
place_order_api_shape(&EndpointGateApproved, &FinamPlaceOrderRequestSpec)
cancel_order_api_shape(&EndpointGateApproved, &FinamCancelOrderRequestSpec)
```

The marker remains unconstructible, so these signatures are compile/API shape,
not implementation enablement.

M3c-8 keeps the actual route template shape internal-only and non-serializable.
Exported gated helpers return only redacted diagnostics:

```text
GatewayRealOrderEndpointRedactedRouteDiagnostic
route_template_redacted = true
route_template_exported = false
```

M3c-9 adds a private approved request-parts design boundary. The public API
shape records:

```text
approved_request_parts_type_internal = true
rendered_path_type_internal = true
rendered_path_exported = false
raw_body_exported = false
diagnostic_can_construct_request_parts = false
constructor_count = 2
```

M3c-10 records the private approved request-parts consumer design:

```text
consumer_internal_only = true
consumer_requires_endpoint_gate = true
consumer_accepts_approved_request_parts_only = true
consumer_accepts_diagnostics = false
consumer_network_enabled = false
consumer_count = 1
```

M3c-11 records the design-only future send result boundary:

```text
outcome_count = 8
future_send_requires_endpoint_gate = true
future_send_consumes_request_parts = true
future_send_network_enabled = false
operation_specific_durable_checkpoint_required = true
request_parts_reuse_after_outcome_allowed = false
retry_after_timeout_unknown_allowed = false
state_machine_transition_required = true
```

M3c-12 records the design-only future send outcome policy matrix:

```text
matrix_serializable = true
outcome_entry_count = 8
accepted_broker_id_policy_entry_count = 4
ack_reason_mapping_redacted = true
operator_disarm_backoff_manual_matrix_present = true
accepted_broker_id_policy_inherited = true
timeout_no_blind_retry_invariant = true
outcome_diagnostic_can_bypass_state_machine = false
state_machine_transition_required = true
```

The operation-specific durable checkpoint capability design remains private and
non-serializable:

```text
PlaceEndpointDurableCheckpointApproved
CancelEndpointDurableCheckpointApproved
created_after_sqlite_transition_only = true
```

M3c-13 records transport-category and accepted-result classifier design:

```text
transport category count = 5
timeout_separated_from_non_timeout_transport = true
non_timeout_transport_does_not_use_timeout_ack_reason = true
accepted_result_classifier_requires_endpoint_gate = true
accepted_broker_id_policy_wired = true
unconditional_submitted_allowed = false
checkpoint marker creation requires SQLite transition commit proof
```

The accepted-result classifier and checkpoint marker creation functions remain
private and diagnostics cannot feed either boundary.

M3c-14 records request-bound checkpoint and captured-envelope design:

```text
proof_bound_to_request_snapshot_fingerprint = true
marker_single_use_required = true
checkpoint_reuse_across_intents_allowed = false
cancel accepted id policy entry count = 3
envelope_diagnostic_redacted_only = true
envelope_accepts_raw_path_body_or_error = false
raw_path/body/error exported = false
status_len_hash_presence_only = true
```

M3c-15 records endpoint-attempt journal and HTTP status outcome matrix design:

```text
journal_internal_only = true
endpoint_attempt_id_hash_len = 64
binds_approved_request_parts = true
binds_checkpoint_marker = true
binds_captured_envelope = true
binds_outcome_classifier = true
diagnostic_redacted_only = true
matrix covers 2xx/400/422/401/403/408/504/429/500/502/503/malformed body
place_cancel_specific_mapping = true
state_machine_transition_required = true
```

M3c-16 records durable attempt journal and FINAM status semantics design:

```text
durable_journal_schema_design_only = true
append_requires_endpoint_gate = true
binds_checkpoint_proof_fingerprint = true
binds_captured_envelope_fingerprint = true
binds_outcome_fingerprint = true
binds_state_transition_result_fingerprint = true
binds_ack_diagnostic_fingerprint = true
documented_success_status_only_200 = true
undocumented_success_201_202_204_require_evidence_or_waiver = true
cancel_404_documented_and_requires_reconciliation = true
cancel_409_410_policy_or_waiver_required = true
status_semantics_can_bypass_state_machine = false
```

M3c-17 records evidence closure and durable journal SQLite schema design:

```text
plan_design_only = true
all_slots_pending_before_implementation_gate = true
order_endpoint_calls_allowed_for_closure = false
schema_design_only = true
table_name = order_endpoint_attempts
schema_version = 1
endpoint_attempt_id_unique = true
idempotent_replay_requires_same_fingerprint_set = true
conflict_replay_rejects_and_disarms = true
raw endpoint/request/broker/path/body/error exported = false
```

The scanner transition guard checks these markers while keeping all real order
endpoint calls forbidden.

M3c-18 records migration/runbook, replay-fingerprint, and attempt-id lifecycle
markers:

```text
migration_runbook_design_only = true
wal_required = true
synchronous_full_required = true
single_writer_lock_required = true
schema_version_guard_required = true
sqlite_integrity_check_required = true
corruption_open_failure_disarms = true
stale_or_unknown_lock_disarms = true
auto_repair_allowed = false
auto_stale_lock_delete_allowed = false
fingerprint_spec_design_only = true
encoding = Utf8JsonObjectSortedKeysNoWhitespace
stable_field_order_required = true
sorted_keys_required = true
whitespace_forbidden = true
refactor_changes_require_schema_bump = true
lifecycle_design_only = true
same_attempt_id_replay_requires_same_fingerprint_set = true
reuse_after_timeout_manual_or_terminal_allowed = false
new_endpoint_attempt_requires_new_id = true
```

These markers are still design/report-only; they do not enable endpoint calls.

M3c-19 records implementation-gate readiness, golden-vector, and operator
runbook markers:

```text
checklist_design_only = true
ImplementedAndTested
PendingEvidenceOrWaiver
release_profile_evidence_or_waiver_pending = true
positive_get_order_evidence_or_waiver_pending = true
route_template_recheck_pending = true
endpoint_calls_allowed_for_readiness = false
golden_vectors_design_only = true
canonical_json_no_whitespace = true
expected_sha256 = d467afd3b7d320c26966a1a400995e00664397ed47bb74320a418cfd2524abc6
all_fields_hash_or_safe_label = true
operator_runbook_design_only = true
conflicting_replay_disarms = true
timeout_requires_new_attempt_id = true
manual_requires_new_attempt_id = true
terminal_requires_new_attempt_id = true
redacted_diagnostics_only = true
```

These markers are also design/report-only; they do not enable endpoint calls.

M3c-20 records evidence closure package and route-template recheck markers:

```text
closure_package_design_only = true
slot_count = 5
all_slots_require_evidence_or_waiver = true
undocumented_2xx_slot_present = true
cancel_409_410_slot_present = true
route_template_recheck_design_only = true
exact_two_route_allowlist_required = true
official_docs_or_waiver_required = true
recheck_before_implementation_gate = true
order_endpoint_calls_allowed_for_recheck = false
evidence_report_readiness_design_only = true
canonical_replay_golden_vector_sha256
canonical_replay_vector_count
readiness_implemented_tested_count
readiness_pending_evidence_or_waiver_count
operator_replay_runbook_case_count
```

The evidence report enrichment remains design/report-only and does not enable
endpoint calls.

## Scanner transition spec

The API shape exports:

```text
current_mode = CurrentDenyAllOrderPostDelete
future_mode = FutureExactTwoRouteAllowlistAfterReview
exact_place_order_surface_count = 1
exact_cancel_order_surface_count = 1
allowed_route_template_count = 2
approved_module_path = crates/finam-gateway/src/real_order_endpoint.rs
real_post_delete_calls_allowed_now = false
```

The future allowlist remains design data only:

```text
POST   /v1/accounts/{account_id}/orders
DELETE /v1/accounts/{account_id}/orders/{order_id}
```

## Shell guard

`scripts/order_endpoint_scanner_transition_spec.sh` verifies that the design-only
module exists, contains the required gate-marker/API-shape markers, and still
has no HTTP send surface.

## Pending evidence slots

Before any implementation gate, these slots must be `EvidenceProvided` or
`WaiverAccepted`:

```text
release_profile_evidence_or_waiver
positive_get_order_evidence_or_waiver
route_template_recheck
```

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
