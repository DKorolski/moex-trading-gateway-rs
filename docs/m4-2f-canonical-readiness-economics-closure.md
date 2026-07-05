# M4-2f canonical readiness / economics closure

Status: no-live canonical package hardening.

M4-2f follows M4-2e and keeps the same trading boundary: no new live orders,
no new live position tests, no runtime-live attachment, and no
command-consumer-to-real-FINAM attachment.

The purpose of this stage is to remove the last obvious local-counter gap
before the next live expansion decision. Future preflight must consume a
canonical package built from typed read-only broker artifacts:

```rust
FinamCanonicalReadinessPackage {
    broker_truth: BrokerTruthSnapshot,
    broker_readiness: BrokerReadinessSnapshot,
    margin_sufficiency: BrokerOrderMarginSufficiency,
    live_entry_decision: BrokerLiveEntryDecision,
    stop_order_policy: BrokerStopOrderReadiness,
    no_live_authorization: true,
}
```

The package builder does not call the broker. It accepts already captured
read-only account/orders/trades/quote/instrument/schedule artifacts and maps
them into canonical broker-neutral truth. A fresh real read-only package is
still required as an operator evidence artifact before any later live test.

## Trading boundary

M4-2f must not:

- send live orders;
- open or close a position;
- enable runtime live;
- connect command-consumer to real FINAM;
- enable Stop/SLTP/bracket/replace/multi-leg;
- authorize further live position tests.

Live expansion remains blocked after M4-2f.

## Canonical package contract

The FINAM mapper now exposes:

- `FinamCanonicalReadinessPackage`;
- `FinamCanonicalReadinessPackageInput`;
- `build_finam_canonical_readiness_package(...)`.

The builder constructs:

- `BrokerTruthSnapshot` from account/orders/trades/instrument specs;
- `BrokerReadinessSnapshot` from the same read-only artifact family;
- instrument-scoped margin sufficiency from canonical truth;
- `BrokerLiveEntryDecision` from canonical readiness/config/capabilities/scope;
- stop-order policy copied from canonical readiness.

This is the intended source for the next preflight layer. Local counters may
remain in diagnostics, but they must not be used as trading truth.

## Economics / margin policy

M4-2f implements a narrow initial-margin policy for futures micro preflight.

`BrokerInstrumentSpec` now carries:

- `long_initial_margin`;
- `short_initial_margin`.

`BrokerTruthSnapshot::required_margin_for_order(...)` derives required margin
from:

- target instrument identity;
- order side;
- quantity;
- reference price sanity guardrail;
- instrument initial-margin fields.

For the current M4 futures micro preflight, `reference_price` is validated as
positive but is not used as the formula multiplier. The derived amount is:

```text
required_margin = broker_provided_initial_margin_per_contract * qty
```

`BrokerTruthSnapshot::margin_sufficiency_for_instrument_order(...)` returns a
structured `BrokerOrderMarginSufficiency`:

- `Sufficient { required_margin }`;
- `Insufficient { required_margin }`;
- `MissingCashSnapshot`;
- `MissingFreeCash`;
- `MissingInstrumentSpec`;
- `MissingInitialMargin`;
- `InvalidQuantity`;
- `InvalidReferencePrice`.

Important policy: missing margin is a blocker. It is not converted into
`Sufficient`.

This is not full economics parity yet. Remaining P0/P1 work:

- commission and fee model;
- variation margin / realized futures PnL semantics;
- exchange-specific intraday margin changes;
- portfolio-level stress and concentration risk;
- broker report reconciliation beyond read-only snapshot shape.

## Stop-order readiness policy

Stop/SLTP/bracket remains blocked.

FINAM readiness keeps:

```rust
BrokerStopOrderReadiness::UnsupportedBlocked
```

This means M4-2f does not use a plain-micro waiver to bypass stop-order
readiness. The blocker is intentionally visible in `BrokerLiveEntryDecision`.

## Acceptance

M4-2f is ready for review when:

- `BrokerTruthSnapshot` can derive side/qty/instrument margin from
  `BrokerInstrumentSpec`;
- missing margin is an explicit blocker;
- FINAM canonical readiness package returns both `BrokerTruthSnapshot` and
  `BrokerReadinessSnapshot`;
- package live decision remains blocked while stop orders are unsupported;
- future preflight source is documented as canonical package only;
- no local counters are introduced as source of truth;
- no broker/live calls are performed by the evidence package;
- runtime-live and command-consumer-to-real-FINAM remain disabled.

## Status after M4-2f

Closed:

- canonical package boundary for FINAM read-only artifacts;
- instrument-derived initial margin guardrail;
- missing margin blocker;
- stop-order unsupported-blocked policy retained;
- no-live trading boundary retained.

Still blocked:

- further live position tests;
- continuous runtime live;
- command-consumer-to-real-FINAM;
- Stop/SLTP/bracket/replace/multi-leg;
- full economics parity.
