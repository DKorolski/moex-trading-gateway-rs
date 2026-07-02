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
real_post_delete_calls_allowed_now = false
```

It contains no reqwest client, no real HTTP send, no `.post(`, no `.delete(`,
and no `Method::POST/DELETE`.

## Gate-marker signatures

Future route-shape functions require `EndpointGateApproved` in their signatures:

```text
place_order_api_shape(&EndpointGateApproved, &FinamPlaceOrderRequestSpec)
cancel_order_api_shape(&EndpointGateApproved, &FinamCancelOrderRequestSpec)
```

The marker remains unconstructible, so these signatures are compile/API shape,
not implementation enablement.

## Scanner transition spec

The API shape exports:

```text
current_mode = CurrentDenyAllOrderPostDelete
future_mode = FutureExactTwoRouteAllowlistAfterReview
exact_place_order_surface_count = 1
exact_cancel_order_surface_count = 1
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
