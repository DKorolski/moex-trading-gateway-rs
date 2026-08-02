# Stage 5G-d R1-a R1 — complete pre-callback transactional admission

Status: review candidate. Base: `0f72478123c8ddf90c5368ce0cef7867257087c3`.

This successor preserves the accepted R1-a source-owned checkpoint authority
and adds a crate-private transactional admission boundary. The boundary proves
all callback-free conditions before transferring ownership to the inherited
Stage 5C callback path.

The preflight validates settlement readiness, unresolved intent absence,
checked bar checkpoint construction, cross-event monotonicity, instrument and
tick binding, recovery/history/settled chronology, explicit event-time
validity, `evaluation now >= bar checkpoint`, and bootstrap expiry.

Every blocker returns the exact incoming `Stage5cTimerSettlement`. Focused
tests compare checkpoint, strategy fingerprint, complete settled batch history,
intent count, recovery counters/timestamps, warmup provenance and admission
identity. A test-only delegate counter proves that blocked paths never reach
the callback delegate and a valid path reaches it exactly once.

The accepted R1-a authority remains unchanged. Removing the two R1 marker
regions reproduces the exact Stage 5C source from `0f72478`. Stage 5G-d R1-b,
Stage 5G-e/f, Redis, FINAM, HTTP order methods, broker execution and runtime-live
remain closed.
