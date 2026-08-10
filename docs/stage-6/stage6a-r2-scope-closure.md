# Stage 6A R2 — durable Place-shape closure

R2 is one direct successor to
`76d49c365f4fc89749e97db16858c5c95bb73bfa` on
`stage6-durable-chain`.

The patch adds one private structural validator to the existing snapshot
intrinsic-validation path:

- Market requires positive quantity and no limit price;
- Limit requires positive quantity and a strictly positive limit price;
- Stop, StopLimit, TakeProfit and TakeProfitLimit fail closed.

This is not order preflight. Quantity/price steps, deviation, min/max size,
notional, instrument allowlist, operator arm and reference-data freshness stay
with their accepted policy authority. No native stop snapshot or FINAM type is
introduced.

Unchanged: durable schema version 1, request/client identity formulas, cancel
correlation, journal record ID, canonical-byte admission, reserved event
rejection and both golden fixtures.

Still closed: Stage 6B+, persistence backend, filesystem, Redis, FINAM,
transport, dispatch, runtime attachment, workers, scheduling and live orders.
