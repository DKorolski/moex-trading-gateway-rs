# Stage 5C-j - paper broker lifecycle facade

Status: review candidate. Date: 2026-07-12.

This facade consumes only a `Stage5cResolvedPaperIntentBatchStrategy` produced
by Stage 5C-i ACK escrow resolution. It does not accept raw semantic-bar output
or unresolved intent batches.

It applies paper broker-state evidence for the already ACKed batch:

- `Market` intents require a `Position` event;
- `Place` entry/exit/protective intents require `Order` evidence and, after a
  terminal fill, `Position` confirmation;
- `Cancel` and `Replace` intents require an `Order` event;
- `CreateStopLimit` intents require `StopOrder` evidence and, after trigger or
  execution, `Position` confirmation;
- `DeleteStopLimit` intents require a `StopOrder` event.
- Market entry validates the resulting position direction against the intent
  side and target quantity;
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
- position confirmation can complete a `Place` / stop-execution lifecycle only
  when the corresponding filled/triggered event precedes it by `total_sequence`;
- non-execution terminal `Place` order statuses (`canceled` / `expired` /
  `rejected`) complete the lifecycle without position evidence, while `filled`
  still requires position confirmation;
- stop statuses are split into working, execution, and non-execution terminal:
  `triggered` / `executed` / `filled` require flat position confirmation, while
  `canceled` / `expired` / `rejected` terminate without position evidence;
- `StrategyRequestId` must belong to the resolved batch;
- `Order` events must carry the exact request ID;
- `BrokerOrderId` / `StopOrderId` mappings are checked where broker evidence
  provides the ID;
- `Order` and `StopOrder` events must carry source-compatible strategy-owned
  HYB attribution with the expected action role and cycle;
- marketable-limit `Place` entry/exit accepts `ENTRY`/`EXIT`, protective
  `Place` accepts `TP`, and cleanup cancel/delete accepts original object
  attribution, including exact cycle/owner/role, instead of an artificial
  `CANCEL` role;
- action-specific side, quantity, price, stop price and expiry fields must match
  the original escrowed intent;
- market and marketable-limit entries block wrong-side broker positions and
  overfill before source callbacks; partial fills preserve a remaining
  `Position` lifecycle expectation until target quantity is reached;
- sequential position preflight tracks accepted position watermark and blocks
  partial entry regression before source callbacks;
- callback-generated cleanup intents are preserved as a new no-send generated
  intent batch and are re-settled through the Stage 5C-g escrow policy;
- cleanup attribution for callback-generated intents is captured from the
  pre-callback TP/SL ledger before the wrapper removes broker object IDs.
- unknown order/stop statuses are blocked before callback;
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
