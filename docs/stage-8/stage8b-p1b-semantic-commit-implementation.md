# Stage 8B-P1-b semantic commit implementation

Status: implementation review candidate.

Accepted predecessors:

- P1-a bootstrap/identity baseline: `6647382bca8950cb1a831cf6082a9f0eacb3bdcc`;
- semantic protocol R1: `952c68924777e8ce65841a30effc08904bfddfa0`;
- journal-ahead addendum R1A: `073ae9f38acd06b7a5febdbfc1e75a7b460adf03`.

This slice attaches one canonical final M10 to the accepted Hybrid semantic
facade and commits the resulting Stage 5G state through the existing Stage 6
journal and Stage 7B recovery seal. It is local/test composition only. It does
not connect to operational Redis, publish commands, invoke a paper provider or
open FINAM transport.

## Canonical M10

`Stage8bP1CanonicalM10EnvelopeV1` is strict JSON with:

- exact P1 broker, strategy, account-bound operational identity and instrument
  mapping;
- final 600-second UTC-aligned OHLCV;
- exactly ten contiguous M1 source identities;
- canonical decimal strings;
- deterministic payload and semantic SHA-256 identities;
- exact Redis-style ID derived from the M10 close in milliseconds.

The in-memory `Stage8bP1LocalM10Stream` models group-before-publication,
exact-ID idempotence/collision, PEL ownership, active-entry retention and
XACK-last without opening a Redis connection. It is not an operational Redis
adapter.

## Single ownership and ordering

`Stage8bP1SemanticCompositionOwner` owns both the sole Stage 7B recovery-ready
authority and the local M10 stream. The transition order is:

```text
exact pending canonical M10
  -> accepted Stage 5C semantic bar
  -> Hybrid callback/timer settlement
  -> deterministic Stage 5G semantic projection
  -> optional RequestAccepted append + journal fsync
  -> authenticated Stage 5G restart package
  -> covering S1 temp write/fsync/rename/directory fsync/reread
  -> typed P1-b result
  -> M10 XACK only for zero-intent
```

For one intent, P1-b stops at
`Stage8bP1SemanticPrepublicationOwner`. The command and source delivery remain
durable/pending, but there is no publication or provider API and no M10 XACK.
P1-c must add and independently accept that boundary.

## Outcomes

- `0 intent`: S1 is committed and reread, then and only then the local M10 is
  acknowledged and the sole composition owner returns Ready.
- `1 intent`: deterministic `RequestAccepted` and S1 are committed; an opaque
  prepublication owner is returned with M10 retained and unacknowledged.
- `>1 intent`: a non-continuable diagnostic is returned; no publication,
  provider or XACK authority is exposed.

The deterministic `RequestAccepted.source_evidence_sha256` binds the source S0
generation, seal/checkpoint/frontier, operational identity, exact M10
identities, semantic batch, strategy request, canonical command and expected
journal record ID.

## Journal-ahead recovery

The only accepted S0-ahead shape is exactly one compatible P1
`RequestAccepted`. It produces `P1SemanticPrepublicationPending`, which is not
Ready and has no publication/provider/XACK authority. Completion requires the
same pending and unacknowledged M10 and must reproduce the exact semantic batch,
request, command bytes/hash, record ID and source evidence before S1 is
committed.

Missing, acknowledged or changed M10, an extra dispatch suffix, an unrelated
request, a malformed seal or arbitrary unbound non-final Stage 6 state fails
closed. A blocked restart remains an explicit blocked diagnostic; it is not
misreported as a positive source-binding mismatch.

## Process crash evidence

The subprocess matrix sends SIGKILL at seven frontiers:

1. before `RequestAccepted` append;
2. after journal fsync;
3. before S1 temp-file fsync;
4. after temp-file fsync and before rename;
5. after rename and before directory fsync;
6. after directory fsync and before reread;
7. after S1 reread and before the still-closed command XADD boundary.

The observed restart states are limited to:

- original S0 `Ready` before append;
- exact typed journal-ahead pending while S0 remains authoritative;
- exact authenticated P1 prepublication when valid S1 is visible.

Temporary files never grant authority.

## Closed surfaces

P1-b does not authorize or implement:

- operational Redis DB 0 activation or a Redis network client;
- `BrokerCommand` XADD or ACK/DLQ publication;
- paper-provider invocation or broker execution;
- FINAM HTTP/WS transport, POST or DELETE;
- runtime-live or real orders;
- P1-c, P1-d, P1-e or P1-f activation.

The existing P0 readonly FINAM stand is unaffected and may remain active.

## Verification boundary

The repository-wide default-feature test profile passes. The P1-b acceptance
gate additionally runs `clippy --all-targets --all-features -- -D warnings` for
the two crates changed by this slice. The repository-wide clippy profile is not
used as P1-b authority because unchanged `finam-gateway` code has three existing
default-feature `dead_code` findings. Likewise, the repository-wide
`--all-features` test profile enables the old `m3j16-actual-one-shot` opt-in and
therefore contradicts that feature's own disabled-marker test. Neither
observation is in the P1-b changed path set, and neither is hidden by the
stage-specific gate.
