# Stage 6 bounded implementation plan

Stage 6 opens only after independent acceptance of Transition Gate 5→6.

1. **6A — durable identity/schema.** Broker-neutral versioned record types,
   canonical encoding, causal IDs and compile/fixture tests. No filesystem,
   Redis or broker transport.
2. **6B — isolated journal backend.** Append/read/checkpoint against a local
   temporary filesystem or in-memory test backend, fsync policy and corruption
   tests. No runtime attachment.
3. **6C — crash/replay state machine.** Exact replay, conflict detection and
   all ten transition crash windows using deterministic fixtures.
4. **6D — Stage 5 paper/mock integration.** Connect the journal facade to the
   accepted Stage 5 callback/lifecycle boundary without Redis, FINAM or broker
   dispatch.
5. **6E — aggregate durable-chain acceptance.** Debug/release/restart evidence,
   semantic Stage 5↔Stage 6 boot cross-binding, opaque accepted broker-truth
   authority, debug/release evidence, negative matrix, immutable descriptor
   and closure package. Implementation candidate is documented in
   [stage6e-live-durable-chain-closure.md](stage6e-live-durable-chain-closure.md).

Each slice requires independent acceptance before the next opens. Stage 7,
Redis `XREADGROUP`/`XAUTOCLAIM`, FINAM POST/DELETE, broker dispatch,
runtime-live, real orders, unattended scheduling and native stop/SLTP/bracket
remain closed throughout Stage 6.
