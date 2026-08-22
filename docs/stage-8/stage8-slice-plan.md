# Stage 8 slice plan

Status: Stage 8A-0 through Stage 8A-5 are independently accepted and closed.
The accepted aggregate closure is
`bf58b47fdef8af774a4107455dfcc6204e594283`. Stage 8A is formally closed.

Independent acceptance of Stage 8A-5 opens only preparation and review of a
separate Stage 8B design package. Stage 8B execution, real FINAM POST/DELETE,
Redis live consumption, broker dispatch, runtime-live, real strategy orders,
native protective orders and unattended execution remain CLOSED.

Stage 8B-D R1 architecture was retained but not frozen. GOV-CI-1 is now closed.
Stage 8B-D R2 is the active docs/checker-only corrective design candidate. It
retains the separately reviewed D/S/I/P/X order and may open only Stage 8B-S
implementation specification after independent acceptance. It additionally
freezes build/feature/toolchain identity, keyed account binding, safe residual
closure, Stage 11 multi-session parity and reachable-action coverage.

It does not authorize a real FINAM POST/DELETE.

## Mandatory Stage 8A order

Each slice requires its own immutable handoff and independent acceptance before
the next slice starts. Acceptance of 8A-0 opens only 8A-1; no Gate acceptance
may skip or open a later slice directly.

1. **Stage 8A-0 — current contract refresh.** Re-fetch the official FINAM REST
   order contract, normalize and hash it, compare it with the project fixture
   and existing vetted builders. Material drift blocks progression.
2. **Stage 8A-1 — protected capability.** Add the private linear no-send
   capability, exact Stage 7B/Stage 6 durable authority, opaque command-bound
   operator arm, frozen allowlist/limit policy, trusted time and scoped
   kill-switch/ownership/ambiguity/truth/schedule/budget authorities. No
   serializer or transport. R3 additionally requires current disk-seal
   revalidation, dispatch-ready durable state, production proof issuers,
   request-keyed one-arm protection, trusted file identities and symmetric
   PLACE/CANCEL revalidation. It requires 76 acceptance rows and 70 exact
   negative cases.
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
