# M3 order-path design — MARKET/LIMIT/CANCEL

Status: M3a dry/non-network implementation in progress. This document defines
the intended M3 micro order path, but current code still does not enable FINAM
order placement, cancel, command stream consumption, real ACK lifecycle,
strategy runtime attachment, `LiveReady`, stop/SLTP, or bracket behavior.

M3a implementation status: the broker-neutral non-network foundation lives in
`broker-core::order_path`. M3a-1 added the order-path state machine, in-memory
store specification for duplicate-id tests, outgoing comment policy, operator
arming TTL/one-shot checks, and place-order preflight validation. M3a-2 adds the
storage trait, JSON-file durable test backend, cancel preflight, reference
price/notional/slippage guards, and synthetic ACK construction. M3a-3 hardens
cancel mapping, raw command-comment rejection, store update invariants, tick
scale tests, and limit-band boundary tests. M3a-4 adds broker-order-id
uniqueness, cancel timeout/no-blind-retry states, safe ACK reason codes, and dry
FINAM place/cancel request builders without HTTP send. M3a-5 adds
preflight-approved request-builder markers and mock-only redacted ACK
publication. M3a-6 adds an approved-only mock execution client, dry execution
simulator, no-blind-retry simulator tests, operator re-arm workflow tests, and
dry window/backoff rate-limit policy. M3a-7 adds
accepted-without-broker-id reconciliation policy, dry cancel execution
simulation, cancel no-blind-retry tests, and the SQLite/WAL implementation
ticket. M3a-8 adds a dry recovery-by-client-order-id helper, cancel accepted
broker-id mismatch policy, source-scan boundary tests, and an
ACK/reconciliation state matrix. M3a-9 adds idempotent recovery, a
SQLite/WAL durable-store prototype, and workspace-wide source-scan coverage. It
does not call FINAM endpoints and is not a live command consumer. M3a-10 hardens
that SQLite path with writer-lock metadata/stale-lock policy, schema-version
guard, read-only diagnostics, transition audit, operator store-failure disarm
signals, and SQLite-backed dry simulator ordering tests. M3a-11 adds WAL/SHM
runtime-file permission hardening, operator-only diagnostic API names, safe
transition audit event-name refinement, store-error-to-disarm mapping, an
explicit pre-endpoint gate decision, and the migration/fixture runbook. M3b-0
adds the endpoint gate marker design, synthetic/redacted response fixtures,
future real transport signature requiring the marker, SQLite runtime-directory
deployment inspector, and transition-audit contract matrix. M3b-1 routes those
fixtures through the order-path state machine and redacted ACK/disarm policy
without adding real endpoint transport. M3b-2 adds local/mock HTTP-shaped
endpoint response classification and proves post-network decode/map errors are
recorded after durable `BeginSubmit`/`RequestCancel`. M3b-3 redacts internal
endpoint results and makes local status classification place/cancel-aware.
M3b-4 hardens the mock/classified transport boundary, keeps accepted response
DTOs/fixtures out of serde export paths, and models dry cancel reconciliation
follow-up after uncertain 404/409/410 responses. M3b-5 defines the
broker-truth source/classification contract for that follow-up, including
redacted diagnostics and stale/unknown truth operator disarm policy. M3b-6 adds
source-specific freshness policy, dry builders for get-order/orders/trades/
position evidence, precedence selection, conflict detection, and
`ReconciliationConflict` operator disarm while keeping all endpoint and runtime
paths disabled. M3b-7 adds the dry broker-truth fetch orchestration simulator:
typed source missing/error reasons, mock get-order/orders/trades/position
fetchers, policy snapshot diagnostics, guarded position-derived truth, and
SQLite-backed follow-up audit coverage. M3b-8 hardens the future read-only
broker-truth boundary with checked get-order identity, strict direct-source
requirements before position-derived terminal truth, read-only HTTP/error
reason mapping, gateway-config policy overrides, and policy fingerprints.
M3b-9 adds the local HTTP-shaped read-only DTO mapper, async-aware fetcher
contract, explicit 408/502/504 policy, and categorical get-order
identity-strength diagnostics without enabling real order endpoints. M3b-10
adds the GET-only local mock read-only transport boundary, refined 4xx
diagnostics, account/instrument scope checks, and weak-by-default
client-order-id fallback policy. M3b-11 adds a disabled-by-default
real-readonly broker-truth gate and separates FINAM documented GET route
templates from local `/readonly/...` placeholders while keeping real order
endpoints disabled. M3b-12 adds the GET-only real-readonly transport,
captured-response mapping, query policy, operator guardrails, source-scan
GET-only checks, and redacted SQLite audit for read-only broker-truth attempts.
M3b-13 makes that read-only path operator-enableable only with a mandatory
run-approved marker, account-hash enforcement, transport error categories,
disabled contract-probe harness, and page-full trades incomplete semantics.

