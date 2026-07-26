# Stage 5E-b3c source-authority freeze extension — design only

Status: proposed additive freeze extension. This package changes no Stage 4,
Stage 5C, broker, transport, runtime or strategy source. It exists to obtain a
reviewed contract before any such source change is permitted.

Baseline: `936250e675ac15b61a7a4e319b59e508cd834f30`.

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

## R1 exact governance contract

This R1 revision is the implementation contract; the JSON inventory is
normative and its complete canonical object and this complete plan are
hash-pinned by the active checker. No prose, owner, schema or algorithm may be
silently substituted.

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
Stage 4 dynamic-session projection. It is the sole producer of both a selected
tradable-open window and an optional discontinuity proof. A discontinuity is
valid only when the normalized interval set proves that no expected tradable
close exists between the stated canonical closes. A caller timestamp pair or
boolean can never stand in for that proof.

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
  exact optional boundary from the same accepted schedule projection proves
  no expected tradable close exists between the two canonical closes
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

The current close must be inside a `TradableOpen` interval with inclusive
closed endpoints. The complete closed range `[previous_close,current_close]`
must be covered by one same-trading-day normalized snapshot without holes or
overlaps. Every expected `t` must be covered by exactly one normalized
interval and none may be `TradableOpen`; only `BreakOrClearing` and
`Maintenance` qualify. Unknown, uncovered or cross-day ranges block. Overnight,
weekend and holiday inference are not authorized until a separately reviewed
multi-day coverage receipt exists.

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
