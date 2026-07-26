# Stage 5E-b3c source-authority production bridge — implementation candidate

Status: the R6 authority freeze was accepted at source ref
`2b2c57d7bacb8e3f1de572b7c35790be906b82a9`. This package implements that
accepted additive bridge and is pending implementation review. R6 below is the
sole operative contract; R1--R5 record the decisions that led to it and are
superseded wherever they differ.

Lineage root / initial design baseline: `936250e675ac15b61a7a4e319b59e508cd834f30`.
Accepted R6 implementation baseline: `2b2c57d7bacb8e3f1de572b7c35790be906b82a9`.

The earlier user-mentioned implementation ref
`e084411f0b8be75e03aec1e0fc177b707febf833` was superseded by the actual
reviewed R6 handoff at `2b2c57d`; all implementation scope and provenance in
this package are therefore measured from `2b2c57d`.

The preceding B3C package is accepted only as private no-I/O plumbing. Its
calendar binding is useful, but it does not establish trusted eligibility:

- accepted Stage 4 schedule evidence does not retain dynamic broker state
  `Open`;
- `UnverifiedMarketSequenceSource` is not a Stage 5C owner capability;
- `"epoch-1"` is not lineage.

## Decision

The implementation may begin only after review of this extension contract. It
uses three additive, owner-issued receipts and does not repurpose old B3C
receipts.

| Authority | Owner | Proposed sealed receipt | Required fact |
| --- | --- | --- | --- |
| Dynamic market state | Stage 4 broker-truth owner | `Stage4AcceptedOpenSessionEvidence` | exact `BrokerMarketSessionState::Open` for the full instrument, fresh at observation |
| Final bar continuity | Stage 5C canonical history / semantic-bar owner | `Stage5cAcceptedMarketSequenceEvidence` | exact final bar identity, canonical predecessor and owner-classified no-gap continuity |
| Eligibility lineage | Stage 5E conjunction owner | `Stage5eContinuationBindingId` | deterministic binding of the B3B event key and all three owner fingerprints |

The first two receipts are source-owner capabilities: no public constructors,
no serializable/raw DTO escape and no caller-provided booleans. Stage 5E only
consumes them through crate-private bridges. The third is not a shared string
or independent broker epoch: it is computed once at conjunction from the
source-owner identities and must replace B3C's placeholder equality check.

## Stage 4 dynamic Open receipt

The Stage 4 extension must preserve the existing accepted schedule API and add
a distinct sealed receipt. Its minimum immutable fields are:

```text
full InstrumentId
BrokerMarketSessionState::Open (exact enum value)
observed_at / expires_at
broker-truth source fingerprint
source snapshot generation or epoch
Stage 4 accepted-report identity
```

It is rejected for `Closed`, `Break`, `Maintenance`, `Unknown`, missing source,
future observation or expiry. A static `TradableOpen` schedule interval is not
a substitute for this receipt.

## Stage 5C sequence receipt

The Stage 5C extension must add a separate owner-issued receipt without
changing its frozen public type-state API. Its minimum fields are:

```text
full InstrumentId
exact semantic bar/event-key identity
timeframe
finality
accepted canonical previous close identity
owner-classified gap-free continuity
recovery/aggregation source fingerprint and generation
observed_at / expires_at
```

The receipt can be issued only after the existing accepted canonical-history
and final-live semantic-bar path has established continuity. A raw bar,
timestamp delta, free-form `gap_free` flag or `UnverifiedMarketSequenceSource`
must not produce the receipt in production. Until the extension lands, that
unverified source is test-only and the production authority path is fail-closed.

## Continuation binding

`Stage5eContinuationBindingId` is a deterministic digest made at the B3C
conjunction. It includes domain separation plus:

```text
B3B event-key fingerprint
Stage 4 Open-session source fingerprint and generation
validated normalized-schedule identity fingerprint
Stage 5C sequence source fingerprint and generation
```

This deliberately replaces a claim that three independent sources share a
pre-existing string epoch. The resulting binding is used only by the opaque
no-I/O eligibility receipt; it does not authorize a callback or execution.

