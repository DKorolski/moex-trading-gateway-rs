# Stage 8B-P1-e R6 deployable paper supervisor design correction

Status: design-only review candidate. P1-e source implementation remains
unauthorized until independent acceptance of this exact R6 package.

R6 is a direct child of held R5 commit
`3232c2447fc8d6aec038ca518efa11dc4d7e959e`. The immutable accepted
predecessor remains P1-d4 closure
`c2a9e1246dfdd59f3a6297268de907dedcb19903`. The independent R5 review is
bound by SHA-256
`de1eb40aaadcab098170f839b0acbcfe1f28845b896bb473d106a118f059141f`.

R6 closes the single R5 P1 acquisition mismatch and its P2 first-boot/checker
hardening. It changes design contracts, checker, evidence, status and handoff
tooling only. Production Rust, Cargo, workflow, active systemd units, deployed
configuration, Redis/FINAM implementation and governance authority remain
byte-unchanged.

## 1. Active contract V6

Active Acceptance Contract V6 composes the exact 217-row V5 contract, removes
eleven superseded rows and adds all 36 R6 rows:

```text
V5 active rows                  217
R6 superseded V5 rows           -11
R6 required rows                +36
-----------------------------------
V6 active rows                  242
```

The superseded rows are limited to obsolete lineage/contract identities and
the broad claimability model. Semantic Authority Registry V6 composes 26
conflict-free keys: twenty unchanged V5 keys, two replacement values and four
new execution keys.

## 2. Exact acquisition partition

Every non-Ready source owner belongs to exactly one class.

### Reclaim-required semantic continuation

Fifteen non-terminal owner/phase routes need processing authority over the
pending delivery. S06 may observe PEL/group state but cannot claim, read a
payload or grant processing authority. The already accepted exact
`resume_*_with_redis` wrapper performs at most one reclaim. Here and only here
`claim_idle_ms` and `PendingNotClaimable` apply. A successful reclaim is
followed by the shutdown-latch check before parse, continuation, provider,
schedule, callback or XACK. A second acquisition is forbidden.

### Terminal source resolution without reclaim

Five logical terminal owner/phase routes have already crossed their final
covering seal:

| Owner/phase | Accepted function | Exact lookup |
| --- | --- | --- |
| `P1SemanticZeroIntentAckPending` / zero-intent terminal | `resolve_stage8b_p1_zero_intent_ack_with_redis` | `exact_delivery_for_evidence` |
| `P1d2TruthCommitted` / `p1d2_s_truth` | `resume_stage8b_p1d2_truth_with_redis` | `exact_delivery_for_binding` |
| `P1d4GeneratedMarketTruthCommitted` / generated `s_truth` | `resume_stage8b_p1d4_truth_with_redis` | `exact_delivery_for_evidence` plus publication revalidation |
| `P1d3TruthCommitted` / `p1d3_s_truth` | `resume_stage8b_p1d3_truth_with_redis` | `exact_delivery_for_binding` |
| `P1d3TruthCommitted` / `p1d3_s_cancel_recovered` | `resume_stage8b_p1d3_truth_with_redis` | `exact_delivery_for_binding` |

These paths perform exact immutable stream-entry and source-binding validation,
then exact PEL/group-frontier validation. They never call XAUTOCLAIM, never
transfer consumer ownership and never depend on `claim_idle_ms`:

```text
exact pending source
  -> post-lookup shutdown latch
  -> exactly one XACK

source absent from PEL + continuous group frontier
  -> post-lookup shutdown latch
  -> AlreadyAcknowledged, zero additional XACK

changed/missing source or discontinuous frontier
  -> fail closed, zero XACK
```

Idle ages zero, one below threshold, equal to threshold and one above threshold
must produce the same terminal disposition. ACK replay, truth replay, callback,
provider and schedule work remain forbidden.

The source oracle pins
`crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs` at SHA-256
`a870fbbb6ec9fc60b7df9c35a2aca7a5daf81f695b019352d7f20c9b51439d16`.
The checker extracts the four terminal function bodies and proves the expected
exact-delivery helper call and absence of reclaim helpers. This is a read-only
compatibility proof, not authorization to modify the accepted source.

## 3. Operational V6 composition

Operational V6 is an exact overlay over immutable V5. It excludes sixteen
terminal rows that encoded `claimable`/`not-yet-claimable` and replaces them
with eleven no-claim cells. Forty non-terminal V5 rows remain byte-identical;
the composed inventory is 51 unique cells.

The four ordinary terminal owners each have:

- one exact pending source at any idle age;
- source already acknowledged with a continuous group frontier.

Cancel-recovered has three cells:

- `R6OC09`: exact pending source at any idle age;
- `R6OC10`: already acknowledged with continuous frontier;
- `R6OC11`: exact pending source plus due Day timer.

There is no terminal `claimable` or `not-yet-claimable` cell. R6OC11 retains
`SOURCE_FIRST_TIMER_DEFERRED`: source resolution and its XACK/AlreadyAcknowledged
disposition precede timer reclassification against the returned authenticated
owner. PaperReady remains forbidden until source resolution and any required
timer reclassification complete.

## 4. Stale receipt-temp conflict

The accepted R5 mandatory adopted-marker model remains unchanged. Transaction
V5 is an exact amendment over V4 and adds `receipt-temp-absent` to the literal
predicates for:

- `ReceiptCommittedMarkerUpdatePending`;
- `SealCommittedToAdoptedMarkerTempPending`.

A valid final receipt together with any stale receipt temp at
`seal_committed` is `CorruptOrIdentityMismatch`, exit 66, before receipt or
marker mutation. The eleven base classes, four marker-temp classes,
exactly-one/no-precedence classifier and adopted-marker ordinary-run authority
remain intact.

## 5. Checker and authorization boundary

The R6 checker composes and cross-validates:

- exact V5 active rows minus eleven supersessions plus 36 R6 rows;
- 26 semantic authorities without conflicts;
- the fifteen/five acquisition partition and source-function oracle;
- V5 operational rows minus sixteen plus eleven R6 cells;
- terminal idle-age independence, zero XAUTOCLAIM and exact XACK/frontier rules;
- stale receipt-temp conflict before marker mutation;
- cancel-recovered source-first timer precedence;
- every accepted R5 network, classification and no-replay correction.

Redigested negatives reject terminal XAUTOCLAIM, terminal claim-idle gating,
reclaim-required no-claim routing, owner overlap/omission, retained terminal
not-yet-claimable rows, stale receipt-temp acceptance, premature PaperReady,
source-oracle drift and active/semantic supersession drift.

Independent acceptance of this exact R6 design may authorize only P1-e source
implementation against V6. It does not authorize installation, P1-f,
operational Redis DB0/VPS, non-loopback Redis, FINAM POST/DELETE, broker
dispatch, runtime-live, real orders, partial fills or protective orders. The
nonblocking `0 < child_pid <= u32::MAX` hardening remains deferred to a
separately authorized source slice.
