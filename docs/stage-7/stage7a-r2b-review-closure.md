# Stage 7A-R2b — narrow review closure

Stage 7A-R2b is the direct successor of rejected candidate `340845c`. It
changes no Stage 6 persistence authority and opens no Stage 7B, FINAM or live
execution surface.

The patch closes the three R2a review findings:

1. A profile mismatch first checks whether the `StrategyRequestId` is already
   established in ACK publication state, canonical ACK recovery or Stage 6
   replay. Established identity returns pending `IdentityConflict`; only a new
   unknown request receives deterministic `LocalValidationRejected`.
2. Direct authority and real-Redis witnesses cover account, instrument,
   PLACE-to-unrelated-CANCEL action/target conflicts, retained F6 recovery,
   PEL retention and absence of ACK/DLQ/XACK.
3. A direct ClientOrderId mismatch witness proves zero paper-provider calls,
   no successful lifecycle and no Redis settlement. The 49-case negative
   inventory is pinned in the source descriptor, checker, harness and A-050
   token.

The accepted Stage 6E-R1 detached gate, F1-F15 matrix, same-authority canonical
ACK recovery and all R2a Redis/readiness repairs remain unchanged.