M3 scope is deliberately small:

- one operator-selected FINAM account;
- one operator-selected instrument at a time for first micro;
- `MARKET`, `LIMIT`, and `CANCEL`;
- no stop/SLTP/bracket;
- no strategy migration until order-path safety is accepted.

## Streams

Recommended broker-neutral stream names:

```text
broker.commands
broker.command_acks
broker.orders.snapshot
broker.trades
broker.readiness
broker.runtime_bridge.dlq
```

M3 command payloads use `Envelope<BrokerCommand>` with `schema_version = 2` and
`msg_type = Command`. ACK payloads use `Envelope<CommandAck>` with
`msg_type = CommandAck`.

M3a-5 implements only a mock/dry ACK publisher. It is allowed to publish
synthetic `CommandAck` envelopes while live command/order/cancel features are
disabled, but it is not a real FINAM ACK lifecycle and is not connected to
strategy command streams.

M3a-6 fixes the ACK contract direction: runtime-facing ACKs remain redacted,
and full broker/client id correlation belongs to the durable mapping store plus
broker-truth reconciliation. See
`docs/m3a6-execution-simulator-decisions.md`.

M3a-7 adds one important ambiguity rule: if a place call is accepted but does
not return a broker order id, the runtime-facing ACK is `UnknownPending` with
`ReconciliationRequired`, not `Submitted`. Cancel is blocked until broker truth
recovers the broker order id.

M3a-8 proves the recovery happy path: broker truth may resolve
`client_order_id -> broker_order_id`, set the broker id once, transition to
`RecoveredByClientOrderId`, and only then allow cancel preflight. It also
defines cancel accepted response policy: a returned broker order id must match
the mapped cancel id; a mismatch requires manual intervention.

M3a-9 makes repeated broker-truth polling idempotent: the same
`client_order_id -> broker_order_id` fact is a no-op success after recovery,
while a different broker id for the same client id remains a reconciliation
mismatch. The SQLite/WAL prototype proves the durable-store direction without
authorizing live endpoint use.

M3a-10 strengthens that proof: dry place/cancel simulator tests use SQLite as
the backing store and a read-only diagnostic connection inside the mock
execution client. This verifies that `SubmitInFlight` and `CancelRequested` are
committed before any future external endpoint call would be attempted.

M3a-11 keeps the external endpoint path blocked, but makes the boundary
operator-visible in code: real endpoint gate decisions are always blocked by
`M3a11PreEndpointReviewRequired`, and runtime-facing ACK id policy remains
`RedactedRuntimeAckOnly`.

M3b-0 adds the compile-time shape of the future real transport without adding
transport implementation: endpoint methods require `EndpointGateApproved` and
FINAM request specs. The marker cannot be obtained from the current blocked
decision.

M3b-1 adds a dry endpoint response integration simulator. It accepts
synthetic/redacted `FinamOrderEndpointFixture` values plus preflight-approved
commands, persists the same local order-path transitions future transport would
need, and returns a redacted integration report. It still does not send FINAM
HTTP and does not consume live strategy commands.

M3b-2 adds local HTTP response classification without real broker calls. It
maps local status/body outcomes such as 401/403, 429, 500/503, timeout,
malformed JSON, and empty broker-order-id cases. The gateway helpers persist
`BeginSubmit` or `RequestCancel` first, then classify the response, then record
safe ACK/disarm state.

