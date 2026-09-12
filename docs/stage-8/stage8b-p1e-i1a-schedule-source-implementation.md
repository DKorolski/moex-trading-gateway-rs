# Stage 8B-P1-e I1A schedule-source implementation

Status: `SOURCE_REVIEW_CANDIDATE`.

Accepted design predecessor:
`aa24e840ed8b7d18c80be6f1fdd8f50facf5b6d4`.

## Implemented boundary

The source slice implements the accepted signed schedule path without opening an
operational supervisor or broker execution surface:

```text
FINAM GET-only schedule DTO
  -> exact raw/registry/Stage4 projection
  -> strict signed V3 envelope and durable publisher high-water
  -> XADD NOMKSTREAM MAXLEN = 4096
  -> XREVRANGE + - COUNT 64 (newest candidate, no older fallback)
  -> signature, canonical payload, identity, freshness and progression checks
  -> route-exact Stage 6 V4 schedule_evidence_bound
  -> append + fsync + covering seal + reread/cross-validation
  -> one linear Market / Working / Cancel / Day-expiry authority
```

The schedule stream is not a work queue. The reader has no consumer group, PEL,
XACK, XAUTOCLAIM, repair, trim or deletion authority. The publisher has no FINAM
write endpoint and persists `Prepared` before Redis; response loss can only replay
the exact signed envelope bytes before a durable `Published` state is recorded.

## Semantic and durability guarantees

The production verifier uses the accepted V3 semantic projection and hash domain.
Open to Closed with unchanged sessions changes the semantic hash and increments the
revision. A heartbeat keeps the hash/revision while advancing publication sequence.
A trading-day change advances the semantic revision. Same-revision/different-hash,
rollback, malformed/unknown fields, signature failure, stale evidence, identity
drift and detached Stage4 evidence all fail closed. Closed denies Market and Working,
permits Cancel, and permits Day-expiry only for the exact last eligible M10 boundary.

Stage 6 V4 adds only `schedule_evidence_bound`. It consumes the next global lifecycle
sequence but is not a business terminal or source-XACK boundary. The existing journal
backend performs append/fsync, writes one covering recovery seal, rereads the framed
journal and authenticates the exact record. Journal-ahead recovery accepts only one
exact successor; covered replay reconstructs the same incomplete binding without a
second seal or strategy effect. V1/V2/V3 wire and replay compatibility is pinned to
the accepted predecessor and exercised by inherited golden/replay tests.

## Latches and evidence

I0 latches A-B remain unchanged. I1A adds executable checkpoints C-F before source
read, after verified read/before binding, after non-cancellable binding, and before
authority effect. A stop before binding returns the original owner. A stop after
binding retains the committed V4 owner for exact restart. Clear latches issue one
route-bound, non-cloneable authority.

The source checker pins the 81-row source acceptance inventory and eight-row R2
semantic overlay. The aggregate source gate runs the accepted R2 design gate, the
new mutation harness, full core/durable/FINAM tests, doctests, strict clippy, exact
P1-d2/P1-d3 regressions and the inherited P1-d4 105 x 2 SIGKILL replay evidence.
The full FINAM suite runs with the default closed feature set, while the new schedule
publisher tests additionally run with all features. A whole-crate all-features FINAM
test run is not a valid no-send gate because the historical
`m3j16-actual-one-shot` feature intentionally changes endpoint-gate behavior; strict
all-target/all-feature compilation remains mandatory through clippy.

## Deliberately closed

Redis DB15/DB0 activation, VPS deployment, schedule private-key installation,
FINAM POST/DELETE, broker command dispatch, runtime-live and real orders remain closed.
This source candidate does not authorize the complete I1 owner loop, first boot,
process signal/panic composition or operational activation.
