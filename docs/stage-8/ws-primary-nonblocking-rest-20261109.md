# Source candidate R1: nonblocking REST comparisons, 09.11.2026

Not accepted, built for Linux, installed or authorized for a new operational run.
Accepted predecessor Git TREE (not commit):
`e3b6dfae65ffa0c7a0558ad5dc12664cddb6f71d`.
Held R0 Git TREE: `4b69656791a27adb04fb6675b8236b2ae1c091d3`.
R0 remains HOLD; this candidate includes its three source corrections and an
explicit additional user decision, not an assertion that R0 was accepted.

## Exact policy

Opt-in `ObservedBoNonBlockingRestV3` changes REST comparison disposition only
for candle-only BO shadow. No change to model, no-riskgate profile, close-fill
feedback, account selection/readonly authorization or broker execution.
Strict/default and previously accepted observed V1 modes retain their rules
and serialized fingerprints. V2 remains a separately identified strict-audit
regression path, with the R0 reviewer defects corrected.

| Situation | V3 action |
| --- | --- |
| Eligible WS, pending REST | Publish WS when existing grace permits; do not wait for REST |
| Comparison disagrees in OHLCV or inventory, or REST data fails | Record explicit diagnostic; no rollback, replacement or second callback |
| Unpublished startup/sparse/boundary/session-end slot needs REST data | Only timely intrinsically admitted whole REST M10 with all known WS minutes present can supply that input |
| Mandatory data missing/invalid/late, WS ineligible | No callback; existing degradation/recovery/HOLD remains |
| Bootstrap or recovery source | Existing strict source admission and reconciliation remain mandatory |
| Auth/account/readonly rejection before an admitted grant | Stop; not an optional market-data comparison failure |
| WS integrity/continuity failure, global STOP, journal I/O, panic, failed worker join | Existing fail-closed behavior remains |

Never fabricate minutes or timestamps, mix prices from competing snapshots,
stretch LIVE deadlines, retry a consumed ticket, replay a model callback or
restart the consumed November-9 run. Last session M10 still requires its real
SessionEnd REST source and existing independent EOD checks. This candidate is
not a parity or live-micro acceptance.

Whole-REST priority over conflicting provisional prices is restricted to the
explicitly agreed StartupPartial case. Sparse/boundary/session-end inputs keep
their existing price-compatibility admission; a rejected alternative is recorded
without poisoning a subsequent eligible WS input. No general mandatory-REST
price override is introduced by making comparisons nonblocking.

## Completed body, deadline and writer races

The production completed-body classifier intrinsically admits the body before
deciding lateness. Malformed JSON, wrong instrument/OHLCV/hash/range do not become
Timeout merely by finishing at the deadline. With V3 they are explicitly
Integrity diagnostics; they never create a canonical capability. Valid late
bodies can be retained as comparison evidence, not as a late model input.
A timer winning before a complete body is Timeout, not a claimed valid body.

Proof completion returning no usable REST requests immediate evaluation by the
real single writer. The writer samples time after actual evidence I/O. An
eligible WS at C+1900ms with 100ms uncertainty may complete under a fixture
clock; one additional millisecond or incomplete WS must not gain permission.
This is a scheduling regression test, NOT a guarantee of production disk or
model latency. The original C+2000ms overall budget, including uncertainty,
callback/feedback/commit fences and exactly-once guards remain in force.

REST tasks in V3 have command-local cancellation: rejecting a comparison body
does not cancel the healthy WS connection. Root STOP and retired connection
cancellation are relayed; no worker may detach. Watchdog expiry records Timeout
with `executor_completed=false`, unknown attempt accounting (zero is only a
known lower bound), not a successful command or a discarded fatal worker panic.
Bootstrap/recovery keep their strict task disposition.

## Evidence for evaluation after soak

Valid comparison sources retain original raw body/hash, request/receipt times
and exact rows. `primary_rest_observation` / `primary_audit_result` retain both
the provisional/committed basis and alternative REST rows, missing/additional
minute labels, price/volume changes and explicit nonblocking disposition.
Scheduled tickets, actual HTTP receipts, canonical origin/rows/digest, model
steps and feedback stay in the existing journal. No comparison rewrites them.

Audit status is `EvidenceOnly`, not a fabricated Match or parity PASS. Errors
are `EvidenceUnavailable` with the actual typed failure. Completed rejected
bodies retain hash/byte count and intrinsic validity, but NOT untrusted raw
error text (which could echo secrets). HTTP status/body transport errors before
a complete body have typed diagnostics only. Failed sources are not replayable
market inputs. Token/account secrets are never added to evidence.

The file-only EOD reader independently recomputes audit differences from retained
REST and committed canonical M1, rejects tampered classifications and missing
failure records, and still cannot assert network/cgroup/parity qualification.
Provisional WS assessments are diagnostics, not independent source authority.

`model_impact=NOT_EVALUATED_REQUIRES_OFFLINE_REPLAY` is intentional: a changed
bar does not prove a changed strategy decision. After an authorized soak,
compare the actual model steps first; if alternatives matter, run a bounded
deterministic replay with the SAME seed/config/feedback policy and ordered
alternate admitted M10 inputs. No second production model or recovery framework
is introduced now. Invalid/missing alternative data is explicitly unassessable.

## Source-only boundary and next step

Offline tests use real local model/writer/managed tasks and loopback peers with
synthetic clocks. The public November-9 REST body/reported WS pair is retained;
original WS wire frames and exact disputed-version RX time were not recorded,
so tests do not pretend to reproduce them. Old failure authorization is consumed.

Source review -> separate Linux release/qualification -> a new, separately
authorized observed soak. No FINAM request, VPS/ELF/inputs/timer/credential/DB0
change, Git commit/push/merge or execution occurs while preparing this source
candidate. Orders, broker dispatch, runtime-live and real orders stay closed.
