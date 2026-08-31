# Stage 8B-P R2A2 — semantic/provenance qualification

R2A2 is a corrective, no-send qualification slice. It retains the R2A1
request topology but removes its caller-forgeable pre-network truth model.

## Trust chronology

Only eleven local facts may exist before network access. Each is carried in a
source-specific HMAC receipt, bound to the exact run, keyed account identity,
accepted effect build, issuer/schema, key generation and observation time.
The key directories are fixed production constants and cannot be selected by
CLI input.

Account/orders/trades/positions and the target position are never accepted in
that envelope. They are derived only from AuthService and fresh BrokerTruth
responses in the same controlled run.

## R1B validation

The helper enforces the complete closed manifest field set and canonical run
identity, exact accepted build/source/archive/executable/config/policy/API and
renderer identities, current authenticated generations/checkpoints/control
facts, bounded expiry, canonical decimal/notional arithmetic, exact CANCEL
lifecycle identities, keyed account HMAC and the accepted endpoint identity
formula.

## Broker truth

The reducers use closed typed DTOs. Missing collections, `{}`, `null`, unknown
fields, unknown order states, wrong account/instrument/client/order identity,
duplicate target positions, non-working CANCEL targets and baseline position
mismatch all block. PLACE and CANCEL produce only redacted semantic counts,
canonical position and domain-separated receipts.

Endpoint body caps are fixed at 64 KiB for AuthService, 256 KiB for exact
order, 4 MiB for orders/account and 16 MiB for trades. Both declared and
streamed oversize bodies block.

## TLS and launch boundary

Controlled rustls tests qualify the standalone helper for a valid chain and
hostname and reject wrong CA and wrong hostname before HTTP. Redirects,
automatic retries and proxies remain disabled.

The R2A2 binary has no credential-bearing production entry and exits
fail-closed. R2B may introduce a launcher only after an independent review
freezes the exact R2A2 release-binary SHA-256; self-hash is not authority.

## Closed surfaces

No real credential, AuthService request, broker GET, arm, dispatch attempt,
effect transport, FINAM order POST/DELETE, Redis execution or runtime-live is
authorized. Status remains `NOT_ISSUED`.
