# Signal-shadow: remaining source acceptance work

Status: **IN DEVELOPMENT — not ready for final source acceptance**.
R1 update: the following historical checklist is superseded for local source
progress by `stage8b-signal-shadow-source-review-r1.md` and the R1 immutable
evidence logs. R1/R2 corrections, independent historical state/reason checks,
bounded prefix reconstruction and EOD-tail checks are now submitted for review.
Operational S5/S9 and VPS acceptance remain unperformed, not waived.
Contract: reviews/REVIEW_SIGNAL_SHADOW_SCOPE_TZ_20261023_RU.txt.
This checklist consolidates the remaining work; it does not extend the roadmap.
Accepted historical O2/O3 and operational namespaces remain untouched.

## Evidence boundary

The executable now exposes offline replay and an explicit observed runner.
REST, BARS/QUOTES workers, admission, timing, model and observer are connected.
Local composition tests do not prove an operational observation process.
September 27 ALOR freeze is a historical
reference, not current live evidence. The five-session synthetic-close feedback
experiment is explicitly conditional, not observed execution parity.

The measured model entry now seals pending intent time to callback completion.
Quotes at or before that completion cannot fill the intent. Timing failure
poisons the model even if the callback has already mutated it; no rollback or
retry is claimed. This entry alone is not feed admission or config binding.

## Ordered closure checklist

1. **Single executable composition (S1/S4/S6/S10): implemented, review pending.**
   Validate the integrated entry and isolated-snapshot regression evidence.
   No broker order client or operational Redis is attached. Actual read-only
   subscription, reconnect, timing and stop remain separate operational evidence.
2. **State/comparison and segment boundaries (S2/S3/S7/S8).**
   Expose BO levels and meaningful suppression reasons; compare reference
   state evolution with identical input/seed/feedback, including compatibility
   checks for report identities. Prove NEW_SEGMENT behavior with pending/nonflat
   virtual state, missing late bars, EOD and next start without fake flat or
   promotion of historical replay to timely observation.
3. **Source handoff.**
   Select and document the exact shared dependencies from the dirty O4 tree;
   do not silently accept all O4 changes. Run focused shared regressions,
   isolation checks and package validation. Deliver exact immutable source
   identity, intentional diff, retained test evidence, ZIP, SHA-256 and safety.
   No final source acceptance package exists merely because this checklist exists.

## Matrix accounting

| IDs | Current evidence | Remaining acceptance evidence |
| --- | --- | --- |
| S1 | Offline/observed entries, profile and four-session seed tests | Source review of observed integration |
| S2 | Five historical sessions, conditional signal/round comparison | Full compatible state/reason comparison |
| S3 | Model callbacks, measured composition and virtual feedback tests | Independent state comparison and actual quote evidence |
| S4 | Sparse admission/dedup/conflict component tests | Process integration and later operational observation |
| S5 | Local protocol/framing tests | Actual subscription and controlled reconnect |
| S6 | Timing fence and post-result causal boundary tests | Integrated measured path, later actual latency |
| S7 | Explicit new-segment seed and exclusive new output | Pending/nonflat restart proof and later actual restart |
| S8 | Unresolved position remains nonflat | Integrated EOD/missing-bar/next-start proof |
| S9 | None; historical replay is not observation | Two separately authorized configured sessions |
| S10 | Offline entry, bounded transports and no order dispatch wiring | Integrated stop/isolation and later operational verification |

Source review precedes separately authorized installation and observation.
S5/S9 operational evidence is not a prerequisite to writing source, but cannot
be marked PASS using local tests. Live-micro authorization remains separate.
