# Stage 2B patch log

Status: patch-log scaffold; no implementation in this document.

Stage 2B implementation plan is accepted in
[`../stage-2b-runtime-source-migration-implementation-plan.md`](../stage-2b-runtime-source-migration-implementation-plan.md).

All Stage 2B implementation patches must remain paper/mock/local and must keep
these boundaries closed:

- runtime-live;
- real FINAM command consumer;
- strategy-driven real FINAM orders;
- Stop/SLTP/bracket/replace/multi-leg live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `i64` surrogate adapter.

## Patch acceptance-note rule

Each Stage 2B implementation patch should add or update a short acceptance note
under this directory before handoff. The note should include:

- what changed;
- what did not change;
- tests added or preserved;
- unsupported live blockers that remain closed;
- evidence that no real FINAM send path was enabled.

## Next planned patch

`2B-1` is the foundation patch:

- broker-neutral runtime-facing id aliases/types;
- legacy numeric ALOR id -> decimal-string import helpers;
- string broker id preservation tests;
- no strategy behavior changes;
- no real FINAM endpoint calls.

Acceptance note:

[`2b-1-id-types-acceptance.md`](2b-1-id-types-acceptance.md)

`2B-1a` is the BrokerOrderId invariant hardening follow-up:

- remove `BrokerOrderId` from the generic unchecked string-id macro;
- keep empty broker-order ids unconstructible through serde and broker-input
  helpers;
- keep Stage 2B paper/mock/local and live send paths disabled;
- freeze the accepted Stage 0–13 macro-roadmap.

Acceptance note:

[`2b-1a-broker-order-id-hardening.md`](2b-1a-broker-order-id-hardening.md)

`2B-2` is the passive DTO/state migration patch:

- add passive runtime order/trade/bootstrap/state/ACK DTOs with
  broker-neutral ids;
- import old ALOR numeric order ids as decimal-string `BrokerOrderId`;
- keep FINAM/native string ids exact;
- reject empty/null broker ids at serde boundaries;
- reject zero/negative only for legacy numeric ALOR imports; native string ids
  like `"0"` / `"-1"` stay exact unless a later policy validator rejects them;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-2-passive-dto-state-migration.md`](2b-2-passive-dto-state-migration.md)

`2B-3` validates runtime order maps and bootstrap working-order maps:

- require map key == payload `order_id`;
- preserve legacy numeric map-key import as broker-order decimal strings;
- serialize new broker-order keys as exact strings;
- convert missing known order ids into readiness/manual-intervention blockers;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-3-runtime-state-order-map-validation.md`](2b-3-runtime-state-order-map-validation.md)

`2B-4` adds CommandAck / OrderEvent / TradeEvent lifecycle boundary contracts:

- ACK pending clearance is keyed by exact `StrategyRequestId`;
- matching `ClientOrderId` or `BrokerOrderId` cannot clear pending by itself;
- `Submitted`/`Accepted`/`Recovered` ACKs without broker id are explicitly
  marked as pending-broker-id;
- rejected/local-rejected style ACKs may omit broker id when lifecycle allows;
- order events classify active/terminal/unknown lifecycle without changing
  strategy behavior;
- duplicate broker order/trade events are classified idempotent at DTO level;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-4-command-ack-order-trade-lifecycle-boundary.md`](2b-4-command-ack-order-trade-lifecycle-boundary.md)

`2B-4a` hardens explicit ACK status policy:

- `Error` no longer clears pending by default;
- `Duplicate` requires prior known outcome and does not clear pending by
  itself;
- `Expired` clears only with explicit local no-send proof;
- `Timeout` / `UnknownPending` keep pending;
- `Rejected` with matching `StrategyRequestId` may still clear pending;
- `Submitted` / `Accepted` / `Recovered` without broker id still become
  pending-broker-id;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-4a-ack-status-policy-hardening.md`](2b-4a-ack-status-policy-hardening.md)

`2B-5` adds passive RuntimeCaches / ownership tracking primitives:

- cache orders by exact `BrokerOrderId(String)`;
- track owned broker order ids as broker-native strings;
- preserve legacy numeric ALOR order ids as decimal strings on import;
- apply `RuntimeOrderEvent` and `RuntimeTradeEvent` into DTO-level caches;
- keep trade-before-order events pending until the exact broker order id appears;
- expose `tracked_order_ids()` as `Vec<BrokerOrderId>`, not `Vec<i64>`;
- reuse Stage 2B-4a ACK status policy for pending cache helpers;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-5-runtime-caches-ownership-tracking.md`](2b-5-runtime-caches-ownership-tracking.md)

`2B-5a` hardens explicit order ownership attribution:

- `apply_order_event()` treats events as observed/account-wide by default;
- runtime-owned orders require explicit `RuntimeOwned` attribution;
- bootstrap adoption requires explicit `AdoptedFromBootstrap` attribution;
- `UnknownOrOrphan` orders remain observed and produce a blocker;
- `observed_order_ids` and `owned_order_ids` are separate;
- `tracked_order_ids()` returns only owned/adopted ids;
- trades for observed orders are known but not strategy-owned;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-5a-runtime-cache-ownership-attribution.md`](2b-5a-runtime-cache-ownership-attribution.md)

