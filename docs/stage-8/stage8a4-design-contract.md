# Stage 8A-4 design R1 — broker-truth reconciliation contract

## Authority and scope

Stage 8A-3 R2 is independently accepted and closed at
`012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d`; the final review SHA-256 is
`2e969db40bd847230f4df426ce3ee235f2f2273b87a778297b4588bf1f127232`.
That acceptance opens Stage 8A-4 reconciliation only.

This R1 package is a design freeze. It changes no production Rust, constructs
no reconciliation result and performs no broker or Redis operation. Its
acceptance may open only a separately reviewed Stage 8A-4 implementation R1.

## Normative ownership boundary

Reconciliation is derived from fresh read-only FINAM broker truth normalized
into `broker_core::BrokerTruthSnapshot`. An HTTP status, Stage 8A-3 classifier
candidate, transport error or historical M3d2 result is never broker truth.

The future implementation is a pure deterministic reducer with three linear
inputs:

1. an opaque durable request context reconstructed from accepted Stage 6/7
   identity and attempt state;
2. an opaque fresh-truth admission produced from canonical snapshots plus
   per-source completeness/freshness evidence;
3. an immutable, fingerprint-bound reconciliation policy issued by trusted
   configuration authority rather than caller-selected numbers.

No input may implement `Clone`, `Serialize` or a public raw-identity getter.
Diagnostics are redacted counts, categories, time ages and hashes only.

## Durable request context

The request context binds all of the following before any correlation:

- exact `StrategyRequestId` and exact stable `ClientOrderId`;
- endpoint kind PLACE or CANCEL;
- exact broker account identity;
- exact FINAM venue symbol and canonical `InstrumentId` registry identity;
- side and original quantity;
- known `BrokerOrderId(String)` when one is already durable;
- durable attempt/effect boundary timestamps;
- bounded correlation event window derived from trusted durable timestamps;
- Stage 8A-3 classification binding/reason where applicable;
- current Stage 6/7 journal generation, seal and request-state fingerprint.

The event-time policy is bounded, non-zero and configuration-fingerprinted.
Its values cannot be supplied ad hoc by the caller. Event time is a fallback
correlation constraint, never an identity replacement.

## Fresh broker-truth admission

The admitted package must include canonical orders, trades, positions and the
instrument registry for the exact account and target instrument. Each source
has an explicit status, observation interval, pagination/completeness marker
and trusted-time age. Admission fails closed unless:

- every required source is present, decoded and complete;
- all paginated orders/trades pages are exhausted without truncation;
- source acquisition starts after the durable possible-effect boundary;
- source timestamps are not in the trusted future;
- source ages and cross-source skew satisfy the sealed policy;
- the account matches exactly;
- the target FINAM venue symbol resolves to exactly one canonical instrument;
- every row used for correlation has compatible account/instrument identity;
- account-wide active/unknown/orphan order counts are retained as safety data.

An empty but proven-complete source can be admitted, but emptiness never proves
rejection, cancellation, flatness or no broker effect. Missing position rows
are not synthetic zero positions.

## Deterministic correlation precedence

Correlation is performed in this exact order:

1. exact stable `ClientOrderId` or equally exact broker-native correlation;
2. known exact `BrokerOrderId(String)`;
3. exact account + instrument + side + quantity + bounded event time.

The first tier containing evidence owns the decision. Lower tiers may confirm
or contradict it but cannot replace it. If exact client and broker identities
point to different orders, the result is `Conflict`. More than one plausible
order at the selected tier is `Conflict`; selection by first row, latest row,
status priority or collection order is forbidden.

Broker-neutral and FINAM venue symbols are not interchangeable fallbacks.
Unknown/ambiguous instrument registry identity is `StillUnknown` or
`Conflict`, never a lower-tier match.

## Supporting evidence

Trades and target-instrument position support a selected order; they never
select an order by themselves. Fill outcomes require the selected order plus
compatible trade evidence whose identities, side and quantities do not
contradict the order. Net position is account/instrument safety evidence only:
other strategies or orders can offset it, so non-zero position does not prove
this request filled and zero/missing position does not prove no fill.

Account-wide active orders are a safety guard. Target-instrument active orders
are lifecycle truth. Unrelated active orders do not rewrite the selected
request outcome, but keep new-command readiness blocked until the separate
safety owner clears them.

## Outcome algebra

The only reconciliation outcomes are:

- `ExactWorking`;
- `ExactPartiallyFilled`;
- `ExactFullyFilled`;
- `ExactTerminalRejected`;
- `ExactTerminalCancelled`;
- `ExactTerminalExpired`;
- `Conflict`;
- `StillUnknown`.

An unknown broker order status is `StillUnknown`. Source contradiction,
identity contradiction, incompatible fill/trade quantities or multiple
plausible matches is `Conflict`. Empty, stale, incomplete, missing or merely
absent truth is `StillUnknown`.

`ProvenNoMatch` remains unconstructible throughout Stage 8A.

## Durable application boundary

The pure reducer cannot mutate a journal, ACK, runtime state or readiness. A
future crate-private application bridge may consume an exact outcome only after
revalidating current journal generation, seal, request identity and attempt
state. It must append an identity-preserving transition before publishing any
derived ACK/readiness effect. `Conflict` and `StillUnknown` only preserve a
reconciliation hold and operator disarm state.

No outcome grants retry, re-arm or resend authority. Any later send requires a
new terminally permitted durable request, new `StrategyRequestId`, new
`ClientOrderId`, new operator arm and a separately accepted execution gate.

## Implementation decomposition after design acceptance

1. Stage 8A-4 implementation R1: private types, fresh-truth admission and pure
   deterministic correlation over synthetic/canonical fixtures only.
2. Stage 8A-4 lifecycle closure: crate-private identity-preserving durable
   transition bridge and crash/restart tests, still with no network send.
3. Stage 8A-4 aggregate closure: inherited gates, exact negatives and immutable
   evidence. Its acceptance may open Stage 8A-5 only.

Every implementation sub-slice requires separate independent review.

## Closed surfaces

Stage 8A-4 design opens none of the following: FINAM POST/DELETE, order
transport, same-request retry, automatic resend, Redis live command consumer,
broker dispatch, runtime-live, real strategy orders, STOP/SLTP/bracket,
replace/multi-leg behavior, Stage 8A-5 implementation or Stage 8B.
