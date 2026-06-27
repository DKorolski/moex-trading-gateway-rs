# MOEX Trading Gateway RS

Broker-neutral MOEX gateway, adapters, and reconciliation tools for live micro trading systems.

The first target adapter is Finam Trade API. The project intentionally starts from clean broker-neutral contracts instead of porting the legacy Alor gateway directly.

## Initial direction

- `broker-core`: normalized contracts for orders, trades, positions, market data, subscriptions, readiness, and command acks.
- `broker-finam`: Finam adapter surface. M0 is read-only/stubbed; live order placement is out of scope until contracts and read-only reconciliation are validated.
- `broker-cli`: operator-facing diagnostics such as version, auth check, positions, orders, trades, and exports.

## Milestones

1. M0 — workspace, contracts, docs, serialization tests.
2. M1 — Finam read-only: auth, accounts, positions, orders, trades, historical import.
3. M2 — stream/shadow mode and reconciliation.
4. M3 — micro live market orders.
5. M4 — limit/stop/bracket lifecycle.
6. M5 — strategy migration for USDRUBF, IMOEXF, and RI.

## Safety posture

No live trading functionality should be enabled until:

- read-only Finam behavior is characterized;
- broker-truth reconciliation works;
- account/position/order/trade streams are normalized;
- secret handling and logging policy are audited;
- explicit operator approval is recorded for order-emitting mode.

See:

- [Architecture](docs/architecture.md)
- [Broker contract](docs/broker-contract.md)
- [Finam API notes](docs/finam-api-notes.md)
- [Migration plan](docs/migration-plan.md)
- [Security policy](docs/security.md)

