# Stage 8A-4 design R2 — broker-truth reconciliation contract

## Authority and scope

Stage 8A-3 R2 is independently accepted and closed at
`012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d`; the final review SHA-256 is
`2e969db40bd847230f4df426ce3ee235f2f2273b87a778297b4588bf1f127232`.
Stage 8A-4 Design R1 retained the architecture but was not accepted. This R2
package closes its three P1 findings and remains docs/scripts/evidence only.

No production Rust, Cargo or workflow is changed. No reconciliation result is
constructed and no broker or Redis operation is performed. Independent R2
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
   source-specific completeness/freshness evidence;
3. an immutable, fingerprint-bound reconciliation policy issued by trusted
   configuration authority rather than caller-selected numbers.

The future admission wrapper and policy capability expose no `Clone`,
`Serialize` or public raw-identity getter. The canonical snapshot remains an
internal owned field. Diagnostics are redacted counts, categories, ages and
hashes only.

## Durable request context and exact order shape

The request context binds all of the following before correlation:

- exact `StrategyRequestId` and exact stable `ClientOrderId`;
- endpoint kind PLACE or CANCEL;
- exact broker account identity;
- exact FINAM venue symbol and canonical `InstrumentId` registry identity;
- side and original quantity;
- exact `OrderType`;
- current Stage 8 `TimeInForce` (`DAY`);
- exact normalized LIMIT price for LIMIT, or explicit absence for MARKET;
- known `BrokerOrderId(String)` when one is already durable;
- durable attempt/effect boundary timestamps;
- bounded correlation event window derived from trusted durable timestamps;
- Stage 8A-3 classification binding/reason where applicable;
- current Stage 6/7 journal generation, seal and request-state fingerprint.

For CANCEL, the shape is the exact durable shape of the target original order,
not the cancel command's lack of type, TIF or price. LIMIT price uses the
canonical exact decimal representation; floating conversion, tolerance and
caller-selected tolerance are forbidden.

The event-time policy is bounded, non-zero and configuration-fingerprinted.
Its values cannot be supplied ad hoc by the caller. Event time is a fallback
constraint, never an identity replacement.

## Source-specific fresh broker-truth admission

The admitted package includes canonical orders, trades, positions and
instrument evidence for the exact account and target instrument. Every source
must be present, fully decoded, fresh under sealed policy, account-scoped and
acquired after the durable possible-effect boundary. Trusted-future timestamps,
excess age/skew or incomplete source evidence fail closed to `StillUnknown`.

### Account orders completeness

`GET /v1/accounts/{account_id}/orders` is a non-paginated account snapshot.
Its proof binds the exact account, trusted request-start and response-received
timestamps, full-body decode, absence of local truncation and sealed age/skew
policy. Cursor/page exhaustion is not invented for this endpoint. Absence from
the decoded list remains no-match-insufficient and never creates
`ProvenNoMatch`; undocumented terminal-history retention is not assumed.

### Account trades completeness

`GET /v1/accounts/{account_id}/trades` is bounded history with a requested
`limit`, inclusive `interval.start_time` and exclusive `interval.end_time`.
The policy-fingerprint-bound proof records each requested interval and limit,
returned count, trusted request/receive timestamps and an exact interval-union
fingerprint.

The admitted union must cover the whole sealed reconciliation event window as
`[start,end)` without a gap. `returned_count >= requested_limit` is incomplete.
A saturated interval is deterministically subdivided under a sealed bounded
policy, or admission returns `StillUnknown`; unbounded recursion and
caller-selected limits/windows are forbidden. Every terminal sub-interval must
be below its requested limit. Overlap/retry rows are normalized by exact
`BrokerTradeId` before arithmetic.

### Instrument registry completeness

Exact target resolution uses exact asset/params/schedule reads and proves one
canonical FINAM target. Cursor exhaustion is required only for full discovery
through `/assets/all`; fictitious pagination is not required for exact-target
reads. Ambiguous or unresolved identity is `StillUnknown` or `Conflict`.

### Conditional exact order lookup

When a durable `BrokerOrderId` is already known, read-only
`GET /v1/accounts/{account_id}/orders/{order_id}` is an optional strong tier-2
source. A successful observation is normalized into canonical order evidence.
It does not replace the account-wide orders safety snapshot. A 404, stale,
unavailable or undecodable response is `StillUnknown`, never `ProvenNoMatch`.
Disagreement with another exact identity source is `Conflict`. Historical
`CancelBrokerTruth::GetOrder` types remain oracle-only and are not reused as
Stage 8 authority.

An empty but proven-complete source can be admitted, but emptiness never proves
rejection, cancellation, flatness or no broker effect. Missing position rows
are not synthetic zero positions. Account-wide active/unknown/orphan order
counts remain safety data.

