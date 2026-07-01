# Redis stream contract

M2 uses Redis Streams only for shadow/read-only publication. It does not define
or enable an order command stream.

## Payload shape

Every gateway publication uses one Redis stream entry with a single field. M2
may include approximate `MAXLEN` trimming:

```text
XADD <stream> [MAXLEN ~ <n>] * payload <json>
```

The `payload` value is a JSON `broker-core::Envelope<T>`:

```json
{
  "schema_version": 2,
  "ts_utc": "2026-06-29T09:10:00Z",
  "source": "finam-gateway",
  "msg_type": "Readiness",
  "payload": {}
}
```

The envelope is broker-neutral even when the publisher is FINAM-specific.
Runtime consumers must validate `schema_version = 2` before using the payload.

Market-data payloads include `source_kind`:

- `HistoricalPoll` for REST/history bar polling;
- `ReadOnlyPoll` for REST/latest quote or trade polling;
- `LiveStream` reserved for future streaming feeds;
- `Recovery` reserved for future recovery/replay flows.

Order snapshot payloads must not expose raw broker-native comments. A mapped
order can carry `comment_fingerprint` with length and SHA-256 when a comment was
present, but `comment` is cleared before broker-neutral snapshot publication.
Runtime consumers must treat raw comments as unavailable in Redis streams.
The M2f dry consumer rejects any externally supplied `OrderSnapshot` payload
that still contains a raw `comment` value.

## Stream names

The source defaults remain FINAM-namespaced for local safety:

```text
finam:health
finam:readiness
finam:portfolio
finam:orders:snapshot
finam:market-data
finam:command-acks
finam:runtime-bridge:dlq
```

For a broker-neutral runtime bridge deployment, stream names are configured
explicitly. The recommended deployment names are:

```text
broker.health
broker.readiness
broker.portfolio.snapshot
broker.orders.snapshot
broker.market_data
broker.command_acks
broker.runtime_bridge.dlq
```

See `config/finam-gateway-shadow.example.json` for a safe synthetic example.

## Message types in M2

Allowed:

- `Health`;
- `PortfolioSnapshot`;
- `OrderSnapshot`;
- `Readiness`;
- `MarketData`.

Not allowed in M2:

- command consumer streams;
- command ACK lifecycle for real orders;
- order placement/cancel streams;
- stop/SLTP/bracket streams.

## Publication order

`finam-gateway-shadow-once` and each `finam-gateway-shadow-loop` iteration
publish:

1. health;
2. portfolio snapshot;
3. order snapshot;
4. market data events from read-only quote/bars endpoints;
5. readiness.

Readiness is intentionally published last, after broker-truth snapshots and
market-data publication. In M2 the readiness phase may reach
`Reconciliation`, but it must not become `LiveReady`.

If a shadow-loop iteration fails after Redis is available, the runner attempts
to publish:

- `GatewayHealthStatus::Degraded`;
- `ReadinessPhase::Degraded`;
- the best broker-neutral `ReadinessReason` for the failed stage.

On graceful loop shutdown, the runner publishes stopped health/readiness.

## Historical bar watermark

`finam-gateway-shadow-loop` currently keeps a producer-side, in-process
watermark for historical bars keyed by:

```text
venue_symbol|timeframe|open_ts
```

This is a low-noise publication heuristic for the current historical-polling
producer. It is intentionally narrower than the runtime/durable key because the
M2 shadow loop publishes only one historical polling source. Within one process,
repeated polling of the same lookback window does not publish duplicate
historical bar events. The loop reports `bars_deduped_count` and cumulative
`deduped_bar_count` in its summary metrics.

This is still shadow-mode only. Before runtime bridge consumption, decide
whether dedupe remains producer-side, consumer-side, or both, and whether the
watermark must become durable.

The planned runtime/durable contract key before live runtime consumption is:

1. keep the producer-side watermark for low-noise publication;
2. persist the latest accepted bar key per
   `(source, source_kind, venue_symbol, timeframe_sec, open_ts, is_final)` in
   Redis or another gateway-local durable store;
