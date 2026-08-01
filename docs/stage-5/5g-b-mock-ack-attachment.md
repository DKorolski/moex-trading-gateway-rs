# Stage 5G-b — deterministic mock ACK attachment

Status: implementation review candidate.

## Accepted predecessor

- Stage 5F-e accepted source: `fb8245e2f91cfc1678548a1228e8558d9adc2181`.
- Stage 5F closure: `cac83da38725aeadd6d029a3078157c2ab7fa004`.
- Stage 5G-a accepted design: `011fd4b7baaa41fffdad7d3c28e463b7977f5989`.
- Development branch: `stage5g-lifecycle`; `main` remains untouched.

The independent Stage 5G-a review authorized only Stage 5G-b mock ACK
attachment. It did not authorize Stage 5G-c order/trade/position convergence.

## Authority flow

```text
accepted Stage 5C settled intent capability
        +
typed paper intent bindings
        |
        v
opaque Stage5gMockAckSession
        |
        +--> exact account/instrument/request/client/action/side/time checks
        |
        +--> Broker Core RuntimePendingRequestIdentity::evaluate_ack
        |
        +--> ambiguous outcome retained without callback
        |
        `--> complete callback-safe ACK vector
                    |
                    v
          existing Stage 5C-i resolver
                    |
                    v
       opaque Stage5gResolvedMockAckPaperStrategy
```

Stage 5G-b does not own a second ACK status vocabulary. The external input is
the broker-neutral `CommandAck`; pending disposition is delegated to Broker
Core; runtime mutation is delegated to the frozen Stage 5C-i facade.

## Linear ownership

`Stage5gMockAckSession` owns the exact `Stage5cSettledPaperStrategy`. It is not
cloneable, serializable, debuggable or constructible outside its reviewed
admission function. Recoverable pre-callback validation errors return the same
session. Once Stage 5C invokes a callback, a later failure is terminal.

An ACK cannot be attached to a summary or a bare request ID. Compile-fail
witnesses in `strategy-runtime-core/src/lib.rs` pin both the settled-capability
requirement and session-construction privacy.

## Correlation contract

Every paper binding is checked against the settled batch in exact order:

- exact `StrategyRequestId`;
- exact existing Stage 5C intent class;
- deterministic market/limit/cancel request identity;
- deterministic FINAM-safe `ClientOrderId` derived from the request ID;
- exact account and complete `InstrumentId`;
- exact place/cancel action and side shape;
- event timestamp not before the source intent and not after lifecycle expiry;
- exact broker-native order ID continuity where one has been observed;
- no generated or synthetic `BrokerOrderId`.

For market intents, the side is included in the deterministic request-ID
validation. For limit intents, the accepted typed binding carries the side and
every ACK event must match it; actual order-side truth remains closed until
Stage 5G-c broker events.

## ACK policy

| Broker-neutral outcome | Stage 5G-b result |
|---|---|
| accepted/submitted/recovered with broker ID | callback-safe canonical ACK |
| accepted/submitted/recovered without broker ID | retain `AwaitingBrokerOrderId` |
| rejected with exact request ID | callback-safe terminal ACK |
| timeout / unknown-pending | retain `ReconciliationPending` |
| duplicate without prior outcome | retain `PriorOutcomeRequired` |
| duplicate matching a canonical prior outcome | deterministic no-op |
| expired without `ExpiredCommand` proof | retain `NoSendProofRequired` |
| expired with exact `ExpiredCommand` proof | callback-safe terminal ACK |
| error | retain `ManualInterventionRequired` |

Only a complete callback-safe vector enters Stage 5C-i. Timeout,
unknown-pending, missing broker ID, unsupported duplicate, unproved expiry and
error never mutate runtime state.

## Acceptance matrix

The ten frozen Stage 5G-b cases are:

1. `GACK01_PLACE_ACCEPTED_EXACT_IDS`
2. `GACK02_SUBMITTED_MISSING_BROKER_ID_KEEPS_PENDING`
3. `GACK03_RECOVERED_EXACT_BROKER_ID`
4. `GACK04_REJECTED_EXACT_REQUEST_CLEARS_PENDING`
5. `GACK05_TIMEOUT_KEEPS_PENDING`
6. `GACK06_UNKNOWN_PENDING_KEEPS_PENDING`
7. `GACK07_DUPLICATE_REQUIRES_PRIOR_OUTCOME`
8. `GACK08_EXPIRED_REQUIRES_EXACT_NO_SEND_PROOF`
9. `GACK09_REQUEST_OR_CLIENT_ID_MISMATCH_BLOCKS`
10. `GACK10_BROKER_ORDER_ID_CONFLICT_BLOCKS`

Additional review-required negative witnesses cover:

- ACK before settled-intent ownership (compile-fail);
- duplicate ACK;
- terminal ACK twice;
- wrong account or instrument;
- wrong side or action;
- ACK after lifecycle expiry;
- ACK request/client identity mismatch;
- ACK cannot change broker truth without an order event.

## Closed surfaces

The implementation contains no:

- Redis stream, consumer or consumer group;
- FINAM DTO, adapter or transport;
- HTTP `POST`/`DELETE`;
- broker dispatch, send or execution;
- order/trade/position event application;
- runtime-live, `LiveReady` authorization or unattended loop;
- real order or protective-order placement.

The exposed capability reports all of these attachments as `false`.

## Review boundary

The next handoff is Stage 5G-b implementation review. Stage 5G-c remains blocked
until this implementation is accepted. No merge to `main`, deployment
or Stage 6 transition is implied.
