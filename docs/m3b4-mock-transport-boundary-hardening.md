# M3b-4 mock transport boundary and endpoint export hardening

Status: dry/non-network. M3b-4 does not authorize FINAM `POST /orders`,
FINAM `DELETE /orders/{order_id}`, real command consumption, real FINAM
CommandAck lifecycle, runtime strategy attachment, `LiveReady`, live micro,
stop/SLTP, or bracket behavior.

M3b-4 closes the remaining dry endpoint-response boundary gaps before any real
order transport can be reviewed.

## Export boundary hardening

`FinamOrderEndpointAcceptedDto` is now deserialize-only. It may carry a
broker-native order id while parsing an accepted endpoint response, so it must
not be used as a report/log/handoff export payload.

`FinamOrderEndpointFixture` is now a synthetic, non-serde fixture. Accepted
fixtures may contain a raw broker order id for dry tests, but the fixture itself
is not exportable. The safe export payload remains:

```text
FinamOrderEndpointResponseDiagnostic
```

Tests cover that diagnostics for accepted, rejected, rate-limited,
maintenance, unauthorized, reconciliation-required, timeout, body-read failure,
and decode-error paths do not contain raw broker ids or raw response bodies.

## Classified transport boundary

M3b-4 adds a dry/mock classified transport contract:

```text
FinamMockClassifiedEndpointTransport
```

The transport receives only approved FINAM request specs and returns only:

```text
FinamOrderEndpointClassifiedResponse
```

Raw local HTTP response bodies and accepted response DTOs must stay inside the
classifier layer. Source/contract tests guard that the mock transport boundary
does not expose `FinamOrderEndpointLocalHttpResponse`,
`FinamOrderEndpointAcceptedDto`, or raw body strings.

The future real endpoint transport compile contract was also tightened to
return `FinamOrderEndpointClassifiedResponse`. `EndpointGateApproved` remains
unconstructible, so this does not enable real endpoint calls.

## Durable ordering proof

Gateway dry tests prove the intended order:

```text
place:  InsertIntent -> BeginSubmit -> classified transport -> state transition
cancel: Submitted    -> RequestCancel -> classified transport -> state transition
```

SQLite-backed tests make the mock transport open the order-path store in
read-only mode at call time and assert that `SubmitInFlight` /
`CancelRequested` is already durable before the classified result is returned.

## Cancel reconciliation follow-up

M3b-4 adds dry follow-up modeling after uncertain cancel endpoint results such
as 404/409/410:

| Broker truth after reconciliation | Dry outcome |
| --- | --- |
| terminal | `CancelRecoveredTerminal` + `RecoveredByBrokerTruth` |
| still working | `ManualInterventionRequired` |
| unknown | `UnknownPending` remains |

The helper is guarded: it only accepts records that are already in an
uncertain cancel state (`CancelTimeoutUnknownPending` or
`ManualInterventionRequired` with `ReconciliationRequired` after a cancel
attempt). Non-cancel or non-uncertain states are rejected.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
