# MOEX Trading Gateway RS

Broker-neutral MOEX trading complex: gateway, protocol contracts, runtime bridge, adapters, and reconciliation tools for live micro trading systems.

The first target adapter is Finam Trade API. The project is developed with gateway-first delivery: build FINAM adapter/gateway and broker-protocol v2 first, then adapt the existing runtime minimally instead of rewriting strategies.

## Initial direction

- `broker-core`: normalized contracts for orders, trades, positions, market data, subscriptions, readiness, and command acks.
- `broker-finam`: Finam adapter surface. Read-only, WebSocket market data, broker-truth mapping, and guarded operator one-shot order harnesses exist; continuous runtime-live order placement remains out of scope.
- `finam-gateway`: FINAM shadow gateway boundary for Redis health/readiness, broker-truth snapshots, read-only market data publication, and mock/paper ACK publication. Command consumption to real FINAM and runtime-live endpoints stay disabled.
- `broker-cli`: operator-facing diagnostics for endpoint defaults, auth checks, and redacted read-only probes. Full exports are a later M1 task.

## Milestones

1. M0 — workspace, contracts, docs, serialization tests.
2. M1 — Finam read-only: auth, accounts, positions, orders, trades, historical import.
3. M2 — stream/shadow mode and reconciliation.
4. M3 — gated path to operator one-shot micro live MARKET/LIMIT/CANCEL.
5. M4 — ALOR parity, paper runtime, broker-truth/bootstrap, and later
   stop/SLTP/bracket lifecycle.
6. M5 — strategy/runtime migration for the target MOEX futures systems.

## Safety posture

Continuous runtime-live trading must not be enabled until:

- read-only Finam behavior is characterized;
- FINAM `TimeInForce`, order status, instrument registry, and schedule
  semantics are aligned and tested;
- REST requests use `Authorization: Bearer <jwt>` and do not log raw tokens;
- REST API errors are redacted by default;
- token types do not expose secret/JWT values through debug/display/JSON export;
- broker-truth reconciliation works;
- account/position/order/trade streams are normalized;
- secret handling and logging policy are audited;
- explicit operator approval is recorded for order-emitting mode;
- ALOR runtime compatibility, broker-truth bootstrap, real strategy/riskgate
  attachment, and command-consumer paper/mock parity are accepted.
- stop/SLTP/bracket features are disabled for Phase 1.

Current status is tracked in
[docs/current-status.md](docs/current-status.md). In short: the guarded
operator one-shot FINAM order harness exists, but `command-consumer-to-real-FINAM`,
runtime `LiveReady`, continuous runtime-live, and Stop/SLTP/bracket remain
disabled.

Stage 1B is accepted for IMOEXF `HybridIntradayRuntime` paper/shadow
compatibility freeze. Stage 2A is now the active design/prep step for migrating
the original runtime source to broker-neutral contract v2. The accepted path is
source migration to `BrokerOrderId(String)`; an `i64` surrogate adapter is not
allowed without a separate ADR. See
[docs/stage-2-runtime-source-migration-plan.md](docs/stage-2-runtime-source-migration-plan.md)
and
[docs/stage-2-runtime-source-migration-inventory.md](docs/stage-2-runtime-source-migration-inventory.md).

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
cancel orders. The VPS example starts with `TIME_FRAME_M1`; strategy parity for
the existing 10-minute systems requires FINAM WebSocket stream characterization
plus canonical M1-to-10m aggregation or a separately characterized FINAM-native
10-minute endpoint.
M4-3b adds a FINAM WebSocket market-data shadow. It publishes only live-stream
QUOTES/BARS events under the separate `finam_ws_shadow:*` namespace:

```bash
cargo run -p broker-cli -- finam-ws-shadow-once \
  --config config/finam-ws-shadow.vps.example.json

cargo run -p broker-cli -- finam-ws-shadow-loop \
  --config config/finam-ws-shadow.vps.example.json \
  --max-iterations 3
```

WS shadow remains no-live: no command consumer, no order placement, no cancel,
no runtime/live attachment, and no Stop/SLTP/bracket.
M4-3b-a adds the bounded VPS runtime evidence collector for this no-live WS
shadow:

```bash
python3 scripts/m4_3ba_vps_ws_runtime_evidence.py \
  --ssh-host root@VPS_HOST \
  --output reports/m4/m4-3b-a-vps-ws-runtime-evidence.json
```

