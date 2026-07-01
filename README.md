# MOEX Trading Gateway RS

Broker-neutral MOEX trading complex: gateway, protocol contracts, runtime bridge, adapters, and reconciliation tools for live micro trading systems.

The first target adapter is Finam Trade API. The project is developed with gateway-first delivery: build FINAM adapter/gateway and broker-protocol v2 first, then adapt the existing runtime minimally instead of rewriting strategies.

## Initial direction

- `broker-core`: normalized contracts for orders, trades, positions, market data, subscriptions, readiness, and command acks.
- `broker-finam`: Finam adapter surface. Live order placement is out of scope; current order work is limited to dry request specs gated by broker-core preflight markers.
- `finam-gateway`: FINAM shadow gateway boundary for Redis health/readiness, broker-truth snapshots, read-only market data publication, and mock-only dry ACK publication. Command consumption and live order endpoints stay disabled.
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
FINAM_SECRET_TOKEN=... FINAM_SYMBOL=TICKER@MIC cargo run -p broker-cli -- finam-bar-finality-golden-check
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
scripts/runtime_bridge_dry_smoke.sh
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
M2f adds only a dry runtime-bridge consumer contract: typed decode and
schema/msg-type validation for the allowed shadow streams, consumer-side
historical-bar dedupe, redacted order-snapshot validation, DLQ classification,
and consumer metrics. It still does not run strategies or consume order
commands.
M2g hardens that dry contract: bar dedupe keys include `source_kind` and
finality, DLQ reasons carry safe expected/actual type context, and order
snapshot serialization has contract coverage to keep raw comments absent.
M2h adds the dry Redis runner around that contract: `runtime-bridge-dry-consume`
uses `XREADGROUP` over the broker-neutral shadow streams, publishes safe DLQ
records without raw payloads, reports consumer/pending metrics, Redis-ACKs
processed stream entries, and emits a dry readiness-simulator decision that is
never `LiveReady`. It also adds a read-only `finam-bar-finality-golden-check`
harness for FINAM bar timestamp/finality evidence. It still does not consume
order commands, produce trading ACKs, call strategies, or enable live orders.
M2i keeps the same shadow-only boundary and adds replay-grade validation:
positive Redis dry-runner smoke with synthetic broker-neutral streams, negative
DLQ smoke that verifies raw payloads are not stored, CI coverage for both, and
clear tail/backfill docs for `--group-start-id`.
M2j keeps the boundary dry and adds pending/reconnect hardening: opt-in
`XAUTOCLAIM` via `--claim-stale-ms`, reconnect smoke for delivered-but-unacked
entries, broader Redis-negative contract cases, pending-age/stream-length
metrics, and a generated `handoff-commit.txt` in review archives.
M2k keeps the boundary dry and makes replay safer operationally: cursor/backlog
`XAUTOCLAIM`, multi-pending reconnect smoke, latest/consecutive DLQ summary
metrics, DLQ retention stress smoke, degraded/stopped readiness simulator
coverage, and documented pending ownership plus durable watermark/dedupe
decisions. It also records the first redacted FINAM M1 bar-finality evidence
summary while keeping runtime consumption and `LiveReady` blocked.
M2l is the final pre-runtime dry acceptance pass: it tightens the XAUTOCLAIM
cursor stop condition, locks shadow-loop readiness metrics with a regression
test, clarifies producer-vs-runtime watermark keys, and extends read-only FINAM
M1 evidence around clearing windows.
M2m is the M2-to-M3 gate/design package: it defines the M2 exit checklist, bar
finality policy, durable watermark/id-mapping designs, operator arming model,
command/ACK lifecycle, no-blind-retry policy, and the M3 MARKET/LIMIT/CANCEL
safety test matrix. It still does not enable order endpoints.
M3a starts with non-network order-path foundation only: broker-neutral state
machine, id-mapping store contract, JSON-file durable test backend, outgoing
comment policy, operator arming/disarm safety signals, place/cancel preflight,
approved marker types for dry request builders, exact cancel mapping checks, raw
command-comment rejection, command TTL expiry, dry rate-limit capacity,
synthetic ACK construction/publication with safe reason codes and redacted ids,
store invariants, broker-order-id uniqueness, cancel timeout policy, FINAM
request DTO builders without HTTP send, and price/reference/notional guard
tests. It still does not call FINAM order endpoints or consume live strategy
commands.

CI runs `cargo fmt --all --check`, `cargo test --all`, and
`cargo clippy --workspace --all-targets -- -D warnings`. The Redis CI job runs
both `scripts/redis_shadow_smoke.sh` and `scripts/runtime_bridge_dry_smoke.sh`.

See:

- [Architecture](docs/architecture.md)
- [Active orders startup policy draft](docs/active-orders-startup-policy.md)
- [Broker contract](docs/broker-contract.md)
- [Finam API notes](docs/finam-api-notes.md)
- [Finam bar finality golden-test plan](docs/finam-bar-finality-golden-test-plan.md)
- [Finam bar finality evidence 2026-06-30](docs/finam-bar-finality-evidence-2026-06-30.md)
- [Finam read-only fixtures](docs/finam-readonly-fixtures.md)
- [Handoff packaging](docs/handoff.md)
- [Migration plan](docs/migration-plan.md)
- [Migration decision](docs/migration-decision-2026-06-27.md)
- [M2-to-M3 readiness gate](docs/m2-to-m3-readiness-gate.md)
- [M3 order-path design](docs/m3-order-path-design.md)
- [Redis stream contract](docs/redis-stream-contract.md)
- [Runtime bridge dry contract](docs/runtime-bridge-dry-contract.md)
- [Runtime bridge pending policy](docs/runtime-bridge-pending-policy.md)
- [Security policy](docs/security.md)
