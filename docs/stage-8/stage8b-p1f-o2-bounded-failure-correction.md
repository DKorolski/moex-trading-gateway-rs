# O2 bounded failure — source correction

Status: SOURCE ACCEPTED at `590304af44830197704503c8ebc67329693ac75b`;
O2 HOLD. Date: 2026-09-29. Acceptance review SHA-256:
`4ff21077154d17b2f911fa2919dba9f9a2adfc8b6ba48f6979eda0ed51da26fe`.
Baseline: `60bc4821dd72126d0b981cc86810c9eb8611cf33`.
Review: `FINAM_O2_BOUNDED_FAILURE_REVIEW_AND_CORRECTION_TZ_2026-09-29.md`,
SHA-256 `c36b4ffea3c4f2317a0636eff6d12a36a4d8d01d970d0721050ebb4784f68211`.
Accepted failure evidence ZIP SHA-256:
`c139083363cb40746710c9f9ed1a5b8b2cc170b4457ce7e562f5a1f865be9fb3`.

## Exact operational boundary — unchanged by this patch

Installed artifact `7196aaac7c0bf45a03d90742d8ef483078649de6`, installation identity
`316c2376cf4d02a7f0ee3837e96d93bbf2cb1b2b8e3aabe10205f783f088aad8`.
Old phase `b8ce96287e82a1268a3617c3e29bf8ec0167014bd216c544011c07b709e3b3b8`,
generation 1, sequence 1, deadline `2026-09-29T14:49:46Z`.
Retained post-deadline observation is ACTIVE; no pending-terminal was recorded.
The future authorized terminal outcome is Expired, not a report-only relabeling.
No target, credentials, FINAM transport or operational Redis was contacted for
this source patch. No new key, claim, genesis or activation is created.

## P1-O2REC01

Public fixed cleanup and runner recovery share `cleanup_with_store`. The seam
owns the existing execution lock through both unit proofs and terminal reread.
It validates fixed selector/installation custody at the native boundary and
exact retained O2 claim/manifest/installation/generation plus existing history
transactions in the store. It never obtains a run permit. Admission still
requires genuine ReadyForBootstrap and unchanged freshness checks.

Bootstrap **and** materializer must have no MainPID/ControlPID/job, be inactive
or failed, and have an empty cgroup. Command/query errors and lock contention
refuse terminal mutation. The runner is never stopped or awaited by its own
ExecStopPost. The existing bounded systemctl stop/kill/proof mechanism is reused;
the result records both unit proofs and whether SIGKILL was actually requested.

Retained pending-terminal has first priority and retains exact reason, state,
sequence and timestamp, even after deadline. Retained stopping is resumed by
the existing transaction without minting another permit. A committed terminal
returns its original receipt. Otherwise verified pre-materialization absence
can finish directly as Failed before deadline or Expired on/after deadline,
never Completed because an unstarted bootstrap reports success. The failure
reason `o2-materialization-incomplete` is independent of terminal classification;
unit evidence retains materializer exit status. Existing stopping reason survives.

Missing ReadyForBootstrap is allowed only with verified config/bootstrap
directories, no source/config (including prepared filenames), materialization
transaction, prepared receipt or execution owner. Foreign/corrupt receipts,
partial materialization and other I/O errors refuse; nothing is deleted.

Collection and final guardian publication acquire the same existing execution
lock. Collection rechecks the exact Active claim/deadline before staged publish.
The selector publisher pre-creates the lock inode. The materializer unit adds
write mounts for **only `.execution.lock` and `.guardian.lock`**, not the authority
directory; signed documents/history stay read-only in its mount namespace.
Existing installed binaries/units have not been replaced. Delivery of this
changed unit and pre-created locks requires the later artifact/deployment gate.

## P2-O2DIAG01

Collection/mapping/history/candidate/canonical-validation failures project a
small typed context into one stderr JSON record (maximum 4096 bytes). It contains
stable reason/stage, manifest/policy/template hashes, fixed symbol, requested
UTC bounds, trusted time and applicable chunk/range/count/timestamps,
session/window/M10/first missing M1 or candidate close/age. Timestamps derived
from responses are parsed to numeric UTC seconds; raw strings are never echoed.

