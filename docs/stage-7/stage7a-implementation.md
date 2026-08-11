# Stage 7A-R1 implementation candidate

Accepted predecessor: `10e357825a701193d964975bb5769bd0745d4986`.

Normative inputs:

- [technical specification](TZ_STAGE7A_REDIS_COMMAND_CONSUMER_PAPER_MOCK_2026-08-11.md)
- [52-row acceptance matrix](STAGE7A_ACCEPTANCE_MATRIX_2026-08-11.csv)

## Implemented boundary

`runtime-command-bridge` is a broker-neutral paper-only crate. Its dependency
graph contains `broker-core`, `strategy-runtime-core`, Redis and local utility
crates. It has no `broker-finam`, `finam-gateway`, `reqwest`, SQL order-path or
broker endpoint dependency.

The processing chain is:

```text
paper Envelope<BrokerCommand>
  -> XREADGROUP or bounded cursor-correct XAUTOCLAIM
  -> one typed handler
  -> trusted strategy/instrument profile
  -> Stage 6 command-admission facade
  -> RequestAccepted + DispatchAttemptRecorded
  -> deterministic process-local paper provider
  -> Stage 6 normalized outcome
  -> Envelope<CommandAck>
  -> ACK XADD
  -> XACK
```

Malformed input follows `redacted DLQ XADD -> XACK`. Raw command bytes never
enter DLQ; only payload length, domain-separated SHA-256, safe reason and Redis
entry metadata are emitted.

## Authority decisions

- Redis entry ID, group, consumer and delivery count are transport metadata.
- Stage 6 remains the sole lifecycle authority.
- PLACE attribution is parsed from the canonical source comment and checked
  against a local account/strategy/instrument profile.
- CANCEL attribution is resolved from the Stage 6-correlated target paper
  order; configuration cannot invent a cycle/owner for an unrelated order.
- Each new request starts its own Stage 6 lifecycle sequence at 1.
- Exact duplicate delivery never appends a second `RequestAccepted` or invokes
  a second paper effect.
- A conflicting duplicate, unresolved prior lifecycle or uncertain effect is
  retained pending and degrades command-path readiness.
- Within one Stage 7A authority lifetime, ACK XADD failure republishes the
  canonical terminal ACK; ACK XADD success followed by command-XACK failure
  emits `Duplicate/DuplicateCommand` with exact request/client/broker identity.
  This matches the accepted Stage 5G ACK lifecycle. No cross-process
  exactly-once claim is made.
- `DispatchForbidden` is not treated as lifecycle terminality. A working,
  filled or reconciled-but-nonfinal PLACE blocks another PLACE. Only one
  source-correlated CANCEL may overlap its known target order.
- Source-read and claim health cannot heal ACK/DLQ/Stage-6 settlement state.
  Blocked state is keyed by exact Redis entry and request where available.
- XAUTOCLAIM retains its returned cursor across bounded calls and resets only
  after Redis reports a completed scan.
- `Beginning` group attachment is rejected unless controlled replay is
  explicitly authorized. Default attachment is `Tail`.
- All local observation/receipt timestamps are minted inside the host bridge;
  envelope/source timestamps are not local temporal authority.

## Tested crash and failure windows

- accepted before dispatch;
- dispatch before paper provider;
- uncertain provider result;
- ACK publish failure;
- DLQ publish failure;
- ACK published before XACK;
- stale pending ownership reclaimed by XAUTOCLAIM;
- exact Redis redelivery under a different entry ID;
- consumer Redis failure, stale polling and explicit stop readiness.

The real-Redis test starts an isolated local `redis-server` with persistence
disabled and exercises `XGROUP`, `XREADGROUP`, `XPENDING`, `XAUTOCLAIM`, `XADD`
and `XACK`. It does not contact FINAM or any external broker endpoint.

The R1 gate emits a row-by-row JSON report for all 52 blocking acceptance
rows. Every PASS is bound to one or more concrete saved gate logs and required
tokens; inventory count alone cannot close the stage.

## Deliberately deferred to Stage 7B+

- file-backed production Stage 6 composition;
- writer lock and fsync production policy;
- cross-process exactly-once;
- continuous broker-truth reconciliation;
- FINAM POST/DELETE or any broker network dispatch;
- runtime-live and real strategy orders;
- native Stop/SLTP/bracket/replace/multi-leg.

Stage 7A acceptance may open only Stage 7B. It cannot directly authorize
Stage 8 or live execution.
