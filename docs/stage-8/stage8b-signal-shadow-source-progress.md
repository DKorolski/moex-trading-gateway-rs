# Signal-shadow r0 — source progress

2026-10-23. Scope accepted; source IN PROGRESS, no deployment.
Authority: [review + S1–S10](reviews/REVIEW_SIGNAL_SHADOW_SCOPE_TZ_20261023_RU.txt).
Owner selected September 27 historical ALOR freeze, not current live.

## R1 review closure candidate

Current local changes and retained limitations are consolidated in
`stage8b-signal-shadow-source-review-r1.md`: rolling quote dedup, real-header
semantic identity, independent state/reason reference, bounded prefix
reconstruction, raw quote retention and final EOD bucket. The increments below
are historical development notes, not the current acceptance verdict.
Only the sealed package logs establish which checks actually completed.

## Latest increment — measured callback timing fence

Added `timing::CallbackFence`, bound to config fingerprint/start/deadline/Lmax/
clock uncertainty and shared StopHandle. Production execution reads real UTC
and monotonic clocks; injected clocks exist only in private unit-test calls.
The fence validates close→receive→canonical-ready→callback-start chronology
and total close-to-result latency including clock uncertainty. It retains all
four observation timestamps and measured callback nanoseconds.

Late history cannot enter through this fence as timely input. Stop, backwards
clock, UTC/monotonic divergence, deadline or latency violation holds the fence.
Callback error/panic also leaves it held. Post-callback failure does NOT roll
back model state; a retry is refused. Synchronous callback execution is not
preemptible, so this is a post-return latency check, not proof of hard process
termination within stop_timeout.

Four deterministic tests cover timing evidence, refusal before callback,
late callback with one effect/no retry, stop during callback and wall-clock
jump. These tests exercise the fence with closures, not the final live model
coordinator. Source admission, timestamp provenance and feedback decision-time
binding still need integration; no LIVE_OBSERVED promotion was added.

Remaining: compose fence/model/observer with admitted input and causal feedback,
then local EOD/NEW_SEGMENT/process evidence and consolidated source package.
No transport or VPS was started.

## Previous increment — one run-config and stop owner for transports

Added `transport::ConfiguredTransports`. It binds the exact validated config
fingerprint, fixed instrument and run deadline to REST/BARS/QUOTES. All three
budgets share the same monotonic deadline and one-way stop channel, not three
independently calculated lifetimes. Subscription budgets come from config;
IO cap is explicitly min(15 seconds, stop_timeout, ws_silence), separate from
model Lmax. BARS/QUOTES initial construction can each be consumed only once.

Constructor rejects unsupported instrument/capacity/budget values and expired
run deadlines. It is not calendar/seed readiness or operational authorization.
No config secrets are read. Existing lower-level APIs remain library seams;
the final executable must exclusively use this composition entry.

Failed/cancelled connect or REST collection signals the common stop through a
drop guard. Dropping the configuration owner also signals stop even when an
external handle clone remains. Quote and bar socket owners still require the
future coordinator to drive/drop them and propagate observe/reconnect errors;
no autonomous process supervisor is claimed here.

Configured max_input_bytes is checked before handing a completed REST snapshot
to consumers. This does not reduce the existing HTTP client's 32 MiB allocation
cap during collection. That distinction remains explicit.

Three added tests cover simultaneous stop of all three in-flight budgets,
failure/cancellation propagation, exact config fingerprint and expired/wrong
instrument refusal. 66 Rust tests PASS; no network/VPS activity.

Remaining: model/observer/timing and EOD/NEW_SEGMENT process composition, local
process acceptance evidence and source-review packaging. Transport construction
alone is not an operational runner.

## Previous increment — dedicated QUOTES socket composition

Added `QuotesSocket` as a narrow wrapper over the same fixed-endpoint socket
owner, not a second transport implementation. The protocol's subscription kind
is selected at construction and cannot change. Each socket owns exactly one
BARS or QUOTES subscription. After handshake the existing FINAM request builder
sends the corresponding request. Data is accepted only for the selected type
and stable subscription key before quote normalization.

Quote normalization/dedup state belongs to the protocol and survives the one
explicit reconnect; old-generation callbacks remain rejected. Repeated quotes
do not extend the new-data silence budget. No automatic resubscribe/renewal,
model execution or credentials discovery has been added.

