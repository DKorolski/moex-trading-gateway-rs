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
```

For a broker-neutral runtime bridge deployment, stream names are configured
explicitly. The recommended deployment names are:

```text
broker.health
broker.readiness
broker.portfolio.snapshot
broker.orders.snapshot
broker.market_data
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

`finam-gateway-shadow-loop` keeps an in-process watermark for historical bars
keyed by:

```text
venue_symbol|timeframe|open_ts
```

Within one process, repeated polling of the same lookback window does not
publish duplicate historical bar events. The loop reports `bars_deduped_count`
and cumulative `deduped_bar_count` in its summary metrics.

This is still shadow-mode only. Before runtime bridge consumption, decide
whether dedupe remains producer-side, consumer-side, or both, and whether the
watermark must become durable.

The planned durable strategy before live runtime consumption is:

1. keep the producer-side watermark for low-noise publication;
2. persist the latest accepted bar key per
   `(source, source_kind, venue_symbol, timeframe_sec, open_ts, is_final)` in
   Redis or another gateway-local durable store;
3. make the runtime bridge idempotent by rejecting already-seen bar keys even if
   the gateway restarts;
4. preserve a recovery path that can replay a bounded historical window with
   `source_kind = Recovery` without being confused with fresh live data.

M2d keeps producer-side watermarking in-process. M2f/M2g add dry consumer-side
dedupe and refine the key shape, but intentionally do not persist it until the
runtime bridge runner/storage contract is reviewed.

## Redis smoke

Local Redis round-trip smoke:

```bash
scripts/redis_shadow_smoke.sh
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
diagnostics/dedupe rules. The dry consumer is not attached to the strategy
runtime. It accepts stream entries in the same shape a Redis
`XREAD`/`XREADGROUP` reader would produce:

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
- raw order comment present.

The dry consumer metrics are:

- entries seen;
- accepted count;
- duplicate bar count;
- DLQ count;
- per-payload-kind counts for health, readiness, portfolio snapshot, order
  snapshot, and market data.

This is still not a live runtime bridge. It does not publish `LiveReady`, does
not consume command streams, does not produce command ACKs, and does not call
strategies.

## Retention policy

M2c defaults use approximate Redis stream trimming:

```text
health: 1000
readiness: 1000
portfolio snapshots: 1000
order snapshots: 1000
market data: 10000
```

These values are configurable through `config/finam-gateway-shadow.example.json`.
Set a value to `0` only in local experiments if unbounded retention is
intentionally required.

In-memory stream retention is covered by unit tests. Redis approximate
`MAXLEN ~ <n>` is covered by the Redis smoke path and should receive a dedicated
integration test before a long-running production shadow deployment.
