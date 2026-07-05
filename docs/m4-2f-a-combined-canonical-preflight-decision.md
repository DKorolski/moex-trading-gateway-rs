# M4-2f-a combined canonical preflight decision

Status: no-live safety patch after M4-2f review.

M4-2f-a closes the main conditional-go finding from the M4-2f review:
`live_entry_decision` and `margin_sufficiency` must not remain two separate
signals that a future caller can accidentally interpret independently.

The final preflight result is now a single canonical decision:

```rust
BrokerCanonicalPreflightDecision {
    readiness_decision: BrokerLiveEntryDecision,
    margin_sufficiency: BrokerOrderMarginSufficiency,
    truth_summary: BrokerTruthInstrumentSummary,
    allowed: bool,
    blocks: Vec<BrokerCanonicalPreflightBlock>,
}
```

`FinamCanonicalReadinessPackage` now carries this field as
`canonical_preflight_decision`.

## Trading boundary

M4-2f-a must not:

- call FINAM;
- send live orders;
- open or close a position;
- enable runtime live;
- connect command-consumer to real FINAM;
- enable Stop/SLTP/bracket/replace/multi-leg;
- authorize further live position tests.

Live expansion remains blocked after M4-2f-a.

## Combined decision invariant

The final preflight decision is allowed only when all canonical layers are clean:

```text
final_preflight_allowed =
    readiness_decision.allowed
    AND margin_sufficiency == Sufficient
    AND stop-order policy has no readiness blocker
    AND target instrument is flat
    AND target instrument has no active/unknown orders
    AND account-wide active/unknown/orphan order safety is clean
```

Account-wide position row count remains diagnostic. Target flatness is based on
target instrument non-zero quantity.

## Block mapping

Readiness blockers are preserved as:

```rust
BrokerCanonicalPreflightBlock::Readiness(BrokerLiveEntryBlock)
```

Margin/economics blockers are explicit:

- `MarginInsufficient`;
- `MissingCashSnapshot`;
- `MissingFreeCash`;
- `MissingInstrumentSpec`;
- `MissingInitialMargin`;
- `InvalidQuantity`;
- `InvalidReferencePrice`.

Truth/safety blockers are explicit:

- `TargetPositionNotFlat`;
- `TargetActiveOrdersPresent`;
- `TargetUnknownOrdersPresent`;
- `AccountActiveOrdersPresent`;
- `AccountUnknownOrdersPresent`;
- `AccountOrphanOrdersPresent`.

This means a future caller cannot use `BrokerLiveEntryDecision.allowed` alone as
the trading decision. The package-level decision is
`canonical_preflight_decision.allowed`.

## Reference price semantics

For the current M4 futures micro preflight, `reference_price is a sanity guardrail input`.
The required-margin amount is derived from broker-provided initial margin per
contract:

```text
required_margin = initial_margin_per_contract * qty
```

where `initial_margin_per_contract` is selected by side:

- buy -> `BrokerInstrumentSpec.long_initial_margin`;
- sell -> `BrokerInstrumentSpec.short_initial_margin`.

Full fee/PnL/variation-margin economics parity is still open and remains a
separate M4 economics task.

## Acceptance

M4-2f-a is ready for review when:

- `BrokerCanonicalPreflightDecision` exists and is exported;
- FINAM canonical readiness package includes `canonical_preflight_decision`;
- all margin failure variants block the combined decision;
- `Insufficient` margin blocks the combined decision;
- missing cash/free cash blocks the combined decision;
- invalid qty/reference price blocks the combined decision;
- stop-order unsupported remains a readiness blocker inside the combined
  decision;
- target flat and account/order safety blockers are represented;
- M4-1c report path still uses canonical `BrokerTruthSnapshot`;
- no broker/live calls are performed by evidence.

## Status after M4-2f-a

Closed:

- M4-2f conditional-go gap where readiness and margin were separate signals;
- explicit final package-level preflight decision;
- margin blocker mapping;
- target/account order safety blocker mapping.

Still blocked:

- live position tests;
- continuous runtime live;
- command-consumer-to-real-FINAM;
- Stop/SLTP/bracket/replace/multi-leg;
- full economics parity.
