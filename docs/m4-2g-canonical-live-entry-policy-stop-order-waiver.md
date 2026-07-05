# M4-2g canonical live-entry policy / stop-order waiver design

Status: no-live policy closure.

M4-2g decides the stop-order question for the next tiny plain market/limit
micro path without expanding the live boundary. The policy answer is:

```text
StopOrderNotRequiredForPlainMicro waiver may exist,
but only as a narrow, explicit, operator-approved preflight waiver.
```

This is not Stop/SLTP/bracket support. It is not continuous runtime-live. It is
not command-consumer-to-real-FINAM.

## Trading boundary

M4-2g must not:

- call FINAM;
- send live orders;
- open or close a position;
- enable runtime live;
- connect command-consumer to real FINAM;
- enable Stop/SLTP/bracket/replace/multi-leg;
- authorize full M4 live expansion.

New live-position tests remain blocked until a later operator-approved run
package.

## Waiver source

New source:

```rust
BrokerStopOrderWaiverSource::StopOrderNotRequiredForPlainMicro
```

New policy:

```rust
BrokerPlainMicroStopOrderWaiverPolicy
```

New decision:

```rust
BrokerStopOrderWaiverDecision {
    source,
    applied,
    rejections,
}
```

The decision is embedded in:

```rust
BrokerCanonicalPreflightDecision.stop_order_waiver_decision
```

and therefore in:

```rust
FinamCanonicalReadinessPackage.canonical_preflight_decision
```

## Scope limits

The waiver can apply only when all conditions are true:

- policy is enabled;
- explicit operator approval is present;
- `qty <= 1`;
- account is in the allowed account list;
- symbol is in the allowed symbol list;
- order type is plain `market` or `limit`;
- order type is in the policy allowed-order-type list;
- runtime-live is disabled;
- command-consumer-to-real-FINAM is disabled;
- Stop/SLTP/bracket/replace/multi-leg features are disabled;
- readiness contains exactly the stop-order unsupported blocker that can be
  waived:

```rust
BrokerLiveEntryBlock::StopOrderUnsupportedBlocked
```

The waiver suppresses only that one readiness blocker. It does not suppress:

- stale/missing stop-order readiness;
- market closed;
- stale account/positions/orders/trades/quotes/spec/schedule;
- unknown orders;
- missing cash/margin;
- margin insufficiency;
- target non-flat;
- active/unknown/orphan order safety blockers;
- any runtime lifecycle blocker.

## Rejection model

Out-of-scope waiver attempts are explicit:

```rust
BrokerStopOrderWaiverRejection::{
    PolicyDisabled,
    StopOrderUnsupportedBlockAbsent,
    OperatorApprovalMissing,
    InvalidQuantity,
    QuantityExceedsMax,
    AccountNotAllowed,
    SymbolNotAllowed,
    OrderTypeNotPlainMarketLimit,
    RuntimeLiveEnabled,
    CommandConsumerToRealFinamEnabled,
    StopSltpBracketReplaceMultiLegEnabled,
}
```

If the stop-order unsupported blocker is present and a requested waiver is
rejected, the combined preflight decision includes:

```rust
BrokerCanonicalPreflightBlock::StopOrderWaiverRejected
```

and remains blocked.

## FINAM package behavior

`FinamCanonicalReadinessPackageInput` accepts:

```rust
stop_order_waiver_policy: Option<&BrokerPlainMicroStopOrderWaiverPolicy>
```

`None` keeps the M4-2f-a behavior: stop-order unsupported remains blocking.

`Some(policy)` evaluates the narrow plain-micro waiver. In the strict positive
case, package-level canonical preflight may become:

```text
canonical_preflight_decision.allowed = true
```

but the package still carries:

```text
no_live_authorization = true
```

That is intentional for M4-2g: this stage closes policy shape only and performs
no broker calls.

## Acceptance

M4-2g is ready for review when:

- waiver source/decision/policy are represented in broker-core;
- waiver source is embedded in `BrokerCanonicalPreflightDecision`;
- strict plain micro waiver can suppress only `StopOrderUnsupportedBlocked`;
- rejected waiver keeps combined decision blocked;
- stale/missing stop-order readiness cannot be waived;
- FINAM package carries the waiver decision in canonical preflight;
- no-live boundary remains explicit in evidence;
- non-versioned evidence artifacts are refreshed to the current commit.

## Status after M4-2g

Closed:

- stop-order waiver design for plain market/limit micro;
- scope-limited operator-approved waiver contract;
- evidence shape showing waiver source in canonical preflight.

Still blocked:

- actual live-position tests;
- continuous runtime live;
- command-consumer-to-real-FINAM;
- Stop/SLTP/bracket/replace/multi-leg;
- full fee / variation-margin economics parity.
