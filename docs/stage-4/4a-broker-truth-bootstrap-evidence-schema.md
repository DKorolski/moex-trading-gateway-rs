# Stage 4A — broker-truth bootstrap evidence schema

Status: Stage 4A / Stage 4A-1 accepted as planning/evidence-schema foundation.
Stage 4B existing type inventory is documented separately.

Date: 2026-07-09.

## Purpose

This document defines the redacted evidence shape for broker-truth bootstrap into
runtime lifecycle.

Stage 4A evidence proves only classification and bootstrap readiness decisions.
It does not attach runtime-live and does not authorize real order execution.

Stage 4B inventory/evidence-alignment decisions are documented in
[`4b-existing-broker-truth-type-inventory.md`](4b-existing-broker-truth-type-inventory.md).

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
  "bootstrap_lifecycle": {},
  "dirty_start": {},
  "adoption": {},
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
- `BootstrapBlocked`;
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

`BootstrapBlocked` is used when broker truth is present/fresh enough to make a
decision, but target-scoped blockers prevent runtime readiness.

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
      "target_runtime_owned_order_count": 0,
      "target_adopted_order_count": 0,
      "target_observed_unowned_order_count": 0,
      "unknown_target_order_count": 0,
      "orphan_target_order_count": 0
    },
    "trades": {
      "target_recent_trade_count": 0,
      "target_strategy_attributed_trade_count": 0,
      "target_observed_unattributed_trade_count": 0,
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

Order ownership classes:

- `RuntimeOwned`;
- `AdoptedFromBootstrap`;
- `ObservedAccountWide`;
- `UnknownOrOrphan`.

Trade correlation classes:

- `StrategyAttributed`;
- `ObservedUnattributed`;
- `UnknownOrOrphan`.

A broker order/trade row alone does not prove strategy ownership.

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

## Bootstrap lifecycle

```json
{
  "bootstrap_lifecycle": {
    "load_broker_truth_snapshot_done": true,
    "load_runtime_state_after_broker_truth": true,
    "notify_bootstrap_snapshot_after_broker_truth": true,
    "notify_runtime_state_restored_after_bootstrap_snapshot": true,
    "warmup_history_after_state_restore": true,
    "recover_pending_streams_after_warmup": true,
    "live_orders_allowed_during_bootstrap": false,
    "live_orders_allowed_during_warmup": false,
    "first_runtime_intent_before_broker_truth_count": 0
  }
}
```

Lifecycle rules:

- runtime state is not trusted before broker truth;
- warmup cannot happen before broker truth and state restore;
- pending stream recovery happens after warmup;
- live orders remain disabled during bootstrap/warmup;
- any runtime intent before broker truth blocks bootstrap readiness.

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

## Adoption evidence

```json
{
  "adoption": {
    "position_adoption_attempted": false,
    "position_adoption_allowed": false,
    "position_adoption_applied": false,
    "order_adoption_attempted": false,
    "order_adoption_allowed": false,
    "order_adoption_applied": false,
    "adopted_target_position_qty": "0",
    "adopted_target_order_count": 0,
    "manual_intervention_reason": null
  }
}
```

Adoption may be applied only when attempted, allowed, and backed by explicit
redacted evidence. Target non-flat cannot silently become flat. Target active
orders cannot silently disappear.

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
    "max_age_seconds": 30,
    "positions_age_seconds": 12,
    "orders_age_seconds": 9,
    "trades_age_seconds": 15,
    "schedule_age_seconds": 20,
    "positions_freshness": "Fresh",
    "orders_freshness": "Fresh",
    "trades_freshness": "Fresh",
    "schedule_freshness": "Fresh",
    "stale_section_count": 0
  }
}
```

Allowed freshness values:

- `Fresh`;
- `Stale`;
- `Unknown`;
- `Unavailable`.

`Fresh` is valid only when age is less than or equal to `max_age_seconds`.
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
- `ManualInterventionRequired`;
- `BootstrapLifecycleOrderInvalid`;
- `FirstRuntimeIntentBeforeBrokerTruth`;
- `AdoptionEvidenceMissing`;
- `OwnershipCorrelationUnknown`.

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

## Stage 4A / 4A-1 acceptance

Stage 4A / 4A-1 acceptance requires:

- redacted broker-truth snapshot shape;
- runtime bootstrap snapshot shape;
- target-vs-account-wide distinction;
- zero-quantity row policy;
- active target order policy;
- unknown/orphan order/trade blockers;
- freshness policy;
- per-section freshness ages;
- dirty-start/adoption policy;
- bootstrap lifecycle order policy;
- runtime-owned/adopted/observed/orphan order/trade classification;
- explicit safety boundary;
- no runtime-live;
- no real FINAM command consumer;
- no real orders.
