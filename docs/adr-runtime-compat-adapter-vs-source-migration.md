# ADR — runtime source migration vs ALOR-compatible adapter

Status: proposed decision for Stage 1B review.

Date: 2026-07-07.

## Context

The original runtime/gateway stack was ALOR-centered. FINAM order identifiers are
broker-native strings, while some legacy runtime paths historically assumed
numeric/order-id shapes. Before implementing a real runtime adapter, we must
choose whether to migrate runtime source code to broker-neutral identifiers or
hide FINAM identifiers behind an ALOR-compatible surrogate layer.

## Decision

Preferred path: migrate/adapt runtime source to broker-neutral contracts and use
`BrokerOrderId(String)` as the authoritative broker order id everywhere below
the runtime boundary.

Do not introduce a lossy `i64` surrogate for FINAM broker order ids in normal
runtime implementation.

## Rationale

- FINAM broker ids are string-native; preserving string ids avoids collisions,
  truncation, and irreversible mapping bugs.
- Broker-neutral runtime state should survive future broker changes without
  encoding ALOR id assumptions.
- Reconciliation requires exact request/client/broker id chains. A surrogate id
  adds another crash-recovery surface before we have parity evidence.
- The existing broker-core already models `BrokerOrderId(String)`.

## Rejected default alternative: binary-compatible surrogate adapter

An ALOR-compatible adapter that maps:

```text
FINAM broker_order_id string
  <-> local surrogate_order_id i64
  <-> legacy runtime state
```

is allowed only if runtime source migration proves impossible for a specific
legacy binary. If used, it requires a separate accepted ADR and all of the
following hard gates:

- durable crash-safe mapping store;
- bijective uniqueness checks on both ids;
- startup blocks on any unmapped broker id;
- no live readiness while mapping store is unavailable or inconsistent;
- reconciliation evidence for restart, duplicate send, cancel, and orphan trade
  scenarios.

## Consequences

- Stage 2 real runtime adapter implementation must target broker-neutral runtime
  source adaptation first.
- Existing ALOR runtime field semantics are preserved, but identifier types are
  made broker-neutral where needed.
- Compatibility fixtures must verify request-id, client-order-id, and
  broker-order-id string propagation.

## Current gate

Until this ADR is accepted by review, Stage 2 may continue design/fixtures only.
It must not start real runtime-live, `command-consumer-to-real-FINAM`, or
strategy-driven sends.
