# TZ — Stage 8A-1 R3 Trusted Issuer / One-Arm / CANCEL Continuation Closure

Base candidate: `166423668b2dea3e1a9ea505f1d452a367c62b64`  
Independent R2 review SHA-256: `c8b20954e89031e292596c9d4a0b245960109889faf2b5ee9510d79ef62ae77e`  
Accepted predecessor: `c949d7f83aa87cf990204a5b8ae66e5ca37c9f1d`

## 1. Goal

Close the final Stage8A-1 authority boundary before builder composition.

R2 made the Stage7B/Stage6 durable authority current-seal and dispatch-ready.
R3 must make the **issuer source itself trusted**, enforce one logical arm per
durable request, and provide current-state revalidation for CANCEL.

Stage8A-2 remains CLOSED.

## 2. Closed surfaces

R3 MUST NOT add:

- broker-finam PLACE/CANCEL builder composition;
- approved request/body extraction;
- mock or real send seam;
- HTTP classifier;
- reconciliation implementation;
- FINAM POST/DELETE;
- Redis live consumer;
- broker dispatch;
- runtime-live;
- real strategy orders.

## 3. Trusted issuer/composition root

`Stage8a1OperationalAuthorityIssuer::open(arbitrary_path)` is not sufficient as
a trust boundary.

Introduce an opaque root/composition authority.

It must bind:

```text
canonical Stage8 authority root identity
current Stage7B operational identity
accepted Stage8 config identity / expected SHA
persistent operator-control source identity
arm-registry root identity
```

A raw public caller must not be able to create a directory with arbitrary
config/control files and turn it into a valid issuer.

Preferred options:

1. crate-private raw issuer + public owner-mediated trusted constructor; or
2. public constructor that requires an opaque `Stage8a1AuthorityRoot` issued by
   trusted startup/composition code.

## 4. Accepted configuration trust

The config sidecar cannot be its own trust anchor.

Required behavior:

```text
attacker config + attacker recomputed sidecar
!= accepted configuration authority
```

Bind the expected config SHA/path identity from an independently trusted startup
or accepted deployment/config authority.

The current config may still be reread on every mint/revalidation, but both
bytes and identity must match that trusted expectation.

## 5. Persistent current control

Kill switch / broker-owner count / reconciliation count / micro budget must be
read through the trusted authority root.

If file-backed:

- use regular-file/no-symlink enforcement;
- pin root/file identity or use descriptor/openat-style ownership sufficient to
  reject path replacement;
- use atomic durable writes for source updates;
- enforce monotonic durable revision where applicable;
- reject root/control replacement after issuer construction.

A caller-created `RunAllowed` JSON file is not authority.

## 6. Current readiness / broker truth / schedule provenance

The minting boundary must not accept arbitrary caller-created snapshots as
authority.

Either:

- accept opaque current-source authorities issued by the owning Stage7B/read-only
  broker-truth composition; or
- keep raw snapshots behind a crate-private issuer method reachable only from
  trusted composition.

Black-box public code must be unable to synthesize fresh snapshots and mint.

## 7. One arm per durable request

Remove free-form caller nonce as the logical one-shot authority.

The trusted operator-control issuer owns arm identity.

Persist a request-keyed issuance record binding:

```text
operational generation
durable request provenance / StrategyRequestId
exact command digest
policy digest
operator authorization artifact/nonce
```

After first arm issue for a durable request:

```text
same nonce -> FAIL
different nonce -> FAIL
different caller string -> FAIL
```

No auto-rearm for the same durable request.

A future retry uses a new durable request identity as already frozen by Gate
7→8.

## 8. Symmetric current-state continuation

R2 PLACE continuation semantics are retained.

Add equivalent CANCEL revalidation.

Before Stage8A-2 can consume a CANCEL authority, revalidate:

```text
current Stage7B disk seal
exact dispatch-ready CANCEL durable identity
exact target mapping / terminal state
accepted config/policy
operator arm registry
kill switch
readiness
ownership
ambiguity
broker truth
schedule
budget
time/expiry
```

Any drift fails closed.

No request extraction is exposed in R3.

## 9. Acceptance

Use:

`STAGE8A_1_R3_ACCEPTANCE_MATRIX_2026-08-15.csv`

Exact mandatory rows: **76**.

Use:

`STAGE8A_1_R3_NEGATIVE_INVENTORY_2026-08-15.md`

Exact negative cases: **70**.

## 10. Required evidence

- trusted-root vs attacker-root black-box matrix;
- config+sidecar co-rewrite negative;
- fake RunAllowed/current-control negative;
- synthetic raw broker/readiness snapshot negative;
- same durable request + different nonce negative;
- root/control replacement/symlink identity drift matrix;
- PLACE and CANCEL post-mint revalidation tests;
- inherited Stage7B seal/dispatch tests;
- exact SHA-pinned closed surface;
- compile-fail opacity;
- focused + serialized workspace tests/doctests;
- clippy/fmt;
- immutable handoff.

## 11. Exit

Independent `ACCEPTED Stage 8A-1 R3` opens **Stage 8A-2 only**.

No builder or transport work belongs in R3.
