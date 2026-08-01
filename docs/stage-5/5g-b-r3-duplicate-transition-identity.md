# Stage 5G-b R3 — duplicate transition identity

R3 is exactly one successor to
`d03f6e5e88fb853290457d6d6dac08f21c2cf28b`. It closes the R2 review's single
fingerprint-collision finding without opening Stage 5G-c or any live surface.

The transition projection now binds
`current_lifecycle_fingerprint_sha256 = stage5g_state_fingerprint(state)`.
Consequently, post-resolution duplicate sequence, count and ACK receive-time
watermark are all part of transition identity. The lifecycle schema/domain is
v4.

Executable evidence starts two identical resolved histories and applies an
exact duplicate at `T+20` versus `T+30`. Their transition fingerprints differ.
A sequence-3 duplicate at `T+25` is accepted only after the first history and
blocks with `NonMonotonicAckTime` after the second. A separate public-wrapper
test reaches this boundary from a real Stage 5C settled Market capability and
proves that duplicate replay changes transition identity without repeating the
Stage 5C callback.

The exact R2 commit is preserved by detached snapshot. The R3 handoff is also
push-bound: local `stage5g-lifecycle` HEAD must equal
`origin/stage5g-lifecycle` before archive construction.

Stage 5C/5D/5F, Broker Core, Redis command consumption, FINAM/HTTP transport,
dispatch, order/trade/position lifecycle, runtime-live, real orders, Stage
5G-c, Stage 6, main merge and deployment remain unchanged and closed.
