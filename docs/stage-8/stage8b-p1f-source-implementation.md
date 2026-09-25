# Stage 8B-P1-f I — source implementation

Status: `REVIEW_CANDIDATE_SOURCE_ONLY_NO_ACTIVATION`.

Accepted predecessor: P1-f R4 design at
`5d81b8e212300858246237a227a95d115dd67c2d`. This slice implements the
local root-guardian authority and its executable proofs. It does not install
files, start a service, connect to Redis or FINAM, dispatch broker commands, or
authorize paper/live execution.

## Authority and custody

`Stage8bP1fAuthorityStoreV1::open_production` opens only
`/var/lib/moex-finam-p1-paper-control` and requires effective UID 0. Every
transition reacquires an exclusive nonblocking guardian lock and rechecks the
opened root device/inode against the named root. Authority directories are
`root:moex-p1-paper 0750`; retained documents are `0440`; the lock is `0600`.
Created directories and files receive explicit UID/GID ownership, independent
of root's primary group. Reads use `O_NOFOLLOW`, require regular single-link
files, cap document size, and compare named and opened inode identity.

The service UID has read access to the retained receipts required for child
admission but no write, unlink, rename, chmod, create, parent-substitution or
guardian-lock authority. `scripts/stage8b_p1f_multi_uid_custody_harness.sh`
executes these checks under two real Linux UIDs. Concurrent guardian ownership
and retained-manifest tampering are independently covered by Rust tests.

## Genesis and phase history

Genesis is a signed canonical JSON transaction:

1. validate the exact target/control-root/generation/time window;
2. retain `genesis-transaction.json` as the crash-resumption selector;
3. commit the signed manifest, local receipt, genesis event and head;
4. accept only the matching signed activation certificate;
5. refuse ordinary phase claims until activation exists.

Activation may resume after either side of its durable head/certificate seam,
but only with the exact certificate. Repeating genesis after activation fails.
Ordinary claim cannot create or reconstruct missing genesis state.

Each phase manifest is canonical JSON with Ed25519 domain separation. Its hash
names a retained manifest directory and is bound into a strictly increasing
event chain. Every history read validates schema/domain, sequence,
predecessor, event hash, receipt hash, retained manifest bytes, manifest
inventory and head. A duplicate active request returns the original receipt
and deadline. A different active request, spent manifest, head rollback,
missing claim directory or modified retained manifest fails closed.

Create-once pending markers make claim, O2 materialization and terminal
transitions resumable only as the same transaction. Terminal evidence and all
consumed manifest directories are retained.

## O2 and bounded admission

O2 accepts only a claimed `O2_MATERIALIZE_BOOTSTRAP` manifest whose signed
policy and template hashes match the supplied exact bytes. The template must
contain the fixed source-hash sentinel; the sole finalized field is
`first_boot_source_bundle_sha256`. The resulting supervisor config is parsed
through the accepted P1-e validator, and the source bundle through the
accepted V5 first-boot source parser. This does not alter committed schedule
V4 recovery.

Source and config are written `0440`, fsynced, atomically renamed, reread and
bound into `materialized-set-receipt.json` plus the authority event chain.
Admission requires exact reread of both files and broker truth age in
`0..=300` seconds. Stale truth cannot mint a run permit.

`Stage8bP1fRunPermitV1` is linear: it is neither cloneable nor serializable.
It binds one active manifest, original claim/deadline and monotonic admission
instant. Wall-clock rollback starts ordered stop. Deadline or clock failure
never returns to Continue; after exactly 30 seconds the decision is
`ForceKill`. The permit grants no Redis, FINAM, broker or process-launch
capability.

## Restore boundary

`execute_stage8b_p1f_permitted_restore_v1` normalizes every source, target and
selected path and rejects equality, ancestor or descendant overlap with the
trusted control root before invoking the mutation callback. A declared or
suspected coherent whole-host/control-root restore writes a permanent
quarantine receipt. No clear/rebind API exists in this source slice. Hidden
coherent rollback is deliberately not claimed locally detectable.

## Executable evidence

The source gate runs:

- formatting and strict package Clippy;
- all P1-f guardian unit/crash/replay tests;
- inherited P1-e V5 freshness, V4 schedule and Redis capability tests;
- the source checker and mutation harness;
- the Linux real-multi-UID custody harness when run in the documented root
  container.

The immutable handoff records the exact source commit, tree, gate log and
multi-UID log. Operational P1F-O0 through O4 and aggregate P1F-A remain
separate review boundaries.

## Closed surfaces

- target installation or systemd activation;
- VPS or Redis DB15/DB0 mutation;
- paper-provider execution;
- FINAM attachment, POST, DELETE or send;
- broker dispatch, runtime-live and real orders;
- authority clear/rebind;
- P1F-O0, O1, O2, O3, O4 and P1F-A acceptance.
