# Stage 5C-f — first semantic-bar facade

Status: review candidate. Date: 2026-07-12.

The facade consumes only `Stage5cPendingRecoveredPaperStrategy` and an opaque
Stage 3-accepted final M10 bar. Before mutation it rechecks broker-truth expiry,
exact instrument/tick binding, future time and strict ordering after both
history and recovery boundaries. Forming, raw M1, native FINAM M10 pending
characterization, stale and duplicate bars fail closed.

Only `on_broker_bar` is invoked in paper/LiveReady context. Generated intents
are captured inside `Stage5cSemanticBarResult`; no sink, Redis command stream,
broker transport or send path is attached. Timers remain closed.
