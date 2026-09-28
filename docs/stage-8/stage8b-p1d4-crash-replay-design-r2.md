# Stage 8B-P1-d4 crash/replay design R2 correction

Status: R2 design-only review candidate. No P1-d4 source implementation is
authorized until this design is independently accepted.

Accepted predecessor:
`7dc7c802feca6e79d3a1a9902c181ad7b6afc506` (P1-d3 governance closure,
CLOSED / ACCEPTED).

Reviewed design revisions:

- R0 `b06c78b46d2a5b7a8209d58f1c327d7cf30ae98f` — HOLD;
- R1 `3a3f14f595b9672b23d421e7a857117fb2c578d2` — HOLD with P0=0,
  P1=3 and P2=1.

This document supersedes conflicting R0 and R1 design text. The accepted
P1-d3 business outcomes and normal-path ordering remain immutable. R2 changes
only design, checker, negative-harness and handoff artifacts. If R2 is
accepted, the later P1-d4 implementation is expressly allowed to add the
narrow dispatch-only recovery composition defined below; that source delta is
not an operational or live-surface authorization.

## One normative acceptance contract

The general requirements are frozen by
`stage8b-p1d4-crash-replay-acceptance-matrix.csv`:

```text
rows: 80
sha256: 57f9e83c9a4d8c6b56cb39792c7e08f63e2717c261c508fdd443317b78aac4c0
```

The sole normative proof-cell registry is
`stage8b-p1d4-scenario-frontier-matrix-v2.csv`:

```text
cells: 92
cell IDs: P1D4C-001..P1D4C-092
scenarios: S01..S11
frontiers: F00..F20
sha256: b54d8d26e5ebb12389946c905f37a029beb85d1005c7ef95edf6a47596bd725a
```

The number 92 is derived from the complete reviewed rows in the CSV. It is not
an independently selected coverage target. A proof cell exists if and only if
its row exists in the v2 CSV. Every cell is required, must execute once in each
clean evidence run and has both byte-identical duplicate and one-field conflict
variants. Missing, extra, reordered, duplicated, skipped, ignored or
`not_applicable` cells fail closed.

The R1 v1 CSV remains historical review evidence only. It is not an alternate
normative registry. General requirements P1D4D-035..P1D4D-056 now use the same
frontier meanings as this document and the v2 cell registry.

## Complete frontier registry

| ID | Exact boundary | Required restart owner and continuation |
|---|---|---|
| F00 | command pending before exact successor/source observation | reclaim exact command; no semantic effect exists |
| F01 | exact source selected/acquired before authority consumption and before request dispatch | retain exact source; one equivalent authority issue is permitted |
| F02 | authority consumed and deterministic result computed before relevant outcome WAL | request-scoped flows already have durable dispatch and recover as `P1d3DispatchPending`; autonomous flows recover from their unchanged prior owner; deterministic re-execution only |
| F03 | Stage6 V3 outcome WAL fsynced/reread before `RequestFinalized` | `P1d3PreAckPending`; finalize exact WAL outcome only |
| F04 | `RequestFinalized` durable before ACK | exact ACK replay only |
| F05 | ACK applied in memory before `S_ack` | exact same ACK replay then `S_ack`; no sequence reallocation |
| F06 | `S_ack` persisted+reread before truth | `P1d3AckCommitted`; truth-only continuation |
| F07 | truth/book mutation applied in memory before its covering seal | restart from preceding authenticated WAL/seal; reconstruct identical truth then commit covering seal |
| F08 | `S_working` or command `S_terminal` reread before source XACK or cancel continuation | exact source XACK or exact bound cancel continuation only |
| F09 | recovered-cancel ACK applied before `S_cancel_recovered` | exact recovered ACK replay then `S_cancel_recovered` |
| F10 | `S_cancel_recovered` reread before command XACK | exact command XACK only |
| F11 | later M10 acquired/reclaimed into PEL before evaluation | exact source reclaim; one equivalent schedule-authority issue |
| F12 | untouched evaluation constructed before `S_eval` | deterministic same-source reevaluation then `S_eval` |
| F13 | autonomous later truth WAL durable before `S_terminal` | reconstruct exact truth from WAL then `S_terminal` |
| F14 | `S_eval` or later `S_terminal` reread before callback | exact same-bar callback once |
| F15 | callback and any one-intent publication durable before bar XACK | exact bar XACK only; callback/publication replay forbidden |
| F16 | Redis accepted XACK and client parsed integer reply `1` before client completion | absent exact PEL plus authenticated group frontier; `AlreadyAcknowledged` only |
| F17 | Day-expiry authority consumed/result computed before WAL | one equivalent Day-boundary authority issue and deterministic re-execution |
| F18 | Day-expiry WAL durable before `S_terminal` | reconstruct exact expiry from WAL then `S_terminal` |
| F19 | Day-expiry `S_terminal` reread before Ready return | direct Ready; callback and XACK forbidden |
| F20 | request `DispatchAttemptRecorded` fsynced/reread before authority consumption/result | `P1d3DispatchPending`; bind exact existing dispatch and source, issue at most one equivalent authority and continue without a second dispatch append |