## Deterministic correlation precedence

Correlation is performed in this exact order:

1. exact stable `ClientOrderId` or equally exact broker-native correlation;
2. known exact `BrokerOrderId(String)` including an admitted exact GET-order;
3. account + exact canonical instrument/FINAM venue symbol + side + original
   quantity + `OrderType` + verifiable `DAY` TIF + exact LIMIT price (or MARKET
   no-price) + bounded trusted event time.

The first tier containing evidence owns the decision. Lower tiers may confirm
or contradict it but cannot replace it. Exact client/broker identities pointing
to different orders produce `Conflict`. Multiple plausible candidates at the
selected tier produce `Conflict`; first/latest/status/collection-order selection
is forbidden.

Tier 3 never weakens the durable shape. MARKET and LIMIT cannot cross-match,
different LIMIT prices cannot match, and contradictory TIF cannot match. If a
required broker shape field cannot be verified, the outcome is `StillUnknown`.
Broker-neutral and FINAM venue symbols are not interchangeable fallbacks.

## Supporting trades and trade identity

Trades and target-instrument position support a selected order; they never
select an order by themselves. `BrokerTradeId` is the primary trade identity.
Identical duplicate rows with one ID count once; the same ID with conflicting
material fields produces `Conflict`. This also applies across overlapping or
retried trade-history intervals.

A trade supports the selected order only through exact compatible broker-order
or client identity plus account, instrument and side. Position cannot supply a
missing order/trade identity. The sum of unique matching trade quantities must
agree with selected-order `filled_qty` under the sealed policy. Incomplete
trade truth cannot create an exact fill outcome.

Net position is account/instrument safety evidence only: unrelated activity can
offset it. Non-zero position does not prove this request filled and zero/missing
position does not prove no fill. Account-wide active orders are a safety guard;
target-instrument active orders are lifecycle truth.

## Orthogonal exact outcome algebra

The exact result is `ExactOrderState` with two independent dimensions:

- lifecycle: `Working`, `TerminalFilled`, `TerminalRejected`,
  `TerminalCancelled` or `TerminalExpired`;
- fill effect: `Zero`, `Partial { filled_qty }` or `Full { filled_qty }`.

It also binds the selected order and supporting deduplicated trade summary.
Therefore cancelled/expired after partial fill preserves both terminal status
and broker effect. `Conflict` and `StillUnknown` remain separate outcomes.

Quantity invariants are normative:

- `qty > 0` and `0 <= filled_qty <= qty`;
- present `remaining_qty == qty - filled_qty`;
- filled status requires `filled_qty == qty`;
- active partial requires `0 < filled_qty < qty`;
- rejected with non-zero fill is `Conflict` under the current pinned contract;
- cancelled/expired may have zero or partial fill;
- status/quantity/trade inconsistency is `Conflict`, never normalized away.

The mandatory semantic set covers working 0/N, active partial K/N, filled N/N,
cancelled 0/N, cancelled K/N, expired 0/N, expired K/N and rejected 0/N.
Contradictory filled K/N, rejected K/N, partial 0/N or N/N, remaining mismatch
and matching-trade quantity mismatch all produce `Conflict`.

Unknown broker status is `StillUnknown`. Empty, stale, incomplete, missing or
merely absent truth is `StillUnknown`. `ProvenNoMatch` remains unconstructible throughout Stage 8A.

## Durable application boundary

The pure reducer cannot mutate a journal, ACK, runtime state or readiness. A
future crate-private application bridge may consume an exact outcome only after
revalidating current journal generation, seal, request identity and attempt
state. It appends an identity-preserving transition before any derived effect.
`Conflict` and `StillUnknown` preserve reconciliation hold and operator disarm.

No outcome grants retry, re-arm or resend authority. Any later send requires a
new terminally permitted durable request, new identities, new operator arm and
a separately accepted execution gate.

## Implementation decomposition after design acceptance

1. Stage 8A-4 implementation R1: private types, source-specific fresh-truth
   admission and pure deterministic reducer over synthetic/canonical fixtures.
2. Stage 8A-4 lifecycle closure: crate-private identity-preserving durable
   transition bridge and crash/restart tests, still without network send.
3. Stage 8A-4 aggregate closure: inherited gates, exact negatives and immutable
   evidence. Its independent acceptance may open Stage 8A-5 only.

Every implementation sub-slice requires separate independent review.

## Closed surfaces

Stage 8A-4 Design R2 opens none of: FINAM POST/DELETE, order transport,
same-request retry, automatic resend, Redis live command consumer, broker
dispatch, runtime-live, real strategy orders, STOP/SLTP/bracket,
replace/multi-leg behavior, Stage 8A-5 implementation or Stage 8B.
