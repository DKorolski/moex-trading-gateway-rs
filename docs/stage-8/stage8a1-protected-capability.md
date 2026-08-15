# Stage 8A-1 R2 — operational authority continuity

Status: implementation candidate; independent acceptance pending.

Base candidate: `ef6b9ac70aa8a3cdd6bfaf93f1c1339b030eb75d`.
Accepted predecessor: Stage 8A-0 R1 at `c949d7f83aa87cf990204a5b8ae66e5ca37c9f1d`.

## Purpose

R2 completes the strictly no-send authority boundary started in R1. An opaque
`Stage8ExecutionCapability` can now be minted through a production issuer, but
only for an exact dispatch-ready Stage 6 command covered by the current
authenticated Stage 7B seal. No request extraction, FINAM request builder,
transport, Redis consumer or broker effect is introduced.

## Durable authority

The owner-mediated bridge takes `&mut Stage7bRecoveryReadyOwner`, the lifecycle
commitment key and the exact durable identity/command. It:

1. requires lifecycle availability;
2. rereads and authenticates the committed seal from disk;
3. refreshes the current Stage 6 frontier;
4. requires exact `RequestAccepted` and exact `DispatchAttemptRecorded`;
5. binds both record IDs, dispatch sequence, frontier and runtime config;
6. durably advances a non-covering seal through the accepted atomic procedure;
7. performs a final disk/HMAC reread immediately before returning authority.

Missing, corrupt or valid-but-different current seals fail closed and make the
owner unavailable. An accepted-only request cannot obtain Stage 8 authority.

## Production authority issuer

`Stage8a1OperationalAuthorityIssuer` opens a canonical local authority root and
authenticates:

- `stage8a1-accepted-execution-config.json` plus its domain-separated SHA-256
  sidecar;
- `stage8a1-current-control-state.json` for persistent kill-switch revision,
  exact owner/reconciliation counts and max-one micro budget;
- current typed Stage 7B composite readiness;
- current broker-neutral truth and readiness/schedule snapshots.

The accepted config fixes broker/account/instrument/strategy scope, the full
`OrderPreflightPolicy`, build/config/endpoint provenance and bounded TTLs. The
caller cannot manufacture any opaque proof or widen policy at mint time.

The issuer creates all non-durable opaque authorities: arm, frozen policy,
trusted clock, readiness, kill switch, ownership, zero ambiguity, fresh broker
truth, schedule and budget. PLACE and CANCEL use the same issuer and exact
scope. CANCEL retains exact durable order mapping and terminal no-op behavior.

## Durable arm nonce

Logical arm nonces are registered under operational seal generation using
exclusive `create_new`, file fsync and directory fsync. The nonce key binds
operational identity and generation; the durable record binds command,
capability policy and durable provenance. Reuse in the same generation fails,
including reuse with command or policy drift. A new recovery generation cannot
reuse an old arm authority.

## Current-state continuation barrier

`revalidate_place_capability` consumes the minted capability and returns only
an opaque `Stage8a1CurrentlyAuthorizedCapability`. Before doing so it rereads:

- current Stage 7B disk seal and exact dispatch-ready command;
- immutable accepted config and sidecar;
- persistent control state;
- composite readiness, ownership, ambiguity, broker truth and schedule;
- micro budget, nonce registration and wall-clock expiry.

Any seal, config, build, endpoint, policy, readiness, kill-switch, ownership,
truth, schedule, budget, nonce or time drift fails closed. The continuation type
still has no approved-request/body extraction or transport consumer.

## Evidence contract

- exact 68-row R2 acceptance matrix;
- exact 62-case negative inventory;
- accepted-only versus dispatch-ready tests;
- disk-seal delete/corrupt tests and forward seal-advance witness;
- production issuer and duplicate logical nonce tests;
- post-mint current-state drift tests;
- compile-fail opacity/linearity, exact SHA-pinned closed surfaces;
- serialized workspace tests/doctests, fmt, clippy and immutable handoff.

## Closed surfaces

R2 does not add broker request builders, request extraction, mock send, HTTP
classification, reconciliation, FINAM POST/DELETE, Redis live command
consumption, broker dispatch, runtime-live or real orders. Independent
acceptance of this exact R2 slice opens Stage 8A-2 only.
