# M3b-6 broker-truth source semantics

Status: dry/non-network. M3b-6 does not authorize FINAM `POST /orders`,
FINAM `DELETE /orders/{order_id}`, real command consumption, real FINAM
CommandAck lifecycle, runtime strategy attachment, `LiveReady`, live micro,
stop/SLTP, or bracket behavior.

M3b-6 extends the M3b-5 broker-truth contract from single-source
classification to multi-source reconciliation precedence. The goal is to make
ambiguous cancel follow-up deterministic before any real endpoint is enabled.

M3b-7 builds on this source/precedence layer with a dry fetch orchestration
simulator, typed source missing/error reasons, and guarded position-derived
truth policy. See `docs/m3b7-broker-truth-orchestration-simulator.md`.

## Source builders

Dry builders now exist for the future broker-truth source classes:

```text
GetOrder
OrdersSnapshot
TradesSnapshot
PositionSnapshot
```

These builders take broker-neutral `Order`, `Trade`, and `Position` values and
produce a redacted `CancelBrokerTruthObservation`.

Freshness is source-based: the observation age is measured from the time the
get-order response or snapshot was received, not from a raw nested broker value.
This lets a fresh trades snapshot provide terminal evidence even when the trade
itself is older than the snapshot response.

Missing get-order or missing snapshot evidence is represented explicitly with
`evidence_present = false` and `Unknown` truth.

## Freshness policy

`CancelBrokerTruthFreshnessPolicy` is config-shaped and source-specific:

```text
get_order_max_age_ms
orders_snapshot_max_age_ms
trades_snapshot_max_age_ms
position_snapshot_max_age_ms
```

Stale source evidence never wins precedence. It becomes `Unknown` with
`ReconciliationStale` as the operator-visible disarm reason if no fresh known
source can resolve the decision.

## Precedence policy

The default precedence order is:

```text
GetOrder
OrdersSnapshot
TradesSnapshot
PositionSnapshot
```

Fresh known evidence wins over stale or unknown evidence. If several fresh
known sources agree on the same truth class, the highest-precedence source is
selected.

If fresh known sources disagree between `Terminal` and `StillWorking`, the
decision becomes:

```text
broker_truth = Unknown
decision_kind = Conflict
operator_disarm_signal = ReconciliationConflict
```

The conflicting source list is exported only as source classes, sorted by the
configured precedence. No raw broker ids, client ids, unknown native statuses,
or raw response bodies are exported.

## Trade and position semantics

Trade-derived evidence is terminal for cancel reconciliation: if a matching
trade is present in a fresh trades snapshot, it can recover a terminal outcome
when direct order truth is missing or unknown.

Position-derived evidence is weaker and lowest precedence. A non-flat matching
position can be terminal evidence for the narrow dry simulator contract, while
a flat or missing position remains `Unknown` rather than proving cancel success.

## Follow-up integration

`simulate_cancel_reconciliation_follow_up_from_broker_truth_decision` applies
the multi-source decision to the same guarded M3b-4/M3b-5 follow-up state
matrix:

- `Terminal` -> recovered terminal;
- `StillWorking` -> manual intervention required;
- `Unknown` -> unknown pending / reconciliation required;
- conflict -> unknown pending plus `ReconciliationConflict` operator disarm.

The helper still accepts only records already in uncertain cancel states after
a cancel attempt.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
