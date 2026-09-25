# Stage 8B-P1-f R0 isolated operational acceptance design

Status: **DESIGN REVIEW CANDIDATE — NO ACTIVATION**.

Accepted predecessor: Stage 8B-P1-e I1 governance closure
`3f171d997de5616cb9a07311d7776e446456c0c1`. The accepted aggregate source is
`a9bcd940635b62c2a13f8d378453e6ca21511e30`; its independent review digest is
`c5be3e66cadd6c45f6678669afa3f9a194c31f6288aae1c3d29fa59aac47ca54`.

This commit is design, inventory, checker and handoff material only. It does
not install files, touch Redis, reload systemd, start a service, contact FINAM
or mutate the target VPS.

## 1. Goal and boundary

P1-f proves the accepted P1 durable paper composition under a real service
manager and real loopback Redis before any operational DB0 or live-order work.
It does not redesign the strategy or lifecycle. ALOR remains an external
behavioral oracle; it is not copied into the FINAM architecture.

Success means a bounded `PaperReady` process consumes canonical M10, emits
paper intents and deterministic paper ACK/order/trade/position truth, survives
the accepted restart cases and stops cleanly with reviewable evidence.
`LiveReady`, FINAM order endpoints, broker dispatch and real orders are always
fail closed.

## 2. Exact target and coexistence

The target is `stage8b-p1f-isolated-vps-1` at `45.150.11.252`
(`nektodk1.ispvds.com`), SSH ED25519 fingerprint
`SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo`. The retained baseline records
Ubuntu 24.04.4, x86_64, systemd 255, 2 CPUs, at least 3.5 GiB memory and at
least 20 GiB free root storage.

The existing `moex-finam-paper-runtime.service` and
`moex-finam-paper-ws.service` are out of P1-f scope and must remain unchanged.
They use the existing P0 DB0 contour. P1 uses only initially empty DB15, the
prefix `finam_imoexf_paper:{finam-imoexf-p1}:`, user `moex-p1-paper`, config
root `/etc/moex-finam-p1-paper`, state root
`/var/lib/moex-finam-p1-paper/state` and the fixed accepted binary path.

Redis remains one loopback-only protected Redis 7 process on port 6379 with
AOF enabled. Sharing the process does not grant cross-database authority: P1
configuration is byte-exact DB15, and every phase proves that no P1 namespace
appears in DB0. A nonempty DB15 or pre-existing P1 path before first
provisioning is a stop condition, not an adoption path.

## 3. Phase order and authority

Operational work is split so that a successful phase cannot automatically
start the next one:

1. `P1F-I` implements provisioning, preflight and evidence tooling locally;
   no remote mutation.
2. `P1F-O0` runs an immutable read-only target preflight and emits evidence;
   no remote mutation.
3. `P1F-O1` installs accepted public material and provisions exact DB15 keys,
   streams, groups, config/source/credential custody; it cannot reload,
   enable or start systemd units.
4. `P1F-O2` performs the one accepted AF_UNIX-only, PrivateNetwork bootstrap,
   rereads the durable root and stops. The ordinary service remains disabled.
5. `P1F-O3` runs a maximum 30-minute synthetic M10 paper session and the six
   bounded restart scenarios, then stops and disables the service.
6. `P1F-O4`, only after independent O3 acceptance, runs a maximum 180-minute
   read-only FINAM-bars session. The market-data process is separate from the
   supervisor and has no order/command-consumer authority.
7. `P1F-A` aggregates accepted operational evidence. Only its later independent
   acceptance may propose the next roadmap boundary.

Each O1–O4 phase requires a separate single-use signed phase manifest binding
the accepted source tree, exact target SSH fingerprint, phase ID, evidence
directory and expiry. No manifest is reusable across a phase or source tree.
There is no unattended phase escalation.

## 4. Provisioning and bootstrap

P1F-I may reuse the accepted fixed installer but must add fail-closed
operational composition rather than weakening it. O1 must preserve exact file
ownership/modes, provision the concrete deployment manifest and all required
DB15 streams/groups before any service, and prove the ordinary process is
still disabled and inactive. Operator config, first-boot source and lifecycle
credential are delivered through separate root-owned custody and never enter
Git, the handoff ZIP, shell output or evidence.

Bootstrap is a separate O2 action with `PrivateNetwork=yes` and AF_UNIX only.
It cannot contact Redis or FINAM. Success requires exactly one authenticated
durable root, no transaction temp, an accepted receipt and a clean authenticated
restart readback. Any incomplete frontier uses only the accepted explicit
recovery unit after a separately issued recovery manifest.

## 5. Synthetic operational matrix

O3 uses signed schedule evidence and canonical synthetic M10 only. It proves:

- clean SIGTERM and exact stopped telemetry;
- SIGKILL at the accepted pre-effect frontier and restart without duplicate
  callback, publication or provider effect;
- stale PEL reclaim before any fresh read;
- exact duplicate idempotence;
- conflicting duplicate fail-closed with source retained;
- uncertain paper-provider outcome recovery without repeated effect.

Every scenario starts from an explicitly named retained checkpoint and has a
maximum runtime. The service is stopped and disabled after evidence capture.
DB15 and durable state remain intact until independent review; cleanup is a
later explicit action.

## 6. Read-only FINAM bars

O4 is not opened by synthetic success. Its separate review must bind a
market-data-only credential and a feeder that can publish only canonical
market-data/schedule input in the P1 DB15 namespace. The P1 supervisor remains
free of FINAM dependencies. The feeder has no command consumer, no account
order/trade/position API, no HTTP POST/DELETE and no lifecycle credential.

The phase first proves gap-free derived M1-to-M10 provenance, then feeds the
same accepted P1 M10 contract used by O3. It records bar identity, session
chronology, runtime-state transitions and paper lifecycle evidence for later
comparison with the ALOR oracle. A read-only token or feed failure degrades and
stops the phase; it never enables order transport.

## 7. Evidence and stop conditions

Each phase retains a canonical manifest of source/target identity, installed
files, service security/activation state, DB15 keys/types/groups/PEL, absence
of the P1 prefix in DB0, durable metadata/hashes, health/readiness/runtime
state, M10 and lifecycle projections, exit/restart facts and process network
endpoints. Logs are redacted and scanned for credentials and raw secrets.

Immediate stop conditions include identity/fingerprint drift, nonempty DB15 at
first provisioning, any pre-existing P1 path, DB0 P1-prefix activity,
non-loopback Redis, unexpected key/group, manifest mismatch, `LiveReady`,
`paper_only=false`, FINAM order/command authority, ambiguous PEL, duplicate
effect, unbounded restart or missing evidence. Stop means no next phase;
preserve evidence and state, stop/disable P1, leave P0 services untouched.

## 8. What design acceptance opens

Independent acceptance of this R0 package opens only P1F-I source
implementation. It does not authorize SSH mutation, installation, systemd
reload/enable/start, DB15 provisioning, bootstrap, FINAM attachment or a paper
session. Each later authority is separately immutable and reviewable.
