# O2 terminal-recovery successor — installation package candidate

Date: 2026-10-07. **Local review candidate; no installation/activation. O2 HOLD.**

## Accepted baseline and current state

The [independent recovery review](reviews/REVIEW_21cc7eb_TERMINAL_RECOVERY_EVIDENCE_20261007_RU.txt)
accepts the actual single `terminal-abort-oct7-fixed` invocation, not O2 success.
Artifact/source `21cc7eb9530ccc4480bae2a4f8264705a7383c01` was merged at
`9f3c3293a5b04738b519feee4ee3284bcf856dcf`; the trees are identical.
[Post-merge CI](https://github.com/DKorolski/moex-trading-gateway-rs/actions/runs/37625769414)
passed rust/redis-smoke for that baseline. This does not claim CI for this new package.

Accepted operational evidence ZIP SHA-256:
`69e245b4b54af5dd5ebebe62d5cfcbf23377bdf8c568104282b2f975ad8329a9`.
Accepted three-ELF artifact ZIP SHA-256:
`ef8efa957222960b6b9cb9c0d75668fd8bc13d9b6d4ab8977270bb8f7a3ca73d`.
Installed predecessor manifest (still `e05b4bf`):
`395f9e3e987ce6f5b2c52b6083d1ebc79c71287df72096c6d313ee3e56da32f5`.

Fresh read-only SSH inventory completed 2026-10-07 17:36:37 UTC. Raw stdout hash:
`433905a1275c2aae8ba3576425916ca28aa982026924318e80617d14ab7c07d1`.
Head is **EXPIRED, generation 1, sequence 12**, event digest
`f245a28a65b73dbeeb6e91bc911befe48660db4be985ebe65136bc383d1defac`.
No Redis/FINAM query, signing, service action or remote write was performed.
The compact retained evidence contains collector source/record/output and the
accepted recovery ZIP; private source bytes and credentials are not exported.

The loader compares the entire fresh control inventory against accepted postflight,
including events 0–12, receipt/intent/complete, both archived materialization files,
all prior receipts and old generation. It also compares retained trees and staged
diagnostic custody. Original source archive mode 0400 is evidence, not a permission
to repair. Configuration, selector, three exhausted timer/service pairs, old
transaction backups and staged recovery invocation records have exact pinned
inventories. Drift fails; no automatic refresh/reset or deletion is allowed.

## Payload: reuse accepted binaries, change only installation inputs

Eight replacement slots are checked; seven change:

| Slot | Change |
| --- | --- |
| materializer/operator/supervisor | exact accepted `21cc7eb` ELFs |
| materialization-policy.json | October-8 short-history range |
| source-template.json | four prior weekday sessions and October-8 candidate |
| supervisor.template.json | unchanged canonical no-LF bytes |
| installation-v1.json | supervisor binary digest only |
| installation-o2-v1.json | exact new inventory, predecessor/installer/artifact bindings |

Runtime Rust, Cargo, deployment units, CI workflows and historical installers are
unchanged. No production ELF rebuild. Generation, installation ID, state roots,
public keys and FINAM permissions remain unchanged.

Candidate date: **2026-10-08**, M10 closes 04:10–20:50 UTC (07:10–23:50 MSK).
Four prior sessions: October 2, 5, 6, 7; history starts October 2 04:00 UTC.
This retains the accepted explicit weekday-session/no-riskgate policy; no synthetic
weekend intervals. The [MOEX schedule](https://www.moex.com/n103379) and
[morning-session extension](https://www.moex.com/n101220) were rechecked October 7.
They support the window, not feed completeness or a future broker status.
Fresh broker schedule/truth and bars must pass operational validation. Candidate
freshness 900 seconds / truth freshness 300 seconds are unchanged. The date is
not a reservation, timer or authorization. If missed, explicitly rebind/review the
calendar; do not auto-roll, reuse expired phase/source or refresh timestamps.

## Limited existing transaction protocol

`scripts/stage8b_p1f_o2_terminal_install.py` is a narrow successor of the accepted
encoding updater: existing execution/guardian locks, root-owned backups, fsync,
atomic replacement, exact mixed-state resume/rollback and preservation checks
before every slot and at completion. Transaction directory:
`/usr/local/share/moex/stage8b-p1e/o2-terminal-successor-21cc7eb`.
Rollback restores installed bytes only; it never rewinds authority/history.
Any new claim, custody drift or foreign bytes block both installation and rollback.

Only the exact exhausted October-5/6/7 timer/service pairs are admitted by the
stopped-unit observer. Their file hashes/properties are checked before filtering:
no next firing, no jobs, no PID/cgroup/restart. Unknown P1 units still fail.
No daemon-reload, timer disable/rearm/reset, bootstrap or new authority is included.

## Local qualification and immutable review

Required gate: 24 prepared-input negatives; 17 Linux filesystem tests including
14 durable slot/action frontiers (7 changed slots × resume/rollback), mutation of
archives/intent/complete/receipt, preserved source mode 0400, exhausted timers,
foreign files and lock/custody failures. Systemd observations in filesystem tests
are mocked explicitly; the snapshot is a separate real read-only observation.
Also: current-tree authority positive + 45 negatives, exact accepted-rlib input
probe, LF negative control, exact three-ELF smoke, protected-source and diff checks.
Docker probes use network none and isolated roots; no VPS test execution.

Entry points: `stage8b_p1f_o2_terminal_install_package.py` (prepare/check),
`stage8b_p1f_o2_terminal_install_review.py` (gate/package/check). Full tracked source,
raw Git commit/tree manifest, accepted reviews/evidence, original artifact,
old→new inventory, payload and bounded gate logs are delivered together.
Installation consumes `installation-terminal/` and requires separate acceptance
and an explicit operator confirmation; the ZIP is not permission to apply it.

Next: review/current CI → separately permitted stopped installation retaining
EXPIRED/1/12 → installation evidence review → one separately authorized bounded O2.
O3/O4 continuity, WS subscriptions, feed freshness/EOD and several paper sessions
versus ALOR remain ahead; none are replaced by historical/offline parity.
FINAM POST/DELETE, broker dispatch, runtime-live and real orders stay closed.
No push/merge or operational mutation is part of this local preparation.
