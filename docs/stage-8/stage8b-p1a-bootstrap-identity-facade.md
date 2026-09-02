# Stage 8B-P1-a bootstrap and identity facade

Status: implementation review candidate, 2026-09-02.

## Delivered boundary

P1-a adds a library-only, no-I/O-to-Redis composition facade around the
accepted Stage 7B durable owner. It provides:

- strict JSON bootstrap config with fixed FINAM-paper IMOEXF identity;
- exact operational-identity digest and identity-derived durable root;
- a fixed, separate P1 Redis namespace whose activation flags are false;
- source-produced zero-intent Stage 5G `TimerReady` first-boot authority;
- authenticated Stage 5G export and restore before durable-root creation;
- one-use explicit first-boot command;
- restart-only normal mode, with no missing-root creation or seed fallback;
- a separate 32-byte lifecycle commitment key loaded through the systemd
  credential directory with no symlink following;
- redacted first-boot receipt proving all live/broker/Redis surfaces remain
  detached.

There is deliberately no CLI, systemd unit, Redis client, M10 publisher,
consumer activation, provider or FINAM dependency in this facade. Those are
later reviewed P1 slices.

## Exact deployment identity

```text
broker_id       finam-paper
strategy_id     hybrid_imoexf
instrument      IMOEXF
venue_symbol    IMOEXF@RTSX
exchange        moex
market          futures
tick_size       0.5
instrument map  ba4e7b1190dc2686559b6d7c0df0185e96a10dfac6b13f6a582a349b14198558
```

Account ID, runtime config fingerprint, deployment ID/generation, gateway ID,
market-data generation, command-consumer generation and accepted writer issuer
public key are supplied by config and validated by the Stage 6 operational
identity boundary. The account ID is hashed in the receipt.

## First boot

The caller must explicitly consume an admin command carrying the exact phrase:

```text
CREATE_NEW_STAGE8B_P1_DURABLE_ROOT
```

The source must be produced by the accepted Stage 5C settlement and Stage 5G
initial `TimerReady` transition. Its exact shape is one callback, zero intents,
zero requests, no broker-truth replay ledger and no Redis/FINAM/broker flags.
Arbitrary JSON and the P0 projection are not accepted source types.

The facade validates the source and fresh runtime, exports and authenticates
the clean-restart package, restores it into the fresh runtime, and only then
creates the identity-derived 0700 directory. Stage 6 first-boot authorization
and Stage 7B ownership are subsequently consumed exactly once.

## Restart

Ordinary restart has no administrative command and no create flag. It opens
only the existing identity-derived directory and delegates to the accepted
Stage 7B restart path. The returned owner is accepted only when the
authenticated Stage 5 package repeats the exact P1 strategy, account,
instrument and runtime-config binding; those fields are deliberately outside
the Stage 6 operational-identity structure and therefore require this explicit
post-restore check. A missing root is an error and leaves the durable parent
unchanged.

## Commitment key

The key filename is exactly:

```text
stage8b-p1-lifecycle.key
```

It must be a 32-byte regular single-link file below an absolute canonical
`CREDENTIALS_DIRECTORY`, owned by root or the effective service UID, with no
group/other permissions. The loader uses `O_NOFOLLOW | O_CLOEXEC`, never logs
the bytes and zeroes its temporary buffer. It is unrelated to all Stage 8B
Generation-2 signing material.

## Review evidence

Targeted positive and negative tests cover:

- exact identity and common Redis hash tag without activation;
- wrong instrument identity and implicit first boot rejection;
- exact credential size/type/permissions and symlink rejection;
- missing-root restart without creation;
- source-produced first boot followed by authenticated normal restart;
- wrong-account restart rejection against the authenticated Stage 5 binding;
- initial Stage 5G checkpoint shape and clean-restart round-trip.

Operational P1 DB0 provisioning remains unauthorized. The existing P0
read-only stand is outside this new durable lifecycle namespace and is not
modified by P1-a.
