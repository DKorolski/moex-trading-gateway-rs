# Candle-only BARS: bounded reconnect / resubscribe, source slice R2

Baseline heartbeat source tree: `f4053899c876c4ce73324c5fcf15e60ef2613899`.
This candidate adds recovery after a classified BARS receive fault. It is not
installed or operationally qualified by the simultaneous heartbeat-only smoke.

## Limits and routing

Runtime transport policy is versioned `candle_bars_heartbeat_recovery_v2`,
records automatic_reconnect=true and all recovery limits, and explicitly keeps
automatic_canonical_resync=false. The prior heartbeat-only policy remains in
the accepted predecessor artifact and in that artifact's live smoke.

Only the candle-only BARS worker uses recovery. Maximum 3 attempts per episode,
1/2/4 second backoff, 45 second overall episode budget, at most 3 episodes per
run. Every attempt acquires a current account/readonly-bound shared auth grant.
Auth error/cancellation is terminal; it is never retried as a network error.
Shared stop interrupts backoff/auth/connect/first DATA. Run/EOD deadline remains
owned by the existing runner. New connect inherits the original transport's
stop and absolute deadline while constructing a fresh receive owner.

Retryable receive causes: EOF/Close/Read, provider ERROR, heartbeat Pong/idle/
DATA silence/send/flush faults. Malformed/stale/conflicting frames, clock,
explicit stop and absolute deadline retain HOLD. Provider errors have bounded
generic retry because the current parser exposes only the closed Provider code;
this slice does not infer a particular upstream/auth error from dynamic text.

After successful connect, handshake/SUBSCRIBE/fresh admitted live BARS must
complete within min(config.reconnect_budget_ms, 5000). A history-only snapshot,
Ping/Pong and connection establishment cannot satisfy the DATA gate. A failed
attempt loses the entire candidate socket before another attempt. Initial
connect failure before the worker exists retains the existing immediate HOLD;
this slice does not silently make initial admission retryable.

Fresh connection/protocol capabilities prevent use of an old connection owner.
External connection generation increases per attempt, including unsuccessful
ones. There is a single BARS producer; it drops the old owner before creating
the replacement. New-generation receipts distinguish bounded recovery from
planned S5 reconnect and auth renewal. No new endpoint is exposed to production;
the localhost connector exists only under cfg(test).

## Model and canonical data

Recovery helper has no model/feedback/REST snapshot capability. Independently
scheduled canonical REST processing remains owned by the existing worker and
ObservedSession; WS provisional bars never become model callbacks. The current
model instance, position, side-used and pending are not recreated during WS
recovery. All ordinary M10 calendar/freshness/Lmax/sparse/frontier guards remain.
If canonical REST fails or misses its deadline, this candidate still stops.

Automatic canonical gap resync is the NEXT R3 slice. It will require explicit
bounded overlap/dedup, model state/callback protection and recovery provenance.
No missed LIVE M10 is reclassified as observed by this R2 implementation.
Likewise it does not add live order entry/exit gates or claim complete readiness.

## Evidence

Recovery started/attempt/failed/succeeded events carry typed reasons, episode,
attempt and generation. A separate shared audit survives loss of the event
queue and is included in runner_end: current/last episode, attempt, stage,
generation, last reason, terminal code, last_episode_succeeded. This is recovery
history, not a general current market readiness boolean. Original first-failure
cause and liveness are retained independently if recovery terminates the worker.

## R1: owner cancellation and terminal recovery completeness

Reviewed R0 tree `88bac1505703c0e7b33602c604f5af240c80acd4` is HOLD for
the EOD/abort race. This correction does not reopen the accepted heartbeat tree.
The BARS worker synchronously latches the original typed fault, auth/connection
generation and liveness before any recovery await. The independent audit keeps
that fault unresolved until fresh DATA in a newer connection generation has
passed validation and recovery events have been published. A worker-owned drop
guard records external cancellation, including abort_all, without requiring a
cooperative stop branch to be polled.

The actual EOD/STOP coordinator drains workers, then checks the unresolved latch.
It preserves the original cause in bars_observe_failure and refuses clean EOD
with runner_bars_recovery_incomplete / INCOMPLETE_runner_failure. Explicit STOP
remains incomplete as before. Completed recovery clears the active fault, not
the historical evidence; aborting a healthy worker at EOD does not re-open it.
There is no await between successful resolution and return of the new owner.

Existing terminal status/segment/failure fields and the accepted file EOD gate
are not weakened or changed. Audit fields are additive: fault, fault_resolved,
resolved_generation/at_ms and cancelled_during. Generated test fixtures connect
the production coordinator's terminal values to the unchanged accepted Python
gate, with synthetic full 101-M10/Flat traces. They are regression evidence, not
real session evidence. Old fabricated clean/unresolved audit is shown to pass
the old gate; corrected runtime terminal values reject it even with a forged
normal-exit fixture. Completed-before-EOD remains accepted by that gate.

Owner-level tests cancel EOF and Close in backoff/auth/first DATA, before the
first restore poll, and via STOP. They verify first cause/liveness, empty task
inventory, canonical snapshot dedup and no duplicate callbacks/fills in a cold
Flat session fixture. A positive actual localhost reconnect completes fresh DATA
before owner finalization. No FINAM or VPS is used for this source correction.

## Tests and qualification boundaries

Controlled localhost WebSocket tests cover disconnect then fresh subscription,
grant acquisition on each attempt, monotonic generation, actual validated DATA,
provider faults / attempts exhaustion, handshake/health without live DATA,
malformed frame, stop during backoff, total budget during auth and run budget.
The helper cannot publish canonical Snapshot events or call the model.
Existing protocol owner/generation and heartbeat tests remain in regressions.
Further end-to-end stale-queue/gap/nonflat tests accompany R3 and readiness.

No Rust/source change from this candidate is placed into the accepted heartbeat
ELF smoke. Source review precedes new Linux release/native/operational recovery
qualification. Remote fault injection is not used against FINAM or production
networking. Graceful wire unsubscribe and restart persistence are not claimed.
