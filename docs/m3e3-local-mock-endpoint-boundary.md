# M3e-3 local-mock endpoint boundary

M3e-3 connects the durable command consumer path to the existing local-mock
classified endpoint boundary. It is the first endpoint-bound command slice, but
it is still not a live or external FINAM order path.

Scope:

- process typed place commands through command lifecycle + order-path store;
- persist `CommandReceived` before endpoint-bound work;
- run local preflight before endpoint attempt;
- persist order-path intent and `BeginSubmit` before local-mock transport;
- publish redacted ACK only after endpoint result integration;
- enforce `request_id` idempotency after local-mock endpoint execution;
- model ACK-before-XACK discipline;
- harden the crash window where ACK publish succeeded but final lifecycle update
  failed.

Crash-window policy:

```text
CommandReceived
→ endpoint-bound durable boundary
→ AckPublishPlanned persisted
→ redacted ACK publish
→ AckPublished persisted
→ modeled XACK
```

If the final `AckPublished` update fails after ACK publish, the source command is
not XACKed. Restart/replay sees `AckPublishPlanned` and emits an explicit
recovered/duplicate ACK without calling the endpoint again.

Still forbidden:

- real Redis `XREADGROUP`, `XACK`, `XAUTOCLAIM`;
- non-loopback endpoint calls;
- `api.finam.ru` order calls;
- real FINAM POST/DELETE;
- runtime/live attachment;
- `LiveReady`;
- stop/SLTP/brackets, replace, or multi-leg order surfaces.

Evidence:

```bash
python3 scripts/m3e3_local_mock_endpoint_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

Required evidence booleans:

```text
m3e3_local_mock_endpoint_boundary_ok = true
local_mock_endpoint_only = true
non_loopback_endpoint_allowed = false
duplicate_request_no_second_endpoint_attempt = true
command_received_persisted_before_endpoint = true
preflight_local_reject_before_endpoint = true
begin_submit_persisted_before_endpoint = true
ack_publish_planned_before_ack = true
ack_publish_before_xack_ok = true
ack_published_store_update_failure_recovery_ok = true
endpoint_attempt_count_incremented_only_after_durable_boundary = true
external_order_endpoint_allowed = false
```
