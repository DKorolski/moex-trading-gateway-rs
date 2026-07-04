# M3j-11 final operator-run authorization gate

M3j-11 is the final authorization gate model for a future tiny operator-run. It
separates three things that must not be conflated:

- authorization decision;
- runtime live attachment;
- actual broker boundary invocation.

This package can model a one-shot authorization decision when actual fresh
artifacts are present:

- accepted M3j-10 refresh package;
- actual timestamp-bound redacted operator GO artifact;
- actual fresh redacted broker-truth artifact;
- active, unknown active, and orphan active orders equal to zero;
- flat or explicitly expected position;
- one-shot TTL arm, not expired, with no auto-rearm;
- runtime kill switch verification and armed policy binding;
- session-bound one-order, tiny-quantity, and micro notional/loss limits;
- durable audit-before-boundary proof;
- explicit signed one-attempt final authorization that expires after TTL;
- no raw operator GO, broker truth, or account export.

Even when the decision is `OneShotAuthorizationReady`, M3j-11 still reports:

- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `boundary_invocation_performed = false`;
- `command_consumer_to_real_finam_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`;
- `stop_sltp_bracket_replace_multileg_allowed = false`.

The `live_micro_go_decision` value is therefore a reviewed authorization
decision, not a runtime switch and not an order send.

Any request to attach runtime/live or perform a boundary invocation inside this
gate is unsafe and blocks the authorization decision.
