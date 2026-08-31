# Stage 8B-P R2A — controlled GET-only preflight contract

Status: implementation/evidence contract candidate. No broker request was sent.
Authorization remains `NOT_ISSUED`.

R2A freezes the mechanism that may later perform one operator-selected,
read-only FINAM preflight. It does not select PLACE or CANCEL, use a credential,
send a GET, issue an arm, append a dispatch attempt, enter effect transport or
send POST/DELETE. A separately accepted R2B run is required for real GET
evidence.

## Exact inherited identity

The only executable permitted for R2B is the previously qualified
`aarch64-apple-darwin` `broker-cli` with SHA-256
`677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06`.
Rebuilds and alternate binaries fail closed. R2A changes no Rust, Cargo,
configuration or workflow and therefore does not create a second transport or
invalidate the accepted P build.

The accepted R1B endpoint and run identity algorithms remain exact. R2 may not
serialize the run identity differently, accept a caller-supplied digest, or
derive an endpoint identity without the exact current keyed account binding.

## Operator selection

The selection file is local, untracked and closed-schema. It selects exactly
one operation, `PLACE` or `CANCEL`, and binds the accepted run fields. PLACE is
only IMOEXF@RTSX LIMIT DAY quantity one with canonical price/notional. CANCEL
requires one exact currently-working broker order from the same lifecycle.
Secrets and raw account IDs never enter the selection or redacted evidence.

R2A intentionally contains no filled operator selection. Independent R2A
acceptance is followed by an explicit operator decision and a separate R2B
evidence run.

## Current source inventory

R2B must pin and freshly validate the Stage 7B recovery seal, exact
dispatch-ready Stage 6 command, Stage 8A root/config/policy/current control,
trusted clock, readiness, RunAllowed kill switch, FINAM ownership, schedule,
instrument specification, account binding, broker orders/trades/positions,
ambiguity/orphan/unresolved lifecycle state, durable micro-budget and the
target-instrument pre-run position.

The qualified read-only command performs at most four ordered GETs:

1. exact GetOrder;
2. account OrdersSnapshot;
3. account TradesSnapshot;
4. account PositionSnapshot.

Timeout is 10 seconds, minimum request interval is 250 ms and the token/account
preflight marker is valid for at most 60 seconds. Retry, redirect, proxy,
background loop and scheduler are disabled. Only redacted hashes, counts,
statuses, route templates and timestamps may cross the evidence boundary.

## Non-authority rule

`R2ReadOnlyPreflightEvidence != Stage8bK2FreshSources`.

R2 evidence cannot satisfy K1/K2, issue an arm, append
`DispatchAttemptRecorded`, enter XE or authorize POST/DELETE. If a future arm is
issued, K2 must freshly reread all current authorities after that arm. Cached R2
evidence is preparation evidence only.

## Promotion

Independent R2A acceptance opens only R2B, requiring:

- an exact local operator selection;
- explicit permission for the one-shot read-only GET run;
- a local read-only credential;
- a fresh public contract refresh;
- exact qualified executable verification;
- a separate immutable evidence package and review.

Stage 8B-XE, arm issuance, attempt append, effect transport, POST/DELETE,
broker effect, Redis execution, runtime-live and strategy-live remain closed.