3. make the runtime bridge idempotent by rejecting already-seen bar keys even if
   the gateway restarts;
4. preserve a recovery path that can replay a bounded historical window with
   `source_kind = Recovery` without being confused with fresh live data.

M2d keeps producer-side watermarking in-process with the narrow heuristic key.
M2f/M2g add dry consumer-side dedupe and refine the runtime key shape. M2h/M2k
keep the runtime-side watermark in-memory inside the dry consumer; durability
is a separate M3/M4 gate because it affects replay, recovery, and operator
incident handling. The current design decision is:

- producer-side in-process dedupe reduces Redis noise;
- consumer-side in-process dedupe makes the dry bridge idempotent within one
  runner process;
- durable dedupe storage must be added before real strategy runtime attachment
  if the bridge must survive restarts without replaying already-consumed bars.

## Redis smoke

Local Redis round-trip smoke:

```bash
scripts/redis_shadow_smoke.sh
scripts/runtime_bridge_dry_smoke.sh
```

or directly:

```bash
cargo run -p broker-cli -- finam-gateway-redis-smoke \
  --redis-url redis://127.0.0.1:6379/ \
  --stream finam:smoke
```

The CLI smoke publishes a synthetic `Health` envelope through the same
`RedisConnectionStreamSink` used by the shadow runner. It then:

- reads the latest entry back with `XREVRANGE`;
- reads a consumer-style entry with `XREAD`;
- decodes the payload as typed `Envelope<GatewayHealth>`;
- verifies `schema_version = 2` and `msg_type = Health`.

The runtime-bridge dry smoke publishes a full synthetic broker-neutral stream
set and verifies the dry `XREADGROUP` consumer path. It covers:

- positive path: five valid payloads, accepted count = 5, Redis `XACK` = 5,
  DLQ = 0, readiness simulator = `DryReady`;
- negative paths: invalid JSON, message-type mismatch, unsupported schema
  version, missing payload field, typed decode failure, and raw order comment;
- reconnect path: multiple delivered-but-unacked entries are recovered with
  cursor-based `XAUTOCLAIM`, processed, and `XACK`ed;
- retention path: multiple bad entries are published to a bounded runtime-
  bridge DLQ stream and exact DLQ `MAXLEN` is verified.

Unit-level stream contract tests also decode the allowed M2 shadow payloads as
typed envelopes:

- `Envelope<GatewayHealth>`;
- `Envelope<BrokerReadiness>`;
- `Envelope<PortfolioSnapshot>`;
- `Envelope<OrderSnapshot>`;
- `Envelope<MarketDataEvent>`.

Command, command-ACK, placement, cancel, stop, SLTP, and bracket payloads remain
outside the allowed M2 stream contract.

## Dry runtime bridge consumer contract

M2f introduces a dry consumer contract in `finam-gateway`, and M2g hardens its
diagnostics/dedupe rules. M2h adds a dry Redis `XREADGROUP` runner around the
same contract. The dry consumer is not attached to the strategy runtime. It
accepts stream entries in the same shape a Redis `XREAD`/`XREADGROUP` reader
would produce:

```text
stream
entry_id
payload
```

For each entry it:

1. maps the stream name to the expected `MessageType`;
2. parses the payload as JSON without exposing raw payload in errors;
3. validates `schema_version = 2`;
4. validates that envelope `msg_type` matches the stream;
5. typed-decodes the envelope payload;
6. rejects raw order comments in `OrderSnapshot`;
7. dedupes bars by
   `(source, source_kind, venue_symbol, timeframe_sec, open_ts, is_final)`;
8. emits either `Accepted`, `DuplicateBar`, or `DeadLetter`.

Dead-letter reasons are classified without storing raw payload text:

- unknown stream;
- invalid JSON;
- missing schema version;
- unsupported schema version;
- missing message type;
- unsupported message type;
- message type mismatch for the stream, with expected and actual known message
  types;
