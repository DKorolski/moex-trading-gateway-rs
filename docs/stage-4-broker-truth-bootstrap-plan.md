# Stage 4A — broker-truth bootstrap plan

Status: Stage 4A / Stage 4A-1 accepted as planning/evidence-schema foundation;
Stage 4B existing type inventory and v2 alignment decision accepted; Stage 4C
validated bootstrap wrapper accepted; Stage 4D FINAM read-only
broker-truth source normalization accepted; Stage 4E broker-truth to runtime
bootstrap application evidence accepted; Stage 4F dirty-start / explicit
adoption / manual-intervention policy accepted; Stage 4G runtime lifecycle
ordering evidence accepted; Stage 4H paper/mock runtime-host bootstrap
integration tests accepted; Stage 4I redacted operator-facing bootstrap
evidence report accepted; Stage 4J FINAM Stage 4 report assembly bridge
accepted. Stage 4 macro-stage accepted/closed as the broker-truth bootstrap
foundation; Stage 5 is the next active macro-stage.

Date: 2026-07-10.

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

Stage 4A-1 expands the Stage 4 breakdown and strengthens the evidence schema so
that coding starts from existing broker-truth/runtime-host surfaces instead of
creating a parallel domain.

Stage 4B documents the actual existing type inventory and records the v2
alignment decision in
[`stage-4/4b-existing-broker-truth-type-inventory.md`](stage-4/4b-existing-broker-truth-type-inventory.md).

Stage 4C adds the validated wrapper described in
[`stage-4/4c-validated-broker-truth-bootstrap.md`](stage-4/4c-validated-broker-truth-bootstrap.md).

Stage 4D adds the FINAM read-only source-normalization wrapper described in
[`stage-4/4d-finam-readonly-broker-truth-source-normalization.md`](stage-4/4d-finam-readonly-broker-truth-source-normalization.md).

Stage 4E adds the validated broker-truth application gate and defensive
consistency guard described in
[`stage-4/4e-runtime-bootstrap-application-evidence.md`](stage-4/4e-runtime-bootstrap-application-evidence.md).

Stage 4F adds the dirty-start / explicit adoption / manual-intervention policy
gate with application-evidence consistency and runtime-owned-order guards
described in
[`stage-4/4f-dirty-start-adoption-policy.md`](stage-4/4f-dirty-start-adoption-policy.md).

Stage 4G adds runtime lifecycle ordering evidence described in
[`stage-4/4g-runtime-lifecycle-ordering-evidence.md`](stage-4/4g-runtime-lifecycle-ordering-evidence.md).

Stage 4H adds paper/mock runtime-host bootstrap integration tests described in
[`stage-4/4h-paper-mock-runtime-bootstrap-integration-tests.md`](stage-4/4h-paper-mock-runtime-bootstrap-integration-tests.md).

Stage 4I adds the redacted operator-facing bootstrap evidence report described
in
[`stage-4/4i-redacted-bootstrap-evidence-report.md`](stage-4/4i-redacted-bootstrap-evidence-report.md).

Stage 4J adds the FINAM Stage 4 report assembly bridge described in
[`stage-4/4j-finam-stage4-report-assembly-bridge.md`](stage-4/4j-finam-stage4-report-assembly-bridge.md).

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

## 4. Existing type inventory gate

Stage 4B must inventory the existing broker-truth and runtime-bootstrap surfaces
before introducing new DTOs or wrappers:

| Existing surface | Location | Stage 4B requirement |
| --- | --- | --- |
| `BrokerTruthSnapshot` | `broker-core::operational_snapshot` | Reuse/extend or explicitly wrap; do not duplicate incompatibly. |
| `RuntimeHostBootstrapSnapshot` | `broker-core::runtime_host` | Inventory target-scoped conversion and lifecycle assumptions. |
| `RuntimeBootstrapSnapshotDto` | `broker-core::runtime_state` | Inventory passive DTO compatibility with broker-neutral ids. |
| FINAM broker-truth mapper | `broker-finam::mapper` | Inventory read-only source normalization and missing fields. |
| M3f broker-truth issue machinery | `finam-gateway` | Inventory orphan/order/trade/stale issue classification. |
| Broker-truth parity helpers | `broker-core::parity` and historical M4 docs | Inventory reusable ALOR/FINAM comparison contracts. |

Stage 4B acceptance must include an explicit decision for every surface:
`reuse`, `extend`, `wrap_v2`, or `out_of_scope_with_reason`.

No duplicate incompatible `BrokerTruthSnapshot` or
`RuntimeHostBootstrapSnapshot` may be introduced without a separate ADR.

## 5. Bootstrap lifecycle order

Stage 4 must prove lifecycle order, not only snapshot shape:

```text
LoadBrokerTruthSnapshot
  -> LoadRuntimeState
  -> NotifyBootstrapSnapshot
  -> NotifyRuntimeStateRestored
  -> WarmupHistory
  -> RecoverPendingStreams
```

Required lifecycle invariants:

- broker truth is loaded before runtime state is trusted;
- bootstrap snapshot notification happens after broker truth is loaded;
- runtime state restored notification happens after bootstrap snapshot
  notification;
- warmup history happens after runtime state restore;
- pending stream recovery happens after warmup;
- live orders are disabled during bootstrap and warmup;
- first runtime intent before broker truth is a blocker.

## 6. Dirty-start policy

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

Adoption applied means all of the following are true:

- adoption was attempted;
- adoption was allowed by explicit strategy policy;
- adoption was applied with redacted evidence;
- adopted target position/order counts match broker truth;
- manual-intervention reason is absent.

Target non-flat cannot silently become flat. Target active orders cannot
silently disappear.

## 7. Position policy

- Target-symbol non-zero position quantity is open position truth.
- Target-symbol zero-quantity rows are not open positions, but must be counted
  diagnostically.
- Account-wide zero-quantity rows are diagnostic only.
- Non-target positions must not make the target strategy non-flat.
- Target position freshness is required before runtime readiness.

## 8. Active order policy

- Target-symbol active orders are lifecycle truth.
- Account-wide active orders are safety diagnostics and may block a shared
  account according to later account-safety policy.
- Unknown target active order status blocks readiness.
- Active orders without known owner/request correlation are orphaned until
  explicitly reconciled or manually waived.
- Terminal orders do not count as active, but recent terminal rows may be used
  for reconciliation diagnostics.
- Ownership/correlation classes must distinguish `RuntimeOwned`,
  `AdoptedFromBootstrap`, `ObservedAccountWide`, and `UnknownOrOrphan`.
- A broker order row alone does not prove strategy ownership.

## 9. Trade policy

- Recent trades are correlation evidence.
- A broker trade row by itself does not prove strategy ownership.
- Unknown trade/order correlation should become a readiness blocker if it
  affects the target instrument and cannot be reconciled.
- Duplicate trade replay must be idempotent by broker trade id.
- Trade classification must distinguish strategy-attributed, observed
  unattributed, unknown, and orphan target trades.

## 10. Freshness policy

Every bootstrap snapshot must include freshness metadata:

- broker truth checked timestamp;
- per-section checked timestamps where available;
- per-section ages in seconds;
- stale position/order/trade indicators;
- schedule/session freshness;
- source status: `Fresh`, `Stale`, `Unknown`, or `Unavailable`.

`Fresh` is allowed only when the section age is less than or equal to the
accepted `max_age_seconds`. Unknown or stale target broker truth blocks
`LiveReady`.

## 11. Readiness blockers

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

## 12. Evidence schema

The Stage 4A evidence schema is defined in
[`stage-4/4a-broker-truth-bootstrap-evidence-schema.md`](stage-4/4a-broker-truth-bootstrap-evidence-schema.md).

The schema is redacted and report-only. It is intended to prove that bootstrap
inputs and blockers were classified before runtime readiness could be considered.

## 13. Stage 4A / 4A-1 acceptance criteria

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
- Stage 4 breakdown expanded without compressing lifecycle/adoption/evidence
  gates;
- existing broker-truth/runtime-host type inventory required before coding;
- lifecycle-order evidence schema added;
- explicit adoption/manual-intervention evidence schema added;
- order/trade ownership and correlation evidence fields added;
- numeric freshness evidence added;
- runtime-live remains blocked;
- real FINAM command consumer remains blocked;
- real orders remain blocked;
- forbidden scanners green;
- cargo fmt/test/clippy green.

## 14. Still forbidden

Stage 4A does not authorize:

- runtime-live;
- real FINAM command consumer;
- strategy-driven real FINAM orders;
- real FINAM `POST`/`DELETE` from runtime;
- Stop/SLTP/bracket/replace/multi-leg live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `i64` surrogate adapter without a new ADR.

## 15. Stage 4 breakdown

After Stage 4A-1 acceptance, the next reviewable slices are:

1. Stage 4B — existing broker-truth type inventory and v2 alignment decision
   (accepted).
2. Stage 4C — `BrokerTruthSnapshot` v2 /
   `RuntimeHostBootstrapSnapshot` DTO types and validation
   (accepted after P1 hardening and final adoption-count guard).
3. Stage 4D — FINAM read-only broker-truth mapper and fixture-backed source
   normalization (accepted).
4. Stage 4E — `BrokerTruthSnapshot` -> `RuntimeHostBootstrapSnapshot`
   application evidence (accepted).
5. Stage 4F — dirty-start / explicit adoption / manual-intervention policy
   (accepted).
6. Stage 4G — bootstrap lifecycle order enforcement (accepted).
7. Stage 4H — paper/mock bootstrap integration tests (accepted).
8. Stage 4I — redacted broker-truth bootstrap evidence report generator
   (accepted).
9. Stage 4J — FINAM Stage 4 report assembly bridge
   (accepted).

Stage 4 is accepted/closed as the broker-truth bootstrap foundation. The next
active macro-stage is Stage 5 — real strategy semantics attachment.

All of these remain paper/mock/read-only until a later macro-stage explicitly
authorizes command-consumer or live execution work.
