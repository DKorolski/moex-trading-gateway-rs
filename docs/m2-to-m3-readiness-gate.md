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

## M3a-7 dry cancel simulator / accepted-without-broker-id status

Implemented while still keeping all broker endpoints disabled:

- accepted place execution without a broker order id moves to
  `SubmittedPendingBrokerOrderId` with `UnknownPending` /
  `ReconciliationRequired`, not normal `Submitted`;
- cancel from `SubmittedPendingBrokerOrderId` is blocked until broker-truth
  reconciliation recovers a broker order id by client order id;
- operator disarm coverage includes accepted-without-broker-id and
  cancel-timeout-unknown-pending safety signals;
- dry cancel simulator persists `RequestCancel` before mock execution;
- dry cancel accepted/rejected/timeout outcomes map to `CancelSubmitted`,
  `ManualInterventionRequired`, and `CancelTimeoutUnknownPending`;
- no-blind-retry from `CancelTimeoutUnknownPending` is blocked before a second
  mock execution call;
- already-terminal cancel preflight remains a no-endpoint/no-mock-call path;
- dry cancel ACK publication remains redacted;
- approved-only execution client contract is locked to FINAM request specs, not
  raw commands;
- SQLite/WAL single-writer production-store requirements are captured in
  `docs/sqlite-order-path-store-implementation-ticket.md`.

Still not implemented in M3a-7:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3a-8 reconciliation-ready dry order path / SQLite planning status

Implemented while still keeping all broker endpoints disabled:

- dry broker-truth recovery helper resolves `client_order_id ->
  broker_order_id`, sets the broker id once, and transitions
  `SubmittedPendingBrokerOrderId` / `TimeoutUnknownPending` to
  `RecoveredByClientOrderId`;
- happy-path recovery proves that cancel preflight becomes allowed only after
  broker id recovery;
- duplicate broker-truth ids are rejected by the store and do not overwrite the
  pending record;
- dry cancel accepted response policy rejects a returned broker id that does
  not match the mapped cancel order id;
- cancel accepted broker-id mismatch moves to `ManualInterventionRequired` and
  returns a redacted `UnknownPending` / `ManualInterventionRequired` ACK;
- operator disarm coverage includes cancel broker-order-id mismatch and stale
  reconciliation safety signals;
- source-scan coverage blocks future raw command execution boundaries and
  direct DELETE calls in the order crates;
- ACK/reconciliation state matrix is documented in
  `docs/m3a8-reconciliation-state-matrix.md`.

Still not implemented in M3a-8:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3a-9 reconciliation idempotency / SQLite-WAL durable store prototype status

Implemented while still keeping all broker endpoints disabled:

- repeated reconciliation of the same `client_order_id -> broker_order_id`
  broker-truth fact is idempotent and returns the existing recovered record;
- same client id with a different broker id returns a mismatch error;
- duplicate broker id mapped to another request remains rejected by the store;
- `SqliteOrderPathStore` prototype is added with WAL, `synchronous=FULL`,
  `BEGIN IMMEDIATE` writes, unique request/client/broker ids, and a sidecar
  single-writer lock;
- SQLite prototype tests cover second writer rejection, crash/reopen,
  `SubmitInFlight` recovery persistence, `CancelRequested` and
  `SubmittedPendingBrokerOrderId` preservation, corrupt database blocking, and
  redacted export;
- approved-only source-scan coverage walks the whole `crates/` Rust source
  tree;
- prototype boundaries are documented in
  `docs/m3a9-durable-store-prototype.md`.

Still not implemented in M3a-9:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3a-10 SQLite production-hardening / dry command-to-store integration status

Implemented while still keeping all broker endpoints disabled:

- SQLite writer-lock metadata records instance id, pid, created timestamp, and
  schema version;
- stale/unknown writer locks are not auto-removed and remain an
  operator-controlled recovery condition;
- a created writer lock is cleaned up if SQLite connection open fails before a
  store instance exists;
- SQLite startup checks `order_path_schema.schema_version` and blocks unknown
  versions;
- read-only diagnostic SQLite store opens alongside the writer and is
  query-only;
- transition-audit rows are appended in the same transaction as order-path
  inserts/updates;
