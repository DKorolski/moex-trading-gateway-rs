# Stage 8B-P R1A corrective authorization contract

Status: design-only corrective candidate; authorization `NOT_ISSUED`.

R1A closes the eight contract gaps found in independent review of R1. It does
not select an operation, use an account credential, perform a broker GET, issue
an operator arm, append a dispatch attempt, construct rendered request parts,
enter transport, or send a FINAM POST/DELETE. Stage 8B-P, Stage 8B-XE, Redis
execution and runtime-live remain closed.

## Lineage and scope

R1A is additive to R1 candidate
`12a7aeec20824d3b90e18caa5961ba28a3eb7fd6` and retains its 48 fail-closed
mutations. It does not reopen Stage 8B-I, Stage 8B-IT, TLS qualification,
GOV-P1 or production Rust. The full accepted execution-build identity is the
single SHA-256 `ca330934df540de69b52d0463a665a1ab0ff89fa13eeb663b162763cc6bc83a0`;
R2 may not reconstruct a weaker identity from a subset of build fields.

## Closed discriminated manifest

The future exact run manifest is a closed `PLACE | CANCEL` discriminated union.
Common fields bind request/client identity, exact process boot, keyed account
and key generation, full execution build, source/archive/executable,
config/policy/instrument/API identities, endpoint and network policy, Stage 7B,
Stage 6, durable budget, kill switch, ownership, freshness authority, expiry,
run identity and approved pre-run position.

`PLACE` alone carries IMOEXF@RTSX, side, exact quantity `1`, LIMIT, DAY,
canonical limit price, canonical maximum notional and the place-body identity.
`CANCEL` alone carries the exact broker order ID, lifecycle fingerprint, exact
matching request/client identities, currently-working proof and cancel-body
identity. Unknown fields, missing fields, fields from the other variant and the
old `limit_price_or_cancel_target` conflation all fail closed.

## Process boot and arm chronology

`process_boot_fingerprint_sha256` must equal the current boot authority.
Omission, mutation, reuse after restart or cross-boot substitution fails closed.

The pre-arm manifest contains no issued-arm-shaped metadata. In particular,
arm ID/nonce, issuance time, durable issuance row, covering seal and arm expiry
do not exist until K1 creates them. An optional `arm_nonce_commitment` is only a
non-authority precommitment and cannot prove arm issuance. Neither R1A nor R2
may issue an arm.

## Canonical decimal and notional

The only accepted decimal spelling matches
`^(0|[1-9][0-9]*)(\.[0-9]*[1-9])?$`; price and maximum notional must also be
strictly positive. No sign, exponent, whitespace, leading zero or redundant
fractional zero is accepted. Price multiplied by exact quantity uses checked
decimal arithmetic. Missing/noncanonical values, overflow and a result above
`max_notional_canonical_decimal` fail closed. The bound is checked before the durable
attempt append and rechecked immediately before K4 may enter transport.

## Endpoint and network identity

The immutable network authority fixes HTTPS/TLS and host `api.finam.ru`.
PLACE is POST + `PlaceOrderV1`; CANCEL is DELETE + `CancelOrderV1`. Redirects,
proxy use, alternate hosts, arbitrary request APIs and automatic transport retry
are false. The run binds both the network-policy digest and a domain-separated
endpoint identity over operation, method, route-template ID, keyed account
binding and the exact accepted endpoint-renderer digest. Raw account IDs and
rendered paths are never exported.

## Freshness authority

R1A proposes the previously pending S-055 numeric decision as one independently
reviewable immutable authority. Runtime current sources use strict 1–5 second
source-specific ages and bounded skew. The public API contract snapshot has a
separate 24-hour age limit and is not compared to runtime cross-source skew.
Missing, unknown, stale, future-dated beyond allowance, caller-selected or
digest-modified policy fails closed. These values become authority only after
independent R1A acceptance; R1A itself issues no execution capability.

## R2 evidence is not K2 authority

`R2ReadOnlyPreflightEvidence != Stage8bK2FreshSources`. Any future R2 GET-only
evidence is preparation evidence only. It cannot satisfy K1/K2 freshness,
cannot be converted into K2, cannot enter XE as current broker truth and cannot
be promoted to transport. After an exact durable arm exists, K2 must freshly
reread and reduce every accepted current source under the frozen budgets.

Only independent acceptance of this R1A package may open a separate R2
operator-selected GET-only preflight. R2 still cannot issue an arm or perform a
broker effect.
