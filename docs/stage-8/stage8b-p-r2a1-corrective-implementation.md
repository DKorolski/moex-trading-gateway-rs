# Stage 8B-P R2A1 — corrective read-only preflight qualification

R2A1 resolves the cross-layer mismatch found in independent review of R2A. It
does not perform a broker read and does not authorize R2B.

## Chosen topology

The project uses explicit Option B: two AuthService POST requests followed by
an operation-specific GET-only BrokerTruth plan. The overall operation is not
described as GET-only.

- PLACE: `POST sessions`, `POST token details`, then orders, trades and
  positions GETs — exactly five source-level requests.
- CANCEL: the same two auth POSTs, then exact order, orders, trades and
  positions GETs — exactly six source-level requests.

The accepted effect executable with SHA-256 `677f277d...f3f06` is unchanged.
The new helper is an independent workspace under
`tools/stage8b-readonly-preflight`; it is non-authority and cannot issue K2,
arm, attempt, dispatch or effect capability.

## Boundary hardening

Both helper clients use HTTPS-only production routing, 10-second timeout,
`retry(never)`, `redirect(Policy::none())` and `no_proxy()`. Tests exercise a
302 and a connection close and prove one source-level attempt. Full local mock
tests prove exact PLACE and CANCEL method/order/count sequences.

The CLI accepts only a closed R1B manifest, a current-source envelope and an
output path. It derives its own executable SHA-256 and reads the account and
secret only from the process environment. Inputs must be regular non-symlink
files and output is create-new.

## Current source contract

All 17 inputs are bound to exact issuer, schema, digest domain, observation
timestamp, inherited R1A freshness budget, skew group and run/account/build
identity. Missing, duplicate, unknown, stale, excessive-future, skewed,
ambiguous or digest-mismatched evidence blocks before any network client is
created. Output contains only source digest and timestamp maps.

## Query/completeness contract

Orders use client-side account/instrument/order-identity filtering. Trades use
limit 1000, a 24-hour window based on `RequestRequestedAt`, and
`SinglePageNoCursor`. A full page blocks as incomplete. Callers cannot override
this policy.

## Closed surfaces

R2A1 uses no credential and sends no real HTTP request. Authorization remains
`NOT_ISSUED`. R2B, operator arm, durable attempt, effect transport, FINAM order
POST/DELETE, Redis execution, runtime-live and real strategy orders remain
closed pending independent acceptance.
