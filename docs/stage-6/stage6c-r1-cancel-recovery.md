# Stage 6C-R1 — cancel recovery and action/event safety

Stage 6C-R1 is a narrow correction to the pure deterministic replay state
machine. Its predecessor is `a4e55c42aac6d2470d6ab874c61c19be1b771b3f`.

## Frozen behavior

- The first unique cancel outcome is authoritative.
- A later unique cancel outcome returns typed `CancelOutcomeConflict`.
- Exact duplicate bytes remain idempotent before transition handling.
- `ExecutionObserved` remains visible after explicit finalization.
- `NoBrokerOrderFound` and `BrokerOrderFound` reconciliation are Place-only.
- Generic broker-order and broker-trade observations are Place-only.
- Cancel may retain `Inconclusive` while reconciliation is required.
- Cancel cannot become `RetryEligibleSameIdentity`.
- Place same-identity retry behavior is unchanged.

The context-free action/event matrix is checked by the record constructor and
decode authority. Replay repeats the matrix check before any state mutation so
future internal callers cannot bypass the lifecycle rule.

## Compatibility

Stage 6A request bytes, Stage 6B storage schema/backend and all accepted Stage
6A/6B goldens remain byte-identical. The replay fingerprint domain remains
`stage6-replay-snapshot-v1`; the accepted golden scenario does not contain a
corrected cancel transition and therefore its fingerprint is unchanged.

## Closed scope and Stage 6D carry-forward

R1 adds no I/O and opens no execution surface. Stage 6D remains closed pending
independent R1 acceptance. Stage 6D must later introduce typed broker-truth
issuance and authenticated boot/frontier authority before any dispatch-safety
fact can influence integration code.
