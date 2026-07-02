# M3b-19 real-readonly request-bound evidence gate

Status: request-binding hardening before a future controlled FINAM
real-readonly evidence run. M3b-19 still does not authorize FINAM order
placement/cancel, real command consumption, real CommandAck lifecycle, runtime
attachment, `LiveReady`, live micro, stop/SLTP, or bracket behavior.

## Request-bound preflight marker

`FinamRealReadonlyTokenAccountPreflightApproved` is now bound to the exact
redacted request snapshot used when the marker is created.

The operator run blocks before any attempt if the current request snapshot does
not match the marker:

```text
TokenAccountPreflightRequestMismatch
```

This prevents a marker created for one broker-truth reconciliation target from
being reused for a different order/client/instrument request, even when the
account hash still matches `RunApproved`.

## Redacted request snapshot evidence

Operator reports now include a `request_snapshot` diagnostic with:

```text
request_snapshot_fingerprint
request_account_id_len / request_account_id_sha256
request_broker_order_id_len / request_broker_order_id_sha256
request_client_order_id_len / request_client_order_id_sha256
request_instrument_identity_len / request_instrument_identity_sha256
requested_at
position_guard_context
```

The diagnostic contains only lengths, hashes, timestamps, and safe enum/boolean
context. Raw account ids, broker order ids, client order ids, and instrument
identity JSON are not exported.

## Ordered source evidence

Operator reports now include a `source_order` diagnostic:

```text
ordered_sources
ordered_sources_sha256
```

This makes the GET-only probe order visible to review and feeds the ordered
source hash into the run fingerprint.

## Probe run identity update

The probe run fingerprint now includes:

```text
request_snapshot_fingerprint
ordered_sources_sha256
```

alongside timestamp, approved account/base URL hashes, timeout settings, source
list, and request cap.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
