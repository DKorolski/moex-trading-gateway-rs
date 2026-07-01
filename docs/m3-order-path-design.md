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
does not call FINAM endpoints and is not a live command consumer.

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
