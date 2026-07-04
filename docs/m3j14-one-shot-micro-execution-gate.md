# M3j-14 one-shot micro execution gate

M3j-14 is a one-shot micro execution gate package, but it is still no-send. It
models the final gate before a separately reviewed real FINAM boundary
invocation.

The current scope intentionally uses the read-only token for gate evidence and
requires that a full trade token is not loaded. A full trade token is required
only for a later actual invocation package with explicit operator approval.

The gate requires:

- accepted M3j-13 no-send runbook;
- explicit operator approval artifact referenced;
- fresh immediate read-only evidence referenced;
- read-only token scope used for this gate;
- full trade token not loaded in this no-send package;
- one account, one symbol, one timeframe, one strategy;
- tiny quantity and max orders equal to one;
- market or limit order type only;
- active, unknown active, and orphan active orders equal to zero;
- flat or expected position;
- kill switch armed and tested;
- fresh one-shot TTL arm;
- begin-submit durable audit-before-boundary requirement;
- post-run reconciliation and EOD report requirements;
- redacted evidence only;
- scanner coverage for unguarded POST/DELETE/send.

M3j-14 keeps all execution boundaries closed:

- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `boundary_invocation_performed = false`;
- `real_finam_order_endpoint_used = false`;
- `command_consumer_to_real_finam_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`;
- `no_stop_sltp_bracket = true`;
- `stop_sltp_bracket_replace_multileg_allowed = false`.

If a real invocation, live runtime attach, continuous command consumer, full
trade token, non-loopback endpoint, or Stop/SLTP/bracket/replace/multi-leg
request is present, the gate is blocked.