- SQLite file permissions are hardened locally where Unix permissions are
  available;
- operator disarm signals include store lock uncertainty, migration mismatch,
  and store unavailability;
- SQLite-backed dry simulator tests prove `BeginSubmit -> mock call` and
  `RequestCancel -> mock call` ordering against a read-only diagnostic view;
- public dry ACK publication remains redacted;
- retention/archive policy is documented in
  `docs/order-path-retention-archive-policy.md`.

Still not implemented in M3a-10:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3a-11 final pre-endpoint order-path gate status

Implemented while still keeping all broker endpoints disabled:

- SQLite runtime-file hardening covers DB, WAL, SHM, and writer-lock files when
  present;
- deployment policy requires `umask 077` and protected local runtime directory
  before any future live-capable process;
- raw read-only SQLite diagnostic methods are operator-only and named with the
  `operator_` prefix;
- redacted reporting/export remains separate through `redacted_records()`;
- transition audit rows record safe inferred event names instead of only
  generic update events;
- store errors map to operator disarm signals for lock uncertainty, migration
  mismatch, and store unavailability;
- real endpoint gate decision is explicit and remains blocked by
  `M3a11PreEndpointReviewRequired`;
- runtime ACK id policy is locked as `RedactedRuntimeAckOnly`;
- SQLite migration/backup runbook and pre-endpoint FINAM response fixture plan
  are documented in `docs/m3a11-final-pre-endpoint-gate.md`.

Still not implemented in M3a-11:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-0 design / fixture gate status

Implemented while still keeping all broker endpoints disabled:

- endpoint gate marker design exists and cannot be approved while the current
  decision contains `M3a11PreEndpointReviewRequired`;
- future real endpoint transport signature requires the marker and FINAM
  request specs;
- synthetic/redacted FINAM endpoint fixtures cover accepted/rejected/timeout/
  rate-limit/maintenance/decode-error classes;
- fixture diagnostics redact raw broker order ids;
- SQLite runtime-directory inspector exists for future deployment/startup
  checks;
- transition audit event-name matrix is table-tested;
- operator raw diagnostics remain operator/internal design only.

Still not implemented in M3b-0:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-1 endpoint response integration simulator status

Implemented while still keeping all broker endpoints disabled:

- synthetic/redacted FINAM endpoint fixtures are routed through the order-path
  state machine;
- accepted/rejected/timeout fixtures produce state transitions and ACKs through
  the same semantics as the dry execution simulator;
- rate-limit, maintenance, and decode-error fixtures persist an endpoint-attempt
  start and then move to `ManualInterventionRequired`;
- rate-limit, maintenance, and decode-error outcomes emit broker-neutral ACK
  reason codes and operator disarm signals;
- rate-limit preserves `retry_after_ms` in the integration report for future
  backoff wiring;
- no-blind-retry is tested after endpoint rate-limit;
- SQLite-backed audit covers `InsertIntent -> BeginSubmit ->
  RequireManualIntervention` with safe reason code `RateLimited`;
- Redis ACK publication remains redacted through the dry ACK publisher;
- `EndpointGateApproved` remains unconstructible.

Still not implemented in M3b-1:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-2 local HTTP endpoint mapper hardening status

Implemented while still keeping all broker endpoints disabled:

- local/mock HTTP-shaped endpoint response classifier exists without real
  broker network calls;
- status/body mapping covers success, broker rejection, 401/403 unauthorized,
  429 rate-limit, 500/503 maintenance, timeout, malformed JSON, and empty
  broker-order-id cases;
- local HTTP gateway integration persists `BeginSubmit` or `RequestCancel`
  before response classification;
- post-network decode/map failures become `ResponseDecodeError` plus
  `ManualInterventionRequired` after durable attempt recording;
- unauthorized responses have safe ACK/error/disarm categories;
- redacted ACK publication is tested for local HTTP success, decode error, and
  unauthorized responses;
- real broker base URL is not used in order tests;
- `EndpointGateApproved` remains unconstructible.

Still not implemented in M3b-2:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-3 redacted endpoint result/status policy status

Implemented while still keeping all broker endpoints disabled:

- internal endpoint result types are not serde export objects;
- internal endpoint result `Debug` output is redacted by presence/length rather
  than raw broker order id;
