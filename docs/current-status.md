# Current status — FINAM migration / ALOR parity

Status date: 2026-08-03.

This document is the operator/developer status source of truth. It intentionally
separates what already exists from what is still forbidden for continuous
runtime-live.

The stable macro-roadmap is fixed in [roadmap.md](roadmap.md). Review may split
an accepted macro-stage into smaller patch gates, but it does not renumber or
replace the Stage 0–13 roadmap without a separate roadmap ADR.

## Current transition

- Stage 5F-e aggregate acceptance was independently accepted at
  `fb8245e2f91cfc1678548a1228e8558d9adc2181`; Stage 5F is formally closed.
- The immutable closure facts are recorded in
  [stage5f-closure-descriptor.json](stage-5/stage5f-closure-descriptor.json).
- Stage 5G is the next allowed functional slice, limited to paper/mock
  ACK, order, position, timer, restart and lifecycle-reconciliation work.
- Stage 5G-a is independently accepted at
  `011fd4b7baaa41fffdad7d3c28e463b7977f5989` on `stage5g-lifecycle`.
  Its accepted plan inventories the existing Stage 5C/5D and Broker Core
  authorities and freezes a 54-case lifecycle matrix.
- Stage 5G-b is an implementation review candidate. It adds one linear,
  paper-only mock ACK session, delegates pending-state policy to Broker Core,
  and invokes only the existing Stage 5C-i ACK facade after a complete
  callback-safe vector. Timeout, unknown-pending, missing broker ID, unproved
  expiry, unsupported duplicate and error outcomes remain retained without a
  runtime callback. Stage 5G-c remains blocked pending independent acceptance
  of this implementation.
- Stage 5G-b R1 is an implementation review candidate on immutable base
  `b6f4194769ce0f6c00a82361eba57dc3ed07e55c`. It binds the exact redacted ACK
  projection into fingerprint schema v2, makes contradictory no-send evidence
  fail closed, requires exact duplicate identity, admits only source-authenticated
  Market intents, and uses one deterministic state machine in debug/release
  evidence and in the linear Stage 5C production wrapper. Public Limit/Cancel
  admission remains `NotYetSourceAuthenticated`; no Stage 5G-c surface opens.
- Stage 5G-b R2 is an implementation review candidate on immutable predecessor
  `00d158978904c177828ff2a330b1f3c1bfb4bb10`. It makes exact no-send proof
  dependent on prior lifecycle provenance, preserves observed broker-order ID
  continuity, adds a non-decreasing ACK-time watermark to fingerprint schema
  v3, and exercises the public Stage 5G wrappers against a real linear Stage 5C
  settled capability. Stage 5G-a and R1 remain pinned by detached snapshot
  gates. Stage 5G-c remains blocked pending independent R2 acceptance.
- Stage 5G-b R3 was independently accepted at
  `92f57c7831d8a15fb2e37668d3b07f1ccea03af7` on immutable predecessor
  `d03f6e5e88fb853290457d6d6dac08f21c2cf28b`. It binds the full current
  lifecycle fingerprint into transition schema v4, so post-resolution
  duplicate ACK sequence/count/time watermark cannot collide. Pure and public
  production-wrapper witnesses distinguish `T+20` and `T+30` duplicate
  histories and their `T+25` continuation behavior without repeating Stage 5C.
  The accepted handoff required `origin/stage5g-lifecycle == HEAD`; P0/P1 were
  empty and Stage 5G-b is formally closed.
- Stage 5G-c is an implementation review candidate. It adds canonical Broker Core
  order/trade/position convergence around the ACK-resolved capability, keeps
  active/partial evidence callback-free, and delegates only terminal-complete
  vectors through the existing Stage 5C-j callback boundary. Stage 5G-d remains
  blocked pending independent acceptance of 5G-c.
- Independent review rejected Stage 5G-c R1 at
  `16591e819c571aa2ccb8e4b0d087d28c84090415`. The only authorized successor is
  Stage 5G-c R2-a authority recovery. R2-a restores the exact frozen Stage 5F
  semantic route, confines the required Stage 5C source projection to
  digest-pinned crate-private read-only regions, and keeps the accepted Stage
  5G-a inventory immutable. The rejected R1 convergence semantics are not
  accepted and their five public witnesses are explicitly deferred until R2-b.
  R2-b requires a separate independent ACCEPTED verdict for R2-a. Stage 5G-d,
  Redis, FINAM transport, runtime-live and real orders remain closed.
- Stage 5G-c R2-a was independently accepted at
  `c6ae2bdaea2575dd41e6da00acad5c231f3c7572`. R2-b is now an implementation
  review candidate pinned to that exact detached authority snapshot. It adds
  monotonic Market entry/exit position progression, explicit target Market
  order-state policy, transactional pre-terminal source authentication,
  exact-correlation watermarks, vector-order-independent broker evidence and
  five non-ignored source-reachable production witnesses. Exact source limit
  price and Replace fields are not present in the accepted Stage 5C projection;
  extending that projection remains blocked pending a separate authority
  review. Stage 5G-d and all live surfaces remain closed.
- Stage 5G-c R2-c-a at
  `581f4f6021dd781e7a5db9177be05feb7d94b12a` was rejected as submitted: its
  exact terminal MARKET evidence validator was retained as useful work, but
  the no-callback completion left stale pending/position state while returning
  a false timer-ready type-state. The only active successor is R2-c-a R1.
- Independent review rejected Stage 5G-c R2-c-a R1 at
  `d1b3116ef0b2bdcedbcfd1888f78b2d301a3c654` as submitted while retaining its
  validation and state-coherence direction. The only authorized successor is
  R2-c-a R2.
