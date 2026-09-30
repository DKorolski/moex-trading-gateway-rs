# O2 non-activating installation / replacement candidate

Date: 2026-09-29. Status: R1_NATIVE_OBSERVER_CORRECTION_REVIEW_CANDIDATE.

Package `304cd56` was independently INSTALLATION PACKAGE ACCEPTED. Its separately
user-authorized operational attempt stopped at native preflight, before calling
install: systemd 255 omitted `ExecStart` for the two not-found O2 units. Only
root-only package staging changed on the target. Exact O1 remains installed;
no replacement transaction or new manifest exists. This local R1 correction is
not deployed and requires review before a resumed installation. See the
[native correction and evidence](stage8b-p1f-o2-installation-native-correction.md).

## Accepted predecessor and present scope

Independent ARTIFACT ACCEPT applies to `7196aaac7c0bf45a03d90742d8ef483078649de6`,
ZIP SHA-256 `1069cbeb597e902126ebdb0b2c01ef52dca45420dfe2a69a6963b939de33ac4a`.
The acceptance review SHA-256 is
`84971a4cee1e0e38168c9fc1fe9005481131ae3d82f652fa703c9d0fe5c97187`.
This package prepares the installation boundary requested by that review. It
does NOT exercise permission to deploy the corrected installer. The earlier
304cd56 attempt used SSH, root-only staging and read-only Redis observations;
it performed no managed-file replacement, FINAM, signing or service operation.

No new Rust build or strategy changes: the installer reads the exact accepted
ZIP and copies its payloads/public templates/units without transformation.
The accepted I1 fixed-install custody helper is reused unchanged. The original
O1 installer remains unchanged; it is not used to force-overwrite its old files.
There is one narrow Python replacement transaction, not a second runtime or
general deployment platform. Historical O1 documentation and receipts remain
immutable in Git and in the accepted nested artifacts.

## Inventory and installation identity

`stage8b-p1f-o2-installation-package.json` records the complete new inventory,
exact installer/helper hashes and canonical installation identity SHA-256.
There are sixteen installed files (including two manifests), three new empty
directories and twelve changed/new file frontiers:

- three accepted Linux/amd64 executables, root:root `0755`;
- five service units, including unchanged ordinary/bootstrap/recovery units;
- accepted sysusers/tmpfiles, including O2 public/staging directories;
- four public O2 inputs: policy, source template, supervisor template and public
  authority key; root:moex-p1-paper `0440` in the protected O2 directory;
- the existing `/usr/local/share/moex/stage8b-p1e/installation-v1.json` updated
  to the new supervisor/tmpfiles hashes, preserving its compatibility shape;
- new `/usr/local/share/moex/stage8b-p1e/installation-o2-v1.json`, binding all
  payload bytes/custody, directories, predecessor and installer identities.

`group=service` in the machine inventory means the unique accepted
`moex-p1-paper` primary group resolved from protected passwd/group databases.

The new installation ID is `stage8b-p1f-o2-baseline07-7196aaa-v1`.
Future separately authorized signed O2 documents must bind this ID and SHA-256
of the canonical new manifest, not the old O1 installation hash. The recorded
inventory SHA in the package must match the installed manifest byte-for-byte.
The old manifest bytes/hash and all seven old managed files/manifest are
retained as root-only before-images under `o2-replacement-7196aaa` in the same
share directory. No rollback recreates old bytes by guessing or downloading.

## Preconditions and failure policy

Operational CLI is native Linux x86-64 `/` only, GLIBC >= 2.34, exact pinned
VPS SSH host public-key fingerprint. It has no target override, SSH transport,
forced-overwrite switch, alternate root or mock-observer CLI option.
The accepted O1 manifest SHA is
`dab3e1426e40acb5101a8743cce5aac5aec8d3ae54d81fb73ac33c9bb18d464f`.
Every old managed file must match its exact bytes, root owner/group, mode and
single-link regular-file contract. Additional target payloads/directories must
be absent before the first transaction. Even an empty unexpected directory is
not automatically adopted; stop and review the actual target difference.

P1 ordinary/bootstrap/materializer/runner units must be stopped with zero
main/control PIDs, no job, no drop-ins and no enabled unit files. Unknown or
failed observation commands reject; a nonzero `is-active` is never accepted as
stopped proof. Recovery instances/unexpected P1 units reject. P1 service-UID
processes or matching root executables reject. Existing cgroups must be empty.
The P0 unit configuration/PID observations are read and compared before/after;
the installer does not act on P0. DB0 is never contacted. DB15 emptiness belongs
to the separate fresh operational preflight; this installer does not assert it
without an observation and never connects to Redis.

The durable state must contain only its empty accepted quarantine directory.
Operator config/source/credentials, transient inputs, genesis, activation,
selector or materialized state block installation and rollback. Files are not
removed to make a target pass. All native command calls have a ten-second bound.

