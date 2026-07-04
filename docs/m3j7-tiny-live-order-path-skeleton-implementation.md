# M3j-7 tiny live-order-path skeleton implementation package

M3j-7 adds a scanner-controlled skeleton for a future tiny live-order path.

This is not a real order implementation. It does not contain a network client,
does not render live route values for transport, and does not submit real FINAM
orders.

Current posture:

```text
implementation_skeleton_only = true
real_boundary_call_reachable = false
live_micro_go = false
```

## Skeleton constraints

The skeleton requires:

- accepted M3j-6 design package;
- compile-time gate closed;
- feature gate disabled;
- endpoint gate closed;
- no operator GO artifact;
- no immediate pre-run read-only evidence;
- kill switch coverage across runtime, command consumer, and endpoint paths;
- one order / one symbol / one account / tiny quantity;
- Market/Limit only;
- Stop/SLTP/bracket/replace/multi-leg forbidden;
- durable audit before future boundary crossing;
- rollback plan;
- post-run reconciliation;
- EOD report;
- scanners blocking unguarded boundary methods;
- redaction.

## Boundary interpretation

`CandidateOnly` means the skeleton structure is reviewable. It does not mean a
real boundary call is reachable.

The report still emits:

- `real_boundary_call_reachable = false`;
- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `external_finam_order_calls_allowed = false`;
- `command_consumer_to_real_finam_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`.

Any future real boundary call must be a separate reviewed package.
