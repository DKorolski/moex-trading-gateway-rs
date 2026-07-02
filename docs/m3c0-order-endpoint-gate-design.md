# M3c-0 / M3c-2 order endpoint gate design

Status: design-only order endpoint gate. These increments do not add or
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
evidence
future_order_endpoint_allowlist
negative_test_plan
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

Checklist status vocabulary is intentionally strict:

```text
DesignRecorded
ImplementedAndTested
EvidenceProvided
WaiverAccepted
Blocked
```

`DesignRecorded` means the policy/design is captured but must not be confused
with implementation readiness. For example, account allowlist, instrument
allowlist, and unknown-active-order startup guard stay design-level items until
the implementation gate proves and tests them.

## M3c-2 evidence report

`broker-cli m3c-order-endpoint-gate-report` emits and saves a self-contained
M3c gate report. The command runs the forbidden-surface scan before writing the
report and records:

```text
evidence.forbidden_surface_scan.status
evidence.forbidden_surface_scan.script_path
evidence.forbidden_surface_scan.script_sha256
evidence.forbidden_surface_scan.checked_at
evidence.forbidden_surface_scan.exit_code
evidence.source.source_commit_full_sha
evidence.source.source_archive_name
evidence.source.source_archive_sha256
```

The report also keeps the remaining implementation-gate evidence slots explicit:

```text
release_profile_evidence_or_waiver = Pending
positive_get_order_evidence_or_waiver = Pending
route_template_recheck = Pending
```

Example operator command:

```bash
cargo run -p broker-cli -- m3c-order-endpoint-gate-report \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

The default output path is:

```text
reports/m3c-order-endpoint-gate/design-evidence.json
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

M3c-2 records the future allowlist as data in the design report:

```text
POST /v1/accounts/{account_id}/orders
DELETE /v1/accounts/{account_id}/orders/{order_id}
```

Before implementation, scan coverage must include negative tests that fail on:

- extra same-module order `POST`;
- extra same-module order `DELETE`;
- generic request wrapper using `POST`;
- generic request wrapper using `DELETE`;
- route-string bypasses;
- non-reqwest client abstractions.

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
