# Stage 8B-P R2B Issuance Package R0-R1

> R0-R1 is retained as the substantive closure record. Its formal
> exact-identity microfix is frozen by
> `STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1A_EXACT_GOVERNANCE_FREEZE_2026-08-29.md`.
> R0-R1A changes governance evidence only and keeps R2B `NOT_ISSUED`.

Status: narrow design-closure candidate, `NOT_ISSUED`.

R0-R1 closes only the two findings against R0 commit
`928168ed47e5b9dd873cd73815fbccecde7a8981`. It does not implement an
activation target, select an operation or account, read credentials, contact
FINAM, issue an arm or start a service.

## Exact R2B read contract

The authoritative R2B contract is the six-document read/auth snapshot embedded
by `r2a3.rs`, `r2a4.rs` and `r2a5.rs`:

`docs/stage-8/stage8b-p-r2a3-finam-read-contract-snapshot.json`

Its SHA-256 is
`7c8e6bcd02f907af93ea1386499d03bff194da76a1eb2b19dd9c2ff1f97403c5`.
The exact inventory is Auth, TokenDetails, GetAccount, Trades, GetOrders and
GetOrder. A public refresh on 2026-08-29 returned HTTP 200 for all six documents
and matched every embedded byte count, SHA-256 and required route/schema marker.
No credential, AuthService request or broker GET was used.

The following equality is mandatory:

`authority snapshot SHA == helper embedded snapshot SHA == unsigned/signed
run-package contract_snapshot_sha256 == fresh refresh evidence snapshot SHA`.

The seven-document order/effect snapshot remains valid lineage for future XE,
but it is not R2B read-contract evidence. The design refresh is not reusable as
activation authority. The final local activation package must repeat the same
six-document refresh no more than 1,800 seconds before admission.

## Package formation boundary

R0-R1 selects model A: a separate future
`stage8b-r2b-run-package-draft-builder` executable and
`moex-stage8b-r2b-run-package-draft-builder.service`.

The builder runs as root only because it must verify files owned by eleven
distinct issuer identities. It has no signing key, no credentials path and no
network address family. It reads only fixed paths, validates the current nonce,
all eleven signed receipts, accepted helper, manifests and operator decision,
then atomically publishes:

`/var/lib/moex-trading/stage8b/r2a5/r2b-run-package.unsigned.json`

as `root:root 0600`, with file and parent-directory fsync. The output is the
exact `R2a5RunPackage` schema with an empty signature. It binds the embedded
read-contract SHA and has a maximum 30-second validity interval. Mixed nonces,
stale receipts, missing sources, reused output, symlinks, wrong ownership/mode
or controlled-fixture producers are fatal.

The existing `stage8b-r2a5-package-issuer` remains a separate no-argument signer.
It alone may read the package-authorization key, validates the fixed unsigned
draft, signs it and atomically publishes the signed package. Construction and
authorization signing therefore remain separate.

The production builder does not yet exist. R0-R1 freezes its contract; the next
implementation package must add the Rust executable, reproducible Linux build,
systemd unit and negative tests without enabling or starting them.

## Frozen fail-closed transaction

The future transaction contains 31 service invocations in six phases:

1. four current-source services;
2. manifest issuer and source adapter;
3. eleven authority producers;
4. eleven authority issuers;
5. draft builder followed by package issuer;
6. root read-only supervisor.

Each phase has one exact target name frozen in the machine authority. A phase
target requires and orders every service in that phase plus its predecessor
target. The aggregate target requires and orders the terminal phase. A failed,
skipped or missing service/output blocks every downstream phase. `Condition*`
skip semantics are forbidden in the implemented transaction: authoritative
fixed-input checks must fail the service instead.

Every output is bound to the current run nonce or its signed current-source
generation. Existing files from a prior run cannot satisfy a phase. Phase 5
requires all eleven current-run receipts; the package issuer requires the exact
current-run unsigned draft; phase 6 requires the exact durable signed package.
Partial producer or issuer fanout is fatal.

The aggregate `moex-stage8b-r2b-issuance.target`, all six phase targets, the
draft-builder executable and its unit remain absent in R0-R1.

## Closed surfaces

`R2B = NOT_ISSUED`. Operator selection, nonce, signed package, credentials,
AuthService, broker GET, order POST/DELETE, dispatch, Redis live, runtime-live,
strategy-live and real orders remain absent or closed.
