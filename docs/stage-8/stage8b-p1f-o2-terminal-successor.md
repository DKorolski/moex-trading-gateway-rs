# O2 full successor after accepted terminal recovery

Date: 2026-09-30. Status: ARTIFACT_AND_UPDATE_REVIEW_CANDIDATE.
No VPS operation, signing, FINAM call or operational Redis activation in this slice.

## Accepted predecessor

`FINAM_O2_TERMINAL_RECOVERY_REVIEW_2026-09-30.md`, SHA-256
`86d30c59b13f5681703abe3b03096f823f008fb3b1ea7cf814a58878a951950d`,
accepts operational ZIP SHA-256
`828fb41cabc590054590a1744587f71cc84b603fff5d44d3021fcdef841e813b`.
One cleanup committed Expired generation 1 / sequence 2; old phase manifest
`b8ce96287e82a1268a3617c3e29bf8ec0167014bd216c544011c07b709e3b3b8`.
Receipt SHA-256 `334b4163ab4229d38b4aaf7ad0c5fdbed27c1587b9aaacb37aa000de3c9a5f43`.
Event digest `de24431f55599787383247ae55d85ec186072ca7ab6b8d547b1d8243a6e90382`.
P0 and installed bytes unchanged, P1 stopped. This is retained evidence.

## Artifact and unchanged semantic boundary

Build ref `589b80144adaa4c615aaa94781035d5a6af64c71`, tree
`9809a954866d60a3dbc0892906f9458e7ad7a6ca` is the accepted synchronized merge.
The existing offline pinned builder produces all three Linux/amd64 release ELFs:
materializer, operator and supervisor. Raw build log/commit and a reconstructed
Git tree are included. No Rust/Cargo, units, CI, public inputs or profile change
is introduced by this preparation. Historical artifact/installer pins remain
unchanged; they are not relabelled as the new artifact.

Full artifact: `moex-trading-project-589b801-o2-terminal-successor-artifact.zip`,
SHA-256 `a671a9b6e238ae9ec4540b29f413ea4a15fedf00c1d4744c5aa20eb9e3d48923`.
The final review package also includes the six-file update payload and exact
accepted terminal evidence. Binary digests and the new installation hash are in
`stage8b-p1f-o2-terminal-update.json`; do not derive identity from a filename.

Baseline07 BO-only, candle-start model time and canonical close-bound identity
are retained byte-for-byte. The accepted diagnostic projection is included in
the rebuilt materializer. A future failure must retain its safe reason/count/
timestamp projection; raw responses/tokens/accounts are not evidence. BarsTruth
coverage/freshness/calendar gates are not loosened to make bootstrap pass.

## Installation identity and exact six-file transaction

Preserve `installation_id=stage8b-p1f-o2-baseline07-7196aaa-v1`: this is the
genesis-bound lineage identifier, not a claim that the binary revision is old.
The new artifact hash, build ref, update revision, predecessor-manifest hash
and full inventory produce a new installation SHA. Future phase manifests bind
that SHA and event 2; old phase documents and receipts remain unchanged.

Only three installed binaries, the materializer service file, the compatibility
manifest and the full O2 installation manifest are replaced. All other payload
files are checked against the exact old inventory. The two lock-file writable
mounts already exist in accepted source; the old installation lacks them.

`stage8b_p1f_o2_terminal_update.py` is NOT the old empty-root installer. It holds
the existing execution then guardian lock inodes, nonblocking, through the whole
transaction. Read-only invariants bind exact accepted authority/config/state/
staging inventories and the host/P0/stopped observation. Signed documents are
hashed in place, not copied to backups or output. Pending/new claim/config/source/
credential, changed host/P0, unknown bytes/modes/links or incomplete observation
refuse without widening the accepted frontier.

Backups and journal are root-only under
`/usr/local/share/moex/stage8b-p1e/o2-terminal-successor-589b801`.
Before the first payload replacement, exact old bytes are saved and fsynced and
PREPARED is durable. Replacement uses existing secure atomic-write/fsync/reread
helpers; manifests are last. Interrupted payload writes permit only exact old/new
bytes. `resume` completes forward; `rollback` restores exact backups, retaining
the same terminal history. A new authority frontier forbids both. Incomplete
backup preparation or unknown temporary/transaction entries fail closed for
inspection; they are not deleted or guessed at automatically.

