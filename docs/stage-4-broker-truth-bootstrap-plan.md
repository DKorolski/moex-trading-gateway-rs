# Stage 4A — broker-truth bootstrap plan

Status: implemented for review.

Date: 2026-07-09.

## 1. Goal

Stage 4 makes broker truth a required input to runtime lifecycle before any
runtime can be considered ready.

The target lifecycle is:

```text
LoadBrokerTruthSnapshot
  -> LoadRuntimeState
  -> NotifyBootstrapSnapshot
  -> NotifyRuntimeStateRestored
  -> WarmupHistory
  -> RecoverPendingStreams
```

Stage 4A is plan/schema only. It does not attach runtime-live, does not consume
real strategy commands, and does not place/cancel/replace real orders.

## 2. Broker truth inputs

`BrokerTruthSnapshot` should carry the broker-observed state needed by runtime
bootstrap:

| Area | Required fields | Source-of-truth policy |
| --- | --- | --- |
| Account | broker account id, account alias/fingerprint, currency, checked timestamp | Broker read-only truth, redacted in reports. |
| Cash/equity/margin | cash, equity, free funds, margin/GO where available | Diagnostic until mapped into strategy risk policy. |
| Positions | instrument id, broker symbol, quantity, average price, market value where available, freshness | Target-symbol non-zero quantity is position truth. |
| Active orders | broker order id, client order id if present, side, qty, remaining qty, status, instrument, freshness | Target-symbol active orders are lifecycle truth. |
| Recent trades | broker trade id, broker order id, qty, price, timestamp, instrument | Correlation input, not strategy ownership proof by itself. |
| Instrument identity | internal symbol, broker venue symbol, exchange, market, lot/tick metadata where available | Instrument-scoped matching is mandatory. |
| Schedule/session | session date, session state, schedule source, unknown state | Unknown schedule blocks readiness. |
| Diagnostics | account-wide row counts, zero-qty rows, orphan/unknown rows | Diagnostic unless target-scoped policy promotes to blocker. |

Raw broker payloads, account numbers, tokens, and unbounded broker dumps must not
be exported.

## 3. Runtime bootstrap conversion

Stage 4 converts broker truth into runtime bootstrap input:

```text
BrokerTruthSnapshot
  -> RuntimeHostBootstrapSnapshot
  -> strategy.on_bootstrap_snapshot(...)
```

`RuntimeHostBootstrapSnapshot` must be broker-neutral and instrument-scoped. It
must distinguish:

- target instrument truth;
- account-wide diagnostics;
- unknown/orphan broker rows;
- freshness gaps;
- dirty-start/manual-intervention blockers;
- values adopted by strategy policy;
- values preserved only as diagnostics.

## 4. Dirty-start policy

| Broker truth state | Strategy capability | Bootstrap disposition |
| --- | --- | --- |
| Target flat, no active target orders | Any | Clean bootstrap may continue. |
| Target non-flat | Strategy supports explicit adoption | Adopt only with explicit adopted-position state and redacted evidence. |
| Target non-flat | Strategy cannot adopt | `manual_intervention_required`. |
| Target active order exists | Explicit adoption/repair policy exists | Adopt/repair only behind paper boundary and evidence. |
| Target active order exists | No explicit policy | `manual_intervention_required`. |
| Unknown/orphan target order/trade | Any | Block `LiveReady`; manual reconciliation required. |
| Account-wide non-target rows | Any | Diagnostic by default; cannot determine target readiness alone. |

Dirty-start adoption must never be implicit.

## 5. Position policy

- Target-symbol non-zero position quantity is open position truth.
- Target-symbol zero-quantity rows are not open positions, but must be counted
  diagnostically.
- Account-wide zero-quantity rows are diagnostic only.
- Non-target positions must not make the target strategy non-flat.
- Target position freshness is required before runtime readiness.

## 6. Active order policy

- Target-symbol active orders are lifecycle truth.
- Account-wide active orders are safety diagnostics and may block a shared
  account according to later account-safety policy.
- Unknown target active order status blocks readiness.
- Active orders without known owner/request correlation are orphaned until
  explicitly reconciled or manually waived.
- Terminal orders do not count as active, but recent terminal rows may be used
  for reconciliation diagnostics.

## 7. Trade policy

- Recent trades are correlation evidence.
- A broker trade row by itself does not prove strategy ownership.
- Unknown trade/order correlation should become a readiness blocker if it
  affects the target instrument and cannot be reconciled.
- Duplicate trade replay must be idempotent by broker trade id.

## 8. Freshness policy

Every bootstrap snapshot must include freshness metadata:

- broker truth checked timestamp;
- per-section checked timestamps where available;
- stale position/order/trade indicators;
- schedule/session freshness;
- source status: `Fresh`, `Stale`, `Unknown`, or `Unavailable`.

Unknown or stale target broker truth blocks `LiveReady`.

## 9. Readiness blockers

Stage 4A defines these bootstrap blockers:

- broker truth missing;
- target position freshness unknown/stale;
- target active order freshness unknown/stale;
- target non-flat cannot be adopted;
- target active order cannot be adopted/repaired;
- unknown/orphan target order;
- unknown/orphan target trade;
- unknown schedule/session;
- instrument identity mismatch;
- broker truth source unavailable;
- raw broker payload export attempted.

## 10. Evidence schema

The Stage 4A evidence schema is defined in
[`stage-4/4a-broker-truth-bootstrap-evidence-schema.md`](stage-4/4a-broker-truth-bootstrap-evidence-schema.md).

The schema is redacted and report-only. It is intended to prove that bootstrap
inputs and blockers were classified before runtime readiness could be considered.

## 11. Stage 4A acceptance criteria

Stage 4A acceptance requires:

- `BrokerTruthSnapshot` field matrix documented;
- `RuntimeHostBootstrapSnapshot` mapping documented;
- source of truth per field documented;
- dirty-start matrix documented;
- zero-quantity position row policy documented;
- unknown/orphan order policy documented;
- active target order policy documented;
- freshness policy documented;
- redacted evidence schema added;
- runtime-live remains blocked;
- real FINAM command consumer remains blocked;
- real orders remain blocked;
- forbidden scanners green;
- cargo fmt/test/clippy green.

## 12. Still forbidden

Stage 4A does not authorize:

- runtime-live;
- real FINAM command consumer;
- strategy-driven real FINAM orders;
- real FINAM `POST`/`DELETE` from runtime;
- Stop/SLTP/bracket/replace/multi-leg live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `i64` surrogate adapter without a new ADR.

## 13. Next expected slices

After Stage 4A acceptance, the next reviewable slices should be:

1. Stage 4B — broker-truth DTO and runtime bootstrap snapshot types.
2. Stage 4C — read-only broker-truth loader and freshness/blocker evaluator.
3. Stage 4D — runtime bootstrap simulator under paper boundary.
4. Stage 4E — fixture-backed ALOR/FINAM broker-truth parity tests.

All of these remain paper/mock/read-only until a later macro-stage explicitly
authorizes command-consumer or live execution work.