The report captures redacted systemd/Redis/one-shot evidence and still does not
authorize runtime-live, order commands, or FINAM cutover.
M4-3c0 adds the broker-neutral observability contract that maps ALOR raw
gateway/runtime streams and FINAM shadow REST/WS streams into the same
canonical channel kinds: gateway health/readiness, broker truth, market data,
command ACK lifecycle, runtime state, and consumer-group ops state. This is the
foundation for an IMOEXF hybrid shadow/dry runtime attachment, not permission
for continuous live trading.
M4-3c1 makes FINAM WebSocket `BARS` the explicit strategy market-data source
for parity. REST bars/quotes remain diagnostic or broker-truth support only;
quotes alone cannot satisfy runtime parity readiness. The WS loop reports
`strategy_market_data_source = FinamWebSocketBarsLiveStream` and degrades
readiness when no bar events arrive.
M4-3c2 adds the ALOR-style market-data lifecycle snapshot for FINAM WS shadow:
history/recovery/live bar counters, first-live-final-bar gate, stale diagnostics,
and explicit `market_data_lifecycle.phase`. This makes FINAM and ALOR comparable
at the data-readiness layer while keeping gateway readiness no-live /
operator-arm blocked.
M4-3c3 adds the FINAM WS closed-bar finalizer. Raw forming `BARS` updates are
counted as diagnostics but are not published as strategy bars; the strategy
market-data stream receives only canonical finalized bars. This preserves the
closed-bar / next-bar-open parity contract before any runtime attachment.
M4-3c4 starts the next source-only parity step: canonical final M1 bars can be
strictly aggregated into final 10-minute buckets for comparison with the
existing ALOR 10m strategy oracle. Fresh online FINAM evidence and ALOR-vs-FINAM
10m comparison still require an active market session.
M4-3c5 adds the broker-neutral FINAM WS reconnect/gap-recovery contract. A
reconnect alone is not enough for market-data readiness: FINAM must replay the
gap window with REST Bars, dedupe overlap, prove contiguity against the previous
final-bar watermark, resubscribe to WS, and observe a first live final bar before
the stream can become live-ready. This remains source-only and does not enable
runtime-live or order endpoints.
M4-3d adds FINAM WS freshness/gap diagnostics to the shadow report. `LiveStream`
remains a transport-source label, while report fields distinguish stale WS
backlog from a fresh final live bar and expose final-bar close timestamp gaps.
This keeps local/VPS runs understandable before the recovery contract is wired
into the loop.
M4-3h wires the accepted warm/cold resync contract into the FINAM WS report as a
source/no-live recovery surface. It exposes final-bar watermark, replay window,
gap-absence blockers, and safety flags with real REST replay wiring disabled
until a controlled GET-only evidence slice is reviewed.
M4-3h-a adds that controlled GET-only REST Bars replay evidence: it reads the
latest Redis final-bar watermark, fetches the warm overlap window from FINAM
REST Bars, proves timestamp contiguity/coverage, and keeps runtime-live and all
order surfaces disabled.
M4-3h-b wires the replay path into the FINAM WS shadow loop itself. Replay bars
are counted as recovery diagnostics with overlap dedupe and
`RecoveryNotStrategyLive` data-quality reasons, while strategy-live publication
still requires fresh final WebSocket bars.
M4-3h-c adds controlled recovery acceptance evidence: the loop starts from an
intentionally older final-bar watermark, replays a bounded warm gap with overlap,
and requires recovery `LiveReady` without enabling runtime-live or order
surfaces.
M4-3o adds a local ARM/Mac Wi-Fi reconnect runbook for the developing FINAM WS
shadow contour. It keeps Redis streams under `finam_ws_shadow_local:*`, expects
safe no-live `Reconciliation + OperatorLiveArmMissing` rather than production
`LiveReady`, and checks reconnect/resubscribe/gap diagnostics after a manual
network break.
M4-3p adds the repeatable post-patch evidence package for that boundary: a
redacted generator consumes local reconnect, SIGTERM, and Ctrl-C stdout logs and
verifies recovery `LiveReady`, empty recovery blockers, gap absence, and stopped
summaries while keeping all live/order surfaces closed.
M4-3i adds a session-aware bar silence watchdog: FINAM schedule data determines
whether final-bar silence is an active-session feed problem or an expected
closed/break/maintenance interval, without enabling runtime-live or order
surfaces.
M4-3i-a hardens the watchdog unknown-schedule path: schedule fetch failure or
unparseable/unknown schedule explicitly blocks readiness instead of remaining a
soft diagnostic.
M4-3j defines the broker-neutral HTTP/debug health surface shape for FINAM:
`/liveness`, `/readiness`, and redacted `/debug/transport`, mapped from the ALOR
`/debug/cws` pattern, without starting an actual HTTP listener or expanding
runtime-live/order boundaries.
M4-3j-a adds the first local-only HTTP/debug listener for that surface:
`finam-local-debug-http` binds to localhost by default, serves GET-only
`/liveness`, `/readiness`, and `/debug/transport`, and keeps runtime-live,
command-consumer-to-real-FINAM, and order POST/DELETE disabled.
M4-3j-b hardens the listener readiness boundary: the normal CLI no longer has a
synthetic `--live-ready` operator flag, synthetic readiness is marked
`not_for_systemd_readiness`, and WS shadow reports now carry a real
`broker_neutral_debug_surface` built from actual WS-loop state.
M4-3k adds the ALOR↔FINAM observability parity report: ALOR `/debug/cws` and
FINAM `/debug/transport` are compared through broker-neutral capability buckets
for routes, readiness semantics, WS generation, subscriptions, data-quality,
recovery, session watchdog, redaction, and no-live/no-order flags.
M4-3k-a hardens readiness HTTP semantics in that parity model: `LiveReady` must
map to HTTP `200`, and every non-`LiveReady` phase must map to HTTP `503`.
M4-3l starts dry runtime attach parity for 10-minute strategies: ALOR native 10m
is treated as the oracle via `BarsGetAndSubscribe(tf=600)`, while FINAM derived M1-to-10m
canonical bars are the accepted FINAM path. raw FINAM M1 bars are rejected by
the strategy-facing timeframe gate; FINAM-native 10m remains a separate
characterization item.
M4-3l-a hardens that gate with explicit sidecar provenance: `timeframe_sec=600`
alone is insufficient. Strategy-facing FINAM 10m bars must be
`FinamDerivedM1ToM10` with `source_timeframe_sec=60`, `target_timeframe_sec=600`,
`aggregation_complete=true`, and `gap_absence_proven=true`; FINAM native M10 is
rejected while characterization is pending.
M4-3m adds active-session bar parity tooling: it reads ALOR native 10m Redis
bars and FINAM `finam_ws_shadow:market_data`, derives FINAM M1-to-M10 buckets,
dedupes exact duplicate FINAM M1 bars, normalizes ALOR active 10m timestamps as
bucket-open timestamps, and reports synchronized bars or explicit pending
reasons such as `MissingAlorOracleStream`. Optional ALOR M1-to-M10 assembly can
be supplied as a native-vs-assembled cross-check. It exports no raw Redis
payload and still keeps runtime-live, real command consumer, and order
endpoints disabled.
M4-3n adds the ALOR-internal timeframe stand evidence: a separate VPS
diagnostic ALOR gateway publishes 1m bars into isolated Redis, while production
native 10m remains the oracle. The report assembles stand M1-to-M10, compares
overlapping OHLCV buckets, and verifies the stand command/ack streams are empty.
M4-3m/M4-3n now also export exact OHLCV/timestamp tolerance policy, compact
max-diff summaries, and candidate strategy-bar provenance fields required before
dry runtime strategy-decision parity.
M4-3e gates FINAM WS strategy bar publication: stale WS final-bar backlog is
still counted and reported, but only fresh final live bars are published as
strategy market-data bars. This prevents stale WebSocket backlog from reaching
downstream runtime consumers as strategy-ready input.
M4-3f adds an ALOR-style FINAM WS data-quality ledger to stdout reports:
`received = emitted + dropped + ignored + pending` for bars and quotes, with
explicit imbalance diagnostics. Suppressed stale WS backlog is now accounted as
dropped rather than silently disappearing.
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

