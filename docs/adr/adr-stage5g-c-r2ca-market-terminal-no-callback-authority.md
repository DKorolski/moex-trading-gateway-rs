# ADR: Stage 5G-c R2-c-a MARKET terminal no-callback authority

Status: rejected as submitted; superseded by
`adr-stage5g-c-r2ca-r1-market-terminal-state-coherence.md`

Base: `e6e761519d43be2c2f08632c6559f7b4bb0ea533`

This document is retained as the immutable design record for commit
`581f4f6021dd781e7a5db9177be05feb7d94b12a`. Independent review found that its
no-callback completion returned a false timer-ready type-state while leaving
terminal MARKET pending/position state unresolved. It is not current
operational authority.

## Context

An accepted MARKET ACK can later be reported by broker truth as Rejected,
Canceled, or Expired. Stage 5C-j only accepts a Position event for MARKET, so a
zero-fill terminal order (and a canceled/expired partial fill) had no bounded
way to complete without fabricating a position transition.

## Decision

Use the allowed alternative: one crate-private, non-serializable, single-consume
Stage 5C completion facade. It consumes `Stage5cResolvedPaperIntentBatchStrategy`
and canonical `BrokerTruthSnapshot` evidence and returns the existing
`Stage5cBrokerLifecycleResolvedPaperStrategy` type-state.

The facade validates before consuming the retry capability:

- exactly one accepted MARKET source intent and matching ACK;
- exact request, broker-order and derived client-order IDs;
- exact account, instrument, side, quantity and source attribution;
- terminal Market order status and coherent lifecycle timestamps;
- Rejected only with zero fill;
- Canceled/Expired with zero fill or with exact correlated trades and aggregate
  target position equal to pre-position plus signed fill;
- duplicate trade IDs and contradictory order/trade/position evidence fail
  closed.

It does not invoke any strategy callback. Strategy state is moved unchanged,
generated intents remain empty, and a validation failure returns the original
resolved capability for reconciliation retry. Linear Rust ownership prevents a
successful capability from being completed twice.

## Consequences

The normal Stage 5C-j event path and normalized public API remain unchanged.
Stage 5G order/position behavior is not modified in R2-c-a. R2-c-b may call this
authority only after independent acceptance of this commit.

Redis, FINAM transport, HTTP POST/DELETE, broker dispatch/execution,
runtime-live, real orders, Stage 5G-d, Stage 6, main merge and deployment remain
closed.
