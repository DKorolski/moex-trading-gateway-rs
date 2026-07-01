# M3b-7 broker-truth orchestration simulator

Status: dry/non-network. M3b-7 does not authorize FINAM `POST /orders`,
FINAM `DELETE /orders/{order_id}`, real command consumption, real FINAM
CommandAck lifecycle, runtime strategy attachment, `LiveReady`, live micro,
stop/SLTP, or bracket behavior.

M3b-7 adds a dry broker-truth fetch orchestration layer on top of the M3b-6
source precedence simulator. It models how a future gateway should collect
truth after ambiguous cancel outcomes without adding real broker order calls.

## Orchestration flow

The default dry flow is:

```text
GetOrder
OrdersSnapshot
TradesSnapshot
PositionSnapshot
classify each attempted source
apply freshness and precedence
produce redacted orchestration report
optionally drive guarded cancel follow-up
```

The source order is config-shaped through
`CancelBrokerTruthOrchestrationPolicy.precedence`. Duplicate configured sources
are ignored. Sources absent from the configured order are reported as
`NotRequested` in fetch diagnostics but are not used for decision selection.

## Typed missing/error reasons

Fetch diagnostics now distinguish source-specific reasons:

```text
NotFound404
Timeout
DecodeError
Maintenance
Unauthorized
NotRequested
MissingFixture
PositionGuardRejected
```

These reasons are exported only as typed classes. They do not include raw
broker ids, client ids, native status strings, or payload bodies.

## Operator disarm policy

The orchestration report carries one selected operator safety signal:

| Condition | Signal |
| --- | --- |
| unauthorized source error | `OrderEndpointUnauthorized` |
| maintenance source error | `OrderEndpointMaintenance` |
| decode source error | `OrderEndpointDecodeError` |
| fresh terminal-vs-working conflict | `ReconciliationConflict` |
| stale-only truth | `ReconciliationStale` |
| unknown-only truth | `UnknownPendingOrder` |

Fatal source errors take precedence over ordinary unknown/stale decisions for
operator disarm reporting.

## Position-derived truth guard

Position-derived terminal truth is guarded before it can participate as
terminal evidence.

The default guard requires:

- instrument match;
- side/intent context present;
- expected position delta context present;
- known strategy state;
- direct order/trade evidence is absent or stale after the direct sources were
  attempted.

If the guard fails, a non-flat position is downgraded to unknown with
`PositionGuardRejected`. This keeps standalone non-flat position evidence from
silently recovering the wrong cancel lifecycle.

## Redacted report

`CancelBrokerTruthOrchestrationReport` is the export boundary. It includes:

- policy snapshot: precedence version, source order, per-source `max_age_ms`,
  and position guard policy;
- per-source fetch diagnostics;
- redacted source truth diagnostics;
- final precedence decision;
- selected operator disarm signal.

The report is serde-exportable and intentionally contains no raw order id,
client id, native status string, or raw payload.

## Follow-up integration

`simulate_cancel_reconciliation_follow_up_from_broker_truth_orchestration`
drives the existing guarded cancel follow-up matrix from an orchestration
report. SQLite-backed tests prove that the post-classification follow-up still
uses the durable transition audit path:

```text
InsertIntent
RequestCancel
RequireManualIntervention
RecoverCancelTerminal
```

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
