# ADR: Stage 5G-c R2-c-a R3 exact receipt-clock bracket authority

Status: review candidate

Base: `3d995af48e88588909e11505fdefc826ff8f66ce`

## Context

R2 correctly removed process wall clock from terminal settlement, but compared
the bracket timer's local receipt-clock milliseconds with broker component
source time truncated to seconds. A terminal event received after a subsecond
timer start could therefore be classified permanently as pre-timer evidence.

## Decision

R3 keeps the exact R2 transaction settlement and replaces only the validation
clock authority. Before terminal evidence is moved into the inherited R1
validator, R3 captures:

```text
evidence_decision_ms = BrokerTruth.received_ts.timestamp_millis()
```

The clock contract is:

- bracket timer origin: local runtime receipt clock, milliseconds;
- terminal grace decision: BrokerTruth package receipt clock, milliseconds;
- component source timestamps: economic evidence identity and chronology only.

R1 continues to prove exact request/order/trade/position correlation and
`component source <= component receipt <= BrokerTruth receipt`. R3 additionally
checks the exact package receipt watermark against ACK processed seconds using
a checked conversion, and against the bracket start in the same local receipt
clock domain.

The behavior is fail closed:

- receipt before timer: typed retryable `EvidenceBeforeBracketTimer`, original
  capability preserved;
- receipt inside the 3-second grace interval: exact broker position applied,
  timer preserved, honest `ReadyForTimer`;
- receipt at or after grace expiry: residual recovery Exit escrowed immediately.

Fresh snapshots may retain immutable component source timestamps while moving
the package receipt watermark forward. Such a snapshot can therefore unblock a
legitimate corrected retry instead of repeating a permanent false stale block.

## Consequences

R3 is two additive marker/digest-pinned Stage 5C regions over the exact rejected
R2 snapshot. It adds no normalized public API and no serialization, Redis,
FINAM transport, HTTP POST/DELETE, broker dispatch or live authority.
R2-c-b, Stage 5G-d, Stage 6, main merge and deployment remain closed.
