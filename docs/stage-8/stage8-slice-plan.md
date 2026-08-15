# Stage 8 slice plan

Status: Stage 8A-0 is independently accepted and closed at `c949d7f`; Stage
8A-1 protected-capability implementation is the only open candidate.

Independent acceptance of Stage 8A-0 opens only Stage 8A-1. Stage 8A-2 through
8A-5, real FINAM POST/DELETE, broker dispatch, runtime-live, real strategy
orders, native protective orders and unattended execution remain CLOSED.

It does not authorize a real FINAM POST/DELETE.

## Mandatory Stage 8A order

Each slice requires its own immutable handoff and independent acceptance before
the next slice starts. Acceptance of 8A-0 opens only 8A-1; no Gate acceptance
may skip or open a later slice directly.

1. **Stage 8A-0 — current contract refresh.** Re-fetch the official FINAM REST
   order contract, normalize and hash it, compare it with the project fixture
   and existing vetted builders. Material drift blocks progression.
2. **Stage 8A-1 — protected capability.** Add the private linear no-send
   capability, exact operator arm, allowlist, limits, Day-only TIF policy and
   fail-closed kill-switch preflight. No serializer or transport.
3. **Stage 8A-2 — builder composition.** Compose only
   `broker_finam::build_place_order_request()` and
   `broker_finam::build_cancel_order_request()` behind a mock/no-send seam. A
   second Stage 8 serializer is forbidden.
4. **Stage 8A-3 — endpoint classifier.** Implement distinct PLACE and CANCEL
   status tables. Generic all-4xx rejection is forbidden.
5. **Stage 8A-4 — reconciliation.** Normalize fresh broker truth, correlate by
   exact durable identity and keep `ProvenNoMatch` unconstructible. Empty,
   missing or stale truth remains `StillUnknown`.
6. **Stage 8A-5 — aggregate acceptance.** Re-run inherited Stage 7B gates, the
   Stage 8-specific scanner, exact negatives, debug/release tests and immutable
   source/evidence binding. All network-send surfaces remain closed.

## Later work

Stage 8B is a separately specified and independently accepted bounded real
engineering micro. It may be designed only after accepted Stage 8A-5. It
requires at most one explicitly armed engineering command, current read-only
broker truth, durable attempt-before-send evidence, the same fail-closed kill
switch and operator authorization. It does not attach an autonomous strategy
runtime.

Stages 9+, continuous reconciliation, runtime-live and native Stop/SLTP/bracket
remain closed. No later stage is opened by this plan.
