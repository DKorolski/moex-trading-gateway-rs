# Sparse M10 source correction — implementation progress, 2026-10-03

Status: **IN PROGRESS, not a source-acceptance or operational handoff**.
Branch: `stage8b-sparse-m10-correction`; parent
`eb1a974d884196f9620f3db2cced3a4ad0991921`.

The accepted 2026-10-02 diagnostic/spec authorizes local implementation. It
does not accept a new sparse contract, model/live parity, or O2. The deliverable
remains **one completed source correction package**, not another design cycle.
No intermediate foundation review is requested.

## Implemented locally

- Pure `broker-finam::sparse_m10` admission: independent planned range/parts,
  fixed GET/endpoint/symbol/M1 identity, closed interval, receipt chronology,
  freshness/history depth, completed response, bounded length/hash/strict decode.
- A completed REST snapshot is the provider trust boundary. Missing minutes
  are **not** declared no-trade or zero-volume. Stream silence cannot create
  this evidence. Caller-provided DTO fields alone are not transport authority;
  the new guarded GET collection method now captures completion, original
  response-length expectation, raw bytes/hash and actual request/receipt times.
  It checks history depth before any request and reads the body incrementally
  under the existing 32 MiB aggregate bound. No new route, redirect, retry or
  scheduler was added. It has not been invoked against FINAM. The fixed
  materializer now selects it only under explicit policy V3; installed policy
  and services were not changed. HTTP-body controls are local unit tests, not a live
  transport witness.
- Exact `[start,end)` normalization: the single exact right-boundary timestamp
  is accounted separately; other out-of-range values reject. Equal duplicates
  deduplicate; conflicting duplicates/boundary copies reject. No partial set of
  responses can shorten the independently specified plan.
- Fixed nominal M10 aggregation over 1–10 actual M1. No placeholders, no
  previous-close fill, no shifted nominal start. Empty buckets reject. Actual
  M1 vector, minute bitmap and raw snapshot digest remain bound; loss/repricing
  after admission rejects.
- `candidate_at` takes an explicit nominal calendar close, checks age and
  closure, and never searches for an older dense bucket. The new
  `Stage8bP1fObservedM10Plan` fixes History windows, candidate and GET range from
  the existing no-riskgate calendar template **before reading bars**. Its
  materializer derives History and candidate from one admitted snapshot.
  Tests cover real sparse History plus explicitly synthetic candidate
  controls, missing first/last minutes, empty candidate/history, stale response,
  end-of-session without a next bar, foreign identity and truncated range.
  This is an additive input path: legacy policy V1/V2 CLI selection and the
  strict canonical builder have **not** been relaxed. The template's V3
  format is used only as calendar/config input, never emitted as sparse V3.
- Shared broker-neutral `ObservedM1Receipt`: full normalized minute inventory,
  original raw snapshot hash, exact range and receipt time. The FINAM adapter
  creates it only after raw admission; the same pure bucket algorithm is used
  by canonical verification. Receipt construction does not itself prove HTTP
  completeness. Restore requires an independent authenticated expected hash.
- Explicit canonical M10 **V2** with policy, nominal bounds, actual M1
  identities/hashes, bitmap, receipt/raw references and range. Its contextual
  parser recomputes against the full retained receipt, not a self-claimed subset.
  Coherently shortened/repriced and rehashed candidates reject against the
  original source binding. Strict V1 still rejects V2 (including dense V2).
  No context-free V2 parser or silent legacy upgrade was added.
- Immutable source context now follows validated canonical input into local
  delivery/reclaim and explicit Redis attachment. A handle has exactly one
  separately admitted receipt and expected operational identity; it cannot
  discover authority from Redis or silently fall back to V1. Publication,
  exact reread, XREADGROUP, XAUTOCLAIM, recovery inspection and successor
  observation all use that context. Receipts are shared in memory, not embedded
  anew in each candle. Default Redis attachment remains strict V1.