Four new tests: in-memory socket handshake→QUOTES request→normalized quote→
duplicate suppression; protocol reconnect retaining dedup; channel/key conflict;
duplicates failing to mask data silence. Together with existing tests:
63 Rust tests PASS. No external TLS/FINAM session or physical reconnect test
was performed; these remain operational evidence, not inferred from mocks.

Remaining source work: bind both channels and GET collection to a single
validated run/config and stop policy; live timing/model/observer composition;
bounded process/EOD/NEW_SEGMENT tests; consolidated source-review evidence.
No change to accepted paper state or operational deployment.

## Previous increment — quote normalization and causal feedback boundary

Added `quote::QuoteInput`, reusing the FINAM QUOTES mapper. It accepts bounded
DATA/QUOTES frames with explicit IMOEXF@RTSX identity, source timestamp and last
price; missing values are not replaced by receive time, bar close or bid/ask.
Decimal arithmetic converts tick=0.5 exactly to integer ticks, with no floating
rounding. Invalid/nonpositive prices, crossed spread, future/stale/reversed
timestamps, conflicting same-time quotes and capacity exhaustion hold.

Dedup is segment-local and bounded to at most 4096 timestamps, no eviction.
Frame validation is atomic. Equal duplicates produce no second feedback input;
identity excludes receive time. Nanosecond timestamps are preserved for dedup;
the existing millisecond feedback schema floors them conservatively, so a quote
within the decision millisecond is not considered strictly post-intent.

The mapper's timestamp is quote source time, NOT independently proven last-trade
time. Last is a hypothetical quoted-price observation, not broker execution.
This selection and raw-source provenance still need binding in final composition;
no real-fill/parity acceptance follows from normalization.

Four tests cover exact tick/dedup/one virtual effect, missing/offtick/stale/future
refusal, conflicting/out-of-order/capacity refusal, pre-intent pending preservation
and crossed spread. All messages and credentials are synthetic. rust_decimal
becomes a direct dependency using the existing workspace version.

This is not yet an active QUOTES subscription: the caller must first bind frames
to its dedicated connection/subscription. Remaining: quote socket integration,
full run-config/clock/model composition and process/EOD/restart checks before
source packaging. No FINAM request or VPS modification.

## Previous increment — bounded socket owner (bars)

Added `socket::BarsSocket`: fixed `wss://api.finam.ru/ws`, existing WebPKI TLS
configuration, bounded frames/buffers and explicit token lease supplied by caller.
The initial upgrade uses a sensitive Authorization header; after provider
handshake, the existing request builder sends the fixed BARS/M1 subscription.
No arbitrary endpoint, credential loading/renewal, order messages, retry loop,
background task or live CLI switch was added. Transport errors are sanitized.

The socket and its protocol are owned together. During each observe future the
socket is moved out of reusable state: failure, stop or caller cancellation drops
it; a later call refuses. A ping is flushed under the same bounded operation,
not considered data readiness. Explicit reconnect consumes the old owner,
drops the old socket before validating/opening a new generation, retains the
original run budget and refuses a third connection before network IO.
Local stop drops the transport immediately; graceful peer close is not promised.
The owner must be driven/dropped by the future process coordinator; an idle
undriven object is not an autonomous stop supervisor.

Tests use real WebSocket framing over Tokio in-memory duplex streams (not HTTP
upgrade/TLS or FINAM): handshake→subscription, ping/pong, stop/read cancellation,
no reuse, malformed frames. Socket open on the fixed external endpoint remains
unexecuted. Direct futures-util/tokio-tungstenite dependencies reuse workspace
versions. No operational secrets or VPS changes.

Remaining: quote path, configuration/clock/model/process composition, measured
stop/EOD/restart evidence, and source-review package. Bars socket existence is
not operational S5 acceptance or permission to run.

## Previous increment — single-subscription WS protocol gate

Added `ws::BarsProtocol`, using the existing FINAM envelope decoder/market-data
mapper and subscribe-request builder. This is a local protocol gate, NOT yet a
socket driver. Its fixed subscription is IMOEXF@RTSX / TIME_FRAME_M1. Handshake
must precede request creation and successful send acknowledgement; a second
subscription or simultaneous connection is refused. Subscription JSON contains
the supplied token and must go directly to the socket, never evidence/logs.

