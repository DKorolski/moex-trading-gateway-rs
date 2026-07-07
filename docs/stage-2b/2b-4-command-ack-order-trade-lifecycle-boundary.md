# Stage 2B-4 — CommandAck / OrderEvent / TradeEvent lifecycle boundary

Status: implementation patch ready for review.

Date: 2026-07-07.

## What changed

Stage 2B-4 adds broker-neutral lifecycle classification at the passive
DTO/runtime-state boundary:

- `RuntimePendingRequestIdentity`;
- `RuntimeAckLifecycleDecision`;
- `RuntimeAckPendingDisposition`;
- `RuntimeAckBrokerOrderIdState`;
- `RuntimeAckLifecycleIssue`;
- `RuntimeOrderEventLifecycle`;
- `RuntimeOrderEventLifecycleClassification`;
- `RuntimeBrokerEventDeduplicator`;
- `RuntimeBrokerEventReplayDisposition`.

The boundary rules are:

- ACK pending clearance is keyed by exact `StrategyRequestId`;
- ACK with mismatched `StrategyRequestId` does not clear pending state;
- matching `ClientOrderId` never clears pending by itself;
- matching `BrokerOrderId` never replaces `StrategyRequestId`;
- `Submitted` / `Accepted` / `Recovered` ACKs without `broker_order_id` are
  represented as `pending_broker_order_id`;
- `Rejected` / local-rejected style ACKs may omit `broker_order_id`;
- `OrderEvent` preserves exact `BrokerOrderId(String)` and classifies
  active/terminal/unknown status;
- `TradeEvent` preserves exact `BrokerOrderId(String)`;
- duplicate order/trade broker events are classified as idempotent at the DTO
  replay boundary.

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

- `matching_strategy_request_id_ack_can_clear_matching_pending_path`;
- `mismatched_strategy_request_id_ack_never_clears_pending_even_with_client_match`;
- `broker_order_id_does_not_replace_strategy_request_id`;
- `submitted_ack_missing_broker_order_id_is_marked_pending_broker_id`;
- `rejected_ack_may_omit_broker_order_id_when_request_matches`;
- `order_and_trade_events_preserve_exact_broker_order_id_and_classify_lifecycle`;
- `broker_event_before_ack_is_representable_without_corrupting_pending_state`;
- `duplicate_broker_events_are_classified_idempotent_at_dto_layer`.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
