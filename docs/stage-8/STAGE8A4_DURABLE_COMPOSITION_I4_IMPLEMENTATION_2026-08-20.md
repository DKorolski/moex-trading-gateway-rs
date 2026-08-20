# Stage 8A-4 durable composition I4 implementation

## Authority

This controlled implementation follows the independently accepted I4 Design
R3 at `81727aae1f648f17961177fc9541e2483cbf07f2`. It opens only a read-only,
no-effect derived terminal ACK/current-readiness facade.

## Implemented chain

```text
complete exact Stage6 mixed replay + RequestFinalized
  + existing authenticated covering S1
  -> owner-issued Stage7bStage8a4TerminalAuthority
  -> FINAM-private timestamp-free ACK facts
  + freshly sampled trusted current readiness
  -> FINAM-private no-effect facade
```

`Stage7bStage8a4TerminalAuthority` is public only to cross the existing
`finam-gateway -> runtime-durable-service` dependency. Its fields are private,
it has no public constructor and it implements none of `Clone`, `Copy`,
`Debug`, `Serialize` or `Deserialize`. The external compile contract proves
that another crate can name/consume the value but cannot mint, clone, debug or
serialize it.

## Durable derivation

The Stage 6 extractor requires exactly one V2 batch, `Complete` deterministic
suffix coverage, exact transition (never a hold), durable F1 agreement and the
same mixed-replay final frontier. CANCEL `ExactWorking` is not terminal.

The sole Stage 7B issuer rereads/authenticates S1, refreshes replay without an
append, and requires S1 to already equal the current authenticated checkpoint.
It never calls `advance_recovery_seal`; lagging, absent, corrupt or replaced S1
fails closed and remains unmodified.

## ACK and readiness

ACK facts contain only durable request/client identity, optional mixed-replay
broker order ID, canonical status/reason and the existing Stage 7B
`terminal_request_ack_identity_sha256`. There is no timestamp and no second
ACK identity domain.

Current readiness independently pins accepted config/control files and binds
operational identity, runtime fingerprint, account, instrument, strategy,
strategy instance, authority root, accepted policy/config, source evidence,
`observed_at` and `valid_until`. Generic Ready requires current PaperReady,
RunAllowed, open/fresh broker sources, zero ambiguity and zero account/target
active orders. A readiness failure produces a redacted Blocked diagnostic but
does not alter the valid historical ACK.

## Closed effects

- no journal/V2/suffix/F1 append;
- no recovery-seal write, repair or advancement;
- no ACK/readiness publication;
- no Redis mutation, XADD, XACK or live consumer;
- no FINAM POST/DELETE/network send;
- no execution capability/operator arm/builder/dispatch;
- no runtime-live or real order.

Publication remains a future separately accepted slice.
