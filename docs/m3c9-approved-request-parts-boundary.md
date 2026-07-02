# M3c-9 approved request-parts boundary

Status: design-only hardening. This increment does not add or authorize real
FINAM order `POST` / `DELETE`, command consumption, real ACK lifecycle,
runtime/live attachment, `LiveReady`, first live micro, stop/SLTP, or bracket.

## Internal capability boundary

M3c-9 adds a design-only internal capability type:

```text
ApprovedOrderEndpointRequestParts
```

It is private to `crates/finam-gateway/src/real_order_endpoint.rs` and is not
`Debug`, `Serialize`, or `Deserialize`.

The rendered path is a separate private type:

```text
RenderedOrderEndpointPath
```

It is also not `Debug`, `Serialize`, or `Deserialize`.

## Required construction inputs

The design-only constructors require all safety inputs:

```text
EndpointGateApproved
approved FINAM request spec
OrderEndpointAccountInstrumentAllowlistApproved
OrderEndpointOperatorArmApproved
OrderEndpointDurableStateCheckpoint
```

The constructors are private and only define the future boundary shape. Because
`EndpointGateApproved` remains unconstructible before implementation review,
these parts cannot be produced in the current code path.

## Diagnostic boundary

Public/reportable shape remains redacted:

```text
rendered_path_redacted = true
rendered_path_exported = false
raw_body_exported = false
diagnostic_can_construct_request_parts = false
```

Diagnostics cannot feed the request-parts constructors. The constructor
signatures accept the approved request specs and safety markers, not exported
diagnostic structs.

M3c-10 adds the next private consumer boundary: it accepts only
`ApprovedOrderEndpointRequestParts`, requires `EndpointGateApproved`, remains
no-network/design-only, and returns only redacted consumer diagnostics.

## Scanner-transition guard

`scripts/order_endpoint_scanner_transition_spec.sh` now also checks that:

```text
RenderedOrderEndpointPath is private and not Debug/Serialize/Deserialize
ApprovedOrderEndpointRequestParts is private and not Debug/Serialize/Deserialize
constructor safety-input markers remain present
diagnostic_can_construct_request_parts = false
rendered_path_exported = false
raw_body_exported = false
```

The existing guard still rejects order-call surfaces and any `reqwest` token in
the design-only module.

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
