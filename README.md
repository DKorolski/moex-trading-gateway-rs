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
4. M3 — gated path to micro live MARKET/LIMIT/CANCEL.
   Current next step after the 2026-07-03 engineering audit is M3d-1:
   FINAM contract alignment before any real order endpoint implementation.
5. M4 — stop/SLTP/bracket lifecycle.
6. M5 — strategy migration for the target MOEX futures systems.

## Safety posture

No live trading functionality should be enabled until:

- read-only Finam behavior is characterized;
- FINAM `TimeInForce`, order status, instrument registry, and schedule
  semantics are aligned and tested;
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
FINAM_SECRET_TOKEN=... FINAM_ACCOUNT_ID=... FINAM_SYMBOL=TICKER@MIC cargo run -p broker-cli -- finam-real-readonly-evidence
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
M4-3a starts the dual-broker shadow parity foundation for migration from ALOR
oracle to FINAM standalone operation. On VPS the new contour is allowed only as
FINAM read-only shadow publication, for example:

```bash
cargo run -p broker-cli -- finam-gateway-shadow-loop \
  --config config/finam-gateway-shadow.vps.example.json \
  --account-id "$FINAM_ACCOUNT_ID" \
  --symbol "$FINAM_SYMBOL" \
  --max-iterations 3
```

