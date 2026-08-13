# Stable macro-roadmap

Status: accepted.

Reviews may split a macro-stage into sub-stages or patch gates, such as
Stage 2A, Stage 2B, and Stage 2B-N patches, but reviews do not renumber or
replace this macro-roadmap unless an explicit roadmap ADR is accepted.

## Stages

- Stage 0 — Baseline / source import / safety gates.
- Stage 1 — ALOR operational contract extraction.
- Stage 1B — IMOEXF `HybridIntradayRuntime` paper/shadow compatibility freeze.
- Stage 2A — Runtime source migration inventory / plan.
- Stage 2B — Runtime source migration implementation.
- Stage 3 — Market-data parity to strategy input level.
- Stage 4 — Broker-truth bootstrap into runtime.
- Stage 5 — Real strategy semantics attachment.
- Stage 6 — Durable request/client/broker id chain.
- Stage 7 — Runtime command consumer paper/mock.
- Stage 8 — Real FINAM execution under command consumer.
- Stage 9 — Orders/trades/positions reconciliation loop.
- Stage 10 — Runtime-live readiness and observability.
- Stage 11 — Dual-broker shadow parity.
- Stage 12 — First runtime-driven live micro.
- Stage 13 — Stop/SLTP/bracket.

## Current active stage

Stage 7B-c-R1 — direct file-backed restart/crash evidence closure for the
authenticated recovery seal and linear ready-owner lifetime, paper/mock only
(review-closure candidate; independent acceptance pending). Stage 7B-c at
`3d443be72b8a6eb24d1295c800849d11789dba6f` passed architecture review but was
not accepted as closure. Stage 7B-b-R2 is accepted and Stage 7B-b is
closed at `ff3fa2e8908440863b40b838991d4716b33caad4`.
Redis-settlement work remains closed for Stage 7B-d.

Stage 2B is closed as the broker-neutral runtime source migration foundation;
Stages 3, 4 and 5 are accepted/closed. Stage 6 is independently accepted and
closed at `10e357825a701193d964975bb5769bd0745d4986`. Stage 7A is independently
accepted and closed at `2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64`.
Stage 7B-c-R1 composes one file-backed Stage 6 authority, OS locking, recovery
seals and paper restart ownership. Stage 7B-d may later compose the accepted
Redis consumer only after independent Stage 7B-c-R1 acceptance. Stage
8+ remain closed pending separate acceptance.

## Still blocked

- Runtime-live.
- Redis attachment outside the validated Stage 7A/7B paper namespace.
- FINAM runtime POST/DELETE and broker dispatch.
- Strategy-driven real FINAM orders.
- Stop/SLTP/bracket/replace/multi-leg live behavior.
- RI/RTS and USDRUBF expansion.
- `i64` surrogate adapter without a separate ADR.
