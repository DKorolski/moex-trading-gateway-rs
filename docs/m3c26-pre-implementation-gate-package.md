# M3c-26 pre-implementation gate package

Status: source-bound review package after all implementation-gate evidence
slots are closed. This increment does not add or authorize real FINAM order
`POST` / `DELETE`, command consumption, real ACK lifecycle, runtime/live
attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Goal

M3c-26 gathers a single review package proving:

```text
release_profile_evidence_or_waiver = EvidenceProvided
positive_get_order_evidence_or_waiver = WaiverAccepted
route_template_recheck = EvidenceProvided
undocumented_2xx_status_semantics = EvidenceProvided
cancel_409_410_status_semantics = EvidenceProvided

evidence_slot_count = 5
evidence_provided_or_waiver_count = 5
evidence_pending_count = 0
```

The package is a request for implementation-gate review. It is not an
implementation approval and does not make `EndpointGateApproved` constructible.

## Evidence artifact

After generating all source-bound evidence artifacts for the current clean
handoff archive, run:

```bash
python3 scripts/m3c_pre_implementation_gate_package.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

It writes:

```text
reports/m3c-order-endpoint-gate/pre-implementation-gate-package.json
reports/m3c-order-endpoint-gate/pre-implementation-gate-package.json.sha256
```

The package verifies:

- all six package inputs are present: design evidence plus five closure
  artifacts;
- all artifacts bind to the same source commit, archive name, and archive
  SHA-256;
- all five evidence slots are closed;
- `endpoint_calls_allowed = false`;
- `marker_constructible = false`;
- `real_post_delete_added = false`;
- `real_order_endpoint_enabled = false`;
- command consumer, order placement, cancel, and stop/SLTP/bracket flags remain
  false;
- forbidden scanners are green.

## Implementation decision request

The review request is limited to a future exact-two-route implementation
decision:

```text
POST   /v1/accounts/{account_id}/orders
DELETE /v1/accounts/{account_id}/orders/{order_id}
```

M3c-26 does not enable these routes. It only packages the evidence required for
a reviewer to decide whether a future implementation step may begin.

## Follow-up

After reviewer acceptance, M3d-0 records the implementation-transition decision
as a separate source-bound artifact. M3d-0 prepares the future exact-two-route
scanner transition rules while keeping the active deny-all order endpoint
scanner mode and the trading boundary closed.

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
