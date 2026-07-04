# M4-1a tiny position lifecycle preflight / no-send runbook

Date: 2026-07-04

## Goal

M4-1a defines the no-send runbook for a future tiny position lifecycle test. It
does not send broker orders and does not open a position.

The future live test, if separately approved, will validate:

```text
entry qty=1 -> broker position snapshot -> exit qty=1 -> final flat reconciliation
```

## Proposed first position-test scope

- Account: one approved FINAM account only.
- Symbol: `IMOEXF@RTSX`.
- Quantity: `1`.
- Entry type: marketable limit preferred for first test.
- Exit type: marketable limit preferred for first test.
- Maximum position lifetime: 30-120 seconds, exact value to be approved before live.
- No strategy runtime.
- No command-consumer-to-real-FINAM.
- No Stop/SLTP/bracket/replace/multi-leg.
- No RI/RTS expansion.

Market orders remain a later explicit test. M4-1a prefers marketable limits
because they keep a visible price bound while still testing position lifecycle.

## Required explicit operator approval text

Before any future M4-1 live position test, the operator must approve text with
all fields filled:

```text
Разрешаю M4-1 tiny position lifecycle actual live test:
symbol = IMOEXF@RTSX
qty = 1
entry_side = buy|sell
entry_type = marketable_limit
entry_limit_price = <price>
exit_type = marketable_limit
exit_limit_price_policy = <policy or exact price>
max_position_lifetime_sec = <30-120>
max_orders_total = 2
no stop/SLTP/bracket/replace/multi-leg
no strategy runtime
no command-consumer-to-real-FINAM
abort if broker truth stale/unclear
post-run final flat reconciliation required
```

## Preflight gates

All gates must pass immediately before any future live entry:

- full trade token loaded and bound to the approved account;
- symbol exactly equals `IMOEXF@RTSX`;
- active/unknown/orphan orders = `0`;
- positions = `0`;
- fresh quote available;
- entry limit price passes price guard;
- max qty = `1`;
- max orders total = `2` (`entry + exit`);
- kill switch armed;
- raw response capture enabled local-only;
- command-consumer-to-real-FINAM disabled;
- continuous runtime-live disabled;
- Stop/SLTP/bracket/replace/multi-leg blocked.

## During-position evidence

While the future test is in position, capture redacted evidence:

- accepted entry order response;
- broker order/trade snapshot;
- broker position snapshot showing qty `1` or `-1`;
- timestamp of position observation;
- quote/reference price at observation;
- no active unexpected orders.

## Exit gates

Exit must be attempted when any of these is true:

- position observed and minimum evidence captured;
- max position lifetime reached;
- operator manually requests exit;
- broker truth becomes stale/unclear;
- unexpected active order appears;
- unexpected fill/partial state appears.

Exit evidence:

- accepted exit order response;
- trades snapshot;
- final broker-truth refresh;
- active/unknown/orphan orders = `0`;
- positions = `0`.

## Abort / manual intervention rules

Abort before entry if:

- active/unknown/orphan orders are not zero;
- existing position is not zero;
- quote is stale;
- price guard fails;
- token/account binding fails;
- any configured feature boundary is unexpectedly enabled.

After entry, if broker truth is ambiguous:

- do not retry blindly;
- reconcile broker truth first;
- if position exists, prioritize controlled flattening;
- preserve raw local-only broker responses and redacted evidence.

## Economics / reports required

M4-1 actual test should produce inputs for M4-2:

- orders report;
- trades report;
- positions report;
- portfolio/cash snapshot;
- commission/fee fields if exposed;
- EOD summary and operator signoff.

## Non-goals

M4-1a does not:

- authorize live entry;
- authorize market order entry;
- enable strategy runtime live;
- enable command-consumer-to-real-FINAM;
- enable Stop/SLTP/bracket/replace/multi-leg;
- enable portfolio-level strategy execution.

## Next stage after review

If M4-1a is accepted, the next possible stage is:

`M4-1b tiny position lifecycle no-send preflight evidence`

It should run real read-only checks only and prove that the account, symbol,
quote, orders, positions and risk gates are ready for a future explicitly
approved actual position test.
