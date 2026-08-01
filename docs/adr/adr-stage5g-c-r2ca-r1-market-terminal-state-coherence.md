# ADR: Stage 5G-c R2-c-a R1 MARKET terminal state coherence

Status: review candidate

Base: `581f4f6021dd781e7a5db9177be05feb7d94b12a`

Accepted authority predecessor: `c6ae2bdaea2575dd41e6da00acad5c231f3c7572`

## Context

The first R2-c-a implementation validated terminal MARKET broker truth but
moved the strategy forward unchanged. A zero-fill terminal order therefore
left a stale pending request, while a partial fill left stale position state
and discarded the recovery work that the mature runtime would emit. Returning
the existing broker-lifecycle-resolved type in either case falsely implied that
the timer path was ready.

## Decision

Validation and mutation are separate, linear phases.

`validate_stage5c_market_terminal_outcome` consumes the original resolved
capability only after validating exact request, canonical ACK, broker/client
order identity, account, instrument, side, quantity, attribution, terminal
order status, correlated trades, aggregate target position and monotonic
timestamps. It returns the crate-private, non-serializable, non-cloneable
`Stage5cValidatedMarketTerminalOutcome`. Validation failure returns the exact
original resolved capability for corrected reconciliation.

`settle_stage5c_validated_market_terminal_outcome` consumes that capability and
reuses the existing broker-neutral Hybrid runtime ACK/position callbacks. No
state is mutated before validation succeeds. It returns the existing
`Stage5cBrokerLifecycleSettlement` directly: coherent outcomes become
`ReadyForTimer`, while generated recovery work becomes `GeneratedIntentBatch`.

The policy follows the ALOR-compatible runtime behavior already implemented by
`HybridIntradayRuntimeStrategy`:

- zero-fill Entry Rejected/Canceled/Expired is represented by a terminal ACK;
  the original pending Entry and active cycle are cleared, position remains
  unchanged and non-window failure enters close-only safe mode;
- zero-fill Exit clears the original pending Exit, preserves the exact open
  position and active cycle, and does not invent an immediate blind retry;
- partial Entry/Exit first terminalizes the original request and then applies
  the exact broker position as a fresh reconciliation event; the mature runtime
  emits a residual emergency exit, which is retained in the existing generated
  intent escrow;
- a fully filled terminal snapshot applies exact position before the terminal
  ACK so a completed fill is not reclassified as an unexplained residual.

Accepted and Confirmed runtime ACKs are eligible. Broker Core's frozen mapping
continues to map source Submitted and Recovered statuses to Confirmed; the
existing Stage 5G-b Submitted→Recovered production witness remains pinned and
is executed by the R1 gate.

## Type-state and chronology

The validated capability has no timer interface. A settled zero-fill outcome
may proceed to the existing timer path only after the original pending ID is
gone and position state is coherent. A partial outcome retains a generated
intent batch, so the timer rejects it until that batch is settled.

The broker-truth receipt cannot precede ACK processing. Every order, trade and
position component must satisfy source time ≤ receipt time ≤ snapshot receipt;
terminal positive-fill evidence cannot precede the ACK. The resulting
lifecycle watermark is the validated broker-truth receipt and cannot precede
the ACK.

## Consequences

The normalized public API is unchanged. No new broker DTO, direct state write,
transport, dispatch or live authority is introduced. Stage 5G-c R2-c-b,
Stage 5G-d, Redis live consumers/groups, FINAM transport, HTTP POST/DELETE,
broker execution, runtime-live, real orders, Stage 6, main merge and deployment
remain closed.
