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

Gate 7→8 specification — current authorized planning target. Stage 7B-e R4 was
independently accepted and Stage 7B formally closed at
`a1044e0dbe324c722b637498ca80ffafd9f0cbee`. The accepted Stage 7B chain rests on
independently accepted
Stage 7B-d-a-R1 at `8418cfb63ecee6702bf8a2873592b7cad1e711ee`. The Redis-free durable lifecycle
and seal-before-settlement authorization remains paper/mock only. The original implementation candidate
`f71eeb926464f6634d485d5720b25c5e026b40d5` was not accepted; R1 closes exact
current on-disk seal revalidation and a real fsynced B-046 effect witness. The original
design candidate `09a22765ae6ee37b304bfed6492bd103da44360d` was not accepted
as frozen; Design R1 was independently accepted/frozen at
`00cead2989493b44e0d86ead29b95d57a7fbcbe2`. Stage 7B-c-R1 is independently
accepted and Stage 7B-c is closed at
`c57ae8d5f98bbb11df0a81f78262d3916b276d81`; Stage 7B-b-R2 is closed at
`ff3fa2e8908440863b40b838991d4716b33caad4`. Stage 7B-d implementation is
split into lifecycle/seal barrier, atomic Redis settlement and composite
readiness/restart transport slices. The accepted d-a R1 implementation covers
B-043..B-051 and B-054..B-056 with file-backed/SIGKILL/fault witnesses.
Stage 7B-d-b-R1 is independently accepted at
`e0bf9b7d9eb209e19b875f199511a493ddcd0da9`. The d-c candidate attaches only
the isolated paper consumer and implements composite storage/seal/Redis
readiness, external task supervision, fresh per-boot consumer identity,
bounded old-PEL reclaim and restart duplicate/conflict evidence for B-052,
B-053 and B-064..B-070. FINAM
transport, runtime-live and real orders remain closed pending later stages.
The original d-c candidate `c427ad1c83a27e6a80f45c7e09311ffcae26c913`
was not accepted. R1 restores accepted deterministic pre-Stage6 rejection
ACKs without Stage 6 mutation and adds integrated real-service `PaperReady`
and child-process Redis `XAUTOCLAIM` witnesses. R1 at
`9b98c360e1153e79971b5935d03fd0a0bdd1f4f4` was not accepted because
marker-only terminal history was checked only after effect. R2 adds the
read-only pre-admission marker veto and exact duplicate-publication path.
Stage 7B-d-c-R2 is accepted at
`2b6371adb905654e0ddd8b6714159bcef737b577`. Stage 7B-e aggregate closure
assembled exact normative X01-X20, the inherited Stage 7A full gate, real
infrastructure evidence and the 80/80 proof map. R1 at
`422bd1a8b45bfd3397aa588f914494cc11f5c401` was not accepted; R2 at
`8cc72f148032bedda6a0ef86f6edda2c1394abc7` closed its gate, fault-semantics
and proof-map findings but retained a stale B-079 proof. R3 at
`d501d62543cde890bfbb8d8ea0dc878e28a711b2` closed the intended prefix
mutation but was not accepted because the prefix model did not cover a
production item after the test module. R4 pins the exact changed-path set and
full-file SHA-256 of every allowed crate delta and adds the 59th,
post-test-module hidden-Stage8 mutation. The independent final review closed
the aggregate stage with 80/80 proof rows, 20/20 fault rows and 59/59 negative
cases. FINAM POST/DELETE, runtime-live, real orders and Stage 8 implementation
remain closed while the Gate 7→8 specification is prepared and reviewed.

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
