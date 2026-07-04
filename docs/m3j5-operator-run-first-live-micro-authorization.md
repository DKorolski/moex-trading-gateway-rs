# M3j-5 operator-run first-live-micro authorization package

M3j-5 is a separate operator-run authorization package after the accepted
M3j-4 decision package.

Current source/evidence posture is still:

```text
authorization = NotAuthorized
live_micro_go = false
```

M3j-5 does not implement a real FINAM order path and does not enable:

- `LiveReady`;
- runtime live attachment;
- external FINAM POST/DELETE;
- command-consumer-to-real-FINAM transport;
- non-loopback order endpoint;
- Stop/SLTP/bracket/replace/multi-leg.

## Required authorization inputs

Before first live micro can become an operator-run candidate, M3j-5 requires:

- accepted M3j-4 NO-GO decision package as baseline;
- explicit operator GO artifact;
- operator GO timestamp;
- account digest;
- symbol digest;
- timeframe;
- strategy;
- config digest;
- endpoint session digest;
- fresh immediate pre-run read-only evidence;
- active orders = 0;
- unknown active orders = 0;
- orphan active orders = 0;
- flat or expected position;
- one-shot TTL operator arm;
- no auto-rearm after restart;
- persistent kill switch tested before run;
- max orders per session;
- max quantity;
- max notional / loss placeholder;
- one account / one symbol / one timeframe / one strategy;
- post-run broker-truth reconciliation requirement;
- EOD report requirement;
- separate reviewed tiny live order path diff before any real transport enablement.

## Boundary interpretation

`OperatorRunCandidate` is not the same as live enablement. It only means the
operator-run authorization package is internally complete.

Both `NotAuthorized` and `OperatorRunCandidate` keep:

- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `external_finam_post_delete_allowed = false`;
- `command_consumer_to_real_finam_transport_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`.

Any actual live order path must be a separate reviewed change with scanners,
route gates, audit, redaction, rollback, and post-run reconciliation evidence.
