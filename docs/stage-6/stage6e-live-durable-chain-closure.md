# Stage 6E — final durable-chain closure

Stage 6E turns the accepted Stage 6A–6D paper chain into the final authority
contract that future Stage 7 code may consume. It adds no Redis consumer,
FINAM transport, network dispatch, runtime-live or real-order capability.

## Restart authority

`Stage6dDurableRuntimeRecovered` is issued on restart only after all of these
checks succeed:

```text
authenticated Stage 5G restore
→ existing validated Stage 6 journal
→ authenticated checkpoint/prefix validation
→ complete Stage 6C replay
→ current Stage 5 ↔ Stage 6 semantic cross-binding
→ integration fingerprint v2
```

The cross-binding covers request ID, durable client ID, account, instrument,
the source-produced Stage 5 attribution fingerprint and strategy definition,
Place/Cancel action, and the Cancel broker/client target when present. Extra
Stage 6 requests are admitted only when explicitly finalized; an unmatched
effect-capable request fails the restart.

## Fresh broker-truth authority

Raw fixture-shaped truth cannot enter the production application function.
The process-local paper path is:

```text
Stage6ePaperFreshBrokerTruthInput
→ exact request/account/instrument/order/trade/replay/checkpoint validation
→ Stage 5G package validation
→ opaque linear Stage6eAcceptedFreshBrokerTruth
→ accepted Stage 5G reducer/application
```

The capability has private fields and implements no `Clone`, `Debug`,
`Serialize` or `Deserialize`. Application rechecks the exact replay,
journal frontier, authenticated checkpoint and semantic cross-binding that
existed at issuance. The public provider trait is a broker-neutral normalized
collection seam only; implementing it grants no issuance authority.

## Fingerprint and compatibility

Integration fingerprint schema v2 binds the Stage 5 restart authority, Stage
6 replay fingerprint, current journal frontier, authenticated checkpoint and
the active semantic cross-binding fingerprint. Stage 6A schema/goldens, Stage
6B storage/backend and Stage 6C replay semantics/golden remain byte-identical
to accepted Stage 6D base `8d4c1f437c02cfb023aa75fb4a411b9394d2d293`.

## Carry-forward to real execution readiness

Before any real FINAM execution stage, a separate gate must decide and test:

- parent-directory fsync for newly created journal files;
- deployment location and ownership of the filesystem journal;
- operator recovery procedure for corruption.

These host policies are intentionally not simulated by this memory-backed
paper closure.

Primary gate:

```bash
bash scripts/stage6e_gate.sh
```
