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
