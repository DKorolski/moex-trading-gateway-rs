# Stage 2B-10 — paper/mock compatibility test pack

Status: accepted.

Date: 2026-07-09.

## What changed

Stage 2B-10 adds a combined paper/mock compatibility package in
`broker-core::paper_mock_compat`.

The package proves that the Stage 2B broker-neutral contracts work together:

- old ALOR-style numeric order ids import as decimal-string `BrokerOrderId`;
- broker-native string order ids remain exact;
- old state -> v2 state -> restored state preserves pending/deferred/manual
  fields;
- exact `StrategyRequestId` ACK clears pending;
- mismatched `StrategyRequestId` ACK does not clear pending;
- `Error`, `Duplicate`, and ambiguous `Expired` ACKs do not clear pending;
- `Expired` ACK clears only with explicit local no-send proof;
- `BrokerOrderId(String)` passes through order, trade, cancel, replace, cache,
  ledger, and hybrid-owned-id paths;
- account-wide observed orders/trades remain non-strategy-owned unless
  explicitly attributed;
- deterministic request ids remain stable after account/instrument migration;
- paper riskgate/oracle seed fields are preserved in the ALOR-compatible hybrid
  projection.

The exported report type is:

```rust
Stage2bPaperMockCompatibilityReport
```

It is intentionally local/paper evidence only and does not enable any live
transport.

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

- `stage_2b10_combined_paper_mock_compatibility_pack_preserves_contracts`;
- `stage_2b10_expired_ack_with_no_send_proof_is_the_only_expired_clear_path`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
