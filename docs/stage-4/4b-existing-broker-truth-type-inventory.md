# Stage 4B — existing broker-truth type inventory and v2 alignment decision

Status: implemented for review.

Date: 2026-07-09.

## Purpose

Stage 4B inventories the existing broker-truth/runtime-bootstrap surfaces before
Stage 4C introduces any validation, DTO extension, or migration code.

The key decision is deliberately conservative: Stage 4 must reuse and extend the
existing broker-neutral broker-truth domain. It must not introduce a parallel
incompatible `BrokerTruthSnapshot` or a lossy runtime-specific broker-truth
shape.

This document is design/inventory only. It does not attach runtime-live, does
not enable the real FINAM command consumer, and does not authorize real orders.

## Decision legend

| Decision | Meaning |
| --- | --- |
| `reuse` | Keep the existing surface as the canonical source without structural replacement. |
| `extend` | Keep the existing surface, then add validation, fields, evidence, or adapters in later Stage 4 slices. |
| `wrap_v2` | Keep the existing surface internally, but expose a validated wrapper/newtype for a stricter Stage 4 boundary. |
| `out_of_scope_with_reason` | Do not use this surface for Stage 4 bootstrap, with an explicit reason. |

## Inventory summary

| Surface | Location | Decision | Stage 4C action |
| --- | --- | --- | --- |
| `BrokerTruthSnapshot` | `broker-core::operational_snapshot` | `extend` | Add/derive v2 bootstrap validation and evidence around the existing canonical aggregate. |
| `RuntimeHostBootstrapSnapshot` | `broker-core::runtime_host` | `extend` | Extend target-scoped bootstrap with blockers/freshness/adoption evidence, or wrap with a validated Stage 4 DTO. |
| `RuntimeBootstrapSnapshotDto` | `broker-core::runtime_state` | `reuse` | Keep as restored runtime-state DTO; do not promote it to broker-truth source. |
| FINAM broker-truth mapper | `broker-finam::mapper` | `extend` | Fill missing Stage 4 bootstrap fields and fixture coverage while continuing to emit canonical broker truth. |
| FINAM broker-truth issue machinery | `finam-gateway` | `reuse` / `extend` | Reuse redacted issue/blocker classes and bridge them into Stage 4 readiness evidence. |
| Broker-truth parity helpers and historical M4 docs | `broker-core::parity`, M4 docs | `reuse` | Use as ALOR/FINAM oracle checks and requirements, not as a new runtime domain. |

## Surface inventory

### 1. `BrokerTruthSnapshot`

Location: `crates/broker-core/src/operational_snapshot.rs`.

Decision: `extend`.

Why:

- It is already the canonical broker-neutral aggregate for positions, orders,
  cash, trades, instruments, and account id.
- It already uses broker-neutral ids such as `BrokerAccountId`,
  `BrokerOrderId`, `BrokerTradeId`, `ClientOrderId`, and `InstrumentId`.
- It already exposes target-instrument helpers required by Stage 4:
  `target_position_qty`, `target_is_flat`,
  `open_positions_for_instrument`, `active_orders_for_instrument`,
  `unknown_orders_for_instrument`, `account_active_orders`,
  `account_orphan_orders`, and `summarize_for_instrument`.
- It already classifies lifecycle/quantity truth and orphan causes for orders
  and trades.

Fields covered:

- account identity;
- orders with broker/client ids, side/type/status/lifecycle, quantity, filled
  quantity, remaining quantity, limit price, optional broker asset/board/expiry,
  source timestamp, and receive timestamp;
- positions with instrument, quantity, average price, PnL, source timestamp,
  and receive timestamp;
- cash/equity/free cash/margin snapshot;
- trades with broker trade id, broker/client order ids, side, quantity, price,
  gross amount, commission, broker asset/board/expiry, source timestamp, and
  receive timestamp;
- instrument specs with broker symbol, internal symbol, exchange, market,
  tick/lot metadata, currency, schedule id, expiry, tradability, and margin
  where available;
- target/account summary counts and margin sufficiency helpers.

Fields or semantics still missing for Stage 4:

- explicit `schema_version` for broker-truth bootstrap evidence;
- per-section freshness status and per-section age values;
- schedule/session state carried in the broker-truth aggregate;
- explicit source status such as `Fresh`, `Stale`, `Unknown`, `Unavailable`;
- explicit runtime ownership classes:
  `RuntimeOwned`, `AdoptedFromBootstrap`, `ObservedAccountWide`,
  `UnknownOrOrphan`;
