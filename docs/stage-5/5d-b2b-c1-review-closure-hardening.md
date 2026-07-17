# Stage 5D-b2b-c1/c1-r7 — review closure hardening

Status: c1-r7 review candidate, 2026-07-17. This section supersedes the c1
crash-window and forbidden-harness claims below without rewriting their review
history.

This patch closes the remaining c1-r3 review findings without calling the final
runtime-state-restored transition or opening Redis, FINAM, transport, dispatch,
runtime-live or broker execution.

## Stage 5D-b2b-c1-r7 superseding closure

c1-r7 keeps the c1-r6 boundary and closes the review findings around codec
closure and source-produced proof:

| Review finding | Fix | Positive proof | Negative/control proof |
|---|---|---|---|
| Source-owned authority decimal codec could alias finite values near zero/integer boundaries | Exact integer detection replaces epsilon tolerance; formatter now checks that its own parser reconstructs the same `f64` bit pattern for every accepted value | Source codec tests cover zero, integers, adjacent representables, `f64::EPSILON`, `EPSILON / 2`, small finite values, min positive normal and subnormal, and large finite values | negative zero, NaN, infinities and noncanonical textual aliases fail closed |
| Strict runtime finalization proof exported an extension but tested a different envelope | Real `HybridIntradayRuntimeStrategy::on_bar` callbacks produce the pending finalization; the exact exported runtime-private extension is assigned to the tested envelope | strict serialize/deserialize, bind, private apply, broker bootstrap, riskgate injection and restart simulation walk `Prepared -> LedgerAppended -> MaterializedUpdated -> AcknowledgedInRuntime -> AlreadyAcknowledged` | duplicate acknowledged replay remains idempotent and keeps runtime pending empty |
| Current-shadow positives were hand-authored JSON | Source callbacks now produce clean/no-tuple, Long open, Short open and realized-PnL current-shadow states before full Stage 5D validation | each source-produced state passes strict envelope round-trip, bind/apply/bootstrap/riskgate injection with `recovery_complete` | unrelated order-pending lifecycle is stripped from the riskgate-only proof envelope; existing source-impossible current-shadow negatives remain pinned |
| Freeze/correspondence hashes were stale after codec hardening | Stage 5C/5D manifests, checker and forbidden scanner correspondence are atomically rebound | `stage5d_b2bc_review_gate.sh` passes all required gates | forbidden-surface negative harness remains 81/81 and Stage 5D negative harness remains 44/44 |

c1-r7 still does not implement the final runtime-state-restored callback and
does not open Redis, FINAM, transport, dispatch, runtime-live or broker
execution.

## Stage 5D-b2b-c1-r6 superseding closure

c1-r6 keeps the c1-r5 boundary and closes the remaining source-consistency and
provenance gaps:

| Review finding | Fix | Positive proof | Negative proof |
|---|---|---|---|
| Stage 5D and source riskgate used separate decimal formatters | Introduce one source-owned fallible riskgate authority decimal codec in the riskgate source module; Stage 5D and runtime export consume that codec | Source codec matrix covers `0.0`, `2.0`, `-0.5`, `0.5`, `0.5000000000000001`, `158.60000000000008`; strict bind/apply/bootstrap/inject path uses the same representation | noncanonical aliases, negative zero and non-finite values fail before authority output or Stage 5D validation |
| Current-shadow chronology used hard-coded UTC+3 | Current-shadow validation now uses the exact bound runtime config timezone and weekend policy from the bootstrapped strategy | UTC+3 normal path and UTC+4 boundary-control pass under their own bound config | UTC+2 hard-coded-UTC+3 false-accept case fails closed; missing processed frontier for open tuple remains invalid |
| Provenance negative coverage was incomplete | Handoff safety validates every manifest field before indexed access and rejects duplicate ZIP entries; negative harness expands to 28 marker-pinned mutations | generated handoff manifest binds review stage, checker hashes, manifest hashes, source short/full SHA and archive name | missing fields, stale hashes, malformed JSON, non-object manifest, marker mismatch, archive mismatch and duplicate member fail with pinned markers and no traceback |

c1-r6 still does not implement the final runtime-state-restored callback and
does not open Redis, FINAM, transport, dispatch, runtime-live or broker
execution.

## Stage 5D-b2b-c1-r5 superseding closure

c1-r5 keeps the c1-r4 authority/recovery boundary and closes the remaining
review-provenance and source-binding findings:

| Review finding | Fix | Positive proof | Negative proof |
|---|---|---|---|
| Actual runtime pending-finalization export could render `0.0`/`2.0` as `0`/`2` | Runtime export now uses the same Stage 5D authoritative riskgate decimal codec as evidence validation and rejects non-finite/negative-zero values before capability construction | Source-canonical export matrix covers `0.0`, `2.0`, `-0.5`, `0.5`, `0.5000000000000001`, `158.60000000000008` and survives JSON round-trip | Actual runtime export rejects `-0.0`, NaN and infinity before producing authoritative extension evidence |
| Current-shadow validation was not fully tied to source chronology | Current-shadow validation now binds `last_day_local`, processed/persisted frontier, entry timestamp and open tuple geometry to the authoritative session | Source-valid shadow tuples after finalized/pending sessions pass | stale/local-date mismatch, impossible frontier, invalid geometry and nonzero PnL without trades fail closed |
| Handoff manifest could carry stale review-stage/hash provenance | Packaging derives `review_stage` from the Stage 5D freeze manifest; archive safety recomputes checker/manifest hashes and validates short/full SHA and marker/manifest/archive-name binding | Generated handoff manifest matches the committed freeze stage and ZIP contents | dedicated provenance negative harness covers missing/stale stage, stale hashes, bad short/full relation and archive-name mismatch |

c1-r5 still does not implement the final runtime-state-restored callback and
does not open Redis, FINAM, transport, dispatch, runtime-live or broker
execution.

## Stage 5D-b2b-c1-r4 superseding closure

c1-r4 keeps the c1/c1-r2/c1-r3 design intact and closes the remaining
authority/recovery gaps:

| Review finding | Fix | Positive proof | Negative proof |
|---|---|---|---|
| Signed zero was ambiguous under `f64 ==` | Stage 5D riskgate authority accepts canonical zero only as `+0.0` serialized `0.0`; sign-negative zero is rejected before capability construction | Canonical decimal matrix includes `0.0`, `2.0`, `-0.5`, `0.5`, `0.5000000000000001`, `158.60000000000008` | `-0.0`, `-0`, empty, plus/exponent/leading-zero/trailing-point/NaN/infinity forms fail; ledger, evidence, materialized, pending and semantic negative-zero paths fail closed |
| Current-shadow copies could agree on a source-impossible tuple | Add crate-private source-valid current-shadow validator after equality checks | Clean `None / 0.0 / 0 / no open tuple` and valid regular source sessions after finalized/pending sessions pass | `None` with PnL/trade/open tuple, weekend sessions, sessions equal/earlier than finalized or pending finalization fail |
| Single-row recovery proof did not cover ordered multi-row frontiers | Add pure no-I/O ordered frontier simulator using production validation/injection after every simulated durable transition | Two-row `Acknowledged + Prepared` and `MaterializedUpdated + LedgerAppended` frontiers reach `recovery_complete` | Replay cannot duplicate append; pending clears only at runtime ack; runtime/materialized/ledger ordering is monotonic |
| Forbidden harness advertised unsupported contention | Pin supported worker contract to default/max four workers and raise CI timeout to 30 minutes | Manifest, checker, worker, scanner and CI all agree on r4 contract | Timeout-lowering and scanner/worker contract-drift negative cases remain pinned |

### Signed-zero policy

```text
canonical zero = +0.0, serialized exactly as "0.0"
negative zero = invalid at every Stage 5D riskgate authority boundary
```

The source semantic oracle is not rewritten in this closure. Stage 5D rejects
sign-negative zero as a validation rule before it can become durable authority.

### Current-shadow source-validity policy

```text
session None
  -> pnl is canonical +0.0
  -> trade_count == 0
  -> shadow open tuple absent

session Some
  -> regular source session
  -> strictly after last finalized ledger session
  -> strictly after every pending finalization session

nonzero pnl OR trade_count > 0 OR shadow open tuple present
  -> session must be Some
```

`risk_gate_shadow_trade_count` and the open-shadow tuple remain semantic-owned;
c1-r4 validates their structural consistency with the authoritative session but
does not reclassify them as ledger-derived.

## Finding closure matrix

