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
- ACK input is provided as records with a strict `total_sequence`;
- duplicate sequence, duplicate request ID and unknown request ID are rejected;
- ACKs before the escrowed intent source timestamp are rejected; for merged
  callback-generated batches this is checked per record, not against one
  batch-level timestamp;
- canonical application order is by `total_sequence`, independent of input Vec
  order;
- recoverable preflight blocks return the original settled type-state;
- successful ACK coverage preserves the full escrow batch and typed ACK
  outcomes for the next lifecycle facade;
- no new request IDs are generated;
- no semantic bar callback is invoked;
- ACKs are applied only through the broker-neutral runtime callback;
- callback-produced intents are explicit terminal evidence.

Still closed:

- order/stop/position fill lifecycle;
- timer path;
- intent sink;
- Redis command stream;
- broker transport;
- FINAM command consumer;
- POST/DELETE order endpoints;
- runtime-live attachment.