- explicit trade correlation classes:
  `StrategyAttributed`, `ObservedUnattributed`, `UnknownOrOrphan`;
- explicit dirty-start/adoption/manual-intervention disposition;
- explicit lifecycle-order evidence;
- zero-quantity row counts as first-class diagnostics.

Safety impact:

- Existing target-scoped helpers are the right safety base for Stage 4.
- Missing freshness/adoption/ownership fields mean `BrokerTruthSnapshot` alone
  is not enough to declare runtime bootstrap ready.
- Stage 4C must add validation/evidence around it rather than replacing it.

Tests required:

- target non-zero position blocks clean-flat bootstrap;
- target zero-quantity rows are diagnostic, not open position truth;
- target active order blocks or requires explicit adoption/repair;
- account-wide non-target active orders are diagnostic unless later account
  policy promotes them;
- unknown/orphan target order/trade blocks readiness;
- per-section stale/unknown truth blocks readiness;
- no raw broker/account payload is exported by evidence.

### 2. `RuntimeHostBootstrapSnapshot`

Location: `crates/broker-core/src/runtime_host.rs`.

Decision: `extend`.

Why:

- It already converts `BrokerTruthSnapshot` into a target-instrument runtime
  bootstrap view through `RuntimeHostBootstrapSnapshot::from_broker_truth`.
- It already preserves the critical target-vs-account distinction:
  target position quantity, target open positions, target active orders, account
  active order count, target flat flag, and received timestamp.
- It lives in the runtime-host boundary, which is the correct place to express
  bootstrap lifecycle readiness before strategy code trusts restored state.

Fields covered:

- account id;
- target instrument;
- target position quantity;
- target open positions;
- target active orders;
- account-wide active order count;
- target flat flag;
- broker-truth receive timestamp.

Fields or semantics still missing for Stage 4:

- unknown/orphan target order/trade counts;
- account-wide unknown/orphan diagnostics;
- recent trade correlation summary;
- cash/margin/instrument/spec/schedule freshness summaries;
- dirty-start disposition;
- explicit adoption attempt/allowed/applied evidence;
- manual-intervention reason;
- lifecycle-order proof;
- first-runtime-intent-before-broker-truth counter.

Safety impact:

- The existing type is a useful Stage 4 starting point, but too small to be the
  final readiness artifact.
- Stage 4C should either extend this type directly or introduce a validated
  wrapper that contains this snapshot plus blockers/freshness/adoption evidence.

Tests required:

- `from_broker_truth` keeps target position/order scope and does not infer
  target state from account-wide rows;
- non-target account activity remains diagnostic in the bootstrap snapshot;
- missing/unknown target truth becomes a blocker in the Stage 4 validated
  wrapper.

### 3. `RuntimeBootstrapSnapshotDto`

Location: `crates/broker-core/src/runtime_state.rs`.

Decision: `reuse`.

Why:

- It is a restored runtime-state DTO, not broker truth.
- It already supports broker-neutral `BrokerOrderId(String)` maps and validates
  that map keys match payload broker order ids.
- It preserves old ALOR numeric order-id import compatibility without creating
  a new FINAM surrogate adapter.

Fields covered:

- `working_orders`;
- `working_orders_strategy`;
- `known_order_ids`;
- `account_wide_orders_count`;
- validation for order-map key/payload mismatch;
- validation for duplicate known broker order ids.

Fields or semantics intentionally not covered:

- broker positions;
- broker cash/margin;
- broker trades;
- broker instrument specs;
- broker-truth freshness;
- dirty-start adoption policy.

Safety impact:

- Runtime state must be loaded after broker truth, but it must not override
  broker truth.
- Stage 4 must compare restored runtime state with broker truth and produce
  blockers/manual-intervention evidence if they disagree.

Stage 4C action:

- Keep this DTO as restored-state input.
- Add the Stage 4 bridge/validation that checks restored pending/working state
  against `BrokerTruthSnapshot` and `RuntimeHostBootstrapSnapshot`.
- Do not introduce an `i64` surrogate adapter.

Tests required:

- old numeric ALOR ids still deserialize as broker-neutral string ids;
- broker-native string ids remain exact;
- ACK/request-id handling remains exact and does not use `ClientOrderId` as a
  substitute for `StrategyRequestId`;
- restored working orders that are absent from broker truth become blockers or
  manual-intervention evidence.

### 4. FINAM broker-truth mapper

Location: `crates/broker-finam/src/mapper.rs`.

Decision: `extend`.

Why:

- The mapper already emits `broker_core::operational_snapshot::BrokerTruthSnapshot`.
- It maps FINAM account positions, orders, trades, cash, and instrument
  artifacts into the broker-neutral domain.
- It already enriches order/trade identity from instrument specs and builds a
  canonical readiness package with broker readiness, margin sufficiency, live
  entry decision, canonical preflight decision, and stop-order waiver policy.

Fields covered:

- account id;
- non-zero positions;
- orders with lifecycle/status/quantity truth;
- optional trades;
- cash snapshot;
- instrument specs from asset/params/schedule artifacts;
- broker readiness freshness inputs;
- margin sufficiency and canonical preflight decision.

Fields or semantics still missing for Stage 4:

- zero-quantity position row diagnostics are filtered out before broker truth;
- schedule/session state is used by readiness mapping but is not carried inside
  `BrokerTruthSnapshot`;
- per-section freshness ages are not first-class broker-truth fields;
- runtime ownership/adoption/correlation classes are not produced;
- unknown/orphan target trade/order counts are not emitted as Stage 4 evidence;
- source availability/failure status is not represented in the canonical
  broker-truth aggregate.

Safety impact:

- FINAM mapper is the correct source normalization layer.
- Stage 4 must extend this mapper/evidence path before treating FINAM
  broker-truth bootstrap as mandatory runtime input.

Stage 4C/4D action:

- Preserve `BrokerTruthSnapshot` as mapper output.
- Add zero-quantity diagnostics or a companion summary.
- Carry schedule/session/freshness into the Stage 4 validated bootstrap
  evidence.
- Add fixture-backed mapper tests for active/terminal/unknown orders, zero
  positions, orphan trades, instrument identity, stale sections, and missing
  read-only surfaces.

Tests required:

- FINAM fixture with zero target position row remains target-flat but records
  zero-row diagnostic count;
- FINAM fixture with target active order produces target active blocker;
- FINAM fixture with unknown order status produces unknown target blocker;
- FINAM fixture with missing/ambiguous instrument identity produces blocker;
- FINAM fixture with stale/unavailable orders/trades/positions produces
  freshness blocker.

### 5. FINAM broker-truth issue machinery

Location: `crates/finam-gateway/src/lib.rs`.

Decision: `reuse` / `extend`.

Why:

- M3f/M3g machinery already classifies broker-truth reconciliation issues in a
  redacted, non-live way.
- It already has issue kinds for:
  `SameIdentityDifferentRequestId`, `OrphanBrokerOrder`,
  `OrphanBrokerTrade`, `PositionMismatch`, and `LocalPendingStale`.
- It already converts snapshot issues/manual-intervention states into
  readiness blockers and keeps raw broker payload export disabled.

Fields covered:

- redacted broker/client/order/instrument fingerprints;
- issue counts;
- manual-intervention count;
- read-only surface boundary;
- no real FINAM order endpoint use;
- runtime-live and LiveReady disabled in reconciliation reports.

Fields or semantics still missing for Stage 4:

- direct linkage to `broker-core::operational_snapshot::BrokerTruthSnapshot`
  summaries;
- explicit dirty-start adoption disposition;
- explicit bootstrap lifecycle-order evidence;
- explicit per-section freshness age values;
- explicit account-wide diagnostic vs target blocker distinction in the Stage 4
  evidence shape.

Safety impact:

- The issue machinery should feed Stage 4 blockers, not become a second broker
  truth model.
- Any issue that affects target instrument bootstrap must block runtime
  readiness until reconciled or manually handled in a later approved policy.

Stage 4C action:

- Bridge M3f/M3g issue kinds into Stage 4 readiness blockers.
- Keep redacted-only evidence and raw payload export guards.
- Keep runtime-live and real command consumer disabled.

Tests required:

- orphan target broker order becomes Stage 4 blocker;
- orphan target trade becomes Stage 4 blocker;
- local pending stale becomes manual-intervention or readiness blocker;
- raw broker payload export attempt invalidates Stage 4 evidence;
- non-target issue remains diagnostic unless shared-account policy says
  otherwise.

### 6. Broker-truth parity helpers and historical M4 docs

Locations:

- `crates/broker-core/src/parity.rs`;
- existing M4 parity docs such as broker-truth/instrument identity, ALOR/FINAM
  parity, and runtime-host parity notes.

Decision: `reuse`.

Why:

- `compare_broker_truth_for_instrument` already compares ALOR and FINAM
  broker-truth snapshots through the canonical `BrokerTruthSnapshot`.
- It already checks target position quantity, flatness, target/account active
  order counts, unknown order counts, orphan order counts, non-target active
  order diagnostics, received timestamp skew, and target instrument spec
  compatibility.
