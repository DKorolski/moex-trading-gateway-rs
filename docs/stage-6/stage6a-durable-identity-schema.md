# Stage 6A — durable identity/schema candidate

Accepted predecessor: `14359aadb3178c83692441b748b060d06ce12903`.

This bounded slice adds only broker-neutral, in-memory schema types. It binds each accepted MARKET/LIMIT/CANCEL command to its `StrategyRequestId`, derived `ClientOrderId`, account, instrument and Hybrid attribution. A cancel has its own derived durable client ID; `CancelOrder.client_order_id` remains a separately stored target-order correlation.

Journal record identity is `SHA256("stage6-journal-record-v1" || request UUID bytes || big-endian non-zero sequence)`. The business payload is excluded from that identity and receives its own canonical SHA-256, allowing a later stage to classify same-position/different-payload conflicts.

The v1 record and command snapshot are opaque and validated on construction and deserialization. Broker observations require a strict non-zero lowercase evidence digest. Golden compact JSON covers place and cancel request acceptance.

Closed in this slice: persistence backend, filesystem I/O, Redis, FINAM, HTTP POST/DELETE, broker dispatch, runtime callback attachment, workers, scheduling, live orders, ReplaceOrder, stop/SLTP/bracket and Stage 6B+.

Acceptance command: `bash scripts/stage6a_gate.sh`.

## R1 hardening candidate

R1 closes constructor/deserializer equivalence. Every identity and snapshot constructor now runs the same intrinsic validator used by deserialization; every record constructor validates the completed record. Empty account, symbol, native symbol and incomplete Hybrid attribution are rejected locally. Place accepts only Entry/Exit/TakeProfit/StopLoss, while Cancel requires Cancel.

`decode_canonical` now requires byte-for-byte equality after validated re-encoding and wire types reject unknown fields. Reserved Stage 6C event names cannot form Marker records in Stage 6A. The compact v1 Place and Cancel golden bytes and hashes remain unchanged.

R1 acceptance command: `bash scripts/stage6a_r1_gate.sh`.

## R2 Place-shape closure candidate

R2 keeps schema v1 and the R1 validation architecture unchanged while closing
the final structural Place invariant. Durable Place snapshots accept only
`Market` with no limit price or `Limit` with a strictly positive limit price;
both require strictly positive quantity. Existing broker-core Stop,
StopLimit, TakeProfit and TakeProfitLimit variants are rejected rather than
stored in the incomplete Stage 6A payload shape.

The rule is private, broker-neutral and called only through snapshot intrinsic
validation, so public construction, snapshot Deserialize and journal decode
share the same authority. Policy-dependent lot/price steps, size limits,
notional, operator arm and fresh market reference remain outside Stage 6A.
Both v1 golden fixtures remain byte-identical.

R2 acceptance command: `bash scripts/stage6a_r2_gate.sh`.