- [Current status](docs/current-status.md)
- [ALOR parity workplan 2026-07-07](docs/alor-parity-workplan-2026-07-07.md)
- [Architecture](docs/architecture.md)
- [ALOR runtime compatibility contract v1](docs/alor-runtime-compat-contract-v1.md)
- [ADR: runtime source migration vs adapter](docs/adr-runtime-compat-adapter-vs-source-migration.md)
- [Stage 1B ALOR runtime compatibility acceptance report](docs/stage-1b-alor-runtime-compat-acceptance-report.md)
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
- [M4-3b FINAM WebSocket stream shadow](docs/m4-3b-finam-websocket-stream-shadow.md)
- [M4-3b-a VPS WebSocket runtime evidence](docs/m4-3b-a-vps-ws-runtime-evidence.md)
- [M4-3c0 broker-neutral observability contract](docs/m4-3c0-broker-neutral-observability-contract.md)
- [M4-3c1 FINAM WS bars stream source](docs/m4-3c1-finam-ws-bars-stream-source.md)
- [M4-3c2 ALOR-style market-data lifecycle](docs/m4-3c2-alor-style-market-data-lifecycle.md)
- [M4-3c3 FINAM WS closed-bar finalizer](docs/m4-3c3-finam-ws-closed-bar-finalizer.md)
- [M4-3c4 fresh-online final bar and M1-to-10m parity](docs/m4-3c4-fresh-online-final-bar-and-m1-to-m10-parity.md)
- [M4-3c5 FINAM WS reconnect/gap-recovery parity](docs/m4-3c5-finam-ws-reconnect-gap-recovery.md)
- [M4-3d FINAM WS freshness and gap diagnostics](docs/m4-3d-finam-ws-freshness-gap-diagnostics.md)
- [M4-3e FINAM WS stale backlog publish gate](docs/m4-3e-finam-ws-stale-backlog-publish-gate.md)
- [M4-3f FINAM WS data-quality ledger](docs/m4-3f-finam-ws-data-quality-ledger.md)
- [M4-3g FINAM WS generation and subscription confirmation](docs/m4-3g-finam-ws-generation-subscription-confirmation.md)
- [M4-3g-a active-session fresh final evidence](docs/m4-3g-a-active-session-fresh-final-evidence.md)
- [M4-3h FINAM WS warm/cold resync contract](docs/m4-3h-finam-ws-warm-cold-resync-contract.md)
- [M4-3h-a REST Bars replay evidence](docs/m4-3h-a-rest-bars-replay-evidence.md)
- [M4-3h-b replay wiring loop evidence](docs/m4-3h-b-replay-wiring-loop-evidence.md)
- [M4-3h-c controlled recovery acceptance evidence](docs/m4-3h-c-controlled-recovery-acceptance-evidence.md)
- [M4-3o local FINAM WS Wi-Fi reconnect runbook](docs/m4-3o-local-finam-ws-wifi-reconnect-runbook.md)
- [M4-3p repeatable FINAM WS reconnect evidence](docs/m4-3p-repeatable-finam-ws-reconnect-evidence.md)
- [M4-3i session-aware bar silence watchdog](docs/m4-3i-session-aware-bar-silence-watchdog.md)
- [M4-3j broker-neutral HTTP/debug health surface](docs/m4-3j-broker-neutral-http-debug-surface.md)
- [M4-3j-a local HTTP/debug listener](docs/m4-3j-a-local-http-debug-listener.md)
- [M4-3j-b synthetic readiness guard](docs/m4-3j-b-synthetic-readiness-guard.md)
- [M4-3k ALOR-FINAM observability parity](docs/m4-3k-alor-finam-observability-parity.md)
- [M4-3k-a readiness HTTP semantics strictness](docs/m4-3k-a-readiness-http-semantics-strictness.md)
- [M4-3l dry runtime attach / M1-M10 parity](docs/m4-3l-dry-runtime-attach-m1-m10-parity.md)
- [M4-3m active-session ALOR-FINAM 10m parity](docs/m4-3m-active-session-alor-finam-10m-parity.md)
- [M4-3n ALOR native vs assembled 10m stand](docs/m4-3n-alor-native-vs-assembled-10m-stand.md)
- [M4-3u ALOR gateway/runtime contract parity notes](docs/m4-3u-alor-gateway-runtime-contract-parity-notes.md)
- [M4-3v broker-neutral runtime host contract](docs/m4-3v-broker-neutral-runtime-host-contract.md)
- [M4-3w review handoff after local runtime parity check](docs/m4-3w-review-handoff-after-local-runtime-parity-check.md)
- [M4-3x seeded ALOR-oracle FINAM paper parity](docs/m4-3x-seeded-alor-oracle-paper-parity.md)
- [M2-to-M3 readiness gate](docs/m2-to-m3-readiness-gate.md)
- [M3 order-path design](docs/m3-order-path-design.md)
- [Order-path retention/archive policy](docs/order-path-retention-archive-policy.md)
- [Redis stream contract](docs/redis-stream-contract.md)
- [Runtime bridge dry contract](docs/runtime-bridge-dry-contract.md)
- [Runtime bridge pending policy](docs/runtime-bridge-pending-policy.md)
- [Security policy](docs/security.md)
- [SQLite order-path store implementation ticket](docs/sqlite-order-path-store-implementation-ticket.md)