- Stage 5G-c R2-c-a R2 is a review candidate on that exact predecessor. It
  rejects Canceled/Expired full fills before mutation, binds terminal evidence
  to the real source-owned owner/cycle payload, applies ACK/position/escrow work
  to an isolated transaction candidate, and uses canonical broker-evidence
  time for bracket grace. Inside grace it preserves the timer and returns
  `ReadyForTimer`; the deterministic timer later escrows the residual Exit.
  Ten focused witnesses run through accepted Stage 5F semantics and Stage 5G-b
  Accepted or Submitted→Recovered ACK lifecycle. A separately pinned local
  test clock also removes inherited weekend dependence from three Stage 5E
  parity fixtures without changing production time authority. R2-c-b remains
  closed.
- Independent review rejected R2-c-a R2 at
  `3d995af48e88588909e11505fdefc826ff8f66ce` as submitted because its grace
  decision mixed a local millisecond timer with broker source seconds. The only
  active successor is R2-c-a R3. R3 preserves the exact R2 transaction boundary
  but captures `BrokerTruth.received_ts.timestamp_millis()` before validation,
  so timer origin and decision watermark share the local receipt-clock domain
  and exact precision. Six source-reachable witnesses cover same-second
  post-start receipt, true stale receipt, delayed post-grace receipt, fresh
  snapshot retry, deterministic replay and inherited full-fill/rollback.
  R2-c-b and all live surfaces remain closed.
- Stage 5G-c and its exact replay-package identity are independently accepted
  and closed at `d7561e6f36d01aea3d0dd67892800fbb6ac0a716`.
- Stage 5G-e-b at `cbe4044bbca8303a7852d225364ec5cf89f02386`
  was rejected as submitted because ExactReplay advanced the committed
  checkpoint without synchronizing the continuing Stage 5G-c session. R1 at
  `1621307a6012fa1f9dcbc89a59651c801f6cc26f` fixed the current-package path
  but was rejected because historical exact replay still entered NewPackage
  chronology and current-slot preflight. R2 at
  `6995f8dd2ac226eff33b781f575927361fdc2c45` is independently accepted and
  closes Stage 5G-e-b. Stage 5G-e-c R1 at
  `e269b709d2c3e1a2d3892a88099585bce12d0778` was rejected as submitted because
  its package tests did not reseal the nested lifecycle authority and its
  TimerReady summary/checkpoint lacked source-settlement authority.
  Stage 5G-e-c R2 at `f2f5b1508171632d2e4b211eae79ee6bf3b18178` was rejected
  because all lifecycle commitments still lived inside one rehashable Stage 5G
  extension. Stage 5G-e-c R3 is the only current implementation review
  candidate. It retains the R2 Stage 5C TimerSettlement/recovery/history
  projection, commits its complete source-derived lifecycle authority into the
  independently checksummed Stage 5D envelope, validates that anchor before
  runtime mutation, and requires a nonempty TimerReady history ending in the
  exact settled batch. Its package-level tests reseal every nested checkpoint,
  source/lifecycle hash, extension, envelope and package checksum before proving
  coherent summary, history, recovery, checkpoint and complete-extension
  mutations fail closed. The accepted
  owning exact-replay proof carries the old
  and new checkpoints plus exact canonical evidence, updates the live session
  through the same canonical Stage 5G-c metadata authority, and proves both
  `A→B→exact A→C` and inherited-old-request replay chains. Replay identity and
  fingerprint classification now precedes NewPackage-only continuation,
  current-slot and broker-state checks. Only local sequence and duplicate count
  may change during exact synchronization; slots, identity ledger, BrokerTruth
  receipt watermarks and callback state remain fixed. All public commit
  authority now performs hard checkpoint validation in release builds. The
  accepted Stage 5C callback authority remains byte-pinned to
  `d0494537d7c1739a16350b2d28f71b304165c812`. The new linear clean-process
  boundary consumes one of four supported Stage 5G capabilities, emits only a
  checksummed Stage 5D package, drops the source, and reconstructs semantic,
  private and riskgate state in a freshly configured runtime. Fresh
  BrokerTruth reconciliation and GRST01–GRST12 remain pending; Stage 5G-f
  remains closed.
- Stable macro-roadmap Stage 5 remains active while Stage 5G is incomplete.
  Stage 6 durable command-chain work is not opened by the Stage 5F acceptance.
- Real FINAM `POST`/`DELETE`, Redis live command consumption, runtime-live,
  unattended execution and real orders remain forbidden.

## What exists

- Broker-neutral core contracts for orders, trades, positions, market data,
  readiness, broker truth, runtime host lifecycle, and paper runtime state.
- Stage 3 market-data parity contract is accepted/closed to strategy-input
  level.
- Stage 4A broker-truth bootstrap plan/evidence schema and Stage 4A-1
  plan/schema alignment are accepted as foundation.
- Stage 4B existing broker-truth type inventory and v2 alignment decision is
  accepted.
- Stage 4C validated broker-truth bootstrap wrapper and validation is accepted.
- Stage 4D FINAM read-only broker-truth mapper/source normalization is
  accepted.
- Stage 4E broker-truth to runtime bootstrap application evidence is
  accepted.
- Stage 4F dirty-start / explicit adoption / manual-intervention policy is
  accepted.
- Stage 4G runtime lifecycle ordering evidence is accepted.
- Stage 4H paper/mock runtime-host bootstrap integration tests are accepted.
- Stage 4I redacted operator-facing bootstrap evidence report is accepted.
- Stage 4J FINAM Stage 4 report assembly bridge is accepted.
- Stage 4 macro-stage is accepted/closed as the broker-truth bootstrap
  foundation. The next active macro-stage is Stage 5 — real strategy semantics
  attachment.
