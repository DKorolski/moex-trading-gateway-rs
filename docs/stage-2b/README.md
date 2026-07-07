# Stage 2B patch log

Status: patch-log scaffold; no implementation in this document.

Stage 2B implementation plan is accepted in
[`../stage-2b-runtime-source-migration-implementation-plan.md`](../stage-2b-runtime-source-migration-implementation-plan.md).

All Stage 2B implementation patches must remain paper/mock/local and must keep
these boundaries closed:

- runtime-live;
- real FINAM command consumer;
- strategy-driven real FINAM orders;
- Stop/SLTP/bracket/replace/multi-leg live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `i64` surrogate adapter.

## Patch acceptance-note rule

Each Stage 2B implementation patch should add or update a short acceptance note
under this directory before handoff. The note should include:

- what changed;
- what did not change;
- tests added or preserved;
- unsupported live blockers that remain closed;
- evidence that no real FINAM send path was enabled.

## Next planned patch

`2B-1` is the foundation patch:

- broker-neutral runtime-facing id aliases/types;
- legacy numeric ALOR id -> decimal-string import helpers;
- string broker id preservation tests;
- no strategy behavior changes;
- no real FINAM endpoint calls.

Acceptance note:

[`2b-1-id-types-acceptance.md`](2b-1-id-types-acceptance.md)

`2B-1a` is the BrokerOrderId invariant hardening follow-up:

- remove `BrokerOrderId` from the generic unchecked string-id macro;
- keep empty broker-order ids unconstructible through serde and broker-input
  helpers;
- keep Stage 2B paper/mock/local and live send paths disabled;
- freeze the accepted Stage 0–13 macro-roadmap.

Acceptance note:

[`2b-1a-broker-order-id-hardening.md`](2b-1a-broker-order-id-hardening.md)

`2B-2` is the passive DTO/state migration patch:

- add passive runtime order/trade/bootstrap/state/ACK DTOs with
  broker-neutral ids;
- import old ALOR numeric order ids as decimal-string `BrokerOrderId`;
- keep FINAM/native string ids exact;
- reject empty/null broker ids at serde boundaries;
- reject zero/negative only for legacy numeric ALOR imports; native string ids
  like `"0"` / `"-1"` stay exact unless a later policy validator rejects them;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-2-passive-dto-state-migration.md`](2b-2-passive-dto-state-migration.md)

`2B-3` validates runtime order maps and bootstrap working-order maps:

- require map key == payload `order_id`;
- preserve legacy numeric map-key import as broker-order decimal strings;
- serialize new broker-order keys as exact strings;
- convert missing known order ids into readiness/manual-intervention blockers;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-3-runtime-state-order-map-validation.md`](2b-3-runtime-state-order-map-validation.md)

`2B-4` adds CommandAck / OrderEvent / TradeEvent lifecycle boundary contracts:

- ACK pending clearance is keyed by exact `StrategyRequestId`;
- matching `ClientOrderId` or `BrokerOrderId` cannot clear pending by itself;
- `Submitted`/`Accepted`/`Recovered` ACKs without broker id are explicitly
  marked as pending-broker-id;
- rejected/local-rejected style ACKs may omit broker id when lifecycle allows;
- order events classify active/terminal/unknown lifecycle without changing
  strategy behavior;
- duplicate broker order/trade events are classified idempotent at DTO level;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-4-command-ack-order-trade-lifecycle-boundary.md`](2b-4-command-ack-order-trade-lifecycle-boundary.md)

`2B-4a` hardens explicit ACK status policy:

- `Error` no longer clears pending by default;
- `Duplicate` requires prior known outcome and does not clear pending by
  itself;
- `Expired` clears only with explicit local no-send proof;
- `Timeout` / `UnknownPending` keep pending;
- `Rejected` with matching `StrategyRequestId` may still clear pending;
- `Submitted` / `Accepted` / `Recovered` without broker id still become
  pending-broker-id;
- keep Stage 2B paper/mock/local and live send paths disabled.

Acceptance note:

[`2b-4a-ack-status-policy-hardening.md`](2b-4a-ack-status-policy-hardening.md)
