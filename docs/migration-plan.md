# Migration plan

This is a trading-complex migration with gateway-first delivery. We are not
building a permanently isolated gateway, and we are not doing a big-bang rewrite
of the old ALOR complex. The first useful delivery is FINAM adapter/gateway plus
broker-protocol v2, while runtime and strategies are adapted only where the
broker-neutral contract requires it.

## M0 — contracts and docs

- Create clean Rust workspace.
- Define broker-neutral contracts and schema v2.
- Capture Finam API notes.
- Add serialization tests and CLI skeleton.
- Record the ALOR sanitized project as legacy baseline/reference.

Exit criteria:

- `cargo test` passes.
- No live trading code exists.
- Docs identify open questions before adapter work.
- `StrategyRequestId`, `ClientOrderId`, `BrokerOrderId`, `BrokerAccountId`, and instrument mapping types exist.
- `ClientOrderId` cannot exceed FINAM's 20 character limit.

## M1 — Finam read-only

- Secret-to-JWT auth.
- JWT renewal model.
- Token details and account list.
- Account snapshot/positions.
- Current orders.
- Historical trades.
- Transactions.
- Asset params and schedules for RI, IMOEXF, USDRUBF.
- CLI export to JSON/CSV.
- Redacted CLI probe for account/reference/history checks without live order actions.

Exit criteria:

- We can reproduce broker-truth trade history from Finam.
- We can verify account flatness/readiness without placing orders.
- We can validate symbol, tick, lot, expiration, and schedule before any live mode.
- API maintenance and market schedule are represented in readiness.
- Read-only CLI does not print secret/JWT or emit order actions.
- REST Authorization uses `Bearer <jwt>`.
- Secret/JWT structs do not expose raw values through `Debug`.
- REST API error bodies are redacted by default and identified by body shape and
  SHA-256 hash.
- CLI transport errors use redacted presentation by default.
- `AccessToken` is not JSON-serializable; `SecretToken` is redacted.
- REST requests have bounded timeout.
- FINAM API capabilities are split from gateway-enabled features.
- Read-only DTO/mappers exist for token details, account snapshot, orders,
  trades, transactions, assets, schedules, quotes, latest trades, and bars.
- Account position mapping exists and synthetic non-flat snapshots are covered.
- Order snapshot statuses are classified as active, terminal, or
  blocking-unknown before readiness work.
- Broker-native client order ids that cannot fit the FINAM-safe core
  `ClientOrderId` limit keep a redacted fingerprint for reconciliation
  diagnostics.
- JSON decode failures are separated from transport errors.
- Unknown FINAM bar timeframe values are rejected instead of producing
  zero-length bars.
- CI runs fmt/test/clippy.

## M2 — streaming/shadow

- Own orders/trades stream.
- Market data bars/quotes stream.
- Subscription readiness.
- Reconnect and daily stream-rotation behavior.
- Runtime bridge in shadow mode.
- Broker-protocol v2 Redis streams.
- Snapshots published before readiness.

M2a allowed scope:

- `finam-gateway` / broker-gateway skeleton.
- Redis connection boundary and stream sink abstraction.
- Publish health/readiness.
- Publish account, position, and order snapshots from read-only broker truth.
- Read-only reconciliation loop skeleton.
- Market data events from read-only/historical paths.
- Order command consumer absent or `FeatureDisabled`.

M2a explicitly not allowed:

- POST/DELETE order endpoints.
- Live order placement or cancel.
- ACK lifecycle for real orders.
- Runtime adaptation.
- Live micro.
- Stop/SLTP/bracket.

M2b allowed scope:

- `finam-gateway-shadow-once` read-only executable runner.
- `FinamAuthManager` token acquisition.
- FINAM read-only account/orders/quote/bars fetch.
- Redis publication of health, portfolio snapshot, order snapshot, readiness,
  and read-only market data events.
- Optional config file for Redis URL, stream names, account id, symbol, and
  timeframe.
- Redis smoke script/command that publishes and reads back a synthetic envelope.

M2b explicitly not allowed:

- POST/DELETE order endpoints.
- Live order placement or cancel.
- Real order ACK lifecycle.
- Runtime adaptation.
- Live micro.
- Stop/SLTP/bracket.

M2c allowed scope:

- `finam-gateway-shadow-loop` periodic read-only runner.
- Interval config and optional safety `max_iterations`.
- Graceful shutdown with stopped health/readiness publication.
- Degraded health/readiness publication on shadow iteration failure.
- Readiness published after snapshots and read-only market-data publication.
- Redis stream retention/MAXLEN policy.
- Optional Redis CI integration smoke.
- Shadow runner summary metrics.

M2c explicitly not allowed:

- POST/DELETE order endpoints.
- Live order placement or cancel.
- Command stream consumer for real trading.
- Real order ACK lifecycle.
- Runtime adaptation.
- Live micro.
- Stop/SLTP/bracket.

M2d allowed scope:

- Remove portfolio-like literals from tests/docs.
- Handoff archive content scan for live-like account ids, token prefixes, and
  JWT-like strings.
- In-process watermark/dedupe for historical bar publication.
- Market-data `source_kind` in broker-neutral contracts.
- Redis consumer-side smoke with `XREAD` and typed envelope decode.
- Shadow metrics for success/failure timestamps, consecutive failures, and
  published/deduped counts.
- Active-orders startup policy draft.
- Crate/docs update for M2c/M2d.

M2d explicitly not allowed:

- POST/DELETE order endpoints.
- Live order placement or cancel.
- Command stream consumer for real trading.
- Real order ACK lifecycle.
- Durable request/client/broker id store.
- Runtime adaptation.
- `LiveReady` publication.
- Live micro.
- Stop/SLTP/bracket.

M2e allowed scope:

- Public-symbol handoff policy: synthetic values in tests/templates, real
  symbols only in API characterization or migration-planning docs.
- Redaction policy and implementation for broker-native order comments in
  broker-neutral `OrderSnapshot` streams.
- Typed envelope decode tests for all allowed M2 shadow payloads: health,
  readiness, portfolio snapshot, order snapshot, and market data.
- Final shadow-loop summary metrics.
- Bar timestamp/finality golden-test plan before runtime consumption.
- Future durable historical-bar dedupe/watermark strategy.

M2e explicitly not allowed:

- POST/DELETE order endpoints.
- Live order placement or cancel.
- Command stream consumer for real trading.
- Real order ACK lifecycle.
- Durable request/client/broker id store in the order path.
- Runtime adaptation.
- `LiveReady` publication.
- Live micro.
- Stop/SLTP/bracket.

M2f allowed scope:

- Dry broker-neutral stream consumer contract for already-published shadow
  streams.
- Typed decode for health, readiness, portfolio snapshot, order snapshot, and
  market data envelopes.
- `schema_version` and `msg_type` validation before typed payload use.
- Consumer-side historical-bar dedupe/idempotency by
  `(source, source_kind, venue_symbol, timeframe_sec, open_ts, is_final)`.
- Redacted `OrderSnapshot` validation that sends raw-comment violations to DLQ.
- DLQ/dead-letter classification for unknown streams, invalid JSON, schema
  mismatch, message-type mismatch, typed-decode failure, unsupported message
  type, and raw-comment violations.
- Consumer metrics for entries, accepted payloads, duplicate bars, DLQ count,
  and per-payload-kind counts.
- Removal of auto-derived `Debug` for CLI command args.

M2f explicitly not allowed:

- POST/DELETE order endpoints.
- Live order placement or cancel.
- Command stream consumer for real trading.
- Real order ACK lifecycle.
- Durable request/client/broker id store in the order path.
- Strategy runtime adaptation or strategy invocation.
- `LiveReady` publication.
- Live micro.
- Stop/SLTP/bracket.

M2g compact hardening allowed scope:

- Dry consumer contract hardening before Redis consumer runner work.
- Source-kind and finality-aware bar dedupe key:
  `(source, source_kind, venue_symbol, timeframe_sec, open_ts, is_final)`.
- DLQ `TypedDecodeFailed` enriched with expected payload kind, without raw
  payload.
