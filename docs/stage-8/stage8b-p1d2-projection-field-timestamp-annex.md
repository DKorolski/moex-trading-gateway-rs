# Stage 8B-P1-d2 projection-field and timestamp annex R1A

Status: R1A design-only review candidate — intermediate ACK-stage recovery
seal closure. This annex does not authorize P1-d2 source implementation,
Redis DB 0 activation, ACK/XACK settlement or any broker/network action.

Accepted predecessor:
`4abb2fd9807adeb47f164a4025c7ac44d33679f6` (formal Stage 8B-P1-d1
governance closure).

Reviewed R1 candidate corrected by this R1A:
`e398cbed771e5617f07fcab6734bc1d7b172a371`.

This annex exact-freezes the first P1-d2 implementation slice: one successful
P1-d1 Market fill is projected into broker-neutral order, trade, position,
truth and ACK values and is then offered to the already accepted Stage 5G
owner. Limit, cancel, expiry, partial-fill and operational deployment behavior
remain later work.

The projection contract is versioned as:

```text
moex.stage8b.p1d2.market-feedback.v1
```

## Authorities and input set

The P1-d1 Market outcome is deterministic but is not, by itself, a durably
finalized command-lifecycle authority. P1-d2 projection construction may
consume only an opaque linear `Stage8bP1d2FinalizedMarketFeedbackInput` issued
after the exact P1-d1 outcome has crossed the existing Stage 6/7 durable
settlement boundary. The input also retains the authenticated Stage 5 source
pre-position and the preceding canonical paper-book target-position
projection.

The finalized input is the sole authority for the exact command,
request/client/order/trade identifiers, execution-bar identity, fill quantity,
fill price, both canonical clocks and Stage 7 terminal facts. The
authenticated Stage 5 source is the authority for `pre_position_qty`; the
preceding paper-book projection supplies its average-price evidence.

No caller may supply a second copy of those facts. Redis entry IDs, consumer
identity, process epoch, callback time and wall clock are never field or
timestamp authorities. `Utc::now()` and equivalent system-clock reads are
forbidden from canonical projection construction and canonical replay bytes.

Input validation is fail-closed before ACK callback or broker-truth mutation.
It rejects account, instrument, request, client-order, broker-order,
broker-trade, command, side, quantity, price, bar-identity, durable-finalization
or chronology conflicts; duplicate/conflicting target-position rows; a prior
quantity different from the authenticated Stage 5 `pre_position_qty`; and a
nonzero prior position without an average price.

## Durable finalized-feedback authority

Exactly one crate-private linear bridge may mint
`Stage8bP1d2FinalizedMarketFeedbackInput`. It consumes the complete P1-d1
Market outcome bundle together with the exact Stage 7/Stage 6 durable owner and
performs, in order:

```text
1. durably apply the exact Stage6dPaperOutcome;
2. verify Stage6dPaperExecutionReport against request/client/order/trade,
   account, instrument, command and P1-d1 outcome identities;
3. finalize the exact Stage 7 request with observed_at = T_receipt;
4. persist and reread the exact RequestFinalized record;
5. prove the request is terminal under the current Stage 6/7 owner;
6. mint the opaque P1-d2 finalized-feedback input.
```

Projection construction, sequence allocation and every Stage 5G ACK/truth
mutation are forbidden before step 6. The generic service's host-clock
finalization path is not an authority for this P1 flow. Both first execution
and replayed finalization use exactly `T_receipt`; `Utc::now()` is forbidden.

After restart, this authority may be reconstructed only from the exact
authenticated Stage 6/7 finalized facts plus the retained deterministic P1-d1
outcome/execution-bar evidence. It is never decoded from an ACK row, broker
truth output or caller-supplied DTO.

The recovery issuer does not call the P1-d1 provider again. It starts from the
retained originating M10 identity and accepted command in the Stage 6/7
journal, read-only selects the unique first schedule-eligible canonical M10
after that predecessor, and verifies that recomputed deterministic
order/trade IDs equal the finalized journal facts. Its semantic ID, payload
hash, open/close timestamps, open price and command quantity then reproduce
the feedback input. Zero or multiple candidates, changed bytes or any derived
ID mismatch is reconciliation-required with no feedback effect.

## Two canonical clocks

The accepted canonical final M10 carries both boundaries. Define:

```text
T_source  = execution_bar.open_ts
T_receipt = execution_bar.close_ts
```

`T_source` is the P1-d1 Market execution timestamp. `T_receipt` is the first
deterministic point at which that final M10 outcome is observable. The
execution authority has already proved:

```text
T_receipt - T_source = 600 seconds
T_receipt = exact execution-bar Redis millisecond ID
```

The P1-d2 projection must copy both timestamps from the consumed authority; it
must not rederive either from a local clock. All emitted timestamps use UTC
with exact millisecond precision:

| Value | source_ts | received_ts |
|---|---|---|
| `BrokerOrderSnapshot` | `Some(T_source)` | `T_receipt` |
| `BrokerTradeSnapshot` | `T_source` | `T_receipt` |
| `BrokerPositionSnapshot` | `Some(T_source)` | `T_receipt` |
| `BrokerTruthSnapshot` | n/a | `T_receipt` |
| `CommandAck` | n/a | `T_receipt` |

Equal ACK and truth receipt timestamps are intentional. Ordering comes from
two distinct consecutive values allocated by the same existing linear Stage
5G owner, not fabricated sub-millisecond offsets:

```text
seq_ack   = next_total_sequence
seq_truth = checked_add(seq_ack, 1)

required: seq_truth == seq_ack + 1
```

Overflow, reuse, reversal, a skipped value or allocation from a different
sequence domain fails closed before ACK. The pair is retained in the durable
feedback transition and reproduced exactly on replay. `seq_ack` belongs to the
`Stage5gMockAckEvent`; `seq_truth` belongs to the subsequent
`Stage5gOrderPositionEvidence`.

## Exact decimal representation

All quantities, prices and zero values remain `rust_decimal::Decimal`. The
P1-d2 implementation must reuse the accepted Stage 5G canonical exact-decimal
encoding (`Decimal::serialize()`); it must not pass through `f32`/`f64` or
stringify and reparse values. Rounding/rescaling is forbidden except for the
single average-price canonicalization rule below.

`qty` and `filled_qty` preserve the exact command quantity representation.
`price` preserves the exact execution-bar Decimal representation. Canonical
zero for newly materialized fields is
`Decimal::ZERO` (positive zero, scale zero). Negative zero, alternate zero
scales and numerically equal values with different exact Decimal bytes are
not interchangeable canonical inputs.

## BrokerOrderSnapshot — Market Filled v1

Exactly one order row is emitted:

| Field | Exact value |
|---|---|
| `account_id` | exact command account as `BrokerAccountId` |
| `broker_order_id` | `Some(P1-d1 BrokerOrderId)` |
| `client_order_id` | `Some(command.client_order_id)` |
| `instrument` | exact command `InstrumentId` |
| `side` | exact command side |
| `order_type` | `OrderType::Market` |
| `time_in_force` | `Some(TimeInForce::Day)` |
| `status` | `OrderStatus::Filled` |
| `lifecycle` | `BrokerOrderLifecycle::Terminal` |
| `qty` | exact command quantity |
| `filled_qty` | exact command quantity |
| `remaining_qty` | `Some(Decimal::ZERO)` |
| `limit_price` | `None` |
| `broker_asset_id` | `None` |
| `board` | `None` |
| `expiration_date` | `None` |
| `source_ts` | `Some(T_source)` |
| `received_ts` | `T_receipt` |

Paper projection does not invent broker metadata from the venue symbol.

## BrokerTradeSnapshot — Market Fill v1

Exactly one trade row is emitted:

| Field | Exact value |
|---|---|
| `account_id` | exact command account as `BrokerAccountId` |
| `broker_trade_id` | exact P1-d1 `BrokerTradeId` |
| `broker_order_id` | `Some(P1-d1 BrokerOrderId)` |
| `client_order_id` | `Some(command.client_order_id)` |
| `instrument` | exact command `InstrumentId` |
| `side` | exact command side |
| `qty` | exact command quantity |
| `price` | exact execution-bar open |
| `gross_amount` | `None` |
| `commission` | `Some(Decimal::ZERO)` |
| `broker_asset_id` | `None` |
| `board` | `None` |
| `expiration_date` | `None` |
| `source_ts` | `T_source` |
| `received_ts` | `T_receipt` |

`commission = Some(Decimal::ZERO)` is the required explicit policy-v1 zero.
`gross_amount = None` is intentionally not inferred as `price * qty`: the
instrument multiplier and settlement currency are outside this projection.
There is no separate fee field in `BrokerTradeSnapshot`; no hidden fee value
may be introduced elsewhere.

## BrokerPositionSnapshot — resulting target position

Exactly one target-instrument position row is emitted for the full Market
fill, including an explicit zero row when the result is flat. Define signed
fill delta `d`: Buy is `+fill_qty`, Sell is `-fill_qty`; prior quantity and
average are `q0` and `a0`; `q1 = q0 + d`; fill price is `p`.

Before arithmetic, numeric Decimal equality must prove:

```text
q0 == authenticated_stage5_pre_position_qty
```

If a prior target row exists, its exact `q0/a0` is used after that binding. A
missing row is allowed only when the authenticated paper-book generation proves
the first/empty state and `pre_position_qty == Decimal::ZERO`; its canonical
interpretation is `q0 = Decimal::ZERO`, `a0 = None`. After any earlier
position event, including an explicit flat event, absence is not equivalent to
flat and fails closed. A present zero row requires `a0 = None`; a present
nonzero row requires `a0 = Some(_)`.

