# Stage 8B-P1-d4 crash/replay design R6 correction

Status: R6 design-only review candidate. P1-d4 source implementation remains
paused until independent R6 acceptance.

Design parent: `b377c0275f1ce5f01cfe9b223724bf1542f985e2` (R5, HOLD).
Accepted design baseline: `e1ce6d3baec3974d8dfd05c2f3de00110e0605bf`
(R3, ACCEPTED). Business baseline:
`7dc7c802feca6e79d3a1a9902c181ad7b6afc506` (P1-d3, CLOSED / ACCEPTED).

R6 retains the accepted-in-principle R5 Option A, one retained M10,
XACK-last and the R4/R5 S09 Correction C1. It replaces the nonexistent
generated-Market V3 assumption with the accepted Stage6 V1 chain, corrects
sequence-pair timing and freezes the exact command-publication identity that
must survive replacement `S_ack` and `S_truth`.

## Active proof inventory

The accepted 88-row general matrix remains immutable. The inherited R5 base
registry remains exactly 92 rows and preserves all 13 R4-to-R5 S05 field
corrections plus the S09 durable-equivalence correction.

```text
stage8b-p1d4-scenario-frontier-matrix-v5.csv
  inherited base cells: 92 (P1D4C-001..P1D4C-092)

stage8b-p1d4-generated-market-crash-submatrix-v2.csv
  corrected composition cells: 13 (P1D4GM-001..P1D4GM-013)

active total: 105 exact positive cells
duplicate variant: required for every cell
conflict variant: required for every cell
```

The total is derived as `92 + 13 = 105`; 102 is not retained as a target.

## Retained-source order

The exact S05 order is:

```text
one later M10 pending
  -> S_eval persisted and reread
  -> one Hybrid bar callback
  -> combined Prepublication package persisted and reread
  -> exact command XADD and publication marker
  -> exact Stage5E schedule authority selected
  -> DispatchAttemptRecorded V1 persisted and reread
  -> deterministic Market provider result
  -> BrokerOrderObserved V1 persisted and reread
  -> BrokerTradeObserved V1 persisted and reread
  -> RequestFinalized V1 persisted and reread
  -> sole ACK owner allocates (seq_ack, seq_truth)
  -> exact ACK callback
  -> combined S_ack persisted and reread
  -> exact broker truth
  -> combined S_truth persisted and reread
  -> package, publication, PEL and group frontier cross-validation
  -> XACK the same later M10 last
```

There is no generated-Market Stage6 V3 record and no second M10 source.
No pre-`S_truth` source-already-acknowledged recovery mode exists.

## Exact accepted V1 journal recovery

R6 authorizes a narrow P1-d4 classifier over the existing immutable V1
record schema. The classifier authenticates the predecessor checkpoint,
request identity, action, sequence, previous/causal links, accepted command
hash, source evidence and exact deterministic IDs. It recognizes only these
strict suffixes for the generated Market request:

```text
DispatchAttemptRecorded
DispatchAttemptRecorded + BrokerOrderObserved
DispatchAttemptRecorded + BrokerOrderObserved + BrokerTradeObserved
DispatchAttemptRecorded + BrokerOrderObserved + BrokerTradeObserved
  + RequestFinalized(Completed)
```

Anything extra, reordered, mixed-version, conflicting or differently bound is
corruption. The existing P1-d2 complete-suffix classifier remains unchanged;
P1-d4 adds the missing partial-frontier classification without broadening it.

### Dispatch-only recovery

After an authenticated dispatch-only restart, the owner is
`P1d4GeneratedMarketDispatchPending`. It must not append another
`DispatchAttemptRecorded` and must not reacquire calendar/schedule authority.
It reconstructs the exact deterministic provider evidence from the retained
decision binding and canonical M10, verifies it against the existing dispatch,
then appends only the exact missing order and trade V1 records. A provider
attempt lost before durability may be recomputed, but the unique semantic
outcome and broker IDs remain exactly one.

### Order-only recovery

After an authenticated dispatch+order restart, the owner is
`P1d4GeneratedMarketOrderPending`. It reconstructs the same deterministic
provider evidence, validates the durable order ID and source evidence, and
appends only the missing `BrokerTradeObserved` V1 record. Reusing
`execute_stage6d_paper_outcome()` unchanged is forbidden because it would
attempt a second order append. No second dispatch, order observation, provider
effect or schedule authority is allowed.

After order+trade and before finalization, only
`P1d4GeneratedMarketPreFinalizationPending` may append and reread the exact
`RequestFinalized(Completed)` V1 record.

## Canonical command-publication binding

R6 freezes one crate-private serializable value:

```text
Stage8bP1d4CommandPublicationBindingV1
  schema_version = 1
  domain = moex.stage8b.p1d4.command-publication-binding.v1
  source_stream
  source_group
  source_m10_redis_id
  semantic_batch_id_sha256
  strategy_request_id
  canonical_command_sha256
  canonical_envelope_sha256
  command_stream
  command_group
  command_entry_id
  prepublication_package_generation
  prepublication_seal_generation
  prepublication_seal_commitment_sha256
  publication_binding_sha256
```

`publication_binding_sha256` is the domain-separated canonical hash over all
prior fields. The publication disposition is an observation (`Published` or
`IdempotentExisting`), not identity, and cannot change this binding. The value
contains no Redis connection, raw capability, provider, XACK owner, transport
or Stage5C authority.

Phase rules are exact:

- before XADD, a Prepublication package has no publication binding;
- after XADD but before `S_ack`, the unchanged Prepublication package plus its
  exact HMAC-covered generation/seal must validate the Redis marker and exact
  command entry; a byte-identical duplicate entry is not equivalent;
- before committing combined `S_ack`, the canonical parsed binding is written
  into the replacement package;
- `AckCommitted` and `TruthCommitted` require the same binding, covered by the
  replacement-package commitment/HMAC;
- before truth and before XACK, marker and exact entry are reread and validated
  against that binding and the current composition generation;
- absent binding, generation downgrade/replay, changed marker, missing original
  entry or any field mismatch fails before truth or XACK.

## Additive composition projection

Source implementation after acceptance may add exactly one crate-private peer
projection named `Stage8bP1d4GeneratedMarketCompositionV1`. Its frozen fields
are:

```text
schema_version = 1
phase = Prepublication | AckCommitted | TruthCommitted
operational_identity_sha256
package_generation
source_m10_stream_key
source_m10_group
source_m10_redis_id
source_m10_semantic_id_sha256
source_m10_payload_sha256
p1d3_replacement_sha256
semantic_commit_sha256
request_id
canonical_command_sha256
post_bar_callback_state_fingerprint_sha256
command_publication_binding = None | Some(exact V1 binding)
p1d2_feedback_sha256 = None | Some(exact phase feedback digest)
composition_sha256
```

The projection remains a peer of, not a field inside,
`Stage8bP1d3ReplacementProjectionV1`. Existing Stage5G package-instance HMAC
and source-lifecycle commitment cover every field and nested binding.

- `Prepublication` requires the P1-d3 replacement and one Market semantic
  commit, no P1-d2 feedback, and no serialized publication binding.
- `AckCommitted` requires exact ACK feedback, the canonical publication
  binding and `seq_truth = seq_ack + 1`.
- `TruthCommitted` requires exact truth feedback and the identical publication
  binding and sequence pair.

## Finite linear owners

Only these new crate-private non-Clone, non-Copy, non-serializable owners are
authorized:

```text
P1d4GeneratedMarketPrepublicationPending
  -> publish or validate exact existing command
  -> select the exact schedule and persist one dispatch

P1d4GeneratedMarketDispatchPending
  -> reconstruct exact deterministic provider evidence
  -> append only missing order and trade

P1d4GeneratedMarketOrderPending
  -> validate durable order against reconstructed evidence
  -> append only missing trade

P1d4GeneratedMarketPreFinalizationPending
  -> append/reread exact RequestFinalized once

P1d4GeneratedMarketPreAckPending
  -> allocate/reconstruct exact adjacent pair and ACK only
  -> commit combined S_ack with publication binding

P1d4GeneratedMarketAckCommitted
  -> revalidate publication binding and apply exact truth only
  -> commit combined S_truth

P1d4GeneratedMarketTruthCommitted
  -> revalidate package, publication, PEL and group frontier
  -> XACK exact retained source last
```

No generic `Ready`, P1-d3-only truth owner or ordinary P1-d2 owner may discard
either half of the composition.

## Sequence timing

The sequence contract follows accepted P1-d2 source exactly:

```text
through BrokerTradeObserved: pair_not_allocated
after RequestFinalized and before allocator:
  pair_not_allocated_but_deterministically_reconstructible
after sole allocator and before ACK:
  exact_pair_allocated_in_memory_and_reconstructed_after_restart
after S_ack: exact pair durable and unchanged
always: seq_truth = seq_ack + 1
```

The post-allocation/pre-ACK cell requires a separate fsync-backed test marker.
After SIGKILL, the final authenticated audit pair must equal the marker byte for
byte. Volatile pair or ACK state never changes restart authority; restart from
the durable pre-ACK package deterministically allocates the same pair.

## Required negative and crash evidence

All 105 cells require byte-identical duplicate and one-field conflict variants.
The generated-Market suite additionally rejects:

1. a second dispatch or order append after partial V1 recovery;
2. wrong V1 order/trade IDs, source evidence, links or sequence;
3. any generated-Market V3 assumption;
4. pair allocation before RequestFinalized or non-adjacent/reallocated pair;
5. changed command entry ID or canonical envelope hash;
6. changed source group, command stream or command group;
7. changed prepublication package/seal generation or seal commitment;
8. marker redirected to a byte-identical duplicate command entry;
9. original entry missing while a replacement exists;
10. absent publication binding in `AckCommitted` or `TruthCommitted`;
11. publication-binding downgrade or replay from another generation;
12. callback, XADD, provider semantic outcome, schedule, ACK, truth or XACK
    duplication;
13. source missing before combined `S_truth`;
14. XACK before persist, reread, authentication and cross-validation of
    combined `S_truth`;
15. any operational Redis DB0/VPS, FINAM or live surface opening.

## Source resumption boundary

Only independent acceptance of this exact R6 artifact may resume source work.
That later source slice is limited to the seven owners, four exact V1 suffixes,
canonical publication binding, 105 positive cells and their duplicate/conflict
variants. It must inherit P1-d3, P1-d2, workspace and strict-clippy gates.

R6 itself changes documentation, evidence and checkers only. It does not
authorize operational Redis DB0/VPS, P1-e, FINAM POST/DELETE, broker dispatch,
runtime-live, real orders, partial fills, fees/slippage, replace/protective,
bracket/multi-leg orders or Generation-2 production authorization.