M3b-3 refines that local classifier: internal mapped/classified endpoint
results are not serde export objects and their debug output is redacted.
Cancel-specific 404/409/410 responses require reconciliation instead of being
treated as ordinary broker rejection. Body-read failure is modeled as a
post-begin decode error.

M3b-4/M3b-5/M3b-6 model cancel reconciliation after uncertain cancel outcomes:
single-source truth maps terminal/still-working/unknown to the guarded
follow-up matrix, and multi-source truth selects only fresh known evidence.
Fresh terminal-vs-still-working disagreement is treated as a broker-truth
conflict and disarms the operator instead of choosing a side silently.
M3b-7 adds the dry orchestration layer above those classifications: source
fetch reasons are typed, fatal source errors have explicit operator disarm
policy, and position-derived terminal truth is downgraded unless the guard
context proves it is safe to use.
M3b-8 adds the read-only boundary rules that a future real FINAM truth fetcher
must obey: get-order evidence must match the requested identity, source errors
map to typed reasons, and position truth cannot compensate for skipped direct
order/trade sources.
M3b-9 adds the local/read-only HTTP fixture mapper and async-facing fetcher
contract so future real read-only network code can be implemented without
blocking-runtime ambiguity or raw HTTP body leakage.
M3b-10 adds redacted GET-only request specs and a local mock read-only HTTP
client boundary while keeping real order endpoints disabled. It also separates
invalid read-only requests from decode errors and prevents wrong-account or
wrong-instrument truth from becoming evidence.
M3b-11 adds the disabled-by-default real-readonly route gate: FINAM GET route
templates are rendered only behind an explicit read-only gate, raw paths stay
private, symbol-only instrument matches no longer pass, and unknown client
errors are operator-visible unknown-pending outcomes.
M3b-12 adds the first executable real-readonly transport path while keeping
order endpoints disabled: GET responses are captured privately, typed through
broker-truth mappers, audited with status/body hash only, and guarded by
HTTPS/account-allowlist/timeout/rate-limit diagnostics.
M3b-13 requires those guardrails to produce a run marker before transport or
fetcher construction, blocks account mismatches before send, records failed
read-only attempts in redacted SQLite audit, and prevents full trades pages from
becoming strong absence evidence.
M3b-14 wraps the contract probe in a one-shot operator harness with explicit
source/max-request bounds, single-account hash enforcement, no retry/loop
semantics, redacted output-location evidence, audit-mode declaration, transport
error action taxonomy, and CI forbidden-surface scanning.
M3b-15 tightens the pre-run gate by narrowing `.post(` CI allowlisting to exact
auth/session paths, binding real-readonly transport config to `RunApproved`, and
making the lower-level probe loop internal so operator code uses the bounded
operator entrypoint.
M3b-16 adds the final evidence-gate preparation: `RunApproved` also binds the
FINAM base URL hash, operator-run probes require redacted token/account preflight,
and reports include a redacted evidence matrix for the requested GET-only
broker-truth sources.
M3b-17 hardens the evidence package with token readonly/scope diagnostics,
per-source attempt records for matrix alignment, and explicit requested/actual
send counters.
M3b-18 converts token/account approval into a non-serializable marker, adds
probe-run id/fingerprint correlation, and splits captured-response volume from
actual HTTP send started/completed counts.

The command consumer must reject unsupported commands without touching FINAM
order endpoints.

## Place order flow

```text
BrokerCommand::PlaceOrder
  -> schema/type validation
  -> operator arm/preflight validation
  -> durable intent + id mapping write
  -> local ACK Accepted
  -> FINAM POST /v1/accounts/{account_id}/orders
  -> Submitted / SubmittedPendingBrokerOrderId / TimeoutUnknownPending / Rejected
  -> broker-truth order/trade reconciliation
```

Required preflight checks:

- operator arm is active for order endpoints;
- account is allowlisted and matches loaded broker-truth state;
- instrument is allowlisted and mapped to FINAM symbol/MIC;
- order type is `Market` or `Limit`;
- raw `PlaceOrder.comment` is absent at the broker-neutral command boundary;
- limit order has valid positive limit price and loaded tick/price step;
- qty is positive and within max contracts;
- side is allowed by operator mode;
- market schedule permits entry;
- instrument params/tick/lot are loaded;
- market orders have a fresh reference price for notional guards;
- limit orders are inside the configured reference-price deviation band when
  that guard is enabled;