## Exact implementation boundary after approval

Only a later accepted implementation package may alter these production files:

```text
crates/broker-core/src/... Stage 4 owner boundary, if required by the existing ownership split
crates/strategy-runtime-core/src/stage5c_paper_host.rs
crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs
```

That package must extend the Stage 4 and Stage 5C freeze manifests additively,
keep their existing public APIs stable, pin new owner bridge regions and add no
network, Redis, transport, dispatch, callback, intent or runtime-live path.

## Required implementation evidence

- `Closed`, `Break`, `Maintenance` and `Unknown` cannot produce Stage 4 Open
  evidence; exact `Open` can.
- a raw/unverified sequence cannot produce production sequence evidence.
- owner-issued Stage 5C sequence preserves finality, predecessor and no-gap
  classification.
- a producer block returns its source and a refreshed retry can succeed.
- source mutation changes `Stage5eContinuationBindingId`.
- production wrapper has no caller-supplied clock; deterministic clock seam is
  test-only.
- no new callback, strategy mutation, intent, Redis, FINAM, transport,
  dispatch, runtime-live or broker execution surface exists.

## Mandatory negative mutations

The implementation review must add mutations for Open-state stripping,
unverified sequence authority, source-owner constructor bypass, constant lineage
replacement, producer source loss, clock-seam exposure and plan/inventory
authority contradiction.

## Still closed

This design and its future implementation remain no-I/O and no-send. In
particular, `on_broker_bar`, strategy state mutation, intent construction,
intent sink, Redis, FINAM I/O, transport, dispatch, runtime-live, autonomous
event loops and broker execution are not authorized.

## R1 historical governance contract

R1 established the governance baseline. Its implementation topology was
superseded by R2--R5; it is retained only to explain the lineage. The complete
R5 contract below, together with the JSON inventory, is normative and
hash-pinned by the active checker.

### Exact owner paths and issuing transitions

Stage 4 authority is limited to:

```text
crates/broker-core/src/stage4_bootstrap.rs
crates/broker-core/src/operational_config.rs
crates/broker-core/src/lib.rs
```

`Stage4AcceptedPaperHostEvidence` is the only public opaque input. The
additive Stage 4 owner methods are exactly:

```text
Stage4AcceptedPaperHostEvidence::issue_stage4_open_session_evidence
Stage4AcceptedPaperHostEvidence::issue_stage4_schedule_discontinuity_evidence
```

They retain the dynamic broker state and schedule facts privately while
requiring an accepted report, an applied application, exact full instrument
identity, fresh present Schedule source section and non-expired source. The
first method requires exact `BrokerMarketSessionState::Open`. The second method
may issue evidence only for one exact interval with no expected tradable close
between the stated timestamps.

Stage 5C authority is limited to:

```text
crates/strategy-runtime-core/src/stage5c_paper_host.rs
```

Its sole issuer is the crate-private function:

```text
issue_stage5c_accepted_market_sequence_evidence(
    Stage5cPendingRecoveredPaperStrategy,
    Stage5cAcceptedSemanticBar,
    Option<Stage4AcceptedScheduleDiscontinuityEvidence>,
)
```

It may issue only after accepted recovery, a final semantic bar, an exact
canonical predecessor and a non-expired recovery boundary. It does not invoke
a callback or construct an intent.

### Exact continuity semantics

The only accepted sequence classifications are:

```text
Contiguous: current_close == previous_close + timeframe
ApprovedNonTradableBoundary: an exact matching Stage 4 boundary evidence proves
                             there was no expected tradable close between them
```

Every other difference, including weekends, clearing, maintenance, holidays or
an unknown schedule, blocks unless it has the exact Stage 4 owner evidence.
Neither a raw timestamp delta nor a caller-supplied `gap_free` boolean is an
authority.

### Construction seal

All three future owner receipts have private fields and forbid `Clone`, `Copy`,
`Serialize`, `Deserialize`, `Default`, `From` and `Into`. They have no raw or
free constructor, no second issuing path and no struct literal outside their
pinned owner region. A recoverable block returns every consumed linear source
unchanged for a refreshed retry.