- Stage 5A semantic inventory, source provenance, callback/state/configuration
  ledger and evidence schema are a review candidate. No strategy code or live
  execution surface is added by Stage 5A.
- Stage 5B-1 pure Hybrid semantic-kernel import is accepted. The new
  `strategy-runtime-core` crate preserves the frozen model/orchestrator/riskgate
  source and tests but does not include the integrated runtime wrapper.
- Stage 5B-1a immutable correspondence, exact integrated-wrapper oracle and
  active high180 profile hardening are accepted.
- Stage 5B-1b structural freeze hardening and the Stage 5B-2 integrated-wrapper
  semantic inventory are accepted. Workspace membership, crate
  target configuration, library root and the complete Cargo/Rust target set now
  fail closed through parsed TOML plus content/path locks. Bracket
  terminal-reconciliation execution-status matrices, timeout suppression,
  mixed clock domains and source-compatible transient restart behavior are
  inventoried and fixture locked.
- Stage 5B-2a separate wrapper correspondence manifest and broker-neutral
  callback/state boundary hardening are accepted. The map
  matches the exact 15 source overrides and six generic seams, defines lossless
  Hybrid ACK/order/stop/position/bar/context/bootstrap contracts, requires
  context-complete callback inputs, validates attribution/bootstrap
  consistency, and applies an exact parsed-workspace wrapper activation lock
  under the accepted trusted-toolchain/clean-repository threat model.
- Stage 5B-2b exact mechanical wrapper import and public boundary closure are
  accepted. The wrapper is copied and compiled only in
  `strategy-runtime-core`, preserves all source tests, and uses broker-neutral
  request/order/stop identities. Its source-compatible host seam is
  crate-private; the only downstream callback path is the typed
  `BrokerNeutralHybridStrategy` facade, which rejects non-final/non-M10 bars and
  context/payload/target instrument mismatches before state mutation. Runtime
  host attachment, FINAM command consumption and live/send remain disabled.
- Stage 5C-a paper/mock host admission provenance/expiry hardening is a review
  accepted. It accepts only one opaque canonical Stage 4E→4I evidence bundle,
  stores the exact applied snapshot and minimum required-source expiry, rejects
  future or stale evidence at admission time, and binds the complete target
  `InstrumentId`, account scope and instrument price step. It invokes no
  strategy callback and attaches neither a runtime host nor an intent sink.
- Stage 5C-b linear bootstrap-notification type-state is accepted. It
  consumes Stage 4 evidence into strategy-bound admission, then consumes the
  concrete strategy plus admission, rechecks expiry/config symbol/tick binding,
  passes only the exact admitted snapshot into `on_bootstrap_snapshot`, and
  returns `Stage5cBootstrappedPaperStrategy` owning that same strategy instance.
  Every later lifecycle step remains false; active target orders stay blocked
  until ownership-complete mapping is accepted.
- Stage 5C-c runtime-state restore facade hardening is accepted. It
  validates provenance and loads persisted state before exact broker bootstrap,
  matching the ALOR lifecycle, then emits exactly one restored-state callback.
  Quantity/side and broker-owned order-ID postconditions preserve broker truth;
  positive legacy ALOR numeric IDs are genuinely normalized only under an
  explicit policy. Warmup, recovery, bars, intents and execution remain closed.
- Stage 5C-d canonical history warmup facade is accepted. It consumes
  the restored type-state, rechecks evidence freshness and lifecycle timestamp
  monotonicity, and accepts only an opaque Stage 3 provenance/gap-proven batch
  of exact-target final chronological M10 history with no future timestamps.
  It returns an opaque warmed type-state. Recovery, semantic bars, intent sink
  and all execution surfaces remain closed.
- Stage 5C-e pending-stream recovery facade is accepted. It consumes
  the warmed type-state, accepts only complete opaque recovery evidence,
  deterministically deduplicates replayed stream entries and rejects callback
  intents. Semantic bars, intent sink, runtime host and execution remain closed.
- Stage 5C-f first semantic-bar facade is accepted. It consumes the
  recovered type-state and a Stage 3-accepted final M10 capability, captures
  generated intents in an opaque paper result, and attaches no sink, Redis
  command stream or broker transport. Timers and execution remain closed.
- Stage 5C-g paper intent settlement/escrow is accepted. It consumes semantic
  results, validates captured intent shapes and returns an opaque state-bound
  batch without Redis, sink, transport or send.
- Stage 5C-h controlled next-bar loop is accepted. It advances only from
  settled zero-intent batches, preserves unresolved nonzero escrow batches, and
  keeps timer, sink, Redis and broker transport closed.
- Stage 5C-i paper intent lifecycle / ACK escrow resolution is accepted. It
  consumes nonzero settled batches, requires exact ordered ACK coverage for
  captured request IDs, preserves full escrow and typed ACK outcomes, applies
  only broker-neutral ACK callbacks, and still leaves Redis, transport and
  execution closed.
- Stage 5C-j paper broker lifecycle facade is accepted. It consumes
  only Stage 5C-i resolved batches, maps active ACK outcomes to expected
  `Order`/`StopOrder`/`Position` paper evidence, blocks terminal ACKs with
  broker-state events, canonicalizes event sequence, deduplicates identical
  events, and keeps timer, sink, Redis, transport, FINAM command consumer and
  runtime-live closed.
- Stage 5C-k controlled paper timer facade is accepted. It consumes
  only fully resolved Stage 5C-j broker-lifecycle type-state, checks timer
  monotonicity against the ACK/broker-event lifecycle watermark, captures
  timer-generated cleanup attribution before callback mutation, and still keeps
  the timer loop, sink, Redis, transport, FINAM command consumer and
  runtime-live closed.
