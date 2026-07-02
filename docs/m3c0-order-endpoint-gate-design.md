# M3c-0 / M3c-1 order endpoint gate design

Status: design-only order endpoint gate. This increment does not add or
authorize FINAM `POST /orders`, FINAM `DELETE /orders/{order_id}`, real command
consumer attachment, real FINAM CommandAck lifecycle, runtime/live attachment,
`LiveReady`, live micro, stop/SLTP, or bracket behavior.

## Gate shape

`GatewayFeatureSet` now has an explicit order endpoint flag:

```text
real_order_endpoint_enabled = false
```

This flag is separate from:

```text
command_consumer_enabled
order_placement_enabled
cancel_enabled
stop_sltp_bracket_enabled
```

The flag is diagnostic and blocking at M3c-0. Setting it to `true` does not make
`EndpointGateApproved` constructible.

## Implementation-review blocker

The real endpoint decision now includes the M3c-specific blocker:

```text
M3cImplementationReviewRequired
```

The legacy M3a/M3b blocker remains:

```text
M3a11PreEndpointReviewRequired
```

Both blockers keep:

```text
endpoint_calls_allowed = false
EndpointGateApproved::try_from_decision(...) = Err(...)
runtime_ack_id_policy = RedactedRuntimeAckOnly
```

## Design report

`GatewayFeatureSet::m3c_order_endpoint_gate_design_report()` exports a
serializable diagnostic report:

```text
design_only
endpoint_calls_allowed
real_order_endpoint_enabled
command_consumer_enabled
order_placement_enabled
cancel_enabled
stop_sltp_bracket_enabled
marker_constructible
gate_decision
checklist
forbidden_surface_scan_must_remain_green
real_post_delete_added
```

The checklist records the minimum gate-design items requested by review:

```text
SeparateRealOrderEndpointFlagDefaultFalse
SeparateCommandConsumerFlagDefaultFalse
EndpointGateApprovedUnconstructibleUntilReview
OperatorArmOneShotTtl
AccountAllowlist
InstrumentAllowlist
OrderTypeTifQuantityNotionalPriceGuards
SqliteDurableStoreMandatory
UnknownActiveBrokerOrderStartupGuard
RateLimitBackoffPolicy
NoBlindRetryAfterSubmitOrCancelTimeout
ManualInterventionStateAndRunbook
RedactedAckExportPolicy
ForbiddenSurfaceScanExtensionPlan
ReleaseProfileEvidenceOrWaiver
RouteTemplateRecheck
PositiveGetOrderEvidenceOrWaiver
```

## Source-scan extension plan

At M3c-0, `scripts/forbidden_surface_scan.sh` must remain green and still blocks
accidental `.post(` / `.delete(` / `Method::POST` / `Method::DELETE` leakage.

A future explicit order endpoint implementation review must update the scan in
the same commit that introduces real endpoint transport, with an exact allowlist
for:

```text
POST /v1/accounts/{account_id}/orders
DELETE /v1/accounts/{account_id}/orders/{order_id}
```

That allowlist must be narrower than the current auth/session allowlist and must
fail on any other POST/DELETE occurrence.

## Preconditions before implementation gate

Before any implementation gate can be accepted, review still requires:

- release-profile real-readonly evidence or accepted waiver;
- current FINAM route-template recheck for `FinamRestDocs20260701`;
- real positive GetOrder evidence for an existing order or accepted waiver;
- operator arm one-shot/TTL runbook;
- SQLite durable id mapping store healthy;
- startup unknown active broker order guard;
- no-blind-retry policy for submit/cancel timeout;
- manual intervention runbook;
- redacted ACK/export policy unchanged.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
