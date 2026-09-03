# Stage 8B-P1-d2 projection-field and timestamp annex

Status: design-only review candidate. This annex does not authorize P1-d2
source implementation, Redis DB 0 activation, ACK/XACK settlement or any
broker/network action.

Accepted predecessor:
`4abb2fd9807adeb47f164a4025c7ac44d33679f6` (formal Stage 8B-P1-d1
governance closure).

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

The projection may consume only one linear, already durable P1-d1 Market
outcome and the preceding canonical paper-book target-position projection.
The P1-d1 outcome is the authority for the exact command, request/client/order/
trade identifiers, execution-bar identity, fill quantity, fill price and fill
source timestamp. The preceding projection is the authority for existing
target quantity and average price.

No caller may supply a second copy of those facts. Redis entry IDs, consumer
identity, process epoch, callback time and wall clock are never field or
timestamp authorities. `Utc::now()` and equivalent system-clock reads are
forbidden from canonical projection construction and canonical replay bytes.

Input validation is fail-closed before ACK callback or broker-truth mutation.
It rejects account, instrument, request, client-order, broker-order,
broker-trade, command, side, quantity, price, bar-identity or chronology
conflicts; duplicate/conflicting target-position rows; and a nonzero prior
position without an average price.

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
the existing linear owner and monotonic `total_sequence`, not fabricated
sub-millisecond offsets.

## Exact decimal representation

All quantities, prices and zero values remain `rust_decimal::Decimal`. The
P1-d2 implementation must reuse the accepted Stage 5G canonical exact-decimal
encoding (`Decimal::serialize()`); it must not pass through `f32`/`f64`, round,
rescale or stringify and reparse values.

`qty` and `filled_qty` preserve the exact command quantity representation.
`price` and any resulting non-flat average preserve deterministic Decimal
arithmetic. Canonical zero for newly materialized fields is
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

The resulting average is exact:

```text
q1 == 0                                  -> None
q0 == 0                                  -> Some(p)
sign(q0) == sign(d)                      -> Some((abs(q0)*a0 + abs(d)*p) / abs(q1))
sign(q0) != sign(d) and sign(q1)==sign(q0) -> Some(a0)
sign(q0) != sign(d) and sign(q1)==sign(d)  -> Some(p)
```

A nonzero `q0` requires `a0 = Some(_)`. Decimal overflow or nonrepresentable
division fails closed; no rounding policy is introduced in this annex.

| Field | Exact value |
|---|---|
| `account_id` | exact command account as `BrokerAccountId` |
| `instrument` | exact command `InstrumentId` |
| `qty` | exact `q1` |
| `avg_price` | formula above; `None` iff `q1 == 0` |
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
sorting/deduplication authority. One P1-d2 package uses one fresh monotonic
`total_sequence` allocated by the existing Stage 5G owner.

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

## Linear application and durability order

The later implementation must preserve this order under one accepted owner:

```text
1. consume the linear P1-d1 outcome after durable provider outcome;
2. validate the complete projection bundle without effects;
3. apply the exact matching CommandAck through the Stage 5G ACK owner;
4. apply the event-scoped BrokerTruthSnapshot through the same Stage 5G owner;
5. persist and reread the post-feedback recovery seal;
6. XACK the originating M10 last.
```

No successful ACK is externally visible without the corresponding order,
trade and resulting position becoming part of the same recoverable feedback
transition. A crash before completion must resume from durable authority; it
must not repeat the Hybrid callback or blindly invoke the provider.

This annex does not authorize those effects. It freezes the data contract that
the P1-d2 source implementation must satisfy after separate acceptance.

## Next implementation slice after acceptance

Annex acceptance may authorize only **P1-d2 Market feedback source**:

- one opaque linear projection bundle;
- exact values frozen above;
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
