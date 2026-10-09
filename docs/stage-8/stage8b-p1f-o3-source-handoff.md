# O3 source review: fixed composition and local production proof

2026-10-09. **SOURCE REVIEW CANDIDATE, NOT operational O3 acceptance.**
Predecessor: preparation ACCEPT `bc9813434b35238e9e6e861476cab2aaf3fc72a2`.
O2 remains CLOSED at installation `2d67937`, historical COMPLETED/1/26.

## Reviewer entry

Review the delta from bc98134, not the accepted migration history. Main changes:
fixed O3 child and guardian claim, constrained AF_UNIX schedule signer,
separate fresh DB15 initializer, retained Published V2 decision/successor
window, and ordinary semantic continuation after canonical P1-d2 Market truth.
No fixture intent injection or callback-clock override in production.
The detailed [source progress](stage8b-p1f-o3-source-progress.md) retains failed
attempts and the narrow corrections rather than relabelling them successful.

Read the ZIP's `o3-source/descriptor.json`, then:

- `evidence/components/result.json`: 23 exact selectors + fmt, production
  build, strict clippy and diff check = 27 recorded passing checks.
- `evidence/production/result.json`: one completed paper Market lifecycle,
  both M10 source rows XACKed, Hybrid position +1, ACK/truth sequences 1/2,
  SIGKILL of the exact child, ordinary restart with identical journal/seal/
  Redis rows/high-water, bounded graceful parent stop (exit 0, no forced kill).
- Production Cargo artifact messages and ELF hashes distinguish production
  libraries (no fixture features) from the separate fixture input generator.
  These are debug-profile Linux ELF identities, NOT a release-installation
  artifact; no executable binaries are included in the source ZIP.
- `o3-source/source-binding.json` binds both recorded checked-source snapshots
  to this commit. All Rust/Cargo, witness scripts and other qualified inputs
  must match exactly. Only the explicitly enumerated documentation/packaging
  additions may differ after qualification; differences retain old/new hashes.

The full core suite (1302 passed) and durable suite (433 passed, 17 ignored)
were supplemental console checks, not retained-log acceptance evidence.
All 13 operational matrix rows remain NOT_RUN. A successful local normal
Market+restart proof does not prove every rare VPS crash frontier.

Offline verification, without Rust/Docker/network/keys:

```sh
python3 -B scripts/stage8b_p1f_o3_source_handoff.py verify PATH_TO_O3_SOURCE_ZIP
python3 -B scripts/test_stage8b_p1f_o3_source_handoff.py
```

The tree-reconstruction unit test needs a Git checkout; archive verification
does not. The ZIP binds its full source Git tree including executable modes,
all evidence bytes, both qualified snapshots and the actual result/log hashes.
The 7 packaging tests include source drift, altered service commands/retry,
secret filenames, invalid rendering and Git mode drift. Safety scanning is
an inventory/filename/PEM check, not a mathematical proof of absence of secrets.
Service templates passed these static contract checks only: systemd-analyze
was not available in the local qualification image, and dynamic systemd
execution remains NOT_RUN until the separately reviewed installation gate.

## Minimal fixed-path packaging

The two `deploy/stage8b-p1f-o3/*.service.in` templates are **disabled source
templates**. They have no timer/Install section/retry or initializer hooks.
Only `@PHASE_SHA256@` may be rendered to one lowercase 64-hex hash.
Rendering creates files in a NEW LOCAL directory; it does not install them.
The package verifier pins and checks the templates. Systemd syntax verification
is separate from dynamic service/custody qualification; no systemd service has
been run on the VPS for O3.

Fixed executable slots (root-owned 0755; future reviewed artifact hashes):

| Target under /usr/local/libexec/moex/ | Role |
| --- | --- |
| stage8b-p1f-o2-operator | Existing authority; new guardian-claim-o3-fixed only |
| stage8b-p1f-local-supervisor | Root guardian, drops fixed child to paper UID |
| stage8b-p1f-o3-composition | Paper UID, sole Published-window/runtime child |
| stage8b-p1f-o3-signer | Root, constrained gen2 schedule signing only |
| stage8b-p1f-o3-provisioner | Separate explicit admin, fresh DB15 only |

Do not deploy the fixture example or invoke bootstrap. The installation
successor must bind release ELF hashes, service bytes and old/new inventory;
the operator source was covered by component clippy/all-targets, not invoked
by the local run7 production-runtime build. Do not claim run7 qualified its ELF.

