# M3d-0 implementation-transition decision package

Status: source-bound transition preparation after accepted M3c-26
pre-implementation gate package. This increment records the reviewer acceptance
and defines the future exact-two-route scanner transition rules. It does not
add, enable, or authorize real FINAM order endpoint calls.

## Goal

M3d-0 starts the implementation-gate transition track without opening the
trading boundary. It records:

- M3c-26 acceptance as the input package;
- current scanner mode remains `CurrentDenyAllOrderPostDelete`;
- future scanner mode is limited to an exact-two-route allowlist only after a
  separate explicit review;
- future implementation module is limited to
  `crates/finam-gateway/src/real_order_endpoint.rs`;
- `EndpointGateApproved` remains unconstructible;
- `endpoint_calls_allowed = false`;
- `real_order_endpoint_enabled = false`.

## Exact future route decision

The only future routes that may be considered by a later implementation review
are:

```text
POST   /v1/accounts/{account_id}/orders
DELETE /v1/accounts/{account_id}/orders/{order_id}
```

M3d-0 does not enable these routes. They remain design data until a later
review explicitly approves executable endpoint source.

## Audit-driven follow-up

The 2026-07-03 ALOR-to-FINAM engineering audit v2 accepts M3d-0 as a safe
transition gate but changes the next work item from endpoint implementation to
M3d-1 FINAM contract alignment. Before executable order endpoint source is
added, the project must align:

- `TimeInForce` enum mapping;
- FINAM order status classification;
- instrument registry / schedule validation as a `LiveReady` blocker;
- pinned FINAM spec fixtures and drift tests.

The broader plan is recorded in
`docs/m3d-operational-parity-roadmap.md`; the next sprint ticket is
`docs/m3d1-finam-contract-alignment.md`.

## Required future scanner failures

A future allowlist scanner must still fail on:

- any extra place-order route;
- any extra cancel-order route;
- generic request bypass;
- route-string bypass;
- non-reqwest order endpoint abstraction bypass;
- order `POST` / `DELETE` in any module other than
  `crates/finam-gateway/src/real_order_endpoint.rs`;
- stop/SLTP/bracket endpoint expansion;
- runtime command-consumer bypass.

## Evidence artifact

After generating the source-bound M3c-26 package for the current clean handoff
archive, run:

```bash
python3 scripts/m3d0_implementation_transition_decision.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

It writes:

```text
reports/m3d-implementation-transition/implementation-transition-decision.json
reports/m3d-implementation-transition/implementation-transition-decision.json.sha256
```

The package verifies:

- M3c-26 package is present, accepted, and source-bound to the same archive;
- all M3c evidence slots remain closed;
- forbidden surface scanner is green;
- negative scanner harness is green;
- scanner transition spec is green;
- trading boundary remains closed.

## Still not allowed

- real FINAM PlaceOrder `POST`;
- real FINAM CancelOrder `DELETE`;
- making `EndpointGateApproved` constructible;
- enabling allowlist scanner mode;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM;
- runtime/live attachment;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
