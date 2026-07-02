# M3c-5 implementation boundary architecture decision

Status: design-only architecture decision. This increment does not add or
authorize real FINAM order `POST` / `DELETE`, command consumption, real ACK
lifecycle, runtime/live attachment, `LiveReady`, first live micro, stop/SLTP,
or bracket.

## Decision

Use the gateway-owned HTTP-send boundary:

```text
GatewayHttpSendBrokerFinamRouteBuilder
```

Meaning:

```text
broker-finam = request spec / route builder only, no real HTTP order send
finam-gateway = EndpointGateApproved owner and future real HTTP send boundary
```

The future implementation module is:

```text
crates/finam-gateway/src/real_order_endpoint.rs
```

This avoids a Rust workspace dependency cycle because `EndpointGateApproved`
stays in `finam-gateway`, and `broker-finam` does not need to depend back on
`finam-gateway`.

## Compile trait decision

`FinamRealOrderEndpointTransport` remains:

```text
ApprovedOnlyCompileContract
```

It is a compile-time contract for the future boundary, not implementation
approval. `EndpointGateApproved` remains unconstructible and
`endpoint_calls_allowed` remains `false`.

## Scanner transition design

Current scanner mode remains:

```text
CurrentDenyAllOrderPostDelete
```

Future scanner mode, only after separate review:

```text
FutureExactTwoRouteAllowlistAfterReview
```

The future exact allowlist may contain only:

```text
POST   /v1/accounts/{account_id}/orders
DELETE /v1/accounts/{account_id}/orders/{order_id}
```

The future scanner must verify:

- exactly one place-order POST surface;
- exactly one cancel-order DELETE surface;
- only the approved `finam-gateway` module path;
- only the two allowed route templates;
- route rendering requires `EndpointGateApproved`;
- HTTP send requires `EndpointGateApproved`;
- all negative harness bypass cases still fail.

## Pending evidence slots

Before any implementation gate, these must be `EvidenceProvided` or
`WaiverAccepted`:

```text
release_profile_evidence_or_waiver
positive_get_order_evidence_or_waiver
route_template_recheck
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
