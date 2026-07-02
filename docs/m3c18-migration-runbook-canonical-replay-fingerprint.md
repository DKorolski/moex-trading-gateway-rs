# M3c-18 migration runbook and canonical replay fingerprint

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Durable journal migration runbook

M3c-18 records the future SQLite journal migration/runbook boundary as design
data only. Before any future endpoint gate can become constructible, the
operator-facing runbook requires:

```text
backup before migration
single-writer lock
SQLite open with WAL
synchronous=FULL
schema-version guard
order_endpoint_attempts table creation
replay indexes
integrity_check
refuse auto-repair
```

Any open failure, corruption signal, stale/unknown writer lock, schema mismatch,
or integrity-check failure disarms order endpoints and requires operator
intervention. Automatic repair and automatic stale-lock deletion are explicitly
not allowed.

The runbook exports only redacted diagnostics. Raw SQLite paths, request values,
broker payloads, secrets, JWTs, paths, bodies, and errors remain outside the
public review/report boundary.

## Canonical replay fingerprint

M3c-18 defines the canonical replay fingerprint as a stable, schema-versioned
field list encoded as UTF-8 JSON with sorted keys and no whitespace. The field
order is part of the contract:

```text
schema_version
operation
endpoint_attempt_id_sha256
request_id_sha256
client_order_id_sha256
account_sha256
instrument_sha256
checkpoint_label
request_fingerprint_sha256
checkpoint_proof_sha256
captured_envelope_sha256
outcome_sha256
state_transition_sha256
ack_diagnostic_sha256
```

The contract stores/exports hashes and safe labels only. Refactors that change
the field set, ordering, or encoding require a schema bump.

## Endpoint attempt id lifecycle

The design-only lifecycle is:

```text
generated after approved request parts
bound before future endpoint send
persisted with the attempt journal
reused only for idempotent replay with the same fingerprint set
never reused for a new attempt after terminal/manual/timeout outcome
```

Timeout/manual/terminal outcomes require a new `endpoint_attempt_id` for any
new network attempt. The raw attempt id is not exported; only the SHA-256 shape
is visible in design/report artifacts.

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
