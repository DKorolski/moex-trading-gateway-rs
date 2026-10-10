# O3 fresh paper admission + observation catch-up — source correction

2026-10-10. Baseline: `2be020c234a01f8627d9f7621623dc603c912e23`.
Assignment: `REVIEW_O3_FRESH_PAPER_READMISSION_CATCHUP_20261009_RU.txt`,
sections 5–6. Source review candidate; operational O3 stays NOT_RUN.
The earlier [diagnostic](stage8b-p1f-o3-session-catchup-diagnostic.md)
is preserved as history, not the current source implementation description.

## Runtime boundary

The ordinary V5 public restart still authenticates O2 marker/receipt/seal,
checkpoint and journal. Initial permission requires P1SemanticReady with no
previous commit, flat BO-only/no-riskgate state, no pending/deferred owner or
trading cycle. It does not repeat first boot or change old receipt timestamps.

`stage8b_o3_readmission` constructs a NEW explicitly synthetic observation
from the exact O3 fixture through production Stage4 validation, opaque accepted
evidence, and `admit_stage5c_paper_host(allow_live_orders=false)`. This is not a
FINAM account observation. The fixed composition binds it to the verified phase,
config, plan, fixture, profile and operational identity. The consuming core join
also requires current seal generation/commitment, checkpoint/frontier, account,
instrument/tick, model cursor and exact observed source/canonical M10 tuple.

Freshness is 60 seconds from the required Stage4 sources, capped by the phase
deadline. New work checks real UTC, inclusive at expiry. Historical model opening
label, canonical close and recorded decision time remain distinct. Old O2
checked/issued/expires are never rewritten.

## Retention and crash frontiers

| Frontier | Authority and effects |
| --- | --- |
| Before RequestAccepted | Exact proof is created once, 0600/no-follow, synced and reread beside protected source receipts. Restart may reuse only the same still-fresh attempt. Corruption/expiry does not overwrite it. |
| RequestAccepted before S1 | Protected source-evidence hash includes proof digest. Reconstruct the exact recorded Stage4/5C decision even after its freshness expires, but only for the matching journal record and within the same current phase. No newly sampled observation. |
| S1 / S_ack / S_truth | Full proof stays in authenticated semantic/feedback projections. Ordinary ACK/truth/source-XACK recovery applies; no fresh callback for committed work. |
| Next new M10 | New proof joins current seal/frontier in the SAME phase. Existing virtual position is retained. Cross-phase or unadmitted non-flat bootstrap is rejected. |

The optional proof fields use default/skip-None serialization: historical
packages and old source-evidence hashes remain byte-compatible. Feedback hashing
includes the new field when present, preserving the phase binding across
replacement S_ack/S_truth packages. There is no new root, key, registry or worker.

Observation uses the existing first-boot observation-only kernel, not
generate-and-discard. Complete covered prefix, overlap, tick values and canonical
candidate are checked before observation. Four days is a resource bound only.

The exact O3 plan now REQUIRES `catchup_coverage`, using the existing explicit
session-window representation. Its legacy `first_close_time_utc` /
`last_close_time_utc` fields denote **opening model labels**, as in History.
Coverage is required for regular model-session labels. With the frozen
`weekends_off=true`, weekend labels are suppressed by the existing model BEFORE
coverage lookup and need no explicit empty weekend sessions (for example,
Friday 2026-10-09 to Monday 2026-10-12). This is model suppression, not a calendar
inference from absent data, and does not change the frozen model. An explicitly
listed empty regular session is closed; an omitted regular-session day is
unknown and fails closed. Sparse M1 is aggregated normally, but an empty
required M10 fails. No holiday or clearing break in a regular model session is
inferred from missing bars. Existing plans
without this field are rejected and must not be reused as new run authority.
Only after source/release review may an operator authorize a new exact plan.

## Evidence and honest limits

Local tests use disposable Redis and V5 roots with test keys. They authenticate
public restart rather than substitute scalar fields for a root. They exercise
next-session observation, ordinary Market/provider/ACK/truth/XACK, pre-append
proof recovery, RequestAccepted-before-S1 replay after **61 real seconds**,
committed restart and AlreadyAcknowledged, zero-intent successor and separate
Market successor after **601 real seconds**. Real clocks are not compressed.

New counters separate observation callbacks, fresh decisions and replay
computations. Existing direct counters cover provider/publication/XACK; request
evidence and unchanged pre-finalization journal establish exact logical replay.
Marker/first-boot receipt bytes remain unchanged throughout the linked chains.
Tests also reject wrong seed/profile/checkpoint/frontier/source, damaged or
missing replay proof, expiry/future/foreign scope, unknown/empty coverage,
bad tick/overlap/truncation and non-flat initial state.

This is a local Rust/Redis witness, NOT a new production ELF/systemd or
operational VPS witness. No generation-2 key was used. Linux credential-backed
SIGTERM/SIGINT barrier cases requiring that credential are not newly executed
by this source slice. Existing local latch/supervision regressions are run;
native release qualification remains a separate next gate. Do not label ignored
process tests PASS. The ZIP records actual commands, counts, logs and omissions.

An initial all-feature durable run without the project's 32 MiB stack setting
aborted. A parallel run with that setting encountered pre-existing process-wide
guardian fault-injection collisions. Logs are retained; neither is a PASS.
The complete durable suite is rerun with `RUST_MIN_STACK=33554432` and
`--test-threads=1`, without changing unrelated guardian code.

Gateway `--all-features` also enables the existing `m3j16-actual-one-shot`
feature, conflicting with its unconditional legacy endpoint-marker refusal
test (522 passed, 1 failed, 4 ignored). Neither that constructor nor its test
was changed. The applicable complete no-default-features gateway suite passes
514 tests (4 ignored). Strict all-features/all-targets Clippy remains required;
the all-features test failure is retained, not relabelled green.

## Next boundary

### Source retry correction after review 7cf760c

Review `REVIEW_7cf760c_O3_SOURCE_RESULT_RU.txt` retained SOURCE HOLD for
P1-O3-R01: an existing complete proof could be visible after write_all but before
file/parent sync. Both create and existing paths now finish the same boundary:
validate canonical/fresh/exact binding → open no-follow regular file and validate
its exact bytes/custody → sync that descriptor → sync parent → exact reread →
return permit. No proof is rewritten or reissued on retry; expired proof remains
rejected. Errors return before callback/RequestAccepted.

The targeted real-Redis/V5 witness interrupts immediately after full write,
then uses public restart to retry the exact attempt. Thread-local test-only
faults separately reject file sync, parent sync and final reread. Positive trace
requires both successful syncs and exact reread before semantic commit; failures
retain identical journal/seal/history/proof, zero callbacks/effects and source
PEL. This is an instrumented durability barrier, not a power-failure experiment.
The P2 weekend wording above is documentation-only. Admission, model, timing,
journal replay and provider algorithms are unchanged. Existing long-wait results
remain evidence of 7cf760c, not newly executed tests of the correction.

Source review → one explicit authority/release artifact → separately permitted
stopped installation preserving root/history → separately authorized bounded O3.
Authority drift is expected for this source candidate and is not rebaselined.
No installation, phase claim/signing, SSH, FINAM, DB0/P0/systemd, GitHub CI or
push/merge is part of this work. O2 remains COMPLETED/1/26.

After accepted O3: O4 bar/other WS subscriptions, reconnect and REST overlap,
freshness, EOD timeliness and several-session ALOR paper parity. Only then the
separately accepted FINAM live-micro gate; no new general recovery platform.
