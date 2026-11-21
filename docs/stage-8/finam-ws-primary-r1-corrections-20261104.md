# FINAM WS-primary — SOURCE HOLD corrections R1 / 2026-11-04

Review target R0: Git **tree**, not commit,
`bc11eec34d7721d7a3f025c3f88733cf6f6cea5b`.
Accepted predecessor tree remains `73ac00b63b792d3e812b70846c64c7788e55190a`.
This document records a candidate correction, not independent acceptance.
Exact R1 tree/archive pins and reproduction receipts live in the outer package.

Historical R1 description: reviewer subsequently closed R1/P1, R2/P1 and R4/P2
on tree `f3466825d72257829368d593b0d0e55849d71c7b`, but left R3/P2 HOLD.
The R3 timer rule below is superseded by the
[R2 due-cursor correction](finam-ws-primary-r2-due-cursor-20261104.md).

## R1 / P1: recovery deadline vs subscription lifetime

`continuous::Open` carries both establishment/episode and original run deadlines.
Auth and connect use the former; `PrimarySocket`/`Driver` pump uses the latter.
The managed owner still bounds SUBSCRIBE, fresh DATA and canonical resync by the
same original 45-second episode. No READY promotion removes that budget early.
Wall/monotonic checks and independent root STOP remain enforced.

Managed Close/EOF tests now give the real Driver the requested run deadline,
not an unrelated 24-hour fixture deadline. They pass beyond the old episode
deadline, commit subsequent M10 on the same model, and stop/drain normally.
`r1_unfinished_resync_still_expires_at_the_original_45_second_budget` is the
negative pair, alongside the existing all-stages shared-budget test.

## R2 / P1: proof response during WS grace

Improved coverage does not make the original GET ticket an integrity error.
A source-admitted answer is persisted and retained while WS grace is pending;
there is no early model callback. Before the eventual WS callback its exact M1
inventory/OHLCV is compared. A mismatch latches failure before model mutation.
Sparse-to-boundary refinement uses the current exception reason, not another GET.
Responses after a committed WS label remain comparison-only. STOP prevents late
callbacks. A retired generation discards deferred answers without clearing the
source-bound recovery gate.

Regression entry points:

- `r2_rest_before_grace_is_retained_and_compared_before_one_ws_callback`
- `r2_sparse_reason_can_improve_to_boundary_without_another_get`
- `r2_r3_early_proof_and_boundary_timer_do_not_need_another_data_frame`
- existing `ws_wins_while_boundary_proof_is_in_flight_without_duplicate_callback`
  and `pending_critical_proof_allows_ws_commit_once_or_known_stop_without_callback`.

## R3 / P2: timer follows actual seal-ready time

Processor exposes the next future action time: nominal grace, exception wait,
actual `max(close,boundary_received)+grace`, and the unchanged uncertainty-aware
live fence. Writer uses this time rather than only three nominal offsets.
Past instants are excluded, preventing a busy loop. No additional DATA or job
completion is required after boundary for a timely WS commit.
If a known proof outage completes just as grace expires, the writer also
re-evaluates WS immediately instead of discarding the already-due timer edge.

`r3_timer_tracks_actual_grace_and_uncertainty_without_relaxing_lmax` covers the
100-ms uncertainty boundary and rejects a too-late seal. The managed R2/R3 test
covers early Unavailable, one boundary, no later DATA, one GET and one WS commit
without artificial recovery. Freshness, grace and Lmax values are unchanged.

## R4 / P2: observable outage is not permanent canonical debt

Verifier checks a known Timeout/Unavailable against an actual scheduled label.
It then requires that label to be committed by WS in the scheduled generation,
or historically repaired in a later generation with retained validated source,
canonical resolution and READY. Missing/foreign/unresolved debt, duplicate failure
and security/integrity/unknown classes still reject EOD; there is no blanket ignore.

- `r4_known_proof_outage_before_or_after_ws_commit_allows_verified_101_label_eod`
  runs Timeout and Unavailable before/after WS commit through the actual managed
  101-label model/writer/export/file verifier. Paired rebound-journal negatives
  retain their refusal despite recomputed hashes.
- `r4_failed_mandatory_proof_requires_source_bound_resolved_recovery` uses an
  early failed proof, actual local peer close, new subscription, historical
  resync, the remaining day and EOD; missing/unbound resolution is refused.
- Existing T34–T36 EOD/worker/receipt/conflict regressions remain required.

## Verification and scope

The outer package must contain narrow R0-to-R1 bytes/modes diff, the original
review, developer gate logs and full from-ZIP reproduction on the exact R1 tree.
T01–T40 remain local evidence, not independent ACCEPT; successful tests here do
not qualify Linux release, native/cgroup supervision, real FINAM auth/reconnect,
full market-session EOD or current ALOR operational parity.

No strategy/profile/feedback/close-fill change, no borrowed secret or real token,
no new recovery framework, no deployment, VPS/DB0 mutation or Git push/merge.
FINAM POST/DELETE, broker dispatch, real orders and runtime-live remain closed.
