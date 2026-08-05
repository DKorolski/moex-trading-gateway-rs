# Engineering review onboarding — broker-neutral MOEX trading complex

## Purpose

This repository evolves the former ALOR-centred trading stack into a
broker-neutral Rust trading complex. FINAM is the first new broker adapter.
The migration is gateway-first, but the strategy runtime and its established
IMOEXF Hybrid semantics remain the behavioural oracle: infrastructure and
broker boundaries may change; trading logic may not drift silently.

The exact reviewed source is always the commit recorded in
`handoff-commit.txt` inside the immutable handoff archive. This document is a
map for a new reviewer, not an alternative acceptance authority.

## Architecture in one view

```text
FINAM REST / WS
      |
      v
broker adapter + normalized broker truth / market data
      |
      v
broker-neutral protocol v2 + Redis-compatible event contracts
      |
      v
runtime host lifecycle (bootstrap, warmup, restore, callbacks)
      |
      v
unchanged HybridIntraday strategy semantics
```

Order, trade, account and instrument identities are broker-neutral string
types. Strategy `request_id` remains a separate source-owned correlation key;
it is never replaced by client or broker order IDs.

## What has been completed

- Stage 0 fixed the imported ALOR baseline, workspace and safety gates.
- Stage 1/1B extracted the ALOR operational contract and froze the IMOEXF
  Hybrid paper/shadow compatibility baseline.
- Stage 2 migrated runtime source boundaries to broker-neutral protocol v2,
  including string `BrokerOrderId`/`BrokerTradeId`, account and instrument
  identity, state compatibility and exact request-ID ACK semantics. A lossy
  `i64` surrogate adapter remains forbidden without a new ADR.
- Stage 3 closed market-data parity to strategy-input level: historical
  warmup, live stream bars, reconnect/gap recovery and deterministic semantic
  bar handling.
- Stage 4 closed broker-truth bootstrap into the runtime: canonical order,
  trade, position, cash and instrument snapshots; dirty-start/adoption policy;
  readiness and paper/mock host integration.
- Stage 5C froze the no-I/O semantic paper host and callback/timer settlement
  API.
- Stage 5D implemented versioned canonical persistence, private-state and
  riskgate recovery, restart validation and crash-state invariants.
- Stage 5E implemented broker-neutral schedule/session eligibility and the
  controlled callback/settlement boundary.
- Stage 5F attached real IMOEXF Hybrid semantics behind the paper/no-send
  boundary and was independently accepted and closed at
  `fb8245e2f91cfc1678548a1228e8558d9adc2181`.
- Stage 5G-a through 5G-d are accepted/closed: mock ACK, canonical
  order/trade/position convergence, deterministic timers and continuation
  arbitration.
- Stage 5G-e-a and 5G-e-b are accepted/closed: replay commit barrier, owned
  candidate application and exact historical replay metadata.

Historical controlled FINAM endpoint characterization and operator-authorized
micro checks were performed earlier in the project. They do not authorize the
current runtime branch to send orders continuously.

## Current review target

Stage 5G-e-d-a R6 was independently accepted and closed at
`4ece2c7c83ca5575dbca306b5fa29a48dae2bd47`.
The active target is **Stage 5G-e-d-b — deterministic mutation-safe fresh mock
BrokerTruth reducer and executable GRST01–GRST12 matrix**, as one clean
successor to that accepted R6 boundary.

R1 through R5 closed the substantive input boundary, direct reducer shape and
`src`/gate/provenance seal. R6 closes only the compilation-control findings:

- the accepted R5 tree is frozen outside the exact ten-file R6 allowlist;
- root/package Cargo manifests, lockfile and workspace members are exact;
- all 23 runtime Rust targets are fixed and alternate roots/build scripts fail;
- repository-local Cargo config and compiler wrappers fail closed;
- the detached R5→R4→R3→R2→R1→f44→b9 chain remains mandatory lineage evidence.

Validated operational identities remain constructor-only. The e-d-b reducer
performs classification and opaque candidate construction only. It retains both
linear inputs, owns no callback or runtime mutation authority, and does not
open persistence, Redis, FINAM, HTTP, dispatch or live execution.

## Deliberately closed surfaces

The current stage does **not** authorize:

- application of an e-d-b owned reconciliation candidate;
- Stage 5G-e-d-c export/restart/reconcile/re-export evidence;
- Stage 5G-f protective completion;
- Redis live consumer groups or a strategy command consumer;
- FINAM HTTP POST/DELETE or broker dispatch/execution;
- runtime-live, unattended execution or real orders;
- Stage 6+, main merge or deployment.

## Planned sequence after accepted e-d-a R6

The accepted Stage 5G plan remains the controlling sequence:

1. Stage 5G-e-d-b: add the separately reviewed deterministic, mutation-safe
   fresh **mock** BrokerTruth reducer and execute GRST01–GRST12.
2. Stage 5G-e-d-c: export/restart/reconcile/re-export evidence and negative
   closure matrix.
3. Stage 5G-f: paper/mock protective target and stop completion for the eight
   frozen lifecycle cases; no native FINAM stop/SLTP/bracket placement.
4. Stage 5G-g: freeze and reproduce the complete 54-case lifecycle matrix in
   debug, release and parallel modes.
5. Stage 5G-h: aggregate immutable acceptance and close Stage 5G.
6. Request the explicit macro-roadmap transition out of Stage 5. Acceptance of
   Stage 5G alone does not open Stage 6.
7. Stage 6: durable request/client/broker ID chain.
8. Stage 7: runtime command consumer in paper/mock mode.
9. Stage 8: separately authorized real FINAM execution under the command
   consumer.
10. Stages 9–12: reconciliation loop, runtime-live readiness/observability,
   dual-broker shadow parity and first runtime-driven live micro.
11. Stage 13: stop/SLTP/bracket, followed only later by instrument expansion
    beyond the proven IMOEXF path.

## Recommended review reading order

1. `README.md`
2. `docs/roadmap.md`
3. `docs/current-status.md`
4. `docs/stage-5/5g-lifecycle-design-and-implementation-plan.md`
5. `docs/stage-5/stage5g-e-c-clean-process-reconstruction.md`
6. `docs/stage-5/stage5g-e-c-clean-process-reconstruction.json`
7. `scripts/stage5g_ec_check.py`
8. `scripts/stage5g_ec_negative_harness.py`
9. The source/evidence manifests and command logs embedded in the handoff.

Review findings should be scoped against the stable Stage 0–13 roadmap. A
finding may split the active substage into a narrow repair, but should not
renumber the macro-roadmap or open live surfaces without a separate governance
decision.
