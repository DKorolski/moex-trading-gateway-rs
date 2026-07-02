# M3c-12 outcome state and ACK policy matrix

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Outcome to state policy

M3c-12 adds a serializable policy matrix for future send outcomes. The matrix
binds each design-only outcome to order-path state-machine events/states,
redacted runtime ACK status/reason policy, and operator action policy:

```text
Accepted              -> SubmitAccepted / CancelAccepted
Rejected              -> BrokerRejected / manual cancel rejection handling
TimeoutUnknownPending -> timeout/unknown state + no blind retry
RateLimited           -> backoff + manual intervention
Maintenance           -> degraded/manual intervention
Unauthorized          -> disarm/operator intervention
DecodeError           -> decode/manual intervention
TransportError        -> typed transport-category manual intervention
```

The timeout policy is operation-aware:

```text
Place  -> TimeoutUnknownPending
Cancel -> CancelTimeoutUnknownPending
```

Every matrix entry records:

```text
state_machine_transition_required = true
result_diagnostic_can_bypass_state_machine = false
runtime_ack_redacted_only = true
```

## Accepted broker id inheritance

M3c-12 also records the future accepted-result identity policy inherited from
the M3b reconciliation work:

```text
accepted with broker id       -> Submitted
accepted without broker id    -> SubmittedPendingBrokerOrderId + reconciliation
empty broker id / decode      -> manual intervention
broker id mismatch            -> reconciliation conflict/manual intervention
```

The policy does not export raw broker order ids. Missing, empty, or mismatched
broker ids require reconciliation/manual handling and no blind retry.

## Durable checkpoint capability design

The durable checkpoint boundary now has explicit private marker types:

```text
PlaceEndpointDurableCheckpointApproved
CancelEndpointDurableCheckpointApproved
```

They remain internal, non-`Debug`, and non-serializable. In a future
implementation they may become constructible only after the corresponding
SQLite transition is durably persisted:

```text
Place  -> BeginSubmit persisted before endpoint send
Cancel -> RequestCancel persisted before endpoint send
```

## Negative boundary

The policy matrix is separate from send diagnostics. Diagnostics remain
redacted and cannot feed transport or bypass the state machine. The scanner
transition spec now checks the M3c-12 markers and still rejects order-call
surfaces in the design-only module.

## M3c-13 follow-up

M3c-13 refines this matrix with typed transport categories and accepted-result
classification. True timeout/unknown-pending is separated from non-timeout
transport failures, and `Accepted` must be classified through broker-order-id
policy before any future state/ACK export.

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
