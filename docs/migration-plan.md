# Migration plan

This is a trading-complex migration with gateway-first delivery. We are not
building a permanently isolated gateway, and we are not doing a big-bang rewrite
of the old ALOR complex. The first useful delivery is FINAM adapter/gateway plus
broker-protocol v2, while runtime and strategies are adapted only where the
broker-neutral contract requires it.

## M0 — contracts and docs

- Create clean Rust workspace.
- Define broker-neutral contracts and schema v2.
- Capture Finam API notes.
- Add serialization tests and CLI skeleton.
- Record the ALOR sanitized project as legacy baseline/reference.

Exit criteria:

- `cargo test` passes.
- No live trading code exists.
- Docs identify open questions before adapter work.
- `StrategyRequestId`, `ClientOrderId`, `BrokerOrderId`, `BrokerAccountId`, and instrument mapping types exist.
- `ClientOrderId` cannot exceed FINAM's 20 character limit.

## M1 — Finam read-only

- Secret-to-JWT auth.
- JWT renewal model.
- Token details and account list.
- Account snapshot/positions.
- Current orders.
- Historical trades.
- Transactions.
- Asset params and schedules for RI, IMOEXF, USDRUBF.
- CLI export to JSON/CSV.

Exit criteria:

- We can reproduce broker-truth trade history from Finam.
- We can verify account flatness/readiness without placing orders.
- We can validate symbol, tick, lot, expiration, and schedule before any live mode.
- API maintenance and market schedule are represented in readiness.

## M2 — streaming/shadow

- Own orders/trades stream.
- Market data bars/quotes stream.
- Subscription readiness.
- Reconnect and daily stream-rotation behavior.
- Runtime bridge in shadow mode.
- Broker-protocol v2 Redis streams.
- Snapshots published before readiness.

Exit criteria:

- Stream events reconcile with REST snapshots.
- Reconnect does not create false orphan trades or stale positions.
- Runtime can consume FINAM-normalized events without strategy logic changes.

## M3 — micro MARKET/LIMIT/CANCEL

- Operator-armed order-emitting mode.
- Market and limit order placement with short client order id and comment.
- Cancel command and terminal-state handling.
- ACK lifecycle separate from fill lifecycle.
- USDRUBF-like simple market lifecycle.

Exit criteria:

- One or more micro live cycles complete and reconcile.
- No bracket/stop semantics yet.
- No blind duplicate after ambiguous place-order timeout.

## M4 — stop/bracket research and implementation

- SL/TP order placement/cancel.
- Partial-fill handling.
- MR bracket lifecycle.

Exit criteria:

- IMOEXF MR bracket can complete cleanly in micro.

## M5 — strategy migration

- USDRUBF simple-market system.
- IMOEXF no-overlap hybrid / MR-priority line.
- RI MR with event-risk pause guard.

Exit criteria:

- Broker-truth PnL and runtime owner attribution are reliable.
- Scale-up decision can be made from net PnL including fees.

## Phase 1 P0 blockers

- `order_id: i64` remains in runtime-facing contract.
- `client_order_id` is missing, longer than 20 characters, or not persisted.
- Broker-truth snapshots are optional before live readiness.
- Instrument mapping/schedule is hardcoded or unvalidated.
- Historical/live bar timestamp convention is not proven.
- Unknown broker order/trade status is ignored or panics.
- Stop/SLTP/bracket is enabled before dedicated FINAM contract tests.
- Place-order timeout can retry before reconciliation by `client_order_id`.