`completed_typed_gets` counts successfully fetched **and decoded** typed responses,
not transport attempts or a failed/undecodable response. `completed_chunks`
counts successfully merged chunks. The failed received chunk can therefore
increase GET count without increasing chunk count. Missing response telemetry
is null, not guessed. Stages are marked completed only after the corresponding
work. No raw account, token, headers, bodies or signed documents enter this
projection. Policy/template identities are hashes of the exact read bytes.

History is still checked before candidate selection. Reason codes distinguish
`chunk_symbol_mismatch`, `chunk_duplicate_conflict`, `m1_mapping_rejected`,
`m1_chronology_conflict`, `history_missing_m1`, aggregation failures,
`candidate_no_complete_closed_window`, `candidate_stale` and canonical rejection.
Fullness, 900-second freshness, canonical identity, calendar and baseline07
BO-only strategy semantics are unchanged. This cannot recover the missing raw
bars or diagnose the actual cause of the old attempt retrospectively.

## Evidence / sufficient test boundary

Prepare a detached, unmodified `60bc482` worktree at
`tmp/o2-bounded-failure-baseline` (only for the compatibility diagnostic below).
Run `python3 scripts/stage8b_p1f_o2_bounded_failure_review.py gate NEW_EVIDENCE_DIR`
from the committed clean source, then `package SAME_EVIDENCE_DIR`.
The immutable ZIP includes source tree/commit proof and actual logs, separately
hashing any expected current-tree authority drift rather than calling it PASS.
Use the same script's `check ARCHIVE` to verify CRC, exact Git tree/commit,
full member inventory, evidence hashes, duplicates, custody modes and paths.

Linked production cleanup seam tests use real signed guardian fixtures and
filesystem transactions; only OS unit observation and clock are substituted:

- no receipt/owner/source; before/exact/after deadline; no admission/Completed;
- pending intent, receipt, event-before-head, head-before-unlink; lost response
  and post-deadline replay preserve one exact terminal event/receipt;
- both unit PIDs/jobs/cgroup/state, query failure and both locks refuse;
- exact claim/install binding, corrupt history, missing directories, foreign
  receipt, partial source/config/transaction/temp files refuse;
- successful materialization/admission, Ready cleanup, stopping and pending
  stopping retain existing behavior; foreign Ready receipt fails;
- collection conflict, mapping, exact history gap, candidate stale/absent,
  canonical rejection, bounded secret-free JSON and complete 121-session source.

The gate runs fmt, FINAM default-feature tests and durable-service all-feature
tests, doctests, strict all-feature clippy,
release O2 recovery/diagnostic tests, existing O2 and ALOR regression gates.
The initial full FINAM `--all-features` probe failed
`endpoint_gate_marker_cannot_be_forged_from_manual_decision`: the legacy
`m3j16-actual-one-shot` feature changes the gate constant that the test expects
closed. The same exact failure is independently reproduced on unmodified
`60bc482` and retained as **FAIL**, not a passing gate. FINAM default features
match the previous accepted source gate; no historical test or endpoint code is
modified/skipped to hide this incompatibility. New O2 release fixtures and strict
clippy still run with all features. No claim of all-feature full-suite PASS.
No new full SIGKILL matrix, Linux installation or real GET is claimed by these
local macOS tests. Native systemd mount behavior remains for artifact acceptance.

## Next gate

SOURCE ACCEPT is recorded above. The immutable source gate below belongs to
that exact source commit; it is not rerun against a governance successor that
has intentionally rebound authority. Next bind authority
to accepted source, build a provenance-pinned recovery executable for the old
installation/phase, and separately authorize its exact delivery and invocation.
Do not overwrite an installed executable under the old manifest, use the old
empty-root installer, remove history or create a new claim. New bounded O2 is
only after old terminal receipt + exact reread and separate explicit permission.
FINAM POST/DELETE, broker dispatch, real orders and runtime-live stay closed.
