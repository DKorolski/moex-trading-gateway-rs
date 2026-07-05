# M4-2i account-trades windowed orphan-resolution package

Status: GET-only evidence hardening. No live order expansion is authorized.

M4-2h proved that the canonical package can be built from real FINAM read-only artifacts, but it also surfaced the next blocker: filled terminal orders were visible in `account_orders`, while `account_trades` failed without an explicit window. M4-2i makes that blocker observable instead of weakening orphan-order safety.

## Goal

Build the same `canonical_readiness_package_typed` record with an explicit account history window:

- `account_trades` uses `trades_window_start_ts` / `trades_window_end_ts`;
- `account_transactions` uses the same window as diagnostic support;
- `BrokerTruthSnapshot` remains the source of truth;
- filled orders without matching trades remain orphan orders;
- canonical preflight stays blocked if trades are unavailable or insufficient.

## Required summary fields

The typed read-only canonical record must include:

- `canonical_preflight_blocks`;
- `orphan_order_reasons_by_kind`;
- `filled_orders_count`;
- `trades_window_start_ts`;
- `trades_window_end_ts`;
- `trades_window_explicit`;
- `trades_probe_ok`;
- `transactions_probe_ok`;
- `transactions_count`;
- `account_orders_history_window`;
- `readonly_broker_calls_performed`;
- `order_post_delete_calls_performed = false`;
- `live_order_calls_performed = false`.

## Safety interpretation

If `trades_probe_ok = true` and account trades explain filled orders, then `account_orphan_orders_count` may become zero.

If `trades_probe_ok = false`, or if trades are empty while filled orders exist, then `account_orphan_orders_count` remains positive and `canonical_preflight_allowed` must remain false.

This is the intended conservative behavior. M4-2i does not add a waiver for old terminal filled orders. Any future horizon/age waiver must be a separate reviewed policy.

## Trading boundary

M4-2i does not authorize:

- real POST `/orders`;
- real DELETE `/orders/{id}`;
- command-consumer-to-real-FINAM;
- continuous runtime-live;
- new live position tests;
- Stop/SLTP/bracket/replace/multi-leg.

The only broker-side action expected for M4-2i evidence is redacted FINAM read-only GET probing through existing typed read-only CLI paths.

## Evidence

The evidence script is:

```text
scripts/m4_2i_account_trades_windowed_orphan_resolution_evidence.py
```

It validates source markers, targeted tests, forbidden-surface scanners, and an optional real read-only report. When the report is required, it must contain a windowed `canonical_readiness_package_typed` record and must prove `order_endpoints_used = false`.

Live expansion remains blocked after M4-2i.