- Stage 5C-l timer-result settlement facade is accepted. It consumes
  only Stage 5C-k timer type-state, turns zero-intent timers into continuation
  checkpoints, and routes nonzero timer-generated batches back through Stage
  5C-i/5C-j without opening sink, Redis, transport or runtime-live.
- Stage 5C-m timer/bar continuation arbitration is accepted. It consumes only
  Stage 5C-l timer settlements, stores the exact millisecond timer checkpoint,
  allows one ready checkpoint to continue to either one later final bar or one
  later timer, preserves ready settlement on recoverable next-bar blocks,
  exposes the settlement only as an opaque public capability, and blocks
  generated timer batches until Stage 5C-i/5C-j lifecycle resolution.
- Stage 5C-n bounded deterministic paper-loop coordinator is accepted. It
  consumes one accepted Stage 5C type-state and one explicit event per call,
  accepts broker lifecycle only as a terminal-complete atomic Stage 5C-j batch,
  rejects incomplete working-only batches before callbacks while preserving
  ACK-resolved state for retry, settles callback-generated broker intents back
  into ACK lifecycle, delegates only to existing Stage 5C facades, preserves
  recoverable state where available, and keeps autonomous loops, Redis, sink,
  transport, FINAM command consumer and runtime-live closed.
- Stage 5C acceptance/API-freeze package is accepted and Stage 5C is formally
  closed. The closure freezes accepted 5C-a...5C-n slices, startup ordering as
  state-load/clean-prepare -> broker-truth bootstrap ->
  runtime-state-restored, the full paper-host type-state API, manifest
  self-completeness checks, the actual coordinator transition matrix, and the
  no-send execution boundary. Any future change to frozen Stage 5C API/source
  requires an explicit Stage 5C reopening review.
- Stage 5D-a/a2/a3 ownership and additive-extension design is accepted. Stage
  5D-b1 dual-baseline enforcement migration is accepted. Stage 5D-b2a adds the
  schema-only versioned persistence envelope/API surface: snapshot identity and
  revision metadata, write generation, per-family timestamp policy, canonical config
  fingerprint, account/instrument/profile/runtime binding, versioned semantic
  StrategyState JSON payload with a strict Stage5d-owned HybridIntraday schema,
  strict instrument binding, strict unknown-field decoding, checksum
  validation, lifecycle watermarks, typed broker-neutral recovery indexes,
  source-compatible runtime-private enums, runtime-private extension DTOs,
  riskgate ledger/materialized/outbox DTOs, source-exact runtime pending
  riskgate finalization payloads with source-producible weekday vector chronology, an opaque validated-envelope
  capability for future gates, deterministic fixtures,
  corrupt/version/config/unknown-field/semantic/timestamp/source-roundtrip/
  pending-lifecycle/recovery-index/riskgate-finalization negative validation,
  and full public Stage5d API-surface freeze enforcement. Stage 5D-b2b-a opens
  the first controlled runtime-private export/apply bridge, and Stage 5D-b2b-b
  adds controlled broker-truth bootstrap notification after private apply: exact
  loaded-capability/envelope pair binding, validated-envelope gated private
  extension apply, retained opaque restore evidence, retry-capable opaque block
  preservation, required cleanup retry state, full pending
  riskgate-finalization vector restoration, persisted-owned semantic projection
  binding, crate-private persisted-vs-clean load provenance, semantic/recovery
  provenance fingerprints, authoritative Stage 5D canonical config/version/source
  build binding, hashed riskgate seed/ledger identity in the canonical
  fingerprint, pending-entry source-shape/config exactness checks,
  checker-owned exact private-layout extension enforcement with negative cases
  for self-authorized semantic drift, source-private invariant preflight, and no
  authoritative working-set rehydration from persistence. Stage 5D-b2b-b treats persistence working sets
  only as hints, requires broker-truth position match, blocks missing expected
  working orders, blocks stop hints until stop-truth surface opens, and still
  fails closed on confirmed active target orders until ownership mapping opens.
  Recoverable bootstrap blocks can now retry only through
  `stage5d_retry_broker_truth_bootstrap(...)` with a fresh matching Stage 5C
  admission; cross-binding refresh attempts preserve the blocked capability and
  fail closed. Stage 5D-b2b-c adds validated authoritative riskgate ledger
  evidence plus riskgate projection injection through
  `stage5d_inject_authoritative_riskgate(...)` after broker-truth bootstrap and
  before runtime-state-restored. It rebuilds materialized state from
  source-compatible normalized ledger records, checks all
  `RiskGateProfileIdentity` fields against runtime config, blocks disabled
  riskgate-profile callback no-ops, validates ledger-tail hash, enforces
  durable outbox crash-consistency/idempotency identity, and supports controlled
  retry with fresh validated ledger evidence without repeating private apply or
  broker bootstrap. Stage 5D-b2b-c1-r8 closed the review hardening around the
  riskgate-injected capability. It distinguishes full authoritative, durable materialized and
  semantic-runtime frontiers; accepts only exact outbox-explained crash lag;
  requires semantic current-shadow session/PnL to match authoritative materialized
  evidence exactly; rejects negative zero at Stage 5D riskgate authority
  boundaries; validates that the current-shadow tuple is source-producible; and
  retains a deterministic no-I/O recovery plan in the opaque injected capability.
  The plan is bound to
  envelope/evidence/identity/generation and exposes only redacted
  count/completion/fingerprint diagnostics. Source-exact decimal canonicality,
  row-derived `seed_loaded`, exact runtime-pending evidence for every lagging
  runtime frontier and stepwise multi-row recovery-to-complete tests with replay
  checks are enforced. c1-r8 additionally restores the canonical immutable Stage
  5C closure manifest, represents riskgate codec changes through Stage 5D-owned
  controlled semantic extension evidence, validates later processed watermarks by
  the bound source runtime policy, and proves source-produced current-shadow
  positives without post-export editing. CI requires the 82-case Stage 5D harness
  plus the isolated marker-pinned 87-case forbidden harness with positive-baseline and
  self-protection checks. The forbidden harness supported worker contract is
  pinned at default/max four workers, 180-second per-case timeout and a
  75-minute CI timeout. Review
  handoffs remain fail-closed and commit-bound. Stage 5D-b2b-d1-r6 is the active
  runtime-restored acceptance-evidence/blocker-uniformity hardening candidate:
  it consumes only the
  opaque `Stage5dRiskGateInjectedPaperStrategy`, requires complete
  source-produced recovery evidence before the callback, delegates through one
  checker-pinned crate-private Stage 5C bridge, returns the exact
  `Stage5cRuntimeStateRestoredPaperStrategy` on success, preserves the input
  capability on pre-callback blocks, treats post-callback failures as terminal,
  rejects callback intents through the runtime guard before any debug assertion,
  enforces bootstrap-notification chronology before callback, and treats
  flat/long/short broker side truth as exact. The Stage 5D checker pins the
  crate-private bootstrap, riskgate and runtime-state-restored bridges to one
  definition and one production call-site each, with negative cases for direct
  calls, aliases, forwarding wrappers, function references, extra Stage 5D
  calls, missing intent guard, debug-assert-only guard, missing timestamp guard,
  missing exact side guard, missing source-produced Long/Short/realized-PnL
  restored-transition proof, missing single/multi-row recovery restored
  transitions, missing pre-callback state-fingerprint preservation, and missing
  compile-fail type-state guards. r4/r5 additionally pin genuine
  broker-position Long/Short positives through strict JSON round-trip,
  non-empty known-order and pending-request retention through strict
  round-trip, open-position side-mismatch blockers, explicit paper-only and
  non-acknowledged recovery-decision blockers, blocker ownership evidence, and
  source pre-bind exact-state proof. r6 additionally converts every
  representable pre-callback blocker to the common retained-capability helper,
  checks retained paper-only/runtime-host/intent-sink flags, adds a
  machine-readable blocker-ownership inventory, pins ownership drift through
  93 Stage 5D negative cases, and documents strict malformed payload ownership.
  The formal mutation policy is
  `controlled_validated_stage5d_apply_then_broker_truth_bootstrap_then_riskgate_injection_then_restored_callback_only`;
  Redis bridge, FINAM execution, broker transport, runtime-live and autonomous
  loop remain closed.
