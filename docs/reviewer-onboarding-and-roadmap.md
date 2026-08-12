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

Stage 6E-R1 is independently accepted at
`10e357825a701193d964975bb5769bd0745d4986`, closing Stage 6. Stage 7A is
independently accepted and closed at
`2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64`.

Stage 7B-a-R1 is independently accepted at
`a947c24bb413a91c5eb0ad97f4ac0b402bfd0641`. The active implementation
candidate and next review target is **Stage 7B-b-R2**, limited to anchored
durable-root validation and kernel single-writer ownership. The broker-neutral
`runtime-durable-service` authority binds one canonical directory to the
authenticated Stage 6 operational identity, retains its directory FD and exact
inode, resolves children via `openat`, and retains root/sidecar locks and journal
as one linear authority.
It remains paper/mock only.

Kernel writer locking, the canonical recovery seal, cross-process restart,
idempotent Redis settlement and the complete X01-X20/B-001..B-080 closure are
planned follow-on Stage 7B slices and are not claimed by Stage 7B-b-R2.
Redis consumer names remain transport metadata and never enter request/client
or Stage 6 durable identity. Stage 7A constructs no fresh-truth package.
Stage 7B, FINAM POST/DELETE, broker
dispatch, runtime-live, real orders and native Stop/SLTP/bracket remain
forbidden.

## Previous e-d-c target

Stage 5G-e-d-a R6 was independently accepted and closed at
`4ece2c7c83ca5575dbca306b5fa29a48dae2bd47`. The first e-d-b implementation at
`8a02f2a6b6e27587539d1e4e4717301bf010e6a1` was rejected on five reducer
semantics findings. R1 at `b0ede8bbdfa99e7b2b06fd7f4f04db128d5f625b`
also remained rejected. R2 at `c5f84bbcf7c1b44c1eac9c2e99857834d333a4c4`
was rejected on LIMIT price authority, cancel correlation, historical rows and
semantic refresh. R3 at `f9bc372f7ad5a56514ce1d6ad7ffd4f54097bb28`
was rejected on global history, flat canonicalization, cancel target authority
and immutable target-order payload findings. R4 at
`66c5fbd2518ec2e7398c88bb59cc7e4dae3ce1bd` closed those findings but was
rejected on all-terminal idempotency, safe same-status terminal late fills and
missing-owned history evidence. Stage 5G-e-d-b R5 was subsequently accepted at
`2b2bcc671c68722b3b84b914b785ffcb83f6802d`.

The active target is **Stage 5G-e-d-c — linear fresh-truth candidate
application**. It consumes the opaque accepted reducer result exactly once,
reuses the canonical order/position transition, proves post-state equality and
returns only a freshly restored authenticated package capability. Replay Policy
B is explicit: ExactReplay remains disabled until a later durable authority is
designed. No callback, Redis, FINAM, HTTP, dispatch, runtime-live or real-order
surface is opened.

The submitted e-d-c commit `18240b26a5bea77ea71c851f72a644706a7e0b57`
is the immutable R1 predecessor. R1 seals the previously reviewable package
minting seam with one non-cloneable post-application token, proves independent
candidate/post/restored hashes and whole-state invariants, and adds 14
phase-specific failure witnesses, 12 full-chain GRST witnesses, exact
disposition coverage, actual-type ownership checks and fully resealed semantic
tamper rejection. R1 remains paper-only and does not claim external durability;
its handoff must be one direct successor on `stage5g-lifecycle`.

R2 is the active e-d-c closure patch on top of
`67e13aeecd3bf0dc33e570770b0e4b90f5fec0cf`. It preserves the R1 application
shape and adds a private linear source proof, field-by-field source/evidence
validation, source-bound fresh `captured_at`, final authority over
`post_restart_package_fingerprint_sha256`, restore-side recomputation of that
fingerprint and real serializer/reconstruction/Policy-B failure phases.

R1 through R5 closed the substantive input boundary, direct reducer shape and
`src`/gate/provenance seal. R6 closes only the compilation-control findings:

- the accepted R5 tree is frozen outside the exact ten-file R6 allowlist;
- root/package Cargo manifests, lockfile and workspace members are exact;
- all 23 runtime Rust targets are fixed and alternate roots/build scripts fail;
- repository-local Cargo config and compiler wrappers fail closed;
- the detached R5→R4→R3→R2→R1→f44→b9 chain remains mandatory lineage evidence.

Validated operational identities remain constructor-only. R4 retains the R3 binding of all twelve
identity fields to authenticated restart authority and treats replay lists as
untrusted hints until an authenticated fresh-truth ledger exists. It uses
domain-separated commitments, exact client/broker trade linkage and computes position from
typed intent plus exact Decimal pre-position. The e-d-b reducer performs
classification and opaque candidate construction only. It retains both linear
inputs, owns no callback or runtime mutation authority, and does not open
persistence, Redis, FINAM, HTTP, dispatch or live execution.

R5 deliberately fails source LIMIT recovery closed until canonical Decimal/tick
price authority exists. It separates cancel command identity from target-order
identity, partitions unrelated terminal orders and historical trades, blocks
partial-ID conflicts, applies semantic terminal refresh equality, and requires
complete exact pre-position truth for Working/New candidates.
It additionally performs the harmless-history partition before no-slot
decisions, emits history counts independently of candidate existence, treats
complete empty and explicit-zero target positions as the same flat state,
derives cancel target-client authority only from accepted target-order rows,
and freezes the complete immutable target-order payload across lifecycle
progress.
It also makes exact Filled/Rejected/Canceled/Expired truth a single GRST06
no-candidate path, permits only proven same-status Canceled/Expired late-fill
supersession, and retains history counts on missing-owned/conflict outcomes.

## Deliberately closed surfaces

The current Stage 5G-f candidate does **not** authorize:

- Stage 5G-g/h aggregate/freeze implementation;
- Redis live consumer groups or a strategy command consumer;
- FINAM HTTP POST/DELETE or broker dispatch/execution;
- runtime-live, unattended execution or real orders;
- Stage 6+, main merge or deployment.

## Planned sequence after accepted e-d-a R6

The accepted Stage 5G plan remains the controlling sequence:

1. Stage 5G-e-d-b R5: independently accept the restart-bound deterministic,
   mutation-safe fresh **mock** BrokerTruth reducer and owning GRST01–GRST12
   evidence.
2. Stage 5G-e-d-c: export/restart/reconcile/re-export evidence and negative
   closure matrix.
   - R2 at `95901eb9bf19e103e9acb82fb9726708f356b4cd` closed the accepted
     application-authority design defects but was held because the
     source-proof field map was not mutation-sealed.
   - R3 is the current narrow candidate: exact machine-readable
     source-proof field-map descriptor, direct production source-map
     mutations, independent source-oracle witnesses and parent
     revision/package-instance cross-binding. Stage 5G-e-d-c closes only after
     independent R3 acceptance.
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
