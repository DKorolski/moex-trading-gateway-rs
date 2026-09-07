# Stage 8B-P1-d4 generated-Market crash/replay source

Status: R3 source-evidence correction review candidate.

Accepted design predecessor:
`1a1ea05775f1d15b86fcc3495ad6863b851e9212` (R7, ACCEPTED).

Accepted business/source predecessor:
`7dc7c802feca6e79d3a1a9902c181ad7b6afc506` (P1-d3, CLOSED / ACCEPTED).

Reviewed source predecessor:
`3d2e54020f929a517bd275b775ae69c1d2974de5` (R2 HOLD with one
base-cell evidence-exactness P1 finding; its production implementation is
retained unchanged).

## Scope

This slice implements the exact R7 retained-source generated-Market graph:

```text
later M10 remains pending
  -> Hybrid callback yields one Market intent
  -> reserve exact command Redis ID in combined HMAC package (W0/G0)
  -> persist, fsync, reread and authenticate reservation
  -> explicit-ID XADD plus byte-exact marker in one Lua operation
  -> schedule/provider and Stage 6 V1 order/trade/finalization suffix
  -> combined S_ack (W1/G1)
  -> combined S_truth (W2/G2)
  -> reread and cross-validate package, entry, marker and PEL
  -> XACK the original M10 last
```

The implementation is broker-neutral and paper-only. Tests use disposable,
loopback Redis processes and generated namespaces. Operational Redis DB0/VPS,
FINAM POST/DELETE, broker dispatch, runtime-live, real orders, partial fills,
protective orders and P1-e remain closed.

## Exact publication identity

`Stage8bP1d4CommandPublicationReservationV1` is written into the authenticated
Prepublication package before command publication. It binds the source stream,
group and M10, semantic batch, strategy request, canonical command/envelope,
command stream/group, current command-stream predecessor, its checked immediate
successor and W0.

Only `P1D4_COMMAND_PUBLICATION_LUA` may publish that reserved command. It uses
the explicit Redis ID, never `*`. If the marker already exists, both its bytes
and the exact command entry must match. If it is absent, the reserved ID must
be absent and `last-generated-id` must still equal the authenticated
predecessor. Entry insertion and marker creation are one script operation.
Stream advancement, missing marker/entry asymmetry, payload substitution and
ID repick fail closed.

The versioned marker stores W0 and G0 as canonical decimal strings. Lua/cjson
numbers are not authority. A real Redis regression uses
`G0 = 9_007_199_254_740_993`, proves exact replay, and rejects its neighbouring
generation before schedule, dispatch, provider, ACK, truth or XACK.

## Independent generation chains

Write and covering-seal generations are independent counters:

```text
Prepublication: W0,     G0
S_ack:          W0 + 1, G0 + 1
S_truth:        W0 + 2, G0 + 2
```

No fixed offset between W0 and G0 is accepted. The source proof runs a valid
non-adjacent lifecycle with W0=41 and G0=100 through W1/G1 and W2/G2, and
rejects altered relations.

## Package-aware restart routing

The read-only discriminator is an authenticated tri-state:

- absent: preserve standalone accepted P1-d2 routing;
- present and valid: route the P1-d4 package before ordinary P1-d2;
- declared but invalid: hard corruption, with no P1-d2 or generic fallback.

Valid Prepublication suffixes map to seven finite owners: Prepublication,
DispatchPending, OrderPending, PreFinalizationPending, PreAckPending,
AckCommitted and TruthCommitted. Checkpoint-equal zero suffix remains a P1-d4
Prepublication owner; it is never collapsed to generic Ready.

Each owner exposes only its exact missing continuation. Before S_ack, ACK may
be reconstructed only from the authenticated V1 suffix. After S_ack, only
truth may continue. After S_truth, only source resolution may continue.

## Crash/replay proof

The active registries are immutable R7 inputs:

- 92 base cells from `stage8b-p1d4-scenario-frontier-matrix-v5.csv`;
- 13 generated-Market cells from
  `stage8b-p1d4-generated-market-crash-submatrix-v3.csv`;
- 105 positive real subprocess/SIGKILL cells total.

Every cell is parsed from all 19 or 28 normative registry columns, runs through
its only legal continuation to a terminal lifecycle state, and emits one
`moex.stage8b.p1d4.crash-replay.evidence.v2` record. The record retains the
complete source row together with process PID/SIGKILL/reap evidence, raw and
normalized marker digests, pre-kill/post-restart/final audit digests, exact
Redis group/PEL/XACK state, sequence labels, callback/provider/schedule and V1
record counters, publication count, restart/final dispositions and variant
results. Missing, extra, duplicated or unasserted cells fail closed.

