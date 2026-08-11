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

Stage 7A-R1 — Redis runtime command consumer closure repair, paper/mock only
(implementation candidate; independent acceptance pending).

Stage 2B is closed as the broker-neutral runtime source migration foundation;
Stages 3, 4 and 5 are accepted/closed. Stage 6 is independently accepted and
closed at `10e357825a701193d964975bb5769bd0745d4986`. Stage 7A may prove Redis
consumer-group delivery/recovery against the paper/mock Stage 6 authority.
Stage 7B and Stage 8+ remain closed pending separate acceptance.

## Still blocked

- Runtime-live.
- Redis attachment outside the validated Stage 7A paper namespace.
- FINAM runtime POST/DELETE and broker dispatch.
- Strategy-driven real FINAM orders.
- Stop/SLTP/bracket/replace/multi-leg live behavior.
- RI/RTS and USDRUBF expansion.
- `i64` surrogate adapter without a separate ADR.
