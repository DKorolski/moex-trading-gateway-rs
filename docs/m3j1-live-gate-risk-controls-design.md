# M3j-1 live gate / operator-risk-controls design

M3j-1 closes the design part of the first-live-micro safety gate.

This is still a pre-live design package only. `live_micro_go` remains `false`.

M3j-1 does not enable:

- `LiveReady`;
- runtime live attachment;
- external FINAM POST/DELETE;
- command-consumer-to-real-FINAM transport;
- non-loopback order endpoint;
- Stop/SLTP/bracket/replace/multi-leg;
- RI/RTS first-live-micro scope.

## Operator arm design

The operator arm is a one-shot gate with explicit expiry and identity binding:

- one-shot arm;
- TTL required;
- expected account digest required;
- expected symbol digest required;
- expected config digest required;
- expected endpoint session digest required;
- no auto-rearm after restart;
- explicit disarm reasons.

The required disarm reasons are:

- TTL expired;
- one-shot consumed;
- restart observed;
- account mismatch;
- symbol mismatch;
- config digest mismatch;
- endpoint session digest mismatch;
- kill switch active;
- risk limit exceeded;
- unknown pending orders;
- readiness degraded;
- manual disarm.

## Kill switch design

The kill switch is a hard global order-emission block. Its design covers:

- runtime path;
- command consumer path;
- protected endpoint path;
- persistence across restart;
- redacted operator report.

## Risk limit design

The M3j-1 risk design requires:

- max orders per day;
- max orders per session;
- max quantity;
- max notional placeholder;
- max loss / stop-out placeholder;
- max unknown pending count = 0 for first live micro;
- no RI/RTS during the initial live-micro scope.

## Scope controls

First live micro is constrained to:

- one account allowlist entry;
- one symbol allowlist entry;
- one timeframe scope;
- one strategy scope;
- Market and Limit order types only.

The following remain disabled:

- Stop;
- SLTP;
- bracket;
- replace;
- multi-leg.

## Boundary interpretation

`m3j1_live_gate_design_ok = true` means only that the operator/risk/scope design
is internally complete for the next evidence stages.

It does not mean live readiness. The report still emits:

- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `external_finam_post_delete_allowed = false`;
- `command_consumer_to_real_finam_transport_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`.

## Next steps

M3j-2: fresh read-only FINAM evidence.

M3j-3: one-symbol dry shadow session report.

M3j-4: explicit pre-live NO-GO/GO decision package.