- Additive fixed-producer high-water **V2**: O3 requires a pinned synthetic
  raw snapshot; O4 takes an admitted closed-REST snapshot, not WS silence.
  Nominal bounds, signed schedule, freshness, phase progression and one
  publication sequence remain checked. V1 and V2 retained state loaders reject
  each other's format, and V2 persistence refuses to overwrite a legacy file.
  Prepared is fsynced/reread before publication; Published follows exact Redis
  reread. Existing feeder role restrictions and Lua scripts are reused.
  The new APIs are not selected by the operational CLI or loader.
- Distinct observed provenance uses `aggregation_complete=true` and
  `gap_absence_proven=false`; it means complete admitted response, not a dense
  minute grid. Legacy dense provenance still requires its former gap proof.
  Recovery source-mode codes 1–5 are unchanged; observed mode uses additive 6.
- Explicit first-boot **wire V4** assembler/parser. All History OHLCV and the
  exact candidate are rechecked against the receipt; calendar coverage, truth,
  numeric/tick, freshness and disabled-RiskGate checks remain. The assembler
  reuses account/order/instrument/schedule/raw-route checks and requires Bars
  observations to match the admitted raw parts exactly. V4 source-plan hash
  differs from V3. The pure composition passes observed provenance to History
  and Replay and survives authenticated export/restore. A linked offline test
  uses all 404 real History bars, five synthetic current-session prefix bars
  and a synthetic seven-M1 candidate; two restores yield the same fingerprint
  and one recorded Replay callback, with no callback repeated by restore.
  This is **not** a Redis/XACK or process-restart proof.
  The fixed-path loader now selects V4 only from the validated schema-2
  bootstrap policy, never by sniffing incoming source JSON. Existing schema-1
  configurations still select strict V2/V3. The installed config and CLI
  artifact have not been switched or deployed.
- Explicit bootstrap schema **2** requires the exact observed V4 source-plan
  hash and no-riskgate runtime fingerprint. This hash is included in the
  Stage 6 operational identity, hence in root naming and authenticated restart
  packages. The optional field is omitted for legacy identities: a literal
  pre-extension wire regression checks byte/hash preservation. Other policies,
  legacy-plus-policy, missing policy and RiskGate-enabled profiles reject.
  Other identity constructors only receive explicit `None`; strategy behavior,
  instrument mapping and old market-data generations are unchanged.
- Existing first-boot transaction V5 binds the V4 plan, enclosing bundle hash,
  exact history and candidate in its authenticated marker and sealed provenance.
  Historical recovery selects the same policy from validated config. A test
  interrupts the existing Prepared-write frontier, resumes after 30 days,
  rejects changed source bytes, and verifies adoption through the existing seal.
  No new transaction/receipt format or recovery engine was added.
- The verified restart outcome can restore the **initial** receipt from the
  retained source bundle only when configured SHA, sealed first-boot provenance,
  operational identity, plan, history and candidate agree. It reconstructs the
  same canonical bytes/hash after restart; wrong key/hash and rehashed alternate
  source reject. This is historical source context, not fresh broker truth or
  authority to execute an order. It does **not** solve later rolling receipts.
- Exact public fixtures imported from accepted compact ZIP
  `5ada885a1b087b2cc74775a1c51a0f8bd6231d69f654ead364f53485f1ce6993`.
  Reproducible importer/verifier checks five file hashes. Real tests cover all
  404 buckets, all 18 sparse matches, two missing-first buckets, four unchanged
  full-bucket differences, and short response 28 raw / 27 in-range / 1 boundary.
  Missing-last, single/empty, lost response parts and bad input are explicitly
  synthetic controls, not broker observations.

