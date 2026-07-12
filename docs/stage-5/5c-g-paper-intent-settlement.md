# Stage 5C-g — paper intent settlement / escrow

Status: review candidate. Date: 2026-07-12.

The facade consumes `Stage5cSemanticBarResult`, validates every captured intent
and returns `Stage5cSettledPaperStrategy` plus an opaque `Stage5cPaperIntentBatch`.
It validates class, exact routed symbol, positive finite quantity, finite
positive tick-aligned prices and future stop expiry.

Request IDs are not escrow-local. They are derived with the exact
source-compatible action mapping used by the hybrid runtime:

- `Place` -> `place / 0`
- `Cancel` -> `cancel / 1`
- `Replace` -> `replace / 2`
- `Market Buy` -> `market / 3`
- `Market Sell` -> `market / 4`
- `CreateStopLimit` -> `create_stop_limit / 5`
- `DeleteStopLimit` -> `delete_stop_limit / 6`

Settlement verifies those IDs against the post-callback pending fields in
`StrategyState`. Entry and exit intents must match their exact pending request.
Protective repair intents must match TP or SL pending request IDs. Source-level
request ID collisions fail closed instead of being hidden by an escrow index.
Replay bars with nonzero intents fail closed as `ReplayIntentNotExecutable`
until a separate observation-only gate is reviewed.

Accepted source hardening: safety exit branches that already emit executable
live exits now arm the same pending exit lifecycle before returning intents.
This covers `sl_triggered_escalation` and `repair_deadline_force_flatten`, so
future ACK handling can still clear pending state only by exact
`StrategyRequestId`. The operational reason remains recorded in
`safe_mode_reason`; the pending-exit reason uses the existing source enum value
`MeanRevTimeCutoff` as lifecycle metadata.

The batch also binds the post-callback strategy-state SHA256 fingerprint.

Zero-intent bars produce an explicit settled batch with count zero. Intents are
never serialized, published or sent. Redis, broker transport, timer and
next-bar loop remain closed.
