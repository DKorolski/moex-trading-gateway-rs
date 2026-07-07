# ALOR parity workplan — 2026-07-07

Status: active implementation plan.

This plan follows the engineering audit conclusion: FINAM is not yet an
operational replacement for the existing ALOR-centered gateway/runtime. The next
steps must narrow semantic gaps under paper/shadow boundaries before any
runtime-driven live trading is discussed.

## Safety boundary

Allowed:

- FINAM read-only and WebSocket market-data shadow;
- FINAM paper runtime;
- ALOR oracle reads for parity/seeding;
- paper/mock ACKs;
- redacted evidence reports.

Forbidden until a later accepted gate:

- continuous runtime-live;
- `command-consumer-to-real-FINAM`;
- strategy-driven real FINAM sends;
- Stop/SLTP/bracket/replace/multi-leg;
- FINAM Runtime `LiveReady`.

## Current active step

M4-3x seeded ALOR-oracle FINAM paper parity.

Goal: keep FINAM paper runtime comparable with the active ALOR IMOEXF hybrid
runtime while we are still missing full ALOR hybrid/riskgate semantic attachment.

Accepted only as a bridge:

- seed ALOR runtime/riskgate context into FINAM paper namespace;
- process FINAM live M1 into canonical M10 paper input;
- compare FINAM paper state with ALOR runtime state field by field;
- never use this seed as live-order permission.

## Immediate sequence

1. M4-3x review package
   - Document the seeded paper parity boundary.
   - Provide runtime-state parity evidence.
   - Keep generated reports out of the source archive unless a review package
     explicitly binds them.

2. M4-3y full-session M10 parity
   - Compare FINAM assembled M10 bars vs ALOR runtime/oracle M10 over a full
     active session.
   - Classify every mismatch: timestamp, OHLCV, source freshness, session break,
     reconnect/gap, or expected broker feed difference.

3. M4-4 broker-truth bootstrap
   - Convert `BrokerTruthSnapshot` into runtime bootstrap input before runtime
     state is trusted.
   - Enforce target-symbol position truth and account-wide safety diagnostics.
   - Block startup on target active/unknown/orphan orders unless an explicit
     adoption policy exists.

4. M4-5 hybrid BO/MR orchestrator attachment under paper boundary
   - Attach the real ALOR-compatible hybrid BO/MR decision semantics to FINAM
     paper input.
   - Preserve closed-bar -> next-bar-open proxy semantics.
   - Keep actual FINAM sends disabled.

5. M4-6 riskgate ledger integration
   - Replace oracle-seeded riskgate projection with real ledger/state integration
     or keep a clearly named paper-only waiver.
   - Prove MR enabled/disabled decisions and rolling ledger values match ALOR
     semantics.

6. M4-7 runtime command consumer with paper/mock ACK
   - Consume strategy commands from the runtime stream.
   - Publish paper/mock ACKs using the broker-neutral ACK contract.
   - Prove request-id exactness, idempotency, DLQ behavior, and pending cleanup.

7. M4-8 durable identity chain
   - Persist `request_id -> client_order_id -> broker_order_id`.
   - Keep client-order-id collision checks and broker-order-id string semantics.
   - Ensure restart recovery cannot create duplicate sends.

8. M4-9 orders/trades/positions reconciliation loop
   - Reconcile broker truth with runtime pending state.
   - Distinguish target-symbol lifecycle truth from account-wide safety guards.
   - Preserve close-only/repair semantics for dirty starts.

9. M4-10 parity closure package
   - Bundle market-data parity, broker-truth bootstrap, runtime state parity,
     command/ACK paper parity, riskgate parity, and reconciliation evidence.
   - Only after this package is accepted should a separate runtime-live design
     gate be opened.

## Acceptance posture

The next review should be able to verify:

- source status clearly says one-shot operator actual exists, but continuous
  runtime-live remains disabled;
- ALOR runtime compatibility contract v1 is present;
- seeded paper parity is documented as a bridge, not an operational shortcut;
- current code passes formatting, tests, clippy, scanners, and Python compile
  checks;
- no order endpoint boundary has been expanded by this work.
