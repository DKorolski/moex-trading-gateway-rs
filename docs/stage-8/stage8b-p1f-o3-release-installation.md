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

No Rust/Cargo/workflow change is made. Five release ELF bytes and their build
records are reused exactly from `e7f8599`, not rebuilt. The only deployment
template directive change is supervisor `User=root` -> `User=` (system-manager
default root). Signer template, installer, capability bounds and sandbox are
unchanged. The original package's authority inventory closure is retained;
this correction does not alter authority/CI policy or closed surfaces.

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
| `/usr/local/share/moex/stage8b-p1e/moex-finam-p1f-o3-supervisor.service.in` | Absent | Corrected disabled template for review, 0644 |
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

Exact qualified template bytes are included. `@PHASE_SHA256@` is the sole render
field. A zero hash used in the disposable systemd negative test is deliberately
NOT an operational phase. There is no operational O3 input set in this bundle;
native evidence contains only the public synthetic test's expired envelope.
Fresh public phase/composition/initial-schedule bytes must be selected and bound
against the preserved O2 root and current clock at the later operational gate;
reusing the source witness or an expired window is forbidden.

Local tests use the actual release bytes and real Linux paths/locks/permissions.
Tests cover preflight/apply/resume/rollback, every replacement frontier, new-slot
rollback frontiers and custody/foreign-byte/state/history/lock failures.
The host observer is mocked only in filesystem unit tests, explicitly labelled.
The original disposable systemd PID1 attempt loaded both old unit templates,
checked syntax/loaded properties, paper-UID credential separation and release
CLI/ABI. **Historical dynamic execution was UNQUALIFIED:** in that local x86
container both signer setup and systemd-journald exit 255. This is not the
expected missing-phase runtime refusal 70 and is not counted as its PASS.
An additional native ARM diagnostic reaches systemd but cannot execute x86
ELFs (203/EXEC); that console diagnostic is context, not retained acceptance.
That exit255 is retained and is not retrospectively relabelled as PASS.

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

## Native qualification correction (2026-10-09, run3)

The user separately authorized the narrow unit correction and **one** new
isolated attempt with the same selected generation-2 schedule key. That attempt
passed on native x86_64/systemd 255.4. Earlier unsuccessful attempts remain local
diagnostics; only run3 is the retained positive start/stop proof.

In this qualified environment, explicit `User=root` with NNP left the guardian
without effective CAP_SETUID (0x60 vs bounded 0xe0), preventing child launch.
Using the system-manager default root (`User=`) restores the exact bounded
guardian capabilities without adding ambient/inheritable authority. The child
still drops to UID/GID987, clears supplementary groups, has no effective,
permitted, inheritable or ambient capabilities, and retains NNP1. This is an
environment-specific observation, not a general claim about all systemd versions.
Group, bounds, restart/stop policy and other sandbox directives are unchanged.

Retained `o3-release/native/` proof includes the exact executed driver and its
support module, all **55** command records/stdout/stderr, actual process
identities/capabilities, journals and independent read-only host snapshots.
The offline verifier checks exact source/template/ELF bindings, record inventory,
live-child identities, graceful-stop result, empty cgroups and equal snapshots.
These are retained developer observations, not externally signed attestation.

- Actual production signer started using systemd credentials and signed the
  initial synthetic schedule with the pinned generation-2 public identity.
- Actual production provisioner created 12 keys in a **disposable Redis DB15**.
  Actual production guardian launched exactly one production composition child;
  both signer and guardian remained active over the eight-second observation.
- Bounded stop completed in **0.493s**: `GRACEFUL_STOP`, child exit0, no forced
  kill or restart, all captured PID/start-tick identities gone, cgroups empty.
- After synthetic phase terminalization the signer refused further operation
  with **exit70**. This status is retained, not translated to success or hidden
  with `SuccessExitStatus`. Synthetic receipt COMPLETED/1/3 is not host O2/1/26.
- Selected schedule key went over private SSH stdin directly to `/run` tmpfs
  (no private stdin/hash retained). Swap was disabled. Explicit key/credential
  deletion and both container removals passed; no key bytes enter the package.

The test container had private PID/cgroup/network namespaces, no host bind mounts
and `network=none`, with bounded memory/CPU/PIDs/core0 and a six-minute shutdown
backstop. Added SYS_ADMIN/NET_ADMIN/BPF/PERFMON and unconfined seccomp/AppArmor
supported nested systemd sandbox tests; these are **test-container settings**,
not additions to either service or the accepted host. Mount propagation was
first made private, then shared within the disconnected container namespace so
systemd credentials reached services. Binfmt/udev/module services were masked.

`input-helper` and `bootstrap-source-witness` are explicitly separate synthetic
setup tools from earlier source evidence, not substitutes for the five release
ELFs. Their identities are recorded; their binaries are not duplicated in this
package. The captured driver is historical execution evidence, not an authorized
one-command rerun: its local import/support paths need reconstruction and any
new credential use/remote attempt needs fresh permission.

Full host before/after snapshots are exactly equal: accepted P0 processes/units,
O2 root/history COMPLETED/1/26, installed 15 slots and seven absent O3 slots.
Host binfmt/container inventories are equal as well. No accepted host unit,
payload, Redis or FINAM endpoint was changed/contacted by the test. Local
installation payloads were regenerated only to bind the corrected template.

This closes the requested **native start/stop qualification evidence**, not a
Market/restart operational O3 run. All 13 operational rows remain NOT_RUN.
Independent review of the corrected template/full installation bundle comes
now; host installation and operational O3 remain closed pending separate gates.

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
