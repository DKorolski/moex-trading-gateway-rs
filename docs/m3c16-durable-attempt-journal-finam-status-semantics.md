# M3c-16 durable attempt journal and FINAM status semantics

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Durable endpoint-attempt journal contract

M3c-16 refines the M3c-15 attempt binding into a future durable journal schema.
The internal append input binds only fingerprints:

```text
endpoint_attempt_id hash
approved request/request snapshot fingerprint
checkpoint proof fingerprint
captured envelope fingerprint
outcome classifier fingerprint
state transition result fingerprint
ACK diagnostic fingerprint
```

The durable record remains private and non-serializable. Public diagnostics are
redacted-only and export no raw endpoint-attempt id, request identity, broker
order id, path, body, or error. The append boundary requires
`EndpointGateApproved`, approved request parts, and the operation-specific
checkpoint marker.

## FINAM status semantics

The current REST-doc characterization records success as documented `200` for
Place and Cancel. `201`, `202`, and `204` are not accepted as submitted by
default; they require implementation-gate evidence or an explicit waiver before
live use.

Design policy:

```text
Place 200 -> accepted only when submitted body identity is decodable
Cancel 200 -> accepted; response body/id may be optional
Cancel 404 -> read-only reconciliation required
Cancel 409/410 -> defensive policy only; evidence/waiver required
Undocumented 201/202/204 -> manual/decode path until evidence/waiver
```

All entries still require state-machine transition and no-blind-retry. Status
semantics cannot bypass the order-path state machine and export no raw
path/body/error/broker-order-id values.

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
