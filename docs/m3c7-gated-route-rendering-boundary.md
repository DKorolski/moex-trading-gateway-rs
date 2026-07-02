# M3c-7 gated route-rendering boundary

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Boundary hardening

`api_shape()` is now a design/report shape only:

```text
api_shape_contains_route_templates = false
```

It must not be used as future transport input.

Route templates are separated into gated route-shape functions:

```text
place_order_api_shape(&EndpointGateApproved, &FinamPlaceOrderRequestSpec)
cancel_order_api_shape(&EndpointGateApproved, &FinamCancelOrderRequestSpec)
```

Those functions remain API-shape only and still do not send orders, but they
make the future route-rendering boundary explicit: any future route rendering
must pass through an `EndpointGateApproved` marker.

## Stronger scanner-transition guard

`scripts/order_endpoint_scanner_transition_spec.sh` now rejects the following
inside `crates/finam-gateway/src/real_order_endpoint.rs` while the module is
design-only:

```text
.post(
.delete(
.request(
.send(
Method::POST
Method::DELETE
reqwest::Client
HttpClient
Transport
Adapter
Backend
```

M3c-8 supersedes the `reqwest::Client` term with a broader any-`reqwest`
token guard and makes route templates internal-only/non-serializable, with
only redacted diagnostics exported.

## Still not allowed

- FINAM real PlaceOrder `POST`;
- FINAM real CancelOrder `DELETE`;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
