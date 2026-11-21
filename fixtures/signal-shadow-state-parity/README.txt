S2 independent historical state evidence — 2026-10-23

Verdict: S2 local historical five-session state/action/reason parity PASS.
All 495 no-intent rows now have independent branch-trace comparisons, not
post-hoc inferred suppression labels. This is the declared S2 dataset/profile/
seed/feedback slice, not universal model equivalence or full TZ/source/
operational acceptance. Other S1-S10 obligations remain with parent integration.

Reproduce from repository root, offline (Python 3.9 + pandas 2.2.3 used):
  python3 scripts/signal_shadow_state_reference.py --check fixtures/signal-shadow-state-parity/python-steps.json
  cargo test --offline -p finam-signal-shadow --test state_parity --test model_feedback --test observer
  cargo test --offline -p strategy-runtime-core signal_shadow::observation_tests

No external absolute path is needed. The package must include this directory,
the script/test, the production read-only seam, and existing shared input files:
  fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv
  fixtures/stage8b-p1f-parity/baseline07_python_reference_trades.csv
Their SHA256 values are pinned in the generator and reference JSON; Rust also
checks the canonical CSV hash before consuming it. No input candles are fabricated.
Optional --freeze ORIGINAL_FREEZE_ROOT verifies the vendored Rust extraction
against the original archive and regenerates from the original Python sources.

Independent authorities and extraction
------------------------------------
Frozen ALOR reference head: a19124b98d601d7c583b3554d4beadad7ad82cdc.
Archive source/alor_reference_HEAD.tar.gz SHA256:
  9aa9bde789824b52aebdbde078fe197ab514b0a4e9dfa0b79dcbac8af661c681
Archive member prefix:
  alor-rs-main/strategy-runtime/src/strategies/hybrid_intraday/
Vendored types.rs, mean_reversion.rs, intraday_breakout.rs, orchestrator.rs are
that archive's exact production prefixes before #[cfg(test)], with trailing
whitespace normalized to one newline. Repository-dependent original test
modules only were omitted. No production formula, branch, or callback changed.
The script verifies prefix identity against the archive when --freeze is used.
The Rust test checks the hashes of the exact bytes it compiles. These modules
do not import the current FINAM strategy core. Do not rustfmt the oracle files.

Both Python research source files are included byte-for-byte under oracle/research.
Their whole-file pins match the freeze SHA256SUMS. Only pure selected definitions
are compiled by the generator; original imports, fetch, main, plotting and
network functions are never executed. Do not run the vendored scripts directly.
The original replay/prepare/tick/Profile and build_daily_context/bo_signal are
used. AST instrumentation adds before/finally row observers and hooks the
original BO None returns and replay position/closed/EOD continue branch, without
changing trading conditions or returned values. Diagnostic predicates are
compiled from the pinned BO AST. Instrumented and uninstrumented replay trade
frames are checked equal. Selected rounds must also equal the pinned frozen CSV.
No current Rust model outputs are consumed in generating Python expectations.

Seed, profile and feedback
--------------------------
Four seed sessions: 2026-09-15/16/17/18, 404 M10 bars, 07:00–23:40 MSK.
Five replay sessions: 2026-09-21 through 25, 505 M10 bars, 101/session.
This is a historical inventory, not exchange-calendar or provider-finality proof.
Fresh flat seed; MR disabled before owner acquisition; riskgate disabled.
BO k=.53, stop1=.51, stop2=.35, big-move=.025, absolute min-range=1.01,
wait=3h, weekdays only, model start=07:00 MSK, same-day EOD.
Both Rust contexts receive the same 404 warm bars and ordered 505 replay bars.
Python builds shifted daily anchors from the same nine-session selection, with
no use of current-session final aggregates in entry/exit decisions. Warmup
trade effects are excluded from the seed. Seed sessions are checked flat by
the reference replay; the two Rust books carry state across replay boundaries.

Historical feedback assumption: 1-lot full fill at close rounded to 0.5 tick,
synthetic quote event=result+1ms, receive=result+2ms, apply=result+3ms, before
the next bar. Each context derives its own intent/position; no actual model
output is used to drive the oracle's book. Zero fee/slippage in the virtual
book; Python research costs are not compared. This does NOT qualify observed
quote matching, broker fills, execution/PnL, or candidate09 close-fill behavior.

