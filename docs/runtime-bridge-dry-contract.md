# Runtime bridge dry contract

M2f prepares the runtime-consumer side of the broker-neutral stream contract
without connecting strategies and without enabling live order paths. M2g hardens
that dry contract. M2h wraps it in a dry Redis `XREADGROUP` runner, still
without strategy invocation or live order paths.

## What exists in M2f/M2g

`finam-gateway::RuntimeBridgeDryConsumer` consumes already-published shadow
stream entries represented as:

```text
stream
entry_id
payload
```

The payload must be a `broker_core::Envelope<T>` with `schema_version = 2`.

Allowed stream payloads:

- `GatewayHealth`;
- `BrokerReadiness`;
- `PortfolioSnapshot`;
- `OrderSnapshot`;
- `MarketDataEvent`.

For each entry the dry consumer validates stream/message compatibility, performs
typed decode, checks that order snapshots contain no raw broker comments, and
dedupes bars by:

```text
source|source_kind|venue_symbol|timeframe_sec|open_ts|is_final
```

Including `source_kind` and finality keeps future `HistoricalPoll`, `Recovery`,
and `LiveStream` bars from being collapsed into the same key accidentally.

Outcomes:

- `Accepted`;
- `DuplicateBar`;
- `DeadLetter`.

The consumer records metrics for seen entries, accepted payloads, duplicate
bars, dead letters, and per-payload-kind counts.

## Dead-letter policy

DLQ records include only:

- stream name;
- entry id;
- reason enum;
- payload length.

They do not store raw payload text. Reasons currently include unknown stream,
invalid JSON, missing/unsupported schema version, missing/unsupported message
type, stream/message mismatch, typed decode failure, and raw order comment
presence.

Safe diagnostics included in reasons:

- `MessageTypeMismatch` includes the expected and actual known message types;
- `TypedDecodeFailed` includes the expected payload kind.

## M2h dry Redis runner

`broker-cli runtime-bridge-dry-consume` is a local/shadow runner around the dry
consumer contract:

```bash
cargo run -p broker-cli -- runtime-bridge-dry-consume \
  --config config/finam-gateway-shadow.example.json \
  --group broker-runtime-bridge-dry \
  --consumer dry-consumer-1 \
  --max-iterations 1
```

The runner:

- creates missing consumer groups with `XGROUP CREATE ... MKSTREAM`;
- reads health, readiness, portfolio snapshot, order snapshot, and market-data
  streams with `XREADGROUP`;
- feeds each `payload` field into `RuntimeBridgeDryConsumer`;
- publishes safe dead letters to the configured runtime-bridge DLQ stream;
- records last seen Redis ids, returned-entry count, pending counts, DLQ
  publication count, missing-payload count, and Redis `XACK` count;
- updates `RuntimeBridgeReadinessSimulator` from the same broker-neutral stream
  entries;
- `XACK`s processed Redis entries so dry-run groups do not accumulate pending
  messages.

The DLQ stream entry contains a single redacted JSON payload:

```text
schema_version
ts_utc
source
consumer_group
consumer_name
dead_letter
```

`dead_letter` contains stream, entry id, reason enum, and payload length. It
does not contain raw Redis payload text.

The readiness simulator is deliberately dry. It reports:

- `WaitingForInputs` until health, gateway readiness, portfolio snapshot, order
  snapshot, and market data have all been observed;
- `DryReady` when those shadow inputs are internally consistent;
- `Degraded` for degraded/stopped gateway states;
- `Blocked` for unknown open-order status or dead letters.

The simulator output always contains `live_ready = false`; it is not a runtime
arming mechanism.

## What M2f/M2g/M2h deliberately do not do

The dry consumer contract and dry Redis runner do not:

- read or process command streams;
- produce trading command ACKs;
- place or cancel broker orders;
- adapt or invoke strategy runtime code;
- publish `LiveReady`;
- arm live trading from the dry readiness-simulator output;
- implement durable order-id mapping;
- implement stop/SLTP/bracket behavior.

## Gates before real runtime bridge

Before attaching a strategy runtime:

1. execute the FINAM bar timestamp/finality golden test;
2. decide whether consumer-side dedupe state must become durable;
3. decide whether the dry DLQ stream policy is sufficient for real runtime
   operations or needs durable external storage;
4. add Redis `XREADGROUP` integration coverage for reconnect/replay behavior and
   consumer lag under load;
5. keep `LiveReady` blocked until broker-truth snapshots, market-data readiness,
   schedule, operator arm, and id mapping gates are all satisfied.
