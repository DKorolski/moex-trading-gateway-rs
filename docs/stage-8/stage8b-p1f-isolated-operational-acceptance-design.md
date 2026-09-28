# Stage 8B-P1-f R4 isolated operational acceptance design correction

Status: **DESIGN CORRECTION REVIEW CANDIDATE — NO ACTIVATION**.

Correction parent: `811ebe8ce22291311bd63fbf0cc5ff723261b758`.
The independent R3 review is bound by SHA-256
`585872992198d80a0f05e76a791303c272b016903e9fbba1bcaf1132cf0365b9`.
Stage 8B-P1-e I1 remains CLOSED / ACCEPTED at governance closure
`3f171d997de5616cb9a07311d7776e446456c0c1`.

This correction changes design, model fixtures and fail-closed review tooling
only. It performs no SSH mutation, installation, Redis command, systemd action,
FINAM request or process start. The 78 matrix rows below are design obligations,
not completed operational scenarios.

## 1. Goal and unchanged phase boundary

P1-f proves the accepted durable paper composition under a real service manager
and real loopback Redis while keeping the P0 contour and all order-capable
surfaces closed. The seven phases remain ordered:

1. `P1F-I`: local source implementation only;
2. `P1F-O0`: immutable read-only target preflight;
3. `P1F-O1`: non-activating provisioning;
4. `P1F-O2`: authorised fresh materialization followed by an AF_UNIX-only,
   `PrivateNetwork` bootstrap;
5. `P1F-O3`: maximum 30-minute synthetic paper session and restart matrix;
6. `P1F-O4`: separately accepted maximum 180-minute read-only FINAM-bars
   session;
7. `P1F-A`: aggregate operational acceptance.

No successful phase starts the next phase. Every O1–O4 transition needs a new
independently accepted, single-use manifest.

## 2. Exact isolation and target

The target remains `stage8b-p1f-isolated-vps-1`, `45.150.11.252`, with SSH
ED25519 fingerprint
`SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo`.

The complete isolation object is an exact contract, not an illustrative list:

- Redis P1 DB is exactly 15; P0 DB is exactly 0;
- namespace is exactly `finam_imoexf_paper:{finam-imoexf-p1}:`;
- service user is exactly `moex-p1-paper`;
- config root is exactly `/etc/moex-finam-p1-paper`;
- state root is exactly `/var/lib/moex-finam-p1-paper/state`;
- binary is exactly
  `/usr/local/libexec/moex/stage8b-p1-paper-supervisor`;
- Redis is loopback-only and every mutating P1 role is DB15/prefix restricted;
- `moex-finam-paper-runtime.service` and `moex-finam-paper-ws.service` remain
  outside P1-f and unchanged.

The checker rejects missing, extra or renamed isolation/evidence fields,
incorrect types and path substitution. DB15 observed empty in the retained
baseline is not authority; O0 must repeat that observation before any mutation.

## 3. Fresh materialization and schedule supply

### O1 to O2

O1 never prepares reusable "fresh" F00 truth. O2 has two subphases under the
same newly issued O2 manifest:

- `P1F-O2-M` may perform only read-only FINAM GET materialization. It cannot
  contact Redis or any order endpoint. The signed manifest binds a
  materialization policy, immutable supervisor-config template and exact
  paths; only `first_boot_source_bundle_sha256` may be finalized from the
  collected bytes.
- `P1F-O2-B` revokes network access and performs the accepted bootstrap through
  AF_UNIX with `PrivateNetwork=yes`. It starts only from a durable
  `ReadyForBootstrap` materialized-set receipt.

The claim identity and materialized identity are intentionally distinct. The
durable Active claim binds the signed policy/template, installation and
deadline before collection. Materialization then advances monotonically:

| State | Exact path | Identity and commit |
| --- | --- | --- |
| `Claimed` | `/var/lib/moex-finam-p1-paper-control/authority/manifests/{manifest_sha256}/claim-receipt.json` | manifest/policy/template/install/deadline/controller plus authority sequence and predecessor head; root-guardian create-new, fsync, hash-chain head commit and reread |
| `SourceCommitted` | `/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json` | SHA-256 of exact F00 bytes; temp+fsync+rename+parent-fsync+reread |
| `ConfigCommitted` | `/etc/moex-finam-p1-paper/supervisor.json` | canonical final config whose sole dynamic field is the exact source SHA; temp+fsync+rename+parent-fsync+reread |
| `ReadyForBootstrap` | `/var/lib/moex-finam-p1-paper-control/authority/manifests/{manifest_sha256}/materialized-set-receipt.json` | hash of policy, source, final config, install, claim and authority history; root-guardian create-new, fsync, append event, reread and cross-validate |

The fixed source and final config are `root:moex-p1-paper 0440`; both parent
directories are `root:moex-p1-paper 0750`. This is readable by the accepted
nonroot service while remaining read-only to it. A stop between source,
config and receipt commits may only finish the deterministic next step under
the same Active claim. It cannot enter O2-B before the final receipt. Any byte,
hash, owner, mode, template or receipt conflict fails closed.

Phase authority is never stored below the accepted service-writable `state`
directory. Its exact sibling control root is
`/var/lib/moex-finam-p1-paper-control`, owned `root:moex-p1-paper 0750` below
`/var/lib root:root 0755`. The `authority`, `events` and `manifests`
directories retain the same root-owned/group-non-writable custody. Authority
receipts are `root:moex-p1-paper 0440`. The runtime UID may read/traverse the
minimum required identities but cannot create, unlink, rename or substitute
any authority directory entry. The existing
`/var/lib/moex-finam-p1-paper/state` remains service-owned `0700` and carries
no phase authority.

The root P1-f guardian is the sole writer. Every signed phase manifest binds an
authority generation, the next sequence and the exact predecessor event hash.
Under one root-owned exclusive lock the guardian creates the manifest-scoped
directory and claim event, fsyncs files and parents, commits and rereads the
hash-chained `history-head.json`, and only then permits a first effect. The
manifest directory plus its claim/terminal chain is the durable spent-manifest
registry while the trusted control root is retained. Missing, corrupt,
substituted, head-only rolled-back or cross-file-inconsistent history then
fails closed and never reconstructs `Unclaimed`. An Active restart must match
the exact history head and original deadline. A newly authorized manifest
binds the retained terminal head and next sequence and creates a distinct
directory without deleting old evidence.

### Authority rollback trust boundary and genesis

The complete control root is the trusted non-rollback authority relative to
all permitted runtime-data, config, evidence and service-state restore
operations. Such tooling must exclude
`/var/lib/moex-finam-p1-paper-control` and fail before mutation when a source,
target or snapshot selection overlaps it. Root operator conduct and the
provider whole-host restore procedure are part of this trust boundary.

A coherent rollback of the entire control root is deliberately **not** claimed
to be locally detectable by the hash chain. A whole-host/control-state restore,
missing control root after activation, or suspected coherent rollback is an
administrative incident: all ordinary guardian starts, claims, restarts and
service starts remain quarantined. Recovery requires a separately reviewed
authority-rebind package, retirement of the old generation in the offline
registry and a distinct generation/ceremony nonce. No such rebind is authorized
by this design correction.

Genesis is a separate one-time operation before the first O1 phase claim, not
an interpretation of missing history. An offline registry outside the VPS
records one Prepared generation and unique ceremony nonce. The root-only
`initialize-authority` command verifies a signed genesis manifest binding the
installation, target host, control root, generation, genesis head and validity
window; commits/fsyncs/rereads the local genesis; and emits a receipt while
ordinary claims remain blocked. The offline operator verifies that receipt,
marks the generation Activated and signs an activation certificate. Only the
exact certificate plus local genesis head admits sequence-1 claim. Before
activation a crash may resume only that genesis transaction; after activation
the genesis command is permanently rejected. Absence after prior activation is
control-state loss, never a fresh installation.

