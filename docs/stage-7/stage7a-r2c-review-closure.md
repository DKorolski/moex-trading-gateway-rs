# Stage 7A-R2c — A-036 boot-instance identity closure

Stage 7A-R2c is the direct successor of reviewed candidate `be62ed0`. It makes
one production correction: automatic Redis consumer naming is now unique per
process boot rather than only per PID and process-local counter.

`paper_default_auto()` obtains one random UUID from a process-lifetime
`OnceLock`, combines it with the PID and an atomic per-consumer generation, and
delegates to an injectable private constructor. Two injected boot UUIDs with
the same PID and generation produce different names. Repeated constructors in
one process reuse the boot UUID but receive different generations.

The checker proves that `consumer_name` is absent from the command handler and
therefore cannot enter `StrategyRequestId`, `ClientOrderId` or Stage 6 durable
execution identity. The negative inventory is increased from 49 to 50 with an
exact mutation that removes the boot nonce from the generated name.

R2c also binds A-044 to the explicit
`cross_process_exactly_once_claimed=false` evidence token. It does not claim
cross-process execution exactly-once, does not add production persistence and
opens no Stage 7B, FINAM, broker dispatch, runtime-live or real-order surface.
