# O2 credential successor — installation review candidate

2026-10-08. Prepared locally, **NOT INSTALLED**, no phase/timer/signing/start/reload.
O2 remains HOLD. This is the existing installation step, not a new roadmap stage.

## Accepted bytes and current predecessor

[Independent source/build acceptance](reviews/REVIEW_817985c_O2_CREDENTIAL_SOURCE_BUILD_20261008_RU.txt)
accepts commit `817985c66c321accbe32d712428cf6f9c6ac39d9` and three Linux ELF
files together. ZIP SHA-256:
`50911141f7152a30a64911a7aaaba7c11480f983914e7e9cfcf05f15151f76c0`.
Those exact ELF bytes are reused, not rebuilt. The qualification test ELF is
not part of the installation payload. Existing source/native/Rust qualification
is inherited as accepted evidence, not claimed as newly rerun.

Fresh SSH read-only snapshot: `installation-evidence/snapshot-r2.stdout`.
SHA-256 `daf2f86d9970c636f2d7153f6fc78861d9c5ad7adf71a6a9d1c7f92969631b1b`.
The captured observation is pinned by the installer and contains exact installed
inventory, all authority/history/receipt/archive hashes, configuration/state and
prior transaction inventories, unit files/properties and exhausted schedules.
All 78 protected file hashes from the accepted native witness match. No Redis,
FINAM, credential content read, service action or target write was performed.

Required predecessor:

- installed manifest: `8d69542f1b46c5f6ad2a34173e19f3e8fc7d0e58896cd254e450bf17ca822221`;
- terminal **FAILED / generation 1 / sequence 19**;
- event: `af881a83fc7e64776689564556b5cd9b65ac32b2e0c39ad22297a54fb9f14b66`;
- P1 has no PID/cgroup/job; both P0 services remain running, with exact observed
  PIDs and unit bytes. Historical bootstrap `failed` is not an active P1;
- no transient FINAM or bootstrap credentials; existing lifecycle key is
  observed by custody/inode only, never read/hashed/exported;
- uninitialized durable root and retained FAILED19 generated inputs unchanged.

Do not use the old EXPIRED/1/12 binding or replay FAILED19.

## Five replacements; no change to runtime configuration

`installation-credential/spec.json` lists exact old/new hashes and sizes.
Only materializer, operator, supervisor, `installation-v1.json` and
`installation-o2-v1.json` change. The installation ID, roots, keys, uid/gid/modes,
unit files, sandbox, control code, policy and templates stay unchanged.
New outer binding includes accepted source/artifact, predecessor and installer
hash. The compatibility manifest also binds the new supervisor bytes.

Installer: `scripts/stage8b_p1f_o2_credential_install.py`, using existing
custody/lock/atomic-write primitives. It serializes with existing execution and
guardian locks, verifies every old payload and preservation invariant, retains
root-only before-images and an fsynced journal, then replaces the five slots.
Preflight/status does not create a transaction. Apply/resume/rollback require a
separate explicit confirmation. All steps recheck history/P0/stopped state;
rollback changes installed files only, never terminal history.
Incomplete backup preparation refuses automatic repair. No daemon-reload is
needed because units are unchanged. No new keys, roles, ACL engine or recovery
framework is introduced.

The three generated FAILED19 inputs (source, supervisor config, unused lifecycle
key) deliberately remain **in place** in this installation scope. This package
is not an execution-ready fresh source. Before any separately authorized next
O2, use the already established root-only rename/retention procedure to archive
those inputs under their original custody, verify the prior materialization
receipt, and materialize a fresh set. Never copy the key to review evidence,
silently delete it, loosen its permissions or reuse the failed source/phase.
Keeping this retirement out of the binary replacement avoids mixing operational
phase preparation with installation acceptance.

## Calendar and execution boundary

Exact existing public calendar is retained: candidate session **2026-10-08**,
nominal closes 04:10–20:50 UTC; four prior sessions October 2, 5, 6, 7,
history request start October 2 04:00 UTC. The package is not a reservation or
permission to run. No new exchange-calendar assertion is made here; the existing
accepted calendar bytes are compared exactly. Live schedule/truth, candidate
freshness and source completeness still require fresh operational validation.

If the date/window is missed, explicitly rebind the existing policy/template
and corresponding installation manifest before signing a fresh phase. No
automatic rollover, timestamp refresh, retry or replay is allowed. A binary
installation itself does not consume the calendar or authorize execution.

## Local qualification and review contents

Included Linux filesystem fixtures exercise apply/resume/rollback, all five
post-write crash frontiers in both directions, exact history and generated-input
preservation, drift/lock/custody/backup refusals. Systemd/proc observations are
explicitly mocked in those fixture tests; the SSH snapshot is separate real
read-only evidence. The exact three ELF files also pass the existing isolated
network-none admission smoke with unchanged public policy/config inputs.
Current-tree authority positive/negative checks and protected-source comparison
ensure no Rust/Cargo/deploy/workflow change versus accepted `817985c`.

Completed qualification: 12/12 Linux filesystem/input tests; 23/23 prepared
negative cases; 10/10 post-write frontiers (five slots × resume/rollback);
authority 45/45; exact-ELF public-input smoke PASS. Initial collector attempts
refused because an uninstantiated template cannot be queried with `systemctl
show` and the stopped main service has its existing `Restart=on-failure`.
The final observer pins the template file, checks main PID/cgroup/job and exact
unit properties, and still requires `Restart=no` for bounded callers. No unit
or restart setting was changed. Both failed captures remain in the package.

The historical ELF smoke's stale-profile negative used the old v1 name, which
did not match the current no-riskgate-v2 template. Its original exit 1 is
retained. A generated test-only adapter changes that one sed search literal;
the negative now genuinely changes the input and exits 64 as required, and
the complete smoke passes. Production source/ELFs and the old smoke script
remain unchanged. The adapter and exact commands are included.

ZIP contains full exact-commit source, source tree/commit proof, accepted review,
fresh snapshot and capture source, old/new public inventories, five-slot payload,
test logs and old ELF bytes for offline filesystem-test reproduction. No secret
files, active authorization, operational source/config contents, Redis dumps or
nested historical handoff archives are included. The old public calendar scaffold
is not a captured operational source. Safety verifier reconstructs the source
tree and checks every generated member and exact installation input.

Reproduce on a disposable Linux Docker root, never the VPS:

```sh
docker run --rm --platform linux/amd64 --network none \
  -e MOEX_CREDENTIAL_DISPOSABLE_TEST=1 \
  --mount type=bind,src="$PWD/scripts",dst=/scripts,readonly \
  --mount type=bind,src="$PWD/installation-credential",dst=/package,readonly \
  --mount type=bind,src="$PWD/installation-evidence/old-elf",dst=/old-elf,readonly \
  rust@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922 \
  python3 -B /scripts/test_stage8b_p1f_o2_credential_install.py
```

Next review concerns only this installation successor. After acceptance and
separate permission: stopped installation/postflight; then fresh-phase
preparation and one separately authorized bounded O2. Nonroot bootstrap and
GET-only FINAM remain. FINAM POST/DELETE, broker dispatch, real orders and
unattended/continuous runtime remain closed. O2 success must demonstrate the
actual chain through first boot, durable root and terminal receipt; loader PASS
does not substitute for that result.
