# Stage 8B-P R1B endpoint/run identity correction

Status: design-only corrective candidate; authorization `NOT_ISSUED`.

R1B corrects only the two identity blockers found in independent R1A review.
It changes no Rust, Cargo, config or workflow and performs no account/broker
request, arm issuance, attempt append, transport entry or FINAM effect.

## Exact qualified endpoint identity

R1B does not invent or migrate endpoint identity. It binds the exact
IT/TLS-qualified `compose_endpoint_identity()` implementation from
`stage8b_no_send.rs`:

```text
domain = stage8b-i-r2-endpoint-identity-v1
SHA256(domain || for each part: u64be(length) || raw_part)
parts = method, route_template_id, keyed_account_binding_hex,
        endpoint_renderer_sha256_hex
```

There is no separate operation component and no NUL-delimited encoding.
Operation is represented by the only accepted method/route pair: PLACE is
POST/PlaceOrderV1 and CANCEL is DELETE/CancelOrderV1. The non-secret qualified
goldens are `84e170…b48` and `f00cff…78a`. Domain, encoding, order, method,
route, account binding and renderer mutations fail closed.

## Canonical accepted-run identity

`run_identity_sha256` is computed, never trusted as a caller assertion. Its
domain is `stage8b-p-r1b-accepted-run-identity-v1`; it uses the same exact
`digest_parts` raw-domain plus u64be-length encoding. Parts are the exact ASCII
values in the frozen common field order (excluding `run_identity_sha256`
itself), followed by the exact PLACE or CANCEL field order.

The operation discriminator is included. Hashes are lowercase hexadecimal,
numeric generations are canonical unsigned decimal strings, expiry is exact UTC
with millisecond precision, and approved position is a canonical signed decimal
string. No Unicode normalization, implicit coercion, alternate object ordering
or serialization is accepted.

The preimage binds process boot, keyed account and generation, complete build,
source/archive/executable, separate config and policy plus aggregate authority,
instrument/API, renderer, corrected endpoint identity, corrected network policy,
Stage 7B/Stage 6/budget/kill/ownership, freshness authority, expiry, approved
position and the operation-specific body digest. PLACE and CANCEL full golden
manifests produce `bd3e79…410c` and `cb2778…3e4` respectively.

## Retained boundary

All accepted R1A rules remain unchanged: process boot, canonical max-notional,
numeric freshness budgets, closed PLACE/CANCEL union, K1 arm chronology,
R2/K2 separation and full build identity. The inherited 98 mutations remain
mandatory and R1B adds 36 identity mutations. R1B and R2 cannot issue an arm.

Only independent R1B acceptance may open a separate GET-only R2 package.
Stage 8B-XE and all real execution surfaces remain closed.