### Exact continuation binding

`Stage5eContinuationBindingId` is exactly a 32-byte SHA-256 digest with domain
`stage5e-continuation-binding-v1`. It encodes tagged, length-prefixed canonical
bytes in this order:

```text
B3B event-key fingerprint
Stage 4 Open schedule-source fingerprint
Stage 4 Open schedule-source generation
normalized schedule identity fingerprint
Stage 5C sequence-source fingerprint
Stage 5C sequence-source generation
```

A constant, free-form epoch or omitted authority field is invalid. This ID is
computed only by the private Stage 5E conjunction; it neither calls strategy
code nor authorizes an execution.

## R2 authority and linear-ownership correction

R1 is accepted as a governance foundation, but its proposed implementation
cannot honestly issue the two receipts: Stage 4 does not retain normalized
intervals, and a second Stage 5C issuer would consume the same linear pair as
the existing observed-bar bridge. R2 replaces that infeasible shape. This is
still design-only; no production Rust, callback, intent, Redis, FINAM,
transport, dispatch or runtime-live path is changed by this package.

### Boundary owner: normalized schedule, not Stage 4

Exact interval proof belongs to the existing private normalized-schedule owner
in `stage5e_no_io_lifecycle.rs`. The future owner transition is exactly:

```text
ValidatedNormalizedInstrumentScheduleSnapshot
+ AcceptedInstrumentRegistryEvidence
+ AcceptedStage4ScheduleEvidence
→ Stage5eAcceptedScheduleProjectionEvidence
```

The projection retains the exact instrument, venue, board, trading day,
validated normalized sessions, source observations/expiry and the accepted
Stage 4 dynamic-session projection. It produces a selected tradable-open
window, but never an optional discontinuity proof. Candidate-specific boundary
classification is deferred until the canonical predecessor, accepted final bar
and admitted timeframe are sealed by the Stage 5C owner. A caller timestamp
pair or boolean can never stand in for that proof.

Stage 4 remains the authority for *current* broker session state and freshness.
The later implementation must retain `BrokerMarketSessionState` privately in
`Stage4AcceptedPaperHostEvidence` while it is built from
`ValidatedStage4BrokerTruthBootstrap`; `project_accepted_stage4_schedule` may
project it only after accepted-report and freshness checks. The exact issue
point is `build_stage4_accepted_paper_host_evidence`, before Stage 5C admission
consumes its opaque evidence. Only exact `BrokerMarketSessionState::Open` is
eligible for the schedule projection.

### One linear observed-bar transition

There will be no independent `Stage5cAcceptedMarketSequenceEvidence` issuer.
The existing bridge is replaced additively by exactly one crate-private
transition in `stage5c_paper_host.rs`:

```text
Stage5cPendingRecoveredPaperStrategy
+ Stage5cAcceptedSemanticBar
+ Stage5eAcceptedScheduleProjectionEvidence
→ Stage5eObservedLiveBarWithSequenceEvidence
```

The output owns the strategy, recovery receipt, accepted semantic bar,
canonical predecessor, classification, schedule projection and sequence
fingerprint. It therefore replaces—not parallels—the old observed-bar bridge.
On every recoverable block it returns all three original linear inputs.

The only classifications are:

```text
Contiguous
  current_close == previous_close + timeframe

ApprovedNonTradableBoundary
  the sealed Stage 5C candidate is classified against the same consumed
  schedule projection; its discrete expected-close grid proves no interior
  tradable close exists between the two canonical closes
```

No callback is invoked and no intent is created in this transition.

### Identity, generation and restart semantics

R2 removes the invented mutable generation counters. The sources use immutable
snapshot identities instead.

- Schedule snapshot identity is the existing
  `stage5e-b3-normalized-snapshot-v2` canonical payload fingerprint.
- Schedule-window identity is the existing
  `stage5e-schedule-window-evidence-v2` deterministic fingerprint.
- Sequence identity is a new private
  `stage5e-b3c-market-sequence-v1` digest over accepted Stage 3 provenance,
  exact semantic bar, canonical predecessor, recovery receipt, classification
  and optional boundary identity.