- `FinamOrderEndpointResponseDiagnostic` remains the safe export boundary;
- local HTTP response classification is endpoint-context-aware;
- cancel 404/409/410 maps to reconciliation-required, not ordinary broker
  rejection;
- 408/504 map to timeout/unknown-pending and can disarm as
  `UnknownPendingOrder`;
- 500/502/503 map to maintenance;
- body-read failure maps to decode error after durable attempt recording;
- operator disarm matrix covers unauthorized/rate-limit/maintenance/decode and
  timeout/ambiguous paths;
- `EndpointGateApproved` remains unconstructible.

Still not implemented in M3b-3:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-4 mock transport boundary / export hardening status

Implemented while still keeping all broker endpoints disabled:

- accepted endpoint response DTO is deserialize-only;
- synthetic endpoint fixture is not serde-exportable;
- redacted endpoint diagnostic remains the export boundary;
- mock classified endpoint transport returns only classified responses;
- future real transport compile contract is classified-response based while
  still requiring the unconstructible endpoint gate marker;
- source/contract tests guard against raw body or accepted DTO crossing the
  mock transport boundary;
- SQLite-backed tests prove durable `BeginSubmit` / `RequestCancel` before
  mock classified transport call;
- dry cancel reconciliation follow-up covers terminal, still-working, and
  unknown broker truth after uncertain cancel responses.

Still not implemented in M3b-4:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-5 broker-truth reconciliation source contract status

Implemented while still keeping all broker endpoints disabled:

- dry execution simulator trait is explicitly named
  `FinamDryApprovedOrderExecutionClient`;
- future real endpoint transport boundary remains classified-response based;
- broker-truth source classes are defined for orders snapshot, get-order,
  trades snapshot, and position snapshot inputs;
- broker-truth observations are non-serde and redacted;
- `CancelBrokerTruthDiagnostic` is the safe truth export boundary;
- terminal / still-working / unknown status classification is covered;
- stale truth maps to `Unknown` plus `ReconciliationStale` disarm;
- fresh unknown truth maps to `UnknownPendingOrder` disarm;
- cancel follow-up can run from broker-truth classification and preserves the
  guarded state matrix;
- production source-scan tests guard against old non-dry trait names and mapped
  result real-transport boundaries.

Still not implemented in M3b-5:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-6 broker-truth source semantics / precedence status

Implemented while still keeping all broker endpoints disabled:

- dry builders exist for `GetOrder`, `OrdersSnapshot`, `TradesSnapshot`, and
  `PositionSnapshot` future broker-truth inputs;
- source freshness is policy-driven with per-source `max_age_ms`;
- freshness is measured from the get-order response or snapshot receive time;
- missing source evidence is represented as unknown with `evidence_present =
  false`;
- trade-derived truth can recover terminal state when order truth is missing or
  unknown;
- position-derived truth is lowest precedence and flat/missing position remains
  unknown;
- multi-source reconciliation selects the highest-precedence fresh known source
  when there is no conflict;
- fresh terminal-vs-still-working disagreement produces
  `ReconciliationConflict` and operator disarm;
- stale-only truth produces `ReconciliationStale`;
- unknown-only truth produces `UnknownPendingOrder`;
- multi-source diagnostics export only source classes, presence/length flags,
  status classes, staleness, and age policy.

Still not implemented in M3b-6:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-7 broker-truth fetch orchestration simulator status

Implemented while still keeping all broker endpoints disabled:

- dry orchestration now models the future order of broker-truth collection:
  get-order, orders snapshot, trades snapshot, and position snapshot;
- mock truth fetchers are dry-only and return redacted observations or typed
  missing/error reasons;
- typed fetch reasons cover `NotFound404`, `Timeout`, `DecodeError`,
  `Maintenance`, `Unauthorized`, `NotRequested`, `MissingFixture`, and
  `PositionGuardRejected`;
- orchestration policy diagnostics snapshot precedence version, source order,
  per-source freshness, and position guard config;
- position-derived terminal evidence is guarded by instrument/intent/expected
  delta/strategy-state context plus absence/staleness of direct order/trade
  evidence;
- fatal source errors select operator disarm before ordinary unknown/stale
  decisions;
