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

Consumer-group start mode matters:

```bash
# Tail mode: consume only entries published after the group is created.
cargo run -p broker-cli -- runtime-bridge-dry-consume \
  --config config/finam-gateway-shadow.example.json \
  --group broker-runtime-bridge-dry-tail \
  --group-start-id '$'

# Backfill/replay mode: consume existing stream history from the beginning.
cargo run -p broker-cli -- runtime-bridge-dry-consume \
  --config config/finam-gateway-shadow.example.json \
  --group broker-runtime-bridge-dry-backfill \
  --group-start-id 0
```

The CLI default is tail mode (`$`). If a dry run returns zero entries with
`--group-start-id '$'`, the summary includes an operator hint to use
`--group-start-id 0` for backfill validation.

The runner:

- creates missing consumer groups with `XGROUP CREATE ... MKSTREAM`;
- optionally recovers stale pending entries with cursor-based `XAUTOCLAIM`
  when `--claim-stale-ms` is supplied;
- reads health, readiness, portfolio snapshot, order snapshot, and market-data
  streams with `XREADGROUP`;
- feeds each `payload` field into `RuntimeBridgeDryConsumer`;
- publishes safe dead letters to the configured runtime-bridge DLQ stream;
- records last seen Redis ids, returned-entry count, pending counts, DLQ
  publication count, latest DLQ summary, consecutive DLQ count,
  missing-payload count, and Redis `XACK` count;
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
The dry runtime-bridge DLQ publisher uses exact Redis `MAXLEN = <n>` trimming
for the DLQ stream so the operator-facing DLQ retention bound is enforceable in
stress smoke.

The readiness simulator is deliberately dry. It reports:

- `WaitingForInputs` until health, gateway readiness, portfolio snapshot, order
  snapshot, and market data have all been observed;
- `DryReady` when those shadow inputs are internally consistent;
- `Degraded` for degraded/stopped gateway states;
- `Blocked` for unknown open-order status or dead letters.

The simulator output always contains `live_ready = false`; it is not a runtime
arming mechanism.

M2i/M2j add the integration smoke command used by CI:

```bash
scripts/runtime_bridge_dry_smoke.sh
```

It creates unique synthetic streams, publishes a valid set of Health,
Readiness, PortfolioSnapshot, OrderSnapshot, and MarketData payloads, consumes
them with `--group-start-id 0`, and asserts accepted counts, Redis `XACK`, DLQ
count, and `DryReady`.

It also runs Redis-negative cases for:

- invalid JSON;
- message type mismatch;
- unsupported schema version;
- missing `payload` field;
- typed decode failure after a valid envelope header;
- raw `Order.comment` in an `OrderSnapshot`.

Each negative case must publish a safe DLQ record, avoid raw-payload/comment
leakage, `XACK` the Redis entry, and leave the simulator `Blocked`.

M2j/M2k add reconnect smoke: synthetic entries are delivered to a consumer group
without `XACK`, leaving them in the PEL. A recovered consumer then uses
`--claim-stale-ms 0` / cursor-based `XAUTOCLAIM` to claim, process, and `XACK`
them. M2k uses multiple pending entries with a smaller claim batch to exercise
the backlog cursor path. This is still dry recovery only; it is not a real
strategy runtime replay mechanism.

M2k also adds a DLQ retention stress smoke: it publishes multiple bad entries,
verifies safe DLQ publication and `XACK`, and asserts that the configured DLQ
stream bound is respected.

The dry summary includes:

- `xautoclaim.enabled`;
- `xautoclaim.iterations`;
- `xautoclaim.claimed_entries_returned`;
- `xautoclaim.deleted_ids_count`;
- `xautoclaim.last_next_ids`;
- `xreadgroup.pending_oldest_idle_ms`;
- `xreadgroup.stream_lengths`;
- `dlq.latest_reason`;
- `dlq.latest_ts`;
- `dlq.latest_stream`;
- `dlq.latest_entry_id`;
- `dlq.consecutive_count`.

Pending ownership, safe `claim_stale_ms` selection, repeated-DLQ handling, and
the durable watermark/dedupe decision are documented in
`docs/runtime-bridge-pending-policy.md`.

## What M2f/M2g/M2h/M2i/M2j/M2k deliberately do not do

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