Opaque owner+generation identity rejects foreign/stale callbacks without
mutating the current connection. One explicit reconnect is allowed only after
the caller confirms physical socket closure and within its configured budget.
Actual physical closure still needs driver tests; this API cannot prove it.
Provider/parse/clock/silence faults hold the protocol rather than auto-retry.
Stop is terminal. Heartbeats do not refresh the data-silence clock.

Frames are bounded to 256 KiB and 512 M1 rows. Explicit instrument/timeframe
conflicts, timestamp overflow/future/staleness, invalid OHLCV and changing
subscription key refuse before observation state updates. Mapper output is
deliberately forced provisional: elapsed wall time is not REST completeness
proof. No WS bar can be passed as model-final through this gate.
Silence uses caller-supplied receive time; the future socket driver must provide
real timestamps and monotonic stop/deadline enforcement. This is not live READY.

Five local tests cover handshake/subscription, provisional mapping, stale and
foreign generations, reconnect budget, heartbeat silence, terminal stop and
bad provider/data frames. They use synthetic messages and no network.

Remaining: actual bounded socket owner and quote path, live clock/config/model
composition, process/EOD/restart evidence and immutable source packaging.

## Previous increment — bounded GET-only collector

Added `transport::ReadOnlyCollector` around the existing fixed-route
`Stage8bP1fO2GetOnlyClientV1::fetch_closed_m1_snapshot`. It returns only an
admitted closed snapshot checked against the caller's original request plan.
Credentials/client/plan are explicitly supplied, not discovered or persisted.
No order client, retry, renewal, background task or operational CLI is added.
The file-only executable remains file-only. Tokio sync/time is now a direct
dependency; the workspace lock is updated without a dependency version upgrade.

Run duration is bounded by both absolute UTC and monotonic deadlines (at most
24 hours); each request has an explicit 0–15 second nonzero budget. A one-way
stop handle cannot reset. Sender loss stops collection. Stop has priority over
ready work, and time/stop checks repeat before returning. Clock reversal holds.
Failure, timeout, or caller cancellation leaves the collector held, without
automatic retry. Completed snapshot admission is synchronous bounded-input
validation; this is not yet proof of a whole-process stop-time bound.

Local tests use futures only, not HTTP: stop-before-poll, inflight cancellation
and resource drop, request timeout, sender loss, sequential success, outer
cancellation and fetch failure. No token was read or real FINAM call performed.
Existing fixed transport still supplies its 32 MiB response bound; tighter
run-config input budgeting must be enforced in final composition.

Remaining before source packaging: WS subscription/reconnect and quote path,
explicit run-config composition, actual-time readiness/model timing,
EOD/missing-data/NEW_SEGMENT and bounded process-stop checks. This collector alone
does not authorize network operation or establish S5/S6/S10 acceptance.

## Previous increment — admitted closed-source input adapter

Added `input::ClosedInputAdapter`, reusing broker-finam's existing
`AdmittedClosedM1Snapshot` and sparse M1→M10 derivation. No second aggregation
algorithm, O4 producer, Redis writer or runtime owner is introduced. The new
production dependency is broker-finam (and its existing HTTP dependencies);
this adapter instantiates no transport/client and performs no IO. This updates
the earlier dependency-free transport-boundary description; source review must
include the dependency addition and Cargo.lock change.

The caller supplies independently fixed required M10 labels and an already
admitted snapshot. Empty required buckets reject the entire batch; no partial
overlap state advances. The adapter preserves nominal start labels and actual
minute bitmaps. Sparse source is accepted only through existing closed-snapshot
admission, never by accepting a provisional WS bar or treating silence as proof.

Exact overlap returns no second input; overlap compares all actual M1 rows, not
only aggregate OHLCV. A changed minute inventory with equal M10 OHLCV conflicts.
Wrong instrument, unknown older label, cache exhaustion or invalid range holds
the adapter for the rest of the segment. Cache is bounded (at most 4096 M10
inventories), with no eviction or hidden recovery. Conversion produces Replay
only, not LIVE_OBSERVED or operational READY.

Tests use synthetic raw REST bodies admitted through the existing checker.
They cover sparse mapping/exact duplicate, empty required bucket, equal-OHLCV
minute conflict, capacity and instrument mismatch. These are not actual provider
observations. The existing snapshot trust boundary still requires honest
transport completion metadata; this adapter does not independently prove HTTP.