- stale/conflict/unknown/unauthorized/maintenance/decode-error disarm matrix
  is covered;
- SQLite-backed tests prove orchestration-driven follow-up uses durable
  transition audit after uncertain cancel outcomes;
- source-scan tests guard the truth fetcher boundary against real endpoint
  request specs and endpoint methods.

Still not implemented in M3b-7:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-8 read-only broker-truth boundary status

Implemented while still keeping all broker order endpoints disabled:

- get-order response building checks requested broker/client identity before
  accepting evidence;
- mismatched get-order identity is represented as typed
  `MismatchedOrderIdentity` without raw ids;
- position-derived terminal truth requires actual direct-source attempts rather
  than policy exclusion;
- explicit `CancelBrokerTruthReadonlyFetcher` contract exists above the dry
  mock implementation;
- read-only HTTP/transport error mapping covers 404, 401/403, 429, 5xx,
  timeout, decode error, and dry missing fixture;
- gateway config contains defaulted broker-truth reconciliation policy;
- shadow config can override per-source freshness, precedence, version, and
  position guard policy;
- orchestration reports include policy snapshot and SHA-256 policy fingerprint;
- source scans guard the read-only truth fetcher boundary against order
  endpoint specs/methods and POST/DELETE calls.

Still not implemented in M3b-8:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-9 read-only broker-truth local HTTP mapper status

Implemented while still keeping all broker order endpoints disabled:

- async-aware `CancelBrokerTruthAsyncReadonlyFetcher` contract exists alongside
  the synchronous dry/read-only fetcher contract;
- owned broker-truth fetch request snapshot is available for future async
  network fetchers;
- local HTTP-shaped read-only fixture mapper covers GetOrder, OrdersSnapshot,
  TradesSnapshot, and PositionSnapshot DTOs;
- local fixture diagnostics expose only status, body presence, body length, and
  body SHA-256;
- read-only HTTP policy explicitly maps 408/504 to `Timeout`, 502/503/5xx to
  `Maintenance`, 401/403 to `Unauthorized`, 429 to `RateLimited`, 404 to
  `NotFound404`, and malformed 2xx bodies to `DecodeError`;
- GetOrder truth diagnostics include categorical `identity_strength` without
  raw broker/client ids;
- default broker-truth policy version is `cancel-truth-default-v1`;
- tests prove async trait use, local DTO mapping, decode failure mapping,
  redacted fixture debug, and report redaction.

Still not implemented in M3b-9:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-10 read-only broker-truth local mock transport status

Implemented while still keeping all broker order endpoints disabled:

- redacted GET-only read-only request specs for GetOrder, OrdersSnapshot,
  TradesSnapshot, and PositionSnapshot;
- async local mock read-only HTTP client boundary that records redacted specs
  and returns local fixture responses;
- captured response public surface exposes only source plus redacted HTTP
  diagnostic, not raw body bytes;
- 4xx policy separates `InvalidRequest`, `UnsupportedEndpoint`, and
  `UnknownClientError` from 2xx body `DecodeError`;
- account/instrument scope mismatches are typed and cannot become evidence;
- `ClientOrderIdFallback` is weak by default and requires explicit policy to
  become strong evidence;
- source scans guard the async/local read-only boundary against order endpoint
  specs, POST/DELETE calls, raw response types, generic JSON values, and public
  raw body/response fields.

Still not implemented in M3b-10:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-11 real-readonly broker-truth transport gate status

Implemented while still keeping all broker order endpoints disabled:

- separate `real_readonly_broker_truth_enabled` feature flag, disabled by
  default;
- real-readonly broker-truth gate that blocks unless read-only is explicitly
  enabled and all order/runtime features remain disabled;
- FINAM REST GET route builder separate from local `/readonly/...`
  placeholders;
- route diagnostics are redacted and raw rendered paths are private;
- async real-readonly transport boundary requires the gate marker and returns
  only captured redacted responses;
- instrument identity no longer accepts symbol-only matches;
- `UnknownClientError` policy is operator-visible
  `UnknownPendingOrder`.

Still not implemented in M3b-11:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-12 real-readonly broker-truth transport status

Implemented while still keeping all broker order endpoints disabled:

- GET-only real-readonly FINAM transport behind
  `RealReadonlyBrokerTruthGateApproved`;
