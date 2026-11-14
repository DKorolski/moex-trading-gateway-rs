# Quiet-market source recovery V7 — source candidate

This opt-in slice follows admitted IMOEXF bridge R1 (a4e374a5), which itself is
pending independent source review. It is NOT a new Linux release or permission
to restart an installed contour. No operational changes accompany this source.

## Incident and evidence limits

The stopped 13 November V6 session had ten known WS M1 in its last unpublished
M10; REST returned nine (missing the known final minute), with a complete body.
Subsequent physical subscriptions received bootstrap but no accepted fresh M1;
zero recovery GETs were scheduled. Low activity explains absence of subsequent
DATA, not whether the missing REST minute was temporary lag or a permanent
source discrepancy. The original journal and local reproduction are retained
as diagnostic evidence; no new provider requests were made for this correction.

## Bounded change

`ObservedBoDataWaitRecoveryV7` must be explicitly selected. Default policy and
V6 inputs/serialization remain unchanged. The extra source mode inherits V6
admission, source conflict checks, sparse-M1 handling, immutable commits, the
2-second LIVE callback fence, four GETs per episode, 1/2/4-second REST backoffs,
45-second episode and existing run/STOP limits. No synthetic M1 is introduced.

When the canonical frontier is uncertain but the physical connection is healthy,
the canonical gate fences callbacks without occupying the transport-failure
slot. The driver retains the SAME actual subscription/owner and starts bounded
REST repair, rather than reconnecting solely because a new trade did not occur.
Late repaired M10 are BACKFILL_REPLAY only. The next timely, newly closed M10
may return to LIVE after both transport and canonical checks succeed.

After a real physical reconnect, successful actual SUBSCRIBE permits REST
resync BEFORE the first fresh DATA. History does not resolve the transport
latch: new-generation, owner-matched DATA are still required to claim readiness.
V7 waits within the SAME remaining episode budget, rather than cycling through
new sockets every five seconds on a quiet market. It does not disable Ping/Pong,
network-failure, freshness, auth, clock or STOP checks. Bound-but-no-DATA expiry
remains HOLD. Its recorded cause is EpisodeDeadline, not a zero-budget drain
artifact; final STOP uses the existing independent drain allowance.

Real socket failure during a data-only repair invalidates its revision/tickets
and promotes the same episode to physical recovery, with no budget reset.
The data-only attempt conservatively consumes one of the existing three
attempts (a subsequent physical generation may therefore skip a number).
Each retryable recovery failure records stage and error before a later attempt
can overwrite its audit last_error. A completed REST resync waiting for fresh
DATA has its own diagnostic event, explicitly not readiness.
An expired writer reply may already have latched STOP: its failure metadata
still uses the existing writer drain path before finalization, rather than
silently losing the terminal attempt reason. No further market action is issued.

## Required checks

Local real WS codec + stub HTTP regressions cover quiet same-socket repair,
four-read exhaustion, STOP, REST-before-first-DATA, first DATA after five seconds,
missing-middle conflict, auth rejection, a real fault during data wait, and
original episode deadline with retained reason. A synchronous gate test proves
a later physical failure cannot be hidden/cleared by data-only recovery.
Policy tests bind versioning and unchanged budgets. Existing V6/full native
tests remain controls. All are synthetic, not provider or EOD soak evidence.

Run from a clean source tree:

    python3 -B scripts/finam_data_wait_recovery_verify.py --output /new/checks --target-dir /build/cache

The verifier records test-thread count explicitly; failed earlier qualifications
are preserved separately, never relabelled PASS by an isolated rerun.

## Deliberately not included

Numerical models, model parameters, fees and feedback are unchanged. The new
numerical-model bridge still explicitly requires V6 and is NOT silently enabled
for V7. Native USDRUBF/RI producer composition, observed context qualification,
full-session readiness and parity remain separate work. DB0, broker dispatch,
FINAM order POST/DELETE, real orders, installed ELF/timers, P0/O3 and main/origin
are untouched. Next gates: independent source review, exact Linux release, then
separately approved bounded operational observation with no readiness inflation.
