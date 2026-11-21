# O2 first-boot Replay successor — installation review candidate

2026-10-08. **PREPARED / NOT INSTALLED. O2 HOLD.**

## Accepted lineage and scope

[SOURCE ACCEPT](reviews/REVIEW_09dfecb_O2_FIRST_BOOT_REPLAY_20261008_RU.txt) and
[ARTIFACT ACCEPT](reviews/REVIEW_09dfecb_O2_FIRST_BOOT_REPLAY_ARTIFACT_20261008_RU.txt)
apply to compiled source `09dfecbe931267c5276b8b805d62d5a5a141ad95`.
Accepted artifact SHA-256:
`9a6fafec0e6dc70f73bbfd973c60303fc37e38c20a2219bb8cc136a07f04aaf9`.
Reuse its three exact ELF bytes, without recompilation. No Rust/Cargo, deploy,
workflow, model parameters, calendar, service identity or permission changes.

The new installer is the prior five-slot credential installer with fixed
successor pins, retained-directory/absent-credential inventory extended for the
last attempt, and public P0 executable/start-time plus libc observations. No new
keys, roles, guardian or recovery framework. The historical installer is not
edited or reused with mismatching FAILED19 pins. Existing locks, fsync,
before-images, bounded resume/rollback and fail-closed drift checks are retained.

## Fresh read-only target binding

Snapshot at **2026-10-08 14:06:27 UTC**: P1 stopped, no jobs/cgroups or runnable
timers; P0 runtime PID `762243`, WS PID `208156`, both still running.
Linux x86-64 / GLIBC 2.39 meets the ELF maximum requirement GLIBC 2.34.
Every installed payload hash/size/mode matches its outer inventory. Public P0
executable fingerprints/start ticks, unit bytes/properties, account IDs and
all protected inventories are pinned. No private key/token/environment content
was read; retained credential files were checked by custody/inode metadata only.

Required predecessor:

- outer installation: `782b651cb5e2cca5a2d4c8a6c611656df67492fc4e113378b056fe252bbe5a9b`;
- terminal **FAILED / generation 1 / sequence 23**;
- history event: `1864813e6375c89b92cb174e01a9fc52e58fc9ce133f117280d46820e7e96bbe`;
- exact canonical observation: `75f7d71b96d2f4aeb3395a3dd089fc6965dce71c8ee3e6b191191fcac37d2247`.

Fresh head/control/state/retired-input inventory and P0 identities are also
compared to the accepted artifact's last-observed public evidence. The final
collection must equal the initial snapshot, except observation time. Any later
drift stops apply for explicit reassessment; a snapshot is not a lasting permit.
No SSH staging, signing, apply, reload, enable/start, Redis or FINAM call occurs
in this preparation. SSH itself is read-only and may generate normal host logs.

## Exact five-slot update

`installation-replay/spec.json` binds old/new hashes and sizes. Only materializer,
operator, supervisor, `installation-v1.json` and `installation-o2-v1.json` change.
New outer installation SHA-256:
`4ab7c2592c5427a0ca7aa1fc455caae0649a9e7652d36ee1d0d5feeeda055fef`.
It binds accepted source/artifact, installed predecessor and exact installer.
The compatibility manifest changes only supervisor hashes; historical non-ELF
fields remain unchanged. The outer inventory governs actual unit/template bytes.

All 11 remaining public payload slots, P0, terminal history/receipts, staging,
retired inputs, previous transactions and exhausted timers are preserved.
Current FAILED23 source/config/key remain **in place**. A separately authorized
fresh-phase preparation must retire them with the existing procedure; they must
not be reused or silently deleted. This installer never initializes a durable
root, signs authority, activates Redis DB0/P1 or sends broker orders.

Existing calendar bytes remain: candidate session October 8; prior sessions
October 2, 5, 6, 7; REST history start October 2 04:00 UTC; allowed candidate
closes October 8 04:10–20:50 UTC. This is not a new market/calendar assertion or
reserved execution window. If missed, stop and explicitly rebind the existing
policy/source/supervisor/manifest before a fresh phase. No rollover or extension
of consumed authority. A non-activating binary update alone does not authorize
use of that calendar.

## Local qualification and handoff

The package includes full exact-commit source, Git reconstruction proof,
accepted reviews, initial/final read-only captures and their executed collectors,
five-slot payload, old public inventory and old ELF fixture bytes, and complete
local test logs. No real credentials, Redis dumps or nested old handoff ZIPs.

Completed locally: 12/12 tests, 23/23 prepared negatives, 10/10 post-write
frontiers, authority check and 45/45 authority negatives, protected-source diff
and exact-ELF smoke PASS. Final read-only capture at 14:11:44 UTC equals the
initial observation except time. Installed files, P0, terminal history and
retained inputs did not change during preparation.

The existing 12 filesystem/input tests cover exact prepared inputs, 23 negative
cases, all ten post-write frontiers (five slots × resume/rollback), history/P0
drift, input preservation, locks, invalid custody and corrupt/incomplete backups.
They use real Linux files/ownership/fsync in a disposable container; only
systemd/proc observations are mocked. Exact three-ELF admission smoke is also
rerun with the accepted test-only profile-literal adapter. Runtime source
qualification is inherited as accepted, not rerun or relabelled. GitHub CI is
NOT_RUN; no push/merge is part of this preparation.

Reproduce the filesystem gate on a disposable Docker root (never the VPS):

```sh
docker run --rm --platform linux/amd64 --network none \
  -e MOEX_REPLAY_DISPOSABLE_TEST=1 \
  --mount type=bind,src="$PWD/scripts",dst=/scripts,readonly \
  --mount type=bind,src="$PWD/installation-replay",dst=/package,readonly \
  --mount type=bind,src="$PWD/installation-evidence/old-elf",dst=/old-elf,readonly \
  rust@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922 \
  python3 -B /scripts/test_stage8b_p1f_o2_replay_install.py
```

Next: installation review → separately authorized stopped update/postflight →
separately authorized fresh phase and one bounded O2. O2 must demonstrate fresh
materialization → ReadyForBootstrap → authenticated V5 root (one Replay callback,
zero requests/journal) → terminal receipt/stopped proof. WS continuity,
reconnect/freshness/EOD, multi-session ALOR parity and live micro remain later
steps. Artifact/source acceptance does not substitute for operational evidence.
