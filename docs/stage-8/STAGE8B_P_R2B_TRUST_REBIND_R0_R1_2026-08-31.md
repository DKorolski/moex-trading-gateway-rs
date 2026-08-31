# Stage 8B-P R2B Trust Rebind R0-R1

## Purpose

This is the evidence/governance closure for the already materialized Generation 2 candidate. It does not generate another key set and does not activate Generation 2.

The actual retained ceremony is verified by the exact Rust verifier. Its private directory is supplied only through `STAGE8B_R2B_TRUST_REBIND_CEREMONY_DIR`; the value is not printed, persisted in project evidence, included in process arguments, or added to the handoff.

## Public verification receipt

The verifier emits a typed public-only receipt bound to the reviewed commit and the exact verifier source digest. The receipt records the public trust and account manifest hashes, the 13 signing-seed bindings, the one account-key binding, ownership/mode/link/inventory checks, and the unchanged `REQUIRED_NOT_VERIFIED` backup state.

The receipt is signed by the Generation 2 package-authorization key in the isolated domain:

```text
stage8b-p-r2b-trust-rebind-verification-receipt-v1
```

That signature proves possession for verification evidence only. It is not an R2B package authorization and cannot issue or execute an order package.

The handoff maker derives private binding counts and public fingerprints from this receipt. It does not write those claims as independent constants.

## Exact transition freeze

The checker exact-compares the complete incident, custody, verification and activation objects. It also exact-compares the supersession record including both candidates, the complete transition-state keyset, the ordered activation preconditions and rollback policy.

Unknown fields, removed fields, substituted preconditions and replacement-candidate fingerprint drift fail closed.

## Deliberately closed state

- encrypted offline backup: `REQUIRED_NOT_VERIFIED`;
- Generation 2 active: `false`;
- package authorization: `NOT_ISSUED`;
- helper acceptance reissued: `false`;
- production binaries rebuilt: `false`;
- production credentials installed: `false`;
- controlled installation: `false`;
- FINAM and AuthService calls: `false`;
- HTTP POST/DELETE: `false`;
- broker dispatch, Redis live and runtime-live: `false`.

After independent acceptance, the next separate stage is encrypted offline backup plus disposable restore verification. Composition rebuild remains blocked until that backup/restore attestation is accepted.
