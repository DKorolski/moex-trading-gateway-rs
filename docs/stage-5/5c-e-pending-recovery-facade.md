# Stage 5C-e — pending-stream recovery facade

Status: review candidate.

Date: 2026-07-12.

The facade consumes `Stage5cWarmedPaperStrategy` and opaque validated recovery
evidence. A warmed-bound claim proof fixes strategy/account/instrument, exact
snapshot timestamp, typed streams, consumer groups, terminal claim cursors and
per-stream snapshot boundaries. Events carry stable stream/entry identities plus a total sequence; identical replayed
entries are deduplicated, conflicting duplicates and sequence collisions fail
closed before callbacks.

Recovery rechecks broker-truth expiry and lifecycle clock monotonicity, rejects
future/unrepresentable events, validates all event instruments before mutation,
suppresses snapshot-covered broker-state events, requires ACK request IDs to be
present in restored pending state, and replays only ACK/order/stop/position
callbacks using paper-only `CatchingUp` contexts. Any callback validation error
or emitted intent blocks creation of the next type-state. No intent sink exists.

Successful recovery returns `Stage5cPendingRecoveredPaperStrategy` and records
replayed/duplicate counts. Semantic bars, timers, runtime host, FINAM consumer,
real send and broker-side Stop/SLTP/bracket execution remain closed.
