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

Stage 7B-d-design-R1 — authority clarification for durable Redis ACK/DLQ/XACK
settlement and paper service composition, still paper/mock only. The original
design candidate `09a22765ae6ee37b304bfed6492bd103da44360d` was not accepted
as frozen; R1 is design/docs/checkers only. Stage 7B-c-R1 is independently
accepted and Stage 7B-c is closed at
`c57ae8d5f98bbb11df0a81f78262d3916b276d81`; Stage 7B-b-R2 is closed at
`ff3fa2e8908440863b40b838991d4716b33caad4`. Stage 7B-d implementation is
split into lifecycle/seal barrier, atomic Redis settlement and composite
readiness/restart transport slices. B-052/B-053 stay pending until d-c supplies
real-Redis restart evidence. This R1 does not attach Redis, and only independent
R1 acceptance may open d-a.

Stage 2B is closed as the broker-neutral runtime source migration foundation;
Stages 3, 4 and 5 are accepted/closed. Stage 6 is independently accepted and
closed at `10e357825a701193d964975bb5769bd0745d4986`. Stage 7A is independently
accepted and closed at `2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64`.
Stage 7B-c-R1 composes one file-backed Stage 6 authority, OS locking, recovery
seals and paper restart ownership. Its acceptance opens only the Stage 7B-d
paper-service composition described in
[stage7b-d-design.md](stage-7/stage7b-d-design.md). Stage 8+ remain closed
pending separate acceptance.

## Still blocked

- Runtime-live.
- Redis attachment outside the validated Stage 7A/7B paper namespace.
- FINAM runtime POST/DELETE and broker dispatch.
- Strategy-driven real FINAM orders.
- Stop/SLTP/bracket/replace/multi-leg live behavior.
- RI/RTS and USDRUBF expansion.
- `i64` surrogate adapter without a separate ADR.
