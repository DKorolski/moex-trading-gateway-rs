# Stage 8B-P1-e I1A R1 — schedule source design correction

Status: `DESIGN_CORRECTION_REVIEW_CANDIDATE_IMPLEMENTATION_NOT_AUTHORIZED`.

This additive package supersedes the held I1A v1 design without rewriting it. It closes
D01–D04 and the publisher no-create mismatch found in review of `d198910`. Foundation
R2 is a separate source boundary at `37088964e50c0ceb4d82887a30c32103110749b0`.
No production schedule adapter, publisher, Stage6 V4 record, owner-loop wiring, Redis
activation, FINAM write, broker dispatch, runtime-live path, or real order is opened.

## 1. Source and publisher ownership

The source remains a separate pre-provisioned signed broker-neutral Redis Stream in
isolated paper DB15:

```text
finam_imoexf_paper:{finam-imoexf-p1}:market-schedule
```

It has no consumer group, PEL, or XACK lifecycle. The supervisor is read/verify-only.
The prospective FINAM GET-only publisher is the sole writer and must use exactly:

```text
XADD <stream> NOMKSTREAM MAXLEN = 4096 * payload <canonical-signed-envelope>
```

`NOMKSTREAM` is the atomic no-create fence. A prior `TYPE` check is diagnostic only;
it cannot replace the command fence. A missing stream fails without creating it.
Provisioning and private-key installation require a later operational acceptance.

## 2. D02 — authenticated snapshot progression

The stream carries authenticated snapshots, not a continuous event log. Envelope v2
separates `publication_sequence` from `semantic_revision` and signs both together with
`schedule_semantic_sha256`. The durable high-water stores generation, sequence,
publication time, semantic revision/hash, and exact envelope hash.

A valid candidate in the same generation may jump from `N` to any greater sequence if
its publication time is strictly newer, semantic revision does not roll back, all
identity/freshness/signature checks pass, and the newest row itself is valid. Thus an
ordinary M10 interval may observe `N → N+600`; an idle day, weekend, or retention loss
uses the same rule. Intermediate one-second publications are not business events and
need not be read or durably committed.

Equal sequence/equal bytes is idempotent. Equal sequence/different bytes and equal
semantic revision/different semantic hash are terminal conflicts. Sequence, time, or
semantic rollback is blocked. A generation change requires a reviewed amendment. An
invalid newest row blocks the source; scanning an older valid row is forbidden.

## 3. D03 — route-specific Stage4 evidence

Envelope v2 distinguishes two signed Stage4 evidence kinds:

- `tradability/open`: required for Market execution and Working LIMIT evaluation;
- `day_boundary/closed`: required for Day expiry after the session boundary.

Cancel may use fresh Open or Closed evidence because cancellation is lifecycle risk
reduction. Closed evidence never grants Market or Working eligibility.

Day expiry additionally binds the exact trading day, normalized reached boundary,
authenticated active working order/book, and exact evaluated last eligible M10. The
runtime cross-validates the producer's boundary timestamps with the Redis and semantic
identity of that M10 and proves no later eligible M10 for the day.

An on-time or delayed expiry may bind a current fresh Closed snapshot. Restart before
durable binding requires a new fresh Closed snapshot and full revalidation. Restart
after binding may reissue only the exact retained historical envelope for the identical
incomplete transition. Historical evidence cannot start new work.

## 4. D01 — typed Stage6 V4 binding and recovery

Before authority issuance, the prospective implementation appends one canonical
`Stage6JournalRecordVersioned::V4(Stage6JournalRecordV4::ScheduleEvidenceBound)` at
`prior lifecycle sequence + 1`, fsyncs the journal, writes and fsyncs the covering
Stage6 recovery seal at `prior seal generation + 1`, then rereads and cross-validates
the exact record and seal.

The record schema binds the operational identity, transition kind/hash, request or
active-order/book identity, exact predecessor/candidate or last eligible M10 identity,
Redis schedule stream ID, source generation/publication sequence, semantic revision
and hash, and exact signed envelope bytes/hash.

The binding is an authenticated internal boundary only. It is not a business-terminal
boundary, does not invoke strategy callback/provider, and never authorizes source M10
XACK. Existing Stage7 `RequestFinalized` and XACK-last rules remain authoritative.
It consumes exactly one Stage6 lifecycle sequence and one covering-seal generation;
the next business record starts at the following sequence. The accepted ACK/truth pair
remains locally adjacent (`seq_truth = seq_ack + 1`). Binding advances only the signed
schedule high-water carried by V4. It does not alter the M10 PEL, increment M10 XACK,
callback, provider, or business-outcome counters, or create schedule PEL/XACK state.

| Phase | Durable state | Restart action | Business effect |
|---|---|---|---|
| source verified | none | acquire a fresh source | none |
| V4 fsynced, seal missing | journal-ahead | write the one exact covering seal | none |
| seal durable, reread pending | covered V4 | reread/cross-validate, no second seal | none |
| binding committed | covered and reread | reissue exact authority for same transition | none yet |
| authority volatile | same binding | continue existing transition, no duplicate callback | existing path only |
| business transition durable | existing accepted lifecycle record/seal | existing P1-d2/d3/d4 recovery | existing path only |
| source XACKed last | terminal existing state | existing terminal route | none |

V1/V2/V3 canonical bytes and replay behavior remain unchanged. Old packages without V4
replay identically. Unknown versions, unknown V4 kinds, malformed suffixes, mismatched
sequence/seal generation, or non-identical transition reissue fail closed.

## 5. D04 — source-first three-step owner loop

Timer work is split across three owner-loop steps:

1. Resolve the exact M10 source; check latch A immediately after source and before any
   timer mutation; if clear, reclassify; check latch B; return an authenticated timer-
   classification boundary. No schedule seal or timer effect is allowed.
2. Check latch C; read and verify route-specific schedule; check latch D before durable
   work; if clear, perform the non-cancellable V4 append/seal/reread; check latch E;
   return an authenticated binding boundary. No timer effect is allowed.
3. Check latch F immediately before the effect; if clear, execute the already
   reclassified timer at most once.

A signal after source but before reclassification preserves the original timer and
creates no binding seal. A signal after reclassification but before schedule work, or
after volatile schedule verification but before binding, creates no seal or effect. A
signal during the non-cancellable binding finishes or recovers exactly that binding and
then stops before execution. A signal after binding also stops before execution.

## 6. Acceptance boundary

The original 81-row v1 matrix remains immutable historical inventory. The R1 correction
matrix supersedes only the eight rows whose v1 wording embodied D01–D04 or the no-create
mismatch and adds executable model cases for the new semantics. The semantic model
computes progression, route eligibility, binding recovery, latch effects, and publisher
no-create outcomes from fixtures; the static checker pins every contract artifact and
the negative harness repins mutated artifacts so semantic drift cannot hide behind raw
hash checks.

Acceptance of this package authorizes only the prospective allowlist in
`stage8b-p1e-i1a-implementation-scope-v2.json`. Production implementation still needs a
separate source review before F00–F17, process supervision, or operational activation.

Production implementation still needs a separate source review.