The [FINAM REST Bars documentation](https://api.finam.ru/docs/rest/), checked
2026-10-02, states M1 history depth of seven days and an exclusive end. The
observed exact-end extra bar remains separately accounted; it is not silently
included in the requested interval. A request outside documented depth fails
closed; splitting an old range does not extend that depth. Confirmation of
no-trade candle generation, finality and end semantics from FINAM is still a
pre-operational requirement, not a blocker for local coding.

## Actual offline model replay

Same accepted V2 BO-only no-riskgate configuration, same four prior completed
freeze sessions (2026-09-22 through 2026-09-25, 404 bars), then both complete
404-bar sequences for 2026-09-28 through 2026-10-01. The new evidence driver
cross-checks every intent, BO state projection and trade against the existing
accepted immediate-current-close simulation. No trading parameters or runtime
trading methods are changed. The FINAM crate dependency is **dev-only**.

Results:

- Four input OHLCV differences retained exactly.
- Zero intent/request differences. Three identical completed paper rounds.
- Two transient full exported-state differences, limited to `last_bar_close`
  and `current_day_close`: 29 September 17:10 MSK, 2266 versus 2266.5;
  30 September 17:00 MSK, 2270.5 versus 2270. The following bars converge.
- First input difference: 28 September 07:00 MSK (open/volume); it does not
  change exported runtime state or a decision in this replay.
- This is nominal-bar offline model parity under one execution assumption,
  **not** actual broker fills, ACK timing or live EOD timeliness. The accepted
  ALOR publication delays remain an operational oracle limitation.

Full before/after-position exported state, input prices, intent debug records,
request IDs and paper rounds are generated into
`reports/stage8b-sparse-m10-local/model-replay.json` (about 5.2 MiB uncompressed).
Machine-readable test/source inventory is `progress-evidence.json` beside it.
These are generated local evidence, not yet included in an immutable handoff.

## Checks and reproducibility

Run `python3 scripts/stage8b_sparse_m10_local_check.py`.
It checks immutable fixtures, fmt, all broker-finam library tests, the explicit
replay, strategy-runtime-core library/integration regressions, gateway library
and O2 binary tests, durable-service library regressions, scoped strict Clippy,
and diff whitespace. Environment Redis endpoints are removed; Cargo runs
offline. Existing integration tests spawn disposable loopback Redis instances;
they do not use operational Redis/VPS. The runner never updates authority.

Previous foundation run, 2026-10-02: **PASS**, all 12 commands returned zero and
the source inventory was unchanged during that run. **These counts predate the
receipt/canonical/first-boot changes and are not current-tree gate evidence.**

| Check | Actual result |
| --- | --- |
| broker-finam library | 97 passed |
| strategy-runtime-core library / integration | 1297 / 26 passed |
| gateway library | 494 passed, 2 ignored helper tests |
| O2 materializer binary | 10 passed |
| durable-service library | 334 passed, 15 ignored helper/opt-in tests |
| explicit four-day model replay | passed; 0 decision differences, 3 equal paper rounds |
| fmt, fixture hashes, whitespace | passed |
| strict Clippy | broker-finam all targets; runtime lib/bins; gateway + durable lib/bins with existing source-adapter feature: passed |

Ignored/opt-in tests are not claimed as independently executed by this command;
some helper child tests are invoked by their parent scenarios. The full durable
regression took about 23 minutes, including existing process crash/restart
matrices; no new exhaustive crash framework was added.

The broader runtime all-target Clippy probe
without artifact features encountered existing unused test failpoint variants
in `stage6_journal_backend.rs`; that unrelated code was not changed or silenced.
Default-feature test builds also emit existing feature-gated helper warnings;
the documented strict lib/bin Clippy scopes passed. These local checks do not
substitute for the final integrated correction gates or authorize authority
fingerprint refreshes.

For the current receipt/first-boot slice use:
`python3 scripts/stage8b_sparse_m10_local_check.py --scope receipt-firstboot`.
It records a separate inventory, logs and replay under
`reports/stage8b-sparse-m10-local/receipt-firstboot/`, runs broker/core/gateway
regressions and targeted durable canonical/observed/first-boot tests, and does
not claim a new full durable process/Redis crash-matrix run. The default full
scope remains available and is required before final source handoff.

The producer/context slice is checked with:
`python3 scripts/stage8b_sparse_m10_local_check.py --scope producer-redis`.
It adds the existing P1-c Redis and fixed-role regressions, retaining actual
logs and source inventory in `reports/stage8b-sparse-m10-local/producer-redis/`.
Direct targeted tests already passed for:

- sparse publication/exact reread, new-consumer XAUTOCLAIM and successor
  readmission against restored independent receipt; wrong/missing context and
  strict/observed policy mixing reject;
- producer disk reload after a real Redis publication whose completion was
  not committed locally, idempotent exact retry and O3-to-O4 sequence 1→2;
- sparse zero-intent callback through the **existing P1-c fixture** to durable
  commit and XACK, followed by ACK-only restart without another callback.

The last test is a source-context integration witness, not a V4 first-boot
operational-policy/seal acceptance or the full order/fill lifecycle proof.
That previous slice used one retained receipt. The rolling-lifecycle slice below
adds an immutable pair, with no replaceable in-memory trust registry.
Use the generated result to determine whether the aggregate slice check passed;
these targeted results do not claim full source correction completion.

## Rolling receipts / synthetic linked lifecycle slice

The current local check is:
`python3 scripts/stage8b_sparse_m10_local_check.py --scope rolling-lifecycle`.
Read its actual `reports/stage8b-sparse-m10-local/rolling-lifecycle/progress-evidence.json`
for PASS/FAIL and the exact source inventory; earlier check inventories are not
evidence for subsequent changes. This scope retains the transaction/seal
regressions and adds the accepted dense P1-d2 market-feedback regression.

Implemented and covered by the targeted tests:

- An immutable predecessor/successor receipt pair selects the previous source
  for its old closes, and the new source for later closes. Reversals, foreign
  identities and changed/lost M1 in the overlap reject. Equal old OHLCV with a
  newer receipt still cannot replace the old canonical M10 bytes.
- Content-addressed full receipts are synced before producer Prepared and
  publication. A later Published high-water does not delete old receipts.
  This is bounded per-file evidence under the existing single-writer protected
  parent, not a registry, retention GC, concurrent-writer service or authority.
  Recovery supplies independently expected hashes; exact pending M10 recovery
  checks sealed Redis/semantic/payload identities and regenerates the canonical
  bytes against the full receipt. Missing, corrupt, symlink/hardlink or swapped
  receipts fail closed. Corrupt existing evidence is not overwritten.
- Both core semantic transition routes compare observed provenance with the
  authenticated operational identity policy before callback. A valid canonical
  sparse payload alone cannot authorize sparse callbacks on a legacy root;
  the observed root likewise rejects a dense legacy payload. Existing legacy
  identity bytes, source-mode codes and dense behavior are retained.
- A **synthetic** V4 History/first-boot transaction feeds an observed decision
  M10 and a successor M10 from another retained source. Real isolated Redis
  drives one command/provider call, `S_ack`, clean restart, `S_truth`, clean
  restart, source XACK and already-acknowledged restart. Direct effect counters
  prove no repeated callback/provider/publication after restart. Sequence pair
  and complete truth audit remain equal; pending count stays one until truth
  is sealed, then becomes zero. Both receipts remain on disk.
- The earlier zero-intent test now also uses an actual V4/no-riskgate first
  boot, rather than attaching sparse context to a legacy fixture root.

This linked scenario starts just before the next M10 close so the existing
five-minute bootstrap broker-truth TTL remains valid. No TTL was extended.
A first attempt with candidate+30s capture correctly rejected the next close
as `BrokerTruthExpired`; this is not a sparse-admission failure. Long-running
truth refresh and timing must be covered in fixed process composition / O4;
the single bounded synthetic lifecycle does not prove them. These tests are
clean restart tests, not new SIGKILL crash-frontier evidence.

The original synthetic source/lifecycle slice and the separate model replay
were not one linked real-History witness. The new bounded cross-crate witness
below closes that particular gap. It does not prove the installed process loop
or continuous fresh-source admission. Source correction remains in progress;
no intermediate source review is requested.

## Fixed materializer composition (local only)

The fixed-path O2 materializer now has explicit policy V3 selection. The new
policy requires exact no-riskgate profile, observed source-plan hash and an
operational identity matching the protected calendar template. V1/V2 reject
the new fields (including explicit null) and retain strict source V2/V3 behavior.
No policy example, installed config, permit, source root or deployment is migrated.

The calendar selects the latest nominal closed M10 and all four prior sessions
before transport. The protected policy bounds must contain that complete request;
narrow bounds cannot truncate History. The actual request remains closed and
inside seven-day M1 depth. Collection uses the same five GET route kinds. Bars
request hashes are rebuilt by the existing fixed URL renderer, and must match
the retained raw observations. No POST/DELETE, fallback or retry is introduced.

Staged package V2 retains raw snapshot once plus exact calendar template alongside
the source V4 containing the normalized receipt. Before initial publication and
on fresh retained replay, raw admission, calendar History/candidate, receipt,
source identity/generation/map, summary and Bars route evidence are cross-checked.
The retained template must equal the currently protected template byte-for-byte.
The serialized package must fit the same 32 MiB read bound before any file write.
Legacy staged V1 bytes omit the new field; neither format is silently accepted
under the other policy. A retained package is not installation/execution authority.

CLI tests use 404 actual FINAM History M10 (all 18 sparse), followed by a clearly
synthetic seven-M1 candidate. Negative cases include narrowed policy, wrong
identity/policy, null/missing evidence, incompatible schema, stale replay,
incomplete raw response, shortened and rehashed raw body, altered source
generation/map/identity, and changed summary/request/response hashes.

Run the source-inventory-bound local checks with
`python3 scripts/stage8b_sparse_m10_local_check.py --scope fixed-materializer`;
actual results are written to
`reports/stage8b-sparse-m10-local/fixed-materializer/progress-evidence.json`.
This remains local implementation evidence, not an immutable final review ZIP
or proof that the fixed process consumes this package end-to-end.

## Fixed recovery context (local, not complete process acceptance)

The production owner authenticates ordinary durable restart before restoring
the initial V4 receipt from the existing protected first-boot source file.
Configured bundle SHA and sealed first-boot provenance must both match. Legacy
policy does not enter this path. The existing S05 verify-only attach receives
the context through a crate-private function and checks exact policy/operational
identity before connecting. Manifest, stream types, exact groups, timeout and
command-audit checks remain in place; missing groups are never repaired.

For typed recovery routes that already carry independently sealed Redis ID,
semantic SHA and payload SHA, the backend reads the exact retained Redis entry
and restores its content-addressed receipt from the configured durable parent
before XAUTOCLAIM. It recomputes canonical V2 bytes against that full receipt.
The same exact receipt is used to validate the claimed entry. A newer valid
receipt cannot rebase or replace an old pending M10. No directory scan, mutable
registry, source mutation, implicit migration or new configuration path is added.

The linked synthetic paper witness now has a second variant: after S_ack and
S_truth, restart retains only the initial context (which cannot parse the later
decision). The old decision receipt is read from disk using sealed evidence;
truth/XACK complete without repeating callback, provider or command publication.
Missing, corrupt, swapped and wrong-sealed-payload cases fail before reclaim,
with PEL ownership/delivery count unchanged and no semantic effects. A fresh
Redis entry cannot use its own receipt hash to gain this recovery privilege.
An invalid fresh entry remains pending after XREADGROUP; it is not XACKed.

The S05 test uses the actual validated schema-2 bootstrap and a pre-provisioned
temporary Redis. It checks policy/context/identity mismatch before network access,
successful verify-only attachment, manifest mismatch and absent-group refusal.
The fixed process code is wired, but this is **not yet a complete sparse fixed
process/subprocess witness**. In particular:

- fresh rolling sources still need independent admission at the fixed producer
  and process boundary; disk file presence or Redis hashes are not authority;
- journal-ahead recovery was previously limited to the initial source. The
  read-only source-digest comparison described below now closes this gap;
- independently pinned retained successor selection and the real-History linked
  witness are now covered below. Automatic fresh rolling selection is not.
  No continuous-loop or fresh-truth claim.

Run `python3 scripts/stage8b_sparse_m10_local_check.py --scope fixed-recovery`.
The runner records actual output and the unchanged source inventory under
`reports/stage8b-sparse-m10-local/fixed-recovery/`; it includes S05, affected
P1d2/P1d3 and feature-enabled existing process regressions. It does not refresh
authority pins or close the final source review gate.

## Fixed staged consumer and guardian (local source wiring)

The existing operator now reads the same fixed root-owned staged file plus the
protected policy, supervisor template and source calendar template. No CLI path
override, new service, process spawn, credential access or broker transport was
added. Bootstrap schema/policy selects the parser; package metadata cannot opt a
legacy deployment into sparse admission. Policy V3 + bootstrap schema 2 require
staged V2 and source V4. Legacy policy V1/V2 still requires staged V1 and strict
source V2/V3, including omission rather than null for observed-only fields.

The consumer binds manifest/source hashes, profile/operational identity,
generation, calendar and evidence summary to those protected inputs. Raw FINAM
response verification/replay remains the fixed materializer's responsibility
before root-custodied publication. The retained raw snapshot is diagnostic input
here, not a caller-issued capability. This is not an independent REST validator.
Existing supervisor validation still checks durable-parent metadata, read-only.

Guardian now selects source V4 only through the validated bootstrap. Signed
phase checks, protected policy/template hashes, lease/lock, transaction recovery,
exact bytes and history semantics are unchanged. The test injects interruption
after materialization starts, recovers the same transaction and replays it
exactly; invalid identity/receipt and legacy-template combinations write no
installed files or history event. It uses a synthetic V4 fixture, not a VPS run.

The separate staged-consumer test uses all 404 real FINAM History buckets
(including all 18 sparse) and a synthetic seven-minute candidate. It rejects
wrong format, manifest/hash/identity, null or swapped retained template,
rehashed generation replacement, calendar changes, stale source, mismatched
summary and legacy output under observed policy. Both strict legacy profiles
have positive exact-byte replay and negative attempted-upgrade controls.

These checks are included in `--scope fixed-recovery`: materializer tests,
staged consumer, full guardian group, S05 and existing feature-enabled process
regressions. Results are emitted to the existing local report directory. This
closes source wiring of the staged consumer, **not** the entire sparse process
or the immutable source acceptance gate. No final review ZIP is claimed yet.

## Real-History to fixed-producer linked witness

`observed_real_history_fixed_producer_paper_truth_xack_restart` passes locally.
It reuses the existing feature-gated P1F-Ie composition, with explicit observed
receipt context and bootstrap-selected runtime profile. It does not introduce
a production override, runner, source registry or recovery framework.

- History is all 404 actual FINAM M10 from the four accepted sessions, including
  18 sparse buckets. The current-session prefix, candidate, decision and
  successor are **synthetic seven-M1 controls**, with both boundary minutes
  absent. Account/orders/schedule responses are also explicit test fixtures.
- The actual calendar materializer and V4 assembler feed the existing first-boot
  transaction. Actual observed fixed producer APIs publish/reread decision and
  successor in disposable local Redis; their high-water reaches sequence 2.
- After independent admission, a newer response cannot change the price or
  volume, remove a minute, or insert a previously absent minute in the overlap.
  All four mutations rehash/decode correctly in isolation but fail lineage
  validation. Producer bytes/file count, M10 stream length and PEL stay unchanged.
- Producer handles are discarded; both content-addressed receipts are restored
  from disk using hashes kept from independent admission, not Redis. The latest
  high-water is reread and verified; the predecessor receipt is still available.
- The existing consumer creates exactly one Market command, commits signed
  schedule state, cleanly restarts, commits ACK and truth, then XACKs the source.
  Command count remains 1, retained M10 count 2, final PEL 0.
- Post-truth readmission discards the rolling pair entirely. It restores the
  initial source via the configured bundle hash and authenticated seal, then
  resolves the later decision receipt using the sealed exact M10 triplet.
  The result is `AlreadyAcknowledged`, with no second command or XACK.

This uses the existing library witness, not the operational guardian executable
or a supervised child/SIGKILL. It does not prove fresh truth renewal, wall-clock
timeliness, WS readiness, or automatic receipt admission in the installed owner.

The local runner now executes this test explicitly with `--nocapture`, rejects
missing/ambiguous evidence or absent execution, checks the measured invariants,
and saves `linked-source-lifecycle.json` alongside the source inventory and logs.
`linked_sparse_durable_path_proven=true` requires **all checks of that run** to
pass on an unchanged tree. `fixed_sparse_process_end_to_end_proven` remains false.

## Journal-ahead source binding (local correction)

For authenticated S0 plus one protected `RequestAccepted`, recovery now creates
an opaque read-only source check without reconstructing a runtime or calling
Hybrid. It binds S0 seal/checkpoint/frontier/operational identity, the exact
embedded Stage5 checkpoint digest, request/command/record identity, and the
existing composite source-evidence SHA. The original wire/hash computation is
shared unchanged; no new journal record, durable schema or surrogate is added.
Legacy strict recovery does not select this new preflight.

Before XAUTOCLAIM, observed recovery reads the single exact PEL entry and uses
its envelope fields only as an **untrusted probe**. The composite must match the
protected journal independently of Redis. Only then may the existing retained
receipt resolver recompute the whole canonical M10; the claimed bytes are
checked again. The RequestAccepted suffix is not called an authenticated S1:
post-permit semantic replay must still reproduce the exact durable candidate
before S1 or command publication. Preflight grants neither.

Tests use a synthetic V4 first boot, a later independently retained receipt,
RequestAccepted without S1 and a fresh owner retaining only the initial source.
They cover successful replay/publication, missing/corrupt receipt, fully valid
but different rehashed source at the same close, copied hashes over changed
payload, and changed outer digest. Negative cases perform no claim/callback/
provider/publication/XACK; PEL owner and delivery count remain unchanged.
Journal bytes never grow during reconstruction; the seal changes only after
successful post-permit replay. A preset shutdown latch prevents callback/S1.
Core tests separately check exact BrokerCommand hash reconstruction for Market,
Limit and Cancel (not the different journal-payload hash), and foreign identity
refusal. These are clean restart/fault controls, not new SIGKILL evidence.

Run `python3 scripts/stage8b_sparse_m10_local_check.py --scope journal-ahead`.
It retains actual logs/source inventory, existing journal-ahead/legacy P1-c/S05
regressions, the feature-enabled post-permit parse check, real-History linked
witness, model replay and scoped Clippy. Its measured result is under
`reports/stage8b-sparse-m10-local/journal-ahead/`. Earlier full-gate evidence
predates this patch and is not presented as a full gate on the revised tree.

Fresh rolling admission at the fixed process boundary is still separate: a
valid file or a self-reported Redis digest must not authorize a new source.
O2 remains HOLD, and final source acceptance is not claimed.

## Explicit Published window into S05 (local, 2026-10-03)

`observed_published_window` consumes two checked fixed-producer states as input:
both Published, adjacent publication sequence and the existing same-phase or
O3-to-O4 phase-ID relation. It creates a non-deserializable bounded input with
two independently admitted receipts and exact canonical payloads. The runtime
constructor only validates those explicit inputs; it is **not** proof of HTTP
provenance or a cryptographic publication token. Receipt filenames and Redis
hashes are not its source of authority.

`Stage8bP1eVerifiedRedisSessionV1::admit_observed_published_window` is available
before the linear session transfers to the owner. Only an observed recovery
context can admit it. Initial-to-predecessor and predecessor-to-successor
overlap must agree; ends/receipt times advance, and there is no uncovered gap
between the source ranges. Both exact canonical entries are reread through the
verified namespace. Only after both match is the window installed. Admission
performs no claims, callbacks, provider calls, Redis writes or disk persistence.
Failure does not partly admit the first entry. A second admission is refused,
not used as a mutable source registry. New inputs cannot rebase the initial
range or admit other canonical payloads derivable from a later snapshot.

The real-History cross-crate witness now provisions a manifest in isolated
Redis, restores the initial source from the authenticated first-boot package
and uses the actual verify-only S05 checks plus this bounded input before the
Market callback and at schedule-committed restart. It retains one command,
durable ACK/truth, XACK-last and already-acknowledged readmission. The final
truth restart still discards the fresh window and recovers from sealed exact
M10 evidence. Published predecessor/successor input is retained in memory by
this witness; receipt/high-water disk restore is checked separately, not
claimed to reconstruct a lost fresh input automatically.

Negative checks cover Prepared/reversed/repeated producer inputs, missing first
or second Redis entry, changed payload, an extra Redis field, foreign identity,
changed initial overlap, repeated admission, rebinding an old bucket and use of
fresh input as a substitute for sealed recovery evidence. Redis stream DUMP,
PEL, command count and receipt-file inventory remain unchanged by admission.

Run `python3 scripts/stage8b_sparse_m10_local_check.py --scope published-window`.
Evidence is in `reports/stage8b-sparse-m10-local/published-window/`. Proof flags
require execution of both new named regressions, journal-ahead tests, the
linked witness, model replay and all scoped gates on an unchanged tree.

The first scoped run stopped at the broad gateway suite: two pre-existing
issuer-root tests hit `AlreadyExists` in their PID-plus-counter temporary path
helper (499 passed / 2 failed / 2 ignored). The failed run and its source
inventory are retained in `published-window-attempt-1/`. Only that `#[cfg(test)]`
helper now uses the already available UUID instead; no authority/runtime code
or old evidence directories are changed. The scoped gate is rerun in full.
The next run passed its test groups but strict Clippy found the new producer
handoff had not been re-exported from the gateway crate. This is corrected by
the explicit `stage8b_p1f_observed_published_window` export (also used by the
linked witness), not a warning suppression. That run is retained separately
in `published-window-attempt-2/`; the final scoped run must pass independently.

This is a bounded **in-process** handoff, not fixed-CLI/daemon source discovery,
continuous history polling or WS readiness. Those are not inferred from a pure
constructor or this clean restart test. No new IPC, source registry, signing
key, source-directory scan, background poller or deployment was introduced.

## Shared fixed-path startup (2026-10-03)

`execute_stage8b_p1e_observed_run_v1(PublishedWindow)` is the additive **in-process**
entrypoint. It loads the same fixed protected config and boot identity, rejects
legacy profiles, and delegates to the same signal-supervised `run` owner.
Credentials, V5 seal/admission and initial protected first-boot source restoration
are unchanged. After S05 and before consumer hygiene/S06, the shared helper
admits the exact published pair under the shutdown latch. A missing initial
context or incompatible pair fails closed; a stop returns no session. Neither
partial verification nor shutdown can grant an acquisition capability.

The real-History linked witness now calls this production startup helper, not
a parallel test implementation of its steps. The seven-case Redis admission
test exercises both the session API and shared helper. It additionally verifies
missing context, preset shutdown and cancellation against paused disposable
Redis, preserving stream bytes, PEL, command count and receipt inventory.
Existing process signal regressions cover the unchanged supervisor.

This does **not** add a new installed CLI verb, input path, serialization of the
window, IPC, automatic polling or on-disk fresh authority. The accepted IC fixed
producer was already a typed adapter; its observed counterpart likewise supplies
typed input, not a FINAM network daemon. End-to-end installed sparse O3/O4 and
live source renewal require later artifact/installation/operational gates. Do
not turn this source correction into that separate activation work.

## Final source verification and review boundary

The transaction/seal slice is checked with
`python3 scripts/stage8b_sparse_m10_local_check.py --scope transaction-seal`.
Its actual logs and source-inventory-bound result are under
`reports/stage8b-sparse-m10-local/transaction-seal/`. This adds bootstrap and
transaction regression groups to the previous producer/Redis checks. It does
not claim a complete linked order/fill witness or full durable regression run.

1. Fixed materializer selection, staged consumer, guardian, first-boot
   transaction, sealed source recovery and bounded in-process producer/startup
   composition are implemented. Installed sparse process execution is not claimed.
2. Initial, fresh two-entry and sealed/journal-ahead recovery authorities remain
   separate. Do not generalize recovery resolvers to fresh Redis messages or
   infer continuous admission from the bounded handoff.
3. Run affected durable/gateway/compatibility regressions and scanners; produce
   one actual immutable ZIP, SHA-256, safety report and honest gate evidence.
   Governance rebind follows accepted source changes, never silent pin refresh.

## Retained operational work

The owner's WS/operational-parity reminder is recorded in the existing roadmap
and [market-data checklist](stage8b-operational-market-data-parity-checklist.md).
Actual subscription readiness, reconnect/resubscribe, bar freshness/duplicates,
REST/live overlap, delivery latency, session breaks/EOD and paper lifecycle
remain O4 / subsequent paper-session observations. Historical replay is not
their acceptance, and this local work does not authorize those sessions.

## Operational boundary unchanged

No VPS, service, DB0/P0, terminal history `FAILED/1/6`, receipt or installation
changes. No broker data requests, execution transport or real orders. Only
public documentation was consulted. O2 remains **HOLD / NOT PASSED**. Artifact,
install and a separately authorized bounded O2 follow source acceptance.