- DLQ `MessageTypeMismatch` enriched with expected and actual known message
  types, without raw payload.
- Contract test that clean `OrderSnapshot` serialization omits raw `comment`
  and empty `comment_fingerprint`.
- Terminology cleanup across M2d/M2e/M2f/M2g docs.

M2h dry Redis runner allowed scope, still without live orders:

- Redis `XREADGROUP` dry consumer runner for broker-neutral streams.
- Consumer metrics for `XREADGROUP` iterations, returned entries, last id,
  pending entries, Redis `XACK` count, missing payloads, and DLQ count.
- DLQ publication stream without raw payload.
- Runtime-readiness simulator/dry runner that consumes health/readiness/
  snapshots/market data but does not run strategies.
- Configurable DLQ stream name and bounded retention.
- Read-only FINAM bar timestamp/finality golden-test harness that emits
  redacted evidence and keeps the acceptance decision manual.
- Consumer-side idempotency/watermark design notes, with implementation still
  in-memory until durability is explicitly approved.

M2h explicitly still not allowed:

- POST/DELETE order endpoints.
- Live order placement or cancel.
- Command stream consumer for real trading.
- Real order ACK lifecycle.
- Durable request/client/broker id store in the order path.
- Strategy runtime adaptation or strategy invocation.
- `LiveReady` publication.
- Live micro.
- Stop/SLTP/bracket.

M2i pre-runtime bridge hardening allowed scope, still without live orders:

- CI Redis integration for `runtime-bridge-dry-consume` with synthetic
  broker-neutral streams.
- Positive dry-runner smoke: accepted counts, Redis `XACK`, DLQ = 0, and
  readiness simulator `DryReady`.
- Negative DLQ smoke: invalid payload goes to safe DLQ, raw payload is absent,
  Redis `XACK` is applied, and readiness simulator becomes `Blocked`.
- Docs/examples for `--group-start-id '$'` tail mode versus
  `--group-start-id 0` backfill/replay mode.
- Operator hint when tail mode returns zero entries.

M2i explicitly still not allowed:

- POST/DELETE order endpoints.
- Live order placement or cancel.
- Command stream consumer for real trading.
- Real order ACK lifecycle.
- Durable request/client/broker id store in the order path.
- Strategy runtime adaptation or strategy invocation.
- `LiveReady` publication.
- Live micro.
- Stop/SLTP/bracket.

M2j dry bridge replay/reconnect hardening allowed scope, still without live
orders:

- Opt-in `XAUTOCLAIM` dry pending recovery through `--claim-stale-ms`.
- Reconnect smoke: create a delivered-but-unacked pending entry, recover it via
  `XAUTOCLAIM`, process it through the same dry consumer/DLQ/XACK path, and
  verify no pending entry remains.
- Redis negative smoke coverage for invalid JSON, message-type mismatch,
  unsupported schema version, missing payload field, typed decode failure, and
  raw `Order.comment` in `OrderSnapshot`.
- Consumer observability additions: claimed-entry count, XAUTOCLAIM iterations,
  deleted-id count, pending oldest idle time, and stream lengths.
- Handoff archive source-commit marker without including `.git/`.

M2j explicitly still not allowed:

- POST/DELETE order endpoints.
- Live order placement or cancel.
- Command stream consumer for real trading.
- Real order ACK lifecycle.
- Durable request/client/broker id store in the order path.
- Strategy runtime adaptation or strategy invocation.
- `LiveReady` publication.
- Live micro.

M2k replay-grade dry validation and operational hardening, still without live
orders:

- Cursor/backlog `XAUTOCLAIM` recovery instead of a single `0-0` claim pass.
- Reconnect smoke with multiple pending entries and a smaller claim batch to
  exercise the backlog cursor.
- Operator-facing DLQ summary: latest reason, timestamp, stream, entry id, and
  consecutive DLQ count.
- DLQ retention stress smoke with exact runtime-bridge DLQ `MAXLEN` trimming.
- Redacted real FINAM M1 bar-finality evidence on several windows, documenting
  open-timestamp support and end-time boundary caveat.
- Pending ownership policy, safe `claim_stale_ms` guidance, repeated-DLQ stop
  rules, and durable watermark/dedupe decision docs.
- Readiness simulator coverage for degraded and stopped health/readiness states.

M2k explicitly still not allowed:

- POST/DELETE order endpoints.
- Live order placement or cancel.
- Command stream consumer for real trading.
- Real order ACK lifecycle.
- Durable request/client/broker id store in the order path.
- Strategy runtime adaptation or strategy invocation.
- `LiveReady` publication.
- Live micro.
- Stop/SLTP/bracket.

M2l final pre-runtime dry bridge acceptance hardening, still without live
orders:

- XAUTOCLAIM cursor-loop stops only on Redis terminal cursor `0-0` or
  non-advancing cursor; an empty page with an advancing cursor remains part of
  the backlog scan.
- Shadow-loop success metrics are regression-tested so one successful
  iteration increments readiness publication count once.
- Producer-side historical watermark is documented as the current low-noise
  heuristic key, while the runtime/durable key remains
  `(source, source_kind, venue_symbol, timeframe_sec, open_ts, is_final)`.
- Redacted FINAM M1 bar-finality evidence is extended around intraday/evening
  clearing windows. The checked windows returned continuous M1 timestamps and
  inclusive `end_time`, so runtime finality must use broker-provided schedule,
  actual bar availability, and receive/probe time rather than generic schedule
  assumptions alone.

M2l explicitly still not allowed:

- POST/DELETE order endpoints.
- Live order placement or cancel.
- Command stream consumer for real trading.
- Real order ACK lifecycle.
- Durable request/client/broker id store in the order path.
- Strategy runtime adaptation or strategy invocation.
- `LiveReady` publication.
- Live micro.
- Stop/SLTP/bracket.

Exit criteria:

- Stream events reconcile with REST snapshots.
- Reconnect does not create false orphan trades or stale positions.
- Runtime can consume FINAM-normalized events without strategy logic changes.

M2m M2-to-M3 readiness gate and order-path design, still without live orders:

- M2 exit checklist for reproducible checks and clean handoff.
- Conservative bar-finality policy: `open_ts`, derived `close_ts`, inclusive
  historical `end_time`, receive/probe-time guard, broker schedule, and actual
  bar availability.
- Runtime/durable watermark key design:
  `(source, source_kind, venue_symbol, timeframe_sec, open_ts, is_final)`.
- Operator arming model with account/instrument/qty allowlists, endpoint arm,
  visible preflight, and automatic disarm triggers.
- MARKET/LIMIT/CANCEL command stream contract using broker-protocol v2
  envelopes.
- ACK lifecycle design separated from order/trade/fill lifecycle.
- Durable `StrategyRequestId -> ClientOrderId -> BrokerOrderId` mapping store
  design.
- No-blind-retry policy for ambiguous order placement timeouts.
- Rate-limit, retry, and backoff policy.
- Order-path fixture plan and market/limit/cancel safety test matrix.

M2m explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer.
- Real CommandAck lifecycle.
- Durable id mapping implementation in the live order path.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M2m acceptance is the design gate for starting M3 implementation. It does not
itself authorize live orders.

## M3 — micro MARKET/LIMIT/CANCEL

M3a-1 non-network order-path foundation:

- Broker-neutral `broker-core::order_path` module.
- Order-path state machine for intent recording, submit in-flight, submitted,
  timeout/unknown-pending, recovery by client order id, cancel request,
  terminal, local/broker rejection, and manual intervention.
- In-memory store specification enforcing duplicate `StrategyRequestId` and
  duplicate `ClientOrderId` rejection. This is a test/spec implementation, not
  the final durable backend.
- Restart recovery behavior: `IntentRecorded` remains not submitted, while
  `SubmitInFlight` recovers as `TimeoutUnknownPending` before any retry.
- Outgoing comment policy: default disabled; optional sanitized deterministic
  comment serializes only a fingerprint and rejects unsafe/too-long values.
