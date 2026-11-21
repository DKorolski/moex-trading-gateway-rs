# Signal-shadow R0 — source review candidate

Historical R0 description. For the corrected candidate use
`stage8b-signal-shadow-source-review-r1.md`; this file is retained for lineage,
not as the current completion matrix.

## Review request and honest boundary

Review the isolated source implementation against
`reviews/REVIEW_SIGNAL_SHADOW_SCOPE_TZ_20261023_RU.txt`.
This is **not a declaration that all S1–S10 requirements are complete**.
No operational installation, provider connection, credential use, Redis write,
O3 position change or live order was performed while preparing this source.
Source acceptance, installation and observation remain separate decisions.

The previous offline-only status has changed: the `observe` command now binds
the selected seed/config to existing GET-only REST and dedicated BARS/QUOTES
workers, admitted sparse M10, actual model callbacks, hypothetical feedback and
append-only JSONL. Workers keep in-flight socket reads alive across coordinator
selections. Stop/error cancels and joins workers under an explicit bound.
Provisional BARS, heartbeat and handshake do not advance the model.

## Implemented and locally testable

- Fixed baseline07 BO-only/no-riskgate profile and explicit four-session seed.
- Existing model, actual position feedback, no retained Stage6/7 paper owner.
- Final REST admission, required nominal sequence, overlap/conflict rejection.
- Callback receive/ready/start/result frontiers; quotes at or before measured
  result cannot fill the new decision. A timing failure is terminal.
- Shared stop/deadline, bounded input/cache/output and exclusive new output.
- Model BO thresholds, pending decision reasons and before/after state.
- Data hashes independent of replay/live delivery mode; report identity
  compatibility checks precede semantic comparison.
- Raw admitted REST evidence retained before selected model advancement.
- Historical five-session conditional comparison using the September 27 freeze.
  Synthetic-close feedback is explicitly not observed QUOTES execution.

## Known source limitations — reviewer must not infer PASS

1. **S2:** full independent ALOR per-step state/suppression-reason equivalence is
   not established by matching entry/exit rounds. No-intent is recorded honestly
   as `model_returned_no_intent`, not a guessed suppression reason.
2. **S7:** observed entry only supports a fresh session-boundary seed. A missed
   current-session prefix is refused. There is no mid-session NEW_SEGMENT replay
   with matching historical quote feedback; refusal is not proof of that scenario.
3. **S8/S9:** the runner is a bounded single-session source path, not two-session
   continuity evidence. Its explicit EOD check does not certify the later 23:40
   label or a subsequent session. Missing due bars/pending/nonflat are INCOMPLETE.
4. **S5/S6/S10 operational:** local protocol/socket/timing tests do not establish
   actual FINAM subscription/reconnect/latency or isolation on the VPS.
5. Read-only credential role is an externally supplied assertion. The runner
   does not inspect permissions at the provider; it exposes no order route.

These are limitations against the original contract, not silently approved
scope reductions. This package can support focused source review, but must not
be used as permission for installation or a claim of completed parity.

## Packaging and dirty-tree boundary

The development tree also contains unaccepted deferred O4 work. The source
package is assembled from its recorded Git baseline plus an explicit allowlist
of signal-shadow source/docs and minimal shared changes; O4 is not imported or
promoted to accepted dependency. Packaging metadata records the exact resulting
tree, baseline, intentional diff and test results from the isolated snapshot.
The working branch and remote refs are not advanced by packaging.

Run the documented offline checks from that snapshot, not merely the development
tree. Existing historical acceptance evidence describes its old commits, not
automatic acceptance of this candidate. Broad governance/freeze checks must not
be represented as PASS unless actually rerun and reconciled for this tree.
