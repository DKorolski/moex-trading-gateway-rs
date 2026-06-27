# MOEX Trading Gateway RS

Broker-neutral MOEX trading complex: gateway, protocol contracts, runtime bridge, adapters, and reconciliation tools for live micro trading systems.

The first target adapter is Finam Trade API. The project is developed with gateway-first delivery: build FINAM adapter/gateway and broker-protocol v2 first, then adapt the existing runtime minimally instead of rewriting strategies.

## Initial direction

- `broker-core`: normalized contracts for orders, trades, positions, market data, subscriptions, readiness, and command acks.
- `broker-finam`: Finam adapter surface. M0 is read-only/stubbed; live order placement is out of scope until contracts and read-only reconciliation are validated.
- `broker-cli`: operator-facing diagnostics for endpoint defaults, auth checks, and redacted read-only probes. Full exports are a later M1 task.

## Milestones

1. M0 — workspace, contracts, docs, serialization tests.
2. M1 — Finam read-only: auth, accounts, positions, orders, trades, historical import.
3. M2 — stream/shadow mode and reconciliation.
4. M3 — micro live MARKET/LIMIT/CANCEL.
5. M4 — stop/SLTP/bracket lifecycle.
6. M5 — strategy migration for USDRUBF, IMOEXF, and RI.

## Safety posture

No live trading functionality should be enabled until:

- read-only Finam behavior is characterized;
- REST requests use `Authorization: Bearer <jwt>` and do not log raw tokens;
- REST API errors are redacted by default;
- token types do not expose secret/JWT values through debug/display/JSON export;
- broker-truth reconciliation works;
- account/position/order/trade streams are normalized;
- secret handling and logging policy are audited;
- explicit operator approval is recorded for order-emitting mode.
- stop/SLTP/bracket features are disabled for Phase 1.

Useful local probes:

```bash
cargo run -p broker-cli -- finam-info
FINAM_SECRET_TOKEN=... cargo run -p broker-cli -- finam-auth-check
FINAM_SECRET_TOKEN=... cargo run -p broker-cli -- finam-readonly-check
```

`finam-readonly-check` is diagnostics-only: it does not place, cancel, replace,
or modify orders.

CI runs `cargo fmt --all --check`, `cargo test --all`, and
`cargo clippy --workspace --all-targets -- -D warnings`.

See:

- [Architecture](docs/architecture.md)
- [Broker contract](docs/broker-contract.md)
- [Finam API notes](docs/finam-api-notes.md)
- [Migration plan](docs/migration-plan.md)
- [Migration decision](docs/migration-decision-2026-06-27.md)
- [Security policy](docs/security.md)