- captured raw status/body remains private and maps through the broker-truth
  DTO classifiers before diagnostics/audit export;
- route request parts are crate-private; public diagnostics expose templates,
  query-key names, and id presence/length metadata only;
- trades query policy is bounded by limit/window and uses
  `request.requested_at` as the window end;
- redacted operator guardrails cover gate state, HTTPS base URL, account
  allowlist, timeout, minimum request interval, and disabled order/runtime
  flags;
- SQLite read-only broker-truth audit rows persist status/body hash/fetch
  reason without raw ids, paths, query values, or bodies;
- source-scan coverage proves the real-readonly transport remains GET-only.

Still not implemented in M3b-12:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-13 real-readonly enablement runbook / contract probe status

Implemented while still keeping all broker order endpoints disabled:

- mandatory `RealReadonlyBrokerTruthRunApproved` marker created only from
  approved gate + allowed operator guardrails;
- transport/fetcher construction requires the run marker;
- account hash enforcement blocks mismatched accounts before route rendering /
  send and records redacted failed audit rows;
- redacted transport error categories distinguish DNS/connect, TLS, send,
  body-read, timeout, request-build, and account-not-allowed conditions;
- page-full trades snapshots without exact identity evidence map to
  `TradesSnapshotIncomplete`;
- disabled-by-default contract-probe harness can exercise selected read-only
  sources through the approved fetcher and emit redacted report/audit records;
- SQLite audit rows cover route-build/account-scope failures and transport
  categories without raw ids, paths, query values, URLs, tokens, or bodies.

Still not implemented in M3b-13:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-14 real-readonly contract probe operator harness status

Implemented while still keeping all broker order endpoints disabled:

- one-shot operator-run harness for the real-readonly contract probe;
- mandatory `RealReadonlyBrokerTruthRunApproved` remains required end-to-end;
- request account hash, timeout, and min interval are checked against the
  approved marker before probing;
- bounded source list and `max_requests <= 4`;
- no retry / no background loop / no scheduler flags are required;
- redacted output-location descriptor is required for enabled runs;
- audit-store mode is explicit as ephemeral evidence or persistent audit;
- transport error categories map to operator-action diagnostics;
- `scripts/forbidden_surface_scan.sh` is included in GitHub Actions CI.

Still not implemented in M3b-14:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-15 real-readonly pre-run hardening status

Implemented while still keeping all broker order endpoints disabled:

- exact `.post(` allowlist for broker-finam auth/session/token-details only;
- transport constructor rejects timeout/min-interval mismatches against
  `RealReadonlyBrokerTruthRunApproved`;
- lower-level contract probe helper is no longer public;
- public operator entrypoint remains
  `run_finam_real_readonly_operator_contract_probe(...)`;
- forbidden-surface scan verifies the operator-only entrypoint invariant;
- persistent audit mode remains non-default and requires a separate operational
  policy before use in real probes.

Still not implemented in M3b-15:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-16 real-readonly contract probe evidence gate status

Implemented while still keeping all broker order endpoints disabled:

- approved FINAM base URL length/SHA-256 is part of `RunApproved`;
- real-readonly transport rejects base URL mismatches at construction time;
- token/account preflight diagnostic is redacted and required for operator-run
  probes;
- evidence matrix is emitted for requested read-only sources with route
  template, HTTP/body hash diagnostics, mapped reason/outcome, operator action,
  and audit-record fingerprint;
- operator-run controlled probe blocks persistent audit mode and remains
  ephemeral evidence only.

Still not implemented in M3b-16:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-17 real-readonly evidence package hardening status

Implemented while still keeping all broker order endpoints disabled:

- token readonly/scope shape is included in redacted preflight diagnostics;
- controlled operator probe requires `token_readonly_flag_value == Some(true)`;
- evidence matrix rows are built from per-source attempt records with
  `attempt_id`;
- operator report includes requested source count, actual send count, and max
  request limit;
- tests assert send count stays within max requests and blocked token scope sends
  no requests.

Still not implemented in M3b-17:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-18 real-readonly pre-evidence gate status

Implemented while still keeping all broker order endpoints disabled:

