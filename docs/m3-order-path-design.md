# M3 order-path design — MARKET/LIMIT/CANCEL

Status: M2m design only. This document defines the intended M3 micro order path
but does not enable FINAM order placement, cancel, command stream consumption,
real ACK lifecycle, strategy runtime attachment, `LiveReady`, stop/SLTP, or
bracket behavior.

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
  -> Submitted / TimeoutUnknownPending / Rejected
  -> broker-truth order/trade reconciliation
```

Required preflight checks:

- operator arm is active for order endpoints;
- account is allowlisted and matches loaded broker-truth state;
- instrument is allowlisted and mapped to FINAM symbol/MIC;
- order type is `Market` or `Limit`;
- limit order has valid limit price;
- qty is positive and within max contracts;
- side is allowed by operator mode;
- market schedule permits entry;
- instrument params/tick/lot are loaded;
- margin/free-cash guard passes;
- no unknown active orders for the account/symbol;
- durable id mapping does not already contain a conflicting request;
- rate limiter has capacity;
- readiness is live-order eligible.

`client_order_id` must be generated before network submission and must satisfy
FINAM's outgoing id limit.

## Cancel flow

```text
BrokerCommand::CancelOrder
  -> schema/type validation
  -> operator arm/preflight validation
  -> durable cancel intent write
  -> local ACK Accepted
  -> FINAM DELETE /v1/accounts/{account_id}/orders/{order_id}
  -> Submitted / TimeoutUnknownPending / Rejected
  -> broker-truth terminal-state reconciliation
```

Cancel requires a known `BrokerOrderId`. If the order is already terminal by
broker truth, the cancel command should be acknowledged as recovered/terminal
without calling FINAM.

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

## Durable mapping store

The first M3 implementation task should be a durable local store. It may start
as SQLite or Redis with atomic semantics, but the contract is independent of the
storage engine.

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
| invalid qty | `Rejected`, no endpoint call |
| invalid limit price | `Rejected`, no endpoint call |
| account not armed | `Rejected`, no endpoint call |
| duplicate client order id | `Duplicate`, no endpoint call |
| broker rejection | `Rejected`, durable state updated |
| place timeout | `Timeout`/`UnknownPending`, retry blocked |
| recovered by client order id | `Recovered` |
| cancel active known order | `Accepted` then `Submitted` |
| cancel terminal order | `Recovered` or safe `Rejected`, no duplicate risk |
| partial fill before snapshot | fill comes from trade event, not ACK |
| trade before order snapshot | reconciliation creates/updates order owner |
| reconnect replay | no duplicate fill/ACK |
| rate limit | disarm or backoff, no blind order retry |

Integration tests before real micro:

- command stream synthetic consumer with FINAM endpoints disabled;
- durable mapping restart/replay test;
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
