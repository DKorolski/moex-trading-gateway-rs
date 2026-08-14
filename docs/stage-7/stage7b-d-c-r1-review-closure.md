# Stage 7B-d-c-R1 review closure

Status: implementation candidate; independent acceptance pending.

Rejected predecessor:
`c427ad1c83a27e6a80f45c7e09311ffcae26c913`.

Accepted Stage 7B-d-b predecessor:
`e0bf9b7d9eb209e19b875f199511a493ddcd0da9`.

R1 is intentionally limited to the three P1 findings from the independent
Stage 7B-d-c review. It does not open Stage 7B-e or any FINAM/live surface.

## Functional closure

Accepted Stage 7A deterministic rejections are restored:

- expired -> `Expired / ExpiredCommand`;
- unsupported shape -> `Rejected / FeatureDisabled`;
- new profile mismatch -> `Rejected / LocalValidationRejected`;
- established-identity profile mismatch -> pending `IdentityConflict`.

An opaque `Stage7aDeterministicRejectionEvidence` is minted only by the
canonical Stage 7A classifier. The Stage 7B owner observes the exact Stage 6
checkpoint and request-index state before classification, rereads them before
settlement and authorizes ACK only when the request remains absent and the
checkpoint remains unchanged. The existing d-b Lua primitive performs the
atomic ACK publication plus source XACK. No poison/DLQ path is used.

Real Redis tests prove zero provider calls, byte-identical Stage 6 journal,
empty PEL, exact status/reason and `stage6_mutation=false`. Exact rejection
redelivery after owner restart uses the request marker and publishes as a
duplicate without a Stage 6 effect.

## B-066 closure

A real recovered owner, temporary Redis, settlement backend and supervised
`Stage7bRedisService` are run together. The witness polls until actual
`PaperReady` with fresh source/claim scans, valid storage/seal, healthy
settlement and zero pending/blocked entries. Aborting and awaiting the task
immediately yields `Stopped / ConsumerNotAlive`.

## B-068 closure

The parent process starts Redis, creates a group and old PEL owned by a dead
consumer. A new child process constructs its own UUID boot identity, starts
the process-local cursor at `0-0` and executes paged `XAUTOCLAIM`. The parent
verifies exact reclaimed IDs, ownership transfer to the child and final cursor
reset to `0-0`.

## Enforcement

The d-c negative inventory is expanded from 25 to 33 cases. New mutations
cover deterministic rejection blocking, classifier removal, owner-authority
bypass, false Stage 6 mutation claims, established-conflict settlement and
removal of each integrated B-066/B-068 witness.

Still closed: FINAM POST/DELETE, broker network dispatch, runtime-live, real
orders, protective live orders and Stage 7B-e aggregate closure.