Checks and results
------------------
505 ordered steps; 5 entries (2 long, 3 short), 5 exits (2 EOD, 3 short stop1),
495 no-intent outcomes; zero differences in the checked historical fields.
Python oracle: before/after anchors and day aggregates, prev-return, BO levels,
side-used, owner/side/position after feedback, action and entry/exit reasons.
The Rust day_before_close register stores the last completed close; it is
mapped explicitly to Python prev_close, not Python's shift(2) column.
Archive oracle: engine anchors/side-used and actual orchestrator Flat/Pending/
Open, pre-fill owner/side, has_pending_entry; all compared before each bar,
after each decision and after feedback against the actual model's read-only
snapshot. Intent type/reason and independently evolved position also match.
Runtime pending entry/exit IDs, retained reasons, book position before fill,
cleared pending after fill, and duplicate-bar no-effect are checked as protocol
assertions. The archive orchestrator has no runtime pending-exit ID/book schema;
those assertions are not misrepresented as an independent Python oracle.
Negative verifier tests reject mutations of anchors, levels, owner, position,
side-used and reason. Compiled oracle source integrity has a separate test.
Twelve directed Python-derived entry probes cover wait/min-range precedence,
minimum-range boundary, strict threshold equality, big-move/side-used vetoes,
and long/short entry. Traced/untraced actual entry outputs equal the unchanged
archive for these probes; diagnostic evaluation leaves the snapshot unchanged.
Trace lifecycle tests cover warmup/reset/restore invalidation, pending/order
gates, and the known archive EOD mismatch. Observer tests reject stale-label,
contradictory-pending, unknown and mismatched reason evidence.

Actual no-intent branch proof — former blocker closed for this slice
-----------------------------------------------------------------
Authorized diagnostics-only changes add evaluate_entry_traced to the actual
BO evaluator. The existing evaluate_entry delegates to that same implementation
and returns its unchanged signal. Original gate order and conditions remain:
time/weekend/session, missing anchors, min-range, long then short eligibility.
The terminal breakout_veto records big-move eligibility, unused-side flags and
strict threshold predicate values at that branch. These are explicit predicate
facts, not claims that every operand of a short-circuit condition was evaluated.

The actual orchestrator calls the traced evaluator and retains its empty-action
branch or the actual position/order/pending/EOD branch. Trace is ephemeral,
outside HybridSnapshot, cleared before each on_bar and on warm/reset/restore.
The read-only seam exposes only a trace matching the model's processed label.
No independent/shadow model is evaluated to explain a production decision.

Counts independently matched against pinned Python return/continue hooks:
before_wait=90, position_hold=120, eod_entry_cutoff=8, breakout_veto=277.
All six veto predicate facts are also compared on every breakout_veto row.
Decision::from_model_observation(None) now uses available actual trace reasons.
Step validation binds the trace label and reason to the recorded decision.
For genuinely untraced paths the generic model_returned_no_intent remains an
honest outcome, not a suppression diagnosis; no historical row uses fallback.
Entry/exit reasons still come from actual retained pending reasons and take
precedence over lower-layer no-intent diagnostics. Wrapper-only suppression
outside this historical BO-only slice is not newly diagnosed by this change.

Other retained limitations
--------------------------
Python exits test stop before EOD; Rust tests EOD before stop. This history
does not exercise a collision and does not qualify universal reason equality.
The frozen Rust orchestrator lacks the same-day EOD entry cutoff already present
in the accepted baseline (881c, per parent audit). It is NOT a new diagnostics
regression. All 505 historical outcomes match, but a directed unused-side EOD
case explicitly asserts the archive entry versus existing current-model cutoff.
The archive is not patched to hide this difference. No broad algorithm-identity
claim is made. Stop2, long stop1, delayed/partial feedback and unresolved overnight
were not observed here. They are not implied PASS by five sessions. Minimum-range
rejection is covered by the separate directed probe, not the historical run.
Historical freeze is not actual current ALOR live, and contains no quote stream.

Scope
-----
Owned changes: scripts/signal_shadow_state_reference.py,
crates/finam-signal-shadow/tests/state_parity.rs, this fixture directory,
crates/strategy-runtime-core/src/hybrid_intraday_runtime/signal_shadow.rs,
crates/strategy-runtime-core/src/hybrid_intraday/intraday_breakout.rs,
crates/strategy-runtime-core/src/hybrid_intraday/orchestrator.rs, and
crates/finam-signal-shadow/src/observer.rs.
The two core modules and observer are explicitly authorized diagnostic changes;
trading conditions/order/outputs and persistent state formats are unchanged.
No seed_binding.rs, network, credentials, git, broker, retained runtime state,
deployment or service operations were used/changed.

Executed verification on 2026-10-23
----------------------------------
Default isolated-package generator --check: PASS, 505 steps / 5 rounds.
Original-freeze --freeze plus --check: PASS, including exact Rust extraction.
state_parity integration suite: 6 passed, 0 failed.
model_feedback integration regression suite: 6 passed, 0 failed.
observer integration regression suite: 12 passed, 0 failed.
signal_shadow::observation_tests: 2 passed, 0 failed; an unrelated existing
stage6_journal_backend test failpoint dead-code warning was emitted.
An intermediate Clippy run found large_enum_variant in the concurrently
integrated runner Start enum. Integration boxed the larger variant; parent then
reran cargo clippy --offline -p finam-signal-shadow --all-targets -- -D warnings:
PASS. The sealed package contains a separate isolated-snapshot Clippy log.
Final rustfmt --check on both core modules, test, seam,
observer and all four vendored modules: PASS. No whole-workspace or operational
qualification is claimed.
