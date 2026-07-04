# M3j-4 explicit pre-live NO-GO / GO decision package

M3j-4 aggregates M3j-0 through M3j-3 and emits an explicit decision.

The default safe decision is `NO-GO` unless an explicit operator GO is present
and every pre-live condition is satisfied.

Even a `GO-candidate` report does not enable live trading by itself.

M3j-4 does not enable:

- `LiveReady`;
- runtime live attachment;
- external FINAM POST/DELETE;
- command-consumer-to-real-FINAM transport;
- non-loopback order endpoint;
- Stop/SLTP/bracket/replace/multi-leg.

## Required decision inputs

M3j-4 requires:

- M3j-0 closed;
- M3j-1 live gate / operator-risk-controls design accepted;
- M3j-2 fresh read-only evidence accepted and still fresh;
- M3j-3 one-symbol dry shadow session accepted;
- active orders = 0;
- no unknown active orders;
- no orphan active orders;
- flat or explicitly expected position;
- one account / one symbol / one timeframe / one strategy scope;
- one-shot TTL/digest-bound operator arm;
- no auto-rearm after restart;
- persistent kill switch blocking runtime, command consumer, and endpoint paths;
- max quantity and max orders limits;
- max loss / notional placeholder;
- daily/EOD reconciliation plan;
- typed optional failures fixed or explicitly waived with scope;
- closed live/order boundary.

## Current package posture

The source-level tests cover two safe outcomes:

- `NoGo` when all technical pre-live inputs pass but `operator_explicit_go=false`;
- `GoCandidate` when all inputs pass and `operator_explicit_go=true`.

Both outcomes keep:

- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `external_finam_post_delete_allowed = false`;
- `command_consumer_to_real_finam_transport_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`.

This separates the decision package from any later live-micro operator-run
authorization.

## Typed optional failure waiver

The M3j-2 optional typed failures remain visible:

- `account_trades_typed` is covered by runtime `TradesSnapshot` for the first
  micro scope;
- `account_transactions_typed` is deferred to EOD/fees/cash reconciliation
  hardening;
- `bars_typed` is deferred because the first micro path uses live-final events,
  not REST bars backfill.

These waivers are scoped to first live micro only.

## Daily/EOD reconciliation plan

Before any live-micro operator authorization, the operator must have a daily/EOD
reconciliation plan covering:

- orders;
- trades;
- positions;
- cash/fees when account transaction evidence is available;
- manual stop procedure if broker truth becomes stale, unknown, or inconsistent.