- order notional and run notional remain inside configured bounds;
- margin/free-cash guard passes;
- no unknown active orders for the account/symbol;
- durable id mapping does not already contain a conflicting request;
- rate limiter has capacity;
- readiness is live-order eligible.

`client_order_id` must be generated before network submission and must satisfy
FINAM's outgoing id limit.

If a broker-side accepted/submitted response does not include a broker order
id, M3a-7 treats it as `SubmittedPendingBrokerOrderId` and requires
broker-truth reconciliation before cancel.

## Outgoing FINAM comment policy

First micro default: no outgoing FINAM `comment`.

If an operator explicitly enables comments later, the only allowed format is a
sanitized deterministic diagnostic string:

```text
strategy=<short_strategy_id>;intent=<intent_class>;cid=<client_order_id>
```

Policy:

- max length is configured and enforced before endpoint use;
- invalid values are rejected, not truncated;
- only ASCII alphanumeric, `_`, and `-` are allowed in strategy/intent tokens;
- forbidden content includes account ids, operator names, secrets, JWT/token
  text, raw broker order ids, and raw strategy state;
- durable mapping stores only a length/SHA-256 fingerprint;
- Redis streams must not expose the raw outgoing comment;
- tests must prove serialization redacts the raw comment value.

Broker-neutral `PlaceOrder.comment` is rejected by preflight. The gateway may
later build an outgoing FINAM comment internally through
`OutgoingOrderCommentPolicy`, but strategy/runtime command streams must not
carry raw broker comments.

## Cancel flow

```text
BrokerCommand::CancelOrder
  -> schema/type validation
  -> operator arm/preflight validation
  -> durable cancel intent write
  -> local ACK Accepted
  -> FINAM DELETE /v1/accounts/{account_id}/orders/{order_id}
  -> Submitted / ManualInterventionRequired / TimeoutUnknownPending / Rejected
  -> broker-truth terminal-state reconciliation
```

Cancel requires a known `BrokerOrderId`. If the order is already terminal by
broker truth, the cancel command should be acknowledged as recovered/terminal
without calling FINAM. M3a dry preflight requires the requested
`BrokerOrderId` to exactly match the existing mapping before classifying a
known active order as submit-ready or a known terminal/local rejected order as
already-terminal. Missing or mismatched mappings are rejected unless an explicit
operator policy permits broker-order-id-only cancel with no mapping.

Ambiguous cancel timeout follows the same no-blind-retry principle as place:
after a cancel request times out, the order path moves to
`CancelTimeoutUnknownPending`; automatic cancel retry is blocked until
broker-truth reconciliation proves terminal/canceled state or an operator-
visible manual-intervention decision is recorded.

If a cancel accepted response includes a broker order id, it must match the
mapped cancel order id. A mismatched returned id is treated as
`ManualInterventionRequired` with an `UnknownPending` ACK because the broker
accepted some cancel-shaped response, but not one that safely matches our
durable mapping.

## ACK publishing rules

ACKs are command-path facts only:

- local validation accepted -> `Accepted`;
- FINAM endpoint returned accepted/submitted -> `Submitted`;
- same `StrategyRequestId`/`ClientOrderId` already exists -> `Duplicate`;
- command TTL elapsed before submission -> `Expired`;
- ambiguous timeout reconciled by broker truth -> `Recovered`;
- validation/operator/readiness/broker rejection -> `Rejected`;
- request timed out -> `Timeout` plus durable `TimeoutUnknownPending` state;
- reconciliation is required and retry is blocked -> `UnknownPending`;
- rate-limit/maintenance/decode-error endpoint fixture -> `Error`,
  `ManualInterventionRequired`, operator disarm, and no blind retry;
- 401/403 local HTTP response -> `Error / Unauthorized`,
  `ManualInterventionRequired`, operator disarm;
- cancel 404/409/410 local HTTP response -> `UnknownPending /
  ReconciliationRequired`, `ManualInterventionRequired`, operator disarm;
