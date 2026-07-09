# Stage 4A — broker-truth bootstrap evidence schema

Status: implemented for review.

Date: 2026-07-09.

## Purpose

This document defines the redacted evidence shape for broker-truth bootstrap into
runtime lifecycle.

Stage 4A evidence proves only classification and bootstrap readiness decisions.
It does not attach runtime-live and does not authorize real order execution.

## Top-level shape

```json
{
  "schema_version": 1,
  "stage": "Stage4BrokerTruthBootstrap",
  "substage": "Stage4A",
  "generated_at": "2026-07-09T00:00:00Z",
  "source_commit": "short-or-full-sha",
  "source_archive_name": "moex-trading-project-<sha>.zip",
  "source_archive_sha256": "sha256",
  "raw_payload_exported": false,
  "scope": {},
  "broker_truth_snapshot": {},
  "runtime_bootstrap_snapshot": {},
  "dirty_start": {},
  "freshness": {},
  "readiness": {},
  "safety_boundary": {},
  "status": "EvidenceIncomplete"
}
```

All broker/account/runtime identifiers must be redacted, synthetic, or
fingerprinted. Raw broker responses and Redis payloads must not be exported.

## Status enum

Allowed statuses:

- `BootstrapReady`;
- `ManualInterventionRequired`;
- `BrokerTruthIncomplete`;
- `BrokerTruthStale`;
- `InstrumentMismatch`;
- `UnknownSchedule`;
- `EvidenceIncomplete`;
- `SafetyBoundaryOpen`.

`BootstrapReady` is allowed only when target broker truth is fresh,
instrument-scoped, free of unknown/orphan target rows, and compatible with the
runtime bootstrap/adoption policy.

## Scope

```json
{
  "scope": {
    "target_instrument": {
      "symbol": "IMOEXF",
      "venue_symbol": "IMOEXF@RTSX",
      "exchange": "MOEX",
      "market": "Futures"
    },
    "account_alias": "ACC_REDACTED",
    "session_date": "YYYY-MM-DD",
    "broker": "FINAM",
    "runtime_kind": "HybridIntradayRuntime",
    "paper_boundary": true
  }
}
```

`account_alias` must not contain a live account id.

## Broker truth snapshot summary

```json
{
  "broker_truth_snapshot": {
    "source": "ReadOnlyBrokerTruth",
    "checked_ts": "2026-07-09T00:00:00Z",
    "account_present": true,
    "cash_present": true,
    "margin_present": false,
    "positions": {
      "target_rows_count": 0,
      "target_non_zero_qty_rows_count": 0,
      "target_zero_qty_rows_count": 0,
      "account_wide_rows_count": 0,
      "non_target_rows_count": 0,
      "target_position_qty": "0",
      "target_avg_price_present": false
    },
    "orders": {
      "target_active_order_count": 0,
      "target_terminal_order_count": 0,
      "account_wide_active_order_count": 0,
      "unknown_target_order_count": 0,
      "orphan_target_order_count": 0
    },
    "trades": {
      "target_recent_trade_count": 0,
      "unknown_target_trade_count": 0,
      "orphan_target_trade_count": 0
    },
    "instrument_identity_match": true,
    "schedule_known": true,
    "session_state": "Open"
  }
}
```

Counts are allowed. Raw rows are not allowed.

## Runtime bootstrap snapshot

```json
{
  "runtime_bootstrap_snapshot": {
    "target_position_state": "Flat",
    "position_source": "BrokerTruth",
    "active_order_state": "NoTargetActiveOrders",
    "order_source": "BrokerTruth",
    "recent_trade_state": "NoUnknownTargetTrades",
    "account_wide_rows_policy": "DiagnosticOnly",
    "zero_qty_position_rows_policy": "DiagnosticOnly",
    "unknown_orphan_policy": "BlockLiveReady",
    "bootstrap_disposition": "CleanBootstrap"
  }
}
```

Allowed `bootstrap_disposition` values:

- `CleanBootstrap`;
- `AdoptTargetPositionExplicitly`;
- `AdoptTargetOrderExplicitly`;
- `ManualInterventionRequired`;
- `EvidenceIncomplete`.

Adoption must be explicit and must never be inferred from account-wide rows.

## Dirty-start evidence

```json
{
  "dirty_start": {
    "target_non_flat": false,
    "target_active_order_exists": false,
    "strategy_supports_position_adoption": false,
    "strategy_supports_order_adoption_or_repair": false,
    "manual_intervention_required": false,
    "adoption_reason": null
  }
}
```

Policy:

- target non-flat + no adoption support => `ManualInterventionRequired`;
- target active order + no adoption/repair support => `ManualInterventionRequired`;
- unknown/orphan target order/trade => block readiness;
- non-target account-wide rows remain diagnostic unless a later account-safety
  policy promotes them to blockers.

## Freshness

```json
{
  "freshness": {
    "broker_truth_checked_ts": "2026-07-09T00:00:00Z",
    "positions_freshness": "Fresh",
    "orders_freshness": "Fresh",
    "trades_freshness": "Fresh",
    "schedule_freshness": "Fresh",
    "max_age_seconds": 30,
    "stale_section_count": 0
  }
}
```

Allowed freshness values:

- `Fresh`;
- `Stale`;
- `Unknown`;
- `Unavailable`.

Unknown or stale target position/order truth blocks `BootstrapReady`.

## Readiness

```json
{
  "readiness": {
    "bootstrap_ready": false,
    "runtime_live_ready_enabled": false,
    "blockers": [
      "BrokerTruthIncomplete"
    ]
  }
}
```

Allowed blockers:

- `BrokerTruthMissing`;
- `TargetPositionFreshnessUnknown`;
- `TargetActiveOrderFreshnessUnknown`;
- `TargetNonFlatCannotAdopt`;
- `TargetActiveOrderCannotAdoptOrRepair`;
- `UnknownTargetOrder`;
- `OrphanTargetOrder`;
- `UnknownTargetTrade`;
- `OrphanTargetTrade`;
- `UnknownSchedule`;
- `InstrumentIdentityMismatch`;
- `BrokerTruthSourceUnavailable`;
- `RawPayloadExportAttempted`;
- `ManualInterventionRequired`.

No blocker may be hidden as diagnostic if it affects the target instrument.

## Safety boundary

```json
{
  "safety_boundary": {
    "runtime_live_enabled": false,
    "real_finam_command_consumer_enabled": false,
    "strategy_driven_real_orders_enabled": false,
    "real_post_delete_enabled": false,
    "stop_sltp_bracket_enabled": false,
    "raw_payload_exported": false
  }
}
```

Any `true` value in the live/order fields makes the evidence invalid for Stage
4A and should produce `SafetyBoundaryOpen`.

## Stage 4A acceptance

Stage 4A acceptance requires:

- redacted broker-truth snapshot shape;
- runtime bootstrap snapshot shape;
- target-vs-account-wide distinction;
- zero-quantity row policy;
- active target order policy;
- unknown/orphan order/trade blockers;
- freshness policy;
- dirty-start/adoption policy;
- explicit safety boundary;
- no runtime-live;
- no real FINAM command consumer;
- no real orders.