- `Stage5eContinuationBindingId` becomes
  `stage5e-continuation-binding-v2`, binding the B3B event key, schedule
  snapshot identity, schedule-window identity and sequence identity.

Receipts and authority identities are not persisted or reconstructed. A
restart repeats Stage 4 acceptance, normalized schedule validation, canonical
history warmup, pending recovery and the single issuer. This makes the source
identity deterministic without a synthetic generation counter.

### Future implementation scope

Only a separately accepted implementation package may modify these production
paths:

```text
crates/broker-core/src/stage4_bootstrap.rs
crates/broker-core/src/lib.rs
crates/strategy-runtime-core/src/stage5c_paper_host.rs
crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs
```

It must also update the exact Stage 5C/5D/B3C freeze manifests, checkers and
negative harnesses listed in the normative inventory. Omitting a required
freeze update is a hard failure. This R2 package does not grant that
implementation permission; acceptance of R2 is a prerequisite for asking for
it.

## R3 complete topology and canonical-authority correction

R3 is the final governance-only correction before a separately reviewed
additive implementation package. It resolves the remaining R2 ambiguity: the
new observed-bar receipt must pass through B3B and B3C without duplicate
schedule ownership, raw construction or a second linear consumer.

### Complete linear topology

The only future successful path is:

```text
Stage4 accepted evidence with privately retained dynamic session state
→ Stage5E normalized schedule owner issues Stage5eScheduleProjectionBridgeInput
→ Stage5C single issuer consumes recovery + semantic bar + bridge input
→ Stage5eObservedLiveBarWithSequenceEvidence
→ B3B bind_schedule_window_sequence_to_observed_live_bar
→ Stage5eBoundScheduleWindowSequenceForObservedLiveBar
→ B3C bind_session_calendar_sequence_from_b3b
→ Stage5eBoundSessionCalendarSequenceForObservedLiveBar
```

There is no side channel around B3B. The Stage 5C output owns the schedule
bridge; B3B consumes that one output and retains the schedule, bar, strategy,
recovery and sequence identity in one monotonic receipt. B3C consumes only
that B3B receipt and does not accept a raw or separately supplied market
sequence source.

On a recoverable Stage 5C block all three original inputs are returned. On a
B3B block the whole observed-bar-with-sequence receipt is returned. On a B3C
block the whole B3B receipt is returned. No successful receipt provides an
`into_inputs` or other reverse-construction API.

### Sealed cross-module bridge

`Stage5eScheduleProjectionBridgeInput` is the only value crossing from the
private Stage 5E schedule owner into `stage5c_paper_host`. It is declared at
the parent `stage5e_no_io_lifecycle` module as a `pub(crate)` opaque type with
private fields. Its only constructor is the private nested owner method:

```text
schedule_window_evidence::issue_schedule_projection_bridge
```

Stage 5C can consume it but cannot construct, clone, serialize, deserialize,
default, convert or unwrap it. This is the explicit construction seal that
keeps normalized interval facts owned by Stage 5E while allowing the one
linear Stage 5C transition.

### Exact discontinuity algorithm

The bar-close grid has Unix epoch zero as origin. Both endpoint closes must be
aligned to a positive `timeframe_sec`. For a candidate gap, enumerate exactly:

```text
t = previous_close + n * timeframe_sec, n >= 1, t < current_close
```

The previous endpoint, current endpoint and every strict-interior expected
`t` must each have exactly one classification in one same-trading-day
normalized snapshot. The current endpoint must be `TradableOpen`; interior
points may be only `BreakOrClearing` or `Maintenance`. Unknown, uncovered,
interior `TradableOpen` or cross-day points block. Continuous wall-clock range
coverage, overnight, weekend and holiday inference are not authorized until a
separately reviewed multi-day receipt exists.

### Canonical nested identities

All identities use SHA-256 over tagged, length-prefixed canonical bytes; no
`Debug`, JSON/serde representation, platform integer width or free-form string
is permitted. The normative inventory pins domain/version, field order, enum
codes, integer timestamp units and IEEE-754 `to_bits` encoding for:

