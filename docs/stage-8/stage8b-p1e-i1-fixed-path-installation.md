# Stage 8B-P1-e I1 fixed-path installation and systemd material

Status: source/material review candidate. Aggregate I1 and operational
activation are not authorized.

The first immutable candidate `37b9d06` received SOURCE/MATERIAL HOLD for
P1-INS01, P1-INS02, P2-INS03 and P2-INS04. This correction keeps the accepted
units and Rust lifecycle unchanged and closes only those installer/evidence
findings.

Accepted predecessor: telemetry source correction
`b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac` (SOURCE ACCEPT).

## Boundary

This slice turns the already accepted deployment identity and process binary
into a reviewable Linux package. It adds no strategy, lifecycle, Redis or
broker semantics. The package consists of:

- `/usr/local/libexec/moex/stage8b-p1-paper-supervisor`;
- three units under `/etc/systemd/system` for `run`, one-shot `bootstrap` and
  one-shot `bootstrap-recover@`;
- one persistent `moex-p1-paper` sysuser;
- fixed configuration, credential and state directories from deployment
  identity V2;
- a non-activating install/status/rollback transaction.

The package has no `[Install]` section. The installer never runs
`daemon-reload`, `enable`, `start` or any supervisor mode. It contacts neither
Redis nor FINAM.

## Operator material

The installer deliberately does not accept, create or overwrite:

- `/etc/moex-finam-p1-paper/supervisor.json`;
- `/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json`;
- `/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key`.

Their exact production creation and validation remain a later operational
ceremony. Keeping them outside package installation prevents a checked-in
secret, a default config or a generated first-boot authority from entering the
I1 source/material gate.

## Unit split

The ordinary unit executes only fixed-path `run`. It uses
`Restart=on-failure`, a 600-second/5-attempt start limit, SIGTERM control-group
shutdown, a 100-second stop deadline and final SIGKILL. It permits AF_UNIX,
AF_INET and AF_INET6 but applies `IPAddressDeny=any` with only IPv4/IPv6
loopback exceptions; the application still enforces exact port 6379 and DB15.

Bootstrap and recovery execute only their fixed commands and confirmations.
Both are AF_UNIX-only with `PrivateNetwork=yes`, have no restart policy and
cannot reach Redis. All units share the fixed service identity, systemd
credential, empty capability sets, strict filesystem protection, disabled
coredumps and writable access only to the fixed state directory.

## Install and rollback semantics

`scripts/stage8b_p1e_i1_fixed_install.py` requires effective uid 0, an
explicit absolute non-symlink `--root`, and for installation one regular,
owner-executable, non-group/world-writable binary. Public files are copied via
same-directory temporary files, fsync and exclusive hard links. Existing
targets are accepted only when bytes, owner, group, mode and link count are
exact. Directory custody is reread after sysusers/tmpfiles execution.

The manifest is committed last and states explicitly that no activation,
daemon reload, Redis contact, FINAM contact or operator-material installation
occurred. Repeating the exact transaction is idempotent.

Rollback verifies every managed hash and refuses when any operator material or
durable-state evidence exists. It removes only public package files and the
installed binary. The persistent service identity and directories remain, and
no config, credential or durable state is deleted.

The manifest is data, never deletion authority. Its schema, canonical bytes,
fixed keys, exact six-file inventory, digests, root ownership, mode, link count
and protected ancestor chain are checked before status or rollback can regard
the installation as exact. Rollback performs a second complete preflight and
unlinks only the compile-time fixed inventory through verified directory file
descriptors. Extra, missing, aliased or noncanonical manifest paths are
rejected before the first mutation.

The target root and all existing protected ancestors are real root-owned
directories without group/world write access. Managed files and the manifest
must retain exact type, ownership, mode and single-link custody. An existing
service identity is accepted only as one unique non-root user/group pair with
the exact primary group and nologin contract; an absent pair may be created by
the fixed sysusers file, while a partial or root-equivalent pair fails before
tmpfiles mutation. The input binary is checked before canonicalization, opened
with no-follow semantics, required to be single-link and copied from a stable
descriptor with source/destination digest equality.

The state directory may contain only the exact quarantine directory, and that
directory must have exact custody and be empty. Nonempty quarantine history,
a symlink, wrong type or an unreadable/drifted state boundary refuses rollback
without changing package, manifest or retained state.

## Verification

The source gate cross-checks all fixed paths and commands against
`stage8b-p1e-deployment-identity-v2.json`, checks shared and mode-specific
hardening, rejects `[Install]`, and inspects the installer for activation or
network clients. Its mutation harness rejects 25 source/material and ten
retained-evidence weakenings.

The target-Linux runner uses Ubuntu 24.04/systemd 255 with an isolated network
namespace (the reproducible local runner uses a container with
`--network none`). It performs clean install, target `systemd-analyze verify`,
exact reinstall, operator-material rollback refusal, durable-state rollback
refusal and clean public-package rollback. It never starts a unit. Evidence is written under
`reports/stage8b-p1e-i1-fixed-install/` and is included only in the immutable
handoff, not in production paths. A 14-case behavioral filesystem matrix uses
the real root CLI to cover manifest extra/missing paths, custody/mode/owner
drift, writable and symlink ancestors, source symlink/hardlink, conflicting
UID/GID 0 identity, and nonempty/symlink/wrong-type quarantine. Every negative
case snapshots the watched package/state surface and proves that refusal makes
no change; an exact restored baseline and empty-quarantine rollback are the
positive controls.

The checked-in runner archives the exact accepted source ref, records archive
SHA-256 and tree identity, builds the release binary from that archive with a
pinned Rust image, writes build result from the actual exit code and digest,
runs the complete isolated rehearsal, and finishes by running the mandatory
source-plus-evidence checker. Its invocation and full output log are retained.
Missing artifacts or any failed build/rehearsal/gate command prevent the final
PASS marker.

The retained target evidence installs the real release supervisor built from
accepted telemetry/process source `b6f6d5b`, verifies its SHA-256 before and
after installation, and binds that source ref and digest in the evidence JSON.
The `/bin/true` fixture is not accepted as final binary-binding evidence.

## Explicitly closed

- operator config/source/credential provisioning;
- service-manager reload, enablement or start;
- operational Redis DB15 or DB0;
- VPS installation;
- paper-provider activation;
- FINAM POST/DELETE/send and broker dispatch;
- runtime-live and real orders;
- aggregate I1 acceptance and P1-f.
