# M3c-25 cancel 409/410 status evidence

Status: evidence-only closure for cancel `409/410` status semantics. This
increment does not add or authorize real FINAM order `POST` / `DELETE`, command
consumption, real ACK lifecycle, runtime/live attachment, `LiveReady`, first
live micro, stop/SLTP, or bracket.

## Goal

M3c-25 closes the last implementation-gate evidence slot:

```text
cancel_409_410_status_semantics: Pending -> EvidenceProvided
```

The evidence is documentation-backed and policy-backed:

- current official FINAM REST documentation for CancelOrder lists documented
  responses `200`, `400`, `401`, `404`, `429`, `500`, `503`, `504`, and
  `default`;
- `409` and `410` are not documented CancelOrder responses;
- existing status matrix policy does not classify cancel `409/410` as blind
  success.

## Evidence artifact

After creating a clean handoff archive, generate:

```bash
python3 scripts/m3c_cancel_409_410_status_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

It writes:

```text
reports/m3c-order-endpoint-gate/cancel-409-410-status-evidence.json
reports/m3c-order-endpoint-gate/cancel-409-410-status-evidence.json.sha256
```

The artifact records source commit, source archive SHA-256, official docs URL,
official docs SHA-256, CancelOrder section hash, documented status presence,
`409/410` absence, forbidden-surface scan status/hash, and the closed
trading-boundary booleans. It does not export raw docs sections.

## Semantics policy

For CancelOrder:

```text
200 -> documented cancel accepted path
404 -> documented not-found / read-only reconciliation path
409/410 -> undocumented defensive reconciliation path
```

The `409/410` defensive path remains:

```text
future_send_outcome = TimeoutUnknownPending
order_path_event = CancelTimedOut
order_path_state = CancelTimeoutUnknownPending
ack_status = UnknownPending
ack_reason_code = ReconciliationRequired
operator_disarm_signal = CancelTimeoutUnknownPending
cancel_reconciliation_required = true
state_machine_transition_required = true
no_blind_retry = true
```

So M3c-25 does not convert cancel `409/410` into blind success or terminal
cancel confirmation.

## Design evidence status

For M3c-25, generate the gate report with:

```bash
cargo run -p broker-cli -- m3c-order-endpoint-gate-report \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip \
  --release-profile-status evidence-provided \
  --positive-get-order-status waiver-accepted \
  --route-template-recheck-status evidence-provided \
  --undocumented-2xx-status evidence-provided \
  --cancel-409-410-status evidence-provided
```

Expected slot counts:

```text
evidence_slot_count = 5
evidence_provided_or_waiver_count = 5
evidence_pending_count = 0
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
