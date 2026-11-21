# V7 R1 — bounded mandatory jobs and transient failure renewal

Source predecessor: f861a56365dd3aefe4ad8000637c9a0e00d34c2b,
SOURCE CHANGES REQUIRED (V7-1/V7-2, both P2).
Bridge a4e374a5b16bb65fa1ee99a59ca64d2f37ce0510 is SOURCE ACCEPT in its
separate IMOEXF/V6/two-arm/declared-replay scope. That acceptance is not a V7
composition, release or execution acceptance. Accepted bridge files/manifests
are not rebound to this new source tree or fingerprint.

## V7-1

Keep the single critical HTTP lane and original ticket deadlines. One mandatory
Bootstrap ticket may be deferred while a Recovery GET is actually in flight;
it is dispatched as soon as that GET completes. A second deferred ticket or
any other overlap still fails. No unbounded queue or extra concurrent GET lane
is introduced. STOP/physical retirement leaves an undispatched ticket terminal,
not a successful proof. Bootstrap cannot be made optional or lost on success.

Writer Update identifies a Recovery completion by exact plan SHA. The resync
loop preserves that response while processing a mandatory Bootstrap result;
an unrelated completion cannot overwrite recovery_ready/recovery_wait/error.
Bootstrap arising during retry backoff is completed before the next GET.
Fresh DATA/owner and both canonical/transport checks remain prerequisites for
readiness, including when REST repair has already completed.

## V7-2

A completed Unavailable/Timeout receipt is checked against the active request,
issued/deadline times, executor completion and at most one send per command.
It consumes that request WITHOUT admitted source, reference/model mutation or
callback. An active foreign, duplicate or previous-generation ticket cannot be
retired. The ordinary replacement path may then issue a new owner-bound ticket
on the SAME physical WS and SAME episode/frontier; no attempt_failed/Failed
stage is entered merely for a transient request failure. Four recovery GETs,
1/2/4-second backoffs and the original 45-second total allowance remain intact.
Bootstrap remains its original separate mandatory singleton role.

The failure event retains actual receipt, error, ticket, generation and scope.
The independent tail reader checks failed-command linkage and renewal/backoff;
it does not treat failure as validated source. Auth, integrity, source conflicts,
missing-middle and old/foreign capability rejection remain terminal. A real
socket failure during retry can still promote the same episode without resetting
the recovery read count. Late repaired candles remain BACKFILL_REPLAY only.

## N1 / N2

Data-wait entry returns either canonical uncertainty or the physical fault that
already won the race under ONE gate lock. An obsolete healthy observation does
not turn a genuine transport fault into Binding. No fault is cleared.
V7 shutdown separately records unresolved_canonical_gap. The transport-fault
field contains only an actual physical/auth/liveness failure, not a synthetic
Source(Continuity) fallback; real causal faults survive candidate replacement.
Legacy V6 report behavior is retained.

## Evidence / boundaries

The two reviewer tests are adopted (formatting/module placement only) and their
original independent failure evidence is preserved. Added cases cover both
Bootstrap/Recovery orderings, STOP with deferred work, Unavailable and Timeout
then valid response, four-read exhaustion, STOP during retry, invalid receipts,
physical failure during retry, atomic transition, foreign/stale ticket rejection
and independent reader negatives. The original controls are rerun too.
Full-suite and focused counts/commands/source hashes are reported separately;
qualification uses two test threads. Synthetic tests add zero observed sessions.

No numerical kernel, strategy parameters, fees, mapping or feedback changes;
no wider LIVE deadline, more reads, time extension or hidden transport reset.
The new numerical bridge still requires V6: enabling it for V7 remains a separate
source task. No governance closure, main/origin change, VPS/ELF/timer installation,
FINAM execution, Redis DB0 activation or real orders are authorized here.
Next gates: independent R1 source review, exact Linux release, then separately
approved execution package/operational observation.
