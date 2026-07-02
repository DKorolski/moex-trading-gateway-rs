# M3c-11 future send result boundary

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Future send outcome shape

M3c-11 adds a design-only classified outcome enum for future order endpoint
attempts:

```text
Accepted
Rejected
TimeoutUnknownPending
RateLimited
Maintenance
Unauthorized
DecodeError
TransportError
```

The classifier remains private and design-only:

```text
classify_future_send_attempt_result(
    &EndpointGateApproved,
    ApprovedOrderEndpointRequestParts,
    GatewayRealOrderEndpointFutureSendOutcome,
)
```

It requires the endpoint gate marker, consumes the internal approved
request-parts capability by value, and does not accept diagnostic DTOs.

## Operation-specific durable checkpoints

The durable checkpoint is now operation-specific:

```text
Place  -> PlaceBeginSubmitPersistedBeforeEndpoint
Cancel -> CancelRequestCancelPersistedBeforeEndpoint
```

This records the intended future invariant:

```text
Place: BeginSubmit persisted before endpoint send
Cancel: RequestCancel persisted before endpoint send
```

## Single-use and state-machine policy

The future send result design records:

```text
future_send_consumes_request_parts = true
request_parts_reuse_after_outcome_allowed = false
retry_after_timeout_unknown_allowed = false
state_machine_transition_required = true
result_diagnostic_can_bypass_state_machine = false
```

This keeps timeout/unknown outcomes out of blind retry behavior and forces
future implementation through the order-path state machine.

## Redacted result diagnostics

`GatewayRealOrderEndpointFutureSendDiagnostic` exports only redacted metadata:

```text
rendered_path_redacted = true
rendered_path_exported = false
raw_body_exported = false
runtime_ack_redacted_only = true
network_enabled = false
```

It does not export raw rendered path, request body, account id, broker order id,
instrument symbol, or client order id.

## Scanner-transition guard

`scripts/order_endpoint_scanner_transition_spec.sh` checks that:

```text
future_send_network_enabled = false
future_send_consumes_request_parts = true
future_send_accepts_diagnostics = false
operation_specific_durable_checkpoint_required = true
retry_after_timeout_unknown_allowed = false
request_parts_reuse_after_outcome_allowed = false
result_diagnostic_can_bypass_state_machine = false
state_machine_transition_required = true
```

The existing guard still rejects order-call surfaces and any `reqwest` token in
the design-only module.

## M3c-12 follow-up

M3c-12 extends this boundary with a design-only outcome state/ACK policy
matrix. Each future outcome now maps to order-path events/states, redacted ACK
status/reason policy, operator disarm/backoff/manual policy, and no-blind-retry
constraints.

The timeout ACK reason policy is operation-aware:

```text
Place  -> TimeoutUnknownPending
Cancel -> CancelTimeoutUnknownPending
```

M3c-12 also records accepted broker-order-id inheritance and private
operation-specific durable checkpoint capability markers. Result diagnostics
still cannot bypass the state machine.

## Evidence slots

These slots remain deliberately pending before any real order implementation
gate:

```text
release_profile_evidence_or_waiver = Pending
positive_get_order_evidence_or_waiver = Pending
route_template_recheck = Pending
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