`2B-5b` hardens `BrokerTradeId` before TradeLedger migration:

- `BrokerTradeId` no longer uses the generic unchecked string-id macro;
- `BrokerTradeId::from_broker_native_exact("")` rejects empty ids;
- serde rejects empty trade ids;
- native broker trade ids are preserved exactly;
- `RuntimeTradeEvent` rejects empty `trade_id`;
- trade dedup can only use valid non-empty `BrokerTradeId`;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-5b-broker-trade-id-invariant-hardening.md`](2b-5b-broker-trade-id-invariant-hardening.md)

`2B-5c` closes the production FINAM mapper follow-up for broker trade ids:

- broker-provided `trade.trade_id` maps through fallible
  `BrokerTradeId::from_broker_native_exact(...)`;
- empty FINAM trade ids return controlled `FinamMapperError`, not panic;
- valid FINAM/native trade ids remain exact strings;
- `BrokerTradeId::new(...)` stays limited to trusted/test-created non-empty ids;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-5c-broker-finam-trade-id-fallible-mapping.md`](2b-5c-broker-finam-trade-id-fallible-mapping.md)

`2B-6` migrates the TradeLedger contract to broker-neutral ids:

- `TradeRecord.order_id` and `OrderRecord.order_id` use `BrokerOrderId`;
- trade ids, when present, use `BrokerTradeId`;
- ledger orders are keyed by exact broker-native string ids;
- legacy ALOR numeric order ids import as decimal strings;
- account-wide observed trades are not automatically strategy-attributed;
- trade-before-order remains pending until exact `BrokerOrderId` match;
- unknown/orphan trades produce a blocker/manual-intervention path;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-6-trade-ledger-broker-neutral-migration.md`](2b-6-trade-ledger-broker-neutral-migration.md)

`2B-6a` hardens TradeLedger lifecycle/replay semantics:

- `blockers()` / `active_blockers()` now represent current active blockers;
- `blocker_history()` preserves audit history separately;
- pending exact-match blockers resolve after the exact owned order appears;
- pending blockers turn into observed-order blockers when the exact order is
  account-wide/observed, not strategy-owned;
- duplicate `Some(BrokerTradeId) + BrokerOrderId` fills are idempotent and do
  not double-apply position/PnL;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-6a-trade-ledger-blocker-dedup-hardening.md`](2b-6a-trade-ledger-blocker-dedup-hardening.md)

`2B-7` adds the broker-neutral owned-id contract for the future
`HybridIntradayRuntime` source migration:

- `tp_order_id` uses `BrokerOrderId`;
- `sl_exchange_order_id` uses `BrokerOrderId`;
- `working_orders` uses `HashSet<BrokerOrderId>`;
- legacy numeric ALOR ids import as decimal strings;
- non-numeric broker-native ids are accepted as normal broker ids;
- order/stop/bootstrap/restore helper paths preserve exact ids;
- cancel-protection and partial-entry-timeout helper paths return
  `BrokerOrderId` targets;
- Stop/SLTP/bracket live behavior remains disabled/future-gated.

Acceptance note:

[`2b-7-hybrid-runtime-owned-ids.md`](2b-7-hybrid-runtime-owned-ids.md)

`2B-8` migrates command/cancel/replace DTO shape to broker-neutral ids:

- `CancelOrder.order_id` accepts legacy ALOR numeric ids as decimal strings;
- broker-native cancel order id strings are preserved exactly;
- empty cancel order ids are rejected by serde/import;
- `build_cancel_command(...)` accepts/produces `BrokerOrderId` without numeric
  `order_id > 0` checks;
- `ReplaceOrder.order_id` uses `BrokerOrderId`;
- replace remains disabled/future-gated and is not added to `BrokerCommand`;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-8-command-cancel-replace-dto-shape.md`](2b-8-command-cancel-replace-dto-shape.md)

`2B-9` freezes deterministic request-id stability:

- request-id namespace remains
  `strategy_id|portfolio|symbol|action|bar_ts|seq`;
- `AccountId` alias renders identically to legacy portfolio string;
- `InstrumentId.symbol` renders identically to legacy strategy symbol;
- broker venue symbol is not part of the request-id namespace;
- `ClientOrderId` does not affect `StrategyRequestId`;
- `BrokerOrderId` does not affect `StrategyRequestId`;
- old pending request ids still match new ACK paths by exact request id.

Acceptance note:

[`2b-9-deterministic-request-id-stability.md`](2b-9-deterministic-request-id-stability.md)
