# Stage 8B-P1-d4 crash/replay design R5 correction

Status: R5 design-only review candidate. P1-d4 source implementation remains
paused until independent R5 acceptance.

Design parent: `ebede1d804f5eff50d6b4b9455edb08735e1be2c` (R4, HOLD).
Accepted design baseline: `e1ce6d3baec3974d8dfd05c2f3de00110e0605bf`
(R3, ACCEPTED). Business baseline:
`7dc7c802feca6e79d3a1a9902c181ad7b6afc506` (P1-d3, CLOSED / ACCEPTED).

R5 accepts R4 Correction C1 unchanged and replaces rejected Correction C2.
It chooses review Option A: preserve the accepted P1-c/P1-d2 source-retention
contract. There is one later-bar M10 source. It remains pending from S05
evaluation through generated-Market `S_ack` and `S_truth`; XACK is last.
Publication never manufactures a second M10 source.

## Normative artifacts

The accepted 88-row general matrix and historical R3/R4 registries remain
immutable. R5 adds an exact 24-row amendment because the historical
`P1D4D-049` wording is valid for zero-intent later-bar paths but not for the
S05 one-intent composition.

The active R5 proof inventory is:

```text
stage8b-p1d4-scenario-frontier-matrix-v5.csv
  base cells: 92 (P1D4C-001..P1D4C-092)

stage8b-p1d4-generated-market-crash-submatrix-v1.csv
  composition cells: 10 (P1D4GM-001..P1D4GM-010)

active total: 102 exact positive cells
duplicate variant: required for every cell
conflict variant: required for every cell
```

R5 changes 13 fields in the base registry, all in S05:

```text
P1D4C-039 precondition
P1D4C-039 expected_restart_disposition
P1D4C-039 only_legal_continuation
P1D4C-039 sequence_expectation
P1D4C-039 provider_delta
P1D4C-039 schedule_authority_delta
P1D4C-039 xack_expectation
P1D4C-040 precondition
P1D4C-040 expected_restart_disposition
P1D4C-040 only_legal_continuation
P1D4C-040 sequence_expectation
P1D4C-040 provider_delta
P1D4C-040 schedule_authority_delta
```

The checker derives and freezes the exact delta; this list is descriptive and
must match that machine result.

## Correction C1 remains accepted

`P1D4C-064` S09/F09 and `P1D4C-092` S09/F04 remain the same durable
equivalence class: `P1d3PreAckPending`. Volatile ACK state, the SIGKILL marker
and test environment are never restart authority. `P1D4C-063` S09/F08 alone
retains `P1d3CancelContinuationPending` before recovered CANCEL V3 and
`RequestFinalized` exist.

## Correction C2 R5: retained-source generated-Market composition

The exact S05 order is:

```text
later M10 pending
  -> S_eval persisted and reread
  -> one Hybrid bar callback
  -> combined prepublication package persisted and reread
  -> exact command XADD/publication marker (same M10 still pending)
  -> exact Stage5E schedule authority and deterministic paper provider
  -> Stage6 V3 WAL and Stage7 RequestFinalized
  -> exact ACK callback and combined S_ack persisted/reread
  -> exact broker truth and combined S_truth persisted/reread
  -> cross-validate package, source PEL binding and group frontier
  -> XACK that same later M10 last
```

No operation between publication and `S_truth` may acknowledge the source.
There is no source-already-acknowledged pre-`S_truth` recovery mode.

## Exact additive composition projection

Source implementation after acceptance may add exactly one crate-private
persisted projection named `Stage8bP1d4GeneratedMarketCompositionV1`. It is a
peer of, not a field inside, `Stage8bP1d3ReplacementProjectionV1`. The outer
Stage5G replacement package carries both projections and the existing
one-intent semantic commit.

Its canonical fields are frozen as:

```text
schema_version = 1
phase = Prepublication | AckCommitted | TruthCommitted
operational_identity_sha256
package_generation
source_m10_stream_key
source_m10_redis_id
source_m10_semantic_id_sha256
source_m10_payload_sha256
p1d3_replacement_sha256
semantic_commit_sha256
request_id
canonical_command_sha256
post_bar_callback_state_fingerprint_sha256
p1d2_feedback_sha256 = None | Some(exact phase feedback digest)
composition_sha256
```

`composition_sha256` is the domain-separated canonical hash over every prior
field. Existing Stage5G package-instance commitment/HMAC and source-lifecycle
commit must cover the complete projection. The projection contains no raw
`Stage5cSettledPaperStrategy`, `Stage5cPendingRecoveryReceipt`, transport,
Redis connection, provider or XACK capability.

Phase validation is exact:

- `Prepublication`: P1-d3 replacement and one Market semantic commit present;
  no P1-d2 feedback; callback-state fingerprint equals persisted runtime.
