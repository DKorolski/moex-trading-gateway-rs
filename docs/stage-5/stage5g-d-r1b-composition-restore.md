# Stage 5G-d R1-b R3 — canonical post-checkpoint evidence parity

Status: implementation review candidate.

Accepted predecessor: `e7b133daa73026c0b7d1b82be368013ff9328667`.

## Scope

R1-b R3 preserves the accepted R2 chronology/latest-ledger hardening and makes
active Stage 5G-c plus post-checkpoint Stage 5G-d consume one crate-private,
pure canonical evidence authority. It does not change Stage 5C callback
authority or open Stage 5G-e/f, Redis, FINAM, transport, runtime-live or real
orders.

The predecessor exposed a real liveness gap: a zero-intent bar owned only a
settled Stage 5C value, while the accepted Stage 5C timer API requires its
private timer-settlement type. A single crate-private, no-callback re-arm
bridge was therefore added beside the accepted R1-a authority. Enforcement
proves the rest of the Stage 5C file remains byte-identical to `d049453...`.

The only bar transition used by Stage 5G-d is
`advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint`.
The explicit evaluation timestamp is the accepted bar checkpoint. The incoming
Stage 5G-d checkpoint must be no older than the inner Stage 5C settlement.

## Ownership and continuation

- raw settled-state exits from timer and bar wrappers are removed;
- zero-intent bar output becomes a replay-owning linear timer-ready wrapper
  accepted by both existing timer and bar continuation APIs;
- bar-generated intents enter the same Stage 5G-b ACK and Stage 5G-c
  BrokerTruth path as timer output;
- order-position attachment copies the exact continuation checkpoint into the
  replay projection;
- every next checkpoint is the maximum of prior continuation, exact broker
  receipt compatibility watermark and accepted event checkpoint;
- retryable failures retain the exact incoming checkpoint.
- ACK receipt and BrokerTruth receipt must be at or after the inherited
  continuation checkpoint in exact milliseconds, before any lifecycle or
  replay mutation;
- retryable order-position admission retains the complete timer-owned ACK
  wrapper rather than exposing a raw Stage 5G-b capability.

## Restore contract

Checksum validity is necessary but not sufficient. Restore also requires an
exact receipt/discriminator pair, derived millisecond watermark, unique replay
ledger with valid fingerprints and exact current-evidence identity membership,
local sequence, continuation watermark and coherent chronology/duplicate count.

Every replay-ledger identity is parsed at full nanosecond precision. Ledger
receipts must be nondecreasing, the current identity must be the final/latest
ledger identity, and the current exact receipt, discriminator and compatibility
millisecond must all describe that same final package. Recomputed checksums do
not make stale-current, reversed-ledger or regressed-current projections valid.

After restore, an exact known identity is classified first and therefore
remains an idempotent replay even when its historical receipt predates the
continuation checkpoint. A genuinely new identity must have a BrokerTruth
receipt at or after the inherited continuation checkpoint and must also pass
the existing last-BrokerTruth receipt regression guard before it can be
appended to the replay ledger.

## Canonical evidence parity

The single authority owns its input and returns an opaque canonical evidence
value. It sorts orders, positions, instruments and cash; groups trades by exact
`BrokerTradeId`; collapses immutable duplicates while retaining the newest
observation receipt; and rejects conflicting immutable trade payloads before
fingerprint or replay mutation. Both active admission and restart
classification obtain identity/fingerprint only from this value.

An exact raw duplicate-trade redelivery therefore produces the same canonical
fingerprint after restart. A new package result owns its canonical candidate,
so a future Stage 5G-e consumer cannot classify one raw projection and apply a
different one. Exact replay owns no candidate.

Identity grammar remains version 3 and is now pinned: canonical lowercase
hyphenated UUID text, a nonempty colon-free account ID, and the exact
full-precision version-1 package discriminator. Changing this representation
requires a new identity schema version.

## Executable witnesses

- `stage5gd_bar_generated_intent_roundtrips_through_ack_truth_and_next_timer`;
- `stage5gd_timer_generated_cleanup_roundtrips_through_ack_truth_and_next_session`;
- `stage5gd_zero_intent_bar_rearms_timer_and_later_bar_without_callback_loss`;
- `stage5gd_bar_preflight_failure_returns_exact_incoming_checkpoint`;
- `semantically_incomplete_checkpoints_fail_even_with_recomputed_hash`;
- `replay_ledger_and_continuation_semantics_are_fail_closed`;
- `new_post_restore_package_requires_continuation_chronology_but_exact_replay_does_not`;
- `multi_package_restore_requires_ordered_ledger_and_latest_current_projection`.
- `post_checkpoint_duplicate_trade_redelivery_matches_active_canonical_fingerprint`;
- `post_checkpoint_known_payload_change_and_trade_identity_conflict_fail_closed`;
- `new_post_checkpoint_package_owns_one_deduplicated_canonical_candidate`;
- `stage5gd_active_path_stores_single_authority_canonical_fingerprint`;
- `stage5gd_active_path_rejects_conflicting_trade_identity_before_replay_append`;
- `replay_identity_grammar_requires_canonical_uuid_and_colon_free_account`.

The timer-generated witness is source-reachable through a partial Exit with an
authoritative bracket-reconciliation timer. An explicit post-grace timer emits
the residual Exit, which enters protected ACK/order-position ownership and
converges back to a semantically restorable Stage 5G-d checkpoint.

## Closed surfaces

Stage 5G-e/f, Redis live consumer/groups, FINAM transport, HTTP POST/DELETE,
broker dispatch/execution, runtime-live, real orders, Stage 6, main merge and
deployment remain closed.
