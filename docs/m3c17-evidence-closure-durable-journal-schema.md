# M3c-17 evidence closure plan and durable journal schema

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Implementation-gate evidence closure plan

M3c-17 keeps all implementation-gate slots explicit and pending until review:

```text
release_profile_evidence_or_waiver
positive_get_order_evidence_or_waiver
route_template_recheck
undocumented 201/202/204 status semantics
cancel 409/410 status semantics
```

Allowed closure methods are controlled evidence, official documentation
confirmation, or reviewer-accepted waiver. Closure artifacts must remain
redacted, source-archive-bound, and cannot use real PlaceOrder/CancelOrder
endpoints.

## Durable endpoint-attempt journal SQLite schema design

Future endpoint attempts are recorded in a design-only table:

```text
order_endpoint_attempts
schema_version = 1
endpoint_attempt_id_sha256 UNIQUE
request_id_sha256 indexed
client_order_id_sha256 indexed
broker_order_id_sha256 optional
replay_fingerprint_set_sha256
```

The schema stores only hashes, safe enums, booleans, integers, and timestamps.
It does not store raw endpoint-attempt ids, request ids, account ids, broker
order ids, paths, bodies, errors, secrets, or JWTs.

The future write contract inherits the order-path SQLite safety posture:

```text
BEGIN IMMEDIATE required
WAL required
synchronous=FULL required
single writer lock required
schema-version guard required
append-only attempt journal
```

Replay policy:

```text
same endpoint_attempt_id + same fingerprint set -> idempotent replay
same endpoint_attempt_id + different fingerprint set -> reject and disarm
```

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
