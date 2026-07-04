# M3j-15 actual invocation preflight

M3j-15 is an actual one-shot invocation preflight package. It may require a full
trade-token scope to be present, but it still does not perform a FINAM boundary
invocation.

This package is the final no-send check before a separately reviewed actual
one-shot micro invocation package.

The preflight requires:

- accepted M3j-14 one-shot micro gate;
- current explicit operator approval;
- full trade-token scope present, redacted, and not exported;
- current and clean immediate read-only broker-truth refresh;
- active, unknown active, and orphan active orders equal to zero;
- flat or expected position;
- one account, one symbol, one timeframe, one strategy;
- config and session digest binding;
- tiny quantity, max orders equal to one, market or limit only;
- kill switch armed and tested;
- fresh one-shot TTL arm with no auto-rearm;
- begin-submit durable audit persisted before boundary;
- post-run reconciliation and EOD report requirements;
- scanner coverage and redacted evidence only.

M3j-15 keeps the actual execution boundary closed:

- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `boundary_invocation_performed = false`;
- `real_finam_order_endpoint_used = false`;
- `command_consumer_to_real_finam_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`;
- `no_stop_sltp_bracket = true`;
- `stop_sltp_bracket_replace_multileg_allowed = false`.

Any request for actual boundary invocation, runtime live attach, continuous
command consumer, non-loopback endpoint, or Stop/SLTP/bracket/replace/multi-leg
blocks this preflight package.
