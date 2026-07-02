# M3c-10 approved request-parts consumer boundary

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Internal consumer boundary

M3c-10 adds a private, gateway-owned consumer boundary:

```text
consume_approved_request_parts_for_future_endpoint(
    &EndpointGateApproved,
    ApprovedOrderEndpointRequestParts,
)
```

The function is private to `crates/finam-gateway/src/real_order_endpoint.rs`.
It accepts the internal approved request-parts capability, not exported
diagnostic DTOs.

## Still no network behavior

The consumer is still a design shape only:

```text
consumer_internal_only = true
consumer_requires_endpoint_gate = true
consumer_accepts_approved_request_parts_only = true
consumer_accepts_diagnostics = false
consumer_network_enabled = false
rendered_path_exported = false
raw_body_exported = false
runtime_ack_redacted_only = true
```

It does not call FINAM, does not submit orders, and does not introduce any
network client or request-builder surface.

## Diagnostic boundary

The consumer returns only a redacted diagnostic shape:

```text
GatewayRealOrderEndpointConsumerDiagnostic
```

The diagnostic records method, operation, presence/length metadata, and safety
flags. It does not export the rendered path, account id, broker order id,
instrument symbol, client order id, or raw request body.

M3c-11 extends this design-only boundary with a future send outcome/result
shape. The classifier still requires `EndpointGateApproved`, consumes
`ApprovedOrderEndpointRequestParts` by value, remains no-network, and records
single-use/no-blind-retry/state-machine-required policy.

## Negative invariants

Tests and scanner guards cover:

```text
consumer requires EndpointGateApproved
consumer accepts ApprovedOrderEndpointRequestParts
diagnostic DTOs cannot feed the consumer
consumer is not public
consumer_network_enabled = false
rendered_path_exported = false
raw_body_exported = false
```

The existing scanner also keeps rejecting order-call surfaces and any `reqwest`
token in the design-only module.

## Evidence slots

These slots remain deliberately pending before any real order implementation
gate:

```text
release_profile_evidence_or_waiver = Pending
positive_get_order_evidence_or_waiver = Pending
route_template_recheck = Pending
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
