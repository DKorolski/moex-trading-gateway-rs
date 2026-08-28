# Stage 8B-P R2B Proposal R1 — production composition and durable evidence correction

Status: **design-only proposal; `NOT_ISSUED`**.

## Purpose and predecessor

R2A8-R1 was independently accepted at
`5b2079d7d524d2fa6f084f44f961c4b5958c042a`. This proposal describes the
only new capability that a later R2B issuance may open: one explicitly
operator-selected, one-shot FINAM broker-truth preflight for either a PLACE or
CANCEL lifecycle. This commit does not issue that capability and does not use
a credential or network.

R1 corrects the rejected `94345a9` proposal: it adds the missing production
current-source writer, orders the composition by its real artifact dependencies,
separates production and controlled build hashes, and makes one durable terminal
record mandatory after every admitted run. The machine-readable authority is
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

The exact production flow is:

1. the production current-source writer,
   `stage8b-r2a8-production-current-source-writer`, consumes one signed,
   fixed-path intake owned by UID 8094, independently restores the Stage 7B
   owner and exact durable request, and atomically publishes the trusted
   current source;
2. `stage8b-r2a8-current-manifest-issuer` reads that source and publishes the
   reader manifest;
3. `stage8b-r2a7-source-adapter` reads that manifest and emits the eleven
   operational authority records;
4. the authority producer and issuer run exactly once for each of the eleven
   source classes;
5. the package issuer, fd-bound launcher and read-only helper each run once;
6. the helper finalizes exactly one durable terminal evidence record for the
   consumed nonce.

The writer accepts no CLI arguments, path parameters or unsigned snapshots.
Its owner-mediated publication seam is crate-private; the controlled seeder is
compiled only in the separate controlled qualification feature domain. The
writer has no FINAM network, credential, Redis, dispatch or runtime-live
authority. It uses fixed roots and revalidates the signed intake against the
root-owned Stage 8A accepted config, Stage 7B recovery owner and exact durable
request before publication.

R2B issuance must bind the exact reviewed Linux/amd64 hashes from
`production_linux_amd64_sha256`. Controlled PLACE/CANCEL hashes live only in
`controlled_qualification_linux_amd64_sha256`; they cannot satisfy a
production package, service or launcher hash requirement.

The helper embeds the hash-free runtime composition contract
`stage8b-p-r2b-runtime-composition-contract.json`; executable hashes remain in
external build and issuance evidence. This prevents a self-referential helper
hash cycle while preserving an exact semantic composition fingerprint in every
terminal record.

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
- the trades query is exactly `limit=1000`,
  `interval.start_time=request_requested_at-86400000ms` and
  `interval.end_time=request_requested_at`, encoded as RFC3339 UTC seconds;
- pagination/cursors and caller overrides are forbidden;
- any failure writes only redacted evidence and terminates.

After the run nonce is durably consumed, both success and failure are finalized
through one outer helper scope. Partial request attempts survive later failure.
The record is written below
`/var/lib/moex-trading/stage8b/r2b-evidence` through a create-new, no-follow,
single-link, bounded pending inode, file `fsync`, atomic link publication and
directory `fsync`. A pre-existing nonce record, custody/mode drift or any write
failure is fail-closed as `EVIDENCE_PERSISTENCE_FAILURE`; the nonce remains
consumed, automatic retry is forbidden and operator review is required. The
record contains no secret, token, raw body or account identifier.

The canonical result is preparation evidence only. It cannot issue K1/K2, an
operator arm, a dispatch attempt or any execution authority. Any future arm
must reread fresh broker truth after that arm under a separately accepted
stage.

## Required independent review before issuance

The R2B proposal review must verify the exact endpoint set, production and
controlled executable hash domains, source-writer implementation and order,
terminal success/failure persistence, credential owner, service sandbox, destination allowlist,
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
