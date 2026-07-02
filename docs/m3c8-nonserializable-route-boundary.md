# M3c-8 non-serializable route boundary

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Boundary hardening

`crates/finam-gateway/src/real_order_endpoint.rs` now keeps future route
templates in an internal-only route shape. That internal route shape is private
to the module and is not serializable.

Only redacted diagnostics may cross export/report boundaries:

```text
GatewayRealOrderEndpointRedactedRouteDiagnostic
route_template_redacted = true
route_template_exported = false
```

The public gated API-shape helpers still require `EndpointGateApproved`, but
they return only redacted diagnostics. They are not transport inputs and do not
submit orders:

```text
place_order_api_shape(&EndpointGateApproved, &FinamPlaceOrderRequestSpec)
cancel_order_api_shape(&EndpointGateApproved, &FinamCancelOrderRequestSpec)
```

## Scanner-transition guard

`scripts/order_endpoint_scanner_transition_spec.sh` now rejects any `reqwest`
token inside the design-only module, in addition to request-builder and
transport-like bypass terms:

```text
.post(
.delete(
.request(
.send(
Method::POST
Method::DELETE
reqwest
HttpClient
Transport
Adapter
Backend
```

This prevents future design-only bypasses through alternative request builder
types, imports, helper aliases, or fully-qualified paths.

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
