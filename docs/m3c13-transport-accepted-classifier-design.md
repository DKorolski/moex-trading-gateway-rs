# M3c-13 transport category and accepted-result classifier design

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Typed transport category policy

M3c-13 separates true timeout/unknown-pending semantics from other transport
failures:

```text
DnsOrConnectError -> non-timeout transport failure
TlsError          -> non-timeout transport failure
HttpSendError     -> non-timeout transport failure
BodyReadError     -> non-timeout transport failure
Timeout           -> timeout/unknown-pending
```

Only the `Timeout` category maps to:

```text
send_outcome = TimeoutUnknownPending
Place ACK    = TimeoutUnknownPending
Cancel ACK   = CancelTimeoutUnknownPending
Place state  = TimeoutUnknownPending
Cancel state = CancelTimeoutUnknownPending
```

Non-timeout transport categories map to manual/degraded handling and do not use
timeout ACK reasons or timeout/unknown states. They remain no-blind-retry and
state-machine-bound.

## Accepted-result classifier design

M3c-13 makes accepted response handling explicit. A future `Accepted` endpoint
result must be classified before state/ACK export:

```text
WithBrokerOrderId      -> SubmitAccepted / Submitted
WithoutBrokerOrderId   -> SubmittedPendingBrokerOrderId + reconciliation
EmptyBrokerOrderId     -> ResponseDecodeError + manual intervention
BrokerOrderIdMismatch  -> ReconciliationConflict + manual intervention
```

This wires the M3c accepted-result classifier to the existing accepted
broker-id policy matrix. `Accepted` is not allowed to mean unconditional
`Submitted`.

The classifier remains private, requires `EndpointGateApproved`, consumes
`ApprovedOrderEndpointRequestParts`, and does not accept diagnostic DTOs.

## Durable checkpoint marker creation design

M3c-13 records the future checkpoint marker creation rule:

```text
Place marker  -> only after BeginSubmit SQLite transition commit proof
Cancel marker -> only after RequestCancel SQLite transition commit proof
```

The marker creation functions are private and require:

```text
EndpointGateApproved
GatewayRealOrderEndpointSqliteTransitionCommitProof
durable_commit_observed = true
diagnostic_or_report_source = false
matching transition event
```

Diagnostic/report layers cannot create checkpoint markers.

## M3c-14 follow-up

M3c-14 binds the checkpoint proof to a redacted request snapshot fingerprint,
adds cancel accepted response/id policy, and records a redacted captured
response/error envelope design. The envelope exports only kind, presence,
length/hash, and typed transport category.

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
