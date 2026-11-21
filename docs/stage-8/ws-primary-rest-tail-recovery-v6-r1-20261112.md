# REST-tail V6 R1 — source-review corrections

Reviewed R0 (SOURCE HOLD): `1e022fef5e252da143e2976cb88bc7733a5d5a03`,
tree `afe3d94cc9e35900b158a7e6ea5897f027094605`.
Accepted predecessor remains V5 R1 `dfc6f64d0a2517d9851040187e4d8729f02ab9a4`.
This is source-only R1, not source acceptance, Linux release, installation,
operational restart authorization, or permission for another provider run.

## V6-1: cancellation-safe backoff

Backoff wake is now part of event selection, before any writer submission.
Once an event has been selected and submitted, the engine awaits its owned
reply under the existing STOP/episode/run rules. Ordinary backoff completion
returns a separate optional wake, not NoFreshData. The worker send-error STOP
guard is unchanged.

The managed duplex test holds a real Frame reply in the blocking writer across
the 1-second wake. It then permits the second REST request, one historical
callback, next live callback and full drain without another physical reconnect.
Separate in-flight variants exercise actual operator STOP and the Tokio episode
deadline. The barrier is compiled only under cfg(test), not into the runtime.

## V6-2: reconstructed inventory

The shared tail checker consumes already validated restart_source.actual_m1
before canonical_commit, just as it consumes canonical_source. This preserves
the committed inventory/frontier for subsequent overlap. RESTART_BACKFILL does
not acquire a new REST request identity; RECOVERY_BACKFILL still requires one.
All existing restart lineage, arithmetic and model checks remain mandatory.

The existing two-reconstruction long/short test is also run with V6. In each
direction, the second reconstructed prefix is reread and reconstructed again,
then its processor admits two overlap labels plus exactly one missing historical
label. Original source bytes, reconstructed book, and committed counters are
checked. The post-reconstruction overlap check is source/model admission, not
an operational auto-restart or a transport-readiness claim.

## V6-3: actual historical publication fence

The common prefix/full scanner retains the RECOVERY_BACKFILL replay deadline
until canonical_commit and checks actual model_result_at_ms and
publication_checked_at_ms against it (inclusive boundary). It also rejects
declared recovery deadlines beyond min(ready + 10000, run deadline).
This closes the inherited V5 reader gap too; valid V5 evidence stays readable.
RESTART_BACKFILL keeps its separate reconstruction/virtual-clock contract.

For V6, the tail checker requires the exact runtime formula:

`min(canonical_ready_at_ms + 10000, run_deadline, episode_deadline)`.

Synthetic full and prefix readers are tested on publication exactly at the
boundary, publication +1 after replay/episode, actual result after replay, and
expanded/narrowed declared deadlines with correctly recomputed typed source
hashes and journal/completion bindings. The model's virtual close is unchanged.
The two exact supplied reviewer mutants are additionally checked by the normal
file-only CLI; they must now refuse, while the supplied original must pass.

## Unchanged boundaries

Policy stays ObservedBoRestTailRecoveryV6. No strategy, feedback, auth, dependency,
CI/authority or source-admission relaxation is introduced. The four requests per
episode, 1/2/4-second REST backoffs, 5-second tickets, 45-second episode,
2-second live fence and existing connection/run limits are unchanged.

Positive later replies and compressed EOD tests remain synthetic. The real
November 12 V5 HOLD is not rewritten, and the real provider's tail-availability
delay is still unknown. Linux runtime/reader release remains a separate next
step after source acceptance; no VPS/main/origin mutation belongs to this R1.
