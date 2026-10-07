# O2 external-file custody correction — 7 October 2026

Status: SOURCE REVIEW CANDIDATE. O2 remains HOLD; no deployment or retry.
Baseline: `e05b4bfae3971459053ab22149aa88b2b5e9382c`.

## Observed failure and exact scope

The October-7 authorized timer fired at 07:15 MSK. Both preflights passed,
FINAM session auth returned 200, claim committed ACTIVE/1/11, and read-only
materializer returned 0. `guardian-materialize-fixed` then returned 70
(`InvalidCustody`); `cleanup-fixed` returned 70 (`PendingRecoveryRequired`).
No bootstrap ran. The window expired at 07:25 with pending sequence 12, not a
terminal receipt. ACTIVE is a retained authority state, not a running process.

Read-only result package: `finam-o2-template-bounded-result-20261007.zip`, SHA-256
`6b0349bd9eab8d20c12a10d2fb94556ecf13906f7879b70b450c0b9189cbab5a`.
P0 properties/PIDs and accepted installation stayed unchanged; four P1 units
stopped, DB15 empty, transient auth/lifecycle credentials absent.

The external source temp was root:987, mode **0400**, one link, 1,876,641 bytes.
Its SHA-256 `f4c135d7f3fec1a2164c4dfbf0aa9f6f960830acb79373fb9f7450f2763f9952`
equals the pending record and exact staged source. Final source/config and
ReadyForBootstrap were absent. With UMask=0077, create mode 0440 becomes 0400.
The writer performed fchown but omitted the explicit fchmod already used by
the authority-file writer. This is a local file-publication defect, not another
BarsTruth rejection or reason to relax bar/source validation.

Production change: one function, `write_external_or_require_exact`, explicitly
sets 0440 on its newly created, exclusive, no-follow descriptor. This precedes
write, fsync, crash injection, custody validation and rename. fchmod errors
remain errors. No mode relaxation or path override is introduced. Existing
temp/final files still require exact custody and bytes; old 0400 files are NOT
silently repaired. No runtime/strategy, schema, Cargo, unit, timer, CI, credential,
authority-manifest or cleanup semantics change.

## Tests and boundary

Four new tests cover:

1. Production V4 materialization and exact replay after temp and receipt crash
   points in isolated child test processes under umask 0022, 0027 and 0077.
   Process-global umask never changes in the parent test runner. Fixture roots
   explicitly use provisioned 0750, independent of mkdir's creation mask.
2. Seven existing-file negative cases: wrong temp/final mode, foreign temp/final
   bytes, oversized temp, symlink and hardlink. No bytes, inode or mode repair.
3. Reopen after fsynced temp and lost-response exact replay, preserving inode.
4. Exact pending/source-0400 incident after the deadline: materialization and
   cleanup refuse it, preserving head, pending, temp bytes/mode; no receipt,
   next event, execution owner or final config/source is minted.

The last test is a safety witness, **not** proof that terminal recovery has been
implemented. Expired operational pending recovery is the separate proposal below.
The source gate also removes just the production fchmod block in a disposable
tree and requires the new umask test to fail. No accepted source/remote files
are mutated by that negative control.

Reproduce from this candidate's clean commit:

```sh
python3 scripts/stage8b_p1f_o2_external_mode_review.py gate tmp/o2-mode-review
python3 scripts/stage8b_p1f_o2_external_mode_review.py package tmp/o2-mode-review
```

Evidence includes debug guardian/systemd regressions, release O2 guardian tests,
doctests, strict affected-crate Clippy, formatting, the negative control and exact
source-tree binding. This is targeted coverage, not a rerun of all historical
macro-stage gates. macOS fixtures do not claim a new native Linux ELF acceptance.
Current-tree authority is deliberately still pinned to the accepted predecessor:
the one guardian production-file drift is recorded as REBIND PENDING, not PASS.

## Separate expired-pending terminal recovery proposal

Status: REVIEW REQUIRED; not implemented or operationally authorized here.
Changing chmod and retrying the old live path is not this proposal.

Inputs must bind the actual retained frontier, not a caller-selected path:

- phase `992be63a6406153eb5bab43d746927df598d59e24df2d113ae12c605752b80b5`;
- head ACTIVE/1/11, event
  `7880c9da6d1ab573bf20fd6b221c0717e35056bbdb12ae318dd298cc82ab89e9`;
- pending sequence 12, SHA-256
  `42322f612f1604a053874baeff3b0c99ba53878e8036678c144c9c1bd8eff9b6`;
- exact source hash/size/custody above; absent final files, materialized receipt,
  execution owner, event12 and terminal12; deadline already elapsed;
- exact accepted installation and signed claim, all predecessor receipts,
  stopped bootstrap/materializer/runner, execution/guardian locks, P0 unchanged.

Requested outcome is terminal EXPIRED with an explicit pending-materialization
failure reason, retaining original pending/temp evidence and hash bindings.
It must NOT publish ReadyForBootstrap, replay Hybrid, fetch FINAM, contact DB0,
issue a new claim, extend a deadline, grant a run permit or start any service.

Before implementation/review, define the narrow durable abort/terminal ordering
and prove restart at each of its write/rename/head-update frontiers. Original
evidence cannot be deleted/reclassified to make ordinary inspect/cleanup pass.
Intermediate recovery state must block admission; response-loss replay must
return the same terminal receipt and not consume a second sequence. Changed
bytes/custody/manifest/head, unexpected published files or running processes
must reject before mutation. This is one bounded extension of the existing
guardian/cleanup transaction, not a generic repair CLI or a new recovery service.

Ask reviewer to accept the source correction and settle this terminal-recovery
contract separately. Then implement/test the accepted terminal-only extension,
produce reviewed native artifacts and request separate stopped installation and
recovery execution permission. Do not deploy this prevention-only patch merely
to attempt the old pending phase again. Only after real terminal recovery and a
fresh complete package/calendar/phase may another bounded O2 be authorized.
O3/O4 WS continuity, freshness, EOD and multi-session ALOR/paper parity remain
ahead on the existing roadmap; live FINAM execution and DB0 stay closed.
