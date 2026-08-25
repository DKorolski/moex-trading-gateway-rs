# Stage 8 slice plan

Status: Stage 8A-0 through Stage 8A-5 are independently accepted and closed.
The accepted aggregate closure is
`bf58b47fdef8af774a4107455dfcc6204e594283`. Stage 8A is formally closed.

Independent acceptance of Stage 8A-5 opens only preparation and review of a
separate Stage 8B design package. Stage 8B execution, real FINAM POST/DELETE,
Redis live consumption, broker dispatch, runtime-live, real strategy orders,
native protective orders and unattended execution remain CLOSED.

Stage 8B-D R1 architecture was retained but not frozen. GOV-CI-1 is now closed.
Stage 8B-D R2 was independently accepted at `f296d0b` and merged to `main` by
`50ed538`. Stage 8B-S R1 at `a675a77` retained the architecture but was not
frozen. Corrective Stage 8B-S R2 at `831eec8` closed its prior findings but was
not frozen because adapter qualification followed exact P authorization.
Stage 8B-S R3 was independently accepted at `afecc258` and merged by
`d158196`. It freezes `D/S/I/IT(no effect)/P(exact build)/XE(one effect)` and
opens only Stage 8B-I no-send implementation and deterministic crash/replay
rehearsal. Stage 8B-I at `a52fbca` was not accepted; corrective Stage 8B-I R2
at `21426ee` was also not accepted.

Stage 8B-I R3 is independently accepted and merged exactly at `0af222f`. The
first Stage 8B-IT candidate `e440539` was rejected. Corrective IT R2 qualifies
module-private request parts through a parent-only adapter, consumes the exact
Stage 8A-2 continuation once and returns only a mandatory Stage 8A-3 classified
observation from controlled numeric-loopback endpoints. It does not authorize
operator-arm issuance, Stage 8B-P/XE, a real FINAM POST/DELETE or any broker
effect. IT R2 was rejected; corrective IT R3 was independently accepted and
merged exactly at `14e01a9f`. Stage 8B-IT-TLS R1 was independently accepted at
`6cb179509fad97e8be56e31bb930b2a86caefc6a` and merged tree-identically. GOV-P1
solo-mode change control and the P preconditions are accepted on `main` at
`16a59bca74f94881c70d9fa39bbdf1c357e65f95`.

Stage 8B-P R1 at `12a7aee` was safely fail-closed but not accepted because its
future exact-run manifest under-bound the accepted Stage 8B-S R3 contract. R1A
at `f922ad6` closed those semantic gaps but was not accepted because its new
endpoint identity differed from the qualified build and run identity lacked a
canonical derivation. Narrow Stage 8B-P R1B binds
the exact qualified endpoint formula/goldens and one canonical computed
PLACE/CANCEL run identity while retaining all R1A protections. R1B selects no
operation, uses no account credential, sends no broker GET or POST/DELETE,
records no dispatch attempt and issues no operator arm. Authorization remains
`NOT_ISSUED`. R1B was independently accepted at `b9a423c` and merged by
`f1070a4`. The active successor is R2A, which freezes a reviewable exact-build,
operator-selected GET-only preparation contract and emits only a redacted
no-GET plan. R2A uses no credential and performs no broker request. Independent
R2A acceptance may open only a separate R2B one-shot GET-only evidence run;
Stage 8B-XE and every execution surface remain closed.

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