- Stage 3 provenance;
- accepted semantic bar;
- pending recovery receipt;
- retained Stage 4 dynamic session;
- non-tradable boundary;
- sequence identity; and
- continuation binding.

The new semantic receipt retains only its canonical Stage 3 provenance digest,
not a raw mutable DTO. Receipts are never persisted or reconstructed: a
restart repeats the complete accepted chain through B3C, and a later test must
prove pre-restart and post-restart identities cannot be mixed.

### Required predecessor governance update set

The implementation package must update both B and B3 predecessor plans,
inventories and checkers, in addition to Stage 5C/5D freeze artifacts and the
existing evidence harnesses. The exact required and unchanged path sets are in
the inventory. Omission of any listed predecessor checker/manifest update is a
hard failure; the active descriptor registry and lifecycle gate remain
unchanged for this implementation.

## R4 historical candidate-flow correction

R4 established the required direction without changing production source:
schedule projection must not precompute a candidate-specific boundary before it
has candidate facts. Its initial candidate/classifier visibility sketch is
superseded by the implementable R5 construction seal below.

### Sealed candidate classification

`Stage5eScheduleProjectionBridgeInput` retains trusted normalized sessions,
the selected Open window and immutable identities, but never an optional
boundary derived without a predecessor/current bar pair. R5 defines the sole
candidate seal, classifier bridge, ownership and blocked-return mechanics.
Stage 5C never receives raw normalized sessions and cannot perform calendar
inference itself.

### Grid-only boundary proof

The authority object is the discrete bar-close grid, not continuous wall-clock
coverage. Previous/current endpoints and every strict-interior expected close
must each have exactly one classification in the same trading-day snapshot.
The current endpoint must be `TradableOpen`; every strict-interior candidate
must be `BreakOrClearing` or `Maintenance`. An interior `TradableOpen`,
unknown, uncovered or cross-day point blocks. No continuous-range inference is
permitted.

### B3C continuation freshness

`bind_session_calendar_sequence_from_b3b` captures production `Utc::now()` at
entry. Before success it verifies that the clock is not before effective
observations, not after effective expiry, and not before the observed bar
close. It revalidates Stage 4 dynamic-session and normalized-schedule
freshness. The deterministic `_at` variant is `cfg(test)` only. A block returns
the full B3B receipt; success stores `bound_at` and `effective_expires_at` and
remains monotonic.

### Stage 4 schedule-section identity

`stage4_schedule_source_identity` means only a canonical fingerprint of the
accepted Stage 4 report's Schedule source section; it does not claim a raw
broker snapshot identity. Its domain, section/status/freshness enum codes,
option encoding, age, max-age, bootstrap flags, report schema/timestamp and
target instrument are pinned in the inventory. Debug/serde formatting and a
constant source identifier are prohibited.

## R5 consolidated implementable authority contract

R5 resolves the remaining R4 implementation ambiguity without changing
production source. This section replaces every earlier topology statement that
conflicts with it. There is exactly one authority flow:

```text
Stage 4 accepted Open evidence
→ Stage 5E schedule owner issues Stage5eScheduleProjectionBridgeInput
→ sole Stage 5C issuer borrows recovered strategy + accepted final bar
→ Stage 5C builds one opaque Stage5cSequenceCandidateSeal
→ consumed projection becomes one opaque Stage5eScheduleCandidateClassifier
→ the seal invokes its sole classifier method with that classifier
→ classification plus the original linear inputs form
  Stage5eObservedLiveBarWithSequenceEvidence
→ B3B → B3C
```

There is no optional precomputed boundary, no raw schedule-session export and
no second classifier or candidate constructor.

### Exact cross-module construction seal

`Stage5cSequenceCandidateSeal` is defined in
`strategy_runtime_core::stage5c_paper_host` as a `pub(crate)` opaque,
non-`Clone`, non-`Copy` type with private fields. Its only constructor is the
private `build_stage5c_sequence_candidate_seal` inside
`stage5e_try_observe_live_bar_after_history_with_sequence_evidence`. It
**borrows** `Stage5cPendingRecoveredPaperStrategy` and
`Stage5cAcceptedSemanticBar`; it owns only canonical scalar copies and private
identity/freshness fields. Consequently a block retains the original linear
inputs unchanged and the ephemeral seal is dropped.

