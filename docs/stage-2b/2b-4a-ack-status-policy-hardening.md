# Stage 2B-4a — explicit ACK status policy hardening

Status: implementation patch ready for review.

Date: 2026-07-07.

## What changed

Stage 2B-4a removes optimistic default-clear behavior from the passive ACK
lifecycle boundary.

Added:

- `RuntimeAckStatusPolicy`;
- explicit status-to-pending policy inside `RuntimePendingRequestIdentity`;
- lifecycle issues for ambiguous statuses:
  - `AmbiguousErrorAckDoesNotClearPending`;
  - `DuplicateAckRequiresPriorOutcome`;
  - `ExpiredAckRequiresNoSendProof`.

Policy matrix:

- `Accepted` / `Submitted` / `Recovered` with broker id: clear command-pending
  at DTO boundary, without marking order lifecycle terminal;
- `Accepted` / `Submitted` / `Recovered` without broker id:
  `KeepPendingBrokerOrderId`;
- `Rejected`: `ClearPending` when `StrategyRequestId` matches;
- `Timeout` / `UnknownPending`: `KeepPending`;
- `Duplicate`: `RequiresPriorOutcome` and keeps pending;
- `Expired`: clears only with `CommandAckReasonCode::ExpiredCommand`; otherwise
  `RequiresNoSendProof` and keeps pending;
- `Error`: `ManualInterventionRequired` and keeps pending.

## What did not change

- No `HybridIntradayRuntime` behavior changed.
- No BO/MR strategy decision logic changed.
- No trade ledger implementation changed.
- No command builders changed.
- No real FINAM command consumer was connected.
- No real FINAM `POST`/`DELETE` path was enabled.
- No runtime-live or FINAM `LiveReady` was enabled.
- No Stop/SLTP/bracket/replace/multi-leg live behavior was enabled.
- No RI/RTS or USDRUBF migration was started.
- No `i64` surrogate adapter was introduced.

## Tests added

- `error_ack_with_matching_request_id_does_not_clear_pending_by_default`;
- `duplicate_ack_with_matching_request_id_requires_prior_outcome_before_clearing`;
- `expired_ack_requires_explicit_no_send_policy_before_clearing`;
- `timeout_and_unknown_pending_ack_keep_pending`.

Preserved from Stage 2B-4:

- `rejected_ack_may_omit_broker_order_id_when_request_matches`;
- `submitted_ack_missing_broker_order_id_is_marked_pending_broker_id`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
