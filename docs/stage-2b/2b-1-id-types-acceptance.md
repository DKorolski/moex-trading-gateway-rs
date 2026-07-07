# Stage 2B-1 — broker-neutral id types acceptance

Status: implementation patch ready for review.

Date: 2026-07-07.

## What changed

Stage 2B-1 adds the runtime-facing broker-order id migration foundation in
`broker-core`:

- explicit Stage 2 runtime-state schema marker:
  `RUNTIME_STATE_SCHEMA_VERSION_V2 = 2`;
- explicit broker-order id encoding marker:
  `BROKER_ORDER_ID_ENCODING = "broker_order_id_string"`;
- explicit legacy ALOR import policy marker:
  `LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT = "decimal_string"`;
- `BrokerOrderIdEncoding::BrokerOrderIdString`;
- `BrokerOrderId::try_from_legacy_alor_numeric(i64)`;
- `BrokerOrderId::from_broker_native_exact(String)`;
- `BrokerOrderIdImportError`.

These items are exported from `broker-core` so runtime-boundary code can use
one broker-neutral contract instead of inventing local conversion rules.

## What did not change

- No strategy trading logic changed.
- No `HybridIntradayRuntime` behavior changed.
- No runtime state migration was applied yet.
- No command consumer was connected to real FINAM.
- No real FINAM `POST`/`DELETE` path was enabled.
- No Stop/SLTP/bracket/replace live behavior was enabled.
- No RI/RTS or USDRUBF migration was started.
- No `i64` surrogate adapter was introduced.

## Tests added

- `legacy_alor_numeric_order_id_imports_as_decimal_string`;
- `legacy_alor_non_positive_order_id_is_rejected`;
- `broker_native_order_id_string_is_preserved_exactly`;
- `empty_broker_native_order_id_is_rejected`;
- `stage2b_runtime_order_id_encoding_markers_are_explicit`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled.

## Evidence collected for handoff

- `bash scripts/forbidden_surface_scan.sh` green;
- `bash scripts/forbidden_surface_negative_harness.sh` green;
- `bash scripts/order_endpoint_scanner_transition_spec.sh` green;
- `python3 -m py_compile scripts/*.py` green;
- `bash scripts/test_m4_3x_evidence_no_redis.sh` green;
- `cargo fmt --all -- --check` green;
- `cargo test --all` green;
- `cargo clippy --workspace --all-targets -- -D warnings` green;
- clean source handoff archive with no `.env`, `.git`, `target`, `tmp`,
  `reports`, logs, or raw broker payloads.
