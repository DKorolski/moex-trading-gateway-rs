# Stage 7A-R2 review closure candidate

Predecessor: `ac8fa7f2f3ff42ae1b351c298ff0b3abd62599b5`.

R2 is a narrow paper-only closure repair for the independent R1 re-review. It
does not open Stage 7B, FINAM transport, broker dispatch or runtime-live.

The five blocking repairs are:

1. The frozen maximum is strict: one non-final Stage 6 command request per
   strategy instance. A normalized paper outcome is followed by explicit
   `RequestFinalized` before ACK. A later CANCEL may use the preserved broker
   order identity, but never overlaps a non-final PLACE request.
2. Source polling and claim scanning own separate health flags and timestamps.
   Group attachment, source-only, or claim-only operation cannot produce
   `PaperReady`.
3. Consumer task liveness has an external `Arc<AtomicBool>` observer and a
   drop guard. Normal return, returned error, panic and JoinError
   all clear liveness without trusting a finally path inside the task.
4. The original R2 fault matrix is superseded by
   `stage7a-r2a-fault-matrix.json`; the replacement preserves F1-F15 while
   closing canonical-ACK recovery and Redis claim-outage gaps.
5. The original R2 proof map is superseded by
   `stage7a-r2a-acceptance-proof-map.json`, the sole current acceptance
   authority for all 52 frozen rows.

A-040/A-041 are closed explicitly as N/A-by-closed-surface because Stage 7A
does not construct fresh broker-truth packages. This is a static policy proof,
not a waiver and not an adjacent behavioral test.

Consumer boot UUID and cross-process exactly-once remain Stage 7B carry-forward
items. Stage 7B, Stage 8+, real orders and native protective execution remain
closed pending separate independent acceptance.
