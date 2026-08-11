# Stage 7A-R1 review closure

Predecessor: `6e53f5428f7f79f3c9c84fbbd15d32b3c26d5d2d`.

This narrow repair addresses all five P1 findings from the independent Stage
7A review without opening Stage 7B or any live surface:

1. ACK publication state distinguishes XADD failure from XADD-success/XACK
   failure and emits the Stage 5G-compatible duplicate form after publication.
2. The current-lifecycle guard uses explicit non-finality and pins the sole
   source-correlated CANCEL overlap.
3. Source, claim, ACK, DLQ and Stage 6 health are independent; exact blocked
   entries cannot be cleared by unrelated success.
4. Bounded XAUTOCLAIM retains its cursor between calls and has a real-Redis
   tail-coverage witness with `COUNT=1`, one page per invocation.
5. The final gate produces 52 evaluated row records with 52 concrete PASS
   witnesses; the handoff evidence reports inventory/evaluated/pass counts
   separately.

Stage 7B, FINAM POST/DELETE, broker dispatch, runtime-live, real orders and
native protective execution remain closed.

The two non-blocking operational findings remain explicit Stage 7B carry-forward
items: service supervision must prove readiness after task panic/unwind through
an external observer, and production consumer identity must use a boot-instance
UUID rather than PID plus a process-local generation. R1 does not claim either
property. The trusted profile account binding and deterministic profile-drift
rejection findings are closed in R1.