The seal owns exactly: full instrument identity, canonical predecessor close,
accepted final current close, non-zero admitted timeframe, semantic-bar and
recovery identities, `sequence_observed_at`, `sequence_expires_at` and the
sequence identity. It has no getter, serializer, raw constructor, conversion,
`Default`, `Debug`, `Clone` or `Copy` implementation.

`Stage5eScheduleCandidateClassifier` is defined in
`strategy_runtime_core::stage5e_no_io_lifecycle::schedule_window_evidence` as
a `pub(crate)` opaque, non-serializable linear type with private fields. Its
only constructor consumes `Stage5eScheduleProjectionBridgeInput` in
`into_stage5e_schedule_candidate_classifier`. Its only callable classification
entry is the crate-private
`Stage5eScheduleCandidateClassifier::classify_from_stage5c_seal_fields`; the
only allowed call site is the private `Stage5cSequenceCandidateSeal` method
`classify_with_owned_projection`. That method is itself callable only from the
sole Stage 5C issuer. It transfers predecessor/current/timeframe directly from
the seal's private fields; no caller can construct a classifier or supply a
free candidate tuple. The classifier consumes both values and returns either:

```text
Approved(classification, returned_projection)
Blocked(reason, returned_projection)
```

The caller keeps the borrowed recovered strategy and accepted bar in both
branches. It emits the observed-bar-with-sequence receipt only on `Approved`;
on `Blocked` it returns exactly the original recovered strategy, accepted bar
and returned projection. No raw normalized session is visible outside the
Stage 5E owner.

`Stage5eObservedLiveBarWithSequenceEvidence` is defined in
`stage5c_paper_host` as a `pub(crate)` opaque, private-field, non-Clone/non-
Copy/non-Serialize receipt. Its only constructor is the same sole Stage 5C
issuer and its only consumer is
`bind_schedule_window_sequence_to_observed_live_bar` in the Stage 5E B3B
owner. It has no free/raw constructor, `into_inputs`, callback or intent API.

### Sequence freshness and exact receipt lifetime

The Stage 5C issuer captures production `Utc::now()` once, after canonical
predecessor/finality admission and before candidate construction. This value is
`sequence_observed_at`; the `_at` test seam is `cfg(test)` only. The candidate
is accepted only when:

```text
recovery_recovered_at <= sequence_observed_at
current_final_bar_close <= sequence_observed_at
sequence_observed_at - current_final_bar_close <= admitted_timeframe_sec
```

The explicit max-age policy is exactly one admitted timeframe. Its lifetime is:

```text
sequence_expires_at = min(
  recovered_bootstrap_broker_truth_expires_at,
  sequence_observed_at + Duration::seconds(admitted_timeframe_sec)
)
```

Both B3B and B3C revalidate `sequence_observed_at <= clock <=
sequence_expires_at`; a stale or future sequence blocks and returns the whole
linear predecessor receipt. `sequence_observed_at`, `sequence_expires_at` and
the admitted timeframe are included in the canonical sequence identity in the
inventory. The schedule projection already defines:

```text
projection_effective_observed_at = max(stage4_dynamic_observed_at,
                                      normalized_schedule_observed_at)
projection_expires_at = min(stage4_dynamic_expires_at,
                             normalized_schedule_expires_at)
```

B3C captures `bound_at = Utc::now()` exactly once and succeeds only when it is
not before both effective observations, not after either expiry and not before
the accepted bar close. The exact successful-receipt formula is:

```text
effective_expires_at = min(projection_expires_at, sequence_expires_at)
effective_observed_at = max(projection_effective_observed_at,
                            sequence_observed_at)
```

No maximum, copied source expiry, omitted sequence expiry or independently
chosen TTL is permitted. Stage 4 Schedule-section `age_ms` is intentionally
part of its identity: it records the accepted report's admission-time freshness
evaluation, not an immutable raw broker-snapshot identity.

