# Stage 8B-P1F O2 immutable execution artifact

Status: `REVIEW_CANDIDATE_EXECUTION_NOT_AUTHORIZED`.

This artifact implements the accepted O2 execution contract on top of
`a9f8fe30a45752c943f9e399775322d83fcd8a36`. It is review material only. It
does not authorize installation, `daemon-reload`, service start, Redis access,
FINAM write/order execution, broker dispatch, runtime-live, or real orders.

## Five thin roles

Two Linux/amd64 binaries provide the five accepted roles without introducing a
second orchestration framework:

1. `stage8b-p1f-o2-operator public-key/sign-*` is the offline canonical
   Ed25519 signer. The private seed remains outside Git, the handoff and the
   target.
2. `stage8b-p1f-o2-operator guardian-*` is the fixed-path root guardian CLI
   over the accepted `Stage8bP1fAuthorityStoreV1` API.
3. `stage8b-p1f-o2-materializer` is the no-argument GET-only source
   materializer.
4. `stage8b-p1f-o2-operator runner-fixed/cleanup-fixed`, owned by the fixed
   runner unit, supervises the accepted bootstrap unit independently of SSH.
5. `stage8b-p1f-o2-operator collect-*` is a read-only, redacted evidence
   collector with no lifecycle-authority operation.

The exact source/build identities, binary and unit hashes, fixed paths and
commands are machine-readable in
`stage8b-p1f-o2-execution-artifact.json`.

All payloads are built from synchronized merge
`9d9bd1192467532d0ee48350d3c531d9e156dee3`. A third payload,
`stage8b-p1-paper-supervisor`, is the baseline07 bootstrap-runtime prerequisite,
not a new facade role. The O1 installed supervisor predates baseline07 and
cannot be retained for this corrected configuration. Its separate reviewed
non-activating replacement is required and is NOT authorized by this package.
See [artifact-resumption](stage8b-p1f-o2-artifact-resumption.md) for hashes,
lineage and the evidence limits. Historical O1/O2 contracts are not rewritten.

## Account and secret boundary

The FINAM account id and read-only token are transient systemd credentials.
The public materialization policy pins only the account-id SHA-256. Broker GET
responses must carry the same exact account id. Durable broker-neutral state
uses the public alias `finam-paper-primary`; it never persists the raw account
id. This preserves exact broker binding while complying with `docs/security.md`.

The handoff excludes the private O2 authority seed, FINAM token, raw FINAM
account id, lifecycle credential, signed time-bounded authority documents and
fresh source/response bytes. The selected public O2 authority identity is
included. Signed genesis, activation and O2 phase documents may be issued only
after a separate execution authorization and must bind the exact hashes in
this artifact.

## Exact public inputs

- `stage8b-p1f-o2-materialization-policy.json` binds the account hash, alias,
  venue, and a bounded 399-day bars interval.
- `stage8b-p1f-o2-source-template.json` contains 121 strictly ordered Moscow
  session dates and explicit aligned M10 windows. Every requested M1 bucket
  must exist; a missing holiday/window/bar fails closed before mutation.
- `stage8b-p1f-o2-supervisor-template.json` binds the accepted baseline07
  BO-only runtime (High180 riskgate shadow retained), DB15 policy, generation-2 schedule issuer, broker-neutral account
  alias and the sole allowed source-hash sentinel.
- `stage8b-p1f-o2-authority-public-key.hex` is the selected generation-1 O2
  authority public key. Its offline private seed is not part of the artifact.

The history template is deliberately strict. It does not infer a calendar from
weekdays at execution time and does not silently skip missing bars. If review
or dry preflight shows that one fixed window is not source-producible, the
artifact is revised and reviewed; the materializer does not weaken coverage.

## GET-only materialization

The client owns a fixed `https://api.finam.ru` base URL and exactly five route
kinds: account, account-wide orders, instrument params, schedule and M1 bars.
Callers cannot supply a method, URL, path, query key, account or symbol.
Redirects and system proxies are disabled. POST, PUT, PATCH, DELETE, foreign
identity, extra query keys, duplicate query keys and unlisted routes fail
before transport. The orders snapshot is accepted read-only broker truth, not
an execution endpoint; every returned order must be terminal before O2 can
continue.

## Supervision and replay

The root runner holds the guardian permit and polls both authority and the
exact bootstrap unit every 250 ms. `systemctl` child exit is never treated as
stopped proof. Proof requires no job, zero main/control PID, inactive-or-failed
state and an empty cgroup. Every observation/stop/kill command has a five-second
bound. SIGTERM/SIGINT begins bounded stopping; the thirty-second force-kill
budget is monotonic and cannot be renewed after process or host restart.

`ExecStopPost` invokes `cleanup-fixed` independently of the original process or
SSH. A terminal receipt is forbidden until stopped proof exists. A retained
pending-terminal transaction is replayed byte-exact with its original state,
reason and timestamp; current wall time cannot reclassify it. Expiry applies
only when selecting a new terminal transaction.

## Review and execution boundary

The artifact gate runs the exact GET/materializer/supervisor tests, the
guardian crash/replay tests relevant to O2, and the linked local O2
materialization → isolated bootstrap → durable root/receipt → stopped proof →
terminal receipt witness. The handoff safety checker reconstructs the packaging
and compiled Git trees independently and verifies all three ELF payloads, both
O2 units and the unchanged bootstrap unit. Raw Linux build logs are retained.

Exact-ELF Linux smoke probes fixed-path custody, no-capabilities preparation,
admission failure exit 70 and supervisor baseline07 config validation. It is
explicitly not a running systemd/cgroup proof (`systemd_runtime_tested=false`).
Typed StopNotProven remains API code 72; the operator process maps failures to
70. Source tests exercise stopped-proof handling, separately from this Linux
admission smoke. No successful target bootstrap or execution is claimed.

After artifact acceptance, a separate O2 execution permission is required.
Only then may an operator issue the time-bounded signed documents, install the
already reviewed bytes and perform one O2 run. O3/O4, ordinary P1, operational
Redis mutation and all FINAM write/order paths remain closed.