Remaining: connect a bounded GET-only/WS driver, record actual timestamps and
enforce readiness/stop/reconnect/EOD rules, then compose with the model and
package source evidence. No operational start/deployment was performed.

## Previous increment — conditional full-session round comparison

Added the test-only experiment
`frozen_five_sessions_rounds_under_explicit_synthetic_close_feedback`,
sharing pinned inputs/seed preparation with the first-entry regression.
All 101 observed bars per September 21–25 session are processed (505 total).
The model itself selects both entry and exit; the reference never injects an
intent. Each actual request receives a synthetic close-price quote explicitly
identified as SYNTHETIC_CLOSE_NOT_OBSERVED. Artificial +1/+2/+3 millisecond
event/receive/apply ordering is test instrumentation, not historical latency.
Production configuration, feedback policy and runtime code are unchanged.

Result: five entries and five exits match the independent Python reference
bar labels/directions and conditional close prices, with exactly two intents
per session, no extra later entry and a flat end in each session:

| Session | Entry MSK | Exit MSK | Side |
|---|---|---|---|
| 2026-09-21 | 12:30 | 14:50 | short |
| 2026-09-22 | 10:30 | 23:30 | long |
| 2026-09-23 | 22:40 | 23:30 | long |
| 2026-09-24 | 12:00 | 15:50 | short |
| 2026-09-25 | 10:00 | 10:50 | short |

This is CONDITIONAL historical signal/round agreement, not fill-policy
equivalence or live parity. Synthetic quotes do not prove availability of a
post-intent quote. Fees/PnL, detailed exit-reason vocabulary, full state traces,
cross-session retained state, restart and actual data latency are not compared.
Each session independently receives a four-session warmup. Test evidence is
temporary and must not be presented as an operational observation package.
The separate no-feedback first-entry test still runs and remains independent
of this synthetic price assumption.

Reproduce:
```text
cargo test --offline --locked -p finam-signal-shadow frozen_five_sessions
```

## Previous increment — independent first-entry comparison (five sessions)

Added an executable Rust integration regression
`frozen_five_sessions_first_entry_matches_independent_python_reference`.
Both frozen CSV hashes are asserted before interpretation. Each September
21–25 session starts independently with its preceding four explicitly listed
weekday sessions (404 observed M10 bars). The test calendar is an explicit
07:00–23:40 nominal grid, not an operational exchange-calendar authorization.
Every observed session bar is processed from 07:00 until the first actual model
intent; therefore an earlier unexpected intent fails, not just a missing expected
one. The reference is the frozen Python CSV, not another run of the Rust model.

Observed first-entry agreement (MSK candle-start labels):

| Session | First entry | Side |
|---|---|---|
| 2026-09-21 | 12:30 | short |
| 2026-09-22 | 10:30 | long |
| 2026-09-23 | 22:40 | long |
| 2026-09-24 | 12:00 | short |
| 2026-09-25 | 10:00 | short |

The current model quantity is asserted as one lot; Python rounds do not prove
broker quantity parity. Each run ends with INCOMPLETE_virtual_pending and no
feedback rows. This proves first-entry label/side agreement only, not complete
state/exit/round/price parity or live timeliness. Test evidence is temporary.
The unreleased observer field was renamed raw_provenance_sha256: the frozen M10
CSV hash is NOT an M1 provenance assertion. Its digest alone does not establish
source coverage; the input adapter must supply and verify underlying evidence.

Inspection of the frozen Python run_audit.py confirms that baseline07 also uses
rounded current-bar close plus/minus slippage for hypothetical entry/exit.
It is not only a candidate09 rule. That policy differs from the current shadow
post-intent-quote rule; no substitution was made. A declared experimental fill
policy and actual quote evidence must not be conflated in subsequent comparison.

Reproduce:
```text
cargo test --offline --locked -p finam-signal-shadow frozen_five_sessions_first_entry_matches_independent_python_reference
```

## Previous increment — real historical reference inventory

Added read-only, standard-library-only
`scripts/signal_shadow_reference_inventory.py`. It pins the raw M10 and
baseline07 Python rounds hashes, checks UTC/MSK label equivalence, availability
at label+10m, monotonic unique labels, finite and consistent OHLCV, and reports
nominal-grid differences without inventing missing candles or calendar authority.