- Stage 5D final restart r2 is retained only as the canonical durable-package
  foundation. Stage 5D-final-restart-r3 now has exactly 21 accepted executable
  rows and 0 `todo_source_produced` rows in the mandatory positive inventory.
  The latest riskgate-recovery r1-r4 evidence closes the final three riskgate
  rows with a crate-private production recovery executor, typed checkpoint and
  final receipts, fresh reader reopen checks, pre/post-commit crash idempotency,
  persisted final-receipt equality, source-produced pending finalizations,
  exact summary goldens and Stage 5C warmup continuation. Aggregate Stage 5D
  closure r2 is accepted as the no-I/O persistence/restart/recovery-semantics
  foundation. Stage 5E-B3F is accepted and closed at
  `e14654f7129aa61011931306140a3bfefe2fcfbc`: the one private callback and
  settlement route is protected by the accepted B3F checker, immutable source
  pins, two complete semantic-region digests, the 580-case provenance harness
  and the eight-case production UI contract. Its descriptor remains a closure
  descriptor and is not repointed by later work.
- Stage 5F is accepted and closed at
  `fb8245e2f91cfc1678548a1228e8558d9adc2181`. Its lineage began with the
  inherited Stage 5F-a atomic-Hybrid entry contract, which has a
  separate descriptor and gate that first runs the accepted B3F checker and UI
  harness from the exact accepted source snapshot. Canonical CI fetches that
  immutable predecessor and uses the same detached-snapshot wrapper as handoff
  packaging for the inherited 580-case provenance matrix; it never applies the
  frozen Stage 5E gate to a Stage 5F head. Stage 5F-a-r3 executes the verified
  wrapper before any Stage 5F repository-owned harness and freezes the workflow,
  wrapper, gate, verifier and both negative harnesses as one accepted authority
  set. Stage 5F-a-r6 restores the R3 gate bytes, so canonical CI, inventory,
  entry checker, handoff checker and the actual gate carry one exact digest.
  Its protected-base, read-only `pull_request_target` authority workflow treats
  PR content only as data. Ordinary PRs preserve the R3 and protected-base
  roots; a dedicated, versioned rotation manifest can advance them only when
  the old base contract binds the exact base SHA, state generation, complete
  candidate `{git_mode, sha256}` bindings and a coherent gate digest. R7 reads
  the committed Git tree, rejects gitlinks/non-blobs/unsupported modes and
  mode-only authority drift, blocks all ordinary PR changes under
  `.github/workflows/**`, and permits only the exact base-authority workflow in
  a reviewed rotation while canonical `ci.yml` remains immutable. Rotation
  admits only Stage 5F governance/docs/fixture paths, never Rust or operational
  surfaces. The first hosted activation exposed an undeclared `rg` dependency
  in the forbidden-surface scanner; the negative harness failed safely rather
  than accepting seven undetected scanner mutations. Stage 5F-a-r8 is merged
  as the generation-2 authority repair and R8a is merged as its governance-only
  generation-3 amendment. The present R9 generation-4 candidate replaces the
  scanner's six `rg` searches with the already-required Python 3.11+ path and
  makes the worker's infrastructure check portable; it atomically rebinds only
  the paired Stage 5D freeze manifest/checker. Its full 87-case negative
  harness passes with `rg` absent from `PATH`. Schema and generation values are
  exact JSON integers (never `true` or a float), and the capability cannot be
  replayed after the generation-4 state. R9 itself changed no workflow,
  Cargo/Rust or operational surface. A bounded R9a CI-preparation patch then
  primes only the exact B3F snapshot's locked Cargo dependencies before the
  unchanged offline UI harness runs on a clean hosted runner; it adds no
  runtime, broker, transport or order path. GitHub
  activation still requires an independent approval after the latest update,
  strict up-to-date branch and no direct/admin bypass. It then
  admits only the existing
  IMOEXF `canonical_final_m10` paper route through the broker-neutral Hybrid
  runtime, high180/riskgate, the single Hybrid orchestrator, ordered semantic
  intents and B3F settlement. Full BO/MR/riskgate/arbitration parity is one
  state-machine acceptance matrix; no BO-only or MR-only slice can claim
  success. ACK/order/position/timer/restart feedback remains Stage 5G and
  same-input ALOR differential replay remains Stage 5H. Redis, FINAM, broker
  transport, dispatch, persistence opening, runtime-live and real execution
  remain closed.
