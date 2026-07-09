# Stage 2B-9 — deterministic request-id stability

Status: accepted.

Date: 2026-07-09.

## What changed

Stage 2B-9 adds a broker-neutral deterministic request-id contract in
`broker-core::request_id`.

The contract preserves the legacy ALOR runtime namespace exactly:

```text
strategy_id|portfolio|symbol|action|bar_ts|seq
```

Typed broker-neutral helpers map into that same namespace:

- `AccountId` / `BrokerAccountId` renders as the legacy portfolio/account alias;
- `InstrumentId.symbol` renders as the legacy strategy symbol;
- `InstrumentId.venue_symbol` is not part of the request-id namespace;
- `ClientOrderId` is not part of the request-id namespace;
- `BrokerOrderId` is not part of the request-id namespace.

This keeps pending/ACK lifecycle stable while other Stage 2B identity surfaces
migrate to broker-neutral IDs.

## What did not change

- No `HybridIntradayRuntime` trading behavior changed.
- No BO/MR strategy decision logic changed.
- No command consumer was connected.
- No real FINAM `POST`/`DELETE` path was enabled.
- No runtime-live or FINAM `LiveReady` was enabled.
- No Stop/SLTP/bracket/replace/multi-leg live behavior was enabled.
- No RI/RTS or USDRUBF migration was started.
- No `i64` surrogate adapter was introduced.

## Tests added

- `deterministic_request_id_is_stable_after_account_alias_migration`;
- `legacy_portfolio_string_and_account_id_alias_produce_same_request_id`;
- `legacy_symbol_string_and_instrument_alias_produce_same_request_id`;
- `action_bar_ts_seq_namespace_unchanged`;
- `old_pending_request_id_still_matches_new_ack_path`;
- `client_order_id_does_not_affect_strategy_request_id`;
- `broker_order_id_does_not_affect_strategy_request_id`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
