# M3j-9 operator-run pre-authorization evidence

M3j-9 closes the next pre-live review gap: it defines the evidence bundle that
must exist before any first tiny operator-run can be discussed.

This package is evidence-only. It does not authorize live trading and does not
perform a broker boundary invocation.

The bundle requires:

- accepted M3j-8 guarded candidate package;
- explicit operator GO artifact, redacted at export boundaries;
- timestamp, account, symbol, timeframe, strategy, config digest, and session
  digest binding;
- fresh and clean immediate FINAM read-only evidence;
- active, unknown, and orphan broker orders equal to zero;
- flat or explicitly expected position;
- one-shot TTL arm and no auto-rearm;
- kill switch tested and armed policy documented;
- max orders per session, max quantity, and micro notional/loss limit documented;
- durable audit-before-boundary proof;
- post-run broker-truth reconciliation and EOD report template;
- no raw secret, account, or broker payload export.

Even when the evidence bundle is complete, M3j-9 keeps:

- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `command_consumer_to_real_finam_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`.

Any live enablement input inside this evidence package is treated as unsafe and
blocks the bundle.