- Operator arm TTL/one-shot model with audit fields.
- MARKET/LIMIT place-order preflight for account/symbol allowlist, order type,
  TIF, quantity bounds/step, market quantity guard, and limit price tick. First
  micro policy rejects invalid broker-boundary values; it does not silently
  round.

M3a-1 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer.
- Real CommandAck lifecycle against FINAM endpoints.
- Durable backend implementation for the live order path.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3a-2 dry order-path hardening:

- `OrderPathStore` trait and JSON-file local durable backend for restart/replay
  tests.
- Intent/state persistence before any future network submission step.
- Duplicate request/client id protection after store reopen.
- Non-network cancel preflight for active, terminal, missing mapping, missing
  broker id, arm/audit, and account-scope cases.
- MARKET/LIMIT risk guards for fresh reference price, loaded price step,
  notional per order/run, and optional limit-reference deviation band.
- Synthetic `CommandAck` construction for dry order-path tests only; ACKs
  remain separate from fills/trades/reconciliation.

M3a-2 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer.
- Real CommandAck publication against FINAM endpoints or Redis command streams.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3a-3 endpoint-adjacent dry hardening:

- Cancel preflight requires exact `BrokerOrderId` match against the existing
  order-path mapping before submit-ready or already-terminal classification.
- Missing broker mapping and mismatched active/terminal mappings are rejected.
- Raw `PlaceOrder.comment` is rejected at preflight; outgoing comments remain
  internally generated/fingerprint-only through policy.
- Store update invariants prevent client id change, known broker id
  change/clear, terminal state overwrite, and timestamp regression.
- Tick/step edge tests cover decimal scales common to MOEX futures.
- Limit reference-band tests cover exact bps boundary, over-boundary, and
  invalid reference price.

M3a-3 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer.
- Real CommandAck publication against FINAM endpoints or Redis command streams.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3a-4 dry request/ACK builder hardening:

- `BrokerOrderId` uniqueness enforced as a secondary store index.
- Duplicate broker ids reject on insert/update and JSON-file reopen.
- Cancel state machine supports recovered active orders, cancel timeout,
  broker-truth terminal recovery, and manual intervention after ambiguous
  cancel timeout.
- Cancel blind retry is blocked from `CancelTimeoutUnknownPending`.
- Cancel preflight rejects already-pending cancel and unknown/manual states.
- `CommandAck.reason` is structured as safe reason code instead of arbitrary
  string text.
- `broker-finam::order_request` builds FINAM MARKET/LIMIT place-order JSON
  bodies and CANCEL path specs without any HTTP send method.

M3a-4 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer.
- Real CommandAck publication against FINAM endpoints or Redis command streams.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3a-5 dry approved-path / mock ACK publisher:

- FINAM dry request builders now require `PreflightApprovedPlaceOrder` /
  `PreflightApprovedCancelOrder` marker types returned only by successful
  preflight.
- Preflight now rejects expired place/cancel commands by command TTL before any
  dry request spec can be built.
- Redacted path/body diagnostics are available for dry request specs; request
  `Debug` output avoids raw account id, broker order id, symbol, quantity,
  price, client order id, and outgoing comment values.
- A mock FINAM dry order client records only redacted request diagnostics and
  has no network implementation.
- `finam-gateway` can publish synthetic dry `CommandAck` envelopes to a bounded
  ACK stream only when command consumer, order placement, cancel, and
  stop/SLTP/bracket features are disabled.
- Redis dry ACK publication clears optional client/broker order ids before
  publishing; correlation remains through `StrategyRequestId` and the durable
  order-path store.
- Dry integration tests cover command -> preflight -> local store -> request
  spec -> mock client diagnostic -> synthetic ACK envelope.
- Operator-arm disarm signals cover degraded gateway, runtime-bridge DLQ,
  unknown-pending order, and restart-recovery safety cases.
- Dry order rate-limit capacity is represented and tested locally.

M3a-5 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3a-6 mock network boundary / execution simulator:

- `broker-finam` defines an approved-only execution client trait. The mock
  implementation accepts only FINAM request specs, never raw `PlaceOrder` /
  `CancelOrder` commands.
- Mock execution outcomes cover accepted, rejected, and timeout paths while
  storing only redacted diagnostics.
- `finam-gateway` adds a dry place-order execution simulator:
  preflight-approved command -> request spec -> persisted `BeginSubmit` ->
  mock execution -> state-machine transition -> synthetic ACK.
- Accepted place execution moves to `Submitted`; rejected execution moves to
  `BrokerRejected`; timeout moves to `TimeoutUnknownPending`.
- No-blind-retry is tested: a second submit attempt from
  `TimeoutUnknownPending` fails before the mock client is called.
- Operator safety workflow now includes explicit re-arm after operator-visible
  disarm.
- Dry rate-limit policy now includes a window reset and backoff state.
- ACK and durable-store decisions are documented in
  `docs/m3a6-execution-simulator-decisions.md`: runtime ACKs remain redacted,
  and SQLite/WAL is the selected production-store direction for first real
  endpoint path.

M3a-6 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3a-7 dry cancel simulator / accepted-without-broker-id policy:

- Accepted place execution without a broker order id is no longer treated as
  normal `Submitted`. It moves to `SubmittedPendingBrokerOrderId`, returns an
  `UnknownPending` ACK with `ReconciliationRequired`, and blocks cancel until
  broker truth recovers the broker order id by client order id.
- Operator disarm signals now include accepted-without-broker-id and
  cancel-timeout-unknown-pending safety cases.
- `finam-gateway` adds a dry cancel execution simulator:
  preflight-approved cancel -> request spec -> persisted `RequestCancel` ->
  mock execution -> state-machine transition -> synthetic ACK.
- Accepted dry cancel execution moves to `CancelSubmitted`; rejected cancel
  moves to `ManualInterventionRequired`; timeout moves to
  `CancelTimeoutUnknownPending`.
- No-blind-retry is tested for cancel timeout: a second cancel attempt from
  `CancelTimeoutUnknownPending` fails before the mock client is called.
- Already-terminal cancel preflight is tested as a no-endpoint/no-mock-call
  recovery path.
- Dry cancel ACK publication remains redacted.
- The approved-only execution boundary is locked by a compile-level contract
  test: future clients accept FINAM request specs, not raw commands.
- `docs/sqlite-order-path-store-implementation-ticket.md` records the SQLite /
  WAL single-writer implementation ticket for the first production durable
  order-path backend.

M3a-7 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3a-8 reconciliation-ready dry order path / SQLite planning:

- `broker-core` adds a dry broker-truth recovery helper that resolves
  `client_order_id -> broker_order_id`, sets `broker_order_id` once, transitions
  `SubmittedPendingBrokerOrderId` / `TimeoutUnknownPending` to
  `RecoveredByClientOrderId`, and then allows normal cancel preflight.
- Recovery rejects duplicate broker ids and non-recoverable states without
  overwriting the durable order-path record.
- Operator disarm signals now include cancel broker-order-id mismatch and stale
  reconciliation safety cases.
- Dry cancel accepted response policy is explicit: a missing returned order id
  is allowed, a matching returned order id is allowed, and a mismatched returned
  order id moves to `ManualInterventionRequired` with an `UnknownPending` /
  `ManualInterventionRequired` ACK.
- Source-scan coverage asserts that future network boundaries do not introduce
  raw `place(order: PlaceOrder)` / `cancel(cancel: CancelOrder)` style APIs or
  direct DELETE calls in the order crates.
- `docs/m3a8-reconciliation-state-matrix.md` records the ACK/reconciliation
  matrix for dry order-path review.

M3a-8 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3a-9 reconciliation idempotency / SQLite-WAL durable store prototype:

- Client-id recovery is idempotent: repeated broker-truth facts with the same
  `client_order_id` and same `broker_order_id` return the existing recovered
  record without raising an operator-visible error.
- Client-id recovery with a different broker id for the same client id returns
  a mismatch error, while duplicate broker ids mapped to another request remain
  rejected by the store.
