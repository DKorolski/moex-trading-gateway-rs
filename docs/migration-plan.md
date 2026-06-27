# Migration plan

## M0 — contracts and docs

- Create clean Rust workspace.
- Define broker-neutral contracts.
- Capture Finam API notes.
- Add serialization tests and CLI skeleton.

Exit criteria:

- `cargo test` passes.
- No live trading code exists.
- Docs identify open questions before adapter work.

## M1 — Finam read-only

- Secret-to-JWT auth.
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

## M2 — streaming/shadow

- Own orders/trades stream.
- Market data bars/quotes stream.
- Subscription readiness.
- Reconnect and daily stream-rotation behavior.
- Runtime bridge in shadow mode.

Exit criteria:

- Stream events reconcile with REST snapshots.
- Reconnect does not create false orphan trades or stale positions.

## M3 — micro market orders

- Operator-armed order-emitting mode.
- Market order placement with client order id and comment.
- Cancel/terminal-state handling.
- USDRUBF-like simple market lifecycle.

Exit criteria:

- One or more micro live cycles complete and reconcile.
- No bracket/stop semantics yet.

## M4 — limit/stop/bracket

- Limit order placement/cancel.
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

