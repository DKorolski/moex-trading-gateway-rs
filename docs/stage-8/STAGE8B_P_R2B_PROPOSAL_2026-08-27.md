# Stage 8B-P R2B Proposal R4 — isolated admission, typed terminal and reachable creator

Status: **design-only proposal; `R2B authorization = NOT_ISSUED`**.

## Purpose and scope

R4 closes the review findings against R3 without issuing a FINAM capability.
The proposed future operation remains one operator-selected, one-shot,
read-only broker-truth preflight for either PLACE or CANCEL context. This
revision does not use a FINAM credential or external network and does not open
order POST/DELETE, Redis live, broker dispatch, runtime-live or real orders.

The machine authority is
`docs/stage-8/stage8b-p-r2b-proposal-authority.json`; the embedded hash-free
composition contract is
`docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json`.

## Authoritative intake creation

The exact production chain now has a built, no-argument
`stage8b-r2a8-authoritative-intake-creator` binary and frozen oneshot unit.
Its public fixed-input entrypoint reconstructs the Stage 7B owner and pinned
Stage 8A issuer, then reaches the crate-private
`create_stage8b_r2a8_owner_signed_intake_from_owner` boundary.
It requires all of the following non-serializable authorities at once:

- the current `Stage7bRecoveryReadyOwner` and its single exact durable request;
- opaque `Stage8a1TrustedCurrentSources` minted by the same pinned issuer;
- the accepted operational identity and execution configuration;
- the protected Stage 8A signing capability.

The accepted owner-signed intake is a pinned bootstrap projection, not a
caller DTO: its signature, config and freshness are checked before its opaque
snapshots can be reminted by the reconstructed issuer. The creator derives
chronology and expiry from those authoritative snapshots,
signs the canonical commitment and atomically publishes one fixed owner-signed
intake under a create-new producer lock. It accepts no caller JSON, snapshots,
timestamps, paths or signing request and has no FINAM credential or network.
The bootstrap predecessor is explicitly one still-fresh owner-signed intake
from accepted R2A8; it is not a first-generation provisioning mechanism. The
creator runs as UID/GID 8094 with only supplementary GID 8095 for the fixed
Stage 5G lifecycle-key custody file. Its unit has no automatic install target.

`stage8b-r2a8-production-intake-stager` is intentionally named and limited to
verifying and staging those exact signed bytes. It is not represented as the
creator. Its exact oneshot unit requires and follows the creator unit, so the
runtime sequence is executable rather than conceptual. An isolated Linux
rehearsal runs the exact production creator and stager binaries with Docker
networking disabled and proves byte-identical staging plus false network and
credential evidence. The remaining current-source writer, manifest issuer, source adapter,
authority producer/issuer and package issuer retain their accepted fixed-path,
no-network boundaries.

## Root-authenticated admission

The R2B launcher is a surviving root supervisor and implements a
root-owned inode-bound admission. Before nonce admission it fail-closes unless
`kernel.yama.ptrace_scope >= 1` and no process already exists under the
dedicated UID 8301. The frozen systemd unit uses `ProtectProc=invisible` and
`ProcSubset=pid`. The helper sets `PR_SET_DUMPABLE=0` before constructing its
async runtime. The Linux rehearsal proves that a same-UID process cannot use
`pidfd_getfd`, `process_vm_readv` or `ptrace` to steal or inspect the admitted
child. Before nonce admission the launcher also
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

Every frame is deserialized as the exact deny-unknown-fields
`R2bSupervisorMessageV1`/`R2bTerminalEvidenceV1` protocol. Root validates all
operation, package, contract, composition, chronology, route, method, attempt,
response and redaction invariants. It derives its own canonical outcome and
reconciles the child report with kernel exit status.

One monotonic 120-second deadline covers startup, every frame byte and child
exit. Root supervises through a pidfd, kills on expiry and bounds reap to two
seconds. Missing/invalid/partial frames, byte-drip, terminal-then-hang, startup
timeout, helper crash, privilege-drop or
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

The controlled Linux rehearsal exercises post-admission failures including:
`fexecve` failure, helper death after its authenticated startup handshake, and
root-finalizer fsync failure, no-start/no-terminal frames, partial header/body,
slow-drip and terminal-then-hang. Timeout paths end in canonical root failure
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

R4 requires reproducible Linux/amd64 production and controlled builds, the
full static/negative gate and an adversarial no-external-network rehearsal that
proves direct/forged helper rejection, root terminal immutability, replay
rejection, same-UID isolation, exact typed terminal validation, absolute
deadline behavior, reachable creator composition, controlled PLACE/CANCEL
behavior, child-failure finalization and explicit effect closure. Nonce and
admission markers perform a second file `fsync` after `chmod 0400` before the
directory `fsync`. Proposal acceptance still does not issue R2B; an
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