- `broker-core` adds `SqliteOrderPathStore` as a dry prototype backend with
  WAL, `synchronous=FULL`, `BEGIN IMMEDIATE` transactions, unique
  request/client/broker ids, sidecar single-writer lock, crash/reopen tests, and
  redacted export tests.
- SQLite prototype tests cover `SubmitInFlight`, `CancelRequested`, and
  `SubmittedPendingBrokerOrderId` reopening behavior, corrupt database open
  failure, and second writer rejection.
- Approved-only source-scan coverage now walks the whole `crates/` Rust source
  tree instead of only current order-adjacent files.
- `docs/m3a9-durable-store-prototype.md` records the prototype boundaries and
  what remains before real endpoint use.

M3a-9 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3a-10 SQLite production-hardening / dry command-to-store integration:

- SQLite writer-lock files include safe metadata: instance id, pid, created
  timestamp, and schema version.
- Stale/unknown writer locks are not auto-removed; lock uncertainty remains an
  operator-controlled recovery path and order-endpoint disarm signal.
- Lock files are cleaned up if they were created but SQLite connection open
  fails before a store instance exists.
- SQLite startup checks `order_path_schema.schema_version`; unknown/newer
  versions block writer startup and map to migration-mismatch safety handling.
- `SqliteOrderPathReadStore::open_readonly` supports diagnostic reads while a
  writer is open and cannot write.
- SQLite writes append transition-audit rows in the same transaction as record
  inserts/updates.
- Operator disarm signals now include store lock uncertainty, migration
  mismatch, and store unavailability.
- `finam-gateway` has SQLite-backed dry place/cancel simulator tests proving
  `SubmitInFlight` / `CancelRequested` are durable before the mock execution
  client is called.
- Dry Redis ACK publication remains redacted even when the backing store holds
  raw local ids for protected reconciliation.
- Retention/archive policy for terminal order-path records is documented in
  `docs/order-path-retention-archive-policy.md`.
- M3a-10 boundaries are documented in
  `docs/m3a10-sqlite-production-hardening.md`.

M3a-10 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3a-11 final pre-endpoint order-path gate:

- SQLite runtime-file hardening now covers the main DB, WAL, SHM, and writer
  lock files when present.
- Deployment policy requires `umask 077` and a protected local runtime
  directory for any future live-capable process.
- `SqliteOrderPathReadStore` raw lookup methods are explicitly operator-only
  via `operator_*` names; reporting/review exports use `redacted_records()`.
- Transition audit rows now record safe inferred event names such as
  `BeginSubmit`, `SubmitAccepted`, `RequestCancel`, `CancelTimedOut`, and
  `RequireManualIntervention` instead of only generic `UpdateRecord`.
- `OrderPathStoreError::operator_disarm_signal()` maps lock uncertainty,
  migration mismatch, and other store failures to operator disarm signals.
- `GatewayFeatureSet::real_order_endpoint_gate_decision()` and
  `FinamGateway::real_order_endpoint_gate_decision()` explicitly keep real
  endpoint calls blocked with `M3a11PreEndpointReviewRequired`.
- Runtime-facing ACK id policy is locked as `RedactedRuntimeAckOnly`; raw
  client/broker ids remain local to protected operator/internal diagnostics.
- SQLite migration/backup runbook and pre-endpoint FINAM response fixture plan
  are documented in `docs/m3a11-final-pre-endpoint-gate.md`.

M3a-11 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-0 design / fixture gate:

- `EndpointGateApproved` marker design is added; the marker has no public
  constructor and cannot be obtained while `M3a11PreEndpointReviewRequired`
  blocks the current decision. M3b-0 also keeps post-review endpoint approval
  false, so a manually constructed allow-looking decision cannot forge the
  marker.
- Future real endpoint transport trait signatures require
  `&EndpointGateApproved` plus FINAM request specs; there is still no HTTP
  implementation.
- Synthetic/redacted FINAM order endpoint fixture DTOs cover accepted with id,
  accepted without id, rejected, timeout, rate-limited, maintenance, and decode
  error response classes.
- Fixture mapping tests classify future endpoint outcomes without leaking raw
  broker ids through debug/diagnostic output.
- SQLite runtime-directory deployment inspector can flag missing/not-directory,
  group/world-accessible Unix permissions, workspace-tree paths, and
  workspace-artifact paths.
- Transition audit event names are locked with a table-driven contract matrix.
- Operator raw diagnostics remain design-only for future explicit operator
  mode; runtime logs/Redis/review exports stay redacted.
- Boundaries are documented in `docs/m3b0-design-fixture-gate.md`.

M3b-0 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-1 endpoint response integration simulator:

- `simulate_place_order_endpoint_fixture()` and
  `simulate_cancel_order_endpoint_fixture()` route synthetic/redacted FINAM
  endpoint fixtures through the order-path state machine.
- Execution-like fixtures map to existing dry semantics:
  accepted/rejected/timeout for place and cancel, including accepted without
  broker order id and cancel returned-id mismatch handling.
- Non-execution fixtures are conservative:
  rate-limit/maintenance/decode-error persist `BeginSubmit` or `RequestCancel`,
  then `RequireManualIntervention`.
- New broker-neutral ACK reason codes cover `RateLimited`, `BrokerMaintenance`,
  and `ResponseDecodeError`.
- New operator disarm signals cover order endpoint rate-limit, maintenance, and
  decode-error responses.
- Rate-limit integration preserves `retry_after_ms` in the redacted report for
  future backoff wiring.
- No-blind-retry is tested after rate-limit because the record remains in
  `ManualInterventionRequired` before any future transport could be called.
- SQLite-backed audit proves `InsertIntent -> BeginSubmit ->
  RequireManualIntervention` with safe reason code `RateLimited`.
- Runtime-facing ACK publication remains redacted through
  `publish_dry_command_ack()`.
- `EndpointGateApproved` remains unconstructible and real HTTP transport is
  still absent.
- Details are documented in
  `docs/m3b1-endpoint-response-integration-simulator.md`.

M3b-1 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-2 local HTTP endpoint mapper hardening:

- `broker-finam` adds `FinamOrderEndpointLocalHttpResponse`,
  `FinamOrderEndpointClassifiedResponse`, and
  `classify_order_endpoint_local_http_response()` for local/mock HTTP-shaped
  order endpoint responses.
- The classifier maps 2xx success bodies, 400-class broker rejection, 401/403
  unauthorized, 429 rate-limit, 500/503 maintenance, timeout, malformed JSON,
  and empty broker-order-id cases without using a real broker URL.
- Local response `Debug` output redacts raw response bodies and broker ids.
- `finam-gateway` adds local HTTP integration helpers that persist
  `BeginSubmit` or `RequestCancel` before classifying response/decode/map
  outcomes.
- Empty accepted `broker_order_id` and malformed JSON now reach
  `ResponseDecodeError -> ManualInterventionRequired` after durable attempt
  recording, not as an early mapper error.
- 401/403 unauthorized responses map to safe ACK/error/disarm categories:
  `Unauthorized`, `OrderPathErrorKind::Unauthorized`, and
  `OrderEndpointUnauthorized`.
- Redis ACK publication remains redacted for successful accepted responses,
  decode errors, and unauthorized responses.
- `EndpointGateApproved` remains unconstructible and real HTTP transport is
  still absent.
- Details are documented in
  `docs/m3b2-local-http-endpoint-mapper-hardening.md`.

M3b-2 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-3 redacted endpoint result boundary and status policy:

- Internal endpoint result types no longer serve as serde export objects:
  `FinamOrderExecutionOutcome`, `FinamOrderEndpointMappedResult`, and
  `FinamOrderEndpointClassifiedResponse`.
- Those internal types now have custom redacted `Debug`; accepted broker order
  ids are shown only as presence/length.
- `FinamOrderEndpointResponseDiagnostic` remains the safe export/reporting
  boundary.
- `FinamOrderEndpointContext` and
  `classify_order_endpoint_local_http_response_for_context()` add
  place/cancel-specific local status policy.
- Shared policy covers body-read failure, malformed JSON, 401/403, 408/504,
  429, and 500/502/503.
