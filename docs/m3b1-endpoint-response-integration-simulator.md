# M3b-1 endpoint response integration simulator

Status: dry/non-network. M3b-1 does not authorize FINAM `POST /orders`,
FINAM `DELETE /orders/{order_id}`, real command consumption, real FINAM
CommandAck lifecycle, runtime strategy attachment, `LiveReady`, live micro,
stop/SLTP, or bracket behavior.

M3b-1 takes the synthetic/redacted endpoint fixtures from M3b-0 and routes them
through the order-path state machine. This proves how future FINAM endpoint
outcomes will affect durable state, redacted ACK publication, operator disarm,
and no-blind-retry behavior before any real transport exists.

## Public simulator surface

`finam-gateway` now exposes dry fixture integration helpers:

```text
simulate_place_order_endpoint_fixture(...)
simulate_cancel_order_endpoint_fixture(...)
EndpointResponseIntegrationReport
EndpointResponseIntegrationOutcomeKind
EndpointResponseIntegrationSimulatorError
```

Inputs remain approved-only:

- `PreflightApprovedPlaceOrder` or `PreflightApprovedCancelOrder`;
- optional policy-generated `OutgoingOrderComment`;
- synthetic/redacted `FinamOrderEndpointFixture`;
- optional operator arm for disarm simulation.

The helpers build FINAM request specs but do not send HTTP. They do not require
or construct `EndpointGateApproved`, and they do not implement a real transport.

M3b-2 follow-up: local/mock HTTP-shaped endpoint responses are now classified in
`docs/m3b2-local-http-endpoint-mapper-hardening.md`. Those helpers persist
`BeginSubmit` or `RequestCancel` before response classification, so
post-network decode/map errors cannot bypass durable attempt recording.

## Outcome mapping

Execution-like endpoint outcomes use the same state semantics as the existing
dry execution simulator:

| Fixture class | Place state | Cancel state | ACK |
| --- | --- | --- | --- |
| accepted with broker id | `Submitted` | `CancelSubmitted` | `Submitted` |
| accepted without broker id | `SubmittedPendingBrokerOrderId` | `CancelSubmitted` | `UnknownPending` for place, `Submitted` for cancel |
| rejected | `BrokerRejected` | `ManualInterventionRequired` | `Rejected` |
| timeout | `TimeoutUnknownPending` | `CancelTimeoutUnknownPending` | `Timeout` |

Non-execution endpoint outcomes are deliberately conservative:

| Fixture class | State | ACK | Error kind | Disarm signal |
| --- | --- | --- | --- | --- |
| rate limited | `ManualInterventionRequired` | `Error / RateLimited` | `RateLimited` | `OrderEndpointRateLimited` |
| maintenance | `ManualInterventionRequired` | `Error / BrokerMaintenance` | `BrokerMaintenance` | `OrderEndpointMaintenance` |
| decode error | `ManualInterventionRequired` | `Error / ResponseDecodeError` | `ResponseDecodeError` | `OrderEndpointDecodeError` |

For non-execution outcomes the simulator first persists `BeginSubmit` or
`RequestCancel`, then persists `RequireManualIntervention`. This mirrors the
future requirement that the local durable path must know an endpoint attempt was
started before any ambiguous or unsafe response handling is recorded.

## No-blind-retry behavior

After rate-limit, maintenance, or decode-error integration, the record is in
`ManualInterventionRequired`. A second submit/cancel attempt against the same
record fails at the state-machine boundary before any future transport could be
called.

Rate-limit fixtures preserve `retry_after_ms` in the redacted integration
report for future backoff wiring.

## SQLite-backed proof

M3b-1 includes a SQLite-backed rate-limit fixture test:

```text
InsertIntent
BeginSubmit
RequireManualIntervention
```

The audit reason is the safe enum name `RateLimited`; `safe_details` remains the
constant component marker `sqlite_order_path_store`. Raw account/client/broker
ids and raw broker payload text are not stored in the audit table.

## ACK redaction boundary

The local integration report may carry the normal local `CommandAck` object for
operator/internal handling. Runtime-facing Redis publication still goes through
`publish_dry_command_ack()`, which removes raw client and broker order ids.

Tests publish rate-limit ACKs and verify that Redis payloads contain only the
safe reason code and no raw account/client/broker identifiers.

## Endpoint gate status

`EndpointGateApproved` remains unconstructible at M3b-1. The real endpoint gate
still blocks on `M3a11PreEndpointReviewRequired`, and the post-review approval
constant remains false.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
