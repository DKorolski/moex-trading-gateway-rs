# Stage 8B-P R2B Controlled Installation / Full Transaction Proof R0

## Status

This package is a design and pre-installation governance boundary. It records
the accepted Implementation R0-R1A predecessor at
`6672819e357a3c2a2c1e73e5408c393da01913a1`, formalizes the pre-production
trust/helper/path supersession, and freezes a separate full 31-service
transaction contract.

It does not install, enable or start any service. It does not select an
operator, materialize real credentials, issue R2B authority, connect to FINAM,
or authorize a production-host deployment.

## Why this package exists

The accepted runtime composition contract is intentionally scoped to the
post-draft execution composition embedded in the accepted helper evidence. The
draft builder is nevertheless part of the six-phase implementation
transaction. Changing the embedded helper contract merely to rename that
scope would require an unnecessary helper/launcher rebuild.

This package therefore adds a non-embedded implementation transaction
contract. It binds the complete graph independently while preserving the
accepted production ELF set unchanged.

## Supersession lineage

`stage8b-p-r2b-preproduction-supersession.json` records:

- old and new accepted helper SHA-256 values;
- old and new trust manifest and public-key-set SHA-256 values;
- old and new account-key manifest and generation-one key SHA-256 values;
- legacy and dedicated draft, signed-package and credential paths;
- accepted and superseded source commits;
- the reason for the rebind;
- zero prior installations, issued R2B packages, real credentials and FINAM
  requests.

The retained numeric `generation=1` fields are explicitly classified as a
pre-production initial rebind, not as proof that old and new public keys are
the same lineage. A future controlled installation must materialize a distinct
reviewed ceremony ID.

## Full transaction contract

`stage8b-p-r2b-implementation-transaction-contract.json` freezes:

1. four current-source services;
2. current-manifest issuer and source adapter;
3. eleven exact producer instances;
4. eleven exact issuer instances;
5. run-package draft builder followed by package signer;
6. root read-only supervisor.

The aggregate contains exactly 31 service invocations and six phase targets.
The contract binds every unique unit/target file and all accepted production
Linux/amd64 executable hashes, including the draft builder omitted from the
post-draft runtime composition.

## Future controlled proof

After independent acceptance of this design package, a separate implementation
package may prepare an isolated staging/VPS contour with canary or offline
credentials. That package must dynamically prove:

- the complete success graph;
- every phase failure blocks downstream phases;
- stale/replayed outputs cannot satisfy a new transaction;
- reset removes all transaction-scoped state before a second run;
- the contour is removed after proof.

The proof contour must have no route to FINAM and must not be the production
account host. Real operator identity and real broker credentials remain
forbidden.

## Closed surfaces

- R2B authorization: `NOT_ISSUED`;
- service installation/enable/start: false;
- production account host: forbidden;
- real credentials and operator selection: forbidden;
- FINAM AuthService and broker GET: forbidden;
- HTTP POST/DELETE and broker dispatch: forbidden;
- Redis live, runtime-live and real orders: forbidden.

## Acceptance boundary

Acceptance of R0 authorizes only the next isolated staging implementation and
proof package. It does not authorize installation, activation or any external
network request by itself.
