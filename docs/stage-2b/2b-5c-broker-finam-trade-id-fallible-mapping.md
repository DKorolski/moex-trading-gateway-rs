# Stage 2B-5c — broker-finam trade_id fallible mapping

Status: implementation patch ready for review.

Date: 2026-07-08.

## What changed

Stage 2B-5c closes the production mapper follow-up for the accepted
`BrokerTradeId` invariant.

Changed in `broker-finam/src/mapper.rs`:

- broker-provided `trade.trade_id` no longer uses `BrokerTradeId::new(...)`;
- FINAM account trade mapping now uses
  `BrokerTradeId::from_broker_native_exact(...)`;
- invalid/empty broker trade ids map to controlled `FinamMapperError` via the
  existing redacted `UnsupportedBrokerValue` path.

Policy:

- `BrokerTradeId::new(...)` is acceptable for trusted/test-created non-empty ids;
- broker DTO / external broker input must use fallible import.

## What did not change

- No TradeLedger implementation changed.
- No `HybridIntradayRuntime` behavior changed.
- No BO/MR strategy decision logic changed.
- No command builders changed.
- No real FINAM command consumer was connected.
- No real FINAM `POST`/`DELETE` path was enabled.
- No runtime-live or FINAM `LiveReady` was enabled.
- No Stop/SLTP/bracket/replace/multi-leg live behavior was enabled.
- No RI/RTS or USDRUBF migration was started.
- No `i64` surrogate adapter was introduced.

## Tests added

- `account_trade_empty_trade_id_returns_controlled_mapper_error_not_panic`;
- `account_trade_valid_native_trade_id_preserves_exact_string`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
