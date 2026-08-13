# Stage 7B-d-b — atomic Redis ACK/DLQ settlement

Status: implementation candidate for independent review.

Accepted predecessor: Stage 7B-d-a-R1 at
`8418cfb63ecee6702bf8a2873592b7cad1e711ee`.

This slice implements only frozen rows `B-057..B-063`. It adds an isolated
paper-namespace Redis settlement backend but does not attach a command consumer,
claim/restart loop, runtime-live path or FINAM transport. Stage 7B-d-c remains
closed and still owns `B-052/B-053` plus `B-064..B-070`.

## Atomic primitive

ACK and redacted poison DLQ share one Lua primitive. The script validates the
source/output/marker key types, marker schema and conflicts, request-level
canonical publication identity, and exact source PEL membership before its
first mutation. One atomic execution then performs exactly one output `XADD`,
persists the entry marker (and the canonical request marker for a first ACK),
and performs `XACK`.

The per-entry marker key is stable over:

`paper hash tag + source stream + consumer group + Redis entry ID + kind`.

It deliberately excludes the payload fingerprint. The marker value stores the
selected exact payload fingerprint, output stream/ID and canonical/duplicate
classification. An exact committed retry returns that marker result without a
second publication and without requiring the entry to remain in the PEL. A
conflicting marker or request-level canonical fingerprint fails before any
publication or acknowledgement.

All keys use one validated `{hash-tag}` and the isolated
`finam_imoexf_paper:` namespace. The guarantee covers a committed script whose
client loses the response; it does not claim protection against Redis
storage/failover rollback.

## Authority boundary

The recovery owner consumes the accepted d-a `Stage7bDurableAckAuthorized`
directly into a private ACK plan. The plan adds only transport identity; none
of its stream/group/entry/marker facts can become Stage 6 execution identity.

Permanent pre-Stage6 poison has a separate non-serializable authority. It is
bound to the exact entry context, permanent reason, redacted payload SHA-256,
payload length and an unchanged Stage 6 checkpoint. The owner rereads the
authenticated disk seal and durable frontier both when observing and when
settling poison. Any intervening Stage 6 mutation or payload drift blocks the
DLQ path. Raw payload bytes are never published.

There is no settlement entry for IdentityConflict, ConflictingDuplicate,
ReconciliationRequired, RecoveryBlocked, durability uncertainty or provider
uncertainty. Such states remain pending for the future d-c supervised loop.

## Evidence

Real isolated `redis-server` tests prove:

- `B-057`: owner-mediated atomic ACK output/marker/XACK;
- `B-058`: stable transport identity excludes payload fingerprints and shares
  one Redis hash slot;
- `B-059`: injected response loss after commit retries without a second XADD;
- `B-060`: pre-commit wrong-type failure leaves the source entry pending;
- `B-061`: redacted, checkpoint-bound atomic poison DLQ/XACK;
- `B-062`: only finalized ACK authority reaches the owner-mediated ACK path;
- `B-063`: backend health stays degraded after failure and becomes healthy
  only after an exact successful retry clears the pending entry.

The checker and mutation harness pin the Lua ordering, marker identities,
linear capability separation, row counts and closed surfaces. The gate runs
debug/release focused Redis tests, workspace tests/doctests, strict clippy and
formatting.

## Still closed

- Redis command-consumer attachment, `XAUTOCLAIM` and restart cursor handling;
- composite PaperReady/readiness publication and task supervision;
- `B-052/B-053`, `B-064..B-070`, Stage 7B-d-c and Stage 7B-e;
- FINAM POST/DELETE, broker dispatch, runtime-live and real orders.
