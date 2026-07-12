# Stage 5C-k - controlled paper timer facade

Status: review candidate. Date: 2026-07-12.

This facade consumes only a fully resolved Stage 5C-j broker-lifecycle
type-state. It calls the broker-neutral `on_broker_timer` callback in paper mode
and does not attach an intent sink, Redis command stream, broker transport,
FINAM command consumer, or runtime-live execution surface.

Gates:

- input must be `Stage5cBrokerLifecycleResolvedPaperStrategy`;
- `remaining_lifecycle_expectations` must be empty;
- any callback-generated batch from Stage 5C-j must be resolved before timer;
- broker-truth expiry is rechecked;
- timer timestamp must be monotonic against the resolved source-event range;
- context remains `TradeMode::Paper`, `allow_live_orders = false`;
- timer callback-generated intents are captured as a no-send generated batch;
- generated records use per-record `source_event_ts`;
- generated executable intents must still match final pending request state;
- generated cleanup remains an explicit no-pending lifecycle;
- nonzero timer output does not dispatch and must go through Stage 5C-g/i/j
  lifecycle before any later timer/bar facade.

Still closed:

- real timer loop;
- intent sink;
- Redis command stream;
- broker transport;
- FINAM command consumer;
- POST/DELETE order endpoints;
- runtime-live attachment.
