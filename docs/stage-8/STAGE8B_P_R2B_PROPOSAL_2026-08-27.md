# Stage 8B-P R2B Proposal R3 — authenticated admission and immutable evidence

Status: **design-only proposal; `R2B authorization = NOT_ISSUED`**.

## Purpose and scope

R3 closes the review findings against R2 without issuing a FINAM capability.
The proposed future operation remains one operator-selected, one-shot,
read-only broker-truth preflight for either PLACE or CANCEL context. This
revision does not use a FINAM credential or external network and does not open
order POST/DELETE, Redis live, broker dispatch, runtime-live or real orders.

The machine authority is
`docs/stage-8/stage8b-p-r2b-proposal-authority.json`; the embedded hash-free
composition contract is
`docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json`.

## Authoritative intake creation

The exact production chain begins inside the accepted owner service at the
crate-private `create_stage8b_r2a8_owner_signed_intake_from_owner` boundary.
It requires all of the following non-serializable authorities at once:

- the current `Stage7bRecoveryReadyOwner` and its single exact durable request;
- opaque `Stage8a1TrustedCurrentSources` minted by the same pinned issuer;
- the accepted operational identity and execution configuration;
- the protected Stage 8A signing capability.

The creator derives chronology and expiry from the authoritative snapshots,
signs the canonical commitment and atomically publishes one fixed owner-signed
intake under a create-new producer lock. It accepts no caller JSON, snapshots,
timestamps, paths or signing request and has no FINAM credential or network.

`stage8b-r2a8-production-intake-stager` is intentionally named and limited to
verifying and staging those exact signed bytes. It is not represented as the
creator. The remaining current-source writer, manifest issuer, source adapter,
authority producer/issuer and package issuer retain their accepted fixed-path,
no-network boundaries.

## Root-authenticated admission

The R2B launcher is a surviving root supervisor and implements a
root-owned inode-bound admission. Before nonce admission it
opens the exact helper once with `O_NOFOLLOW`, rejects setuid/setgid bits and
`security.capability`, hashes the opened FD, and creates the structured
terminal socketpair. Root then consumes the nonce and creates immutable
root-owned `0400` nonce and admission records under root-only `0700`
directories.

The helper receives this explicit FD allowlist only:

```text
0,1,2  standard descriptors
3      root-owned sealed admission receipt
4      root-created structured terminal channel
5      exact root-owned admission-record inode
6      exact root-owned nonce-marker inode
7      exact already-verified helper executable
```

The receipt binds operation, nonce, package, helper, supervisor hash, contract,
both root-owned inode identities, terminal-channel identity and a 30-second
expiry. The helper verifies owner/group/type/mode/link count, exact memfd seals,
bounded size and every device/inode binding. A UID 8301 process cannot create
or open equivalent root-owned records; direct helper execution therefore fails
before credentials or network.

Root validates only package and authority material and never opens the FINAM
secret. In the child, receipt provenance is verified before credential loading;
credential files are opened only after the irreversible UID/GID drop.

Before `fexecve` the child applies `PR_SET_NO_NEW_PRIVS`, clears supplementary
groups and ambient capabilities, performs `setresgid(8301,8301,8301)` and
`setresuid(8301,8301,8301)`, verifies real/effective/saved identities and empty
inheritable/permitted/effective/ambient capability sets, then closes every FD
above 7. The same verified FD 7 is executed.

No repository test key is presented as independent helper approval. Helper
acceptance is the exact frozen SHA-256, reproducible build provenance, signed
run-package binding and independently accepted supervisor hash.

## Unified admission-to-terminal lifecycle

After durable nonce consumption all fallible outcomes are owned by the root
supervisor. `HELPER_PROCESS_STARTED` is recorded only after the helper has
completed privilege-drop/fexecve/bootstrap and sent its authenticated channel
handshake. The lifecycle states are:

```text
ADMISSION_REQUESTED
ADMISSION_MARKER_CREATED
ADMISSION_DURABLE
HELPER_EXEC_ATTEMPTED
HELPER_PROCESS_STARTED
HELPER_TERMINAL_RECEIVED
HELPER_EXITED_SUCCESS | HELPER_EXITED_FAILURE
TERMINAL_EVIDENCE_DURABLE
ADMISSION_PERSISTENCE_FAILURE | TERMINAL_PERSISTENCE_FAILURE
```

Missing/invalid frames, startup timeout, helper crash, privilege-drop or
`fexecve` failure and post-admission package/credential failure produce a
root-generated redacted fallback terminal. The nonce stays consumed and no
automatic retry is allowed.

## Immutable final evidence

The UID 8301 helper cannot write the final evidence directory. It only sends a
bounded structured terminal message through FD 4. Root validates the message
against the admission, waits for the child and wraps it in
`R2bRootTerminalRecordV1`, binding:

- admission commitment and supervisor/package/helper hashes;
- nonce-marker and admission-record device/inode identities;
- kernel-observed child PID, exit code or signal;
- redacted semantic request attempts and broker-truth summary.

Root publishes exactly one final file below
`/var/lib/moex-trading/stage8b/r2b-evidence` using create-new, `O_NOFOLLOW`, a
bounded fsynced pending inode, atomic hard-link publication and directory
`fsync`. Directory custody is `root:root 0700`; final file custody is
`root:root 0400`. UID 8301 cannot truncate, chmod, unlink, rename or recreate
the record. Finalization failure writes a separate root-owned admission marker
and requires operator review.

The controlled Linux rehearsal exercises three post-admission failures:
`fexecve` failure, helper death after its authenticated startup handshake, and
root-finalizer fsync failure. The first two end in root-owned terminal failure
evidence. The fsync case consumes the nonce and leaves a separate root-owned
`TERMINAL_PERSISTENCE_FAILURE` marker; it cannot be retried automatically.

## Frozen read-only network contract

A later, separate issuance may allow only `https://api.finam.ru`: two
AuthService POST calls (`/v1/sessions`, `/v1/sessions/details`) followed by the
fixed broker-truth GET sequence. Order POST/DELETE remain forbidden. Redirects,
proxies, retries, arbitrary request APIs, loops and caller query overrides are
forbidden. Trades use a single page of 1000 rows and the exact inclusive-start,
exclusive-end interval `[request_time-24h, request_time)`.

## Required acceptance before issuance

R3 requires reproducible Linux/amd64 production and controlled builds, the
full static/negative gate and an adversarial no-external-network rehearsal that
proves direct/forged helper rejection, root terminal immutability, replay
rejection, controlled PLACE/CANCEL behavior, child-failure finalization and
explicit effect closure. Proposal acceptance still does not issue R2B; an
independent issuance package remains mandatory.

## Explicit closure

```text
R2B authorization       NOT_ISSUED
FINAM external network  not used
order POST/DELETE       closed
Redis live              closed
broker dispatch         closed
runtime-live            closed
real orders             false
```
