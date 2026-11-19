# USD/RI executable/input glue R1 — correction map

Reviewed parent: `1a843321b325a70c40b45a2927af4804b52a4352`,
CHANGES REQUESTED in FINAM_Shadow_Engineer_Package_20261119.zip.
Accepted library baseline `545b832f` is retained. R1 is a source-only candidate,
not self-accepted and not a Linux/native/operational qualification.

| Finding | R1 correction | Evidence |
|---|---|---|
| B1/P1 | Every admitted raw M1 contributes to exact sorted nonempty M10 inventory. RawCalendar covers the entire historical horizon; REST covers every trading/auction interval through cutoff. | Known terminal/interior omissions refuse; actual empty RI bucket stays absent; unqueried Sunday refuses, queried empty Sunday admits. |
| B2/P1 | Required independent EXPECTED_SHA256 before parsing/admitting current frozen bytes. No unpinned production admission overload. | Retimestamped metadata fails original prepare pin; fresh self-pin positive exists only to demonstrate why workflow must retain the independent receipt. |
| B3/P2 | One saved completion predicate for exporter and entry reader; exact two joined/finalized lanes. Actual entry window must be true. | Resealed missing-finalization result refuses; completed HELD remains HELD; false window refuses; repeated failure preserves existing final and adds no incomplete marker. |
| B4/P2 | Exact nested schema at public raw M1 boundary; shared permissive broker DTO unchanged. | Bar/decimal extension marker refuses; public entry-history is included in account-marker absence assertions. |
| A1/P1 | Healthy generation check and native live advance share continuity mutex. SQL/model/readback/logging remain outside it. | Another thread records Close after final readback; SQL prefix persists, source does not advance, shared HOLD/STOP. Commit-first/fault-first fence ordering tested. |

Numerical kernels/profiles/fees, broker, Cargo, CI, auth, socket transport,
recovery policy and core managed reader are not changed. Source fingerprint
now explicitly binds continuity.rs and historical calendar validator.
Bare output parent is normalized to "." without changing create-new semantics.

The synthetic two-worker fixture now propagates one shared test clock into
blocking writer commands and advances UTC/monotonic together during drain.
The old per-command frozen UTC could regress across serialized peer events or
disagree with real drain elapsed time under contention. Changes in timing.rs
and writer.rs are cfg(test) only; production clocks, 100ms consistency check
and STOP/deadline budgets are not relaxed. Original failed fixture logs are
retained alongside fresh qualification of the corrected fixture.

The existing fresh-first-session 07:00–07:09 MSK window, stop budgets and
no-resume contract remain. History is DECLARED_CLOCK_REPLAY, never current DATA.
No real token, key, account lookup, FINAM/VPS call, DB0, install or timer.

A2/full-day seal and EOD remain a separate required development slice before
counting full clean sessions, not a prerequisite for source-only prefix tests.
Neither 10 nor 20 accepted sessions is introduced as a new requirement here.
General audit remains scoped/partial; no new overall production-ready verdict.

After independent source acceptance: exact Linux release/native qualification,
then separately accepted bounded execution. See the updated input guide for CLI.
