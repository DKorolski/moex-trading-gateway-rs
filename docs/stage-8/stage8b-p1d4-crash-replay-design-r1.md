# Stage 8B-P1-d4 crash/replay design R1 correction

Status: R1 design-only review candidate. Source/test implementation is not
authorized by this document.

Accepted predecessor:
`7dc7c802feca6e79d3a1a9902c181ad7b6afc506` (P1-d3 governance closure R1,
CLOSED / ACCEPTED).

Reviewed R0:
`b06c78b46d2a5b7a8209d58f1c327d7cf30ae98f` (HOLD; three P1 and one P2).

This document is a normative correction to
`stage8b-p1d4-exhaustive-crash-replay-design.md`. Where the R0 text conflicts
with this R1, R1 wins. P1-d3 business semantics and its accepted source remain
immutable.

## Exact proof registry

`stage8b-p1d4-scenario-frontier-matrix-v1.csv` is the sole normative proof-cell
registry. It contains exactly 80 rows with IDs `P1D4C-001..P1D4C-080`, 11
scenario IDs and 20 frontier IDs `F00..F19`. No prose phrase such as “all
applicable” creates or removes a cell. A combination not present in the CSV is
not required; the registry contains no `not_applicable` value.

Every cell fixes its source kind, precondition, exact kill-hook name, recovered
disposition, sole legal continuation, sequence/callback/provider/schedule
deltas, PEL state, XACK expectation, duplicate/conflict obligations and test
identity. The checker validates the complete row bytes through a checked-in
SHA-256 `d1d765f1fa6db1dc948725273f58938c1d0cabd614d26076bf3ac2ddd1ad36ed`
and also validates semantic columns for every row. Row deletion,
duplication, renaming, reordering or field weakening fails closed.

The 11 scenarios are:

```text
S01 initial_limit_working
S02 initial_limit_filled
S03 initial_limit_expired
S04 later_untouched_zero_intent
S05 later_untouched_one_intent
S06 later_filled_zero_intent
S07 later_day_expired
S08 cancel_canceled
S09 cancel_execution_observed_target_fill
S10 cancel_execution_observed_already_filled
S11 cancel_already_terminal_non_execution
```

S03 retains its originating command M10 and therefore includes real
command-source XACK response-loss cell `P1D4C-030`. Only S07 has no new Redis
source at the expiry boundary. S07 includes both pre-WAL and post-WAL crashes
and post-`S_terminal` direct-Ready recovery with XACK forbidden.

S04 proves a zero-intent same-bar callback. S05 proves a one-intent callback,
including durable P1-c command publication before bar-source XACK. Their F14,
F15 and F16 rows prevent a zero-intent-only suite from claiming publication
idempotency without exercising it.

S09 proves target-first fill before cancel recovery. S10 and S11 explicitly
import the already accepted P1-d3 barriers around `S_cancel_recovered`:

```text
accepted ref: 7dc7c802feca6e79d3a1a9902c181ad7b6afc506
redis.rs blob: 5ceca40f8bbb3cb9f2dc61a1ebf43c617fbbd0d9
recovery.rs blob: 27b0edada9ef05bde8b44ba77f321a57bf729d54
test: p1d3_subprocess_sigkill_brackets_s_cancel_recovered
```

The implementation gate must verify those blobs/tests at the accepted ref or
replace each inherited cell with a new equivalent P1-d4 subprocess case.

## Refined frontier registry

The R0 count of 12 is superseded. The exact R1 registry has 20 frontiers:

| ID | Boundary | Recovery rule |
|---|---|---|
| F00 | command pending before exact successor/source observation | reclaim same command; no durable semantic effect |
| F01 | exact successor selected or source acquired before authority consumption | retain/reclaim exact source ID; phase-scoped equivalent authority reissue allowed |
| F02 | authority consumed and deterministic result computed before WAL | old durable owner; equivalent authority reissue and deterministic re-execution allowed |
| F03 | Stage6 V3 outcome WAL fsynced before RequestFinalized | pre-ACK recovery from exact WAL; authority reissue forbidden |
| F04 | RequestFinalized durable before ACK | exact ACK replay only |
| F05 | ACK applied in memory before `S_ack` | exact same ACK replay; no sequence reallocation |
| F06 | `S_ack` persisted+reread before truth | truth-only continuation |
| F07 | truth/book mutation in memory before covering seal | resume from preceding seal/WAL without duplicate truth |
| F08 | `S_working`/command `S_terminal` persisted+reread before command XACK or cancel continuation | exact XACK or exact cancel continuation only |
| F09 | recovered cancel ACK in memory before `S_cancel_recovered` | exact recovered ACK replay only |
| F10 | `S_cancel_recovered` persisted+reread before command XACK | exact command XACK only |
| F11 | later M10 acquired/reclaimed into PEL before evaluation | exact XAUTOCLAIM/source binding; equivalent schedule authority reissue allowed |
| F12 | untouched evaluation constructed before `S_eval` | old book + same source; equivalent authority reissue and deterministic re-evaluation allowed |
| F13 | autonomous later fill WAL durable before `S_terminal` | WAL-only truth reconstruction; authority reissue forbidden |
| F14 | `S_eval`/later `S_terminal` reread before same-bar callback | callback-only continuation |
| F15 | callback and any one-intent publication durable before bar XACK | exact bar XACK only; callback/publication replay forbidden |
| F16 | Redis accepted exact XACK and successful integer reply was parsed but client result was discarded | prove absent exact PEL plus group frontier; `AlreadyAcknowledged` only |
| F17 | Day-expiry authority consumed/result computed before WAL | old book; equivalent boundary authority reissue and re-execution allowed |
| F18 | Day-expiry WAL fsynced before `S_terminal` | WAL-only expiry reconstruction; authority reissue forbidden |
| F19 | Day-expiry `S_terminal` persisted+reread before Ready return | restart directly Ready; callback and XACK forbidden |