- 408/504 local HTTP response -> timeout/unknown-pending and operator disarm
  when an arm is supplied;
- local non-retryable error -> `Error`.

ACKs do not imply fill. The runtime must use normalized orders/trades for fill,
partial fill, terminal state, and PnL.

ACK reasons are structured safe codes, not arbitrary strings. Redis
`broker.command_acks` must not carry raw broker response text, account ids,
raw order ids, secrets, JWTs, or raw payload fragments.
The dry publisher clears optional client/broker order ids before Redis
publication; operators must correlate through `StrategyRequestId` and the local
mapping store.

## Durable mapping store

The first M3 implementation task should be a durable local store. It may start
as SQLite or Redis with atomic semantics, but the contract is independent of the
storage engine.

M3a-1 adds an in-memory store specification only. It proves duplicate
`StrategyRequestId` and duplicate `ClientOrderId` rejection, but it is not the
final durable backend for live order emission.

M3a-2 adds the `OrderPathStore` trait and a JSON-file store implementation used
for dry restart/replay tests. The file backend persists recorded intent and
state updates before any future network step, rebuilds duplicate-id indexes on
open, and intentionally remains a test/local implementation rather than a final
production store choice.

M3a-3 adds dry store update invariants: `ClientOrderId` cannot change,
`BrokerOrderId` cannot be changed or cleared after it is known, terminal states
cannot be overwritten by non-terminal states, and `last_update_ts` cannot
regress.

M3a-4 makes `BrokerOrderId` a unique secondary key across records. Duplicate
broker ids are rejected on insert, update, and JSON-file reopen because cancel
and reconciliation by broker id must never be ambiguous.

M3a-9 adds `SqliteOrderPathStore` as the first dry SQLite/WAL prototype. It uses
WAL, `synchronous=FULL`, `BEGIN IMMEDIATE` write transactions, unique
request/client/broker ids, a sidecar single-writer lock, and redacted exports.
It remains non-network and is not yet wired to any live endpoint path.

M3a-10 adds schema-version startup guard, writer-lock metadata, no automatic
stale-lock removal, cleanup on connection-open failure, read-only diagnostic
store access, transaction audit rows, and store-failure operator disarm signals.
It also documents terminal-record retention/archive policy for the protected
local store.

M3a-11 hardens runtime sidecar permissions for DB/WAL/SHM/lock files when
present, makes raw read-only lookups explicitly operator-only, refines audit
events to safe transition names, and maps store errors to endpoint-disarm
signals.

M3b-0 adds a runtime-directory inspector for future deployment/startup checks:
missing/not-directory paths, group/world-accessible Unix directories, workspace
tree locations, and workspace artifact locations can be flagged before any
endpoint-capable mode is armed.

Primary key:

```text
strategy_request_id
```

Unique secondary key:

```text
client_order_id
```

Optional broker key:

```text
broker_order_id
```

M3a-4 treats this as a unique secondary key once present.

Persisted fields:

- strategy request id;
- client order id;
- broker order id when known;
- command kind;
- account reference/fingerprint;
- instrument;
- side;
- order type;
- quantity;
- limit price;
- time in force;
- created timestamp;
- last update timestamp;
- submit attempt count;
- cancel attempt count;
- current state;
- last ACK status;
- last error kind;
- last reconciliation source.

The mapping write must happen before network submission.

## No-blind-retry sequence

Place order timeout is dangerous because the broker may have accepted the order
even when the client did not receive a response.

Required sequence:

1. persist `IntentRecorded`;
2. move to `SubmitInFlight`;
3. call FINAM once;
4. if response is unknown, move to `TimeoutUnknownPending`;
5. emit `UnknownPending`/`Timeout`;
6. reconcile by `client_order_id`, broker order snapshots, and trades;
7. if found, move to `RecoveredByClientOrderId`;
8. if not found after bounded reconciliation window, require operator-visible
   decision before any resubmit.

Automatic resubmit with the same or a new client order id is not allowed until
broker-truth reconciliation proves the absence of a broker order.

## Rate limit and backoff

Read-only/reconciliation calls may use bounded retry with exponential backoff
and jitter.

