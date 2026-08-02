# Stage 5G-d — deterministic timer and continuation arbitration

Status: R1-b R4 review candidate.
Accepted predecessor: `5fcc538a9bed574cdd9df268a9bb1368c608e11e`.
Accepted Stage 5C callback authority: `d0494537d7c1739a16350b2d28f71b304165c812`.

## Decision

Stage 5G-d reuses Stage 5C-k/l/m/n as the only timer callback and bounded
continuation authority. It adds a linear Stage 5G wrapper and a versioned
broker-package replay checkpoint. It does not add a scheduler, clock read,
thread, sleep, Redis, FINAM, HTTP, broker dispatch or runtime-live path.

Each public transition consumes one capability and one explicit timer or bar
input. Equal or reversed timer checkpoints fail before the Stage 5C callback.
The accepted Stage 5C settlement prevents one checkpoint from being consumed
by both a bar and a timer.

The only Stage 5C bar entry point used by Stage 5G-d is
`advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint`.
The wrapper passes its exact incoming continuation checkpoint, verifies it is
not older than the inner Stage 5C settlement checkpoint, and derives bar
evaluation time from the accepted bar checkpoint itself.

## Exact checkpoint

`Stage5gTimerCheckpointEnvelope` preserves:

- package schema discriminator;
- full `BrokerTruthSnapshot.received_ts`, including nanoseconds;
- exact evidence identity/fingerprint ledger;
- exact source-owned identity of the current broker package;
- exact and millisecond broker-truth watermarks;
- duplicate-evidence count;
- last local `total_sequence`;
- last continuation checkpoint in milliseconds;
- SHA-256 over the complete canonical payload.

Restore validation derives the package discriminator and compatibility
millisecond watermark from the exact timestamp. Mandatory exact receipt,
sequence and continuation fields, unique replay identities, valid SHA-256
fingerprints, current-package membership and continuation chronology are
validated independently of the payload hash. A recomputed hash cannot admit a
semantically incomplete checkpoint.

Validation parses every ledger identity at full nanosecond precision and
requires nondecreasing receipt chronology. The current evidence identity must
be the final/latest ledger entry; its exact receipt, package discriminator and
millisecond compatibility projection must describe that same package. Thus a
valid checksum cannot admit a stale current package, reversed ledger or a
current receipt projection regressed to an earlier package.

Package identity remains derived only from request/account and the exact
broker package receipt discriminator. `total_sequence` and payload fingerprint
are not identity inputs. Exact redelivery must carry a new increasing local
sequence and is classified idempotently. Changed payload under the same
identity is rejected. Distinct packages inside one millisecond remain distinct
through nanosecond receipt precision.

Post-restore classification checks an exact known identity before applying
new-package chronology. Historical exact redelivery is therefore idempotent.
A new identity is rejected if its BrokerTruth receipt millisecond predates the
inherited continuation checkpoint, then remains subject to the existing exact
last-BrokerTruth receipt regression guard before ledger append.

## Canonical evidence authority

Active Stage 5G-c and post-checkpoint Stage 5G-d use the same crate-private,
pure canonicalization function before identity/fingerprint classification. Its
opaque owned result is the only value from which these paths obtain a replay
fingerprint. Exact duplicate trade IDs collapse by immutable payload while the
newest observation receipt is retained; conflicting immutable payloads fail
before ledger mutation. Other vector-shaped broker truth is canonically sorted.

Immutable trade equality is defined by the versioned
`Stage5gCanonicalImmutableTradePayloadV1` projection. It binds the exact
structural `InstrumentId` and every broker/economic/source field except the
observation-only `received_ts`. Duplicate merge retains the complete row with
the maximum receipt, so neither vector order nor the first row can select a
different canonical representation. The same projection and merge authority
protect both raw snapshot deduplication and the committed trade ledger.

`NewPackage` retains the canonical evidence candidate in its result ownership;
`ExactReplay` does not create a candidate. This is a type-level boundary for a
future Stage 5G-e consumer, not an authorization to open Stage 5G-e.

The checkpoint returned for `ExactReplay` is committed immediately. For
`NewPackage`, the returned checkpoint is a candidate only; it must not be
persisted until the exact owned canonical candidate has successfully completed
the active Stage 5G-c transition. Stage 5G-e must enforce that distinction with
type-state before it is opened.

Version-3 replay identities require canonical UUID text and a nonempty,
colon-free account ID. The package suffix remains the exact nanosecond
version-1 BrokerTruth receipt discriminator. Any encoding change requires a
new identity schema version.

## Generated intents

Timer or bar output with intents is retained in
`Stage5gTimerGeneratedIntentEscrow`. Its only public ownership exit is the
Stage 5G-d wrapper transition that attaches Stage 5G-b. Raw Stage 5C settled
state is not exposed. Therefore the path remains:

```text
Stage 5G-d timer
  -> Stage 5C generated-intent escrow
  -> Stage 5G-b mock ACK
  -> Stage 5G-c broker-truth convergence
```

No transport-shaped request or direct send is exposed.

Zero-intent bar output is re-armed into `Stage5gTimerReadyPaperStrategy`; the
same linear value can consume exactly one later timer or bar. Re-arming is a
crate-private no-callback Stage 5C type-state bridge; all strategy callbacks
still execute only through the accepted Stage 5C authority. Every following
checkpoint is the maximum of the prior continuation, the exact broker receipt
compatibility watermark and the accepted bar checkpoint.

Timer-generated ACK receipt and subsequent BrokerTruth receipt are rejected
before mutation when their exact millisecond receipt precedes the inherited
continuation checkpoint. Retry retains the complete timer-owned wrapper.

## Review boundary

This handoff closes only Stage 5G-d. Stage 5G-e deterministic full-runtime
restart and Stage 5G-f protective completion remain closed until independent
acceptance of this package. Their already accepted roadmap scope is unchanged.
