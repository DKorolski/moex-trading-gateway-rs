# Stage 2A runtime source migration inventory

Status: Stage 2A inventory snapshot.

Date: 2026-07-07.

Source inspected:

```text
alor_project/bybit_barter_test_sanitized/alor-rs-main/
```

This inventory is read-only evidence. It identifies ALOR-centered assumptions
that must be migrated before the FINAM contour can attach to the existing
runtime semantics. It does not authorize runtime-live or real FINAM sends.

## Summary

The existing runtime already has a strong `StrategyRequestId`/UUID discipline.
The main migration risk is not request identity; it is broker-native identity
and transport shape:

- orders are still keyed by `i64` in runtime state, snapshots, trade
  correlation, ACK handling, and bootstrap;
- stop-order identity is partly string already, but exchange order ids remain
  numeric in places;
- command DTOs still carry `portfolio`, `exchange`, and `symbol` as raw strings;
- active/terminal order detection uses string status buckets;
- default stream names encode the legacy portfolio-shaped namespace;
- runtime state restore exposes known order ids as `Vec<i64>`;
- the hybrid runtime has many pending/deferred/protective/riskgate fields that
  must be preserved even when their live execution remains disabled.

## Inventory table

| File | Line/function | Old assumption | Risk | Migration action | Test required |
| --- | --- | --- | --- | --- | --- |
| `strategy-runtime/src/strategy_host.rs` | `Intent::Cancel`, lines 33-35 | Cancel target is `order_id: i64`. | FINAM broker ids are strings; lossy surrogate would break cancel/reconciliation. | Replace with `BrokerOrderId` in source migration. Legacy numeric id imports as decimal string. | Cancel intent serde migration and compile-only strategy trait test. |
| `strategy-runtime/src/strategy_host.rs` | `Intent::Replace`, lines 36-40 | Replace target is `order_id: i64`. | Replace is out of scope; accidental migration could enable unsupported path. | Keep replace disabled; migrate type only behind later replace gate. | Forbidden-surface/no-live test. |
| `strategy-runtime/src/strategy_host.rs` | `CreateStopLimit`, lines 41-50 | ALOR-specific stop-limit fields include `instrument_group`. | FINAM Stop/SLTP/bracket semantics are not approved. | Preserve markers only; classify as `future_stop_bracket_only`. | Feature-disabled protective-order fixture. |
| `strategy-runtime/src/strategy_host.rs` | `tracked_order_ids() -> Vec<i64>`, lines 144-146 | Strategy hook returns numeric broker ids. | Runtime restore/adoption cannot represent FINAM ids. | Replace with typed string broker id list or migrate via compatibility shim inside Stage 2B. | Runtime-state-restored fixture with string ids. |
| `strategy-runtime/src/strategy_host.rs` | `StrategyCtx`, lines 199-210 | Context uses raw `portfolio`, `exchange`, `symbol` strings. | Broker-neutral runtime needs account/instrument identity, not ALOR config names. | Introduce typed `RuntimeStrategyContext`/aliases while preserving legacy config names as input. | Instrument/account alias config test. |
| `strategy-runtime/src/strategy_host.rs` | `OrderEvent`, lines 257-280 | `order_id: i64`, string status/side/type. | String broker ids and canonical status cannot be represented. | Map to broker-neutral order event/snapshot with `BrokerOrderId` and canonical lifecycle. | ALOR order fixture -> canonical order snapshot. |
| `strategy-runtime/src/strategy_host.rs` | `TradeEvent`, lines 283-300 | Trade correlates through `order_id: i64`. | Orphan trade detection cannot match FINAM string ids. | Use `BrokerTradeId` and `BrokerOrderId`. | Trade-to-order correlation fixture. |
| `strategy-runtime/src/strategy_host.rs` | `StopOrderEvent`, lines 303-328 | `stop_order_id: String`, `exchange_order_id: Option<i64>`. | Mixed id shapes can silently lose protective-state identity. | Wrap stop id as broker id or future stop id type; exchange id becomes string if preserved. | Protective placeholder migration fixture. |
| `strategy-runtime/src/strategy_host.rs` | `BootstrapSnapshot`, lines 363-367 | Working orders keyed as `HashMap<i64, OrderEvent>`. | Bootstrap truth cannot represent FINAM active order ids. | Key active orders by `BrokerOrderId`; feed from `RuntimeHostBootstrapSnapshot`. | Broker truth bootstrap fixture. |
| `strategy-runtime/src/strategy_host.rs` | `RuntimeStateRestored`, lines 371-373 | Known order ids are `Vec<i64>`. | Stale pending cleanup and adoption lose FINAM order ids. | Change to `Vec<BrokerOrderId>`; count semantics remain. | Old state -> migrated restored state. |
| `strategy-runtime/src/state.rs` | `StrategyState::Placed`, lines 113-119 | `order_id: Option<i64>`. | Legacy paper/live state stores numeric broker id. | Migrate to string broker id field in versioned state. | Legacy placed-state migration fixture. |
| `strategy-runtime/src/state.rs` | `HybridIntradayRuntime`, lines 229-341 | Pending/deferred/protective/riskgate fields are serialized strategy state. | Dropping any field changes strategy behavior or hides dirty-start risk. | Preserve every field; map ids to typed strings; unsupported execution remains blocker. | Existing Stage 1B fixtures plus old->new->restored tests. |
| `strategy-runtime/src/state.rs` | `tp_order_id`, `sl_exchange_order_id`, lines 273-277 | Protective broker/exchange ids can be numeric. | Stop/bracket repair cannot be safely represented for FINAM later. | Preserve as string ids; live stop/bracket remains disabled. | Protective state fixture. |
| `strategy-runtime/src/state.rs` | `CancelSent`, lines 476-479 | Cancel state uses `order_id: i64`. | Cancel resume/recovery cannot handle FINAM ids. | Migrate to `BrokerOrderId`. | CancelSent legacy migration test. |
| `strategy-runtime/src/state.rs` | `RuntimeState.orders`, lines 488-496 | Orders are `HashMap<i64, OrderEvent>`. | Runtime persisted state cannot store string broker ids. | Use `HashMap<BrokerOrderId, ...>` or a serializable string-key map wrapper. | Old runtime state JSON with numeric key -> string-key state. |
| `strategy-runtime/src/runtime.rs` | constants lines 50-62 | Active/terminal order status is stringly typed. | Broker status differences can misclassify active/terminal/unknown. | Move status mapping to broker-neutral `BrokerOrderLifecycle`. Unknown blocks readiness. | Status matrix test. |
| `strategy-runtime/src/runtime.rs` | `OrdersSnapshot`, lines 77-80 | Snapshot orders keyed by `i64`. | Broker truth snapshots from FINAM cannot be represented. | Use `BrokerTruthSnapshot` and broker id string keys. | FINAM/ALOR snapshot canonical parity test. |
| `strategy-runtime/src/runtime.rs` | runtime fields lines 179-195 | `our_order_ids`, `pending_trades_by_order_id`, `pending_exec`, `next_sim_order_id` are numeric. | ACK/trade ownership and simulator can diverge from FINAM string ids. | Convert real broker ids to `BrokerOrderId`; keep simulator ids synthetic and typed distinctly. | ACK ownership and orphan trade tests. |
| `strategy-runtime/src/runtime.rs` | `handle_ack`, lines 1739-1754 | ACK inserts `ack.broker_order_id` numeric into owned ids. | ACK accepted with FINAM string id would not clear/correlate. | Use broker-core `CommandAck` and exact `BrokerOrderId(String)`. | Matching/mismatched request-id tests. |
| `strategy-runtime/src/runtime.rs` | `handle_ack`, lines 1755-1791 | ALOR ACK statuses drive pending behavior. | Stage 2 must preserve semantics while mapping to broker-core ACK statuses/reasons. | Define status mapping table: accepted/submitted/rejected/duplicate/timeout/unknown. | ACK status mapping fixture. |
| `strategy-runtime/src/runtime.rs` | trade handling lines 1928-1967 | Non-positive numeric order id is ignored; orphan detection uses numeric set. | FINAM ids are not numeric; non-empty string validation is needed. | Replace with broker-id non-empty validation and broker-truth recovery. | Orphan trade fixture with string id. |
| `strategy-runtime/src/runtime.rs` | order ledger lines 2201-2255 | Filled order detection uses `status == "filled"` and numeric id. | Different brokers may use different status vocabulary. | Use canonical filled/terminal lifecycle and filled quantity truth. | Filled/partial/terminal status tests. |
| `strategy-runtime/src/runtime.rs` | bootstrap snapshot filtering lines 1230-1355 | Target filtering by symbol string; active orders keyed by numeric id. | Account-wide rows may be mistaken for target truth; FINAM symbol identity can differ. | Filter by `InstrumentId`; account-wide rows diagnostic; target active/unknown blocks. | Target-symbol active order and account-wide diagnostics tests. |
| `strategy-runtime/src/runtime.rs` | `notify_runtime_state_restored`, lines 3132-3166 | Restored known ids copied from numeric `our_order_ids`. | Restored strategy hooks lose FINAM ids. | Expose `Vec<BrokerOrderId>` and pending `StrategyRequestId`. | Restore hook fixture. |
| `strategy-runtime/src/runtime.rs` | `log_bootstrap_dump`, lines 3205-3250 | Diagnostic order dump contains numeric ids and raw comments. | Review/handoff must avoid raw sensitive comments and support string ids. | Redact comments; use typed ids. | Redaction fixture. |
| `strategy-runtime/src/config.rs` | `default_streams`, lines 2285-2297 | Default stream names are derived from legacy portfolio strings. | FINAM contour needs broker-neutral roles and isolated namespaces. | Keep role names stable; configure concrete stream names per contour. | Stream role mapping test. |
| `alor-protocol/src/lib.rs` | `OrderCommand`, lines 66-78 | Command carries `portfolio`, `exchange`, `symbol` raw strings. | Command producer/consumer boundary is ALOR-shaped. | Convert into broker-core `BrokerCommand` with typed account/instrument. | Legacy command decode -> broker command mapper test. |
| `alor-protocol/src/lib.rs` | `CommandAck`, lines 127-143 | Primary `broker_order_id` is `Option<i64>`; string id is auxiliary. | This is the opposite of the accepted ADR for FINAM. | Make string broker id primary; legacy numeric only compatibility input. | ACK with both ids conflict test. |
| `alor-gateway/src/models.rs` | `OrderEvent`/`TradeEvent`, lines 29-55 | Gateway publication emits numeric order ids. | FINAM mapper must not mimic numeric ids. | FINAM emits broker-neutral snapshots; ALOR numeric ids imported as strings in oracle mode. | ALOR fixture -> canonical snapshot parity test. |
| `alor-gateway/src/models.rs` | `OrdersSnapshot`, lines 76-79 | Snapshot map key is `i64`. | FINAM snapshots cannot share this shape without surrogate adapter. | Use broker-neutral snapshot vector or string-key map. | Snapshot migration fixture. |
| `alor-gateway/src/services/command_consumer.rs` | command idempotency lines 94-103 | Idempotency keyed by UUID request id. | This part is good and must be preserved. | Keep `StrategyRequestId` exactness. | Duplicate request id test. |
| `alor-gateway/src/services/command_consumer.rs` | CWS ACK handling lines 528-664 | Broker id is taken from ALOR CWS response and request map uses numeric id. | FINAM ACK may not include broker id immediately or may be string. | Use durable request/client/broker chain and reconciliation-required state. | Accepted-without-broker-id test. |

## Stage 2B implementation order recommended by inventory

1. Introduce broker-neutral runtime event/id aliases and migration helpers.
2. Migrate `CommandAck` and pending-clear path to exact `StrategyRequestId`
   plus `BrokerOrderId(String)`.
3. Migrate persisted `RuntimeState.orders`, `known_order_ids`, and
   `tracked_order_ids`.
4. Migrate bootstrap snapshot and order/trade correlation.
5. Replace string status checks with canonical lifecycle mapping.
6. Add paper/mock command consumer tests.
7. Only then consider implementation review for a constrained IMOEXF
   `HybridIntradayRuntime` subset.

## Explicit blockers

The following remain blockers after Stage 2A:

- real FINAM command consumer;
- FINAM Runtime `LiveReady`;
- strategy-driven real FINAM sends;
- Stop/SLTP/bracket/replace/multi-leg;
- RI/RTS/USDRUBF source migration;
- i64 surrogate adapter without a new ADR.
