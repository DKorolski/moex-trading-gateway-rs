# TZ — Stage 8A-1 R2 Operational Authority Continuity

Base candidate: `ef6b9ac70aa8a3cdd6bfaf93f1c1339b030eb75d`
Independent R1 review SHA-256: `6c98f034f2456a3004ca4fd162ff632372dc22d0199240cd990784025d89a9a6`
Accepted predecessor: `c949d7f83aa87cf990204a5b8ae66e5ca37c9f1d`

## 1. Goal

Finish Stage8A-1 as a **usable, operationally continuous, strictly no-send
authority layer**.

R1 made proof objects opaque. R2 must connect those objects to their real
production sources and make the Stage7B durable bridge current-on-disk and
dispatch-ready.

Stage8A-2 remains closed.

## 2. Absolute closed surfaces

R2 MUST NOT add:

- `broker_finam::build_place_order_request()` composition;
- `broker_finam::build_cancel_order_request()` composition;
- approved request/body extraction;
- local/mock send seam;
- HTTP classifier;
- broker reconciliation implementation;
- FINAM POST/DELETE;
- Redis live command consumer;
- broker dispatch;
- runtime-live;
- real strategy order flow.

## 3. Stage7B / Stage6 owner-mediated durable authority

Replace the cached-seal read-only bridge with an owner-mediated transition that
has access to:

```text
&mut Stage7bRecoveryReadyOwner
Stage5gLifecycleCommitmentKey
exact Stage6 durable identity/command
```

Before issuing Stage8 durable authority it must:

1. require lifecycle availability;
2. reread and authenticate the committed recovery seal from disk;
3. fail on missing/corrupt/valid-but-different seal;
4. prove exact durable `RequestAccepted`;
5. prove exact durable `DispatchAttemptRecorded` for the same request;
6. bind dispatch record id/sequence/frontier into the Stage8 authority;
7. refresh the current Stage6 frontier;
8. if current checkpoint is not covered, durably advance the seal using the
   accepted temp-fsync -> rename -> root-fsync -> reread/HMAC procedure;
9. reread/authenticate the final covering seal immediately before returning
   authority.

No provider or broker transport may run.

An accepted-only request is not execution-ready Stage8 authority.

## 4. Forward-path witness

Add a real no-send positive integration test:

```text
fresh Ready Stage7B owner
-> new canonical command
-> durable RequestAccepted
-> durable DispatchAttemptRecorded
-> current Stage6 checkpoint
-> covering recovery seal durably committed/reread
-> Stage8 durable authority
-> Stage8 capability
-> zero paper provider effect
-> zero FINAM builder/HTTP effect
```

Do not use a restart-only presealed fixture as the sole positive witness.

## 5. Production proof issuers

Implement production issuers for every non-durable opaque authority.

Required issuer domains:

```text
Stage8a1OperatorArmAuthority
Stage8a1FrozenExecutionPolicy
Stage8a1TrustedClockAuthority
Stage8a1ReadinessAuthority
Stage8a1KillSwitchAuthority
Stage8a1BrokerOwnershipAuthority
Stage8a1ZeroAmbiguityAuthority
Stage8a1FreshBrokerTruthAuthority
Stage8a1ScheduleAuthority
Stage8a1MicroBudgetAuthority
```

The exact crate/module ownership may vary to avoid dependency cycles.

Rules:

- no public field/literal constructors;
- no “digest string = authority” constructor;
- issuer validates the source and creates the opaque proof;
- issuer and proof are no-send;
- black-box integration code outside the private capability module can obtain
  proofs only through these production issuers.

## 6. Operator-arm issuer / nonce replay protection

The production operator issuer owns nonce generation/registration.

It must bind the exact command/policy and reject duplicate logical nonce issue.

A test-local `HashSet` is not evidence.

At minimum:

```text
same nonce/current generation -> second issue FAIL
same arm authority -> second mint impossible/FAIL
restart generation change -> old arm invalid
command/policy/build/config/endpoint drift -> FAIL
```

## 7. Frozen policy issuer

The frozen policy must be issued from the accepted current configuration source,
not merely self-hashed.

The issuer must bind the complete relevant `OrderPreflightPolicy` surface,
Stage8 allowlist and fixed TTL/freshness bounds.

No public caller chooses:

```text
max_arm_ttl_ms
max_evidence_age_ms
max qty/notional/slippage/reference-age
```

at mint time.

## 8. Kill switch / ownership / ambiguity / truth / schedule / budget provenance

Opaque proofs must be issued from real reviewed sources.

At minimum:

- kill-switch proof comes from the persistent current state/revision;
- ownership proof is exact FINAM + account/instrument/strategy scoped;
- ambiguity proof is exact account/strategy scoped;
- broker-truth proof derives from accepted current read-only truth;
- schedule proof derives from current schedule/session source;
- budget proof derives from trusted max-one account/strategy source;
- readiness derives from current composite readiness;
- all sources share the same operational identity/seal/config generation.

## 9. Current-state continuation barrier

Add a **no-send** revalidation boundary for an already minted capability.

The future Stage8A-2 consumer must not be able to use capability solely because
it was valid at mint time.

Before later command extraction/composition, revalidate current:

```text
Stage7B seal / operational identity
config/build/endpoint policy
readiness
kill switch
broker ownership
ambiguity
broker truth
schedule
budget
time/expiry
```

Any drift -> fail closed.

R2 itself exposes no approved request extraction.

## 10. Acceptance

Use:

`STAGE8A_1_R2_ACCEPTANCE_MATRIX_2026-08-15.csv`

Exact mandatory rows: **68**.

Use:

`STAGE8A_1_R2_NEGATIVE_INVENTORY_2026-08-15.md`

Exact negative cases: **62**.

## 11. Required evidence

- current-seal disk tamper matrix;
- accepted-only vs dispatch-ready durable matrix;
- forward non-restart no-send integration witness;
- production issuer black-box integration tests;
- duplicate arm nonce issuer test;
- post-mint drift revalidation tests;
- compile-fail privacy/linearity;
- exact SHA-pinned closed surface;
- focused tests;
- serialized workspace all-target tests/doctests;
- clippy/fmt;
- immutable SHA-bound handoff.

## 12. Exit

Independent `ACCEPTED Stage 8A-1 R2` opens **Stage 8A-2 only**.

No real or mock request-builder composition is implemented in R2.
