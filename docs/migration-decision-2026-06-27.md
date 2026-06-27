# Migration decision — gateway-first trading complex

Date: 2026-06-27

Decision: the migration is a development of the trading complex toward a broker-neutral architecture. The first delivery is gateway-first, not a small isolated gateway and not a big-bang rewrite.

## Target shape

```text
FINAM API/gRPC/REST/WS
        ↓
finam-client / finam-gateway
        ↓
broker-protocol v2 over Redis Streams
        ↓
strategy-runtime
        ↓
existing strategies with minimal required adaptation
```

Runtime and strategies remain as unchanged as practical. We adapt only the parts needed for the broker-neutral contract: string broker order ids, broker-neutral readiness, snapshots, idempotency, and reconciliation.

## Delivery order

1. Baseline: keep the current ALOR version as legacy reference and verify `cargo fmt`, `clippy`, and tests.
2. Contract first: schema v2 with `StrategyRequestId`, short `ClientOrderId`, `BrokerOrderId`, `BrokerAccountId`, instrument map, and Decimal broker boundary.
3. Gateway first: implement FINAM auth/JWT renewal, accounts, assets, asset params, schedule, bars, orders, and trades.
4. Finam gateway: Redis streams, health/readiness, snapshots, command consumer, ACK, idempotency, and reconciliation.
5. Runtime adaptation: minimal migration to string order ids and broker-neutral health.
6. Dry-run/paper: real FINAM data, no live orders.
7. First live micro: one account, one instrument, MARKET/LIMIT/CANCEL only.
8. Expansion: Stop/SLTP/bracket, IMOEXF, then RI/RTS.

## Phase 1 exclusions

The first live path must not include:

- native broker Stop/SLTP/bracket lifecycle;
- RI/RTS live trading;
- scale-up logic;
- blind retry after ambiguous order timeout;
- restoring ALOR runtime state under FINAM without explicit migration.

## Accepted P0 gates

No FINAM live order-emitting mode until all are true:

- broker protocol schema is version 2;
- broker-facing order ids are strings;
- `ClientOrderId` is FINAM-safe: non-empty, persisted, and at most 20 characters;
- every order command has `StrategyRequestId`, `ClientOrderId`, and durable mapping;
- account, positions, orders, trades, instruments, schedule, and first live bar are loaded before `LiveReady`;
- maintenance window and market schedule block entries;
- ambiguous place-order timeout reconciles by `client_order_id` before any retry;
- unknown order/trade status goes to blocked/DLQ, not panic or silent ignore;
- reconciliation report can join commands, acks, broker orders, broker trades, transactions, and fees where available;
- Stop/SLTP/bracket commands are feature-gated off.
