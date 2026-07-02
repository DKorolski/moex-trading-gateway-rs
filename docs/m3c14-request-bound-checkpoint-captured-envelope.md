# M3c-14 request-bound checkpoint and captured envelope design

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Request-bound durable checkpoint proof

M3c-14 binds the future durable checkpoint proof to a redacted request snapshot
fingerprint. A checkpoint marker is not just tied to an event; it is tied to the
specific future order intent:

```text
request_id hash present
client_order_id hash present
account hash present
instrument hash present
request snapshot fingerprint sha256 length = 64
raw request values exported = false
```

The proof must match the operation and approved request parts. A Place
checkpoint cannot be reused for Cancel, and a checkpoint from one intent cannot
be reused for another intent.

Marker creation remains private and requires:

```text
EndpointGateApproved
SQLite transition commit proof
durable_commit_observed = true
diagnostic_or_report_source = false
marker_single_use = true
matching request snapshot fingerprint
```

## Cancel accepted response/id policy

M3c-14 records cancel accepted response semantics:

```text
matching broker order id -> CancelSubmitted
missing broker order id  -> accepted pending reconciliation
mismatched broker id     -> manual/reconciliation conflict
```

Response body/id is documented as optional for the design stage; raw broker
order id is never exported. Missing or mismatched identity remains
no-blind-retry.

## Redacted captured response/error envelope

Future captured response/error evidence must use a redacted envelope:

```text
kind = AcceptedResponse | BrokerErrorResponse | TransportError
status code presence only
body presence/len/sha256 only
transport category only
error len/sha256 only
raw path/body/error exported = false
```

The envelope maps transport categories from M3c-13 and cannot feed transport or
runtime ACKs directly.

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