- Place generic 4xx responses stay broker-rejected in the dry classifier.
- Cancel 404/409/410 map to `ReconciliationRequired`, not ordinary broker
  rejection.
- Gateway integration maps cancel reconciliation-required responses to
  `ManualInterventionRequired` with `UnknownPending` ACK and
  `UnknownPendingOrder` disarm.
- Timeout/ambiguous local responses disarm operator arm as `UnknownPendingOrder`
  when an arm is supplied.
- Operator disarm matrix covers unauthorized, rate-limit, maintenance,
  decode/body-read failure, and timeout/ambiguous outcomes.
- Details are documented in
  `docs/m3b3-redacted-endpoint-result-status-policy.md`.

M3b-3 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-4 mock transport boundary and endpoint export hardening:

- `FinamOrderEndpointAcceptedDto` is deserialize-only; it is not a
  report/log/handoff export boundary.
- `FinamOrderEndpointFixture` is a synthetic, non-serde fixture; accepted
  fixtures may carry raw broker ids only inside dry tests/local mapping.
- `FinamOrderEndpointResponseDiagnostic` remains the only endpoint result
  export/reporting boundary.
- `FinamMockClassifiedEndpointTransport` returns only
  `FinamOrderEndpointClassifiedResponse`; raw local HTTP bodies and accepted
  DTOs do not cross the transport boundary.
- Future real endpoint transport compile contract also returns classified
  responses, while `EndpointGateApproved` remains unconstructible.
- SQLite-backed tests prove `BeginSubmit` / `RequestCancel` is durable before
  mock classified transport is called.
- Dry cancel reconciliation follow-up after 404/409/410 covers terminal
  broker truth, still-working broker truth, and unknown broker truth.
- Details are documented in
  `docs/m3b4-mock-transport-boundary-hardening.md`.

M3b-4 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-5 broker-truth reconciliation source contract:

- The older dry execution simulator trait is explicitly renamed to
  `FinamDryApprovedOrderExecutionClient`.
- Future production FINAM transport remains classified-response based:
  `EndpointGateApproved + request spec -> FinamOrderEndpointClassifiedResponse`.
- Broker-truth source classes are defined for `OrdersSnapshot`, `GetOrder`,
  `TradesSnapshot`, and `PositionSnapshot`.
- `CancelBrokerTruthObservation` is non-serde and redacted; the export boundary
  is `CancelBrokerTruthDiagnostic`.
- Fresh terminal statuses map to `Terminal`; fresh active statuses map to
  `StillWorking`; missing or unknown statuses map to `Unknown`.
- Stale truth maps to `Unknown` with `ReconciliationStale` operator disarm.
- Fresh unknown truth maps to `UnknownPendingOrder` operator disarm.
- Cancel follow-up can now run from broker-truth classification and persists
  the same guarded state matrix as M3b-4.
- Production source-scan tests guard the dry-only trait name and classified-only
  future transport boundary.
- Details are documented in
  `docs/m3b5-broker-truth-reconciliation-contract.md`.

M3b-5 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-6 broker-truth source semantics / precedence simulator:

- Dry builders now cover future `GetOrder`, `OrdersSnapshot`,
  `TradesSnapshot`, and `PositionSnapshot` broker-truth inputs.
- Source freshness is policy-driven with per-source `max_age_ms`; stale source
  evidence cannot win precedence.
- Missing get-order/snapshot evidence is represented explicitly as unknown with
  redacted diagnostics.
- Multi-source reconciliation selects the highest-precedence fresh known truth
  when sources agree.
- Fresh terminal-vs-still-working disagreement becomes an explicit
  `ReconciliationConflict` operator disarm instead of silently choosing a side.
- Trade-derived evidence can recover terminal truth when direct order evidence
  is missing/unknown.
- Position-derived evidence is lowest precedence; flat or missing position is
  unknown, not proof of success.
- Cancel follow-up can now run from the multi-source decision while preserving
  the same guarded uncertain-state matrix.
- Details are documented in
  `docs/m3b6-broker-truth-source-semantics.md`.

M3b-6 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-7 broker-truth fetch orchestration simulator:

- Added a dry orchestration layer over M3b-6 source classifications.
- Mock dry fetchers model `GetOrder`, `OrdersSnapshot`, `TradesSnapshot`, and
  `PositionSnapshot` without calling real FINAM order endpoints.
- Source diagnostics now distinguish typed reasons: `NotFound404`, `Timeout`,
  `DecodeError`, `Maintenance`, `Unauthorized`, `NotRequested`,
  `MissingFixture`, and `PositionGuardRejected`.
- The orchestration policy snapshots precedence version, source order,
  per-source `max_age_ms`, and position guard config into redacted reports.
- Position-derived terminal truth is guarded by instrument/intent/expected
  delta/strategy-state context and by absence/staleness of direct order/trade
  evidence.
- Operator disarm selection covers stale, conflict, unknown, unauthorized,
  maintenance, and decode-error outcomes.
- SQLite-backed tests prove post-classification orchestration follow-up still
  persists the existing durable transition audit.
- Source-scan tests ensure the truth fetcher boundary stays dry and does not
  reference real order endpoint request specs or endpoint methods.
- Details are documented in
  `docs/m3b7-broker-truth-orchestration-simulator.md`.

M3b-7 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-8 read-only broker-truth boundary hardening:

- Added checked get-order response building: matching broker id or broker-id-
  absent matching client id can become evidence; mismatched identity becomes a
  typed `MismatchedOrderIdentity` reason.
- Position-derived terminal truth now requires direct order/trade sources to be
  actually attempted and missing/stale/unknown; excluding direct sources from
  precedence no longer strengthens position truth.
- Added explicit `CancelBrokerTruthReadonlyFetcher` boundary while keeping the
  existing mock implementation dry.
- Added read-only failure mapping: 404 -> `NotFound404`, 401/403 ->
  `Unauthorized`, 429 -> `RateLimited`, 5xx -> `Maintenance`, timeout ->
  `Timeout`, decode failure -> `DecodeError`.
- Added defaulted `broker_truth.cancel_reconciliation` gateway config section
  and shadow-config parsing for policy overrides.
- Orchestration reports now include a redacted policy snapshot with SHA-256
  policy fingerprint.
- Source-scan tests guard the read-only truth fetcher boundary against real
  order endpoint request specs, endpoint methods, `.post(`, and `.delete(`.
- Details are documented in
  `docs/m3b8-readonly-broker-truth-boundary.md`.

M3b-8 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-9 read-only broker-truth local HTTP mapper:

- Added an async-aware `CancelBrokerTruthAsyncReadonlyFetcher` contract using
  an owned request snapshot so future real read-only network code does not need
  to hide a blocking runtime behind the synchronous dry trait.
- Added local HTTP-shaped read-only fixtures for broker-truth fetches. Fixtures
  carry status plus optional body bytes; debug/diagnostic output exposes only
  status, body presence, body length, and body SHA-256.
- Added typed local DTO mapping for GetOrder, OrdersSnapshot, TradesSnapshot,
  and PositionSnapshot into `CancelBrokerTruthFetchResult`.
- Clarified read-only HTTP status policy: 408 and 504 map to `Timeout`; 502,
  503, and other 5xx map to `Maintenance`; 404, 401/403, 429, decode, and dry
  fixture failures keep their previous typed reasons.
- GetOrder reports now include categorical `identity_strength`
  (`BrokerOrderIdExact` or `ClientOrderIdFallback`) without raw broker/client
  identifiers.
- Default broker-truth policy version is now the neutral
  `cancel-truth-default-v1`.
- Tests cover async trait use, local DTO mapping, malformed body decode errors,
  redacted fixture debug, and broker-truth report redaction.
- Details are documented in
  `docs/m3b9-readonly-fetcher-local-http.md`.

M3b-9 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-10 read-only broker-truth local mock transport:

- Added GET-only redacted read-only request specs for GetOrder,
  OrdersSnapshot, TradesSnapshot, and PositionSnapshot.
