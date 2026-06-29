# Redis stream contract

M2b/M2c use Redis Streams only for shadow/read-only publication. They do not define
or enable an order command stream.

## Payload shape

Every gateway publication uses one Redis stream entry with a single field. M2c
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

## Message types in M2b/M2c

Allowed:

- `Health`;
- `PortfolioSnapshot`;
- `OrderSnapshot`;
- `Readiness`;
- `MarketData`.

Not allowed in M2b/M2c:

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
market-data publication. In M2b/M2c the readiness phase may reach
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

The smoke publishes a synthetic `Health` envelope through the same
`RedisConnectionStreamSink` used by the shadow runner. It then:

- reads the latest entry back with `XREVRANGE`;
- reads a consumer-style entry with `XREAD`;
- decodes the payload as typed `Envelope<GatewayHealth>`;
- verifies `schema_version = 2` and `msg_type = Health`.

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
