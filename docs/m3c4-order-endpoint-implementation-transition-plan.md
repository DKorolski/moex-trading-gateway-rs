# M3c-4 order endpoint implementation gate transition plan

Status: design-only transition plan. This document does not add or authorize
real FINAM order `POST` / `DELETE`, command consumption, ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Decision on existing compile trait

`FinamRealOrderEndpointTransport` remains an approved-only compile contract.
It is intentionally allowed to exist so the future real transport signature can
require `&EndpointGateApproved`.

It is not an implementation approval:

```text
EndpointGateApproved remains unconstructible
endpoint_calls_allowed remains false
real_post_delete_calls_allowed_now remains false
```

Any real implementation of the trait still requires a separate implementation
review.

## Scanner transition plan

Current scanner mode:

```text
CurrentDenyAllOrderPostDelete
```

Future scanner mode after a separate implementation review:

```text
FutureExactTwoRouteAllowlistAfterReview
```

The only future routes eligible for allowlisting are:

```text
POST   /v1/accounts/{account_id}/orders
DELETE /v1/accounts/{account_id}/orders/{order_id}
```

The M3c-5 architecture decision resolves the implementation module as:

```text
crates/finam-gateway/src/real_order_endpoint.rs
```

`broker-finam` remains request-spec/route-builder only and must not introduce
real order endpoint HTTP send surfaces. This avoids a dependency cycle because
`EndpointGateApproved` stays in `finam-gateway`.

## Gate requirements

Future route rendering and HTTP send must both require the endpoint gate marker:

```text
EndpointGateApproved
```

The marker remains impossible to construct until a later implementation review
changes the gate constant and proves the required evidence.

## Negative tests after allowlist

After a future allowlist is introduced, the scanner/harness must still reject:

- same-module extra `.post(`;
- same-module extra `.delete(`;
- generic `Method::POST`;
- generic `Method::DELETE`;
- route-string bypasses;
- non-reqwest order endpoint HTTP abstractions.

## Evidence slots before implementation gate

Before any real order endpoint implementation gate can be accepted, these slots
must be either `EvidenceProvided` or `WaiverAccepted`:

```text
release_profile_evidence_or_waiver
positive_get_order_evidence_or_waiver
route_template_recheck
```

## Serializable report

`M3cOrderEndpointGateDesignReport` includes
`implementation_transition_plan` with:

```text
design_only = true
current_scanner_mode = CurrentDenyAllOrderPostDelete
future_scanner_mode = FutureExactTwoRouteAllowlistAfterReview
implementation_location_decision = GatewayHttpSendBrokerFinamRouteBuilder
approved_future_module_path = crates/finam-gateway/src/real_order_endpoint.rs
broker_finam_future_role = request_spec_route_builder_only_no_http_send
finam_gateway_future_role = endpoint_gate_marker_owner_and_future_real_http_send_boundary
dependency_cycle_risk_resolved = true
compile_trait_decision = ApprovedOnlyCompileContract
endpoint_gate_marker_required = true
route_rendering_requires_gate_marker = true
http_send_requires_gate_marker = true
real_post_delete_calls_allowed_now = false
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
