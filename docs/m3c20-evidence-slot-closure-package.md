# M3c-20 evidence slot closure package and route-template recheck plan

Status: design/report-only hardening. This increment does not add or authorize
real FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Evidence closure package

M3c-20 records a self-contained closure package for all implementation-gate
evidence slots:

```text
release_profile_evidence_or_waiver
positive_get_order_evidence_or_waiver
route_template_recheck
undocumented_2xx_status_semantics
cancel_409_410_status_semantics
```

Each slot must close through evidence or reviewer-accepted waiver before the
implementation gate. Closure artifacts must be redacted, source-archive-bound,
and reviewer accepted. Real order endpoint calls are not allowed for closure.

## Route-template recheck plan

Route-template recheck remains design/report-only:

```text
POST   /v1/accounts/{account_id}/orders
DELETE /v1/accounts/{account_id}/orders/{order_id}
```

The recheck requires official docs confirmation or reviewer-accepted waiver,
exactly two route templates, no rendered live routes, and no raw account/order
ids. It does not call FINAM order endpoints.

## Evidence report enrichment

`broker-cli m3c-order-endpoint-gate-report` now includes the M3c-19 readiness
and golden-vector summary fields in `design-evidence.json`:

```text
canonical_replay_golden_vector_sha256
canonical_replay_vector_count
readiness_implemented_tested_count
readiness_pending_evidence_or_waiver_count
operator_replay_runbook_case_count
evidence_slot_count
evidence_pending_count
evidence_provided_or_waiver_count
```

The CLI also accepts pending/evidence/waiver statuses for undocumented `2xx`
and cancel `409/410` semantics while keeping all defaults pending.

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
