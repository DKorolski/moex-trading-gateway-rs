# M3j-8 guarded tiny live-order-path implementation diff

M3j-8 is a pre-live implementation-diff package for the future first tiny FINAM
order path. It does not perform the broker boundary call and does not make live
trading reachable by default.

The accepted M3j-7 skeleton stays the guardrail. M3j-8 adds a reportable
candidate state that can be prepared only when the reviewed gates are modeled as
satisfied:

- accepted M3j-7 skeleton;
- compile-time and feature review fixtures;
- endpoint gate approval;
- explicit operator-go artifact;
- fresh immediate pre-run readonly evidence;
- one account, one symbol, one tiny market/limit order;
- no stop, SLTP, bracket, replace, or multi-leg semantics;
- durable audit persisted before the boundary;
- rollback, reconciliation, and EOD report requirements;
- redacted diagnostics only.

Even in the positive candidate fixture the boundary remains non-executing:

- `boundary_invocation_performed = false`;
- `default_reachable = false`;
- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `command_consumer_to_real_finam_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`.

The package also carries the M3j-7 diagnostic hardening requested by review:
unsafe input that tries to open a live boundary is now surfaced separately as
`unsafe_boundary_input_detected`, while exported live-enablement fields remain
forced closed.

M3j-8 is therefore suitable for review as a guarded implementation-diff package,
not as a live trading authorization.
