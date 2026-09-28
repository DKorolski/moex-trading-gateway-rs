# Stage 8B-P1-c real Redis semantic source and command publication

Status: reviewed at `a85ef845f86f99bcfd45654792cc688240457d3d`;
superseded for restart/group-continuity closure by
`stage8b-p1c-r1-redis-restart-group-continuity.md`.

Accepted predecessor:
`ed6d98cb2bbc70c36e1033c6215d64dd6218cedf` (P1-b closed).

This slice composes the accepted P1-b semantic owner with real Redis Streams.
It does not activate the operational DB 0 stand and does not attach a paper
provider, FINAM transport, broker dispatch or runtime-live capability.

## Implemented boundary

The source path is:

```text
canonical final M10 bytes
  -> exact Redis ID <close_ts_utc_ms>-0
  -> P1 M10 consumer group / PEL
  -> accepted P1-b Hybrid semantic transition
  -> authenticated S1
  -> exact Envelope<BrokerCommand>
  -> canonical Stage 7 command stream
```

Both the M10 group and the Stage 7 command group are created at `0-0` with
`MKSTREAM` and verified before an M10 may be published. Every deployment uses
the fixed accepted P1 namespace and one Redis hash tag.

The M10 Lua operation checks that its consumer group still exists and then
performs an exact-ID `XADD`. A repeated exact ID is accepted only when `XRANGE`
returns the same single `payload` field byte-for-byte. A collision, absent
group or malformed entry fails closed. P1-c performs no `XTRIM` or `XDEL`.

## One-intent publication

P1-b now retains the exact `BrokerCommand` inside its opaque S1 authority.
Only the crate-private P1-c transition can consume the Stage 7 owner, S1
evidence and command together. Public callers cannot extract the command,
Redis connection or durable owner.

Command publication is one Redis Lua transaction. Before `XADD`, it verifies:

- exact source stream, group, PEL entry and M10 bytes;
- exact semantic batch and deterministic `StrategyRequestId`;
- exact canonical command and envelope SHA-256;
- S1 generation and commitment;
- existence of the canonical Stage 7 command stream and consumer group.

The transaction atomically appends the canonical Stage 7A-compatible command
envelope and writes a deterministic request-bound publication marker. The
Redis command entry ID allocated by Redis is permanently bound by that marker;
the business/replay identity remains the deterministic `StrategyRequestId`.
On a lost response, restart reconstructs the exact S1 command and the same Lua
operation returns the existing entry after verifying the marker and command
payload. It cannot append a duplicate.

The source M10 remains pending after command publication. P1-c exposes no
method that can invoke the provider or acknowledge that M10. P1-d will consume
the retained linear authority and may acknowledge the source only after the
accepted downstream settlement and feedback protocol completes.

## Zero-intent and restart behavior

For zero intent, S1 is committed, fsynced and reread before exact source
`XACK`. If the ACK response is lost, restart validates the retained source and
accepts either the exact pending entry or the exact already-acknowledged state.
It never repeats the Hybrid callback.

For a journal-ahead `RequestAccepted`, restart requires exactly one PEL entry,
reclaims it with `XAUTOCLAIM`, deterministically replays the callback, proves
the accepted P1-b identities and commits S1 before publication.

For a valid one-intent S1, restart also requires exactly one matching PEL
entry. Zero, multiple or conflicting entries are not guessed. Reclaim uses a
fresh consumer name, bounded pages and a retained `XAUTOCLAIM` cursor.

## Real Redis evidence

The isolated tests start a temporary loopback `redis-server`, never DB 0, and
cover:

1. group-before-M10 publication, exact replay and collision rejection;
2. zero-intent S1-before-XACK and already-XACKed restart resolution;
3. command response loss with exactly one retained command;
4. journal-ahead PEL reclaim through `XAUTOCLAIM`;
5. rejection when source M10 was acknowledged early;
6. atomic rejection when the Stage 7 command group is absent;
7. tampered publication marker rejection without duplicate command;
8. rejection of an ambiguous multi-entry PEL on S1 restart.

The source checker and mutation harness additionally enforce the command
publication order, both atomic group checks, no command-path `XACK`, no
retention deletion and no provider/FINAM/runtime-live surface.

## Explicitly closed

- operational Redis DB 0 activation;
- a deployable P1 process or systemd unit;
- paper provider invocation and settlement;
- ACK/order/trade/position feedback;
- FINAM HTTP POST/DELETE or any broker network dispatch;
- runtime-live, real orders and unattended execution.

Independent semantic review is required before governance rebinding or any
operational deployment. P1-d remains closed until that review accepts P1-c.