First derive the numeric average candidate:

```text
q1 == 0                                    -> None
q0 == 0                                    -> Some(p)
sign(q0) == sign(d)                        -> Some((abs(q0)*a0 + abs(d)*p) / abs(q1))
sign(q0) != sign(d) and sign(q1)==sign(q0) -> Some(a0)
sign(q0) != sign(d) and sign(q1)==sign(d)  -> Some(p)
```

Every non-`None` candidate then passes one mandatory canonicalization:

```text
P1D2_AVG_PRICE_SCALE    = 8
P1D2_AVG_PRICE_ROUNDING = RoundingStrategy::MidpointNearestEven

canonical_avg_price =
  candidate.round_dp_with_strategy(8, MidpointNearestEven)
           .rescale_exactly_to(8)
```

The scale and rounding mode are compile-time contract constants and cannot be
configured at runtime. `rescale_exactly_to(8)` means the resulting
`Decimal::serialize()` must have positive scale 8; inability to construct that
representation or checked arithmetic overflow fails closed before ACK. This
policy applies equally to opening, same-side increase, reduction and flip, so
all non-flat output averages have one canonical representation.

Required golden vectors include:

```text
exact division: q0=1 a0=100 d=1 p=102 -> 101.00000000
repeating long: q0=1 a0=100 d=2 p=100.5 -> 100.33333333
repeating short: q0=-1 a0=100 d=-2 p=100.5 -> 100.33333333
flat: q0=1 a0=100 d=-1 p=99 -> None
flip: q0=1 a0=100 d=-2 p=99.5 -> 99.50000000
```

The same active and restart input must produce byte-identical
`Decimal::serialize()` output. Tie vectors must prove
`MidpointNearestEven`; implicit/default rounding is forbidden.

| Field | Exact value |
|---|---|
| `account_id` | exact command account as `BrokerAccountId` |
| `instrument` | exact command `InstrumentId` |
| `qty` | exact `q1` |
| `avg_price` | canonical scale-8 result above; `None` iff `q1 == 0` |
| `unrealized_pnl` | `None` |
| `source_ts` | `Some(T_source)` |
| `received_ts` | `T_receipt` |

`unrealized_pnl = None` means not modeled. It must not be confused with an
explicit monetary zero. Account-wide row count is diagnostic only; flatness
remains target instrument plus exact nonzero quantity.

## BrokerTruthSnapshot — event-scoped feedback package

P1-d2 v1 emits one event-scoped broker-neutral truth package, not an invented
full-account broker snapshot:

| Field | Exact value |
|---|---|
| `account_id` | exact command account as `BrokerAccountId` |
| `orders` | vector containing the one order row above |
| `positions` | vector containing the one resulting target-position row above |
| `cash` | `None` |
| `trades` | vector containing the one trade row above |
| `instruments` | empty vector |
| `received_ts` | `T_receipt` |

No unrelated account order, position, cash or instrument row is copied or
fabricated. Vector order is the listed canonical order; before immutable
Stage 5G fingerprinting, the accepted Stage 5G canonicalizer remains the sole
sorting/deduplication authority. The truth application uses `seq_truth`; it
must not reuse `seq_ack`.

## CommandAck — exact matching request

Exactly one ACK is emitted for the successful Market fill:

| Field | Exact value |
|---|---|
| `request_id` | exact command `StrategyRequestId` |
| `client_order_id` | `Some(command.client_order_id)` |
| `broker_order_id` | `Some(P1-d1 BrokerOrderId)` |
| `status` | `CommandAckStatus::Accepted` |
| `reason` | `None` |
| `received_ts` | `T_receipt` |

The ACK may resolve only the exact pending request. `ClientOrderId` does not
replace `StrategyRequestId`; a broker order ID does not authorize a different
pending slot. Duplicate byte-identical feedback is idempotent. Any duplicate
with a changed field, timestamp, Decimal bytes or vector membership is a
conflict and fails closed before callback or state mutation.

The ACK event uses `seq_ack`. The subsequent truth event uses `seq_truth`.
Both carry `T_receipt`, and `seq_truth == seq_ack + 1` is mandatory.

## Linear application and durability order

The later implementation must preserve this order under one accepted owner:

```text
1. consume the linear P1-d1 deterministic Market outcome;
2. durably apply Stage6dPaperOutcome and verify its exact report;
3. durably finalize Stage7 with observed_at = T_receipt and reread it;
4. mint Stage8bP1d2FinalizedMarketFeedbackInput;
5. cross-bind q0 to authenticated Stage5 pre_position_qty;
6. construct and validate the complete ACK/truth bundle without effects;
7. allocate consecutive seq_ack and seq_truth from one Stage5G owner;
8. apply the exact matching CommandAck at seq_ack;
9. attach the resulting Stage5gOrderPositionSession;
10. export Stage5gCleanRestartSource::OrderPositionAwaiting and persist and
    reread the intermediate ACK-stage recovery seal S_ack as
    OrderPositionAwaitingCommitted;
11. apply the event-scoped BrokerTruthSnapshot at seq_truth;
12. persist and reread the final post-truth recovery seal S_truth;
13. XACK the originating M10 last. This is allowed only after S_truth.
```

No Stage5G mutation is allowed before durable Stage7 finalization. No
successful ACK is externally visible without the corresponding order, trade
and resulting position becoming part of the same recoverable feedback
transition. A crash before completion must resume from durable authority; it
must not repeat the Hybrid callback or blindly invoke the provider.

`S_ack` is internal durable recovery authority, not an externally published
ACK. It contains and proves the exact resolved ACK/order-position continuation
and canonical `seq_ack`. On restart after `S_ack`, the implementation derives
the already reserved truth sequence only as:

```text
recovered_seq_truth = checked_add(recovered_seq_ack, 1)
```

Allocating a new unrelated truth sequence after restart is forbidden. The
persisted ACK slot/restart projection must cross-bind the recovered sequence
to the original pair. Broker truth is forbidden before `S_ack` is durably
persisted and reread. The originating source M10 remains pending through both
seals and cannot be XACKed until `S_truth` is durably persisted and reread.

## Required crash/replay matrix for source acceptance

The later source implementation must prove at least:

```text
crash after Stage6 outcome, before RequestFinalized
  -> Stage6/7 recovery only; no Stage5G ACK/truth

crash after RequestFinalized, before ACK
  -> reconstruct exact finalized input; apply same seq_ack/seq_truth bundle

crash after ACK in memory, before S_ack is committed
  -> ACK is not durable; reconstruct pre-ACK authority and deterministically
     replay the exact ACK at seq_ack; do not replay provider

crash after S_ack reread, before truth
  -> restart from OrderPositionAwaitingCommitted; ACK is durable; never
     reapply ACK; derive seq_truth from seq_ack and apply only exact truth

crash after truth in memory, before S_truth
  -> restart from S_ack; replay only exact truth; no ACK/provider replay

crash after S_truth reread, before M10 XACK
  -> resolve source only; no provider, ACK, truth or Hybrid replay; XACK last
```

All fields, timestamps, sequences, projection fingerprints and canonical
average Decimal bytes must be identical on active and restart paths.

The ACK-stage durability claim begins only after `S_ack` has been persisted
and reread. A completed in-memory ACK callback before that frontier is not
durable evidence. Conversely, once `S_ack` is reread, reapplying ACK is a
conflict rather than an idempotent continuation. `S_truth` is the only final
post-feedback seal and the only authority that permits source XACK.

The source slice also emits one redacted, non-authoritative feedback audit
digest binding the P1-d1 outcome hash, Stage 6 report and Stage 7 final-record
identity, `T_source`, `T_receipt`, `seq_ack`, `seq_truth`, order/trade/position
projection hashes and post-feedback seal generation/hash. It contains no
secret and grants no replay, settlement or writer capability.

This annex does not authorize those effects. It freezes the data contract that
the P1-d2 source implementation must satisfy after separate acceptance.

## Next implementation slice after acceptance

Annex acceptance may authorize only **P1-d2 Market feedback source**:

- one opaque linear finalized-feedback authority and projection bundle;
- exact values frozen above;
- deterministic Stage7 finalization at `T_receipt`;
- consecutive `seq_ack`/`seq_truth` from one Stage5G owner;
- intermediate `OrderPositionAwaitingCommitted` ACK-stage seal before truth;
- truth-only continuation after that seal and final `S_truth` before XACK;
- authenticated Stage5 pre-position binding and scale-8 nearest-even average;
- existing Stage 5G ACK and order/position authorities;
- pure and isolated tests first;
- no operational DB 0 activation.

P1-d3 working limit/cancel, P1-d4 crash/replay closure, P1-e supervisor and
P1-f operational acceptance remain separate stages. A governance-only
current-tree authority rebind is deferred until accepted P1-d2 production
source exists; the current authority remains pinned to accepted P1-d1.

## Explicitly closed

- P1-d2 production source implementation;
- operational Redis DB 0 activation and VPS paper service activation;
- source-M10 XACK and ACK feedback effects;
- P1-d3 limit/cancel/expiry and partial fills;
- FINAM HTTP POST/DELETE and broker network dispatch;
- real orders and runtime-live / `LiveReady`;
- protective/stop/bracket/replace/multi-leg execution;
- Generation-2 production authorization.
