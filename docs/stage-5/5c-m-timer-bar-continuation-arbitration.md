# Stage 5C-m - timer/bar continuation arbitration

Status: review candidate. Date: 2026-07-12.

This facade consumes only `Stage5cTimerSettlement` produced by Stage 5C-l. It
does not dispatch intents, attach Redis, attach broker transport, or invoke
FINAM command surfaces.

Gates:

- input must be a Stage 5C-l timer settlement type-state;
- only `ReadyForContinuation` can advance;
- `GeneratedIntentBatch` is blocked until Stage 5C-i ACK lifecycle and Stage
  5C-j broker lifecycle resolve it;
- a ready checkpoint can be consumed by exactly one next transition:
  - next final semantic bar; or
  - next monotonic timer;
- timer continuation requires `now_ts_utc_ms` strictly greater than the
  checkpoint timestamp;
- broker-truth expiry is rechecked before timer continuation;
- timer continuation reuses Stage 5C-k no-send settlement rules for generated
  intents and attribution;
- Rust move semantics prevent reusing the same checkpoint for both bar and
  timer paths.

Still closed:

- automatic runtime loop;
- intent sink;
- Redis command stream;
- broker transport;
- FINAM command consumer;
- POST/DELETE order endpoints;
- runtime-live attachment.