- FINAM REST read-only/auth/client DTO and mapper foundation.
- FINAM WebSocket market-data shadow path for `BARS`/`QUOTES`.
- Closed-bar finalizer and FINAM M1-to-canonical-M10 paper runtime path.
- Paper-only hybrid runtime-state projection.
- ALOR-oracle seeded FINAM paper runtime state for IMOEXF hybrid parity:
  previous-day features, current-day features, `next_cycle_seq`, and riskgate
  summary can be seeded from the ALOR runtime state stream before paper
  processing.
- Guarded operator one-shot actual FINAM order harness for controlled
  `MARKET`/`LIMIT`/`CANCEL` micro checks.
- Durable order-path and endpoint-boundary design/evidence for guarded
  one-shot use.

## Still disabled

- Continuous runtime-live trading.
- `command-consumer-to-real-FINAM`.
- Strategy-runtime-to-real-FINAM order routing.
- Runtime `LiveReady` for FINAM.
- Stop/SLTP/bracket/replace/multi-leg.
- RI/RTS expansion.
- Any automatic live send from strategy intents.

## Current parity status

The FINAM contour is a paper/shadow parity stand, not a drop-in replacement for
the ALOR gateway/runtime yet.

Stage 1B hard-freeze scope:

- in scope: IMOEXF `HybridIntradayRuntime` paper/shadow parity;
- out of scope: USDRUBF `AlorUsdrubfHybrid`, RI Author41/42,
  `SessionGapStandalone`, generic `CancelSent`/`Done` migration,
  Stop/SLTP/bracket, runtime-live.

Stage split:

- Stage 1A is a draft/spec foundation: README/status/workplan, seeded bridge,
  and safety boundary.
- Stage 1B is accepted as the hard compatibility-freeze work for IMOEXF
  `HybridIntradayRuntime` paper/shadow parity: field-by-field mappings, Redis
  stream/group mapping, fixtures, seed-required policy, accepted ADR, and
  stronger evidence.
- Stage 2A is accepted and closed: runtime source migration inventory and plan
  for the accepted broker-neutral `BrokerOrderId(String)` path are complete.
- Stage 2A-final inventory completion added concrete `HybridIntradayRuntime`,
  `trade_ledger`, runtime command-builder, and ALOR cancel/replace DTO surfaces
  to the migration inventory.
- Stage 2B implementation plan is accepted. Controlled runtime source migration
  implementation proceeded paper/mock/local only, in small reviewable patches.
  Runtime-live and the real FINAM command consumer remain blocked.
- Stage 2B is closed as the runtime source migration contract/foundation after
  Stage 2B-11 acceptance report. Stage 2B-N patch gates were implementation
  safety gates inside Stage 2 and do not replace Stage 3 market-data parity or
  later macro-stages.
- Stage 2B-1 and Stage 2B-1a are accepted. Stage 2B-2 adds passive DTO/state
  migration contracts for old ALOR numeric order ids and broker-native string
  ids; it does not attach runtime-live or real FINAM command consumption.
- Stage 2B-3 is accepted. It validates passive runtime order maps and bootstrap
  working-order maps: map keys must match payload broker order ids, and missing
  known order ids become readiness/manual-intervention blockers.
- Stage 2B-4 is accepted as CommandAck / OrderEvent / TradeEvent lifecycle
  boundary foundation.
- Stage 2B-4a is accepted: explicit ACK status policy hardening is complete.
- Stage 2B-5 is accepted as passive RuntimeCaches / ownership tracking
  foundation.
- Stage 2B-5a is accepted: explicit order ownership / attribution hardening is
  complete.
- Stage 2B-5b core BrokerTradeId invariant is accepted.
- Stage 2B-5c broker-finam trade_id fallible mapping is accepted.
- Stage 2B-6 TradeLedger migration is accepted as broker-neutral foundation.
- Stage 2B-6a TradeLedger blocker lifecycle and duplicate fill replay hardening
  is accepted.
- Stage 2B-7 HybridIntradayRuntime-owned id migration is accepted as
  broker-neutral contract layer.
- Stage 2B-8 command builders / CancelOrder / ReplaceOrder DTO shape migration
  is accepted.
- Stage 2B-9 deterministic request-id stability is accepted.
- Stage 2B-10 combined paper/mock compatibility test pack is accepted.
- Stage 2B-11 acceptance report closes Stage 2B as broker-neutral runtime
  contract/foundation.
- Stage 3 followed Stage 2B and is now accepted/closed as market-data parity to
  strategy input level.
- Stage 3A is accepted: market-data parity plan and evidence schema are
  accepted as the planning/schema foundation.