Public inputs, root-owned regular files (0440, paper group, no symlinks):
`/etc/moex-finam-p1-paper/o3/{phase-manifest.signed.json,composition.json,initial-schedule.json}`,
the existing supervisor.json and O2 authority-public-key.hex.
Composition JSON pins the two synthetic observations, source receipts,
schedule semantic identity and unchanged runtime profile. No operational
phase/input set is issued in this source package.

Producer directory:
`/var/lib/moex-finam-p1-paper-o3-producer`, paper UID/GID, 0700.
It is new producer state, NOT a new durable strategy root.
Signer socket directory `/run/moex-finam-p1-o3` is root:paper 0750;
socket 0660. Signer checks peer UID, admitted phase, identities and deadline.
Selected generation-2 schedule credential is root-only tmpfs input,
loaded into the fixed signer credential path by systemd. No ceremony folder,
phase-authority signing key, broker token or private seed enters the payload.
The paper child must fail to read either schedule-key copy.

Lifecycle credential is different from the schedule signing key.
Retain its exact accepted bytes; stage only that existing key in the dedicated
tmpfs `/run/moex-finam-p1-o3-runtime-credentials` (paper UID/GID 0700,
`stage8b-p1-lifecycle.key` paper UID 0400). This preserves the already used
child-readable credential-directory seam after the root parent drops UID.
Do not rotate keys or expose root-only signer credentials to fix read access.
The templates leave this explicit preparation to a separately authorized
installation/execution procedure; they never copy or chown secrets themselves.

## Future operation order — NOT executed by this package

1. Source review; exact release artifact and stopped installation qualification.
   Fresh read-only preflight must match accepted O2 root, marker, receipt,
   history seed, installed inventory, P0, authority head and empty DB15.
   Snapshot COMPLETED/1/26 is historical, not permission to assume sequence 27.
2. Choose fresh decision/successor timestamps against the existing strategy
   state and real clock; issue one O3 phase with exact plan/config hashes.
   Do not reuse the local run7 fixture seed/root or its expired phase.
3. Explicit `stage8b-p1f-o2-operator guardian-claim-o3-fixed` as root;
   preserve history/generation. Start the constrained signer with that hash
   only after active-claim admission. Check its socket and key isolation.
4. As paper UID: `stage8b-p1f-o3-composition prepare-initial-schedule HASH
   PREPARE-FRESH-O3-SCHEDULE`. Retain its exact stdout public envelope;
   install that exact envelope as initial-schedule.json with required custody.
   Prepared checkpoint must already be durable. Do not reserialize or sign it
   again. No Redis effect is allowed before this checkpoint.
5. As root: `stage8b-p1f-o3-provisioner initialize-fixed HASH
   INITIALIZE-FRESH-O3-DB15`. It rejects missing/different checkpoint or a
   nonempty inventory; no FLUSH, rewind or repeat initialization on restart.
6. Start the fixed guardian service once. It executes only the paper-UID
   composition child. Runtime attach is verify-only. The child restores
   producers, rereads Redis, admits typed Published window in the SAME process,
   then runs ordinary paper settlement, truth commit and source XACK-last.
7. Stop guardian normally; inspect terminal receipt/head and all descendants,
   then signer and temporary credentials. No new claim/retry on failure.
   Signer exit 70 after independently proved terminal revocation was observed
   locally and is recorded as 70, not hidden as success. While ACTIVE, signer
   failure is fatal and must never be accepted as a harmless terminal exit.
   Do not use O2 cleanup-fixed against an O3 phase; recovery, if needed, must
   use the existing reviewed guardian terminal mechanism and exact evidence.

Guardian max 1800 seconds from claim, deadline not extendable, child budget
5 starts/600 seconds, drain 30 seconds. Systemd 40-second stop timeout is an
outer backstop, not a new guardian grace. No automatic signer/parent restart.

## Honest qualification boundary and next steps

Run7 used a disposable LOCAL root made by ordinary production bootstrap,
recent explicitly synthetic history and the real gen2 schedule trust set.
It did NOT replace/rebootstrap the accepted VPS O2 root. It proves the source
composition, not that a selected operational fixture produces a trade against
the actual O2 seed. That binding requires the later fresh preflight.

Current-tree governance authority still describes the accepted predecessor.
No new authority/CI/GitHub acceptance is claimed by this source handoff.
After source acceptance: narrow authority/release-artifact closure, stopped
installation and separately authorized bounded O3 using the existing O2 root.
No per-patch main merge is needed here.

Then O4: read-only FINAM bars/WS subscriptions, reconnect, freshness, REST
overlap and session/EOD timing, followed by frozen ALOR operational comparison
over several paper sessions. FINAM POST/DELETE, broker dispatch, DB0 changes,
runtime-live and real orders remain CLOSED. No new orchestration framework.
