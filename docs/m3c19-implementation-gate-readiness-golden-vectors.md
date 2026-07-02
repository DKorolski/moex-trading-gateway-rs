# M3c-19 implementation-gate readiness and golden vectors

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Implementation-gate readiness checklist

M3c-19 records a design-only readiness checklist separating items that are
already implemented/tested from items still pending evidence or waiver.

Implemented/tested items:

```text
forbidden-surface scanners
EndpointGateApproved unconstructible
durable store safety
operator arm
rate-limit/backoff
no-blind-retry
redacted ACK/export policy
```

Pending evidence or waiver before implementation gate:

```text
release-profile evidence/waiver
positive GetOrder evidence/waiver
route-template recheck
```

Design-recorded items added in this step:

```text
canonical replay golden vectors
operator replay runbook
```

Readiness checks cannot call FINAM order endpoints and cannot export raw
account/order/path/body/error values.

## Canonical replay golden vector

M3c-19 adds a fixed synthetic vector for schema v1:

```text
name = place_order_schema_v1_sorted_keys_no_whitespace
encoding = UTF-8 JSON, sorted keys, no whitespace
expected_sha256 = d467afd3b7d320c26966a1a400995e00664397ed47bb74320a418cfd2524abc6
```

The vector uses only synthetic hashes and safe labels. Any change to the field
set, ordering, encoding, or vector hash requires an explicit schema bump and
reviewer acceptance before implementation gate.

## Operator replay runbook

M3c-19 links endpoint-attempt-id lifecycle to operator-visible cases:

```text
same fingerprint replay -> same endpoint_attempt_id allowed, no disarm
conflicting replay -> disarm, new endpoint_attempt_id required
timeout unknown pending -> disarm, new endpoint_attempt_id required
manual intervention -> disarm, new endpoint_attempt_id required
terminal outcome new attempt -> new endpoint_attempt_id required
```

All runbook diagnostics remain redacted. Raw endpoint-attempt ids are not
exported.

## Follow-up in M3c-20

M3c-20 adds the self-contained evidence closure package, route-template recheck
plan, and `design-evidence.json` enrichment with M3c-19 readiness/golden-vector
summary fields. All five implementation-gate slots remain closable only by
evidence or reviewer-accepted waiver, with order endpoint calls still forbidden.

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