- Stage 3B is accepted after Stage 3B-1 hardening: source-only market-data
  parity comparator contract, synthetic fixture tests, strict M1-only
  derivation, and publication counters that do not treat blocked candidates as
  strategy/model bars.
- Stage 3C is accepted after Stage 3C-1 hardening: source/report-only
  multi-bucket redacted report generator with explicit duplicate bucket
  normalization and no silent overwrite.
- Stage 3D is accepted as offline/source-controlled evidence collector
  foundation: controlled inputs, FINAM M1-to-M10 derivation, Stage 3C report
  invocation, source/session metadata, and redacted JSON artifact writing.
- Stage 3D-1 is accepted as recovery/session/input-gate hardening foundation:
  blocks failed or missing recovery, unknown schedule state, invalid ALOR oracle
  shape, invalid source archive hashes, and invalid session dates before the
  report can be treated as synchronized strategy-input evidence.
- Stage 3D-2 is accepted: recovery/session consistency hardening rejects
  `AttemptedAndComplete` unless replay was attempted, gap absence was proven,
  the first fresh live final bar was observed, and entry stayed blocked while
  the gap was unproven; unknown schedules must remain blocking.
- Stage 3D-3 is accepted as offline controlled operator-run input adapter
  foundation: it reads approved/redacted ALOR native M10 and FINAM final M1
  source files, validates source kind/session/instrument/finality, invokes the
  Stage 3D collector, and writes a redacted parity report plus counts-only
  operator summary.
- Stage 3D-3a is accepted as approved input schema/session-window hardening:
  approved input source schema is
  explicitly versioned as v2 and config/source `session_window_utc` is required;
  ALOR/FINAM bars outside the approved window are rejected before evidence is
  accepted.
- Stage 3E is accepted as recovery/gap evidence foundation: reconnect/gap
  recovery evidence wraps the broker-neutral market-data recovery report, proves
  entry is blocked while gap is unproven, keeps exit/cancel/repair unblocked by
  that entry guard, suppresses replay/overlap bars from strategy/model
  publication, and allows strategy input only after gap proof plus first fresh
  live final bar.
- Stage 3E-1 is accepted as recovery-report consistency and counter hardening:
  recovery report must be M10 strategy timeframe, recovery timestamps must sit
  inside the approved session window, reconnect summary/report phases must not
  contradict each other, and post-recovery publication counters must be
  arithmetically valid.
- Stage 3E-2 is accepted as replay-window evidence completeness hardening:
  `RecoveryComplete` now requires explicit replay-window evidence fields,
  positive replay bar count, valid replay-window ordering, and first fresh live
  final strictly after replay.
- Stage 3E-3 is accepted and closes Stage 3E: replay window must cover the last
  final strategy-bar watermark, recovery mode must match warm/cold attempt
  flags, and `checked_ts` must not precede the first fresh live final.
- Stage 3F is accepted as the Stage 3 market-data parity acceptance report.
  Stage 3 is accepted/closed as market-data parity to strategy-input level.
- Stage 4 was accepted/closed as broker-truth bootstrap into runtime. Current
  active macro-stage is Stage 5: real strategy semantics attachment.
- Stage 4A is accepted/closed as broker-truth bootstrap
  planning/evidence-schema foundation.
- Stage 4A-1 is accepted/closed as plan/schema alignment: the Stage 4
  breakdown is expanded to 4A–4J; existing broker-truth/runtime-host type
  inventory is required before coding; lifecycle-order, explicit adoption,
  ownership/correlation, and numeric freshness evidence are represented in the
  schema.
- Stage 4B is accepted as existing broker-truth type inventory and v2 alignment
  decision. It chooses reuse/extend/wrap decisions around the
  existing `BrokerTruthSnapshot`, `RuntimeHostBootstrapSnapshot`,
  `RuntimeBootstrapSnapshotDto`, FINAM mapper, M3f/M3g issue machinery, and
  broker-truth parity helpers.
- Stage 4C is accepted after P1 hardening and final adoption-count guard as a
  validated wrapper around existing
  `BrokerTruthSnapshot` and `RuntimeHostBootstrapSnapshot`, with broker-truth
  source status, safe schedule freshness semantics, strict adoption validation,
  stronger target trade correlation, freshness,
  ownership/correlation summaries, dirty-start/adoption disposition, restored
  runtime-state checks, external issue bridge, and closed safety boundary.
- Stage 4D is accepted as FINAM read-only broker-truth source-normalization
  into the Stage 4C validator. It adds explicit FINAM source evidence
  (`Present`, `Missing`, `Unavailable`, `DecodeFailed`, `Incomplete`),
  per-section freshness for positions/orders/trades/cash/instruments/schedule,
  target-bound schedule state handling, placeholder snapshot semantics for
  missing/unavailable/decode-failed source, and fixture-backed blockers for
  active/unknown target orders, unowned target trades, missing/ambiguous
  instrument identity, stale source sections, and schedule-symbol mismatch.
- Stage 4E is accepted as an application-evidence gate around validated broker
  truth. Runtime bootstrap notification is allowed only for an
  internally consistent `BootstrapReady` report; all
  incomplete/stale/mismatch/unknown-schedule/manual/evidence/safety statuses
  remain blocked, and contradictory `BootstrapReady` reports are rejected as
  `ValidatedBootstrapInconsistent`. Restored runtime state is accepted only
  after broker truth, cannot overwrite broker truth, target/account scopes stay
  separated, and live/execution authorization remains closed.