This publishes FINAM shadow health/readiness/truth/market-data streams under
the `finam_shadow:*` namespace and still does not consume commands or place /
cancel orders.
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
approved-only mock execution simulation for accepted/rejected/timeout outcomes,
accepted-without-broker-id reconciliation policy, dry cancel execution
simulation, recovery-by-client-order-id helper, cancel accepted broker-id
mismatch policy, idempotent recovery, SQLite/WAL durable-store prototype,
SQLite schema/version guard, writer-lock metadata/stale-lock policy, read-only
diagnostic store, transition audit journal, SQLite-backed dry simulator
ordering tests, WAL/SHM runtime-file permission hardening, operator-only
diagnostic API boundary, safe transition audit event names,
store-error-to-operator-disarm mapping, explicit pre-endpoint gate decision,
endpoint gate marker design, synthetic/redacted FINAM response fixtures,
future transport signature with required gate marker, SQLite runtime-directory
deployment inspector, endpoint response integration simulator for accepted /
rejected / timeout / rate-limit / maintenance / decode-error fixtures, local
HTTP endpoint mapper hardening for 401/403/429/500/503/timeout/malformed/empty
id cases, redacted internal endpoint-result boundaries, context-aware local
status policy for place/cancel, post-network decode/map-error ordering tests,
mock classified endpoint transport boundary hardening, deserialize-only
accepted endpoint DTOs, non-serde synthetic endpoint fixtures, cancel
reconciliation follow-up dry scenarios after 404/409/410, dry-only execution
client naming, broker-truth reconciliation source contract/classifier,
redacted truth diagnostics, stale/unknown truth operator disarm policy,
broker-truth source freshness/precedence simulation, conflict disarm policy,
broker-truth fetch orchestration simulator, typed missing/error source
reasons, guarded position-derived truth policy,
read-only broker-truth boundary hardening, checked get-order identity,
config-driven truth policy fingerprints, async-aware read-only truth fetcher
contract, local HTTP truth DTO mappers, identity-strength diagnostics,
read-only local mock transport, refined 4xx truth policy,
account/instrument scope checks, weak client-id fallback policy,
disabled-by-default real-readonly FINAM route gate, GET-only real-readonly
transport, query policy, operator guardrails, mandatory read-only run approval
marker, contract-probe harness, page-full trades incomplete semantics,
redacted SQLite audit, non-serializable token/account preflight marker,
probe-run identity, explicit actual HTTP send counters, request-bound preflight
markers, redacted source-order evidence, preflight freshness/TTL, and per-row
actual HTTP send flags, explicit operator-run clock policy, computed
preflight age, a controlled one-shot real-readonly evidence command, and
self-contained/timed/parsed-count evidence closeout, GetOrder 200 fixture
closeout, M3c pre-order gate policy, M3c order endpoint gate design diagnostics,
M3c self-contained gate evidence report, source-archive content binding,
negative forbidden-surface harness, M3c implementation transition plan, and
M3c implementation-boundary architecture decision, and M3c scanner transition
API shape with gated route-rendering boundary, M3c outcome state/ACK policy,
M3c transport/accepted-result classifier design, M3c request-bound
checkpoint/captured-envelope design, M3c endpoint-attempt journal/status
matrix design, M3c durable attempt journal / FINAM status semantics design,
and M3c evidence-closure / durable journal SQLite schema design,
store invariants, broker-order-id uniqueness, cancel timeout policy, dry
window/backoff rate limiting, FINAM request DTO builders without HTTP send,
workspace-wide source-scan guard tests, and price/reference/notional guard
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
- [M3a-6 execution simulator decisions](docs/m3a6-execution-simulator-decisions.md)
- [M3a-8 reconciliation state matrix](docs/m3a8-reconciliation-state-matrix.md)
- [M3a-9 durable store prototype](docs/m3a9-durable-store-prototype.md)
- [M3a-10 SQLite production hardening](docs/m3a10-sqlite-production-hardening.md)
- [M3a-11 final pre-endpoint gate](docs/m3a11-final-pre-endpoint-gate.md)
- [M3b-0 design / fixture gate](docs/m3b0-design-fixture-gate.md)
- [M3b-1 endpoint response integration simulator](docs/m3b1-endpoint-response-integration-simulator.md)
- [M3b-2 local HTTP endpoint mapper hardening](docs/m3b2-local-http-endpoint-mapper-hardening.md)
- [M3b-3 redacted endpoint result and status policy](docs/m3b3-redacted-endpoint-result-status-policy.md)
- [M3b-4 mock transport boundary hardening](docs/m3b4-mock-transport-boundary-hardening.md)
- [M3b-5 broker-truth reconciliation contract](docs/m3b5-broker-truth-reconciliation-contract.md)
- [M3b-6 broker-truth source semantics](docs/m3b6-broker-truth-source-semantics.md)
- [M3b-7 broker-truth orchestration simulator](docs/m3b7-broker-truth-orchestration-simulator.md)
- [M3b-8 read-only broker-truth boundary](docs/m3b8-readonly-broker-truth-boundary.md)
- [M3b-9 read-only fetcher local HTTP mapper](docs/m3b9-readonly-fetcher-local-http.md)
- [M3b-10 read-only fetcher local mock transport](docs/m3b10-readonly-fetcher-local-mock-transport.md)
- [M3b-11 real-readonly transport gate](docs/m3b11-real-readonly-transport-gate.md)
- [M3b-12 real-readonly broker-truth transport](docs/m3b12-real-readonly-broker-truth-transport.md)
- [M3b-13 real-readonly enablement runbook](docs/m3b13-real-readonly-enable-runbook.md)
- [M3b-14 real-readonly contract probe operator harness](docs/m3b14-real-readonly-contract-probe-operator-harness.md)
- [M3b-15 real-readonly pre-run hardening](docs/m3b15-real-readonly-pre-run-hardening.md)
- [M3b-16 real-readonly contract probe evidence gate](docs/m3b16-real-readonly-contract-probe-evidence-gate.md)
- [M3b-17 real-readonly evidence package hardening](docs/m3b17-real-readonly-evidence-package.md)
- [M3b-18 real-readonly pre-evidence gate](docs/m3b18-real-readonly-pre-evidence-gate.md)
- [M3b-19 real-readonly request-bound evidence gate](docs/m3b19-real-readonly-request-bound-evidence-gate.md)
- [M3b-20 real-readonly pre-run freshness gate](docs/m3b20-real-readonly-pre-run-freshness-gate.md)
- [M3b-21 real-readonly operator clock gate](docs/m3b21-real-readonly-operator-clock-gate.md)
- [M3b-22 controlled real-readonly evidence package](docs/m3b22-real-readonly-evidence-package.md)
- [M3b-23 real-readonly evidence closeout hardening](docs/m3b23-real-readonly-evidence-closeout.md)
- [M3b-24 / M3c-0 pre-order readiness closeout](docs/m3b24-m3c0-pre-order-readiness-closeout.md)
- [M3c-0 / M3c-3 order endpoint gate design](docs/m3c0-order-endpoint-gate-design.md)
- [M3c-4 order endpoint implementation transition plan](docs/m3c4-order-endpoint-implementation-transition-plan.md)
- [M3c-5 implementation boundary architecture decision](docs/m3c5-implementation-boundary-architecture-decision.md)
- [M3c-6 scanner transition API shape](docs/m3c6-scanner-transition-api-shape.md)
- [M3c-7 gated route-rendering boundary](docs/m3c7-gated-route-rendering-boundary.md)
- [M3c-8 non-serializable route boundary](docs/m3c8-nonserializable-route-boundary.md)
- [M3c-9 approved request-parts boundary](docs/m3c9-approved-request-parts-boundary.md)
- [M3c-10 approved parts consumer boundary](docs/m3c10-approved-parts-consumer-boundary.md)
- [M3c-11 future send result boundary](docs/m3c11-future-send-result-boundary.md)
- [M3c-12 outcome state and ACK policy matrix](docs/m3c12-outcome-state-ack-policy-matrix.md)
- [M3c-13 transport category and accepted-result classifier design](docs/m3c13-transport-accepted-classifier-design.md)
- [M3c-14 request-bound checkpoint and captured envelope design](docs/m3c14-request-bound-checkpoint-captured-envelope.md)
- [M3c-15 endpoint attempt journal and HTTP status outcome matrix](docs/m3c15-endpoint-attempt-journal-http-status-matrix.md)
- [M3c-16 durable attempt journal and FINAM status semantics](docs/m3c16-durable-attempt-journal-finam-status-semantics.md)
- [M3c-17 evidence closure and durable journal schema](docs/m3c17-evidence-closure-durable-journal-schema.md)
- [M3c-18 migration runbook and canonical replay fingerprint](docs/m3c18-migration-runbook-canonical-replay-fingerprint.md)
- [M3c-19 implementation-gate readiness and golden vectors](docs/m3c19-implementation-gate-readiness-golden-vectors.md)
- [M3c-20 evidence slot closure package](docs/m3c20-evidence-slot-closure-package.md)
- [M3c-21 release-profile evidence](docs/m3c21-release-profile-evidence.md)
- [M3c-22 route-template recheck evidence](docs/m3c22-route-template-recheck-evidence.md)
- [M3c-23 positive GetOrder waiver package](docs/m3c23-positive-get-order-waiver.md)
- [M3c-24 undocumented 2xx status evidence](docs/m3c24-undocumented-2xx-status-evidence.md)
- [M3c-25 cancel 409/410 status evidence](docs/m3c25-cancel-409-410-status-evidence.md)
- [M3c-26 pre-implementation gate package](docs/m3c26-pre-implementation-gate-package.md)
- [M3d-0 implementation-transition decision](docs/m3d0-implementation-transition-decision.md)
- [M3d operational parity roadmap](docs/m3d-operational-parity-roadmap.md)
- [M3d-1 FINAM contract alignment](docs/m3d1-finam-contract-alignment.md)
- [M4-3a dual-broker shadow parity foundation](docs/m4-3a-dual-broker-shadow-parity.md)
- [M2-to-M3 readiness gate](docs/m2-to-m3-readiness-gate.md)
- [M3 order-path design](docs/m3-order-path-design.md)
- [Order-path retention/archive policy](docs/order-path-retention-archive-policy.md)
- [Redis stream contract](docs/redis-stream-contract.md)
- [Runtime bridge dry contract](docs/runtime-bridge-dry-contract.md)
- [Runtime bridge pending policy](docs/runtime-bridge-pending-policy.md)
- [Security policy](docs/security.md)
- [SQLite order-path store implementation ticket](docs/sqlite-order-path-store-implementation-ticket.md)
