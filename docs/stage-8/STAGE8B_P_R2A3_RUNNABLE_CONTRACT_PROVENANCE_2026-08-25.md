# Stage 8B-P R2A3 — runnable contract/provenance correction

Status: implementation candidate; independent acceptance required. R2B is not
authorized and no real FINAM credential or request is used in this slice.

## Exact runnable boundary

The release helper contains the same production one-shot entry intended for
R2B. It accepts only `--r2b-one-shot` or `--qualify-controlled`; there is no
scheduler, loop or caller-selected path. The production mode first requires a
root-owned `ISSUED` run package whose helper digest, operation, run nonce and
read-contract snapshot match the executable and manifest. R2A3 ships no such
package, so the production entry stops before credentials and network.

The Linux launcher opens the fixed helper with `O_NOFOLLOW`, checks root
ownership, mode, link count, inode identity and SHA-256 on the open descriptor,
then uses `execveat(..., AT_EMPTY_PATH)`. No ambient environment is forwarded.

## Current FINAM read contract

The six official FINAM pages for Auth, TokenDetails, GetAccount, Trades,
GetOrders and GetOrder are stored byte-for-byte and hash-bound by
`stage8b-p-r2a3-finam-read-contract-snapshot.json`. Strict DTOs and six golden
fixtures cover every currently documented response field, including
`triggered_order_id`, account margin/PnL/portfolio fields and trade comment,
accrued-interest and currency. Future unknown fields block and require a new
contract refresh.

## Provenance and custody

Eleven source-specific producer identities (`8101..8111`) publish only a closed
typed claim projection at fixed paths. Eleven different issuer identities
(`8201..8211`) read those owner-pinned projections and hold one source-specific
Ed25519 private key each. The helper has only root-owned public keys. Every
signature binds source, producer snapshot hash, run identity, keyed account,
build identity, observation time and run nonce.

Producer service name, UID, positive generation, run nonce and exact claim-name
set are checked before signing. PLACE and CANCEL have distinct exact Stage 6
claim inventories. Private keys must be issuer-owned and mode `0600`; public
keys and run package are root-owned and non-writable by group/other. Receipts
are non-secret issuer-owned `0644` files. A root-owned `O_EXCL` nonce registry
permanently blocks a second invocation after a run package is claimed.

Key provisioning or rotation is outside an R2A3/R2B run and requires separate
change control. A JSON issuer label without the source-specific signature and
custody checks is rejected.

## Freshness and broker truth

Control-source skew is at most 1000 ms; schedule/instrument skew is at most
5000 ms. Manifest and signed authorities are revalidated before AuthService,
before the BrokerTruth class, before every GET and before final evidence.
Token creation/expiry timestamps are parsed. Request start/end are recorded,
and broker GET starts are separated by at least 250 ms.

PLACE blocks any prior matching trade. CANCEL requires each relevant trade to
bind the exact target order. Exact-order and list views must agree on broker
order ID and all accepted immutable order fields.

## Controlled TLS qualification

The exact runnable helper executes two controlled AuthService POSTs and the
complete three-GET PLACE BrokerTruth sequence through rustls. This is local
controlled traffic with a non-production secret. Existing wrong-CA and
wrong-host tests prove rejection before HTTP; redirects, automatic retries and
proxy use remain disabled.

## Explicitly closed

R2A3 authorizes none of: real credential, real AuthService POST, real broker
GET, operator arm, dispatch, effect transport, order POST/DELETE, Redis
execution or runtime-live. Independent R2A3 acceptance is required before an
operator may consider a separately issued R2B read-only run package.