- Stage 4F is accepted as a dirty-start policy gate after Stage 4E. It carries
  full adoption evidence into the application/operator decision,
  evaluates position and order adoption separately, requires explicit
  attempted/allowed/applied adoption with exact broker-truth qty/count matches,
  requires Stage 4E application evidence to exactly match the canonical
  decision for the same validated report,
  treats runtime-owned active target orders as non-adoptable lifecycle truth,
  keeps non-target account-wide dirty state diagnostic by default, and still
  forbids runtime-live, real FINAM command consumption, POST/DELETE, and
  Stop/SLTP/bracket.
- Stage 4G is accepted as runtime lifecycle ordering evidence after accepted
  Stage 4E/4F. It requires canonical application/policy evidence, validates
  ALOR-compatible lifecycle order, suppresses final bootstrap notification on
  any lifecycle blocker, and still forbids runtime-live, real FINAM command
  consumption, POST/DELETE, and Stop/SLTP/bracket.
- Stage 4H is accepted as paper/mock runtime-host bootstrap integration tests.
  It emits a deterministic mock runtime event trace only after accepted Stage
  4G and emits no bootstrap/restore/warmup/pending events for stale broker
  truth, unknown schedule, manual intervention, noncanonical policy, invalid
  lifecycle order, live authorization attempts, or internally
  inconsistent/tampered Stage 4G lifecycle DTOs.
- Stage 4I is accepted as a redacted operator-facing bootstrap evidence report.
  It summarizes Stage 4C validation, Stage 4D per-section source evidence,
  Stage 4E application, Stage 4F dirty-start/adoption policy, Stage 4G
  lifecycle ordering, and Stage 4H mock runtime trace. Required non-present
  source evidence blocks the report; blocked reports carry an explicit reason
  chain and emit no runtime events. Runtime-live, real FINAM command
  consumption, POST/DELETE, and Stop/SLTP/bracket remain forbidden.
- Stage 4J is accepted as the broker-core/FINAM Stage 4 report assembly
  bridge. It builds the full Stage 4C→4I report from a FINAM Stage 4D read-only
  package using the preferred source-evidence path, not the synthetic
  compatibility builder. It remains report/evidence only.
- Stage 4 is accepted/closed as broker-truth bootstrap foundation. It provides
  the accepted read-only/paper evidence chain required before real strategy
  semantics can attach. It does not authorize runtime-live, real FINAM command
  consumption, POST/DELETE, or Stop/SLTP/bracket.
- Next active macro-stage: Stage 5 — real strategy semantics attachment.

Green / mostly closed:

- FINAM WS live market-data reaches Redis.
- Fresh M1 final bars can produce canonical M10 runtime input.
- FINAM paper runtime state can now match ALOR IMOEXF hybrid state on the active
  M10 bar after ALOR-oracle seeding.
- ALOR-oracle seed now preserves pending/deferred/safe-mode/protective-state and
  dirty-start/manual-intervention placeholders as explicit paper parity fields.
- `seed_required=true` can hard-block a parity run when the ALOR oracle seed is
  missing or cannot be parsed.
- Stage 5C is closed and Stage 5D has a final restart-closure r2 candidate:
  canonical source-owned export from the actual `HybridIntradayRuntimeStrategy`
  to a strict Stage 5D restart package containing both the persistence envelope
  and durable riskgate ledger evidence. The r2 path proves strict package JSON
  decode after source drop, package/evidence checksum validation, loaded-state
  binding, private apply, broker-truth bootstrap, authoritative riskgate
  injection, return to the Stage 5C restored capability, explicit history
  warmup continuation, durable crash/replay states and golden-vector
  determinism in paper/no-send tests.
- Safety flags remain closed in paper state:
  `live_orders_enabled=false`, `runtime_live_ready_enabled=false`,
  `command_consumer_to_real_finam_enabled=false`,
  `external_order_endpoint_enabled=false`, `stop_sltp_bracket_enabled=false`.

Amber:

- Full-session operator FINAM-vs-ALOR M10 evidence is still required before
  runtime-live/cutover decisions; Stage 3F closes the input contract but does
  not replace later same-session strategy evidence.
- Stage 4 makes validated broker truth and lifecycle ordering available as the
  mandatory foundation, but the real Hybrid strategy has not yet consumed that
  chain.
- Paper runtime projection has ALOR-compatible fields, but it is not yet the
  real ALOR hybrid BO/MR orchestrator.
- Riskgate state can be seeded/projected, but true riskgate ledger integration
  is not complete.
- Stage 5D final restart r2 closure is still a review candidate until accepted.
  It proves the clean-process paper/no-send restart path through a durable
  package boundary and scenario inventory, but does not authorize Stage 6+
  durable command-chain work, command consumers or live execution.

Red / not yet implemented:

- Real ALOR strategy-runtime semantic attachment.
- Runtime command consumer under paper/mock ACK parity.
- Runtime-driven live micro.
- Orders/trades/positions streaming or polling reconciliation loop at ALOR-level
  maturity.
- Any default or implicit `i64` surrogate adapter for FINAM broker order ids.

## Required gates before runtime-driven live

1. ALOR runtime compatibility contract v1 accepted.
2. Runtime source adaptation vs binary-compatible adapter ADR accepted.
   Current accepted decision: runtime source migration to broker-neutral
   `BrokerOrderId(String)`; surrogate adapter remains forbidden without a new
   ADR.
3. Stage 3F accepted, plus any additional full-session operator parity evidence
   required by the later runtime-live/cutover review.
4. Broker truth bootstrap wired into runtime lifecycle.
5. Real hybrid BO/MR/riskgate semantics attached behind paper boundary.
6. Request-id/client-order-id/broker-order-id durable chain implemented.
7. Runtime command consumer proven in paper/mock ACK mode.
8. Orders/trades/positions reconciliation loop accepted.

Only after these gates should `command-consumer-to-real-FINAM` or
runtime-driven live micro be discussed.