Fresh admission retains the accepted 300-second maximum broker-truth age. If
the source is 301 seconds old before O2-B, the phase becomes `Failed`, bootstrap
performs no mutation, evidence is retained and a new manifest is required.
A long review delay after O1 therefore causes fresh O2-M collection, never a
freshness exception. Recovery is split into two independent accepted
mechanisms. A pre-seal V5 first-boot marker authenticates the exact historical
F00 bytes and allows their continuation after the live freshness window; V5
administrative actions do not read F00. A committed schedule V4 continues its
already-bound effect without a new schedule read. V4 never substitutes for V5
first-boot recovery.

### O3 synthetic supply

O3 uses a production synthetic observation producer and production M10 feeder.
The manifest binds the public fixture digest. The phase clock binds realtime at
claim to monotonic elapsed time; rollback or discontinuity fails closed. Stage4
positions, orders, instruments and schedule observations are explicit
synthetic evidence, refreshed at most every two seconds and passed through the
ordinary production verifier and generation-2 signer. Test-only constructors,
fixture-key verification and bypass capabilities are forbidden in the
operational binary.

### O4 FINAM supply

The market-data feeder remains bars-only. A separate, separately authorized
read-only broker-truth observer performs the required FINAM GET observations
for positions, orders, instruments and schedule. It emits only exact response
hashes and a Stage4 report; it cannot emit M10 or any broker command. Schedule
and Stage4 observations are no more than five seconds apart and observations
refresh at most every 30 seconds. This explicit role does not add account API
authority to the bars feeder.

The schedule publisher emits at most every two seconds, so transport age stays
within the accepted five-second bound. A phase-start health gate may prevent
children from starting, but it is not lifecycle authority. Once I1 is active,
schedule failure is route-specific and preserves accepted predecessor effects:

| Route | Schedule-read frontier | Failure disposition |
| --- | --- | --- |
| Market / generated Market / initial LIMIT / CANCEL | exact `CommandPublished*` owner; callback and command publication may already exist | retain exact published owner and M10 PEL; no provider/truth/XACK |
| Ready Working LIMIT | routed continuation with broker truth and schedule high-water already durable | retain routed owner; no new callback/command/truth/XACK |
| Ready Day expiry | source-free Ready owner after timer due and high-water restore | retain Ready owner; no cancel/expiry/source/XACK |

The design does not claim a universal pre-callback schedule check and does not
alter accepted I1 route ordering.

## 4. Produced artifact contract

| Artifact | Producer / authority | Provenance and identity | Freshness / signer / custody | Output and restart |
| --- | --- | --- | --- | --- |
| phase manifest | offline operator; one O1–O4 phase | accepted tree, target, phase, policy/template/install identities; canonical SHA-256 | not-before/deadline; pinned P1-f Ed25519 public key; private key offline | one durable claim; manifest cannot be claimed twice |
| fresh first-boot source | O2-M under claimed O2 manifest and policy | exact FINAM read-only truth plus accepted history/riskgate; source SHA and operational/account/instrument identity | at most 300 seconds; root:moex-p1-paper 0440 under 0750 parents | deterministic final config input only; same-claim incomplete materialization may continue |
| O2 materialized-set receipt | deterministic O2-M finalizer | policy/template, exact source, final config, install and claim hashes | create-new; fsync file+parent; reread and cross-validate | sole O2-B admission token; partial commits are never bootstrap-ready |
| synthetic Stage4 report | O3 production observation producer | fixture/phase/clock/report hashes | refresh <=2 seconds; embedded in generation-2 envelope | report/schedule input; resume exact high-water |
| FINAM Stage4 report | O4 dedicated GET-only observer | raw-response hashes, checked time, account/instrument and exact sections | refresh <=30 seconds and cross-source skew <=5 seconds; token in root credential | Stage4 report only; refresh after restart |
| signed schedule | accepted normalizer/publisher | envelope hash, source generation, sequence and semantic revision | publish <=2 seconds; `schedule-ed25519-v1` generation 2 via constrained AF_UNIX signer | DB15 schedule stream; Prepared replays exact bytes, Published advances sequence |
| synthetic M10 | O3 production feeder | fixture hash, phase clock and canonical bytes | finalized active-session boundary; no test constructor | canonical M10 only; equal replay idempotent, conflict fails |
| FINAM M10 | O4 M1-to-M10 feeder | exact M1 set, aggregation hash and canonical bytes | finalized bar inside signed schedule | canonical M10 only; resume without duplicate/gap synthesis |
| phase/publisher state | local guardian and publisher | manifest/config/install/target/process identities and publisher high-water | absolute deadline never extends; fsync+reread | audit/evidence only; terminal state cannot restart |

