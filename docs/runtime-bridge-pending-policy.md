# Runtime bridge pending and DLQ policy

Status: M2k dry/shadow policy. This document does not permit live trading,
command consumption, order placement, cancel, or trading ACK lifecycle.

## Pending ownership

`runtime-bridge-dry-consume` may claim pending entries only inside the configured
Redis consumer group and only when the operator explicitly supplies
`--claim-stale-ms`.

Allowed dry use cases:

- local reconnect smoke with `--claim-stale-ms 0`;
- replay-grade dry validation after a known crashed dry consumer;
- operator-supervised recovery where the previous consumer is stopped or known
  stale.

Do not run two active dry consumers with the same group unless one is intended
to claim stale work from the other. Consumer names should be stable per process
instance and unique across simultaneously running processes.

For continuous dry runs, choose `claim_stale_ms` higher than the expected max
processing time plus Redis/network jitter. Local smoke may use `0`; production-
like shadow runs should use a conservative value such as `60000` ms or higher
until measured processing latency supports a lower value.

## XAUTOCLAIM backlog cursor

Pending recovery must drain with the Redis `XAUTOCLAIM` cursor:

1. start at `0-0`;
2. process claimed entries through the same dry consumer, DLQ, readiness, and
   `XACK` path as fresh `XREADGROUP` entries;
3. continue with Redis `next_stream_id` while it advances;
4. stop when Redis returns `0-0`, the cursor stops advancing, or no claimed/
   deleted entries are returned.

The dry summary exposes `xautoclaim.last_next_ids` so an operator can see where
the latest cursor pass ended for each stream.

## DLQ handling

Dry DLQ records are terminal for that dry consumer group entry: after a safe DLQ
record is published, the source entry is `XACK`ed to avoid poison-message loops.
The DLQ record contains only redacted metadata and payload length, never raw
Redis payload text.

Operator-facing summary fields:

- `dlq.latest_reason`;
- `dlq.latest_ts`;
- `dlq.latest_stream`;
- `dlq.latest_entry_id`;
- `dlq.consecutive_count`.

Stop the dry bridge and investigate instead of continuing if:

- `dlq.consecutive_count` keeps increasing;
- the same `latest_reason` repeats after a deploy;
- DLQ appears on broker-truth order snapshots;
- readiness remains `Blocked` after pending backlog is drained.

## Durable watermark and dedupe decision

M2k keeps dry replay state process-local except Redis consumer-group state. This
is enough for shadow validation, but not enough for a real runtime bridge.

Before a real runtime bridge or live order lifecycle is allowed, choose and
implement a durable watermark/dedupe store for:

- last processed stream id per broker-neutral stream and consumer group;
- market-data bar keys already accepted by the runtime bridge;
- command/request/client-order/broker-order id mapping.

Until that durable store exists, dry readiness may report `DryReady`, but it
must not publish `LiveReady` and must not arm live trading.