- non-serializable token/account preflight approval marker for the operator
  probe, while keeping the redacted diagnostic as report-only evidence;
- probe-run identity fields: `probe_run_started_at`, `probe_run_id`, and
  `probe_run_fingerprint`;
- fingerprint propagation into attempt records and evidence-matrix rows;
- explicit split between attempted sources, captured responses, actual HTTP
  sends started, and actual HTTP sends completed;
- compatibility `actual_send_count` kept as the started-send count;
- controlled operator runs still block `PersistentAuditStore`.

Still not implemented in M3b-18:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-19 real-readonly request-bound evidence gate status

Implemented while still keeping all broker order endpoints disabled:

- token/account preflight approval marker is bound to the redacted request
  snapshot used at construction time;
- operator run blocks with `TokenAccountPreflightRequestMismatch` if the marker
  does not match the current request;
- operator report includes redacted request snapshot fingerprint, account/order
  / client id hash+length evidence, instrument identity hash+length evidence,
  requested timestamp, and position guard context;
- operator report includes ordered source list and `ordered_sources_sha256`;
- probe-run fingerprint includes request snapshot and ordered source hashes.

Still not implemented in M3b-19:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-20-pre real-readonly pre-run freshness gate status

Implemented while still keeping all broker order endpoints disabled:

- preflight marker diagnostic includes `preflight_checked_at` and
  `preflight_max_age_ms`;
- operator run blocks with `TokenAccountPreflightExpired` before any attempt
  when the marker is stale;
- evidence matrix rows include `actual_http_send_started` and
  `actual_http_send_completed`;
- aggregate actual-send counters remain present for report-level review;
- actual controlled FINAM evidence run remains a separate future artifact.

Still not implemented in M3b-20-pre:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-21-pre real-readonly operator clock gate status

Implemented while still keeping all broker order endpoints disabled:

- enabled operator runs require an explicit `probe_run_started_at`;
- missing enabled-run clock blocks with `ProbeRunClockMissing` before any
  attempts;
- report includes `probe_run_clock_source`;
- report includes `computed_preflight_age_ms`;
- transport-like fixture coverage proves per-row send started/completed flags
  can be true and aggregate counters follow them.

Still not implemented in M3b-21-pre:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-22 controlled real-readonly evidence package status

Implemented while still keeping all broker order endpoints disabled:

- controlled one-shot `finam-real-readonly-evidence` CLI command;
- fresh request-bound token/account preflight marker before any source loop;
- explicit operator-provided probe clock and computed preflight age in the
  report;
- strict single-account and base-URL approval hashes;
- GET-only broker-truth source loop capped by `max_requests <= 4`;
- retry, background loop, and scheduler disabled;
- `EphemeralEvidenceStore` only;
- redacted evidence package written under `reports/finam-real-readonly-evidence/`.

Still not implemented in M3b-22:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-23 real-readonly evidence closeout hardening status

Implemented while still keeping all broker order endpoints disabled:

- self-contained evidence metadata for source commit/archive, broker-cli
  version/build profile, forbidden-surface scan status/script hash, and runbook
  version;
- forbidden-surface scan is executed before FINAM evidence collection;
- per-attempt and actual HTTP-send timing fields in the redacted evidence
  matrix;
- redacted parsed-count summaries for orders, trades, and position snapshots;
- documented GetOrder 200 evidence plan without creating orders.

Still not implemented in M3b-23:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3b-24 / M3c-0 pre-order readiness closeout status

Implemented while still keeping all broker order endpoints disabled:

- GetOrder 200 synthetic real-shape fixture for exact broker order identity;
- GetOrder 200 synthetic real-shape fixture for mismatched identity mapping to
  `MismatchedOrderIdentity`;
- positive reconciliation fixture proving parsed/matched counts and terminal
  truth for exact identity;
- release-profile evidence policy for future gate review;
- FINAM route-template recheck policy for `FinamRestDocs20260701`;
- M3c order endpoint gate design policy and readiness matrix.

Still not implemented in M3b-24:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.

## M3c-0 / M3c-3 order endpoint gate design status

Implemented while still keeping all broker order endpoints disabled:

