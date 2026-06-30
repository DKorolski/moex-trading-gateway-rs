# Runtime bridge dry contract

M2f prepares the runtime-consumer side of the broker-neutral stream contract
without connecting strategies and without enabling live order paths. M2g hardens
that dry contract before a Redis consumer runner is introduced.

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

## What M2f/M2g deliberately do not do

The dry consumer contract does not:

- read or process command streams;
- produce command ACKs;
- place or cancel broker orders;
- adapt or invoke strategy runtime code;
- publish `LiveReady`;
- implement durable order-id mapping;
- implement stop/SLTP/bracket behavior.

## Gates before real runtime bridge

Before attaching a strategy runtime:

1. execute the FINAM bar timestamp/finality golden test;
2. decide whether consumer-side dedupe state must become durable;
3. define a real DLQ stream/storage and retention policy;
4. add Redis `XREADGROUP` integration coverage for multi-stream consumption and
   consumer lag;
5. keep `LiveReady` blocked until broker-truth snapshots, market-data readiness,
   schedule, operator arm, and id mapping gates are all satisfied.
