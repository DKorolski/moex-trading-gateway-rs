# Stage 5G-d — deterministic timer and continuation arbitration

Status: R1-b R1 review candidate.
Accepted predecessor: `7724b4472d603b3c2ef7c3ff22aa371aa64d8592`.
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

Package identity remains derived only from request/account and the exact
broker package receipt discriminator. `total_sequence` and payload fingerprint
are not identity inputs. Exact redelivery must carry a new increasing local
sequence and is classified idempotently. Changed payload under the same
identity is rejected. Distinct packages inside one millisecond remain distinct
through nanosecond receipt precision.

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
still execute only through the accepted Stage 5C authority. Every following checkpoint is the maximum of the prior continuation, the
exact broker receipt compatibility watermark and the accepted bar checkpoint.

Timer-generated ACK receipt and subsequent BrokerTruth receipt are rejected
before mutation when their exact millisecond receipt precedes the inherited
continuation checkpoint. Retry retains the complete timer-owned wrapper.

## Review boundary

This handoff closes only Stage 5G-d. Stage 5G-e deterministic full-runtime
restart and Stage 5G-f protective completion remain closed until independent
acceptance of this package. Their already accepted roadmap scope is unchanged.
