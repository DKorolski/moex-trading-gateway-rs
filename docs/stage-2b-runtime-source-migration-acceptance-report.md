# Stage 2B — runtime source migration acceptance report

Status: implementation acceptance report ready for review.

Date: 2026-07-09.

## 1. Scope

Stage 2B closes the paper/mock/local broker-neutral runtime source migration
contract layer for the accepted architecture decision:

- source migration to broker-neutral contract v2;
- `BrokerOrderId(String)` as the broker-order source of truth;
- `StrategyRequestId` as the runtime pending/ACK identity;
- `ClientOrderId` as broker correlation only;
- legacy ALOR numeric order ids imported as decimal strings;
- no `i64` surrogate adapter without a new ADR.

The scope is a foundation for migrating/attaching the real runtime source. It
is not continuous runtime-live and not real FINAM execution.

## 2. What was actually done

Stage 2B added broker-neutral contracts and tests for:

- runtime-facing id types and legacy numeric import helpers;
- passive runtime order/trade/bootstrap/state/ACK DTO migration;
- runtime state order-map validation and readiness blockers;
- exact `StrategyRequestId` ACK lifecycle policy;
- explicit ambiguous ACK handling for error/duplicate/expired/timeout states;
- runtime caches with owned/observed/orphan attribution;
- `BrokerTradeId` invariants and FINAM mapper fallible trade-id handling;
- broker-neutral `TradeLedger` order/fill correlation;
- `HybridIntradayRuntime` owned-id contract for TP/SL/working-order ids;
- broker-neutral cancel/replace DTO shape, with replace still disabled;
- deterministic request-id stability under account/instrument migration;
- combined paper/mock compatibility pack across state, ACK, caches, ledger,
  command DTOs, request id, hybrid-owned ids, and paper riskgate/oracle seed.

## 3. What was not done

Stage 2B does not prove or include:

- market-data parity;
- broker-truth bootstrap into the runtime lifecycle;
- real strategy invocation;
- real FINAM order execution;
- continuous runtime-live;
- real FINAM command consumer;
- Stop/SLTP/bracket/replace/multi-leg live behavior;
- RI/RTS or USDRUBF migration;
- changes to BO/MR trading logic.

These are future Stage 3+ gates.

## 4. Accepted patch list

| Patch | Status | Closure |
| --- | --- | --- |
| 2B-1 | accepted | Runtime-facing id foundation and legacy numeric import helpers. |
| 2B-1a | accepted | `BrokerOrderId` invariant hardening. |
| 2B-2 | accepted | Passive DTO/state migration. |
| 2B-3 | accepted | Runtime state order-map/bootstrap validation. |
| 2B-4 | accepted | Command ACK / order / trade lifecycle boundary. |
| 2B-4a | accepted | Explicit ACK status policy hardening. |
| 2B-5 | accepted | RuntimeCaches and ownership tracking foundation. |
| 2B-5a | accepted | Explicit ownership attribution hardening. |
| 2B-5b | accepted | `BrokerTradeId` invariant hardening. |
| 2B-5c | accepted | FINAM trade-id fallible mapper. |
| 2B-6 | accepted | Broker-neutral TradeLedger foundation. |
| 2B-6a | accepted | TradeLedger blocker lifecycle and duplicate replay hardening. |
| 2B-7 | accepted | Hybrid runtime owned-id contract layer. |
| 2B-8 | accepted | Command cancel/replace DTO shape migration. |
| 2B-9 | accepted | Deterministic request-id stability. |
| 2B-10 | accepted | Combined paper/mock compatibility test pack. |

## 5. Checks and gates

Required local checks for the Stage 2B closure package:

```bash
cargo fmt
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/forbidden_surface_scan.sh
bash scripts/forbidden_surface_negative_harness.sh
bash scripts/order_endpoint_scanner_transition_spec.sh
python3 -m py_compile scripts/*.py
```

Stage 2B closure requires these gates to remain true:

- no real FINAM endpoint path opened;
- runtime-live still blocked;
- real FINAM command consumer still blocked;
- strategy-driven real FINAM orders still blocked;
- `i64` surrogate adapter still forbidden;
- Stage 2B acceptance notes are accepted;
- Stage 3 entry criteria are explicit and limited to market-data parity.

## 6. Future-gated work

The following remain future-gated:

- Stage 3 — market-data parity to strategy input level;
- Stage 4 — broker-truth bootstrap into runtime;
- Stage 5 — real strategy semantics attachment;
- Stage 6 — durable request/client/broker id chain;
- Stage 7 — runtime command consumer paper/mock;
- Stage 8 — real FINAM execution under command consumer;
- Stage 9+ — reconciliation loop, runtime-live readiness, dual-broker parity,
  first runtime-driven live micro, and Stop/SLTP/bracket.

## 7. Still forbidden

The following are still explicitly forbidden after Stage 2B:

- runtime-live;
- real FINAM command consumer;
- strategy-driven real FINAM orders;
- real FINAM `POST`/`DELETE` from runtime;
- Stop/SLTP/bracket/replace/multi-leg live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `i64` surrogate adapter;
- changing BO/MR trading logic under the name of compatibility migration.

## 8. Why Stage 3 can start

Stage 3 can start because Stage 2B now provides the broker-neutral runtime
contract foundation needed by market-data parity work:

- strategy-facing identity contracts are typed;
- old ALOR state and broker ids can be imported safely;
- request-id/ACK identity semantics are stable;
- order/trade/cache/ledger attribution does not silently adopt account-wide
  broker events;
- paper/mock compatibility has a combined regression pack;
- live order paths remain closed by scanners and policy.

Stage 3 should focus only on market-data parity to strategy input level.

## 9. Why runtime-live and real FINAM command consumer are still not allowed

Stage 2B did not prove the gates required for runtime-live:

- full-session FINAM M10 vs ALOR M10 parity is not accepted yet;
- broker-truth bootstrap is not wired into runtime lifecycle;
- real old ALOR `HybridIntradayRuntime` source is not yet migrated/attached in
  this repo;
- runtime command consumer is not proven in paper/mock ACK mode;
- durable request/client/broker id chain is not yet accepted for runtime use;
- orders/trades/positions reconciliation loop is not ALOR-level mature.

Therefore real FINAM command consumer and runtime-driven live remain blocked.

## 10. Known caveat

The real old ALOR `HybridIntradayRuntime` source has not yet been fully moved
into this new repo. Stage 2B closes the broker-neutral contract layer and
compatibility foundation for that migration, but it is not proof that the real
old runtime source already runs unchanged against FINAM.

This caveat is intentional and should be carried into Stage 3/Stage 4 planning.

## Stage 3 entry criteria

Stage 3 must prove market-data parity to strategy input level:

- only final M10 bars reach the strategy/model input;
- FINAM-derived M10 `close_time_utc` matches the ALOR oracle contract;
- raw M1 bars never reach the strategy runtime as model bars;
- reconnect recovery performs replay/gap closure before first fresh final live
  strategy bar;
- a post-silence gap blocks Entry but does not block Exit/Cancel/Repair.

Stage 3 must remain paper/shadow until its own acceptance report allows moving
to later stages.
