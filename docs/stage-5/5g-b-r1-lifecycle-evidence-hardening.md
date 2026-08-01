# Stage 5G-b R1 — lifecycle evidence hardening

Status: implementation review candidate.

## Scope

R1 repairs the independently reproduced fingerprint, no-send, duplicate,
source-authority and reproducibility gaps in Stage 5G-b. It does not open
Stage 5G-c or any broker/runtime transport surface.

The immutable implementation base is
`b6f4194769ce0f6c00a82361eba57dc3ed07e55c`. The accepted Stage 5G-a design
authority remains
`011fd4b7baaa41fffdad7d3c28e463b7977f5989` and is re-executed from a detached
snapshot by `scripts/stage5g_a_snapshot_gate.sh` (54 entry cases and 30/30
negative mutations).

## Deterministic ownership split

The implementation has one ACK algorithm and two ownership layers:

```text
Stage5gMockAckState
  deterministic admission / ordering / correlation / evidence only
  no callback, no transport, no broker truth
             |
             v
Stage5gMockAckSession
  exact linear Stage5cSettledPaperStrategy ownership
             |
             v
existing frozen Stage 5C-i resolver, once and only after a complete vector
```

Production always enters through `attach_stage5g_mock_ack_session`; therefore
the pure state cannot create a runtime capability. The focused R1 tests use the
same internal state machine with fixed timestamps and the accepted Stage 5F
IMOEXF High180/LB120 market configuration. They deliberately do not invoke a
Stage 5C callback. This avoids changing the frozen Stage 5C wall-clock facade
and avoids disguising a wall-clock read as deterministic evidence.

The production wrapper still owns and consumes the real Stage 5C linear
capability. No test-only constructor for `Stage5cSettledPaperStrategy` was
added.

## Fingerprint schema v2

Every lifecycle slot binds:

- exact request and client identities;
- intent class, action, side and source timestamp;
- state, status and coherent reason code;
- exact nanosecond RFC3339 ACK receive timestamp;
- canonical total sequence;
- Broker Core pending disposition and status policy;
- a domain-separated SHA-256 of exact broker order ID bytes.

Raw broker IDs are not exported in summaries. The transition fingerprint binds
the ordered canonical ACK projection as well as pre/post lifecycle state.

The accepted deterministic Stage 5F market evidence hash is:

```text
f03a86a0f9f9e6c64b2a3c6bdabb4a3af86eac5674e75859ad8e13f4cf491308
```

Debug and release focused tests must reproduce the same value.

## No-send and duplicate rules

`ExpiredCommand` is accepted as no-send proof only when no broker ID has ever
been observed and the current ACK also has no broker ID. Any contradiction
retains the session in `ManualInterventionRequired`, returns
`NoSendProofContradictsBrokerIdentity`, and does not enter Stage 5C.

A duplicate is an idempotent no-op only with exact request ID, exact client ID,
exact broker ID continuity, `DuplicateCommand`, and a monotonic total sequence.
Missing broker ID cannot match a prior exact broker ID.

## Current source authority

Variant A from the remediation assignment is selected. Only accepted Stage 5F
market intents are admitted. Public Limit and Cancel bindings return
`NotYetSourceAuthenticated`. No trusted Stage 5C Limit/Cancel projection is
invented in this patch.

## Closed boundary

R1 adds no Redis consumer, FINAM transport, HTTP POST/DELETE, broker dispatch,
order/trade/position application, runtime-live, real order or protective
execution. Stage 5G-c remains blocked pending independent R1 acceptance.
