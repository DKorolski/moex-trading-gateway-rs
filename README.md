# MOEX Trading Gateway RS

Broker-neutral MOEX trading complex: gateway, protocol contracts, runtime bridge, adapters, and reconciliation tools for live micro trading systems.

The first target adapter is Finam Trade API. The project is developed with gateway-first delivery: build FINAM adapter/gateway and broker-protocol v2 first, then adapt the existing runtime minimally instead of rewriting strategies.

## Initial direction

- `broker-core`: normalized contracts for orders, trades, positions, market data, subscriptions, readiness, and command acks.
- `broker-finam`: Finam adapter surface. M0 is read-only/stubbed; live order placement is out of scope until contracts and read-only reconciliation are validated.
- `finam-gateway`: FINAM shadow gateway boundary for Redis health/readiness, broker-truth snapshots, and read-only market data publication. Command consumption and live order endpoints stay disabled in M2a.
- `broker-cli`: operator-facing diagnostics for endpoint defaults, auth checks, and redacted read-only probes. Full exports are a later M1 task.

## Milestones

1. M0 — workspace, contracts, docs, serialization tests.
2. M1 — Finam read-only: auth, accounts, positions, orders, trades, historical import.
3. M2 — stream/shadow mode and reconciliation.
4. M3 — micro live MARKET/LIMIT/CANCEL.
5. M4 — stop/SLTP/bracket lifecycle.
6. M5 — strategy migration for the target MOEX futures systems.

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
FINAM_SECRET_TOKEN=... cargo run -p broker-cli -- finam-typed-readonly-check
```

`finam-readonly-check` is diagnostics-only: it does not place, cancel, replace,
or modify orders. Add `--output tmp/finam-readonly-redacted.json` to save the
same redacted records as a fixture for DTO/mapper work. The fixture keeps
bounded JSON shape metadata only, not scalar broker values or dynamic raw keys.
`finam-typed-readonly-check` validates typed DTO decoding and core mappers while
still emitting only redacted counts/flags.

M2a starts the Redis/shadow gateway skeleton only. It may publish health,
readiness, portfolio snapshots, order snapshots, and read-only market data
events, but order command consumption, order placement/cancel, ACK lifecycle,
runtime adaptation, and stop/SLTP/bracket features remain disabled.

M2b adds executable shadow-mode commands without enabling live orders:

```bash
cargo run -p broker-cli -- finam-gateway-shadow-once \
  --config config/finam-gateway-shadow.example.json

cargo run -p broker-cli -- finam-gateway-shadow-loop \
  --config config/finam-gateway-shadow.example.json \
  --max-iterations 3

scripts/redis_shadow_smoke.sh
```

The example config uses synthetic account/symbol placeholders. Put real FINAM
inputs only in local `.env` or an ignored local config file.
The periodic loop remains shadow/read-only: it refreshes health/readiness,
publishes degraded/stopped states, and applies Redis stream retention, but it
does not consume commands or emit broker order actions.
M2d adds shadow hardening only: historical-bar watermark/dedupe, market-data
source kind, typed Redis XREAD smoke, handoff content scanning, and draft active
order startup policy.
M2e keeps the same read-only/shadow boundary while hardening the runtime bridge
contract: broker-native order comments are redacted from neutral snapshots,
typed decode coverage is expanded for all allowed shadow payloads, final loop
summaries include cumulative metrics, and bar-finality/durable-dedupe policy is
documented before any runtime consumer is attached.

CI runs `cargo fmt --all --check`, `cargo test --all`, and
`cargo clippy --workspace --all-targets -- -D warnings`.

See:

- [Architecture](docs/architecture.md)
- [Active orders startup policy draft](docs/active-orders-startup-policy.md)
- [Broker contract](docs/broker-contract.md)
- [Finam API notes](docs/finam-api-notes.md)
- [Finam bar finality golden-test plan](docs/finam-bar-finality-golden-test-plan.md)
- [Finam read-only fixtures](docs/finam-readonly-fixtures.md)
- [Handoff packaging](docs/handoff.md)
- [Migration plan](docs/migration-plan.md)
- [Migration decision](docs/migration-decision-2026-06-27.md)
- [Redis stream contract](docs/redis-stream-contract.md)
- [Security policy](docs/security.md)
