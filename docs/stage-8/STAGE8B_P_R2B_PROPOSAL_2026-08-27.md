# Stage 8B-P R2B — separate read-only preflight proposal

Status: **design-only proposal; `NOT_ISSUED`**.

## Purpose and predecessor

R2A8-R1 was independently accepted at
`5b2079d7d524d2fa6f084f44f961c4b5958c042a`. This proposal describes the
only new capability that a later R2B issuance may open: one explicitly
operator-selected, one-shot FINAM broker-truth preflight for either a PLACE or
CANCEL lifecycle. This commit does not issue that capability and does not use
a credential or network.

The machine-readable authority is
`docs/stage-8/stage8b-p-r2b-proposal-authority.json`. It is normative for this
proposal.

## Exact network surface proposed

The destination is exactly `https://api.finam.ru`. The authentication phase is
exactly two AuthService POST requests:

1. `POST /v1/sessions` — obtain a short-lived read-only token;
2. `POST /v1/sessions/details` — verify that token belongs to the one bound
   account.

These are authentication requests, not order-effect POST endpoints. The
broker-truth phase is GET-only:

- PLACE context: account orders, trades and account/position snapshot;
- CANCEL context: exact order, then account orders, trades and account/position
  snapshot.

`POST /v1/accounts/{account_id}/orders` and
`DELETE /v1/accounts/{account_id}/orders/{order_id}` remain forbidden.
Redirects, proxies, alternate destinations, arbitrary request APIs, retries,
background loops and schedulers remain forbidden.

## Composition and custody

The proposal pins the source adapter, trusted-current manifest issuer,
authority producer/issuer, package issuer, fd-bound launcher and read-only
helper as one ordered composition. R2B issuance must bind their exact reviewed
Linux/amd64 hashes. Controlled/fixture features cannot enter the production
composition.

The account ID and FINAM read-only secret remain fixed credential files under
`/run/credentials/moex-trading/stage8b/r2a5`. A later service definition must
give access only to the dedicated one-shot helper identity. Every signed local
authority, executable hash and package field must validate before the first
credential read.

Network access must be constrained both in process and externally by a
dedicated one-shot service/network namespace that permits only
`api.finam.ru:443`. The service receives no caller-supplied paths and has no
Redis or runtime socket access.

## Request and failure policy

- total requests: PLACE 5, CANCEL 6;
- timeout: 10 seconds per request;
- minimum interval between broker GETs: 250 ms;
- retries: none;
- redirects: none;
- response status: exact 200 only;
- DTO decode, completeness, lifecycle and account binding: strict/fail-closed;
- a full 1000-row trades page means incomplete truth;
- any failure writes only redacted evidence and terminates.

The canonical result is preparation evidence only. It cannot issue K1/K2, an
operator arm, a dispatch attempt or any execution authority. Any future arm
must reread fresh broker truth after that arm under a separately accepted
stage.

## Required independent review before issuance

The R2B proposal review must verify the exact endpoint set, executable hashes,
source-writer order, credential owner, service sandbox, destination allowlist,
authentication, timeout/retry policy, error taxonomy, freshness, DTO mapping,
snapshot completeness, rate limiting, startup/degraded behavior, redacted
evidence and negative harness.

After proposal acceptance, issuance still requires all pending fields in the
machine authority to be closed: fresh official contract refresh, exact Linux
build recheck, production service and credential-custody review, explicit
operator selection and a separately signed exact run package.

## Explicitly closed

This proposal does not authorize or perform FINAM network access. It does not
open order POST/DELETE, Redis live consumption, broker dispatch, Stage 8B-XE,
runtime-live, strategy-live or real orders.
