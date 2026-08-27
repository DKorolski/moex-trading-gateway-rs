# Stage 8B-P R2A8 — trusted current source and schema compatibility

Status: implementation candidate. Real R2B remains `NOT_ISSUED`.

## Scope

R2A8 closes only the two R2A7 review gaps:

1. the ten operational records now have a strict downstream schema that
   requires `adapter_domain` and `adapter_mode`;
2. broker truth/readiness can reach the adapter only through an owner-mediated,
   Ed25519-signed current-source envelope and the fixed one-shot manifest
   issuer.

It also restores exact lowercase-hex single-line grammar for the lifecycle
commitment key.

## Production chain

```text
Stage7bRecoveryReadyOwner
  + exact Stage6 request
  + pinned Stage8A authority root/current sources
    -> publish_stage8b_r2a8_trusted_current_source_from_owner
    -> stage8b-r2a8-trusted-current-source.json (UID 8095)
    -> stage8b-r2a8-current-manifest-issuer (UID 8096)
    -> stage8b-r2a7-reader-manifest.json (UID 8096, atomic)
    -> stage8b-r2a7-source-adapter (UID 8095)
    -> ten provenance-bearing operational records
    -> R2A5 producer / issuer / signed receipts
    -> existing readonly helper preparation boundary
```

The current-source commitment is signed with the Stage8A writer-issuer key
already pinned by the durable operational identity. The adapter recomputes the
commitment, verifies that public key against the durable identity, verifies the
signature, generation, timestamps, 30-second expiry, exact domain and exact
`one_shot_recovery_reader` mode. A root-prepared HMAC manifest without the
trusted source signature is rejected.

The manifest issuer accepts no paths, request IDs, broker truth or readiness on
its CLI. Its production interface is only `--one-shot-production`; all inputs
are fixed paths. Publication is same-directory atomic and fsync-backed.

## Provenance schema

`OperationalAuthorityRecord` requires:

- `adapter_domain`: `production` or `controlled_qualification`;
- `adapter_mode`: `one_shot_recovery_reader`;
- one exact source-specific payload selected by `source_name`.

Production producer entry requires `production`. The separately named
qualification entry requires `controlled_qualification`. Missing, unknown or
cross-domain provenance is fail-closed; fields are not stripped before payload
reduction.

## Key custody and filesystem ownership

- source writer / adapter UID: `8095` (`m8a8095`);
- manifest issuer UID: `8096` (`m8m8096`);
- trusted source root: UID 8095, non-writable by issuer;
- manifest root: UID 8096, non-writable by adapter;
- lifecycle key: UID 8096, group 8095, mode `0640`;
- accepted grammar: exactly 64 lowercase hex characters and at most one final
  LF; no CR, whitespace normalization, uppercase, NUL or extra line.

## Controlled qualification

PLACE and CANCEL use the same current-source writer, manifest issuer, adapter,
record parser, producers and issuers as production. Only the fixed domain and
fixed roots differ. The rehearsal proceeds through signed receipts and the
existing controlled readonly-helper boundary. Production downstream rejects
the controlled records.

## Closed surfaces

- no FINAM credential access;
- no FINAM Auth/GET or order endpoint network;
- no R2B package or operator arm;
- no Stage 8B-XE, dispatch, effect or runtime-live;
- no retry, redirect or proxy changes;
- accepted R1B effect executable remains unchanged.