The machine-readable inventory is authoritative and contains all table fields:
producer, phase authority, source/provenance, canonical identity, freshness,
signer/trust, custody, allowed outputs and restart policy.

## 5. Publisher continuity

O3 and O4 use source generation `1`, schedule key generation 2 and one durable
publisher high-water. O4 requires a new phase manifest but does not reset the
publisher. It advances publication sequence exactly once from the retained
Published state; semantic revision changes only if semantic identity changes.
The consumer V4 high-water is retained across the phase transition. Missing or
conflicting state cannot invoke first-publication authority.

## 6. Single-use phase lifecycle and deadline

The lifecycle is:

`Unclaimed -> Active -> Stopping -> Completed | Failed | Expired`.

Before the first effect, the guardian creates a receipt with create-new
semantics, writes and fsyncs it, fsyncs the parent directory, rereads it and
binds manifest hash, source tree, host key, phase, materialization-policy hash,
config-template hash, installation, start/deadline and controller identity. A
concurrent controller loses before effect. O2 source/final-config hashes are
added only through the materialized-set receipt. A crash after claim may resume
only the same `Active` receipt before the original deadline; this is
continuation, not reuse of the manifest.

The deadline begins at durable claim: 1800 seconds for O3 and 10800 seconds for
O4. Restart never extends it. A local guardian/deadline enforcer operates
without SSH. Child starts require the exact Active receipt and stop being legal
at expiry. Clock rollback or untrusted time expires the phase. After reboot no
child starts automatically; stale Active state is expired before any manual
continuation.

On stop/expiry/failure the guardian stops feeder, observer, publisher, signer
and supervisor, then force-kills after a 30-second grace. It preserves DB15,
PEL and durable evidence. Telemetry failure cannot keep a phase running or
cause XACK: a local terminal receipt is persisted and children stop.

## 7. Source-exact Redis role capabilities and P0 protection

All production Redis access is through typed role adapters; no raw Redis
connection escapes. Mutating roles use DB15 and the exact P1 prefix. The
machine-readable inventory maps every accepted public operation to its real
command, key/argument constraints, required role capabilities and response-loss
behavior. The checker proves all ten source operations are reachable through
their exact role capabilities. It pins eight
Lua programs by SHA-256: namespace initialization and verify-only attach, M10
publication, two ordinary command publication/revalidation scripts, two P1-d4
reserved publication/revalidation scripts and atomic stale-consumer cleanup.
Arbitrary `EVAL` is never allowed.

The public fresh-namespace trace is exact and two-step: the provisioner invokes
`namespace-initialization-v1` and then `namespace-verify-v1` over the same two
keys and group arguments. Its role therefore has both pinned capabilities.
Response-loss retry repeats the complete ordered trace and succeeds only after
the verifier returns exact frontiers. The reserved generated-Market trace also
records the `XINFO STREAM` nested authority used to compare the command
stream's `last-generated-id` with the bound predecessor before `XADD`.

