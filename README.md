# MOEX Trading Gateway RS

Broker-neutral trading infrastructure for MOEX futures, written in Rust.

The project is migrating an existing ALOR-based trading complex to FINAM while
preserving strategy behavior and operational safety. Broker integrations,
runtime contracts and strategy semantics are kept separate so another broker
adapter can be added without rewriting the strategies.

## Current status

Stages 0–7 are accepted and closed. They provide broker-neutral contracts,
FINAM market data and broker truth, migrated Hybrid strategy semantics, durable
request identity/restart recovery, and the isolated paper command-consumer
lifecycle.

Stage 8A-0 through Stage 8A-3 are independently accepted. Stage 8A-4 Design R2
is accepted at `cc58c10`; its pure reducer Implementation R4 is accepted and
closed at `4caf07c`. Durable-composition Design R2 is accepted and closed at
`6ddf54e`. Implementation Specification R2 is accepted and closed at
`dd01253`. I1 R1 at `0678354` was not accepted; I1 R2 is accepted and closed at
`113d282`. I2 R1 at `6527619` was not accepted. I2 R2 at `e04edea` closed its
four findings but exposed one final accepted-command cross-binding seam. I2 R3
is accepted and closed at `90f4605`. I3 R1 at `a490bbe`, I3 R2 at `62e5e05`
and I3 R3 at `3aa2670` were not accepted. I3 R4 at `4403068` retained pre-crash
execution objects; I3 R5 at `0d1b14f` closed that gap but required readable
control during recovery issuer reopen. I3 R6 is independently accepted and
closed at `593ff25`: its structurally recovery-only issuer treats
missing/unreadable/stale/stopped control as conservative post-effect evidence
while readable identity conflicts remain hard failures. I4 Design R1 at
`06bb09f` was not accepted. I4 Design R2 at `d1a050a` closed its semantic P1s
but was not accepted because its crate-private terminal authority could not
cross into FINAM. I4 Design R3 is accepted at `81727aa`. I4 Implementation R1
at `1da0a65` and R2 at `6a7f07c` were not accepted. I4 Implementation R3 is
independently accepted and closed at `4a11688`; it reconstructs the private
read-only issuer from terminal/root authority and preserves historical ACK
facts when current readiness is unavailable. Stage 8A-5 is independently
accepted and closed at `bf58b47`; Stage 8A is formally closed. GOV-CI-1B is
accepted and merged at `7bc9fda`. Stage 8B-D R2 is accepted at `f296d0b` and
merged at `50ed538`. Stage 8B-S R1 at `a675a77` retained the architecture but
was not frozen. Corrective Stage 8B-S R2 at `831eec8` closed its substantive
findings but exposed a preflight/build-order contradiction. Stage 8B-S R3 is the
active specification/checker-only slice; it freezes
`I → IT(no effect) → P(exact build) → XE(max one effect)`. It is not implementation,
operator-arm or execution authority.

FINAM POST/DELETE, Redis live consumption, broker dispatch, runtime-live, real
strategy orders and Stage 8B execution authority remain closed.

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
cargo fmt --all --check
cargo test --workspace --all-targets -- --test-threads=1
cargo test --workspace --release --all-targets -- --test-threads=1
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings
# Current authoritative governance gate:
bash scripts/current_tree_ci_gate.sh
# Active Stage 8B-S specification/checker gate:
bash scripts/stage8b_spec_gate.sh
```

Historical stage gates remain in the repository as immutable lineage evidence;
they are not substitutes for the current authoritative gates above.

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