Reproduced against the repository fixtures; their hashes also match the original
September 27 freeze directory byte-for-byte:

- Raw M10: 7307 rows, SHA-256
  `e4a30fb81a0abf5711a3702d19b918cad681820380f24422f6ba476b74f92cec`.
- Baseline07 Python rounds: SHA-256
  `2accaf0eec6c81f3446832059c231471415094c6cf43dc91e6ddbba85ccfe384`.
- Proposed historical seed: September 15–18 (four sessions).
- Historical replay window: September 21–25; five reference BO rounds.
- Each of these nine dates has 101 observed baseline07 M10 labels, 07:00–23:40
  MSK, with no missing nominal-grid labels.

The raw file ends September 25. The earlier September 22–25 seed test cannot
prove a September 28 replay from this package. This inventory does not establish
an independently authoritative exchange calendar or current ALOR live state.

Important execution limitation: these CSVs contain M10 and Python round prices,
not a post-intent quote stream. The current first-post-intent-quote policy cannot
be evidenced by fabricating quotes from candle closes. Signal/state comparison
and a declared hypothetical execution experiment must be separated; actual fill
and latency parity remain unproven. No policy was silently changed.

Reproduce:
```text
python3 scripts/signal_shadow_reference_inventory.py --fixtures fixtures/stage8b-p1f-parity
python3 -m unittest discover -s scripts -p test_signal_shadow_reference_inventory.py -v
```

Four tests cover actual pinned input/window, changed bytes, duplicate/clock/OHLCV
corruption and explicit missing-label reporting. No network/VPS/operational
changes. Next: bind this historical reference to comparison input preparation,
keeping synthetic feedback distinctly labelled; live input/quote adapter and
bounded process checks remain required before source review.

## Previous increment — file-only executable

The package now builds a local executable:

```text
finam-signal-shadow replay CONFIG REFERENCE MAPPING CALENDAR SEED EVENTS NEW_OUTPUT
```

CONFIG is the strict ShadowConfig JSON; reference/mapping/calendar/seed bytes
are bound by its hashes before model construction. EVENTS is a JSON array
(1–4096 records) with tagged bar/quote records:

```json
{"kind":"bar","bar":{},"raw_provenance_sha256":"<64 lowercase hex>","virtual_result_at_ms":0}
{"kind":"quote","quote":{},"virtual_applied_at_ms":0}
```

The empty objects/time values above describe keys only, not valid model inputs.
Bar and Quote use the library schemas. Times are virtual, all model output is
BACKFILL_REPLAY. The supplied provenance digest is not independent source proof.
No network or credentials are read. Files must be trusted local inputs; this is
not a filesystem sandbox. Nonregular/final-symlink inputs refuse. Config is
limited to 16 KiB; the remaining inputs share a budget capped at 16 MiB and the
configured max_input_bytes. Parsing/binding completes before output creation.

Exit 0 means only offline end-flat; exit 2 means pending/nonflat incomplete;
exit 1 means input/model/evidence failure. Failure after creation preserves
partial evidence and never resumes or truncates it. New output is exclusive.
Flat end is not coverage, EOD timeliness, parity or operational readiness.

Validation: 36 tests PASS, including subprocess execution, no-overwrite,
malformed-event refusal before output creation and unresolved pending exit 2.
The pending fixture includes the initial session bar before the breakout bar;
starting at the breakout bar alone correctly produced no entry in the model.
Real historical input preparation and read-only live/process checks remain
open; no deployment or source acceptance is claimed.

## Previous increment — unified offline model/feedback/evidence cycle

`replay::OfflineReplay` consumes the bound `SeededSegment` and owns the fixed
model and new-file observer. Replay uses HistorySim/CatchingUp callbacks, not
live execution. Every decision is obtained from the actual model; virtual quote
feedback updates that model before the next step. Evidence failure terminates
the coordinator and prevents subsequent callbacks; it does not claim rollback
of the callback that preceded a failed write.

Exact last-bar replay is a no-op without another evidence row. Live-origin bars,
out-of-window or backwards virtual timestamps refuse. Processing timestamps
are explicitly virtual replay time, not measured operational latency. Output
contains WARMUP/BACKFILL_REPLAY only, never LIVE_OBSERVED. End records distinguish
flat completion from incomplete pending/nonflat state.

