# Current status — FINAM migration / ALOR parity

Status date: 2026-07-07.

This document is the operator/developer status source of truth. It intentionally
separates what already exists from what is still forbidden for continuous
runtime-live.

## What exists

- Broker-neutral core contracts for orders, trades, positions, market data,
  readiness, broker truth, runtime host lifecycle, and paper runtime state.
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
- Stage 2B is planning-only until
  `docs/stage-2b-runtime-source-migration-implementation-plan.md` is accepted;
  implementation remains blocked.

Green / mostly closed:

- FINAM WS live market-data reaches Redis.
- Fresh M1 final bars can produce canonical M10 runtime input.
- FINAM paper runtime state can now match ALOR IMOEXF hybrid state on the active
  M10 bar after ALOR-oracle seeding.
- ALOR-oracle seed now preserves pending/deferred/safe-mode/protective-state and
  dirty-start/manual-intervention placeholders as explicit paper parity fields.
- `seed_required=true` can hard-block a parity run when the ALOR oracle seed is
  missing or cannot be parsed.
- Safety flags remain closed in paper state:
  `live_orders_enabled=false`, `runtime_live_ready_enabled=false`,
  `command_consumer_to_real_finam_enabled=false`,
  `external_order_endpoint_enabled=false`, `stop_sltp_bracket_enabled=false`.

Amber:

- Full-session FINAM-vs-ALOR M10 parity evidence is still required.
- Broker-truth snapshots are available, but broker truth is not yet mandatory
  runtime bootstrap input.
- Paper runtime projection has ALOR-compatible fields, but it is not yet the
  real ALOR hybrid BO/MR orchestrator.
- Riskgate state can be seeded/projected, but true riskgate ledger integration
  is not complete.
- Stage 2B implementation still requires a separate accepted implementation
  plan and fixture-backed parity tests.

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
3. Full-session FINAM M10 vs ALOR M10 report accepted.
4. Broker truth bootstrap wired into runtime lifecycle.
5. Real hybrid BO/MR/riskgate semantics attached behind paper boundary.
6. Request-id/client-order-id/broker-order-id durable chain implemented.
7. Runtime command consumer proven in paper/mock ACK mode.
8. Orders/trades/positions reconciliation loop accepted.

Only after these gates should `command-consumer-to-real-FINAM` or
runtime-driven live micro be discussed.
