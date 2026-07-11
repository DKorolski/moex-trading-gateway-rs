# MOEX Trading Gateway RS

Broker-neutral trading infrastructure for MOEX futures, written in Rust.

The project is migrating an existing ALOR-based trading complex to FINAM while
preserving strategy behavior and operational safety. Broker integrations,
runtime contracts and strategy semantics are kept separate so another broker
adapter can be added without rewriting the strategies.

## Current status

Stages 0–4 are closed: broker-neutral contracts, FINAM read-only and WebSocket
data, canonical M10 strategy input, reconciliation foundations and broker-truth
runtime bootstrap are in place.

Stage 5 is active: the real IMOEXF `HybridIntradayRuntime` semantics are being
migrated from the frozen ALOR source. The BO/MR/high180/riskgate kernel has been
imported behind a paper/no-send boundary; the integrated runtime wrapper is not
attached yet.

This repository is not enabled for continuous live trading.

## Architecture

- `broker-core` — broker-neutral IDs, orders, trades, positions, market data,
  readiness, reconciliation and runtime-host contracts.
- `broker-finam` — FINAM REST/WebSocket client, typed DTOs and canonical
  mappers.
- `finam-gateway` — Redis shadow gateway, health/readiness publication,
  broker-truth snapshots and guarded execution infrastructure.
- `strategy-runtime-core` — broker-neutral Hybrid strategy semantics imported
  from the accepted ALOR source oracle.
- `broker-cli` — read-only probes, diagnostics, evidence tooling and controlled
  operator commands.

The intended flow is:

```text
FINAM market data
  -> canonical broker-neutral events
  -> validated broker truth and runtime bootstrap
  -> strategy semantics
  -> paper/mock lifecycle
  -> gated execution only after later acceptance stages
```

## Safety boundary

The following remain disabled:

- continuous runtime-live;
- strategy-driven FINAM order routing;
- command-consumer-to-real-FINAM;
- FINAM runtime `LiveReady`;
- real Stop/SLTP/bracket/replace/multi-leg execution;
- RI/RTS and USDRUBF runtime migration.

The repository contains a guarded operator one-shot MARKET/LIMIT/CANCEL harness
used for earlier controlled micro checks. Its existence does not authorize
strategy-driven or continuous execution.

Secrets and broker identifiers belong only in local ignored files such as
`.env`. Logs, reports and handoff archives must remain redacted.

## Development

Requirements: a recent Rust toolchain. Redis is needed only for Redis-backed
shadow/runtime smoke tests.

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/forbidden_surface_scan.sh
bash scripts/forbidden_surface_negative_harness.sh
```

Read-only FINAM diagnostics:

```bash
cargo run -p broker-cli -- finam-info
FINAM_SECRET_TOKEN=... cargo run -p broker-cli -- finam-auth-check
FINAM_SECRET_TOKEN=... cargo run -p broker-cli -- finam-typed-readonly-check
```

Example shadow run:

```bash
cargo run -p broker-cli -- finam-gateway-shadow-once \
  --config config/finam-gateway-shadow.example.json
```

Example configs contain synthetic placeholders. Never commit real account IDs,
tokens or raw broker responses.

## Documentation

- [Current status](docs/current-status.md)
- [Stable roadmap](docs/roadmap.md)
- [Architecture](docs/architecture.md)
- [Security policy](docs/security.md)
- [Handoff packaging](docs/handoff.md)
- [ALOR runtime compatibility contract](docs/alor-runtime-compat-contract-v1.md)
- [Stage 5 strategy-semantics plan](docs/stage-5-real-strategy-semantics-plan.md)
- [Stage 5 source/profile hardening](docs/stage-5/5b-1a-correspondence-oracle-profile-hardening.md)
- [Stage 5 structural freeze](docs/stage-5/5b-1b-structural-freeze-hardening.md)
- [Stage 5 wrapper inventory](docs/stage-5/5b-2-integrated-wrapper-semantic-inventory.md)