F20 and request-scoped F02 have the same durable journal suffix but are not
collapsed. Their pre-kill audits differ: F20 has no authority consumption or
computed result, while F02 has both. Each has its own subprocess hook and
evidence cell.

Authority reissue is permitted only at F01, F02, F11, F12, F17 and F20 for the
same authenticated source and boundary. It is forbidden at and after F03,
F13, F18 or any covering replacement seal. No recovery reads wall clock,
selects another bar, changes trading day or uses marker bytes as authority.

## Dispatch-only recovery composition

### Why it is required

The accepted request path persists and rereads `DispatchAttemptRecorded`
before consuming the Stage5E observation authority and before constructing the
P1-d3 transition result. A crash at F20 or F02 therefore leaves an exact
one-record journal-ahead suffix. Returning the old
`P1SemanticPrepublicationReady` owner would ignore that durable suffix and is
forbidden.

R2 adopts the explicit dispatch-durable model. After R2 acceptance, the P1-d4
source implementation may add only these production recovery elements:

```text
Stage6Stage8bP1d3DispatchOnlyCandidate
classify_stage8b_p1d3_dispatch_only_candidate(...)
Stage8bP1d3DispatchPendingOwner
Stage7bRestartOutcome::P1d3DispatchPending(...)
resume_stage8b_p1d3_dispatch_pending(...)
```

Names may receive normal module qualification, but the ownership and behavior
contract may not be weakened.

### Exact classifier

The classifier receives the current versioned journal and the checkpoint from
the committed replacement seal. It returns a candidate only when the suffix is
exactly one V1 `DispatchAttemptRecorded` row and all of the following hold:

1. The committed prefix validates byte-for-byte against the seal checkpoint.
2. The prefix ends at the exact `RequestAccepted` row for the same durable
   identity and action `Place` or `Cancel` in P1-d3 scope.
3. The dispatch has `attempt_ordinal == 1` and its accepted-payload hash equals
   the accepted row's canonical payload hash.
4. Durable identity, account, instrument, strategy request ID, durable client
   order ID and optional target order ID match exactly.
5. Dispatch `previous_record_id` and `causal_parent_id` equal the accepted row
   ID, and its lifecycle sequence is accepted sequence plus one.
6. The dispatch row is the journal frontier; no V3 outcome, finalization or
   later row exists.
7. Operational identity, package generation, commitment key and the committed
   Stage5G replacement package all cross-validate.

Zero rows, two dispatch rows, another attempt ordinal, changed payload,
identity, target, linkage, sequence, action, source binding or any trailing row
returns no candidate and the ordinary fail-closed blocked path remains in
force.

### Linear restart owner

`Stage8bP1d3DispatchPendingOwner` is a process-local linear capability. It owns
the writable durable root, the committed predecessor seal, the exact accepted
request and dispatch-only candidate, reconstructed runtime and source binding.
It is never serialized and exposes no Ready, XACK, callback, truth, generic
dispatch or broker-execution capability.

The only successful method consumes the owner and requires a newly issued,
one-use Stage5E authority for the exact source already bound by the accepted
command. Before mutation it compares:

- operational identity, account, instrument and package generation;
- accepted request/payload, strategy request, client order and target IDs;
- dispatch row ID, sequence, predecessor and dispatch payload hash;
- Redis source ID, semantic ID, payload hash, open/close time;
- schedule fingerprint, window, trading day and candidate close time;
- P1-d3 book generation, ordinal, previous transition hash and sequence
  frontier.

