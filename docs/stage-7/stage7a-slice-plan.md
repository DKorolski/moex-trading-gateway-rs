# Stage 7A — Redis paper/mock command consumer

Baseline: `10e357825a701193d964975bb5769bd0745d4986`.

Status: implementation candidate. Stage 6 is independently accepted and
closed. Stage 7B and Stage 8+ remain closed.

## Boundary

The Stage 7A bridge consumes `Envelope<BrokerCommand>` from a validated paper
Redis namespace. Both new delivery (`XREADGROUP`) and stale-pending recovery
(`XAUTOCLAIM`) enter one canonical handler. Redis entry IDs and consumer names
are transport metadata only; `StrategyRequestId` and the Stage 6 durable
`ClientOrderId` remain execution identity.

Stage 6 is the sole lifecycle authority. The paper outcome provider can be
invoked only after durable `RequestAccepted` and `DispatchAttemptRecorded`
facts exist. Stage 7A has no FINAM, HTTP order endpoint, runtime-live or native
protective-order dependency.

## Initial safety policy

- At most one unresolved execution lifecycle per accepted strategy instance.
- Exact redelivery enters Stage 6 replay/dedupe; conflicting identity fails
  closed and is not converted to poison-message DLQ success.
- `XACK` follows successful ACK publication, or successful redacted DLQ
  publication for permanent poison input. Uncertainty remains pending.
- ACK publication is at-least-once; exact repeated ACK application must be
  idempotent.
- Local collection/receipt/validation times are minted by the trusted host or
  provider. Redis and broker timestamps are data only.

## Work slices

1. Narrow Stage 6 command-admission facade and authority tests.
2. New `runtime-command-bridge` crate with paper-only dependency graph.
3. Canonical command classification and deterministic paper outcomes.
4. Redis group attach, `XREADGROUP`, cursor-correct bounded `XAUTOCLAIM`, ACK,
   DLQ and `XACK` settlement.
5. Consumer supervision/readiness and deterministic fault matrix.
6. Real Redis integration, negative/closed-surface gates and immutable
   handoff.

Slices 1–5 and the implementation portion of slice 6 are complete. Independent
review is still required before Stage 7A can be marked accepted/closed.

## Exit

Independent review must accept all 52 blocking rows in the Stage 7A
acceptance matrix. Acceptance opens only Stage 7B production durability
composition; it does not authorize a real FINAM effect.
