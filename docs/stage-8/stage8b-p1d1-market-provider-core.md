# Stage 8B-P1-d1 Market provider core

Status: source implementation review candidate.

Accepted policy predecessor:
`0d59d54d42fc29ae7b31359c1ded8efbd3a348fd`.

## Scope

P1-d1 implements only the first source slice authorized by the accepted P1-d0
policy. It adds:

1. one crate-private Stage5E bridge which consumes the retained opaque schedule
   projection and classifies an exact predecessor/candidate M10 pair;
2. a P1-c adapter that copies already validated canonical M10 facts into a
   read-only execution-bar observation;
3. linear `AwaitingExecutionBar`, `ExecutionEligible` and
   `MarketDispatchReady` type states;
4. a pure deterministic Market provider that fills the complete quantity at
   the exact eligible bar open and binds the source timestamp to that bar's
   open timestamp;
5. full domain-separated deterministic `BrokerOrderId` and `BrokerTradeId`.

The production entry seam which combines the retained Stage5E projection with
the P1 command remains crate-private. There is no serialized schedule receipt,
boolean eligibility flag, raw-calendar parser or public constructor for the
eligibility capability.

## Ordering and authority

The allowed type-state path is:

```text
P1-c canonical command publication
        |
        v
AwaitingExecutionBar
        |
        | exact read-only canonical later M10
        | + accepted Stage5E sequence projection
        v
ExecutionEligible
        |
        | exact linear Stage6dPaperDispatchReceipt
        v
MarketDispatchReady
        |
        v
MarketOutcomeBundle
```

`AwaitingExecutionBar` and `ExecutionEligible` expose no provider call.
`MarketDispatchReady` cannot be constructed from IDs or flags; it requires the
fsync-backed dispatch receipt. Thus `DispatchAttemptRecorded` remains the sole
effect boundary owned by Stage6/7.

No input returns the same linear wait owner. A blocked observation also retains
that owner but grants no dispatch/provider capability. Same-bar execution is
rejected before schedule classification.

The Stage5E classifier is reused without a second calendar implementation. The
only positive results are `Contiguous` and
`ApprovedNonTradableBoundary`. A skipped TradableOpen grid point, unknown or
uncovered interior evidence is `ExecutionBarGap`; cross-day classification,
expired/not-yet-observed evidence and a future candidate all fail closed.

## Exact Market v1 result

For a Market/Day PLACE with `ttl_ms = None`:

```text
fill_qty            = command.qty
fill_price          = execution_bar.open (exact Decimal)
fill_source_ts      = execution_bar.open_ts_utc_ms
remaining_qty       = 0 (materialized in P1-d2)
Stage6 outcome      = MarketFilled
```

The order ID is:

```text
P1D-O- + sha256(
  moex.stage8b.p1d.order-id.v1 NUL
  operational_identity_sha256 NUL
  StrategyRequestId NUL
  canonical_command_sha256)
```

The trade ID is:

```text
P1D-T- + sha256(
  moex.stage8b.p1d.trade-id.v1 NUL
  BrokerOrderId NUL
  execution_bar_semantic_id_sha256 NUL
  1)
```

Redis entry ID, consumer name, process epoch and observation wall clock are not
inputs to either identifier or the fill result.

## Deliberately closed

P1-d1 cannot:

- consume the operational Redis DB0 command group;
- invoke a provider before exact next-bar eligibility and durable dispatch;
- apply BrokerOrder/Trade/Position projections;
- produce or settle an ACK;
- XACK the originating M10;
- maintain Working LIMIT/cancel/expiry state;
- call FINAM POST/DELETE or any broker transport;
- enable runtime-live or real orders.

The outcome bundle retains the Stage6 receipt and normalized Stage6 outcome for
P1-d2, but its settlement decomposition remains crate-private. Public accessors
expose evidence only and all feedback/ACK/XACK flags return false.

## Review boundary

P1-d1 acceptance authorizes only preparation of the exact P1-d2 projection
field/timestamp freeze identified by independent P1-d0 review. P1-d2 source
must not start until that annex is accepted. P1-d3 additionally requires the
separate command-outcome versus active-order-transition split and exact cancel
timing freeze.
