# O2 V4 timestamp installation successor

2026-10-05: INSTALLATION_PACKAGE_REVIEW_CANDIDATE; O2 HOLD.
Preparation is not installation or execution permission. No signing, new timer,
FINAM/Redis call, service start/reload/reset-failed or remote file write occurs.
The only VPS action in preparation was a read-only inventory at 15:48 UTC.
Rust/Cargo, deploy units, historical installers and CI workflows are unchanged.

## Accepted artifact and predecessor

[Artifact/governance review](reviews/REVIEW_1062691_O2_V4_TIMESTAMP_ARTIFACT_RU.txt)
accepts `1062691ee9694e1827e91cd011a686c3b3ca863c`;
compiled source is `f3b349949802abd5eff80cad6b9e9fc37bc327e1`.
Full accepted artifact SHA-256:
`cb831ede43c4b1379a0e128b9240db4c33cb649de1eb1d8467de5213631ae799`.
All three binaries are reused byte-for-byte; no rebuild or single-ELF update.

| ELF | SHA-256 |
| --- | --- |
| materializer | `f954bf98fa24c62dfa669eb159b7dc06717fe5edb61587c5404001ed38bdd0a3` |
| operator | `81e88b129896287e2b6b849043d2dfb09b860ddcd3861d1094f7e76a857b40e7` |
| supervisor | `b70d1eaf222c747909a907de3af565f601c7a2d4f2c8caaed1b0851d8bcfba1b` |

Prior installation package: `moex-trading-project-b8732cc-sparse-o2-installation.zip`,
SHA `d68bfad3a173fd634334b7821eb29445b30ff2ff5a4ec8f4dcf02f24b23f759d`.
Actual installed manifest, confirmed read-only:
`5357fe30f6a43908e7f5fa2ea053b693146d5972a1f108afec59afdcf7bbf9f5`.
Its 15 payload slots plus the manifest remain exact. Public old bytes are
reconstructed from that pinned package and retained as a compact extract.
The artifact ZIP is kept intact; old multi-stage archives are not recursively
copied. No private key, token, account response or Redis dump is included.

## Terminal and scheduler preservation

Actual retained head: **FAILED / generation 1 / sequence 8**.
Latest logical event digest:
`12a71f6278c6d00a48a2f5c0acb9b1d3026e6619fe1d0be99be48762f39ac6ad`.
Retained October-5 bounded evidence SHA:
`ea94bf20deedb4bd1f45581a88fb60281938b33cf43a584fafd4047f0504761d`.
Fresh read-only snapshot SHA:
`d88e1c58f384cf6c8a4a9e6fac3c9f462c6a554e3cafddb9212c6c9c891013bf`.
Snapshot capture command, exit/stdout/stderr hashes, all terminal event/receipt
bytes from the retained report and hashes/custody of protected trees accompany
the package. The fresh head and every retained report event/receipt hash must
match the actual authority inventory. Raw private file values are not exported.

The October-5 timer is still `active/elapsed`, no realtime next elapse,
monotonic next elapse `infinity`, `Persistent=no`; its oneshot service is
`failed/failed`, exit 1, PID/control PID 0, empty cgroup, `Restart=no`.
The new observer recognizes **only these two exact units**, after checking
their installed byte hashes and exact loaded-property snapshot. The unchanged
custody observer still rejects every other unexpected P1 unit. This is a
disclosed installation-preflight delta, subject to this package review, not
permission to alter/enable/rearm/remove the scheduler.

Both old scheduling units, `/root/moex-o2-once-20261005` (including `attempt.json`
and evidence), and the preceding `/usr/local/share/moex/stage8b-p1e/o2-sparse-4668b42`
backup transaction are preserved exactly. New changes, including an old marker
reset or a future timer firing, block installation. P0 properties and unit
hashes are fixed to the fresh observation; legitimate later P0 drift requires
explicit inspection/rebinding, not a silent replacement or restart.

## Exact update and calendar

