# M3c-6 scanner transition spec and pre-implementation API shape

Status: design-only. This increment does not add or authorize real FINAM order
`POST` / `DELETE`, command consumption, real ACK lifecycle, runtime/live
attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## API shape module

The future gateway-owned order endpoint boundary is represented by:

```text
crates/finam-gateway/src/real_order_endpoint.rs
```

This module is deliberately API-shape only:

```text
mode = DesignOnlyNoHttpSend
api_shape_contains_route_templates = false
real_post_delete_calls_allowed_now = false
```

It contains no `reqwest` token, no real HTTP send, no `.post(`, no `.delete(`,
no `.request(`, no `.send(`, and no `Method::POST/DELETE`.

## Gate-marker signatures

M3c-7 separates the design/report shape from gated route-shape functions.
Future route-shape functions require `EndpointGateApproved` in their signatures:

```text
place_order_api_shape(&EndpointGateApproved, &FinamPlaceOrderRequestSpec)
cancel_order_api_shape(&EndpointGateApproved, &FinamCancelOrderRequestSpec)
```

The marker remains unconstructible, so these signatures are compile/API shape,
not implementation enablement.

M3c-8 keeps the actual route template shape internal-only and non-serializable.
Exported gated helpers return only redacted diagnostics:

```text
GatewayRealOrderEndpointRedactedRouteDiagnostic
route_template_redacted = true
route_template_exported = false
```

M3c-9 adds a private approved request-parts design boundary. The public API
shape records:

```text
approved_request_parts_type_internal = true
rendered_path_type_internal = true
rendered_path_exported = false
raw_body_exported = false
diagnostic_can_construct_request_parts = false
constructor_count = 2
```

M3c-10 records the private approved request-parts consumer design:

```text
consumer_internal_only = true
consumer_requires_endpoint_gate = true
consumer_accepts_approved_request_parts_only = true
consumer_accepts_diagnostics = false
consumer_network_enabled = false
consumer_count = 1
```

M3c-11 records the design-only future send result boundary:

```text
outcome_count = 8
future_send_requires_endpoint_gate = true
future_send_consumes_request_parts = true
future_send_network_enabled = false
operation_specific_durable_checkpoint_required = true
request_parts_reuse_after_outcome_allowed = false
retry_after_timeout_unknown_allowed = false
state_machine_transition_required = true
```

## Scanner transition spec

The API shape exports:

```text
current_mode = CurrentDenyAllOrderPostDelete
future_mode = FutureExactTwoRouteAllowlistAfterReview
exact_place_order_surface_count = 1
exact_cancel_order_surface_count = 1
allowed_route_template_count = 2
approved_module_path = crates/finam-gateway/src/real_order_endpoint.rs
real_post_delete_calls_allowed_now = false
```

The future allowlist remains design data only:

```text
POST   /v1/accounts/{account_id}/orders
DELETE /v1/accounts/{account_id}/orders/{order_id}
```

## Shell guard

`scripts/order_endpoint_scanner_transition_spec.sh` verifies that the design-only
module exists, contains the required gate-marker/API-shape markers, and still
has no HTTP send surface.

## Pending evidence slots

Before any implementation gate, these slots must be `EvidenceProvided` or
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