### R5 implementation gates

The implementation package must prove the single constructor and call-site
seals, sealed candidate blocked ownership, sequence freshness at B3B and B3C,
the exact `min(...)` effective expiry, and no raw observed-bar-with-sequence
construction. Negative mutations listed in the inventory are mandatory. This
remains no-I/O/no-callback/no-intent work until a later separately reviewed
stage authorizes anything else.

## R6 final identity lifecycle and B3B consuming bridge

R6 closes the two remaining data-flow seams without changing production source.
It supersedes R5 wherever R5 says that a pre-classification candidate owns a
final sequence identity or leaves the Stage 5C-to-B3B consumer mechanism
implicit.

### Post-classification identity lifecycle

`Stage5cSequenceCandidateSeal` is strictly pre-classification material. It
owns only:

```text
full instrument identity
canonical predecessor close
accepted final current close
non-zero admitted timeframe
accepted Stage 3 provenance identity
accepted semantic-bar identity
recovery identity
sequence_observed_at
sequence_expires_at
```

It does **not** own, expose or precompute `sequence_identity_fingerprint`, a
classification code or a boundary fingerprint. Its creator rejects a receipt
before it exists when `sequence_expires_at < sequence_observed_at`.

The candidate's only successful terminal method remains
`classify_with_owned_projection`. It consumes the candidate and the Stage 5E
classifier. After the classifier returns a concrete `Contiguous` or
`ApprovedNonTradableBoundary(boundary_fingerprint)` result and its projection,
that same private Stage 5C method creates the opaque
`Stage5cClassifiedSequenceSeal`. This is the only point that computes the final
`sequence_identity_fingerprint` with the inventory's canonical
`stage5e-b3c-market-sequence-v2` encoding.

`Stage5cClassifiedSequenceSeal` is a `pub(crate)` opaque, private-field,
non-Clone/non-Copy/non-Serialize linear type owned by `stage5c_paper_host`. Its
only constructor is the successful branch of
`Stage5cSequenceCandidateSeal::classify_with_owned_projection`; its only
consumer is the sole Stage 5C observed-bar issuer. It owns classification,
optional boundary fingerprint, final `sequence_identity_fingerprint`, returned
projection and the copied freshness/identity material. A blocked classifier
branch creates no classified seal and returns only the original recovered
strategy, accepted semantic bar and returned projection.

The old name `sequence_source_fingerprint` is forbidden in the new B3B/B3C
topology. Every final downstream field and event-key input is named
`sequence_identity_fingerprint` and means exactly the final canonical sequence
identity. If an implementation needs an upstream provenance identity, it must
use the independently defined `stage3_provenance_identity`; it may never be
substituted for the final sequence identity.

### Exact Stage 5C receipt → Stage 5E B3B consuming bridge

`Stage5eB3bConsumeSeal` is defined by
`stage5e_no_io_lifecycle::schedule_window_evidence` as a `pub(crate)` opaque
type with private fields. Its only constructor is the private B3B issuer
`issue_stage5e_b3b_consume_seal` immediately before it consumes an observed
receipt. It is non-Clone, non-Copy, non-serializable and cannot be created by
Stage 5C or any other module.

`Stage5eObservedLiveBarWithSequenceEvidence` has exactly one crate-private
consuming method:

```text
consume_for_b3b(self, Stage5eB3bConsumeSeal)
  -> Stage5eB3bObservedLiveBarBridgePayload
```

The method is defined in `stage5c_paper_host`, consumes both the receipt and
the seal, and is callable only at
`bind_schedule_window_sequence_to_observed_live_bar`. It transfers only these
private owned parts to the Stage 5E-owned opaque bridge payload:

```text
strategy
recovery receipt
accepted semantic bar
schedule projection
sequence classification
optional boundary fingerprint
sequence_identity_fingerprint
sequence_observed_at
sequence_expires_at
```

`Stage5eB3bObservedLiveBarBridgePayload` is defined in the Stage 5E B3B owner
with private fields. It has no constructor other than the consume-seal method,
no getters, no generic `into_parts`, no reverse conversion, no callback and no
intent API. B3B consumes this payload immediately to produce its own receipt.
There is no second B3B consumer, no Stage 5C-created consume seal and no
alternate cross-module extraction surface.

