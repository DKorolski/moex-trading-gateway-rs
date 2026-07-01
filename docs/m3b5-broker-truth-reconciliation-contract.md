# M3b-5 broker-truth reconciliation source contract

Status: dry/non-network. M3b-5 does not authorize FINAM `POST /orders`,
FINAM `DELETE /orders/{order_id}`, real command consumption, real FINAM
CommandAck lifecycle, runtime strategy attachment, `LiveReady`, live micro,
stop/SLTP, or bracket behavior.

M3b-5 defines the broker-truth side of cancel reconciliation after uncertain
cancel endpoint outcomes such as 404/409/410.

M3b-6 extends this single-source classification contract with source freshness,
precedence, conflict handling, and multi-source follow-up simulation. See
`docs/m3b6-broker-truth-source-semantics.md`.

## Dry-only execution client naming

The older approved execution simulator trait is now explicitly dry-only:

```text
FinamDryApprovedOrderExecutionClient
MockFinamDryApprovedOrderExecutionClient
```

This trait is for M3a/M3b tests and scripted dry outcomes only. It must not be
used as the production FINAM network abstraction.

The future real endpoint boundary remains:

```text
EndpointGateApproved + FinamPlace/CancelOrderRequestSpec
    -> FinamOrderEndpointClassifiedResponse
```

## Broker-truth sources

Dry cancel reconciliation can be characterized from these source classes:

```text
OrdersSnapshot
GetOrder
TradesSnapshot
PositionSnapshot
```

M3b-5 does not implement real `get_order`, trades, or position polling for
order recovery. It defines the typed/redacted contract that those sources must
feed later.

## Observation and diagnostic boundary

`CancelBrokerTruthObservation` is not serde-exportable. It may be built from a
broker-neutral order object, but its `Debug` output is redacted.

The export/reporting boundary is:

```text
CancelBrokerTruthDiagnostic
```

The diagnostic includes only:

- truth source class;
- order id presence/length;
- client order id presence/length;
- status present flag;
- terminal / still-working / unknown status class;
- stale flag;
- age and max-age in milliseconds.

It does not include raw broker order ids, raw client order ids, raw unknown
native status strings, or raw response bodies.

## Classification table

Fresh broker truth:

| Broker-neutral order status | Truth |
| --- | --- |
| `Filled` / `Canceled` / `Rejected` / `Expired` | `Terminal` |
| `New` / `Working` / `PartiallyFilled` | `StillWorking` |
| `Unknown(_)` or missing order/status | `Unknown` |

Stale broker truth:

| Condition | Truth | Operator signal |
| --- | --- | --- |
| age > `max_age_ms` | `Unknown` | `ReconciliationStale` |
| negative/unparseable age | `Unknown` | `ReconciliationStale` |

Fresh unknown truth uses `UnknownPendingOrder` as the operator disarm signal.

## Follow-up state matrix

The M3b-4 follow-up outcomes are now driven by broker-truth classification:

| Classified truth | Follow-up outcome | ACK |
| --- | --- | --- |
| `Terminal` | `CancelRecoveredTerminal` | `RecoveredByBrokerTruth` |
| `StillWorking` | `ManualInterventionRequired` | `ManualInterventionRequired` |
| `Unknown` | `UnknownPending` | `ReconciliationRequired` |

The helper remains guarded: it only accepts records already in uncertain cancel
states, such as `CancelTimeoutUnknownPending` or
`ManualInterventionRequired + ReconciliationRequired` after a cancel attempt.

## Source guards

M3b-5 adds production source-scan guards:

- old non-dry execution trait names must not reappear;
- production real transport boundaries must not return mapped/raw endpoint
  results;
- classified response remains the real transport boundary shape.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