The method reuses the existing dispatch row and its reread checkpoint. It must not call either dispatch-append helper and must not append a second
`DispatchAttemptRecorded`. It deterministically rebuilds the same initial
LIMIT or CANCEL plan, appends and rereads exactly one P1-d3 V3 outcome and then
hands control to the already accepted finalization/ACK/replacement flow.

A mismatch fails before provider execution, WAL append, finalization, ACK,
truth, replacement seal, callback, publication or XACK. A successful replay
proves:

```text
dispatch rows                 1 -> 1
P1-d3 V3 outcomes             0 -> 1
RequestFinalized rows         0 -> 1 where request-scoped
provider attempts after restart <= 1 exact equivalent
source PEL                    pending until final covering seal and XACK
reserved sequence/projection bytes unchanged
```

This is a narrow recovery-composition delta, not a P1-d3 business-semantic
amendment. The normal no-crash path and all outcome builders remain unchanged.
It does not authorize Redis DB0/VPS activation, a supervisor, FINAM transport,
broker dispatch, runtime-live or real orders.

### Mandatory production-reachable proofs

At minimum initial LIMIT and CANCEL execute both F20 and F02:

```text
RequestAccepted
-> DispatchAttemptRecorded persisted+reread
-> [F20 kill before authority consumption]

RequestAccepted
-> DispatchAttemptRecorded persisted+reread
-> exact authority consumed and result computed
-> [F02 kill before P1-d3 V3 append]
-> restart as P1d3DispatchPending
-> one equivalent authority at most
-> no second dispatch
-> exactly one P1-d3 outcome and normal finalization
```

The test also compares the accepted request, dispatch row, source/candidate,
reserved sequence and projection bytes literally across restart. A one-field
conflict fails before a new durable or external effect.

## Added uncovered-effect cells

The v2 registry adds these previously absent accepted-source boundaries:

| Cell | Boundary |
|---|---|
| P1D4C-088 | S06/F07 later-fill truth in memory before `S_terminal` |
| P1D4C-089 | S07/F07 Day-expiry truth in memory before `S_terminal` |
| P1D4C-090 | S09/F07 target-fill truth in memory before target `S_terminal` |
| P1D4C-091 | S09/F03 recovered-cancel V3 WAL before `RequestFinalized` |
| P1D4C-092 | S09/F04 recovered-cancel `RequestFinalized` before ACK |

Cells P1D4C-081..087 add F20 for S01, S02, S03, S08, S09, S10 and S11.
Request-scoped F02 cells P1D4C-003, 013, 023, 052, 061, 068 and 075 now
expect `P1d3DispatchPending`, not `P1SemanticPrepublicationReady`.

For every F07 cell the parent proves the covering seal is absent while the
child is blocked. Restart reconstructs truth from the preceding authenticated
WAL, preserves all identities and sequence values and commits exactly one
covering seal. For S09/F03 and S09/F04, the target `S_terminal` remains exact
and the cancel outcome/finalization/ACK cannot be confused with the target
outcome.

## Exact S11 F09/F10 witnesses

The accepted P1-d3 test and source blobs remain immutable regression evidence:

```text
accepted ref: 7dc7c802feca6e79d3a1a9902c181ad7b6afc506
redis.rs blob: 5ceca40f8bbb3cb9f2dc61a1ebf43c617fbbd0d9
recovery.rs blob: 27b0edada9ef05bde8b44ba77f321a57bf729d54
test: p1d3_subprocess_sigkill_brackets_s_cancel_recovered
```

They do not complete P1-d4 cells. P1D4C-078 and P1D4C-079 require new hooks
`p1d4-s11-f09` and `p1d4-s11-f10`, new test IDs
`p1d4_s11_f09_exact_sigkill` and `p1d4_s11_f10_exact_sigkill`, and the complete
P1-d4 kernel-observed marker/SIGKILL protocol. Substitution of the old marker
or exit contract is a required negative failure.

## Exact crash-marker contract

`Stage8bP1d4CrashMarkerV1` is an exact JSON object with no extra or missing
fields:

```text
schema_version:             integer 1
domain:                     "moex.stage8b.p1d4.crash-marker.v1"
child_pid:                  positive integer fitting u32
cell_id:                    exact P1D4C ID
scenario_id:                exact S ID from that cell
frontier_id:                exact F ID from that cell
kill_hook_name:             exact hook from that cell
pre_kill_audit_sha256:      64 lowercase hexadecimal characters
```

Raw marker bytes are canonical JSON using recursive UTF-8 bytewise lexical
object-key ordering, array order preserved, no insignificant whitespace, no
trailing newline, minimal RFC 8259 escaping and base-10 integers without
leading zeros. Floats, duplicate keys and noncanonical input are rejected.
Therefore the serialized key order is:

```text
cell_id, child_pid, domain, frontier_id, kill_hook_name,
pre_kill_audit_sha256, scenario_id, schema_version
```

`raw_marker_sha256` is SHA-256 of those exact committed bytes. The normalized
marker view is a deep copy in which only `child_pid` is replaced by integer
`0`. No semantic field may be removed or normalized. Its retained digest is:

```text
SHA256(
  b"moex.stage8b.p1d4.crash-marker.normalized.v1\0" ||
  u64_be(canonical_normalized_marker_length) ||
  canonical_normalized_marker_bytes
)
```

The child commits the raw marker with `write_all`, file `sync_all`, atomic
rename and parent-directory `sync_all`, then enters a non-returning barrier.
The parent validates all marker fields and both digests, proves marker PID
equals `child.id()`, proves the child is alive and the next effect absent,
sends signal 9, waits/reaps, and requires `code() == None` and Unix
`signal() == Some(9)` before opening the durable root in a new process.

Required marker negatives include constant digest, omitted/extra field,
changed domain, changed semantic field, normalization of anything except PID,
noncanonical bytes, old P1-d3 phase-only marker, normal exit, SIGTERM, wrong
PID and missing wait/reap.

## Evidence contract

R1 `Stage8bP1d4CrashReplayEvidenceV1`, its semantic-view allowlist and its
domain-separated digest remain unchanged except that `cells` now contains
exactly the 92 v2 rows in CSV order and `matrix_sha256` is the v2 matrix hash.
`normalized_marker_sha256` is computed only by the marker algorithm above and
remains in the semantic view.

Two clean runs must have identical semantic digests. Each cell retains exact
pre-kill, post-restart and final audit digests, process signal/reap evidence,
Redis PEL/group/XACK evidence, sequence/callback/provider/schedule/publication
counters, restart/final dispositions and duplicate/conflict results.

## Required R2 negative coverage

The design checker and mutation harness must reject at least:

- any mutation of each corrected general row P1D4D-035..P1D4D-056;
- deletion or weakening of P1D4C-081..P1D4C-092 individually;
- restoring any request F02 owner to `P1SemanticPrepublicationReady`;
- omission of F20 or permission to append a second dispatch;
- dispatch-only suffix with changed identity, payload, linkage, sequence,
  ordinal, source, schedule, book or trailing row;
- substitution of inherited P1-d3 completion for P1D4C-078/079;
- constant/omitted marker digest, changed marker domain or normalization of a
  semantic marker field;
- changing either normative matrix hash or treating v1 as active;
- opening an operational or live surface.

The general matrix and cell matrix are independently byte-hash pinned. The
checker also validates semantic cell sets and named rows rather than relying
only on counts or hashes.

## Scope after independent R2 acceptance

Allowed P1-d4 implementation scope:

- the exact dispatch-only classifier, linear owner, restart enum variant and
  no-second-dispatch continuation defined above;
- test-only kill hooks and exact marker implementation;
- isolated subprocess fixtures and loopback-only ephemeral Redis;
- read-only audit/evidence generation and source gates;
- no changes to outcome business rules or normal-path ordering.

Still forbidden:

```text
operational Redis DB0/VPS
deployable paper supervisor / P1-e
FINAM POST/DELETE
broker dispatch
runtime-live
real orders
partial fills
fees/slippage
replace/protective/bracket/multi-leg orders
Generation-2 production authorization
```

P1-e remains unauthorized until P1-d4 source acceptance and a separate
governance-only current-tree authority rebind.