F02, F12 and F17 are intentionally pre-write-ahead frontiers. They do not
pretend that Stage6 V3/current `S_eval` evidence already exists.

## Phase-scoped deterministic re-execution

The preferred narrow recovery model is adopted. Before a full Stage6 V3
outcome or current evaluation replacement is durable, a clean restart may ask
the existing Stage5E source owner to issue a new one-use authority only for the
same authenticated source/boundary. This is a new attempt-local capability,
not deserialization of the consumed capability and not reconstruction from a
persisted fingerprint.

Reissue is legal only when all of these compare exactly:

- operational identity, account, instrument and generation;
- source Redis ID, semantic hash, payload hash and canonical close time;
- predecessor/last-evaluated source binding;
- schedule-window/trading-day identity and schedule fingerprint;
- candidate or Day-boundary timestamp;
- P1-d3 book generation, ordinal and transition hash.

The deterministic result must equal the fsync-backed pre-kill semantic audit.
That marker is proof instrumentation only and never runtime authority. Before
the eventual WAL/replacement seal, the repeated attempt may create no broker
truth, sequence, callback, XACK, book mutation or command publication.

Attempt-local counters distinguish:

```text
provider_attempt_delta
schedule_issue_attempt_delta
durable_outcome_delta
durable_truth_delta
callback_delta
publication_delta
sequence_delta
```

F01/F02/F11/F12/F17 permit exactly one post-restart equivalent authority
issue. F00 permits ordinary first issue. At and after F03, F13 or F18, and at
or after any covering replacement, provider execution and schedule-authority
reissue are forbidden (`delta = 0`). A changed source or authority fingerprint
fails before any durable effect.

For S04/S05 untouched evaluation there is no Stage6 V3 autonomous outcome.
F12 therefore resumes from the old authenticated book, exact reclaimed M10 and
new equivalent Stage5E authority, deterministically reconstructs the same
untouched evaluation, then persists+rereads `S_eval`. This resolves the R0 F08
contradiction without adding a production reservation or sidecar.

For S07, F17 uses the same rule for the exact Day boundary. Once the expiry
WAL is durable at F18, reissue is forbidden and recovery consumes only the
authenticated WAL. F19 returns exact Ready without Redis source acquisition,
Hybrid callback or XACK.

## Kernel-observed SIGKILL witness protocol

Every required cell uses this protocol on Unix:

1. Parent starts a child fixture that must not spawn descendants.
2. Child reaches the exact `kill_hook_name` from the CSV.
3. Child writes `Stage8bP1d4CrashMarkerV1` containing schema/domain, child PID,
   cell/scenario/frontier IDs and pre-kill semantic-audit digest using
   `write_all`, `sync_all`, atomic rename and parent-directory `sync_all`.
4. Child enters a non-returning barrier. No destructor, cleanup, flush or next
   lifecycle operation can run after marker commit.
5. Parent decodes the marker, checks IDs/digest, checks marker PID equals
   `child.id()`, and verifies `child.try_wait() == None`.
6. Parent independently checks that the next-phase durable/Redis effect is
   absent while the child is blocked.
7. Parent sends signal 9 to that exact PID using the platform SIGKILL path.
8. Parent calls `wait`, reaps the child and on Unix requires both
   `status.code() == None` and `ExitStatusExt::signal() == Some(9)`.
9. Only after successful reap may a new restart process open the durable root.

Any marker timeout, early exit, returning barrier, normal exit code, SIGTERM,
wrong signal, PID mismatch, missing reap or observed next-phase effect fails
the cell. Non-Unix execution is an explicit unsupported-platform gate failure,
not a skipped/ignored passing cell.

