# Stage 5C-i - paper intent lifecycle / ACK escrow resolution

Status: review candidate. Date: 2026-07-12.

This facade consumes a `Stage5cSettledPaperStrategy` with a nonzero paper
intent batch and applies an explicit ACK lifecycle outcome for every exact
`StrategyRequestId` captured by Stage 5C-g settlement.

Gates:

- input batch must have `intent_count > 0`;
- current strategy state fingerprint must still match the settled batch
  fingerprint;
- every ACK request ID must belong to the settled batch;
- every settled batch request ID must receive exactly one ACK;
- duplicate and unknown ACK request IDs are rejected;
- no new request IDs are generated;
- no semantic bar callback is invoked;
- ACKs are applied only through the broker-neutral runtime callback;
- callback-produced intents are rejected.

Still closed:

- order/stop/position fill lifecycle;
- timer path;
- intent sink;
- Redis command stream;
- broker transport;
- FINAM command consumer;
- POST/DELETE order endpoints;
- runtime-live attachment.