Validation: 34 tests PASS, strict all-targets package Clippy PASS. Four new
integration tests cover unified no-signal/dedup, terminal evidence-budget failure,
pre-callback input refusal, and actual model entry → virtual fill → EOD exit →
flat finish. The EOD case is synthetic and skips intermediate bars; it is not
full-session coverage or ALOR parity evidence.

Remaining within the existing source slice: executable/input wiring and real
reference/calendar provenance; report-level comparison; read-only live input
and quote adapter; bounded process/deadline/EOD/restart verification; intentional
source diff and immutable review package. The library coordinator is not yet a
CLI or an operational runner. No VPS, deployment, source acceptance or live
authorization is claimed.

## Previous increment — bounded observer and pairwise comparator

`observer::Observer` creates a NEW JSONL file exclusively; it never resumes,
truncates or appends to an old segment file. The caller must supply a trusted
isolated parent directory. Existing files/final-component symlinks refuse.
Seed/model/no-signal/feedback/end records retain run/segment/config identities.
Each successful row is written and synced; output/row limits are explicit.
Validation/IO/limit errors make the writer terminal. Partial IO failure may
leave an incomplete last row; no retry/recovery success is claimed.

Model steps include modes, nominal close and receive/processing timestamps,
canonical-input/raw-provenance hashes, before/after state and decisions/reasons.
Hypothetical feedback is a separate mode-labelled event, not retroactive
mutation of an earlier decision. Duplicate/nonmonotonic step labels refuse.
The writer does not independently prove that a supplied step/feedback came from
the model: coordinator composition remains required.

The pairwise comparator separates NOT_COMPARABLE, DIFFERENT_INPUT,
SAME_INPUT_MATCH and SAME_INPUT_MODEL_DRIFT. Host request-ID values are ignored,
but pending-ID presence is retained. Timestamp latency checks remain separate
from semantic equality; SAME_INPUT_MATCH is not timely-delivery or ALOR parity
acceptance. Profile/seed/reference compatibility must be established by the
future report-level comparator, not guessed from a pair of Step objects.

Validation: 30 tests PASS, strict package Clippy and fmt/diff PASS. Six new
observer tests cover new-file-only evidence, no-signal rows, bound refusal
without append, duplicate/bad-clock refusal, symlink preservation and comparison.
No live run, process-crash evidence or operational acceptance.

Remaining: actual reference/inventory preparation, unified model+observer
execution, report-level comparator and read-only input adapter, then bounded
process/deadline/EOD/restart tests and source packaging. This does not add a
separate review stage or new recovery framework.

## Previous increment — bound seed-to-model composition

`segment::SeededSegment::from_inputs` combines exact binding with fresh
model construction. No caller-supplied model/context/receipt is accepted by
this entry. Paper/flat/SyncingHistory context and local request namespace are
constructed internally from validated run identity and mapping. The model stays
private; the currently exposed result is a read-only seed observation.

The exact raw seed is checked before decoding; its canonical decoded hash is
then passed to the existing warmup adapter. Config/seed/profile/reference/
feedback identities, run/segment/previous-segment linkage, history frontier
and state are preserved in `SeedObservation`. Its mode is always WARMUP and
live observation count is zero. BACKFILL_REPLAY/LIVE_OBSERVED are schema values,
not implemented execution paths yet.

Added an explicit `max_quote_age_ms` config field: quote freshness is not
silently borrowed from the separate model latency limit. Conversion overflow
refuses. All values still require an actual approved run config.

This initial constructor supports 07:00 MSK weekday boundary seeds only;
mid-session initialization refuses until a proper prefix/state path exists.
Weekday/time checks do NOT prove exchange-calendar eligibility. A previous
segment ID is recorded, not proof that its output/frontier has been retained.
This is not a complete NEW_SEGMENT operational restart implementation.

Validation: 10 unit + 6 model integration + 8 seed/composition tests PASS;
strict package Clippy and fmt/diff PASS. Synthetic inventory tests remain
separate from real historical coverage proof. No VPS or operational writes.

Next: real reference/calendar inventory provenance; bounded append-only
observer and comparator, then live data/quote adapter and process-level
deadline/stop/restart/EOD handling. No source acceptance/operational READY.

## Previous increment — exact run/seed and inventory binding

