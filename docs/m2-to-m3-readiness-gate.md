# M2-to-M3 readiness gate

Status: M2m design/checklist. This document is not an implementation of order
placement, cancel, command consumption, live readiness, strategy runtime
attachment, stop/SLTP, or bracket lifecycle.

M2 is considered a dry/shadow gateway and runtime-bridge characterization phase.
M3 may start only after every blocking gate below is accepted in review.

Primary references:

- FINAM REST docs: https://api.finam.ru/docs/rest/
- FINAM getting started / limits: https://api.finam.ru/getting-started/
- FINAM REST docs list `POST /v1/accounts/{account_id}/orders` and
  `DELETE /v1/accounts/{account_id}/orders/{order_id}` for future exchange
  order placement/cancel;
- FINAM REST docs: request errors include rate-limit and timeout classes such
  as 429, 5xx, and 504, so M3 must avoid blind retry;
- `docs/broker-contract.md`;
- `docs/active-orders-startup-policy.md`;
- `docs/redis-stream-contract.md`;
- `docs/finam-bar-finality-evidence-2026-06-30.md`;
- `docs/m3-order-path-design.md`.

## Gate summary

| Gate | Requirement | M2m status |
|---|---|---|
| G0 | Clean handoff and reproducible checks | design accepted before M3 |
| G1 | Broker-truth snapshots and readiness | M2 shadow path exists; live readiness remains blocked |
| G2 | Bar finality policy | design required before runtime bars drive orders |
| G3 | Durable watermark/dedupe | design required before runtime bridge attachment |
| G4 | Active order ownership | must block unknown ownership |
| G5 | Operator arming | explicit multi-factor arm required |
| G6 | Command stream contract | MARKET/LIMIT/CANCEL only |
| G7 | ACK lifecycle | ACK separated from fills and reconciliation |
| G8 | Durable id mapping | required before any order POST/DELETE |
| G9 | Retry/rate-limit/backoff | no blind retry after ambiguous submit |
| G10 | Fixture/test matrix | required before first micro live |

## G0 — M2 exit checklist

Required before M3 code:

- latest M2 handoff archive is clean;
- `cargo fmt --all --check` passes;
- `cargo test --all` passes;
- `cargo clippy --workspace --all-targets -- -D warnings` passes;
- Redis shadow smoke passes;
- runtime bridge dry smoke passes;
- local secret/JWT/account/order identifiers are absent from committed files and
  handoff archive;
- order endpoints are still disabled in M2 handoff.

## G1 — broker-truth and readiness

M3 order-emitting mode must not arm until broker-truth state is loaded from
FINAM and normalized:

- account is reachable and matches the operator-selected account;
- cash/portfolio snapshot is loaded;
- positions are loaded;
- active/terminal orders are loaded;
- recent trades or own-trade stream is loaded;
- instrument params, lot, tick, board/MIC, expiration, and schedule are loaded;
- gateway health is not degraded/stopped;
- no unknown broker order/trade status is present;
- no raw broker-native comment is exposed to runtime streams.

`ReadinessPhase::LiveReady` remains blocked until the M3 implementation encodes
these checks and the operator arming model below.

## G2 — bar finality policy

FINAM M1 evidence supports open-timestamp mapping for checked windows, but the
policy must remain conservative because evidence is instrument/window-specific.

Runtime order decisions may use a bar only when all are true:

- `open_ts` is mapped from FINAM `bar.timestamp`;
- `close_ts = open_ts + timeframe`;
- `close_ts <= receive_ts - finality_grace`;
- the bar key is not already accepted by durable dedupe;
- the instrument schedule and actual returned bars do not indicate a forming or
  ambiguous boundary bar;
- historical REST `end_time` is treated as inclusive unless future evidence
  proves otherwise.

If any condition is false, the bar is observability-only and must not trigger an
entry. Near-session-boundary bars require extra caution: actual bar availability
and broker-provided instrument schedule win over generic exchange-session
assumptions.

## G3 — durable watermark/dedupe

The M2 producer watermark is an in-process low-noise heuristic:

```text
venue_symbol|timeframe|open_ts
```

The M3 runtime/durable dedupe key must be:

```text
source|source_kind|venue_symbol|timeframe_sec|open_ts|is_final
```

The durable store must support:

- atomic insert-if-absent for a runtime-accepted bar key;
- latest processed Redis stream id per stream/group;
- recovery replay without confusing `HistoricalPoll`, `Recovery`, and future
  `LiveStream` bars;
- operator-visible diagnostics for duplicate/replayed bars.

## G4 — active order ownership

Before `LiveReady`:

- active broker orders must be empty, terminal, externally approved, or owned by
  durable local state;
- unknown owner, unknown status, unexpected account/symbol/qty/side, or
  stop/SLTP/bracket lifecycle must block;
- operator-visible reports may include redacted ids/fingerprints only.

## G5 — operator arming model

M3 requires explicit arming, not implicit readiness:

- static config: order mode disabled by default;
- account allowlist;
- instrument allowlist;
- max contracts per instrument;
- max gross exposure and max daily order count;
- dry-run/shadow mode selected by default;
- explicit operator arm flag for market/limit/cancel;
- separate flag for using real FINAM order endpoints;
- visible preflight summary before arming;
- automatic disarm on degraded health, unknown active order, DLQ burst, clock
  skew, schedule closed, or reconnect gap.

`DryReady` is never enough to place orders. `LiveReady` may be published only
after all M3 gates and operator arming checks pass.

## G6 — command stream contract

M3 command stream design is MARKET/LIMIT/CANCEL only:

- `BrokerCommand::PlaceOrder` with `OrderType::Market` or `OrderType::Limit`;
- `BrokerCommand::CancelOrder`;
- one command payload per Redis stream entry;
- envelope `schema_version = 2`;
- idempotency by `StrategyRequestId` and `ClientOrderId`;
- rejected immediately if account/symbol/qty/side/type is outside operator arm;
- stop/SLTP/bracket commands remain absent/disabled.

The command stream must not be consumed in M2m. It becomes an M3 implementation
task after this design is accepted.

## G7 — ACK lifecycle

ACK is transport/command-path state, not fill state.

Allowed M3 ACK meanings:

- `Accepted`: command passed local validation and was accepted by the command
  path;
- `Submitted`: FINAM order endpoint returned a broker-side accepted/submitted
  response;
- `Duplicate`: command already exists by durable id mapping;
- `Expired`: command TTL elapsed before submission;
- `Recovered`: ambiguous prior submit was reconciled by broker-truth state;
- `Rejected`: local validation/operator/readiness/broker rejection;
- `Timeout`: request timed out and reconciliation is required;
- `UnknownPending`: final command state is not known yet and retry is blocked;
- `Error`: non-retryable local/transport error.

Fills, partial fills, terminal order states, and PnL come only from broker-truth
orders/trades/reconciliation streams.

## G8 — durable id mapping

Before any FINAM POST/DELETE order call, create a durable mapping store with:

```text
StrategyRequestId
ClientOrderId
BrokerOrderId optional until broker response/reconciliation
account_id fingerprint/reference
instrument
side
order_type
qty
limit_price optional
created_ts
last_update_ts
submit_attempt_count
state
last_error_kind
```

State machine:

```text
IntentRecorded
LocalRejected
SubmitInFlight
Submitted
TimeoutUnknownPending
RecoveredByClientOrderId
BrokerRejected
CancelRequested
CancelSubmitted
Terminal
ManualInterventionRequired
```

The store must be written before network submission so a process crash cannot
turn an unknown order into an unowned broker order.

## G9 — no-blind-retry / rate-limit / backoff policy

No blind retry after ambiguous place-order timeout.

Submit policy:

1. persist intent and id mapping;
2. call FINAM once;
3. on success, persist returned broker id if present and emit `Submitted`;
4. on timeout/5xx/transport unknown, mark `TimeoutUnknownPending`, block retry,
   reconcile by `client_order_id` and broker orders/trades;
5. only after reconciliation proves no broker order exists may a new submit be
   considered, and it must be operator-visible.

Retry/backoff policy:

- retry read-only/reconciliation calls with bounded exponential backoff;
- respect FINAM rate-limit errors such as 429;
- do not retry order placement blindly;
- cancel can be retried only when broker order id is known and order remains
  active by broker truth;
- all retries must be bounded and visible in metrics.

## G10 — fixtures and safety test matrix

Required before first live micro:

- place MARKET accepted;
- place LIMIT accepted;
- place validation rejected locally;
- broker rejects order;
- place timeout then recovered by `client_order_id`;
- duplicate command id returns `Duplicate`;
- cancel active order;
- cancel terminal/missing order is handled safely;
- partial fill before order snapshot;
- trade before order snapshot;
- reconnect replays order/trade without duplicate ACK/fill;
- active unknown broker order blocks `LiveReady`;
- rate-limit/backoff path;
- degraded/stopped health disarms operator mode.

Fixtures must be synthetic or redacted; no raw account/order ids, JWTs, or
broker payloads may be committed.

## M3 entry rule

After M2m review acceptance, M3 may start with design-to-code work in this
order:

1. durable id mapping store;
2. order-path validation and operator arming preflight;
3. command ACK publisher using synthetic commands only;
4. FINAM order DTO/mappers with fixture tests;
5. dry order-path integration without POST/DELETE;
6. only then real MARKET/LIMIT/CANCEL micro behind explicit operator arming.

Stop, SLTP, bracket, strategy migration, scaling, and RI remain outside M3
micro scope.

## M3a-1 non-network foundation status

Implemented in `broker-core::order_path`:

- state-machine transition table with negative transition tests;
- in-memory store specification that rejects duplicate `StrategyRequestId` and
  duplicate `ClientOrderId`;