For F16, the adapter must receive and parse Redis's successful integer XACK
reply (`1` for the exact pending entry), commit the crash marker, and then
block. A pre-send disconnect, unparsed response, XACK reply `0`, SIGTERM or
normal return is not response-loss evidence.

## Exact evidence and semantic digest V1

The implementation result type is
`Stage8bP1d4CrashReplayEvidenceV1` with domain:

```text
moex.stage8b.p1d4.crash-replay.evidence.v1
```

It has fixed root fields:

```text
schema_version, domain, accepted_predecessor_ref, source_ref, source_tree,
matrix_sha256, run_ordinal, cells, aggregate
```

`cells` is a lexicographically ordered array by `cell_id` and contains exactly
the 80 registry cells. Each cell contains every CSV field plus:

```text
passed
process { child_pid, exit_code, exit_signal, reaped, wall_duration_ms }
filesystem { scratch_root, raw_marker_sha256, normalized_marker_sha256 }
redis { port, pel_before, pel_after, group_frontier, xack_reply, xack_disposition }
pre_kill_audit_sha256
post_restart_audit_sha256
final_audit_sha256
sequence_before, sequence_after
callback_before, callback_after
provider_attempts, schedule_issue_attempts
durable_outcomes, durable_truths, command_publications
restart_disposition, final_disposition
duplicate_result, conflict_result
```

No floating-point JSON number is permitted. IDs, hashes, enums and Redis IDs
are strings; counts and PID/port/duration are nonnegative integers; optional
exit/XACK values are explicit tagged strings, never ambiguous nulls.

The semantic-view transform deep-copies one run and replaces only these exact
JSON Pointer patterns with fixed same-type sentinels:

```text
/run_ordinal                                      -> 0
/cells/*/process/child_pid                        -> 0
/cells/*/process/wall_duration_ms                 -> 0
/cells/*/filesystem/scratch_root                  -> "<VOLATILE_PATH>"
/cells/*/filesystem/raw_marker_sha256             -> "<VOLATILE_MARKER_SHA256>"
/cells/*/redis/port                               -> 0
```

No other field may be removed, renamed, replaced or ignored. In particular,
`passed`, exit signal/code, normalized marker digest, all three semantic audit
digests, sequence/callback/provider/schedule/publication counters, PEL/group
frontier, XACK reply/disposition, restart/final dispositions and
duplicate/conflict results remain in the semantic view.

Canonical JSON encoding is frozen as recursive UTF-8 bytewise lexical object
key ordering, array order preserved, no insignificant whitespace, UTF-8 output,
lowercase JSON literals, base-10 integers without leading zeros, and RFC 8259
minimal string escaping. NaN/infinity/floats and duplicate object keys are
rejected. Cells must already be in exact CSV order before encoding.

The per-run semantic digest is:

```text
SHA256(
  b"moex.stage8b.p1d4.crash-replay.semantic-evidence.v1\0" ||
  u64_be(canonical_semantic_view_length) ||
  canonical_semantic_view_bytes
)
```

Two clean runs must have identical semantic digests. The checker recomputes
both views and digests independently. Mutating or excluding any retained
semantic field fails validation or changes the digest.

## R1 negative obligations

The R1 design negative harness must reject at least:

- cell deletion, duplication, reorder, ID/frontier/scenario/hook drift and
  weakening of any recovered disposition or continuation;
- removal of initial-expiry F16, recovered-cancel F10, Day-expiry F19,
  zero-intent callback or one-intent publication cells;
- treating untouched F12 as if Stage6 V3 evidence existed;
- forbidding exact pre-WAL reissue or permitting post-WAL reissue;
- reissue for changed source/schedule/book binding;
- SIGTERM, normal exit, returning barrier, kill-before-marker, PID mismatch,
  wrong signal and missing wait/reap;
- F16 pre-send loss, unparsed XACK, reply zero and absent group-frontier proof;
- broad volatile-field exclusion and exclusion/mutation of every retained
  semantic category named above;
- opening any operational/live surface.

## Allowed R1 and later implementation scope

This R1 changes only documentation, checker, negative harness and handoff
tooling. It does not change Rust/Cargo/workflow/config/authority files.

After independent R1 acceptance, P1-d4 implementation may add only test-only
barriers, isolated subprocess/Redis fixtures, phase-scoped test authority
issuance, read-only audits, evidence generation and its source gate. Any
production-state reservation or P1-d3 business-semantic change requires a new
design amendment.

Operational Redis DB0/VPS, deployable supervisor, FINAM POST/DELETE, broker
dispatch, runtime-live, real orders, partial fills and protective orders remain
closed. P1-e remains unauthorized until P1-d4 source acceptance and a separate
governance-only current-tree authority rebind.