- `AckCommitted`: the same replacement/semantic/source identities and exact
  P1-d2 ACK feedback are present; `seq_truth = seq_ack + 1`.
- `TruthCommitted`: the same identities and exact P1-d2 truth feedback are
  present; post-truth state and sequence pair cross-validate.

Any missing peer projection, non-Market command, changed request/source/hash,
phase skip, feedback/phase mismatch or old package replay fails before a
callback, XADD, provider, sequence allocation, truth or XACK effect.

## Finite linear owners

Only these new crate-private owners are authorized:

```text
P1d4GeneratedMarketPrepublicationPending
  -> exact first XADD or validate exact existing XADD
  -> exact schedule/provider continuation

P1d4GeneratedMarketPreAckPending
  -> finalize/reconstruct exact ACK from authenticated Stage6 V3 only
  -> commit combined S_ack

P1d4GeneratedMarketAckCommitted
  -> exact truth only
  -> commit combined S_truth

P1d4GeneratedMarketTruthCommitted
  -> exact retained-source XACK only
```

Each owner is non-Clone, non-Copy, non-serializable and consumed linearly.
No generic Ready, P1-d3-only truth owner or ordinary P1-d2 owner may be used
to drop either half of the composition. Existing P1-d2 reduction logic may
be reused internally, but its output must preserve the authenticated P1-d3
replacement in every replacement package.

## Recovery classifier

The classifier uses only authenticated journal and replacement-package
state. Redis publication marker/command verification happens in the Redis
composition wrapper after owner construction; it does not alter the package
phase.

```text
combined Prepublication package, no generated Market V3
  -> P1d4GeneratedMarketPrepublicationPending

combined Prepublication package + exact generated Market V3
  -> P1d4GeneratedMarketPreAckPending

combined AckCommitted package
  -> P1d4GeneratedMarketAckCommitted

combined TruthCommitted package
  -> P1d4GeneratedMarketTruthCommitted
```

The original M10 must be the single exact PEL entry for every pre-XACK owner.
A missing source before `TruthCommitted` is corruption, not idempotent
completion. Only `TruthCommitted` may interpret exact PEL absence plus an
authenticated advanced group frontier as `AlreadyAcknowledged` after an
XACK response-loss restart.

## Crash and counter scope

The base S05 rows describe the bar callback/publication boundary and terminal
XACK response-loss boundary. The generated-Market submatrix exclusively owns
provider, schedule, sequence, `S_ack` and `S_truth` counters. Therefore no
field is implicitly scoped:

- bar callback total is exactly 1 from GM00 onward;
- command publication total is exactly 1 from GM01 onward;
- provider and schedule total are exactly 1 after the covering V3 WAL;
- sequence is absent before WAL and is one unchanged adjacent pair after WAL;
- combined `S_ack` and `S_truth` each commit once;
- source XACK total is 0 through GM09 and exactly 1 only at base F16;
- final source disposition is `AlreadyAcknowledged` only after parsed reply 1
  or exact response-loss proof from `TruthCommitted`.

Every one of the 102 cells requires byte-identical replay, one-field conflict,
real child process, fsynced marker, SIGKILL and post-restart audit evidence.

## Required negative cases

The R5 implementation gate must reject at least:

1. source missing before combined `S_truth`;
2. wrong source Redis ID, semantic hash or payload hash;
3. missing/tampered publication marker or command entry;
4. wrong command payload/hash/request ID;
5. wrong P1-d3 replacement or semantic-commit hash;
6. raw Stage5C authority embedded in the composition;
7. ACK feedback in Prepublication or truth feedback in AckCommitted;
8. feedback sequence reallocation or non-adjacent pair;
9. callback, XADD, provider, schedule, ACK or truth replay;
10. XACK before reread/cross-validation of combined `S_truth`;
11. second XACK after exact response-loss proof;
12. package phase skip or downgrade;
13. absent/wrong authenticated group frontier for `AlreadyAcknowledged`;
14. any operational Redis DB0/VPS, FINAM or live surface opening.

## Source resumption after R5 acceptance

After independent R5 acceptance, the saved source WIP may resume only to:

1. remove the rejected early-XACK/source-independent S05 path;
2. retain accepted C1 durable-equivalence behavior;
3. implement the exact composite projection and four owners above;
4. preserve one pending M10 through combined `S_truth` and XACK it last;
5. execute all 102 positive cells and duplicate/conflict variants;
6. run inherited P1-d3/P1-d2/workspace/strict-clippy gates;
7. produce a separate immutable source review package.

R5 itself is documentation/checker/handoff only. It does not authorize
operational Redis DB0/VPS, P1-e, FINAM POST/DELETE, broker dispatch,
runtime-live, real orders, partial fills, fees/slippage, replace/protective,
bracket/multi-leg orders or Generation-2 production authorization.
