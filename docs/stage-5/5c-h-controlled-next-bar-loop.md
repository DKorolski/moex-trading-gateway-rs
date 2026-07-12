# Stage 5C-h - controlled next-bar loop

Status: review candidate. Date: 2026-07-12.

The facade advances only from an already settled `Stage5cSettledPaperStrategy`.
It consumes one `Stage5cAcceptedSemanticBar`, reuses the Stage 5C semantic bar
callback path, and then requires successful Stage 5C-g settlement before
returning the next `Stage5cSettledPaperStrategy`.

Gates:

- input must be a settled type-state, not a raw strategy or raw semantic result;
- next bar close time must be strictly greater than the previous settled batch;
- broker-truth expiry is rechecked through the semantic bar path;
- Stage 3 final-M10 admission is reused before callback;
- escrowed intents from previous batches are never redispatched;
- redacted settled batch summaries are accumulated for parity evidence.

Still closed:

- timer path;
- intent sink;
- Redis command stream;
- broker transport;
- FINAM command consumer;
- POST/DELETE order endpoints;
- runtime-live attachment.