| Review finding | Implementation fix | Positive test/evidence | Negative test/evidence |
|---|---|---|---|
| P1-01 copied forbidden baseline was incomplete | Shared full-tree `copy_review_baseline.py` with one exclusion/symlink policy; baseline scanner runs before mutations | Clean copied baseline passes `forbidden_surface_scan.sh` | Existing forbidden mutations must still fail with their expected markers |
| P1-02 Stage 5D negative harness was absent from CI | Separate CI step plus bounded-parallel isolated harness with manifest inventory equality, measured timeout and deterministic summary | CI reports declared/passed `44/44`, no missing/extra cases | Every checker-bypass mutation must fail with its pinned marker |
| P1-03 generation was only nonempty | Exact source constant and envelope binding; separate v1 evidence metadata fingerprint | Source generation and envelope generation inject successfully; metadata changes alter fingerprint | Empty, unknown and envelope-mismatched generations return `LedgerGenerationMismatch` |
| P1-04 ledger rows were parseable but not proven source-producible | Source functions rebuild per-prefix rolling/gate fields; source/status/date/numeric/timestamp chronology is checked | Source-exact seed prefix and runtime rows validate | Each derived field, source/status order and invalid/decreasing/future timestamp mutations fail closed |
| P1-05 outbox accepted impossible crash states | Exact state table and pure deterministic recovery decision; strict session/generation/state/identity progression | Prepared missing row plans append; prepared existing exact row plans advance without duplicate append | Acknowledged/no-ledger, payload mismatch, missing row, duplicate/reordered/stale states fail closed |
| P1-06 handoff provenance did not match packaging contract | Clean-tree-only package, exact marker, generated manifest, external SHA-256 and archive safety verification | Marker/manifest/archive name bind to committed source | Dirty tree, unsafe path/symlink/excluded artifact/live-like literal or marker mismatch aborts packaging |

## Required gate

Run:

```bash
bash scripts/stage5d_b2bc_review_gate.sh
```

The gate is mandatory and fail-fast. Handoff packaging is a separate operation
and is permitted only from the clean committed tree. The gate validates both
the source-tree safety policy and a generated temporary ZIP fixture against the
same archive checker; the actual review archive is verified again by the
packaging script.

## Boundary statement

```text
Redis closed
FINAM closed
transport closed
dispatch closed
runtime-live closed
broker execution closed
runtime-state-restored return not implemented
```

The next authorized transition, after separate c1 acceptance, is Stage
5D-b2b-d. This c1 package does not implement or pre-authorize that transition.

## Stage 5D-b2b-c1-r2 superseding closure

c1-r2 separates three private concepts that the first c1 candidate conflated:

```text
validated full ledger -> authoritative projection
durable outbox states -> exact materialized/runtime prefix frontiers
frontier delta -> ordered, bound, no-I/O recovery plan
```

Validation now derives the outbox/frontiers before comparing local projections.
Only exact outbox-explained lag is accepted; there is no generic stale-state
tolerance. The injected opaque capability retains the recovery plan bound to
the envelope checksum, ledger-evidence fingerprint, riskgate identity,
generation and ordered decisions. Public diagnostics expose only decision
count, completion and a redacted plan fingerprint.

### c1-r2 finding-to-test matrix

| Review finding | Fix | Positive proof | Negative proof |
|---|---|---|---|
| Valid crash windows rejected before recovery analysis | Derive authoritative/materialized/runtime frontiers from source-exact ledger plus durable outbox before projection comparison | Public injection tests cover Prepared absent/present, LedgerAppended, MaterializedUpdated, Acknowledged and multi-row frontier | Every state rejects unrelated rolling sum, MR flag, date, count or generation drift; semantic-ahead/materialized-ahead and non-prefix states fail closed |
| Recovery decisions were discarded | Retain a deterministic private recovery plan inside `Stage5dRiskGateInjectedPaperStrategy`; validate its complete binding before capability construction | Decision count/completion/fingerprint survive successful injection | Plan/evidence/envelope binding tamper returns `RecoveryPlanBindingMismatch`; compile-fail docs prove no direct restored transition or public plan/decision construction |
| Forbidden harness accepted unrelated failures and was sequential | Machine-readable 81-case inventory, isolated bounded-parallel workers, case-specific markers, infrastructure-marker rejection and positive baseline | `81/81`, no missing/extra, four workers, 20-second per-case bound, about 40 seconds measured locally | Self-protection mutations remove marker checks/inventory/baseline check, lower CI timeout, drift worker/scanner contract and must fail with pinned diagnostics |
| Decimal text admitted non-source forms | Parse finite value, format through the source formatter, require exact original text | Golden source forms include integer-as-one-decimal and ordinary fractional values | Whitespace, plus, exponent, leading zero, missing leading zero, trailing point, negative zero and wrong integer form fail |
| `seed_loaded` was independently selectable metadata | Derive from any validated `Seed` ledger row and require evidence/materialized agreement | Seed and runtime-only fixtures produce the source value | Evidence/materialized contradictions fail closed |