- explicit `real_order_endpoint_enabled` feature flag, default false;
- `M3cImplementationReviewRequired` endpoint gate blocker;
- `m3c_order_endpoint_gate_design_report()` diagnostic and
  `m3c_order_endpoint_gate_design_report_with_evidence(...)` for
  self-contained review artifacts;
- checklist for operator arming, allowlists, validation guards, SQLite durable
  store, startup unknown-active-order guard, no-blind-retry, manual
  intervention, redacted ACK/export policy, source-scan extension plan,
  release-profile evidence, route-template recheck, and positive GetOrder
  evidence/waiver;
- strict checklist statuses that distinguish design, tested implementation,
  provided evidence, accepted waiver, and blocked state;
- CLI-generated M3c evidence report with forbidden-surface scan status/hash,
  source commit/archive fingerprint, future route allowlist, and negative-test
  plan;
- source archive content-binding against `handoff-commit.txt` inside the
  supplied ZIP;
- explicit evidence/waiver slot status handling for release-profile,
  positive-GetOrder, and route-template-recheck slots;
- negative forbidden-surface harness for injected POST/DELETE/method/route
  bypasses;
- explicit M3c-4 transition plan: approved-only compile trait decision,
  future exact-two-route scanner mode, approved future module path, and
  `EndpointGateApproved` requirement for route rendering and HTTP send;
- M3c-5 crate-boundary decision: `broker-finam` remains request-spec/route
  builder only, while future real HTTP send stays in `finam-gateway` to avoid
  dependency cycles around `EndpointGateApproved`;
- M3c-6 design-only `real_order_endpoint` API shape and scanner transition
  guard, still without HTTP send surfaces;
- M3c-7 gated route-rendering boundary: design/report shape no longer contains
  route templates, while gated route-shape functions require
  `EndpointGateApproved`;
- M3c-8 non-serializable route boundary: route templates remain private
  internal-only design data, exported diagnostics are redacted, and the
  scanner transition guard rejects any `reqwest` token in the design-only
  module;
- M3c-9 approved request-parts boundary: private non-`Debug`/non-serializable
  request parts and rendered path types require `EndpointGateApproved`,
  approved request specs, account/instrument allowlist approval, operator-arm
  approval, and durable-state checkpoint before any future endpoint boundary;
- M3c-10 approved request-parts consumer boundary: private gateway-owned
  consumer accepts only `ApprovedOrderEndpointRequestParts`, requires
  `EndpointGateApproved`, remains no-network/design-only, and exports only
  redacted consumer diagnostics;
- M3c-11 future send result boundary: design-only future outcome/result shape
  records accepted/rejected/timeout/rate-limit/maintenance/auth/decode/transport
  outcomes, operation-specific durable checkpoint labels, single-use
  request-parts policy, no retry after timeout/unknown, and mandatory
  state-machine transition before any future ACK/runtime export;
- M3c-12 outcome state/ACK policy matrix: serializable outcome-to-order-path
  mapping, operation-aware timeout ACK reasons, operator
  disarm/backoff/manual-intervention policy, accepted broker-order-id
  inheritance, no-blind-retry invariant, and private durable checkpoint
  capability marker design;
- M3c-13 transport category and accepted-result classifier design: separates
  true timeout/unknown-pending from non-timeout transport failures, wires
  accepted responses into broker-order-id reconciliation policy, and records
  private checkpoint marker creation from SQLite transition commit proof;
- M3c-14 request-bound checkpoint and captured envelope design: binds commit
  proof to request/client/account/instrument fingerprint, requires single-use
  markers, documents cancel accepted id semantics, and redacts future captured
  response/error evidence to kind/presence/len/hash/category only;
- M3c-15 endpoint attempt journal / HTTP status outcome matrix: binds future
  approved request parts, operation-specific checkpoint marker, captured
  envelope, and outcome classifier through a private endpoint-attempt
  fingerprint, and records Place/Cancel-specific status/body-shape mappings
  that still require state-machine transition before any ACK/runtime export;
- `EndpointGateApproved` remains unconstructible.

Still not implemented in M3c-3:

- FINAM POST/DELETE order endpoint calls;
- real command stream consumer connected to strategies;
- real ACK lifecycle against FINAM endpoints;
- runtime strategy attachment;
- `LiveReady`;
- live micro;
- stop/SLTP/bracket.
