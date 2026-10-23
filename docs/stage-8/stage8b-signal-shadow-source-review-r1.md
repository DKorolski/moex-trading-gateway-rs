# Signal-shadow R1 — source corrections and local contract evidence

Review predecessor: immutable source candidate
`3b200dd84e763a2eff904a242b1c445480f7e6c1`.
Engineering contract: `reviews/REVIEW_SIGNAL_SHADOW_SCOPE_TZ_20261023_RU.txt`.
Review request: corrections R1/R2 plus local S2/S7/S8 evidence, not operational
acceptance. The exact new commit/tree and completed test counts are in the
top-level handoff and evidence files. Older R0 documents describe prior scope.

## Reviewer corrections

- R1: QUOTES deduplication now uses its own 4096-entry sliding window and a
  monotonic frontier, not a lifetime limit derived from the M10 cache. Retained
  duplicates have no effect, conflicts and evicted-old events stop the segment.
  Multi-row admission remains atomic, including identities evicted within a
  frame. Tests exceed 4096 quotes and cover reconnect. The policy version is
  bound in both configuration fingerprints.
- R2: the complete provenance hash still binds every configuration field.
  A separate semantic hash excludes only run_id, segment_id and
  previous_segment_id. Comparison tests create real validated configurations
  and seeded headers instead of editing attributed headers by hand.
- Socket waits now use the remaining configured data-silence allowance rather
  than the shorter REST/connect timeout. Heartbeats do not replenish data
  silence. Absolute deadline, cancellation and shared stop still apply.
- EOD validation includes the 23:40 M10 label, nominally closed at 23:50.
  Earlier cutoff or a missing final bucket is rejected; intent alone never
  proves a flat position.

## Evidence interpretation

Local S2 compares 505 canonical steps over five historical sessions, with the
same four-session seed and explicitly conditional feedback. The frozen Rust
orchestrator is compiled separately from FINAM; the Python expectations are
regenerated from pinned source, not captured FINAM output. Reasons for empty
decisions are retained at the actual evaluator branches, with a current-label
binding; no post-hoc state heuristic is used. This narrow diagnostic change
does not change the accepted evaluator's trading predicates or priorities.
The existing same-day EOD entry guard differs from the old ALOR archive and is
documented rather than erased from the reference.

Local S7 adds bounded, pinned prefix reconstruction with re-admitted raw M1
evidence and ordered QUOTES feedback. The new segment retains its predecessor
identity; reconstructed rows remain BACKFILL. CLI `restart` is file-only;
`observe-prefix` is a separate read-only entry and was not run against FINAM.
Live promotion requires a separately trusted plan pinning the prior-output
inventory and prefix receipt. Rehashing a locally edited payload is not proof
that no quotes were omitted. Unproven coverage, stale handoff and unresolved
pending refuse promotion; nonflat state is never reset to flat.
The positive production-clock handoff test is session-window dependent. Its
separate nocapture log must contain an executed-success marker before sealing;
a test's early return outside the window does not qualify that boundary.

Local S8 covers actual-model EOD exit, explicit feedback, final 23:40 label,
missing-tail refusal and following-session/weekend rollover. One next-Monday
endpoint is explicitly synthetic because the historical freeze ends Friday.
These tests do not substitute for two real observed sessions.

The September 27 freeze is the user-selected historical reference, not current
ALOR live. Independent archived ALOR code and Python reference are test-only.
Conditional historical feedback and synthetic boundary cases are explicitly
labelled; they do not establish observed quote execution or latency.

The package includes source, pinned oracle inputs, intentional diff, test logs,
byte-level manifest and archive safety. Tests must be run against this isolated
source snapshot, not merely the dirty development worktree.

No claim of S5/S9 operational completion is made: actual read-only FINAM
subscription/reconnect, measured timing, restart and two observed sessions
require separately authorized installation/observation. No token, signer,
VPS connection, Redis mutation or O3 position change is part of this work.
No FINAM order route, Stage6/7 retained paper owner or deferred O4 implementation
is added to the signal-shadow composition.

## Packaging boundary

The complete tracked accepted baseline is retained with a narrow shadow
allowlist. The local object commit identifies that exact source tree without
moving main, remote refs or the development index. Inherited historical
reports are not evidence of acceptance for this candidate.
Source acceptance, operational acceptance and live-micro authorization remain
separate decisions.