### R6 required evidence

The implementation package must prove that final sequence identity is absent
before classification and changes for classification/boundary changes; that a
sequence cannot be created already expired; that only the B3B issuer creates a
consume seal and calls `consume_for_b3b`; and that no source fingerprint can be
used where the final sequence identity is required. The existing no-I/O,
no-callback and no-intent restrictions remain unchanged.

## R6 additive implementation outcome

The implementation adds only the accepted private authority chain:

```text
accepted Stage 4 dynamic Open state
→ validated normalized schedule projection
→ Stage 5C pre-classification candidate
→ sealed discrete-grid classifier
→ post-classification canonical sequence identity
→ consuming B3B seal
→ fresh B3C continuation binding
```

The production bridge is marked by
`STAGE5E-B3C-R6-SEALS: additive-no-io-v1` in the Stage 5C owner and
`STAGE5E-B3C-PRODUCTION-BRIDGE: trusted-no-io-v1` in the Stage 5E owner.
Stage 4 retains the exact accepted `BrokerMarketSessionState` privately and
projects only `Open`.

The old `private-no-io-v1` B3C enclave is retained only as a hash-pinned legacy
test oracle. Its `UnverifiedMarketSequenceSource` cannot enter the production
R6 path.

No strategy callback is invoked; no strategy state is mutated; no executable
intent is created. Redis, FINAM I/O, transport, dispatch, runtime-live,
autonomous loops and broker execution remain closed. The next stage must be
separately designed and reviewed before any callback-capable continuation is
allowed.

## Implementation-r1 review closure

The conditional review of `25fea30` found that the original B3B implementation
consumed the Stage 5C receipt before validation and returned an internal
payload on block. Implementation-r1 repairs that mismatch without opening any
new execution surface.

B3B now issues a private one-use `Stage5eB3bPreflightSeal` and obtains only a
borrowed, non-decomposable view of the exact Stage 5C receipt. Every binding
check is completed against that view. `consume_for_b3b` is called only after
preflight succeeds. A blocked transition owns and returns the original
`Stage5eObservedLiveBarWithSequenceEvidence`, so the existing B3B entry is the
single retry transition and strategy/recovery ownership cannot become a
payload dead end.

Block reasons are classified exactly:

```text
RetrySameReceipt:
  ClockBeforeEffectiveObservation
  BarObservedInFuture

RefreshScheduleRequired:
  EvidenceExpired
  BarOutsideSelectedOpenWindow

TerminalIntegrityBlock:
  InstrumentMismatch
  SequenceIdentityMissing
  SequenceClassificationMismatch
```

`RefreshScheduleRequired` preserves the complete receipt but does not authorize
reuse of expired evidence. A future provider/registry attachment must define a
separately reviewed refresh transition. Production normalized-schedule input,
Redis, FINAM and runtime-live therefore remain closed.

The canonical no-I/O behavioral proof now starts from a real validated Stage 4
bootstrap and `Stage4AcceptedPaperHostEvidence`, calls
`project_accepted_stage4_schedule`, validates normalized schedule and registry
evidence, and follows the sealed Stage 5C -> B3B -> B3C path. Closed, Break and
Maintenance Stage 4 states cannot enter the projection. The test also proves
exact minimum expiry and unchanged strategy state.

Both expected-close endpoints must be `TradableOpen`; only strict-interior grid
points may be Break/Clearing or Maintenance. Trading-day comparison remains
deliberately limited to the current UTC civil-day fixture contract. Overnight
or venue-defined trading-day mapping is blocked until a future separately
reviewed broker-neutral calendar provider exists.

The handoff package must include source-tree-bound result and stdout/stderr
artifacts for the Stage 5D 303-case and forbidden-surface 87-case negative
suites in addition to Cargo and provenance evidence. The official provenance
matrix contains 207 cases, including seven implementation-r1 mutations for
blocked output, retry ownership, disposition taxonomy and canonical Stage 4
test bypass/removal.