Order POST rules:

- no automatic retry after timeout/unknown;
- no retry on validation/broker rejection;
- rate-limit response disarms order submission and schedules reconciliation;
- M3b-1 preserves `retry_after_ms` from rate-limit fixtures in the dry
  integration report for future backoff wiring;
- retry only transport setup before request is known to have reached FINAM, if
  the client can prove no order request was sent.

Cancel rules:

- may retry only when broker order id is known and broker truth still shows an
  active order;
- terminal state wins over cancel retry;
- repeated cancel failures require operator visibility.

## Operator arming and disarm triggers

Required arm inputs:

- account id selected through config/env, not hardcoded;
- instrument allowlist;
- max qty/contracts;
- order type allowlist;
- endpoint arming flag;
- dry/live mode flag;
- operator confirmation timestamp or token;
- preflight summary persisted to logs/artifacts without secrets.

M3a implements TTL/one-shot arm semantics in the broker-neutral domain model.
One-shot arms are consumed after an endpoint-attempt marker; process restart
must require a fresh arm in any future endpoint-enabled runner.

Automatic disarm triggers:

- readiness degraded/stopped/blocked;
- unknown active order;
- command DLQ;
- order-path timeout unknown pending;
- rate-limit burst;
- broker maintenance/schedule closed;
- clock skew over threshold;
- Redis reconnect gap;
- durable store unavailable.

## Fixture and test matrix

Unit/fixture tests before real micro:

| Case | Expected result |
|---|---|
| valid market place | local `Accepted`, synthetic `Submitted` |
| valid limit place | local `Accepted`, synthetic `Submitted` |
| FINAM market/limit request builder | JSON body/path built, no HTTP send |
| invalid qty | `Rejected`, no endpoint call |
| invalid limit price | `Rejected`, no endpoint call |
| missing limit tick/reference data | `Rejected`, no endpoint call |
| stale reference price | `Rejected`, no endpoint call |
| order/run notional exceeds guard | `Rejected`, no endpoint call |
| limit outside reference band | `Rejected`, no endpoint call |
| account not armed | `Rejected`, no endpoint call |
| raw place-order comment | `Rejected`, no endpoint call |
| duplicate client order id | `Duplicate`, no endpoint call |
| broker rejection | `Rejected`, durable state updated |
| place timeout | `Timeout`/`UnknownPending`, retry blocked |
| place accepted without broker order id | `UnknownPending`, cancel blocked |
| recovered by client order id | broker id set once, then `Recovered` |
| cancel active known order | `Accepted` then `Submitted` |
| cancel mismatched broker order id | `Rejected`, no endpoint call |
| cancel accepted with returned broker id mismatch | `UnknownPending`, manual intervention |
| cancel rejected by broker | `ManualInterventionRequired` |
| cancel timeout | `CancelTimeoutUnknownPending`, no blind retry |
| cancel recovered terminal | `Recovered` by broker-truth reconciliation |
| cancel terminal order | `Recovered` or safe `Rejected`, no duplicate risk |
| partial fill before snapshot | fill comes from trade event, not ACK |
| trade before order snapshot | reconciliation creates/updates order owner |
| reconnect replay | no duplicate fill/ACK |
| rate limit | disarm or backoff, no blind order retry |

Integration tests before real micro:

- command stream synthetic consumer with FINAM endpoints disabled;
- durable mapping restart/replay test using the order-path store contract;
- broker-truth reconciliation against redacted read-only fixtures;
- DLQ/rejected command does not call FINAM;
- explicit arming required for every endpoint-using test.

## First live micro checklist

Only after M2m design review and M3 dry order-path tests are accepted:

1. account is funded with intentionally small amount;
2. account is flat or active ownership is known;
3. one instrument is selected;
4. one contract or minimum valid qty;
5. operator confirms schedule is open;
6. market/limit/cancel endpoint arm is enabled for one run;
7. place/cancel is watched live;
8. broker truth reconciles order and any fill;
9. operator disarms immediately after the micro cycle;
10. journal records gross/net outcome and any fees if available.

M3 first live micro success does not authorize stop/SLTP/bracket or strategy
scale-up. Those remain later gates.
