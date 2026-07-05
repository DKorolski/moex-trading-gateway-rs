# M4-2b-1 FINAM canonical broker-truth mapper

Status: no-live mapper parity step.

M4-2b-1 starts wiring FINAM read-only DTOs into the broker-neutral
`BrokerTruthSnapshot` model introduced in M4-1c-a/M4-2a. This is still not a
live trading expansion and does not connect runtime or command consumers to real
FINAM order endpoints.

## Implemented scope

`broker-finam` now exposes:

```rust
map_finam_broker_truth_snapshot(
    account: &AccountResponse,
    orders: &AccountOrdersResponse,
    received_ts: DateTime<Utc>,
) -> Result<BrokerTruthSnapshot, FinamMapperError>
```

The M4-2b-1 mapper covers:

- account positions -> `BrokerPositionSnapshot`;
- account orders -> `BrokerOrderSnapshot`;
- account cash/equity/portfolio margin fields -> `BrokerCashSnapshot`;
- `remaining_quantity` -> quantity-aware order truth;
- target-instrument summary via `BrokerTruthSnapshot::summarize_for_instrument`.

Trades and instrument specs remain explicit follow-up work.

## Accepted invariants

- target flat ignores zero-quantity FINAM position rows;
- other-instrument non-zero position remains account context, not target
  non-flat truth;
- target terminal order is not target active lifecycle truth;
- other-symbol active order is visible as account-wide safety guard;
- active order truth is status + target instrument + remaining quantity;
- unknown order statuses remain separate blocking truth in broker-core.

## Not implemented in M4-2b-1

- ALOR fixture converter into `BrokerTruthSnapshot`;
- FINAM trades -> `BrokerTradeSnapshot` in the aggregate mapper;
- FINAM asset/asset params/schedule -> `BrokerInstrumentSpec`;
- readiness binding to canonical snapshot freshness;
- M4/M5 preflight replacement of local counters with canonical summary;
- runtime-live or command-consumer-to-real-FINAM.

## Trading boundary

Until the full M4-2b review accepts the canonical mapper and parity tests:

- no new live position tests;
- no runtime-live attachment;
- no command-consumer-to-real-FINAM;
- no stop/SLTP/bracket/multi-leg expansion.
