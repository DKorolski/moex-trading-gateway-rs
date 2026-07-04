# M3j-12 runtime-live attachment dry gate

M3j-12 is a no-send attach rehearsal package. It checks whether the runtime and
command-consumer path can be rehearsed against the accepted M3j-11 authorization
state without enabling a real FINAM boundary call.

The package requires:

- accepted M3j-11 final authorization state;
- authorization state bound and not expired;
- one-shot TTL arm present, not expired, with no auto-rearm;
- kill switch armed and trip-tested;
- durable audit-before-boundary proof;
- dry runtime attach requested;
- dry command-consumer attach requested;
- loopback-only rehearsal scope;
- redacted dry attach evidence;
- post-run reconciliation and EOD report templates.

Even when the dry rehearsal is ready, M3j-12 keeps:

- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `boundary_invocation_performed = false`;
- `real_finam_order_endpoint_used = false`;
- `command_consumer_to_real_finam_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`;
- `stop_sltp_bracket_replace_multileg_allowed = false`.

The only positive rehearsal signal is `dry_runtime_attachment_rehearsed = true`.
It means the no-send dry gate is internally coherent; it does not mean live
attachment or order execution.