Added `seed::bind_seed` and required `seed_sha256` in the not-yet-released
shadow config. It checks the fixed model fingerprint and feedback-policy hash,
exact reference-manifest/mapping/calendar/seed bytes, run/segment config identity,
instrument/tick/lot mapping and input/cache bounds. A diagnostic receipt binds
the configuration and seed; it grants neither readiness nor restore authority.

Calendar inventory contains explicit required M10 labels for four sessions;
it is separately hash-pinned, NOT inferred from received history. Every label,
order, bar and exact range boundary must match. Removing an interior bar and
rehashing the seed still fails coverage. Nonfinal/mismatched-instrument/invalid
OHLCV input refuses. No missing candle is fabricated and no 10-M1 density rule
is reintroduced.

Five new tests use an explicitly synthetic calendar to verify binding/negative
cases, not actual MOEX coverage. The real reference manifest, mapping and
calendar inventory still need preparation/provenance verification; matching
arbitrary resealed inputs cannot prove external truth. The small reference
manifest is the hashed artifact here, not the entire historical ZIP.

Validation: 21 tests PASS, strict package all-targets Clippy PASS, fmt/diff PASS.
Next: compose this receipt check with model construction so the executable
cannot skip it; prepare exact historical inventory and bind run-mode/frontier
identities in the observer/comparator. No CLI/deployment/operational READY yet.

## Previous increment — fixed profile and fresh history seed

Production `profile::baseline07_config` no longer needs the durable-service
constructor. It pins the exact profile JSON SHA-256
`b267f680d2d376d15c62d347acf89dc9dd26d8ce0a850a66f2dd7710d44282be`.
Its complete model-config fingerprint matches the existing V2 constructor in
an integration regression. The service dependency remains test-only.
Public arbitrary-model injection was removed from ShadowModel; its constructors
select the fixed no-riskgate config. Profile-to-run-config binding remains open.

A narrow core adapter constructs a NEW flat model and invokes the existing
history-warmup algorithm; it never resets a retained model/owner. It validates
closed, ordered final History M10 inputs, target instrument, no-riskgate/BO-only,
paper-only context and a 4096-bar bound. A detached read-only state projection
exposes anchors, side-used, pending IDs and owner for the future comparator;
it supplies no restore token, setter or lifecycle authority.
This is a new public source surface to include in source review/enforcement;
no claim of freeze/governance acceptance is made.

Host seed construction binds the canonical history hash and four explicit
session dates, rejects current/future-session seeds, requires observed 07:00
and 23:40 boundaries for this baseline, and calls the core adapter.
Tests use real frozen sessions 2026-09-22 through 2026-09-25, with the next
session at 2026-09-28 07:00 MSK. Flat/no pending and close 2275.5 are confirmed.
Wrong hash, missing boundary, future input and duplicate timestamp refuse.

IMPORTANT: these boundary checks do not prove every required interior bucket
or exchange-calendar eligibility. Source coverage/calendar admission and any
mid-session prefix/state remain OPEN; the returned host is not operational
READY. The cold constructor likewise does not authorize a cold-flat live start.
No multi-month fallback or High180 history was added.

Validation: 10 unit + 6 integration tests PASS, strict package all-targets
Clippy PASS. No operational S1–S10 evidence or aggregate acceptance is claimed.
Next: exact run/profile/seed binding and coverage, observer/comparator with
mode/frontier identities, then read-only executable/data composition.

## Previous increment — actual model callback composition

`model::ShadowModel` now owns the existing HybridIntradayRuntimeStrategy and
calls the public broker-neutral bar/position callbacks. Supported output is
one exact-quantity Market intent; unsupported/multiple outputs stop the segment.
The virtual book feeds the resulting position back to the actual model before
the next bar. No Stage6/7 owner or provider is attached.

An exact repeated last bar is a no-op. Conflicting/older, unfinished or invalid
OHLCV input stops the segment; pending feedback blocks advancing to the next
bar. Callback/feedback failure leaves the segment held, not silently retried.
This is a local composition boundary, not validated warmup/seed/profile admission.
The constructor's supplied model is not yet checked against the fixed profile;
that binding remains required before any executable/readiness.

