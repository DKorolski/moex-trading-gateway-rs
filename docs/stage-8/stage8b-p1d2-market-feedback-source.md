# Stage 8B-P1-d2 Market feedback source implementation

Status: source implementation review candidate.

Accepted design predecessor:
`0cf1cd810a6ff479b69afb914db3b2aa2259593a` (P1-d2 Projection Annex R1A).

## Scope

This slice implements the first deterministic Market/full-fill feedback path:

```text
retained predecessor M10
  -> exact first canonical successor M10
  -> deterministic P1-d1 Market outcome
  -> durable Stage 6 outcome
  -> durable/reread Stage 7 RequestFinalized
  -> ACK at seq_ack
  -> replacement S_ack persisted/reread
  -> event-scoped broker truth at seq_ack + 1
  -> replacement S_truth persisted/reread
  -> source M10 XACK last
```

The implementation does not activate operational Redis DB 0. Tests use an
isolated disposable Redis process and generated namespace. No FINAM transport,
broker dispatch, runtime-live or real-order path is attached.

## Canonical source and finalized authority

`Stage8bP1RedisCommandPublished::execute_next_canonical_market` consumes an
opaque one-use `Stage8bP1d1ExecutionScheduleAuthority` and reads only the first
Redis entry after the retained predecessor. The authority owns the exact Stage
5E projection produced from normalized schedule, registry and fresh Stage 4
session evidence; canonical Redis bytes cannot mint it. No synthetic
`TradableOpen` window or Redis-local calendar fallback exists. The successor
Redis ID must be exactly the predecessor close plus 600 seconds with sequence
zero. Missing input, non-contiguous input, malformed bytes, identity drift or
schedule rejection fails before provider or feedback effects and leaves the
source M10 pending.

The deterministic P1-d1 outcome is passed through the existing Stage 6/7
writer. The file backend writes, `sync_data()`s and fully rescans every appended
frame. `RequestFinalized` uses `T_receipt`, then the finalized facts are
reconstructed from that rescanned journal. The P1-d2 mint cross-binds:

- durable request identity, account, instrument and attribution;
- strategy request and durable client-order IDs;
- canonical command digest;
- deterministic broker order and trade IDs;
- terminal `Completed` disposition and exact final record/sequence;
- execution-bar identity, quantity, price and both canonical timestamps.

The restart path never invokes the provider again and never reacquires or
reconstructs schedule evidence. The durable P1-specific dispatch proves that
the source-produced schedule authority was consumed before the original
append. A strict journal-ahead classifier accepts only the exact deterministic
P1-d2 suffix and reconstructs the same outcome from Stage 6 facts plus the
retained exact contiguous canonical successor.

## ACK/truth sequence authority

One linear Stage 5G owner reserves the sequence pair. The truth sequence is
never taken from top-level `last_total_sequence` and is never newly allocated
after restart. It is recovered from the exact resolved ACK slot:

```text
seq_truth = checked_add(exact_resolved_ack_slot.seq_ack, 1)
```

Missing, overflowed, stale, reused, reversed, skipped or caller-substituted
values fail closed. The separate pair-allocation SIGKILL test proves that a
crash after allocation but before ACK reconstructs the same pair.

## Replacement seals and phase-linear API

Both durable transitions use `commit_stage8b_p1_replacement_seal`; a plain
outer-generation advance is not used. Each transition embeds a newly exported
Stage 5G package, atomically replaces the Stage 7B seal, syncs the directory,
rereads the committed bytes and cross-validates the recovered runtime against
the new package and current Stage 6 checkpoint.

The accepted annex names `Stage5gCleanRestartSource::OrderPositionAwaiting` as
the available substrate. The implementation uses two non-constructible narrow
source variants, `P1d2Ack` and `P1d2Truth`. Both serialize as the existing
`OrderPositionAwaitingCommitted` lifecycle; they do not add a lifecycle kind.
Their only purpose is to retain the authenticated feedback projection needed
for phase-correct replay:

- `P1d2Ack` carries the resolved ACK state and exposes truth continuation only;
- `P1d2Truth` carries post-truth state and exposes source resolution only.

This is the narrowly generalized equivalent allowed by R1A. It prevents a
generic `OrderPositionAwaiting` package from dropping the P1-d2 source binding,
sequence pair or projected truth.

The public service types preserve the same phase split:

- pre-S_ack owner: reconstructed ACK only; no provider, truth or XACK;
- S_ack owner: truth only; no ACK replay or XACK;
- S_truth owner: exact source XACK only; no ACK or truth replay.

The absent methods are also protected by compile-fail doctests.

## Projection and audit evidence

P1-d2 emits exactly one Filled/Terminal Market order, one trade and one
target-instrument position in an event-scoped `BrokerTruthSnapshot`. The first
source slice requires authenticated flat `pre_position_qty`; it does not
pretend that an absent later paper-book row is flat. Non-flat average prices
use checked Decimal arithmetic, scale 8 and `MidpointNearestEven`; floating
point and wall-clock values are absent.

The authenticated S_truth can produce a redacted, non-authoritative audit
record. It contains only identifiers, canonical timestamps, sequence values,
domain-separated hashes of the P1-d1 outcome, Stage 6 finalized report, order,
trade, position and complete feedback projection, plus the final seal
generation/commitment. It contains no HMAC, key bytes, Redis connection,
writer lease or mutation capability. The audit digest is stable across an
S_truth restart.

## Crash/restart closure

Real subprocess SIGKILL tests cover:

1. after Stage 6 outcome, before `RequestFinalized`;
2. after `RequestFinalized`, before ACK;
3. after ACK in memory, before S_ack;
4. after S_ack, before truth;
5. after truth in memory, before S_truth;
6. after S_truth, before source XACK.

A seventh narrow test stops after sequence-pair allocation and before ACK. The
child fsyncs the allocated pair into a separate test-only crash marker before
the SIGKILL barrier; after restart the parent compares both values exactly
with the final authenticated audit pair. Every case restarts through the
expected typed authority, reaches S_truth, XACKs the exact source once, leaves
PEL empty and keeps exactly one command entry. Before the terminal XACK the
source remains pending.

## Deliberately closed

- operational Redis DB 0 and VPS activation;
- operational paper-provider supervisor execution and schedule-source adapter;
- FINAM POST/DELETE and broker network dispatch;
- runtime-live and real orders;
- LIMIT, cancel, expiry and partial-fill lifecycle (P1-d3);
- current-tree authority rebind before independent P1-d2 acceptance.

Acceptance of this source candidate may authorize only the next documented
P1-d slice. It does not itself open any operational surface.