No daemon-reload, reset-failed, unit start/enable, claim, genesis or activation
occurs. Disk-unit update and systemd-manager reload remain distinct: after
separate installation authorization, review the before/after inventory first;
reload and new O2 require their explicit operational gate. P0 is never stopped.

## Tests and proof limits

- Fresh amd64 ELF smoke: custody/admission rejection and baseline07 supervisor
  config on the actual three rebuilt binaries, network none, no authority inputs.
- Real systemd 255 mount proof runs natively on ARM Linux in an isolated container.
  Production unit bytes are unchanged; only ExecStart/logging use a fixture probe.
  Positive opens/flocks both exact same lock inodes; authority/config stay EROFS
  even when fixture DAC permits owner write. Two controls remove one mount each
  and require the corresponding lock to fail EROFS. Empty capabilities and
  NoNewPrivileges remain enforced. This proves Linux mount semantics, not native
  x86-64 systemd or real FINAM materialization. Docker privileged PID1 is confined
  to a private cgroup namespace, no host binds, no Docker socket and network none.
  systemd-binfmt is masked; post-test amd64 launch must still work.
- Initial amd64-emulated systemd exited before the probe. An initial ARM probe
  incorrectly demanded EROFS for a DAC-readonly file; fixture modes were corrected
  to distinguish mount denial from DAC denial. Earlier unmasked container setup
  disturbed the local Docker VM's amd64 binfmt registration; registration was
  restored and the masked fixture passed, including its post-test amd64 check.
  Retain these failures; none involved the VPS or production paths.
- Linux/root update fixtures exercise real files, fsync, flock, custody and
  atomic replacement. Host/systemd observations are explicitly mocked. Reopen
  after six durable replacement frontiers in forward/rollback directions (12
  cases); these are injected exceptions after fsync, NOT SIGKILL/power-loss claims.
- Inherited Rust source acceptance is unchanged; no new all-features FINAM PASS
  is asserted. The known unrelated all-features baseline limitation remains.

## Review and later operational sequence

The outer review ZIP is a complete source snapshot of the preparation commit;
the nested full binary artifact is compiled from the separately identified
accepted merge `589b801`. `handoff-commit.txt` distinguishes both refs. Run
`python3 scripts/stage8b_p1f_o2_successor_review.py check /path/to/review.zip`
from its extracted source to recheck CRC, safe members, raw Git tree/commit,
artifact/terminal pins, gate logs and six-file payload binding without any VPS.
The `gate` subcommand is for a clean Git checkout and the local fixture inputs
recorded in its `commands.json`, not an operational command.

The systemd fixture image is a pre-existing local ARM image, ID
`sha256:9bec3cfe280a732eea281cb1add79f75523d1fe2ac86c12b129e4bc2509ec163`;
it is not claimed to be pullable from a registry. Its construction recipe is the
Ubuntu/systemd Dockerfile block in
`scripts/stage8b_p_r2b_implementation_r0_r1_linux_runner.sh` (do not run that
historical runner for this task). A fresh apt-based image rebuild can differ:
record its actual digest and rerun the probe; do not reuse this PASS by renaming
an image. The accepted evidence embeds the actual container inspection and unit
probe commands. The unprivileged filesystem tests use the same image and their
exact bind inputs/command are in the final gate record. The old `7196aaa`
artifact is the fixture predecessor, never an instruction to reinstall it.

1. Accept this full artifact/update candidate, its new manifest, mount proof and
   bounded tests. No additional generic recovery framework is proposed.
2. Obtain separate permission; fresh target preflight must still match exact
   Expired/2 accepted history and unchanged P0. Verify the immutable review ZIP
   before extracting, use a new root-only staging directory and never copy
   unsigned/unreviewed bytes into the installed locations.
3. Use only reviewed `preflight`, then one `apply` with explicit confirmation
   `UPDATE_O2_AFTER_EXPIRED_2_WITHOUT_ACTIVATION`. Retain stdout/exit and exact
   before/after inventories. Any unexpected state stops; no automatic target fix.
4. Only after installation acceptance/reload admission request one new bounded
   O2 with a fresh signed phase tied to the new installation hash and event 2.
   No execution permission or fresh authority is included in this artifact.
5. Successful O2 precedes bounded paper windows/ALOR comparison and later live micro.