Three integration tests use the real no-riskgate V2 model factory and the frozen
CSV. They prove actual model-generated entry → post-decision synthetic quote →
position callback → later model-generated exit → flat, exact-bar duplicate
suppression, input conflicts and pending/invalid-quote refusal.
The historical CSV is deliberately delivered as synthetic Live callbacks for
this local test, NOT recorded as LIVE_OBSERVED evidence. There is no assertion
of four-session warmup correctness or ALOR same-input parity from these tests.

`runtime-durable-service` is a DEV dependency only, used to obtain the
existing exact V2 model constructor in integration tests; no service starts.
Production dependencies added: broker-core and strategy-runtime-core. Their
current dirty-tree changes still need an explicit intentional-diff inventory.
No changes to model semantics or private-interface visibility were made.

Remaining immediate work: fixed profile construction/binding without the service
dependency, four-session warmup/seed, observational state projection and exact
input/output frontier. Historical foundation notes follow.

## Dependency selection

- Initial isolated `crates/finam-signal-shadow` foundation used only
  serde/serde_json/sha2 dependencies. No executable, transport or model adapter
  at that point. See the latest increment above. There is still no working
  executable runner or readiness capability.
- Existing `finam-paper-runtime-consume` constructs a Redis-backed
  PaperRuntimeAdapter, optional oracle seed and stream publication.
  Do not use it as-is for the isolated target.
- Existing P1-d1 paper-provider requires a Stage6 dispatch receipt.
  Do not connect its owner chain or expose bypass constructors for shadow.
- Existing BrokerNeutralHybridStrategy exposes bar/position/ACK callbacks.
  Inspect these for the thin model adapter; keep the legacy Strategy private.
- Existing sparse replay uses immediate-current-close position simulation.
  That test assumption is NOT the selected baseline07 execution rule.
  Feedback causality and entry→feedback→exit proof remain open.

## Implemented foundation

Typed config binds model/reference/feedback/mapping/calendar identities,
four ordered warmup date strings and range, observation/deadline, latency/clock,
WS/reconnect/stop and input/cache/output budgets. Missing/unknown/duplicate JSON
fields refuse. Selected restart mode is NEW_SEGMENT.

This is syntax/bounds validation, not proof of trading sessions, source hashes,
anchors, fill policy, allowed endpoints or actual instrument mapping.
Fixture budget values are test data, not accepted production thresholds.
No deployment config with guessed thresholds is supplied.

Verification: five config tests PASS, zero ignored. These are foundation tests,
not S1–S10 acceptance or a model invocation witness.

## Virtual feedback kernel increment

Added pure `feedback::VirtualBook`: one pending intent, signed integer-lot
position, integer-tick price, increasing local sequence, no broker IDs or IO.
Selected explicit provisional comparison assumption:
`moex.shadow.first-eligible-post-intent-quote.full-fill.zero-cost.v1`.
Full exact lots, zero costs/slippage, no rounding. A quote event must occur
strictly after the actual decision time; apply time cannot precede receive.
Quote age is explicitly bounded. No current-close shortcut.

This is an idealized diagnostic policy, NOT a statement of ALOR broker fills.
Both comparison contexts must use it, fingerprint it, and distinguish resulting
conditional parity from historical reference fill policy. A trade/quote data
adapter still must establish provenance, ordering and eligibility; arbitrary
quote construction is not source evidence. No claim of live-feed support yet.

Entry/exit requests do not change the position until eligible feedback.
Missing late quote leaves virtual pending/nonflat, including at EOD.
Same sequence cannot be applied twice; extra entry/reversal/oversize quantity,
wrong-instrument/future/stale quote refuse without clearing pending.

Five kernel tests plus five config tests pass; strict package Clippy and fmt
pass. These tests verify the book, NOT the real model's pending/owner/side-used.
The public broker-neutral model callbacks were inspected, but the model adapter
and entry→model-feedback→exit integration test remain the immediate next work.
No Stage6/7 dependency was added; no operational or readiness claim is made.

## Remaining source work

1. Fixed model/profile and reference identity; causal feedback, supported
   callback adapter, seed/warmup and entry→feedback→exit local witness.
2. Canonical feed/dedup/frontier, observations/comparator, NEW_SEGMENT replay
   classification, EOD/status and bounded stop.
3. Thin executable/read-only adapter, local matrix checks, intentional diff
   and shared dependency inventory, immutable source handoff.

Actual WS/reconnect/session/restart/EOD observations require source acceptance
and separate deployment permission. O4 remains deferred; paper +1 is untouched.
