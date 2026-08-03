# Stage 5G-e-c R2 — fully resealed source-bound lifecycle reconstruction

Base commit: `e269b709d2c3e1a2d3892a88099585bce12d0778`.

R2 preserves every accepted R1 boundary and closes the nested-reseal findings.
The only durable document remains the Stage 5D canonical restart package;
Stage 5G contributes one strict, versioned extension and never returns the
consumed source capability.

Timer-ready persistence now retains a crate-private source projection of the
exact Stage 5C `ReadyForContinuation` settlement: zero-intent settled batch,
ordered settled-batch history, recovery-receipt fingerprint and inner
settlement checkpoint. It also retains the source-owned Stage 5G summary and
checkpoint. Restore compares every summary field with that source authority,
requires the outer checkpoint to equal the source checkpoint and not precede
the inner settlement, and rejects missing or grafted authority.

Every extension is bound to one Stage 5D package instance through snapshot
identity/revision/generation, persisted timestamp, the Stage 5D payload
checksum and lifecycle-watermark fingerprint. A source lifecycle commit joins
those Stage 5D components to the Stage 5G lifecycle/source/checkpoint
fingerprints. A complete extension graft from another same-strategy package
therefore fails closed.

The reconstructed capability contains a non-serializable validated next-stage
authority. Timer-ready observations derive from the validated settlement
projection; committed-awaiting observations derive summary/checkpoint from the
persisted order-position state. No observation reads an unvalidated free
summary.

Package-level semantic mutation first strict-decodes the extension and
recomputes the nested lifecycle proof before recomputing extension/envelope/
package checksums. The 34 focused tests include four public clean-process
roundtrips and exact-error witnesses for request/count/fingerprint mutation,
valid checkpoint graft with adjusted watermarks, inner-settlement regression,
recovery-receipt graft and complete same-binding extension graft. The 25-case
negative matrix pins these enforcement points.

Fresh BrokerTruth reconciliation, GRST01–GRST12, Stage 5G-f, Redis/FINAM/HTTP,
broker execution, runtime-live, real orders, main merge and deployment remain
closed.
