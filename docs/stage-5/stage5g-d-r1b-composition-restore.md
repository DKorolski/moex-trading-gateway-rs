# Stage 5G-d R1-b R1 — continuation liveness and causal chronology

Status: implementation review candidate.

Accepted predecessor: `7724b4472d603b3c2ef7c3ff22aa371aa64d8592`.

## Scope

R1-b R1 composes the accepted transactional Stage 5C bar authority with the
Stage 5G-d ownership wrappers. It does not change the accepted Stage 5C source
callback logic or open Stage 5G-e/f, Redis, FINAM, transport, runtime-live or
real orders.

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
ledger with valid fingerprints and exact current-evidence identity membership, local sequence,
continuation watermark and coherent chronology/duplicate count.

## Executable witnesses

- `stage5gd_bar_generated_intent_roundtrips_through_ack_truth_and_next_timer`;
- `stage5gd_timer_generated_cleanup_roundtrips_through_ack_truth_and_next_session`;
- `stage5gd_zero_intent_bar_rearms_timer_and_later_bar_without_callback_loss`;
- `stage5gd_bar_preflight_failure_returns_exact_incoming_checkpoint`;
- `semantically_incomplete_checkpoints_fail_even_with_recomputed_hash`;
- `replay_ledger_and_continuation_semantics_are_fail_closed`.

The timer-generated witness is source-reachable through a partial Exit with an
authoritative bracket-reconciliation timer. An explicit post-grace timer emits
the residual Exit, which enters protected ACK/order-position ownership and
converges back to a semantically restorable Stage 5G-d checkpoint.

## Closed surfaces

Stage 5G-e/f, Redis live consumer/groups, FINAM transport, HTTP POST/DELETE,
broker dispatch/execution, runtime-live, real orders, Stage 6, main merge and
deployment remain closed.