- restart recovery rule: recorded intent is not submitted, submit-in-flight
  becomes timeout/unknown-pending before any retry;
- outgoing comment policy: disabled by default, sanitized deterministic mode
  available, raw value skipped from serialization, unsafe/too-long values
  rejected;
- operator arm TTL/one-shot semantics;
- place-order preflight that rejects invalid account, symbol, order type, TIF,
  quantity step/bounds, market quantity, and limit price tick.

Still not implemented in M3a-1:

- final durable backend;
- FINAM POST/DELETE order endpoint calls;
- real command stream consumer;
- ACK publication against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro.

## M3a-2 dry order-path hardening status

Implemented in `broker-core::order_path` while keeping all broker endpoints
disabled:

- `OrderPathStore` trait plus `JsonFileOrderPathStore` for local durable
  restart/replay tests;
- persisted intent/state recovery with duplicate `StrategyRequestId` and
  `ClientOrderId` checks after reopening the file store;
- cancel preflight for active, terminal, missing-arm, missing-broker-id,
  account-mismatch, and missing-mapping cases;
- MARKET reference-price freshness and notional guards;
- LIMIT requirement for loaded price step, positive tick-aligned price,
  optional reference-price deviation band, and notional guards;
- synthetic `CommandAck` builder for dry tests, explicitly separated from fill
  or trade semantics.

Still not implemented in M3a-2:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer;
- ACK publication against FINAM endpoints or Redis command streams;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3a-3 endpoint-adjacent dry hardening status

Implemented in `broker-core::order_path` while still keeping all broker
endpoints disabled:

- cancel preflight now requires `cancel.order_id` to exactly match
  `existing.broker_order_id`;
- missing existing broker-order mapping rejects as `CancelMappingMissing`;
- mismatched active or terminal mapping rejects as `CancelMappingMismatch`;
- raw `PlaceOrder.comment` rejects at place preflight before persist/endpoint
  work;
- one-shot operator arm now reports canonical `OneShotAlreadyUsed` after an
  endpoint-attempt marker;
- store update invariants reject client id change, broker id change/clear,
  terminal-to-non-terminal overwrite, and regressed `last_update_ts`;
- decimal tick/step tests cover common futures scales;
- limit reference-band tests cover exact boundary, over-boundary, and invalid
  reference price cases.

Still not implemented in M3a-3:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer;
- ACK publication against FINAM endpoints or Redis command streams;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3a-4 dry request/ACK builder hardening status

Implemented while still keeping all broker endpoints disabled:

- `BrokerOrderId` is a unique secondary key across order-path records;
- duplicate broker ids reject on in-memory insert/update and JSON-file reopen;
- cancel state machine supports `RecoveredByClientOrderId -> RequestCancel`;
- cancel timeout moves to `CancelTimeoutUnknownPending` and blocks blind retry;
- cancel timeout may recover to `CancelRecoveredTerminal` by broker-truth
  reconciliation or move to `ManualInterventionRequired`;
- cancel preflight rejects already-pending cancel and unknown/manual states;
- `CommandAck.reason` is now a safe structured reason code, not arbitrary text;
- `broker-finam::order_request` builds dry FINAM MARKET/LIMIT/CANCEL request
  path/body specs without sending HTTP.

Still not implemented in M3a-4:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer;
- ACK publication against FINAM endpoints or Redis command streams;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3a-5 dry approved-path / mock ACK publisher status

Implemented while still keeping all broker endpoints disabled:

- request builders accept only preflight-approved marker types, not raw
  commands;
- command TTL expiry is rejected by preflight for place and cancel commands;
- dry request path/body diagnostics are redacted and mock FINAM dry client
  stores only diagnostics;
- synthetic dry `CommandAck` Redis publication exists only in disabled/mocked
  mode and refuses command-consumer/order/cancel/SLTP-enabled configs;
- dry ACK Redis payloads clear raw client/broker order ids and keep structured
  reason codes;
- ACK stream name and retention are configurable and bounded;
- dry integration test covers preflight -> store -> dry request spec -> mock
  diagnostics -> synthetic ACK envelope;
- operator disarm safety signals and dry rate-limit capacity are covered by
  local tests.

Still not implemented in M3a-5:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3a-6 mock network boundary / execution simulator status

Implemented while still keeping all broker endpoints disabled:

- approved-only execution client trait and scripted mock execution client;
- mock execution diagnostics remain redacted;
- dry place-order simulator persists `BeginSubmit` before mock execution;
- accepted/rejected/timeout outcomes map to `Submitted`, `BrokerRejected`, and
  `TimeoutUnknownPending`;
- no-blind-retry from `TimeoutUnknownPending` is blocked before a second mock
  execution call;
- operator disarm/re-arm workflow is covered by local tests;
- dry window/backoff rate-limit policy is covered by local tests;
- ACK redaction and SQLite/WAL durable-store direction are documented in
  `docs/m3a6-execution-simulator-decisions.md`.

Still not implemented in M3a-6:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.
