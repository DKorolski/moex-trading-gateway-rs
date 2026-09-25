# Stage 8B-P1-f R1 isolated operational acceptance design correction

Status: **DESIGN CORRECTION REVIEW CANDIDATE — NO ACTIVATION**.

Correction parent: `58bb4cafd3eb80f43c8d0bfd182f7be18ea92d00`.
The independent R0 review is bound by SHA-256
`ea5b184ad7c412e63229c8b98bacda895da151c09fc666057dc512968cc24ba5`.
Stage 8B-P1-e I1 remains CLOSED / ACCEPTED at governance closure
`3f171d997de5616cb9a07311d7776e446456c0c1`.

This correction changes design, model fixtures and fail-closed review tooling
only. It performs no SSH mutation, installation, Redis command, systemd action,
FINAM request or process start. The 64 matrix rows below are design obligations,
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
  contact Redis or any order endpoint. Exact source bytes and SHA-256 are
  added to the already durable Active phase receipt.
- `P1F-O2-B` revokes network access and performs the accepted bootstrap through
  AF_UNIX with `PrivateNetwork=yes`.

Fresh admission retains the accepted 300-second maximum broker-truth age. If
the source is 301 seconds old before O2-B, the phase becomes `Failed`, bootstrap
performs no mutation, evidence is retained and a new manifest is required.
A long review delay after O1 therefore causes fresh O2-M collection, never a
freshness exception. Marker-bound historical continuation remains available
only for an already committed V4 transaction and does not perform new schedule
admission.

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
within the accepted five-second bound. Missing/stale schedule or expired Stage4
evidence blocks fresh admission before callback/source acquisition and retains
the PEL.

## 4. Produced artifact contract

| Artifact | Producer / authority | Provenance and identity | Freshness / signer / custody | Output and restart |
| --- | --- | --- | --- | --- |
| phase manifest | offline operator; one O1–O4 phase | accepted tree, target, phase, inputs/config/install identities; canonical SHA-256 | not-before/deadline; pinned P1-f Ed25519 public key; private key offline | one durable claim; manifest cannot be claimed twice |
| fresh first-boot source | O2-M under claimed O2 manifest | exact FINAM read-only truth plus accepted history/riskgate; source SHA and operational/account/instrument identity | at most 300 seconds; hash bound to signed receipt; root 0400 | one O2-B input; stale input fails the phase |
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
binds manifest hash, source tree, host key, phase, inputs, config, installation,
start/deadline and controller identity. A concurrent controller loses before
effect. A crash after claim may resume only the same `Active` receipt before
the original deadline; this is continuation, not reuse of the manifest.

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

## 7. Exact Redis role capabilities and P0 protection

All production Redis access is through typed role adapters; no raw Redis
connection escapes. Mutating roles use DB15 and the exact P1 prefix. The
machine-readable role list freezes command forms and keys for provisioner,
synthetic feeder, FINAM bars feeder, schedule publisher and supervisor. The
provisioner uses the accepted pinned namespace-initialization Lua for the M10
and command streams/groups; only non-consumed output streams may carry one
retained provisioning marker. It cannot substitute ad-hoc group creation. The
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

The checked model file contains 20 positive/fail-closed cases for: 301-second
F00 expiry, long review wait, stale/missing schedule, expired Stage4 evidence,
publisher restart, O3-to-O4 continuity, committed V4 continuation, two
controllers, manifest replay, crash after claim, SSH loss, restart before/after
deadline, expiry during Redis wait, telemetry failure, P0 negative/positive
controls, forbidden Redis administration and resource pressure.

Evidence inventory is exact: target and installed identities, systemd state,
phase claim/deadline, artifact freshness, publisher/consumer high-water,
role-command audit, DB15 streams/groups/PEL, P0 identity and controls, durable
hashes, health/readiness/runtime state, lifecycle projections, process/deadline/
network facts, resource growth and redacted secret-free logs.

## 10. What acceptance opens

Independent acceptance of this R1 correction opens only `P1F-I` source
implementation. It does not authorize SSH mutation, installation, systemd
reload/enable/start, DB15 provisioning, bootstrap, FINAM attachment, paper
provider operation, broker dispatch, runtime-live or real orders.
