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

Exit criteria:

- Stream events reconcile with REST snapshots.
- Reconnect does not create false orphan trades or stale positions.
- Runtime can consume FINAM-normalized events without strategy logic changes.

## M3 — micro MARKET/LIMIT/CANCEL

- Operator-armed order-emitting mode.
- Market and limit order placement with short client order id and comment.
- Cancel command and terminal-state handling.
- ACK lifecycle separate from fill lifecycle.
- USDRUBF-like simple market lifecycle.

Exit criteria:

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