Eight bounded slots in the existing replacement order:
three ELFs; materialization policy; source calendar template; supervisor template;
compatibility manifest; full installation manifest **last**.
Seven slots change. Supervisor template already has accepted bootstrap schema 2
and is byte-for-byte unchanged (checked, backed up, never needlessly rewritten).
Machine-readable `installation-v4-timestamp/spec.json` binds all exact old/new
hashes/sizes. Genesis ID remains `stage8b-p1f-o2-baseline07-7196aaa-v1`.
Operational identity, source V4 policy constant and no-riskgate V2 profile stay
unchanged. No durable-root migration, generation reset or history repair.

Candidate session: **6 October 2026**. Previous explicit weekday sessions:
30 September, 1 October, 2 October, 5 October. Fetch bounds:
`2026-09-30T04:00:00Z` to `2026-10-06T20:50:00Z`, under seven days.
Candidate close bounds: 07:10–23:50 MSK (04:10–20:50 UTC).
This is a full-session planning envelope, **not a signed phase or run schedule**.
Production selects only a completed fresh candidate/prefix; max candidate age
900s and broker truth 300s remain unchanged. No future or synthetic missing bars.

General hours were rechecked on October 5 against the official
[September schedule](https://www.moex.com/n103379) and
[July extension](https://www.moex.com/n101220). Applying the weekday template
to October 6 is a planning inference, not proof of broker feed availability or
absence of an exceptional halt. An actual attempt still needs schedule/data
validation. Explicit weekday history does not assert that weekend trading does
not exist. If review/installation misses October 6, rebind the calendar and
manifest explicitly; never auto-roll or reuse expired authorization.

## Local proof and later apply

`stage8b_p1f_o2_v4_timestamp_install_package.py` prepares/checks exact inputs.
`stage8b_p1f_o2_v4_timestamp_install_review.py` runs bounded local gates and seals
an immutable full-source ZIP with generated inputs, logs and source commit/tree.
The archive checker can run without Git against the sealed package.

The local gate covers 20 prepared-input negatives (including mixed old/new ELF
sets), 16 Linux/root filesystem fixture tests, 14 injected post-fsync change
frontiers, authority checker/45 negatives, accepted-release rlib calendar probe,
actual-ELF input smoke, syntax/format and unchanged crates/Cargo/deploy/workflows.
Filesystem systemd observations are explicitly mocked in network-none native
ARM Linux. Interruption tests use exceptions/reopening, not SIGKILL or power loss.
Calendar/ELF probes use network-none Linux/amd64 emulation. This is not fresh
FINAM collection, native VPS execution or successful O2. No new full Rust suite
or GitHub Actions success is claimed by these local packaging checks.

Initial packaging gate at `2aaefdd` passed commands 0–6, then stopped before
linking with `ambiguous accepted dependency`: the accepted release-test run had
left multiple rlib variants in the same target directory. The successor gate
selects the exact filenames already retained in the accepted artifact's rlib
manifest and verifies their hashes. No library rebuild or newest-file fallback;
the initial partial run is not labelled PASS. Final evidence is a separate run.

The new fixed updater reuses the existing locks, backup/fsync/journal protocol.
Its own transaction is `o2-v4-timestamp-f3b3499`; historical transaction untouched.
Preflight checks exact stopped/P0/history/config/durable/staging/schedule/backup
inventories before writes and between slots. Resume allows only known old/new
bytes; rollback restores only listed slots, never history or receipts. Incomplete
backup preparation and newer history both stop for inspection.

Next: package review + ordinary current CI → separately permitted stopped apply
→ installation evidence acceptance → separately permitted single bounded O2,
terminal result and stop, no automatic retries. Main/remote branch publication
and GitHub CI are not performed by the offline package scripts.
O2 remains HOLD; exact cause of the previous rejection is still unproven.
O3/O4 retain WS subscribe/reconnect, freshness/REST overlap/EOD and multi-session
ALOR paper parity. FINAM POST/DELETE, broker dispatch, real orders and runtime-live
remain closed. No new recovery framework or roadmap stage.
