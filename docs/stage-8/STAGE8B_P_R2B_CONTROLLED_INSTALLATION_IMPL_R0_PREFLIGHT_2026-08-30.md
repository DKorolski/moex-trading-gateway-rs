# Stage 8B-P R2B Controlled Installation — Implementation R0 Preflight R1A

Status: production request-boundary proof-oracle microfix; independent review
required; execution not started.

## Lineage and correction scope

This R1A follows the rejected R1 preflight commit
`9fd9fa9e7eea38371bb412a713f0419697671f7c`, which closed the earlier R0
findings but admitted a category-only production proof oracle. It retains the accepted
Controlled Installation R0 design at
`1e4db79288b0809fd5975edfdd0fc14740bcc8c6` and the accepted production
Implementation R0-R1A at `6672819e357a3c2a2c1e73e5408c393da01913a1`.

No production Rust or production unit is changed. R1A keeps the R1 trigger,
ceremony, container and cleanup closures and narrows the production proof
oracle to structured failed-request evidence.

## Two proof lanes

### Lane A — exact production artifacts, expected fail-closed boundary

Lane A installs the 12 exact production ELF and 18 exact production
unit/target files. The helper remains hard-bound to `https://api.finam.ru`.
The contour has no network route, so a successful broker read is neither
expected nor claimed.

The exact expected result is:

```text
Phases 1–5                         SUCCESS
Phase 6 local authority/admission  SUCCESS
POST /v1/sessions attempt #1       EXPECTED FAIL-CLOSED
root terminal evidence             DURABLE
aggregate target                   EXPECTED FAILED
outer proof runner                 PASS
```

Outer PASS requires `EXACT_TYPED_ROOT_TERMINAL_EVIDENCE`. The root admission,
helper authority validation and child terminal protocol must have succeeded;
the evidence must contain failed attempt ordinal `1`, method `POST`, route
template `/v1/sessions`, no HTTP status and no response body. The only accepted
attempt outcomes are `NETWORK_CONNECT_FAILURE` and request-level `TIMEOUT`.
All order-effect, broker-dispatch and real-order flags must be false.

`AUTH_SESSION_FAILURE` without an attempt, local HTTP-client construction
failure, a root supervisor lifecycle timeout, a wrong route/method/ordinal or
an HTTP response cannot satisfy Lane A. String matching on a terminal category
is not an oracle. Outer PASS never means that the production aggregate target
succeeded.

Lane A requires ephemeral materialization of the accepted pre-production
trust set. Public fingerprints must exactly match the embedded production
authority. A newly generated random canary key set is forbidden in Lane A. The
corresponding private ceremony is not stored in Git or any handoff; if a
separately reviewed matching offline ceremony is unavailable, Lane A aborts
before container creation. The earlier accepted Phase-5/6 evidence may be
referenced as inherited semantic qualification, but cannot be represented as
a new dynamic run.

### Lane B — controlled TLS read-pipeline success

Lane B is a separate controlled qualification domain. It uses the accepted
controlled-custody feature, loopback-only TLS server, controlled endpoint/CA
and a fresh canary trust domain. It may prove successful AuthService/GET
semantics, but controlled binaries and results may not be counted as Lane A
production-binary proof.

The accepted Stage 8B-IT-TLS R1 at
`6cb179509fad97e8be56e31bb930b2a86caefc6a` is inherited for controlled TLS
semantics. Any new Lane B execution must pin its own controlled ELF and server
hashes in its execution package.

## Exact graph trigger

Direct `systemctl start moex-stage8b-r2b-issuance.target` is forbidden because
the production target has `RefuseManualStart=yes`. The sole allowed activation
entry is the proof-only unit:

```text
stage8b-r2b-controlled-proof-trigger.service
```

It has exact `Requires=` and `After=` edges to the aggregate target,
`Type=oneshot`, `ExecStart=/bin/true`, no install section and SHA-256
`c30da9c111a0e681de6cd4cc23bab3b1d58f5b15b86aff885d0838cf43c6cf0f`.
It is the nineteenth installed unit file, is not part of the 31 production
service-invocation arithmetic, is never enabled, and is removed after proof.
No production drop-in or `RefuseManualStart` mutation is allowed.

## Frozen contour

The execution contour is a disposable Linux/amd64 systemd container on the
local developer workstation. The exact final image ID is
`sha256:3cc66c640df0444530a626d2acbcfeda9742039b917a747fd023b315ef2c1526`.
Its base image, build recipe, systemd version and package inventory are pinned
in the staging inventory. Rebuilding under the same tag is not accepted; the
image ID must match before execution.

The exact Docker boundary uses `--platform linux/amd64`, `--network none`,
`--privileged`, `--cgroupns=host`, a cgroup mount and four allowlisted mounts.
`--privileged` grants broad capabilities and device access inside the
disposable Docker Desktop Linux VM; no separate `--cap-add` or `--device`
flags are supplied. The VM has no sensitive co-tenant workload. Host root,
Docker socket, `.env`, broker configuration and production runtime directories
are not mounted.

`/work` must be a fresh extraction of the reviewed handoff. Its
`handoff-commit.txt`, source manifest and archive SHA must pass before Docker
creation. A developer checkout is forbidden.

The runner must receive `--artifact-root`; all 12 ELF bytes must match the
accepted transaction contract before Docker creation.

## Reset and uninstall

The reset contract now lists every one of 19 unit destinations and every one
of 12 binary destinations explicitly. Wildcards are diagnostic only and are
not cleanup authority. Each installed artifact has exactly one removal entry
and one absence check.

After the first run, all nonce, receipt, current-source, authority, package and
ceremony projections are removed before the second run. Run-1 tmpfs files,
file descriptors and projected copies are destroyed and cannot be retained.
Run 2 must freshly rematerialize the same accepted offline key identities into
a fresh tmpfs projection. Cleanup runs on success and failure, removes all
units, binaries, state and canary material, reloads systemd, resets failed
state and destroys the container. Any cleanup failure makes the outer proof
fail.

## Closed boundary

- this R1A does not authorize execution;
- `R2B = NOT_ISSUED`;
- no real operator, account or broker credential;
- no FINAM route, AuthService, broker GET, POST or DELETE;
- no broker dispatch, Redis live, runtime-live or real orders.
