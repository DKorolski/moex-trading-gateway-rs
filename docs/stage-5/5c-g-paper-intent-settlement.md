# Stage 5C-g — paper intent settlement / escrow

Status: review candidate. Date: 2026-07-12.

The facade consumes `Stage5cSemanticBarResult`, validates every captured intent
and returns `Stage5cSettledPaperStrategy` plus an opaque `Stage5cPaperIntentBatch`.
It validates class, exact routed symbol, positive finite quantity, finite
tick-aligned prices and future stop expiry. Deterministic paper request IDs are
bound to strategy/account/instrument/bar and intent index. The batch also binds
the post-callback strategy-state SHA256 fingerprint.

Zero-intent bars produce an explicit settled batch with count zero. Intents are
never serialized, published or sent. Redis, broker transport, timer and
next-bar loop remain closed.