- Historical M4 docs preserve operational parity requirements learned from the
  ALOR contour and should remain the requirements oracle.

Fields covered:

- target quantity and flatness parity;
- active/unknown/orphan order parity;
- received timestamp skew;
- instrument spec compatibility;
- bar parity for market-data evidence.

Fields or semantics still missing for Stage 4:

- lifecycle-order evidence;
- dirty-start adoption evidence;
- per-section broker-truth freshness evidence;
- runtime ownership/correlation class parity;
- bootstrap-ready vs bootstrap-blocked status mapping.

Safety impact:

- Parity helpers are suitable acceptance checks, not live enablers.
- `live_order_authorized` remains false in parity reports.

Stage 4C action:

- Reuse existing parity helpers for Stage 4 fixture-backed ALOR/FINAM broker
  truth checks.
- Add bootstrap-specific parity/evidence checks only after the existing
  broker-truth surfaces are wrapped or extended.

Tests required:

- ALOR fixture and FINAM fixture with equivalent target truth produce no
  blocking parity issues;
- target position/order mismatch blocks cutover;
- stale/skewed broker-truth timestamp blocks cutover;
- instrument spec mismatch blocks cutover;
- parity reports never authorize live orders.

## No duplicate domain policy

Stage 4C must not add a second incompatible broker-truth aggregate. The accepted
path is:

```text
FINAM read-only mapper
  -> broker-core::operational_snapshot::BrokerTruthSnapshot
  -> Stage 4 validated bootstrap wrapper/evidence
  -> RuntimeHostBootstrapSnapshot / strategy bootstrap notification
```

Allowed:

- extend existing broker-core types;
- add a validated wrapper around existing broker-core types;
- add redacted evidence summaries;
- add mapper diagnostics that are clearly derived from canonical broker truth.

Not allowed without a new ADR:

- a parallel `BrokerTruthSnapshotV2` that duplicates the existing aggregate with
  incompatible semantics;
- an `i64` surrogate adapter for FINAM order ids;
- a runtime-local broker truth model that bypasses `broker-core`;
- treating `RuntimeBootstrapSnapshotDto` as broker truth;
- treating a broker order/trade row as strategy-owned without exact
  request/correlation evidence.

## Stage 4C action list

Stage 4C should implement a small validated bootstrap layer, still paper/mock
and read-only:

1. Define the validated Stage 4 bootstrap wrapper around existing
   `BrokerTruthSnapshot` and `RuntimeHostBootstrapSnapshot`.
2. Include explicit status:
   `BootstrapReady`, `BootstrapBlocked`, `ManualInterventionRequired`,
   `BrokerTruthIncomplete`, `BrokerTruthStale`, `InstrumentMismatch`,
   `UnknownSchedule`, `EvidenceIncomplete`, or `SafetyBoundaryOpen`.
3. Add per-section freshness evidence for positions, orders, trades, cash,
   instruments, and schedule/session.
4. Add target-vs-account-wide position/order/trade summaries.
5. Add zero-quantity position-row diagnostics.
6. Add ownership/correlation classification for target orders and trades.
7. Add dirty-start/adoption/manual-intervention disposition.
8. Bridge existing M3f/M3g issue kinds into Stage 4 readiness blockers.
9. Keep `RuntimeBootstrapSnapshotDto` as restored runtime state and validate it
   against broker truth instead of merging the two concepts.
10. Add fixture-backed tests for clean-flat, dirty-start position, target active
    order, unknown/orphan order, orphan trade, stale truth, missing schedule,
    and instrument mismatch.

## Acceptance checklist

- Existing `BrokerTruthSnapshot` inventoried.
- Existing `RuntimeHostBootstrapSnapshot` inventoried.
- Existing `RuntimeBootstrapSnapshotDto` inventoried.
- Existing FINAM mapper inventoried.
- Existing broker-truth issue machinery inventoried.
- Broker-truth parity helpers and historical M4 requirements inventoried.
- Every surface has a decision: `reuse`, `extend`, `wrap_v2`, or
  `out_of_scope_with_reason`.
- No duplicate incompatible `BrokerTruthSnapshot` introduced.
- Stage 4C action list produced.
- Runtime-live remains blocked.
- Real FINAM command consumer remains blocked.
- Real orders remain blocked.
- Stop/SLTP/bracket/replace/multi-leg live behavior remains blocked.
- RI/RTS and USDRUBF migration remain blocked.
- `i64` surrogate adapter remains forbidden without a new ADR.
