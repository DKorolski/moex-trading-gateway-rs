# O3 unified release / authority / stopped installation

2026-10-09. Review candidate, not operational permission.

Source acceptance: `fb1ceb3dac40b365d0d23983af788ca1c9c72907`, tree
`bf7b2353a8a972ffd5af0ba1fc7e86e7eeb06b31`.
[Independent source verdict](reviews/REVIEW_fb1ceb3_O3_SOURCE_ACCEPT_20261009_RU.txt):
P1-O301 CLOSED. No repeated source/design audit is requested.

## Exact boundary

The builder exports that Git commit, mounts it read-only and builds exactly five
production executables in Linux amd64 release mode, offline/network-none, with
the pinned existing Rust image. Cargo compiler-artifact messages prove no
fixture features/test artifacts. ELF identities, raw commit object, source
tree, logs and build command are retained. Public Cargo cache reuse is not
used as proof of a binary identity: every selected artifact is built and hashed.

No Rust/Cargo/deploy-template/workflow change is made in this package. The
authority manifest only updates exact production/control inventories; it does
not reopen historical Stage5 authority, CI policy or closed surfaces.

## Installation delta

Existing O2 outer manifest SHA:
`4ab7c2592c5427a0ca7aa1fc455caae0649a9e7652d36ee1d0d5feeeda055fef`.
Fresh read-only observation confirms COMPLETED/1/26, event
`fb4fa038d829f0d21e5f2d8e39b1bd26e05ba4f4186b95dfa6205b41d47933d8`.
It is a comparison baseline, not a standing authorization to assume the next
sequence or to reuse the expired O2 phase.

Nine fixed write slots, all root-owned:

| Slot | Before | After |
| --- | --- | --- |
| `/usr/local/libexec/moex/stage8b-p1f-o2-operator` | Accepted O2 executable | Accepted-source O3-capable release, 0755 |
| `…/stage8b-p1f-local-supervisor` | Absent | Release, 0755 |
| `…/stage8b-p1f-o3-composition` | Absent | Release, 0755 |
| `…/stage8b-p1f-o3-signer` | Absent | Release, 0755 |
| `…/stage8b-p1f-o3-provisioner` | Absent | Release, 0755 |
| `/usr/local/share/moex/stage8b-p1e/moex-finam-p1f-o3-supervisor.service.in` | Absent | Accepted disabled template, 0644 |
| `…/moex-finam-p1f-o3-signer.service.in` | Absent | Accepted disabled template, 0644 |
| `…/installation-o3-v1.json` | Absent | Five ELF + two template bindings, 0644 |
| `…/installation-o2-v1.json` | Accepted outer manifest | Exact successor with retained O2 entries, 0644 |

The bootstrap compatibility manifest does not own the operator slot and is
preserved byte-for-byte. Materializer/paper-supervisor, public O2 config/profile,
HMAC metadata, adopted strategy state/journal/seal/marker/receipt and all
authority history remain unchanged. P0 PID/start ticks/program/unit bytes are
compared by the reused observer. No new service account or role.

The installer reuses accepted secure reads, root-owned path checks, dual
flock, fsync and atomic replacement. Explicit absent-before slots are the only
extension: rollback unlinks only an exact known new file. Foreign bytes,
links/custody drift, missing old slots, changed root/history or live P1 fail
closed. Existing backups bind old bytes; mixed state resumes or rolls back.
As in the inherited installer, interruption while initially creating backups
before PREPARED fails closed with untouched managed slots; it is not silently
treated as an applied transaction. No general fault/recovery framework added.

Install writes disabled templates under the existing shared package directory,
**not** executable phase-bound units under `/etc/systemd/system`. It does not
reload systemd, start/enable services, initialize Redis, create O3 producer
state, issue public phase inputs, stage credentials or call bootstrap.

## Inputs and qualification

Exact accepted template bytes are included. `@PHASE_SHA256@` is the sole render
field. A zero hash used in the disposable systemd negative test is deliberately
NOT an operational phase. There is no signed O3 input set in this bundle.
Fresh public phase/composition/initial-schedule bytes must be selected and bound
against the preserved O2 root and current clock at the later operational gate;
reusing the source witness or an expired window is forbidden.

Local tests use the actual release bytes and real Linux paths/locks/permissions.
Tests cover preflight/apply/resume/rollback, every replacement frontier, new-slot
rollback frontiers and custody/foreign-byte/state/history/lock failures.
The host observer is mocked only in filesystem unit tests, explicitly labelled.
A separate disposable real systemd PID1 loads both accepted unit templates,
checks syntax/loaded properties, paper-UID credential separation and release
CLI/ABI. **Dynamic systemd execution is UNQUALIFIED:** in this local x86
container both signer setup and systemd-journald exit 255. This is not the
expected missing-phase runtime refusal 70 and is not counted as its PASS.
An additional native ARM diagnostic reaches systemd but cannot execute x86
ELFs (203/EXEC); that console diagnostic is context, not retained acceptance.
The exact units are not weakened to work around the local environment.

The bundle preserves the **incomplete** `qualification-r5` command records;
it does not label that attempt an aggregate PASS or claim a completed run of
the subsequently revised driver. File tests (14 tests including 16 replacement/
new-slot rollback subcases) and CLI/static/custody command results are separate
from the unqualified dynamic launch. Later PID1 attempts were also unsuccessful.

Local tooling incident: privileged native-systemd diagnostics removed the Docker
VM's x86 binfmt registration. VPS was unaffected. The unintended Python 3.12
handler was removed and amd64 support restored using the already cached
`tonistiigi/binfmt:qemu-v10.0.4` image
`sha256:8f58e6214f4cc9dc83ce8f5acad1ece508eb6b20e696a8c1e9f274481982c541`.
A Linux amd64 execution check passed; Docker and the existing ALOR buildkit
container were not restarted. The exact previous handler inventory was not
captured, so byte-for-byte restoration is not claimed. The revised qualification
driver masks systemd-binfmt, but a successful rerun is not claimed here.
No further local privileged systemd experiments are required by this handoff.

This is **not** a positive active-phase systemd Market/restart run. The accepted
source witness supplies the local production-feature debug lifecycle proof;
operational O3 and its 13 rows remain NOT_RUN. Private signing/HMAC keys are not
used for release qualification. No service has been loaded on the VPS for O3.
The package is reviewable source/build/installer evidence with an explicit
remaining native x86 systemd qualification, NOT a claim of fully qualified
deployment. Agree the narrow native qualification boundary at package review;
do not start a VPS test or broaden installation permission implicitly.

## Handoff and next authorization

`o3-release/descriptor.json` identifies source and packaging commits separately.
The offline verifier reconstructs both Git trees and commit objects, checks
full inventory/ELF/preimage hashes and qualification records. Sidecars report
archive SHA-256 and safety. These checks are integrity/binding evidence, not
an assertion that a locally generated PASS replaces independent review.

After package acceptance, request **stopped installation without activation**.
Before writing, repeat exact read-only preflight under the existing locks;
any mismatch stops the install. After installation evidence acceptance, request
one bounded O3 with fresh phase/input bytes, retained O2 root and selected local
signer custody. Do not transfer ceremony/private inputs in the handoff.

No push/main merge or three-hour CI run is needed for each intermediate step.
Synchronize at the accepted logical checkpoint. O4 retains FINAM read-only WS
subscriptions, freshness/reconnect/REST overlap/EOD and multi-session ALOR
parity. FINAM POST/DELETE, broker dispatch, DB0 changes, runtime-live and real
orders remain closed.