- Added async `CancelBrokerTruthReadonlyHttpClient` and
  `LocalMockCancelBrokerTruthReadonlyHttpClient` for local read-only transport
  tests without network I/O.
- Captured read-only responses expose only source plus redacted HTTP
  diagnostic; raw body bytes stay private to the mapper boundary.
- Refined 4xx mapping: 400/422 -> `InvalidRequest`, 405 ->
  `UnsupportedEndpoint`, 409/410/other 4xx -> `UnknownClientError`.
- Added account/instrument scope checks for read-only broker-truth DTOs:
  matching order/client ids from a wrong account or instrument no longer become
  evidence.
- Added identity policy: `ClientOrderIdFallback` is weak by default and
  downgrades to `WeakIdentityNeedsConfirmation` unless explicitly allowed by
  config/policy.
- Source-scan tests now cover the async/local read-only transport boundary and
  reject order endpoint specs, POST/DELETE calls, raw `reqwest::Response`,
  generic `serde_json::Value`, and public raw body/response fields there.
- Details are documented in
  `docs/m3b10-readonly-fetcher-local-mock-transport.md`.

M3b-10 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-11 real-readonly broker-truth transport gate:

- Added disabled-by-default `real_readonly_broker_truth_enabled` feature flag.
- Added a separate real-readonly broker-truth gate that can approve only when
  read-only broker-truth is explicitly enabled and command/order/cancel/SLTP
  features remain disabled.
- Added FINAM REST read-only route builder separate from local `/readonly/...`
  placeholders. Route templates cover GetOrder, OrdersSnapshot, TradesSnapshot,
  and PositionSnapshot using documented `GET /v1/...` routes.
- Raw rendered route paths remain private to the route type; public route
  diagnostics expose only method, template, route source, query-key names, and
  presence/length metadata.
- Added async `FinamRealReadonlyBrokerTruthTransport` boundary requiring the
  real-readonly gate marker and returning only captured redacted responses.
- Hardened instrument identity: symbol-only equality no longer passes; venue
  and exchange must agree, with market allowed to be unknown only when absent
  from the broker DTO.
- Documented and tested `UnknownClientError -> UnknownPendingOrder` operator
  policy.
- Details are documented in
  `docs/m3b11-real-readonly-transport-gate.md`.

M3b-11 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-12 real-readonly broker-truth transport:

- Added GET-only `ReqwestFinamRealReadonlyBrokerTruthTransport` behind
  `RealReadonlyBrokerTruthGateApproved`.
- Added `FinamRealReadonlyBrokerTruthAsyncFetcher`, which builds FINAM GET
  routes, captures raw status/body privately, maps through typed broker-truth
  DTO classifiers, and exports only redacted diagnostics/audit records.
- Added `FinamRealReadonlyBrokerTruthQueryPolicy`: trades use a bounded
  single-page window ending at `request.requested_at`; orders/positions are
  filtered client-side after broker account snapshots.
- Added redacted operator guardrails for enabling read-only broker truth:
  HTTPS base URL, account allowlist, bounded timeout, minimum request interval,
  and disabled order/runtime flags.
- Added `SqliteFinamRealReadonlyBrokerTruthAuditStore` for redacted
  real-readonly attempt audit rows.
- Added source-scan tests proving the real-readonly transport remains GET-only
  and does not introduce order endpoint request specs, `.post(`, or `.delete(`.
- Details are documented in
  `docs/m3b12-real-readonly-broker-truth-transport.md`.

M3b-12 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-13 real-readonly enablement runbook / contract probe:

- Added `RealReadonlyBrokerTruthRunApproved`, constructible only from the
  real-readonly gate marker plus an allowed operator guardrail decision.
- Real-readonly transport/fetcher construction now requires the run marker, and
  fetch attempts for a different account hash are blocked before route rendering
  or HTTP send.
- Added redacted transport error categories: DNS/connect, TLS, HTTP send, body
  read, timeout, request-build, and account-not-allowed.
- Added trades page-full semantics:
  `TradesSnapshotIncomplete -> UnknownPendingOrder` rather than strong absence
  evidence.
- Added disabled-by-default `FinamRealReadonlyContractProbeConfig` and probe
  report harness for selected read-only broker-truth sources.
- Extended redacted SQLite audit records for route-build/account-scope failures
  and transport error categories.
- Details are documented in
  `docs/m3b13-real-readonly-enable-runbook.md`.

M3b-13 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-14 real-readonly contract probe operator harness:

- Added a one-shot operator-run harness around the M3b-13 read-only contract
  probe.
- The harness requires the existing `RealReadonlyBrokerTruthRunApproved`
  carried by the fetcher and re-checks the request against the approved account
  hash before any probe can run.
- Added explicit probe limits: source list, `max_requests <= 4`, timeout match,
  min-interval match, no retry, no background loop, no scheduler, and documented
  disable procedure.
- Added redacted output-location descriptor and explicit audit-store mode:
  `EphemeralEvidenceStore` or `PersistentAuditStore`.
- Added transport-error operator-action taxonomy so reports preserve DNS/TLS/send
  / body-read / timeout / request-build / account-scope differences.
- Added `scripts/forbidden_surface_scan.sh` plus GitHub Actions CI invocation.
- Details are documented in
  `docs/m3b14-real-readonly-contract-probe-operator-harness.md`.

M3b-14 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-15 real-readonly pre-run hardening:

- Tightened `scripts/forbidden_surface_scan.sh` so `.post(` is allowed only in
  exact auth/session/token-details function ranges and exact `/v1/sessions`
  / `/v1/sessions/details` REST paths.
- Bound `ReqwestFinamRealReadonlyBrokerTruthTransport::try_new(...)` timeout and
  min-interval configuration to the `RealReadonlyBrokerTruthRunApproved`
  diagnostic at construction time.
- Made the lower-level contract-probe loop internal; the public operator
  entrypoint remains `run_finam_real_readonly_operator_contract_probe(...)`.
- Added scan/test coverage for the operator-only entrypoint invariant.
- Documented that `PersistentAuditStore` remains modeled but controlled
  real-readonly probes should use `EphemeralEvidenceStore` until a persistent
  audit operational policy is accepted.
- Details are documented in `docs/m3b15-real-readonly-pre-run-hardening.md`.

M3b-15 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-16 real-readonly contract probe evidence gate:

- Added approved FINAM base URL length/SHA-256 to
  `RealReadonlyBrokerTruthRunApproved`.
- `ReqwestFinamRealReadonlyBrokerTruthTransport::try_new(...)` now rejects
  configs whose base URL differs from the approved marker.
- Added redacted token/account preflight diagnostic for token-details/account
  hash match and no-order-feature checks.
- Operator-run probe now requires passing token/account preflight before any
  source loop can run.
- Added redacted evidence matrix rows for route template, status, body hash,
  mapped reason/outcome, operator action, and audit-record fingerprint.
- Operator-run controlled probe blocks `PersistentAuditStore`; M3b-16 evidence
  runs remain `EphemeralEvidenceStore` only.
- Details are documented in
  `docs/m3b16-real-readonly-contract-probe-evidence-gate.md`.

M3b-16 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-17 real-readonly evidence package hardening:

- Added token readonly/scope diagnostics:
  `token_readonly_flag_present`, `token_readonly_flag_value`, and
  `md_permissions_count`.
- Operator-run token/account preflight now requires `readonly == Some(true)` in
  addition to token/account hash match and order-feature disablement.
- Added per-source attempt records with `attempt_id` so evidence matrix rows are
  built from an attempt record rather than positional array alignment.
- Added operator report counters:
  `requested_sources_count`, `actual_send_count`, and `max_requests`.
- Tests assert `actual_send_count <= max_requests` and blocked token scope sends
  zero requests.
- Details are documented in `docs/m3b17-real-readonly-evidence-package.md`.

M3b-17 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-18 real-readonly pre-evidence gate:

- Kept the token/account preflight diagnostic redacted and serializable for
  reporting, but moved operator approval to
  `FinamRealReadonlyTokenAccountPreflightApproved`, a non-serializable marker
  that can only be constructed from checked readonly token scope, account hash
  match, and disabled order feature flags.
