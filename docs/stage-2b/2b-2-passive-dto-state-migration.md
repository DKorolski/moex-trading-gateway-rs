# Stage 2B-2 — passive DTO/state migration

Status: accepted.

Date: 2026-07-07.

## What changed

Stage 2B-2 adds passive broker-neutral runtime DTO/state migration contracts
without changing strategy behavior:

- reusable legacy broker-order-id serde helpers:
  - old ALOR numeric `order_id` imports as decimal-string `BrokerOrderId`;
  - FINAM/broker-native string ids are preserved exactly;
  - empty broker-native string ids are rejected;
  - zero/negative/null rejection applies to the legacy numeric ALOR import path;
  - native broker string ids such as `"0"` or `"-1"` are preserved exactly unless
    a later policy validator explicitly rejects numeric-looking strings;
- `RuntimeOrderEvent` with `BrokerOrderId`;
- `RuntimeTradeEvent` with `BrokerTradeId` + `BrokerOrderId`;
- `RuntimeBootstrapSnapshotDto` with string-key working-order maps;
- `RuntimeStateSnapshot` with v2 schema/encoding markers and old-state import;
- `RuntimeCommandAckDto` with legacy `broker_order_id` /
  `broker_order_id_str` compatibility and conflict rejection;
- exports from `broker-core` for later runtime-source migration patches.

## What did not change

- No `HybridIntradayRuntime` behavior changed.
- No BO/MR strategy decision logic changed.
- No runtime state was deployed or read from live systems.
- No command builder behavior changed.
- No trade ledger implementation changed.
- No real FINAM command consumer was connected.
- No real FINAM `POST`/`DELETE` path was enabled.
- No runtime-live or FINAM `LiveReady` was enabled.
- No Stop/SLTP/bracket/replace/multi-leg live behavior was enabled.
- No RI/RTS or USDRUBF migration was started.
- No `i64` surrogate adapter was introduced.

## Tests added

- `old_alor_numeric_order_id_deserializes_as_broker_order_id`;
- `finam_string_broker_order_id_serializes_deserializes_exact`;
- `empty_broker_order_id_rejected_at_runtime_state_boundary`;
- `old_state_snapshot_reads_numeric_ids_and_can_write_v2_markers`;
- `runtime_command_ack_imports_legacy_ids_without_replacing_request_id`;
- `runtime_command_ack_rejects_conflicting_primary_and_legacy_string_ids`;
- `client_order_id_does_not_replace_strategy_request_id`;
- legacy id serde helper tests for scalar/option/vector imports.

Required broker id fields reject null and empty values. Zero/negative rejection
is scoped to legacy numeric ALOR imports. Optional `broker_order_id` fields may
be absent or null only where the ACK lifecycle allows `broker_order_id=None`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
