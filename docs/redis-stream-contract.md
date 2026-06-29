# Redis stream contract

M2b uses Redis Streams only for shadow/read-only publication. It does not define
or enable an order command stream.

## Payload shape

Every gateway publication uses one Redis stream entry with a single field:

```text
XADD <stream> * payload <json>
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

## Message types in M2b

Allowed:

- `Health`;
- `PortfolioSnapshot`;
- `OrderSnapshot`;
- `Readiness`;
- `MarketData`.

Not allowed in M2b:

- command consumer streams;
- command ACK lifecycle for real orders;
- order placement/cancel streams;
- stop/SLTP/bracket streams.

## Publication order

`finam-gateway-shadow-once` publishes:

1. health;
2. portfolio snapshot;
3. order snapshot;
4. readiness;
5. market data events from read-only quote/bars endpoints.

Readiness is intentionally published after broker-truth snapshots. In M2b the
readiness phase may reach `Reconciliation`, but it must not become `LiveReady`.

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
`RedisConnectionStreamSink` used by the shadow runner, then reads the latest
entry back with `XREVRANGE` and verifies `schema_version = 2`.

## Retention policy

No `MAXLEN` trimming is applied in M2b. Retention/maxlen policy must be decided
before always-on runner or runtime bridge rollout.