- Added `probe_run_started_at`, `probe_run_id`, and `probe_run_fingerprint` to
  the operator report, and copied the fingerprint into attempt records and
  evidence-matrix rows for redacted audit correlation.
- Split operator counters into `requested_sources_count`, `attempt_count`,
  `captured_response_count`, `actual_http_send_started_count`,
  `actual_http_send_completed_count`, `actual_send_count`, and `max_requests`.
  `actual_send_count` remains a compatibility alias for started sends.
- Continued to block `PersistentAuditStore` in controlled operator runs.
- Details are documented in
  `docs/m3b18-real-readonly-pre-evidence-gate.md`.

M3b-18 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-19 real-readonly request-bound evidence gate:

- Bound `FinamRealReadonlyTokenAccountPreflightApproved` to the exact redacted
  request snapshot used at marker construction time.
- Added `TokenAccountPreflightRequestMismatch` so operator runs block before
  any attempt when a marker is reused for a different broker-truth target.
- Added redacted `request_snapshot` evidence to operator reports:
  request fingerprint, account/order/client id lengths and SHA-256 hashes,
  instrument identity length/hash, requested timestamp, and position guard
  context.
- Added redacted `source_order` evidence with ordered sources and
  `ordered_sources_sha256`.
- Included `request_snapshot_fingerprint` and `ordered_sources_sha256` in the
  probe-run fingerprint.
- Details are documented in
  `docs/m3b19-real-readonly-request-bound-evidence-gate.md`.

M3b-19 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-20-pre real-readonly pre-run freshness gate:

- Added explicit preflight freshness metadata:
  `preflight_checked_at` and `preflight_max_age_ms`.
- Added `TokenAccountPreflightExpired` so stale request-bound markers block the
  operator run before any GET attempt.
- Added per-row evidence matrix flags:
  `actual_http_send_started` and `actual_http_send_completed`.
- Kept aggregate counters and `actual_send_count` compatibility alias.
- Documented that M3b-20-pre is still not the actual FINAM evidence run; the
  controlled one-shot real-readonly evidence package must be a separate
  artifact.
- Details are documented in
  `docs/m3b20-real-readonly-pre-run-freshness-gate.md`.

M3b-20-pre explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-21-pre real-readonly operator clock gate:

- Added explicit operator-run clock configuration:
  `probe_run_started_at: Option<DateTime<Utc>>`.
- Enabled operator runs without an operator-provided clock block with
  `ProbeRunClockMissing` before any attempts.
- Added `probe_run_clock_source` to the report. `OperatorProvided` is required
  for controlled enabled runs; `MissingFallbackToFetcherObserved` is only a
  blocked-report diagnostic fallback.
- Added `computed_preflight_age_ms` to operator reports.
- Added transport-like test coverage proving per-row and aggregate send flags
  propagate for both started/completed and started/not-completed cases.
- Details are documented in
  `docs/m3b21-real-readonly-operator-clock-gate.md`.

M3b-21-pre explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-22 controlled real-readonly evidence package:

- Added `broker-cli finam-real-readonly-evidence` as the controlled one-shot
  operator command for the first real-readonly FINAM broker-truth evidence
  package.
- The command requires a fresh request-bound
  `FinamRealReadonlyTokenAccountPreflightApproved` marker, an explicit
  operator-provided `probe_run_started_at`, and exports
  `computed_preflight_age_ms`.
- The run stays single-account/single-base-URL by approved hashes, uses
  `EphemeralEvidenceStore`, disables retry/background loop/scheduler, and
  caps actual GET-only broker-truth requests at `max_requests <= 4`.
- The evidence package contains only redacted scope, request, source-order,
  send-flag, route-template, status/body-hash, mapped-reason, operator-action,
  and audit-fingerprint data.
- Details are documented in `docs/m3b22-real-readonly-evidence-package.md`.

M3b-22 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-23 real-readonly evidence closeout hardening:

- Added self-contained evidence metadata: full source commit, optional source
  archive name/SHA-256, broker-cli package version/build profile, forbidden
  surface scan status/script SHA-256, and runbook version.
- The evidence command runs `scripts/forbidden_surface_scan.sh` before FINAM
  broker-truth requests.
- Added per-attempt timing and actual HTTP-send timing to evidence matrix rows.
- Added parsed-count reconciliation summaries for orders, trades, and
  positions, separating HTTP contract evidence from target broker-truth
  evidence.
- Documented the future read-only-only GetOrder 200 evidence plan.
- Details are documented in `docs/m3b23-real-readonly-evidence-closeout.md`.

M3b-23 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3b-24 / M3c-0 pre-order readiness closeout:

- Added explicit GetOrder 200 synthetic real-shape fixture coverage for exact
  identity and `MismatchedOrderIdentity`.
- Added redacted fixture assertions for positive reconciliation summary:
  parsed/matched counts and `BrokerOrderIdExact` terminal truth.
- Documented release-profile evidence policy for future pre-order gate review.
- Documented FINAM route-template recheck policy for `FinamRestDocs20260701`.
- Documented M3c feature-flag/off-by-default order transport plan and
  pre-order readiness matrix.
- Details are documented in
  `docs/m3b24-m3c0-pre-order-readiness-closeout.md`.

M3b-24 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3c-0 / M3c-1 order endpoint gate design:

- Added explicit `GatewayFeatureSet::real_order_endpoint_enabled`, default
  `false`.
- Added `M3cImplementationReviewRequired` as a real endpoint gate blocker.
- Added a serializable M3c design report and checklist covering feature flags,
  operator arming, allowlists, validation guards, SQLite durable store,
  unknown-active-order startup guard, no-blind-retry, manual intervention,
  redacted ACK policy, source-scan extension plan, release-profile evidence,
  route-template recheck, and positive GetOrder evidence/waiver.
- `EndpointGateApproved` remains unconstructible; `endpoint_calls_allowed`
  remains `false`.
- Details are documented in `docs/m3c0-order-endpoint-gate-design.md`.

M3c-0 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3c-2 order endpoint implementation gate design hardening:

- Added strict checklist status vocabulary:
  `DesignRecorded`, `ImplementedAndTested`, `EvidenceProvided`,
  `WaiverAccepted`, and `Blocked`.
- Added self-contained M3c evidence fields for forbidden-surface scan status,
  scan script SHA256, check timestamp, source commit, and source archive hash.
- Added `broker-cli m3c-order-endpoint-gate-report` to emit and save the M3c
  gate report after running the forbidden-surface scan.
- Added future order endpoint allowlist data for exactly:
  `POST /v1/accounts/{account_id}/orders` and
  `DELETE /v1/accounts/{account_id}/orders/{order_id}`.
- Added negative-test plan entries for same-module bypasses, generic request
  wrappers, route-string bypasses, and non-reqwest client abstractions.
- Release-profile evidence/waiver, positive GetOrder evidence/waiver, and
  current route-template recheck remain explicit `Pending` evidence slots.
- Details are documented in `docs/m3c0-order-endpoint-gate-design.md`.

M3c-2 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3c-3 implementation-gate preconditions hardening:

- Added source archive content-binding for M3c evidence generation: the CLI now
  opens the supplied ZIP, reads `handoff-commit.txt`, and verifies that
  `source_ref` equals the current source ref and `archive_name` equals the
  supplied archive file name.
- Extended `M3cSourceEvidence` with handoff source ref, handoff archive name,
  and `source_archive_content_binding_verified`.
- Added explicit evidence slot status options for release-profile,
  positive-GetOrder, and route-template-recheck slots:
  `pending`, `evidence-provided`, and `waiver-accepted`.
- Added `scripts/forbidden_surface_negative_harness.sh` and tightened the
  forbidden-surface scan for literal FINAM order route bypasses in
  `broker-finam` source.
- M3c-3a completes the negative harness with a non-reqwest order endpoint HTTP
  abstraction case and adds unit coverage for stale `handoff-commit.txt`
  content binding, including stale `source_ref` and stale `archive_name`.
