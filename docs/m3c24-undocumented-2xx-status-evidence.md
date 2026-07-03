# M3c-24 undocumented 201/202/204 status evidence

Status: evidence-only closure for undocumented `201/202/204` status semantics.
This increment does not add or authorize real FINAM order `POST` / `DELETE`,
command consumption, real ACK lifecycle, runtime/live attachment, `LiveReady`,
first live micro, stop/SLTP, or bracket.

## Goal

M3c-24 closes:

```text
undocumented_2xx_status_semantics: Pending -> EvidenceProvided
```

The evidence is documentation-backed and policy-backed:

- current official FINAM REST documentation lists `200` as successful response
  for PlaceOrder and CancelOrder;
- `201`, `202`, and `204` are not documented success statuses for those
  endpoints;
- existing status matrix policy does not classify undocumented `201/202/204` as
  immediate accepted/submitted.

## Evidence artifact

After creating a clean handoff archive, generate:

```bash
python3 scripts/m3c_undocumented_2xx_status_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

It writes:

```text
reports/m3c-order-endpoint-gate/undocumented-2xx-status-evidence.json
reports/m3c-order-endpoint-gate/undocumented-2xx-status-evidence.json.sha256
```

The artifact records source commit, source archive SHA-256, official docs URL,
official docs SHA-256, per-endpoint section hashes, documented success status
presence, undocumented `201/202/204` absence, forbidden-surface scan status/hash,
and the closed trading-boundary booleans. It does not export raw docs sections.

## Semantics policy

For both PlaceOrder and CancelOrder:

```text
200 -> documented success path
201/202/204 -> undocumented 2xx defensive path
```

The undocumented defensive path remains:

```text
future_send_outcome = DecodeError
order_path_state = ManualInterventionRequired
ack_status = Error
ack_reason_code = ResponseDecodeError
operator_disarm_signal = OrderEndpointDecodeError
state_machine_transition_required = true
no_blind_retry = true
broker_truth_reconciliation_required = true
```

So M3c-24 does not convert `201/202/204` into accepted/submitted live behavior.

## Design evidence status

For M3c-24, generate the gate report with:

```bash
cargo run -p broker-cli -- m3c-order-endpoint-gate-report \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip \
  --release-profile-status evidence-provided \
  --positive-get-order-status waiver-accepted \
  --route-template-recheck-status evidence-provided \
  --undocumented-2xx-status evidence-provided
```

Expected slot counts:

```text
evidence_slot_count = 5
evidence_provided_or_waiver_count = 4
evidence_pending_count = 1
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

## Follow-up in M3c-25

M3c-25 closes the final implementation-gate evidence slot,
`cancel_409_410_status_semantics`, with official FINAM REST documentation
evidence plus the existing defensive cancel reconciliation policy. It remains
evidence-only and does not authorize real order endpoints.