Every cell also runs a byte-identical duplicate restart to the same audit and a
one-field operational-config binding conflict relevant to the cell's own
authenticated runtime. A globally wrong commitment key remains an additional
defense. Both conflicts must fail closed as an error or Blocked owner. Every
crash child writes and fsyncs a canonical marker whose audit hash covers the
local durable files before SIGKILL.

The complete 105-cell collector runs twice in clean roots and independent
loopback Redis processes. Only run ordinal, PID, duration, scratch path, raw
PID-bearing marker digest and Redis port are normalized. The two
domain-separated semantic digests must be byte-identical. Both full JSON runs
and the retained digest are immutable handoff members and are independently
validated by `stage8b_p1d4_crash_evidence_check.py`.

R2 additionally retained a typed sequence audit for each restart and final
state. It is reconstructed from authenticated Stage 6 V3 outcome evidence and
contains the complete gap-free journal sequence vector, exact business
allocation frontier, its exact journal-record index, ACK/truth sequence values
and outcome kinds. The checker
anchors the deterministic fixture at business frontier 2, validates every
pair or single allocation literally, requires the pre-restart allocation list
to remain an exact prefix, and checks the scenario-specific final outcome
sequence. `truth_bearing_outcomes` is counted independently from rows that
actually carry a truth sequence.

R3 freezes a separate versioned 92-row base evidence oracle. For every base
cell it defines the exact pre-kill allocation prefix, final allocation vector,
ordered effect vector, package phase before and after continuation, absolute
write generations, generation advance, truth-bearing V3 count and successful
truth replacement count. The Rust collector and independent Python checker
both consume this oracle; neither derives expected facts from the evidence it
is validating.

Provider and schedule counts now come from test-only observers placed at the
operational invocation sites. Generated-Market `S_ack` and `S_truth` counts
come from nonzero recovery-seal generations read from the authenticated owners
returned by the real transition calls. The retained full effect-event order is
validated exactly, and each cell's counters are derived from its prefix through
the actual crash-frontier continuation boundary; a zero expectation cannot
disable observation. Package phase and generation transitions are cross-bound
to those seal generations. R3 additionally observes every successful
replacement only after persist, reread and authentication, retains the
complete generation/phase history, and independently derives the
committed-truth count from that history. A truth-bearing V3 row is therefore
insufficient unless the frozen base oracle also requires the corresponding
persisted/reread truth package. No matrix branch assigns those counters.

The three per-cell audit hashes are no longer opaque. Their canonical payloads
bind cell/scenario/frontier, phase, restart disposition, filesystem snapshot
digest and the complete typed runtime audit. The checker recomputes each
SHA-256 and cross-validates its sequence, package, callback and outcome facts
with the structured cell evidence.

Fourteen redigested evidence mutations retain the prior seven cases and add
valid F00 allocation-before-WAL, missing F03 allocation, reversed base effect
order, wrong final package phase, wrong absolute package generation,
truth-bearing V3 without a final truth replacement, and canonical audit/hash
mismatch cases.

GM08 and GM09 use a dedicated create-once, `sync_all()` sequence-pair marker
written after allocation and before their non-returning crash hooks. Recovery
must reproduce the exact pre-kill `(seq_ack, seq_truth)` pair in the final
authenticated audit; adjacency alone is insufficient. GM07 proves the marker
is absent because allocation has not occurred.

The exhaustive collector also exposed and closed two narrow recovery gaps:

- a day-expiry WAL can reconstruct directly to `ReadyForEvaluation`, and F19
  must not reissue an already consumed expiry authority;
- target-first CANCEL recovery authenticates the exact
  `RequestAccepted -> DispatchAttemptRecorded -> autonomous LaterFilled`
  checkpoint chain instead of comparing its pre-dispatch Stage5G checkpoint
  directly with the post-dispatch outcome predecessor.

Both paths remain input-free after their durable frontier and preserve the
existing provider, schedule, callback and XACK restrictions.

Correction C1 is implemented literally. S09/F04 and S09/F09 both restart as
`P1d3PreAckPending`: each has the same recovered CANCEL V3 plus
RequestFinalized suffix without S_cancel_recovered. The volatile in-memory ACK
at F09 is not restart authority. S09/F08 remains the distinct
`P1d3CancelContinuationPending` class.

## Deliberately closed

- current-tree authority rebind before independent source acceptance;
- P1-e deployable supervisor;
- operational Redis DB0 and VPS activation;
- FINAM POST/DELETE and broker network dispatch;
- runtime-live and real orders;
- partial fills, replace, Stop/SLTP, bracket and multi-leg orders.

Acceptance of this candidate may authorize only a governance-only authority
rebind. It does not authorize operational activation.