- Real endpoint approval remains impossible and `endpoint_calls_allowed`
  remains `false`.

M3c-3 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3c-4 explicit order endpoint implementation gate transition plan:

- Added serializable `implementation_transition_plan` to the M3c gate report.
- Recorded the decision that `FinamRealOrderEndpointTransport` remains an
  approved-only compile contract, not implementation approval.
- Recorded current scanner mode as `CurrentDenyAllOrderPostDelete`.
- Recorded future scanner mode as `FutureExactTwoRouteAllowlistAfterReview`.
- Recorded initial future implementation module candidate; superseded by the
  M3c-5 gateway-owned HTTP-send boundary decision.
- Recorded that future route rendering and HTTP send must both require
  `EndpointGateApproved`.
- Release-profile evidence/waiver, positive GetOrder evidence/waiver, and
  route-template recheck remain required before implementation gate.
- Details are documented in
  `docs/m3c4-order-endpoint-implementation-transition-plan.md`.

M3c-4 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3c-5 implementation boundary architecture decision / scanner transition design:

- Resolved the crate-boundary decision as
  `GatewayHttpSendBrokerFinamRouteBuilder`.
- `broker-finam` remains request-spec/route-builder only and must not contain
  future real order HTTP send surfaces.
- Future real HTTP send boundary is planned inside `finam-gateway` at
  `crates/finam-gateway/src/real_order_endpoint.rs`, where
  `EndpointGateApproved` already lives.
- This avoids a Rust workspace dependency cycle between `broker-finam` and
  `finam-gateway`.
- The future scanner transition remains design-only:
  `CurrentDenyAllOrderPostDelete` -> `FutureExactTwoRouteAllowlistAfterReview`
  after a separate implementation review.
- Details are documented in
  `docs/m3c5-implementation-boundary-architecture-decision.md`.

M3c-5 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3c-6 exact-two-route scanner transition spec / pre-implementation API shape:

- Added design-only module `crates/finam-gateway/src/real_order_endpoint.rs`.
- The module records API shape only: no reqwest client, no real HTTP send,
  no `.post(`, no `.delete(`, and no `Method::POST/DELETE`.
- Route-shape functions require `EndpointGateApproved` plus the broker-finam
  place/cancel request specs in their signatures.
- Added `GatewayRealOrderEndpointScannerTransitionSpec` with current deny-all
  mode and future exact-two-route mode.
- Added `scripts/order_endpoint_scanner_transition_spec.sh` to verify the
  design-only module and gate-marker/API-shape markers.
- Details are documented in `docs/m3c6-scanner-transition-api-shape.md`.

M3c-6 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3c-7 gated route-rendering boundary / design-shape hardening:

- `api_shape()` is now design/report shape only and does not contain route
  templates.
- Future route template access is separated into gated route-shape functions
  requiring `EndpointGateApproved`.
- `scripts/order_endpoint_scanner_transition_spec.sh` now also rejects
  `.request(`, `.send(`, `reqwest::Client`, `HttpClient`, `Transport`,
  `Adapter`, and `Backend` in the design-only module.
- Details are documented in `docs/m3c7-gated-route-rendering-boundary.md`.

M3c-7 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3c-8 non-serializable route boundary / redacted diagnostic hardening:

- Route templates inside `crates/finam-gateway/src/real_order_endpoint.rs` are
  now held only by a private, non-serializable internal route shape.
- Public gated helpers still require `EndpointGateApproved`, but return only
  `GatewayRealOrderEndpointRedactedRouteDiagnostic`.
- Exported diagnostics set `route_template_redacted = true` and
  `route_template_exported = false`.
- `scripts/order_endpoint_scanner_transition_spec.sh` now rejects any `reqwest`
  token in the design-only module, not only `reqwest::Client`.
- Details are documented in `docs/m3c8-nonserializable-route-boundary.md`.

M3c-8 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3c-9 approved request-parts boundary / internal capability hardening:

- Added private `ApprovedOrderEndpointRequestParts` design type inside
  `crates/finam-gateway/src/real_order_endpoint.rs`.
- Added separate private `RenderedOrderEndpointPath` type for rendered paths.
- Both internal types remain non-`Debug`, non-`Serialize`, and
  non-`Deserialize`.
- Private constructors require `EndpointGateApproved`, an approved FINAM
  request spec, account/instrument allowlist approval, operator-arm approval,
  and a durable-state checkpoint.
- Exported diagnostics still redact rendered paths and raw body data and cannot
  feed the request-parts constructors.
- Details are documented in
  `docs/m3c9-approved-request-parts-boundary.md`.

M3c-9 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

M3c-10 approved request-parts consumer boundary / future endpoint consumer
shape:

- Added private `consume_approved_request_parts_for_future_endpoint` inside
  `crates/finam-gateway/src/real_order_endpoint.rs`.
- The consumer requires `EndpointGateApproved` and accepts only the private
  `ApprovedOrderEndpointRequestParts` capability.
- The consumer remains design-only/no-network and returns only
  `GatewayRealOrderEndpointConsumerDiagnostic`.
- Exported consumer diagnostics redact rendered path, raw body, account id,
  broker order id, instrument symbol, and client order id.
- Scanner guards now check that the consumer is private, diagnostics cannot
  feed it, `consumer_network_enabled = false`, `rendered_path_exported = false`,
  and `raw_body_exported = false`.
- Details are documented in
  `docs/m3c10-approved-parts-consumer-boundary.md`.

M3c-10 explicitly still not allowed:

- FINAM POST/DELETE order endpoint calls.
- Real command stream consumer connected to strategies.
- Real CommandAck lifecycle against FINAM endpoints.
- Strategy runtime adaptation or invocation.
- `LiveReady` publication.
- First live micro.
- Stop/SLTP/bracket.

Future M3 targets after dry-order-path review acceptance:

- Operator-armed order-emitting mode after M2m acceptance.
- Market and limit order placement with short client order id and comment.
- Cancel command and terminal-state handling.
- ACK lifecycle separate from fill lifecycle.
- USDRUBF-like simple market lifecycle.

Exit criteria:

- Durable id mapping survives restart/replay.
- No-blind-retry behavior is proven by tests.
- Operator arming and automatic disarm are proven by tests.
- One or more micro live cycles complete and reconcile.
- No bracket/stop semantics yet.
- No blind duplicate after ambiguous place-order timeout.

## M4 — stop/bracket research and implementation

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

## Phase 1 P0 blockers

- `order_id: i64` remains in runtime-facing contract.
- `client_order_id` is missing, longer than 20 characters, or not persisted.
- Broker-truth snapshots are optional before live readiness.
- Instrument mapping/schedule is hardcoded or unvalidated.
- Historical/live bar timestamp convention is not proven by golden tests.
- Unknown broker order/trade status is ignored or panics.
- Stop/SLTP/bracket is enabled before dedicated FINAM contract tests.
- Place-order timeout can retry before reconciliation by `client_order_id`.

## Review-fix backlog before Redis gateway

- Fixture recording mode for read-only responses with bounded redacted JSON
  shape metadata.
- Durable `StrategyRequestId -> ClientOrderId -> BrokerOrderId` mapping store.
- Fixture-based typed DTO tests from checked-in sanitized fixtures.
- Golden test proving FINAM bar timestamp convention around normal bars and
  session gaps.

## Allowed after M1.2 safety patch

- Run `finam-auth-check` with the real secret token.
- Run `finam-readonly-check` with real `account_id` and `symbol`.
- Run `finam-typed-readonly-check` with real `account_id` and `symbol`.
- Save redacted response shapes/fixtures via `--output`.
- Start typed DTO/mappers from real FINAM responses.

Allowed after M1.5 acceptance:

- Start M2a Redis/shadow gateway skeleton only.
- Keep live-order and runtime work gated behind later review.

Still not allowed before M2/M3 approval:

- command consumer / ACK lifecycle;
- order placement or cancel;
- runtime adaptation;
- live micro;
- Stop/SLTP/bracket work beyond API research.
