# Stage 8B-P R2A7 production source-reader qualification

Status: implementation candidate. This slice does not authorize R2B or any
FINAM request.

## Scope

R2A7 adds one exact one-shot executable:

```text
stage8b-r2a7-source-adapter --one-shot-production
```

The executable accepts no path, request id, JSON document, credential, arm or
effect argument. It reads only fixed durable roots, obtains exclusive Stage 7B
recovery ownership, derives exactly one current reconciliation-required Stage
6 request from authenticated replay, composes current Stage 8A sources and
calls the existing owner-mediated R2A6 publication seam once.

The production feature `stage8b-r2a7-source-adapter` contains no fixture
feature. `stage8b-r2a7-controlled-qualification` is a separate superset used
only by the fixture seeder and Linux qualification. Both PLACE and CANCEL
qualification invoke the exact production adapter executable built without
fixture features.

## Fixed production topology

```text
/var/lib/moex-trading/stage8b/r2a7/production
  stage8b-r2a7-reader-manifest.json
  stage8b-r2a7-lifecycle-key.hex
/var/lib/moex-trading/stage7b
/var/lib/moex-trading/stage8a1-authority
/var/lib/moex-trading/operational-authorities
```

The reader manifest is authenticated with the existing Stage 5G lifecycle
commitment key and binds the operational identity, accepted Stage 8A config,
current broker truth/readiness, adapter domain and fixed runtime profile. This
key is internal restart authentication material, not a FINAM credential. The
manifest and key are root-owned, non-symlink, single-link, bounded regular
files (production files are installed `root:m8a8095`, mode `0640`). The
adapter runs as UID/GID 8095 and has no network address family. Published
records remain mode `0644`, because the already accepted downstream producer
identities are distinct read-only Unix users.

Production orchestration must stop the prior Stage 7B writer before this
one-shot reader starts and must make the accepted durable root writable by the
fixed adapter identity. Lock contention, missing input, zero/multiple current
requests, stale/final requests, identity drift, HMAC drift or source provenance
drift fail closed before publication.

## Provenance separation

Every output record contains both:

```json
{"adapter_domain":"production","adapter_mode":"one_shot_recovery_reader"}
```

or, under the disjoint qualification roots:

```json
{"adapter_domain":"controlled_qualification","adapter_mode":"one_shot_recovery_reader"}
```

The adapter re-reads all ten published records and verifies the exact expected
domain. Controlled records therefore cannot satisfy a production R2B input.
R2B must additionally bind the accepted R2A7 build and executable hashes in
its typed decision and signed package before any credential read.

## Deliberately closed

- FINAM credentials and network;
- AuthService and broker GET/POST/DELETE;
- Redis command consumption;
- operator arm, dispatch and effect transport;
- background loop and unattended execution;
- R2B, Stage 8B-XE, runtime-live and real orders.

R1B effect build/executable, R2A5 helper, TLS, package, receipts and reducers
remain unchanged.