The real source semantics are retained: verify-only attach runs the pinned
namespace verifier; canonical M10 publication runs the pinned group-checking
script and resolves response loss with exact-id `XRANGE`; retention admission
uses `XLEN`; the schedule reader performs `XREVRANGE + - COUNT 64` and validates
newest plus bounded progression; stale-consumer cleanup first discovers at
most 64 consumers, examines at most 16 and then atomically rechecks pending=0
and idle>=86400000 before `XGROUP DELCONSUMER`. Source acquisition remains
bounded `XPENDING`/exact `XAUTOCLAIM` before fresh `XREADGROUP`, and source
`XACK` remains last after durable truth. The conformance fixtures include
positive publish/duplicate/response-loss, attach, schedule, hygiene and
recovery traces plus wrong DB/key/script/argument/count failures.

The provisioner alone holds namespace-initialization authority. Only
non-consumed output streams may carry one retained provisioning marker. The
auditor has bounded read-only DB0/DB15 access. Guardian and broker-truth
observer have no Redis capability.

The following are globally forbidden to P1, including fault scenarios:
`FLUSHALL`, `FLUSHDB`, `CONFIG`, `ACL`, `MODULE`, `DEBUG`, `SHUTDOWN`, Redis
save/rewrite/restart operations, migration/restore/move/swap, rename, delete,
unlink, `XDEL` and `XTRIM`. P1 faults stop P1 only and never mutate Redis
process/configuration or act on P0 units.

P0 evidence is not whole-DB equality because P0 may legitimately evolve. It
instead includes before/after hashes for P0 unit/config identities, zero P1-
initiated P0 service actions, the immutable role-command audit, a negative
attempt to mutate an existing non-P1 DB0 key and a positive control where P0
legitimately updates its own DB0 key without a false incident.

Resource limits are checked every five seconds: total PEL at most 64, retained
DB15/evidence budget 512 MiB and at least 10 GiB root free. Crossing a limit
stops P1 while retaining evidence; it never trims/deletes data or reconfigures
Redis. Telemetry and schedule streams use exact MAXLEN 4096 where their accepted
contract permits it.

## 8. Exact restart scenarios

The JSON inventory freezes start frontier, fault, exit, counter delta, PEL and
terminal checkpoint for all six O3 scenarios:

- clean SIGTERM from `PaperReady` with no post-latch effect;
- SIGKILL with an exact pending M10 before semantic effect, then one bounded
  restart through the same receipt and no duplicated callback/provider effect;
- stale PEL reclaim by one `XAUTOCLAIM` before fresh `XREADGROUP`;
- exact duplicate replay with zero additional effect;
- conflicting duplicate failure with source retained pending and zero XACK;
- uncertain provider outcome reconciliation under the exact idempotency key,
  with at most one provider effect and XACK-last after durable truth.

Fault injection selects an ordinary production frontier. It cannot mint a
test-only authority or replace the production callback/provider path.

## 9. Model fixtures and evidence

The checked model file contains 45 positive/fail-closed cases. It adds O2
source/config/receipt crash cuts, byte/hash conflict, separate V5 source and
administrative recovery, distinct V4 schedule continuation, exact Redis attach,
M10 publication/response-loss, COUNT 64 schedule read, stale-consumer cleanup,
retention and all six route-specific schedule frontiers to the existing
authority/deadline/P0/resource cases. R3 additionally proves the complete
initializer trace, reserved-publication `XINFO STREAM` authority, protected
multi-UID custody, consumed-manifest rollback refusal and preserved history
across a newly authorized manifest. R4 separates ordinary duplicate, partial
head/directory rollback, coherent full rollback under the explicit trust
boundary, one-time genesis, repeated genesis and same-Active restart cases.

Evidence inventory is exact: target and installed identities, systemd state,
phase claim/deadline, artifact freshness, publisher/consumer high-water,
role-command audit, DB15 streams/groups/PEL, P0 identity and controls, durable
hashes, health/readiness/runtime state, lifecycle projections, process/deadline/
network facts, resource growth and redacted secret-free logs.

## 10. What acceptance opens

Independent acceptance of this R4 correction opens only `P1F-I` source
implementation. It does not authorize SSH mutation, installation, systemd
reload/enable/start, DB15 provisioning, bootstrap, FINAM attachment, paper
provider operation, broker dispatch, runtime-live or real orders.