- typed decode failure, with expected payload kind;
- missing `payload` field in the Redis stream entry;
- raw order comment present.

The dry consumer metrics are:

- entries seen;
- accepted count;
- duplicate bar count;
- DLQ count;
- per-payload-kind counts for health, readiness, portfolio snapshot, order
  snapshot, and market data.

`runtime-bridge-dry-consume` adds Redis-runner metrics:

- `XREADGROUP` iteration count;
- cursor-based `XAUTOCLAIM` iteration, claimed-entry count, deleted-id count,
  and last returned cursor per stream when pending recovery is enabled;
- returned-entry count;
- last seen Redis id per stream;
- `XPENDING` count per stream;
- oldest pending idle time per stream;
- stream length per stream;
- Redis `XACK` count;
- DLQ publication count;
- latest DLQ reason/timestamp/stream/entry id;
- consecutive DLQ count;
- missing-payload count.

Redis `XACK` here means only that a dry consumer group has processed the stream
entry. It is not a broker command ACK and it is not an order lifecycle event.

Consumer groups support two operator modes:

```text
Tail mode:      --group-start-id '$'  # only entries published after group creation
Backfill mode:  --group-start-id 0    # existing stream history from the beginning
```

The CLI default is tail mode. Use backfill mode for replay-grade dry validation
when streams were populated before the consumer group was created.

The runner also emits a dry readiness-simulator decision. The decision can be
`WaitingForInputs`, `DryReady`, `Degraded`, or `Blocked`, but it always keeps
`live_ready = false`. It is an observability aid for the future runtime bridge,
not an arming signal.

When a dead letter is produced, the runner publishes a safe
`RuntimeBridgeDlqRecord` to the configured DLQ stream. The DLQ payload includes
schema version, timestamp, gateway source, consumer group, consumer name, and
the redacted dead-letter fields. It does not store raw Redis payload text.
The source entry is then `XACK`ed by the dry consumer group to prevent poison
loops; repeated DLQ bursts should stop the dry bridge for operator review.
The dry runtime-bridge DLQ publisher uses exact `MAXLEN = <n>` trimming so the
DLQ bound is testable and enforceable.

Pending ownership and recovery rules for `XAUTOCLAIM` are defined in
`docs/runtime-bridge-pending-policy.md`.

This is still not a live runtime bridge. It does not publish `LiveReady`, does
not consume command streams, does not produce command ACKs, does not call
strategies, and does not arm live trading from simulator output.

M3a-5 adds a separate mock-only dry ACK publisher in `finam-gateway`. It
publishes `CommandAck` envelopes to the configured ACK stream only while live
command/order/cancel features are disabled, and it redacts optional
client/broker order ids before Redis publication. This does not connect the dry
runtime bridge to strategy command streams and does not authorize FINAM
POST/DELETE order endpoints.

M3a-6 keeps runtime-facing ACKs redacted as the selected direction for future
real ACK work. Full client/broker id correlation belongs to the protected
durable mapping store and broker-truth reconciliation path.

M3a-7 extends that rule to dry cancel ACKs and accepted-without-broker-id
ambiguity: Redis ACKs may say `UnknownPending` / `ReconciliationRequired`, but
must not expose raw client or broker order ids.

## Retention policy

M2c defaults use approximate Redis stream trimming for health/readiness/
snapshot/market-data streams. The runtime-bridge DLQ uses exact trimming in the
dry consumer.

```text
health: 1000
readiness: 1000
portfolio snapshots: 1000
order snapshots: 1000
market data: 10000
command ACKs: 1000
runtime bridge DLQ: 1000
```

These values are configurable through `config/finam-gateway-shadow.example.json`.
Set a value to `0` only in local experiments if unbounded retention is
intentionally required.

In-memory stream retention is covered by unit tests. Redis approximate
`MAXLEN ~ <n>` remains for the gateway publication streams. Runtime-bridge DLQ
exact `MAXLEN = <n>` is covered by a retention stress check in
`scripts/runtime_bridge_dry_smoke.sh`.
