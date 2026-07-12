# Stage 5C-j - paper broker lifecycle facade

Status: review candidate. Date: 2026-07-12.

This facade consumes only a `Stage5cResolvedPaperIntentBatchStrategy` produced
by Stage 5C-i ACK escrow resolution. It does not accept raw semantic-bar output
or unresolved intent batches.

It applies paper broker-state evidence for the already ACKed batch:

- `Market` intents require a `Position` event;
- `Place`, `Cancel` and `Replace` intents require an `Order` event;
- `CreateStopLimit` and `DeleteStopLimit` intents require a `StopOrder` event.
- Market entry validates the resulting position direction against the intent
  side;
- Market exit validates a nonzero previous position transitioning to flat.

Gates:

- full escrow batch and typed ACK outcomes are preserved from Stage 5C-i;
- terminal ACK statuses (`Rejected`, `Expired`, `Error`) expect no broker-state
  event;
- active ACK statuses (`Accepted`, `Confirmed`, `Duplicate`) require exactly one
  matching broker-state event;
- events are sorted by `total_sequence`, independent of input Vec order;
- exact duplicate events are deduplicated;
- conflicting duplicate events are blocked;
- multiple lifecycle events for the same request are allowed when they are
  distinct transitions, for example `working -> filled`;
- `StrategyRequestId` must belong to the resolved batch;
- `Order` events must carry the exact request ID;
- `BrokerOrderId` / `StopOrderId` mappings are checked where broker evidence
  provides the ID;
- `Order` and `StopOrder` events must carry strategy-owned HYB attribution with
  the expected action role;
- action-specific side, quantity, price, stop price and expiry fields must match
  the original escrowed intent;
- event instrument must match the admitted target instrument;
- event timestamp must not predate the ACK timestamp;
- deterministic event validation is completed before the first source callback
  mutates the strategy;
- the resolved type-state preserves the full intent batch and exposes remaining
  lifecycle expectations for non-terminal `working` / partial transitions;
- callback-produced intents remain terminal evidence.

Still closed:

- next semantic bar until this facade succeeds;
- timer path;
- intent sink;
- Redis command stream;
- broker transport;
- FINAM command consumer;
- POST/DELETE order endpoints;
- runtime-live attachment.
