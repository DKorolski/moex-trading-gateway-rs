# ADR: Stage 5G-c replay package identity gate

Status: review candidate

## Context

The accepted Stage 5G-c replay key truncated `BrokerTruthSnapshot.received_ts` to milliseconds. Two legitimate snapshot packages observed inside one millisecond therefore shared an identity and the second package was rejected as conflicting duplicate evidence.

Stage 5G-d restart/durable replay and stream reuse require a source-owned discriminator that is independent of strategy lifecycle fields and independent of the payload fingerprint.

## Decision

Use a versioned canonical package discriminator derived exclusively from the full-precision source-owned `BrokerTruthSnapshot.received_ts`:

```text
moex.broker-truth.package.v1:<unix-seconds>:<nanoseconds-nine-digits>
```

The Stage 5G evidence identity is domain-separated `v3` and binds request, account and this package discriminator. The canonical payload fingerprint remains separate.

Consequences:

- exact package replay has the same identity and fingerprint and remains idempotent;
- changed payload under the same exact source receipt has the same identity but a different fingerprint and fails closed as `ConflictingDuplicateEvidence`;
- different full-precision source receipts inside one millisecond have different identities and are both admissible;
- source package order is checked against an exact `DateTime<Utc>` continuation watermark, not a millisecond truncation;
- reverse exact-receipt order fails as `BrokerTruthTimeRegression`;
- missing package receipt is structurally impossible because `BrokerTruthSnapshot.received_ts` is required by the broker-neutral schema;
- `total_sequence`, attribution and payload hash do not contribute authority to package identity.

The snapshot assembly authority must preserve the exact receipt across persistence/restart. An exact-receipt collision with a changed payload is treated as ambiguous and fails closed; it is never silently converted into a new identity.

## Schema impact

The order/position lifecycle schema advances to v4 and the evidence fingerprint schema to v3. The lifecycle fingerprint binds both the exact package discriminator and the compatibility millisecond watermark.

## Closed surfaces

This gate does not open Stage 5G-d. Redis live consumers/groups, FINAM transport, HTTP POST/DELETE, broker dispatch/execution, runtime-live, real orders, Stage 6, `main` merge and deployment remain closed.
