# M4-2g-a orphan-order truth derivation

Status: no-live canonical truth hardening.

M4-2g-a closes the remaining review blocker after M4-2g: orphan-order truth must
be derived from `BrokerTruthSnapshot`, not manually injected into
`BrokerTruthInstrumentSummary`.

## Trading boundary

M4-2g-a must not:

- call FINAM;
- send live orders;
- open or close a position;
- enable runtime live;
- connect command-consumer to real FINAM;
- enable Stop/SLTP/bracket/replace/multi-leg;
- authorize live-position tests.

Live expansion remains blocked after M4-2g-a.

## Canonical orphan-order semantics

An order is considered orphaned when canonical snapshot relationships cannot
prove it belongs to the current account/instrument/order lifecycle truth.

Derived reasons:

```rust
BrokerOrderOrphanReason::{
    AccountMismatch,
    MissingCorrelationId,
    UnknownInstrumentIdentity,
    FilledQuantityWithoutMatchingTrade,
    MatchingTradeAccountMismatch,
    MatchingTradeInstrumentMismatch,
    MatchingTradeSideMismatch,
    MatchingTradeQuantityLessThanFilledQuantity,
}
```

### Account binding

`BrokerOrderSnapshot.account_id` must match `BrokerTruthSnapshot.account_id`.

Other-account orders in an account-scoped truth snapshot are treated as
orphan/safety blockers. This is intentionally conservative: a broker should not
return another account's order in an account snapshot, and if it does, the
preflight must not continue silently.

### Correlation identity

An order must have at least one correlation id:

- `broker_order_id`; or
- `client_order_id`.

If both are missing, the order is orphaned via `MissingCorrelationId`.

### Instrument identity

An order must have a venue-level instrument identity. If
`InstrumentId.venue_symbol` is missing, the order is orphaned via
`UnknownInstrumentIdentity`.

If the snapshot has an instrument registry/spec list, the order instrument must
match at least one `BrokerInstrumentSpec`. If it does not, it is also orphaned
via `UnknownInstrumentIdentity`.

When no instrument specs are present, venue-symbol identity is accepted as the
best available read-only identity, while other readiness gates still decide
whether the snapshot is usable for live preflight.

### Order/trade relationship

If an order has `filled_qty > 0`, it must have a matching trade by
`broker_order_id` or `client_order_id`.

The matching trade set must prove:

- same account;
- same instrument;
- same side;
- consistent trade quantity covering the order filled quantity.

Otherwise the order is orphaned through the relevant trade mismatch reason.

## Summary derivation

`BrokerTruthSnapshot::summarize_for_instrument(...)` now derives:

```rust
account_orphan_orders_count = self.account_orphan_order_count()
```

from:

```rust
BrokerTruthSnapshot::account_orphan_orders()
BrokerTruthSnapshot::orphan_reasons_for_order(...)
```

No caller should manually set `account_orphan_orders_count` as the source of
truth.

## Preflight impact

`BrokerCanonicalPreflightDecision` already blocks on:

```rust
BrokerCanonicalPreflightBlock::AccountOrphanOrdersPresent
```

After M4-2g-a, that block can be reached from a real derived
`BrokerTruthSnapshot` summary. This closes the prior gap where tests could set
the field manually but snapshot derivation always emitted `0`.

## Acceptance

M4-2g-a is ready for review when:

- `BrokerOrderOrphanReason` exists and is exported;
- orphan orders are derived from account binding, correlation id, instrument
  identity, and order/trade relationships;
- `account_orphan_orders_count` is derived in
  `BrokerTruthSnapshot::summarize_for_instrument(...)`;
- derived orphan count blocks `BrokerCanonicalPreflightDecision.allowed`;
- FINAM M4-2f/M4-2g package regressions remain green;
- no broker/API calls are performed;
- runtime-live and command-consumer-to-real-FINAM remain disabled;
- non-versioned M4-2g evidence is rebound to the current commit.

## Status after M4-2g-a

Closed:

- orphan-order truth derivation;
- account/trade/instrument/correlation orphan semantics;
- canonical preflight orphan blocker from derived summary.

Still blocked:

- actual live-position tests;
- continuous runtime live;
- command-consumer-to-real-FINAM;
- Stop/SLTP/bracket/replace/multi-leg;
- full fee / variation-margin economics parity.
