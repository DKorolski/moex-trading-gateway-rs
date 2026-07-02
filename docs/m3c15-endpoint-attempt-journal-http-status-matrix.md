# M3c-15 endpoint attempt journal and HTTP status outcome matrix

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Endpoint attempt journal design

M3c-15 records the future endpoint-attempt binding:

```text
ApprovedOrderEndpointRequestParts
operation-specific checkpoint marker
captured response/error envelope
future send outcome classifier
endpoint_attempt_id hash
request snapshot fingerprint
```

The attempt journal binding remains private and non-serializable. Public output
is only a redacted diagnostic:

```text
endpoint_attempt_id hash present
endpoint_attempt_id sha256 length = 64
request snapshot fingerprint present
raw path/body/error exported = false
state_machine_transition_required = true
state_machine_bypass_allowed = false
```

## HTTP status/body-shape outcome matrix

M3c-15 adds a serializable design matrix for Place and Cancel:

```text
2xx accepted body variants
400 / 422 broker reject or invalid request
401 / 403 unauthorized
408 / 504 timeout/unknown-pending
429 rate limit
500 / 502 / 503 maintenance
malformed body decode error
transport category failures via the captured-envelope transport matrix
```

The matrix is operation-specific:

```text
Place accepted with broker id      -> SubmitAccepted / Submitted
Place accepted without broker id   -> SubmittedPendingBrokerOrderId
Cancel accepted matching id        -> CancelAccepted / CancelSubmitted
Cancel accepted missing id         -> accepted pending reconciliation
Cancel accepted mismatched id      -> manual/reconciliation conflict
```

All entries require captured envelope + attempt journal + state-machine
transition and export no raw path/body/error.

Diagnostics remain output-only. They cannot construct request parts, recreate a
captured envelope, feed a future transport boundary, or bypass the order-path
state machine.

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