### Exact single-row crash matrix

| Durable state | Ledger row | Materialized frontier | Runtime frontier | Deterministic action |
|---|---:|---|---|---|
| Prepared | absent | pre-row | pre-row with exact pending | `AppendMissingLedgerRow` |
| Prepared | exact row present | pre-row or exact explained later frontier | exact explained frontier | `AdvanceToMaterialized` (never append again) |
| LedgerAppended | present | pre-row | pre-row | `AdvanceToMaterialized` |
| MaterializedUpdated | present | post-row | pre-row with exact pending | `ReackRuntime` |
| AcknowledgedInRuntime | present | post-row | post-row, no pending | `AlreadyAcknowledged` |

Multiple rows must form one strictly ordered session/generation tail. Runtime
cannot be ahead of materialized, materialized cannot be ahead of authoritative
ledger, and every lagging row has exactly one deterministic action.

### Crash-window sequence

```text
Prepared
  -> [possible crash: append action retained]
LedgerAppended
  -> [possible crash: materialize action retained]
MaterializedUpdated
  -> [possible crash: runtime re-ack retained]
AcknowledgedInRuntime
  -> recovery complete
```

The c1-r2 boundary remains no-I/O. It does not execute these actions and does
not implement Stage 5D-b2b-d. Redis, FINAM, transport, dispatch, broker
execution, runtime-live and the restored callback remain closed.

For review, run `bash scripts/stage5d_b2bc_review_gate.sh`. The gate includes
both freeze checkers, both negative harnesses, no-Redis/safety/fixture checks,
workspace all-target tests, doc tests and clippy. Package only a clean committed
tree after this gate passes.

## Stage 5D-b2b-c1-r3 superseding closure

c1-r3 closes the remaining review findings from the c1-r2 handoff while keeping
the same no-I/O boundary. It does not implement the Stage 5D-b2b-d restored
transition.

| Review finding | Fix | Positive proof | Negative proof |
|---|---|---|---|
| Semantic current-shadow session/PnL was not bound to authoritative evidence | Add private exact overlap proof from validated evidence/materialized source and compare it to semantic runtime state before capability construction | Existing successful riskgate injection passes with matching overlap | Null/different semantic session and materially or minimally different semantic PnL fail closed |
| Materialized current-shadow PnL could default to implicit zero and authority comparisons used epsilon | Require non-empty source-canonical materialized current-shadow PnL and exact source-compatible equality for rolling/current-shadow comparisons | Golden source decimals include `0.0`, `0.5`, `0.5000000000000001` and `158.60000000000008` where applicable | Empty, noncanonical and exact-but-wrong current-shadow materialized values fail closed |
| Runtime-lagging recovery frontiers could lose pending evidence | Require exact pending finalization whenever runtime frontier excludes an outbox row; forbid pending once runtime includes it | Prepared, LedgerAppended, MaterializedUpdated and Acknowledged frontiers validate only with the expected pending state | Prepared/LedgerAppended/MaterializedUpdated without pending and Acknowledged with pending fail closed |
| A first recovery action could lead to a dead-end frontier | Add pure no-I/O stepwise recovery tests: validate F0, simulate one durable action, revalidate F1, repeat to complete | Every accepted single-row crash window reaches `AlreadyAcknowledged` | Removed pending is rejected before planning |

### Current-shadow ownership

`current_shadow_session_date` and `current_shadow_pnl_points` are authoritative
overlap fields: they exist in the evidence/materialized source and must match
semantic runtime state exactly at injection time.

`risk_gate_shadow_trade_count` and the open-shadow tuple
(`risk_gate_shadow_entry_ts_utc`, price, side, target and stop) remain
semantic-owned current-session state in this closure. They are bound by the
exact envelope checksum, but c1-r3 does not claim they are ledger-derived.

### Executable frontier sequence

```text
Prepared + pending
  -> AppendMissingLedgerRow when ledger row is absent
  -> LedgerAppended + pending
  -> AdvanceToMaterialized
  -> MaterializedUpdated + pending
  -> ReackRuntime
  -> AcknowledgedInRuntime without pending
  -> AlreadyAcknowledged / recovery_complete
```

Marker lag is accepted only when the durable projection proves the later state
and the next planned action remains monotonic and replay-safe. Runtime never
advances ahead of materialized state, materialized never advances ahead of
ledger, and pending is cleared only at runtime acknowledgement.