## Transaction, receipt and limited rollback

An exclusive installer flock serializes replacement/recovery actions. The root
operator must also reserve a maintenance window with no concurrent privileged
activation/provisioning. This is not a defense against an adversarial root user.
Fresh observation and exact bytes/custody are repeated before each mutation.

Durably copied before-images and canonical PREPARED journal precede target
payload writes. Each payload replacement uses a same-directory temporary file,
owner/mode setup, file fsync, atomic rename, parent fsync and exact reread.
The compatibility manifest follows payloads; the complete new manifest is last.
Only full verification and an unchanged observation permit APPLIED. `status`
then returns `EXACT_O2_INSTALLED_NOT_ACTIVATED` with new installation identity.
No old-hash current installation identity is reported as a successful update.

After interruption at a completed file boundary, `resume` accepts only the
known old/new combination under the exact retained journal and before-images.
Foreign bytes, missing old files, custody drift or an unexpected journal stop.
Rollback from PREPARED/APPLIED durably marks ROLLING_BACK, restores exact old
bytes, removes only exact newly installed files and empty new directories, and
finishes ROLLED_BACK. Service identity, durable parent and forensic history are
retained. ROLLED_BACK cannot be reinstalled by replaying this transaction.

This is a bounded pre-activation rollback, not an arbitrary crash-repair tool.
Incomplete backup staging, an interrupted temp-file write, or a directory
creation interrupted before final custody can remain fail-closed for manual
inspection. They are not silently pruned, reclassified as success or forcefully
adopted. No automatic rollback is attempted after authority/materialized/durable
state appears or the P0 observation changes. Review the failure and retained
files; do not start bootstrap to test a partial installation.

## Reproduction and evidence limits

Local gate command (paths are inputs, not authority):

```sh
python3 scripts/stage8b_p1f_o2_install_gate.py \
  --artifact accepted-artifact/moex-trading-project-7196aaa-stage8b-p1f-o2-execution-artifact.zip \
  --old-o1 accepted-artifact/old-o1.zip \
  --output tmp/o2-install-evidence
```

The Docker runner is pinned by image ID, uses Linux/amd64, network none and
mounts only public scripts and accepted ZIP fixtures read-only. It tests actual
old/new binary bytes, files, UID/GID/modes, symlink/hardlink rejection, flock,
exact install/replay/rollback and twelve injected durable-file interruptions.
Native observation response tests cover stopped/active/PID/job/enabled/unknown
query/foreign-host cases; `/proc` fixtures cover service and root executables.
These observations are explicitly substituted. `systemd_manager_tested=false`:
neither a real manager transition nor target compatibility/PIDs/stopped proof
are claimed. Exceptions at durable file boundaries are not OS SIGKILL/power-loss
tests. The raw gate log records actual test results.

The gate compares protected source/deployment bytes directly with the accepted
nested artifact, so it also runs from an extracted ZIP without `.git`. The
pinned local Docker image must already be available; the gate does not install
or silently replace that prerequisite. The handoff contains both exact accepted
ZIP inputs, raw Git commit/tree evidence, this review and fresh test logs.

The Linux suite now has 17 test methods, including native not-found response
regressions and narrowly scoped rejection controls. The 45/45 negative cases in
the gate are the inherited current-tree authority harness, not 45 new behavioral
installation tests. Real VPS read-only responses are retained as a fixture;
the corrected installer itself has not been run on the target.

The installer has no `systemctl start/stop/reload/enable`, daemon-reload,
systemd-sysusers/tmpfiles execution, FINAM, Redis, signing or broker-send call.
Sysusers stays unchanged; explicit empty-directory creation is the only
provisioning needed in addition to exact file replacement.

## After package acceptance — still needs explicit user permission

1. Confirm a fresh read-only target/O0 preflight (including DB15 and current P0)
   and independently verify the package SHA and public target identity.
2. Stage the accepted ZIP on the correct host in root-only custody, retaining
   `.github`/all public source and nested `accepted-artifact/` members. No secrets
   belong to this package. Use the reviewed installer from that root-owned tree.
3. Run `preflight`, then only if separately authorized `install --confirm
   INSTALL_ACCEPTED_O2_7196AAA_WITHOUT_ACTIVATION`. Retain stdout/stderr, journal
   and before/after read-only evidence; require a fresh `status` result.
4. If necessary before activation, separately authorize `rollback --confirm
   ROLLBACK_O2_7196AAA_BEFORE_ACTIVATION`. No force or cleanup workaround.
5. Review the actual installation evidence. Only then separately authorize
   signing, any necessary daemon-reload, fresh GET materialization and one
   network-isolated bounded O2 bootstrap. Collect real stopped proof, terminal
   receipt and durable-root/adoption evidence. O3/O4 paper windows follow that;
   ordinary runtime-live and FINAM writes/live micro remain separate gates.
