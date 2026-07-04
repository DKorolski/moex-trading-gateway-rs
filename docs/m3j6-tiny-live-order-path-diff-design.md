# M3j-6 tiny live-order-path diff design package

M3j-6 is a design/review package for a future tiny live-order-path diff.

It still does not implement real FINAM order transport and does not authorize
first live micro.

Current posture remains:

```text
live_micro_go = false
real_send_reachable_by_default = false
```

M3j-6 does not enable:

- `LiveReady`;
- runtime live attachment;
- external FINAM POST/DELETE;
- command-consumer-to-real-FINAM transport;
- non-loopback order endpoint;
- Stop/SLTP/bracket/replace/multi-leg.

## Required design properties

A future tiny live-order-path diff must prove:

- no real send is reachable by default;
- real send is compile-time or feature-gated;
- endpoint gate is required;
- explicit operator GO artifact is required;
- immediate pre-run read-only evidence is required;
- kill switch blocks runtime, command consumer, and endpoint paths;
- first micro scope is one order / one symbol / one account / tiny quantity;
- Market/Limit only;
- Stop/SLTP/bracket/replace/multi-leg disabled;
- durable audit required;
- rollback plan required;
- post-run reconciliation required;
- EOD report required;
- scanners block unguarded POST/DELETE;
- redaction is required.

## Boundary interpretation

`tiny_live_order_path_diff_design_ok = true` means only that the future diff
design is reviewable.

It does not mean that live order transport exists or is allowed. The report
still emits:

- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `external_finam_post_delete_allowed = false`;
- `command_consumer_to_real_finam_transport_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`;
- `real_send_reachable_by_default = false`.

Any actual real transport must be a separate reviewed implementation package.
