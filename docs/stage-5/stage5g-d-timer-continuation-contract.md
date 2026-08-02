# Stage 5G-d — deterministic timer and continuation arbitration

Status: review candidate.  
Accepted predecessor: `d7561e6f36d01aea3d0dd67892800fbb6ac0a716`.

## Decision

Stage 5G-d reuses Stage 5C-k/l/m/n as the only timer callback and bounded
continuation authority. It adds a linear Stage 5G wrapper and a versioned
broker-package replay checkpoint. It does not add a scheduler, clock read,
thread, sleep, Redis, FINAM, HTTP, broker dispatch or runtime-live path.

Each public transition consumes one capability and one explicit timer or bar
input. Equal or reversed timer checkpoints fail before the Stage 5C callback.
The accepted Stage 5C settlement prevents one checkpoint from being consumed
by both a bar and a timer.

## Exact checkpoint

`Stage5gTimerCheckpointEnvelope` preserves:

- package schema discriminator;
- full `BrokerTruthSnapshot.received_ts`, including nanoseconds;
- exact evidence identity/fingerprint ledger;
- exact and millisecond broker-truth watermarks;
- duplicate-evidence count;
- last local `total_sequence`;
- last continuation checkpoint in milliseconds;
- SHA-256 over the complete canonical payload.

Restore validation derives the package discriminator and compatibility
millisecond watermark from the exact timestamp. Dropped nanoseconds, an empty
ledger, discriminator drift or payload mutation fail closed.

Package identity remains derived only from request/account and the exact
broker package receipt discriminator. `total_sequence` and payload fingerprint
are not identity inputs. Exact redelivery must carry a new increasing local
sequence and is classified idempotently. Changed payload under the same
identity is rejected. Distinct packages inside one millisecond remain distinct
through nanosecond receipt precision.

## Generated intents

Timer output with intents is retained in
`Stage5gTimerGeneratedIntentEscrow`. Its only ownership exit is the existing
`Stage5cSettledPaperStrategy` input accepted by Stage 5G-b. Therefore the path
remains:

```text
Stage 5G-d timer
  -> Stage 5C generated-intent escrow
  -> Stage 5G-b mock ACK
  -> Stage 5G-c broker-truth convergence
```

No transport-shaped request or direct send is exposed.

## Review boundary

This handoff closes only Stage 5G-d. Stage 5G-e deterministic full-runtime
restart and Stage 5G-f protective completion remain closed until independent
acceptance of this package. Their already accepted roadmap scope is unchanged.

