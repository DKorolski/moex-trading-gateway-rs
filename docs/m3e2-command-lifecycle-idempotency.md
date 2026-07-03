# M3e-2 command lifecycle idempotency

M3e-2 adds idempotency and durable command-lifecycle modeling after the accepted
M3e-1 command consumer skeleton.

Scope:

- persist typed commands as `CommandReceived` before any future endpoint path;
- enforce `request_id` idempotency;
- publish redacted duplicate ACKs for duplicate/replayed commands;
- persist expired commands as terminal local `ExpiredCommand` outcomes;
- keep invalid command payloads in redacted DLQ without command-state mutation;
- model ACK/DLQ publish-before-XACK discipline;
- prove publish failure blocks modeled XACK.

Persistence:

- `M3eInMemoryCommandLifecycleStore` is used for focused unit tests;
- `M3eJsonCommandLifecycleStore` is file-backed and used for restart/reopen
  tests.

Still forbidden in M3e-2:

- real Redis `XREADGROUP`, `XACK`, `XAUTOCLAIM`;
- endpoint transport invocation;
- external FINAM POST/DELETE;
- command consumer attachment to strategies/runtime/live;
- `LiveReady`;
- stop/SLTP/brackets, replace, or multi-leg order surfaces.

Evidence:

```bash
python3 scripts/m3e2_command_lifecycle_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

Required evidence booleans:

```text
m3e2_durable_command_lifecycle_ok = true
request_id_idempotency_ok = true
duplicate_request_no_second_endpoint_attempt = true
ack_publish_before_xack_ok = true
ack_publish_failure_blocks_xack = true
dlq_publish_failure_blocks_xack = true
endpoint_transport_invoked = false
external_order_endpoint_allowed = false
```
