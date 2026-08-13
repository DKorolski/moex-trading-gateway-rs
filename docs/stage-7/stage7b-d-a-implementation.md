# Stage 7B-d-a — durable lifecycle and covering-seal authority

Status: implementation R1 candidate; independent acceptance pending.

The original implementation candidate
`f71eeb926464f6634d485d5720b25c5e026b40d5` was not accepted. R1 is the
narrow review-closure patch for current on-disk seal authority and the B-046
during-effect crash witness; it does not open a new production surface.

Accepted design authority:

`00cead2989493b44e0d86ead29b95d57a7fbcbe2`

This slice remains paper/mock and Redis-production-free. It implements only
the d-a rows frozen by Design R1:

`B-043..B-051` and `B-054..B-056`.

`B-052/B-053` remain pending real-Redis restart proofs owned by d-c. d-b, d-c,
FINAM POST/DELETE, broker dispatch, runtime-live and real orders remain closed.

## Authority chain

The linear `Stage7bRecoveryReadyOwner` retains the file journal, recovered
Stage 6 runtime and kernel writer lease. It is the only d-a entry for:

1. source-bound paper command admission;
2. normalized paper outcome recording;
3. request finalization or replay finalization;
4. reconstruction of exact finalized facts from Stage 6 journal/replay;
5. recovery-seal advancement and terminal ACK authorization.

`Stage7bDurableAckAuthorized` is crate-private, non-Clone, non-Copy and
non-serializable. It has no public constructor and binds the exact operational identity, strategy
request, durable client identity, canonical command digest, broker order when
known, final disposition/record/sequence, Stage 6 checkpoint, seal generation,
seal commitment and canonical ACK fingerprint.

Before this authority is minted, d-a first rereads the current committed seal
from disk, verifies its canonical encoding, HMAC and operational identity, and
requires exact equality with the cached predecessor. It then replays the owned
file journal to obtain the actual current frontier. If the frontier advanced,
it advances the authenticated Stage 6 package without allowing Stage 5G or
operational-identity substitution, atomically commits the next seal, fsyncs the
root, rereads and authenticates the committed bytes, and checks that the seal
covers the current frontier. If the cached seal already covers the frontier,
the exact on-disk seal is reread again immediately before authority minting.
Deleted, corrupt and valid-but-different disk seals all fail closed; the code
never silently adopts or overwrites unexpected authority. A write or reread
failure puts the owner in fail-stop `SealCommitUncertain`; readiness is false
and no ACK authority can be minted.

## Crash and lifecycle proofs

The focused test set includes real subprocess SIGKILL barriers for:

- B-044: accepted durable, dispatch missing;
- B-045: dispatch durable, provider not called;
- B-046: a test-only provider effect witness is created exactly once, fsynced
  with its parent directory, and only then reaches the SIGKILL barrier while
  the outcome is still missing;
- B-047: outcome durable, finalization missing;
- B-048: finalization durable, covering seal missing;
- B-051: covering seal durable, transport settlement absent.

Restart either appends the one safe missing transition, holds for
reconciliation, or reconstructs finalization/canonical ACK entirely from the
journal. It never requires a process-memory ACK publication map.

B-050 uses a filesystem commit fault and proves that seal uncertainty blocks
authorization and readiness. B-054 uses an authenticated Stage 5G CANCEL slot
plus a finalized Stage 6 LIMIT PLACE with working broker-order identity; the
target and attribution are resolved from durable history rather than invented
by the caller.

## Deferred transport boundary

`classify_publication(None)` means the first recovered terminal ACK remains
canonical. `Duplicate` is selected only when a future d-b transport authority
provides the exact known canonical fingerprint; a different fingerprint is a
conflict. No Redis key, entry ID, group or marker enters Stage 6 execution
identity in d-a.

## Slice gate

The Stage 7B-d-a gate uses its exact production-scope checker and 32-case
mutation harness to keep Redis settlement, FINAM, broker dispatch and live
surfaces closed. It does not inherit the historical Stage 5C forbidden-surface
scanner: that scanner's immutable semantic-kernel inventory predates the
accepted Stage 5G/6/7 production files. Rebinding the old freeze would be a
separate governance change outside this slice.
