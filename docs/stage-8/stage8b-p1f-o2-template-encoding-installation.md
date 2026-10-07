# O2 canonical-template successor — installation package candidate

Date: 2026-10-06. **Non-activating package; independent review required. O2 HOLD.**

## Accepted predecessor and observed target

The [independent review](reviews/REVIEW_f0e880d_TEMPLATE_ENCODING_20261006_RU.txt)
accepts `f0e880d8dc0ef9ccf0b7653c2c83500a08f8edfb` as SOURCE / PACKAGING CORRECTION
ACCEPT, and accepts the October-6 failure evidence/cleanup, not O2 success.
The installed manifest remains
`00675c852bc9ea713a96c4019c4771a633e8d4fda630b75ce5972280bb824ed8` from `836a297`.

Fresh read-only VPS inventory completed **2026-10-06 05:45:27 UTC**. It confirms
FAILED / generation 1 / sequence 10, head digest
`beb359ea89cdf4211f83ad5c3134d0d90412678a03d21c34898bc1b050d7cbe9`.
It performed no Redis/FINAM call, deployment, reload, signing or start. Snapshot
stdout SHA-256: `254d2774de8031a707893494cb66a021fc2a15b6703d4354befa34fc432fe4bf`.
The collector source, record and raw public output are in `retained-terminal.zip`.
Private staged-source contents are excluded; their exact hash/custody is retained.

The predecessor loader cross-checks all regular authority files against the
accepted preserved-history hashes plus exact phase/claim/event 9/terminal/event 10.
It binds the complete fresh inventory, not just the head value. Genesis,
activation, events 0–10, receipts, selector/config inputs, materializer diagnostics,
three existing install transactions, and both exhausted October-5/6 schedules
must remain exact. P0 and stopped P1 observations are also pinned. A mismatch
requires inspection, not an automatic baseline refresh.

## Payload and calendar

Reuse the exact three ELF files of accepted artifact `1062691`, compiled from
`f3b349949802abd5eff80cad6b9e9fc37bc327e1`. No production rebuild or Rust change.
Eight manifest slots are checked; **four change**:

| Slot | Change |
| --- | --- |
| materializer / operator / supervisor ELF | none |
| materialization-policy.json | history range for October 7 |
| source-template.json | four prior sessions and October-7 candidate window |
| supervisor.template.json | remove final LF only; accepted canonical bytes |
| installation-v1.json | none |
| installation-o2-v1.json | exact new hashes/sizes, predecessor and installer binding |

Candidate date: **2026-10-07**; M10 close window 04:10–20:50 UTC (07:10–23:50 MSK).
Four prior weekday sessions: October 1, 2, 5, 6. History starts October 1 04:00 UTC.
This retains the accepted explicit weekday-session policy, without synthesizing
weekend intervals. The published [MOEX session schedule](https://www.moex.com/n103379)
and [morning-session extension](https://www.moex.com/n101220) were checked October 6.
This calendar is a proposed eligibility window, **not a scheduled execution**.
The fixed materializer must still validate fresh broker schedule/truth and bars.
If missed, rebind and review the calendar explicitly; do not roll dates or reuse
the old staged source/authorization. Freshness limits stay 900s candidate / 300s truth.

## Existing transaction protocol, limited successor

`stage8b_p1f_o2_encoding_install.py` is a fixed successor of the accepted timestamp
updater: same custody, existing lock inodes, backups, fsync/atomic replacement,
resume/rollback and before/after preservation checks. New transaction directory:
`/usr/local/share/moex/stage8b-p1e/o2-template-encoding-f0e880d`.
No new authority initialization, bootstrap path or recovery framework is introduced.
Prior scripts and their historical fixtures are retained unchanged.

Only the exact two exhausted timer/service pairs are filtered from the unchanged
base stopped-unit observer after checking their file hashes and loaded state.
Neither timer can have a next firing; both callers must have PID 0, no cgroup,
no restart. The fresh pinned properties also require no queued jobs. Unexpected
P1 units still fail. Timer files/markers and all old backups are never rewritten.

## Qualification and delivery

The immutable package includes full tracked source, raw commit/tree manifest,
accepted correction review, compact predecessor/terminal evidence, the accepted
binary artifact, exact payload, old→new inventory and retained gate logs.

Required local gate: 22 prepared-package negatives (including LF and LF with
rehashed metadata), 16 Linux filesystem tests with eight injected durable
frontier/action combinations, authority positive + 45 negatives, accepted-rlib
input probe, original-LF negative control, and unchanged exact-ELF smoke. Host
systemd observations in filesystem tests are explicitly mocked. Container tests
use network none and isolated state, not the VPS. No fresh CI success is claimed.

Gate/package/check entry point: `scripts/stage8b_p1f_o2_encoding_install_review.py`.
The installer CLI consumes `installation-template-encoding/` from the reviewed
ZIP. Its existence is not authorization to apply it. Installer/source/schema
checks must pass before any write, with old→new hashes taken from delivered bytes.

After package acceptance/current CI: separately authorized stopped installation
with preservation evidence. Only then, a separately authorized single bounded O2:
fresh preflight → claim → fresh materializer → guardian ReadyForBootstrap →
isolated bootstrap → stopped proof → terminal receipt. No auto-retry. Existing
O3/O4 WS, freshness/EOD and multi-session ALOR parity remain ahead. FINAM order
writes, broker dispatch, runtime-live and real orders remain closed. No push,
merge, service installation, timer activation or operational execution in this slice.
