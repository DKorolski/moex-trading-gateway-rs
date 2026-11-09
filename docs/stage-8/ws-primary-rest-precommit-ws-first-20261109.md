# Candidate: precommit REST / eligible WS-first, 09.11.2026

Historical R0 proposal: SOURCE HOLD, not accepted, installed or activated.
The current source candidate and user-approved policy change are specified in
[ws-primary-nonblocking-rest-20261109.md](ws-primary-nonblocking-rest-20261109.md).
The V2 regression path now validates intrinsic body integrity before timing,
checks late known-minute inventory, and requests immediate writer reevaluation.
This historical proposal is not the current V3 operational contract.
Accepted predecessor Git tree (not commit):
`e3b6dfae65ffa0c7a0558ad5dc12664cddb6f71d`.

## User decision and exact interpretation

The user selected: if REST misses the deadline, follow WS-first. The already
published WS callback is never replayed or rewritten by a delayed REST proof.
This does not authorize delivering incomplete WS or publishing either source
after the common LIVE deadline. Source selection and publication deadlines are
different facts: a REST receipt before the deadline does not prove that the
later model/feedback/durable commit also finished on time.

The explicit opt-in policy is `ObservedBoRestPrecommitWsFirstV2`. Both earlier
delivery variants retain their selection and deadline behavior/fingerprints.
Old executables reject the unknown enum variant rather than silently applying
old rules. Strategy, feedback, quote policy, auth/account binding, reconnect,
calendar entry, persistence and broker execution boundaries are unchanged.

## Selection rules

1. Eligible complete WS is published as soon as the existing WS readiness and
   grace rules permit, without waiting for an outstanding REST request.
2. For the still-unpublished `StartupPartial` slot only, a timely independently
   admitted REST snapshot can supply the entire M10 even if provisional WS
   prices differ. No per-minute cherry-picking, mixed bucket or repeated model
   call. Record `primary_precommit_rest_selected` before the canonical callback.
3. Known WS minutes must still exist in the REST inventory. Identity, tick,
   OHLCV/body/hash/status/transport completeness, fixed range, actual receive
   time, generation, clock and exactly-once guards are not bypassed.
4. A valid late proof is retained as raw evidence and explicitly marked
   `primary_rest_proof_late_ignored`; it grants no source/model/readiness
   capability and cannot halt or rewrite an already committed WS slot merely
   because this optional late comparison differs. A timely postcommit proof
   comparison and regular audit retain their existing price-conflict HOLD.
5. `Timeout`/`Unavailable` means no proof retry; normal WS progress may continue.
   Auth/integrity/unknown/cancellation and unaccounted worker failures remain
   fail-closed. In the new mode's Proof HTTP path only, a response that finishes
   at/after its deadline is classified Timeout, not malformed-source Integrity;
   clock rollback stays Integrity. Other roles and old modes are unchanged.
6. If no eligible WS exists (in particular incomplete startup coverage), a late
   REST proof cannot manufacture a WS fallback. Existing degraded/recovery/HOLD
   behavior applies. No zero/minute fill, relaxed physical subscribe window,
   guessed source timestamp, new retry or late LIVE label.

`SparseUnproven`, `BoundaryUnproven`, `SessionEnd`, recovery overlap and regular
audits retain their previous price/inventory compatibility rules. The startup
choice is not a general claim that REST is newer, final, or authoritative for
all provider candles. Broadening it requires a separate explicit source policy.

## Budget

No new waiting timer or additional GET is introduced. Proof remains one GET per
M10, starting no earlier than C+1000 ms. Overall observed-BO LIVE publication
must fit C+2000 ms including configured clock uncertainty (C+1900 ms for 100 ms).
There is no fallback that begins after this global limit: WS must itself have
become eligible and been processed within that limit. The same source/commit
and irreversible model-call fences remain active.

## November-9 failure and diagnostic limit

Original immutable failure ZIP SHA-256:
`fe7abe51a9ebd5d1d244e1a0b413a4df704c044b9c7767591845cad815a51d5b`.
The REST request began 07:10:01.003547126 MSK, returned 01.030558738,
and the last socket-wide BARS receipt was 07:10:00.363. The corresponding
recorded deltas are 640.547126 ms to GET, 667.558738 ms to response and
27.011612 ms GET duration. The exact receive time/hash of the disputed 07:09
WS version was not retained in that old artifact and must not be invented.

The new `primary_source_change` REST_PROOF diagnostic includes the matching
cached WS generation, first receipt, last changed receipt and frame SHA. These
fields describe that specific cached version, not the socket-wide liveness
stamp. Raw REST body/hash/request/receive remain preserved independently.

The real REST body and reported WS OHLCV pair are a fixture. Constructed WS
frames/times and cold-model context in tests are explicitly synthetic; this is
not an original 389-frame replay, provider smoke, latency qualification, ALOR
parity or live acceptance. The old mode reproduces the price conflict while
the candidate admits the timely complete REST bucket exactly once.

## Required tests and operational boundary

Cover opt-in/fingerprint compatibility; timely startup source selection;
missing known minute refusal; WS overtaking a pending REST timeout; late
conflicting proof after a WS callback with unchanged canonical/model/commit;
no late model callback without eligible WS; wrong range and security failures;
postcommit audit remains strict; cached WS diagnostic provenance; HTTP deadline
edges and clock rollback; full existing regressions and strict clippy.

This candidate does not change VPS ELF, inputs, timers, old run or credentials.
The consumed November-9 run is not repeated. Source review precedes any release
build/qualification and separately authorized future operational execution.
Order routes, broker dispatch, runtime-live, real orders and Redis DB0 remain
closed. Git/main/origin are not modified by this source package.
