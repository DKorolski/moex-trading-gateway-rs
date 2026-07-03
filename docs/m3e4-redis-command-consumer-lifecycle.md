# M3e-4 Redis command consumer lifecycle

M3e-4 moves the accepted M3e-3/M3e-3a place+cancel local-mock endpoint boundary
onto a real Redis consumer-group lifecycle.

Scope:

- consume synthetic command entries with real `XREADGROUP`;
- publish redacted ACK or DLQ through the existing Redis stream sink;
- apply real `XACK` only after ACK or DLQ publication succeeds;
- leave entries pending when ACK or DLQ publication fails;
- recover pending entries with real `XAUTOCLAIM`;
- prove duplicate/replayed `request_id` commands do not create a second
  endpoint attempt;
- cover both `PlaceOrder` and `CancelOrder`;
- keep endpoint execution local-mock-only.

Recovery policy:

- replay after `CommandReceived` is conservative and does not reach endpoint;
- replay after endpoint attempt / `AckPublishPlanned` publishes a recovery-style
  duplicate ACK and does not blindly retry endpoint;
- replay after ACK publish but before real Redis `XACK` publishes an explicit
  duplicate ACK and does not retry endpoint.

Still forbidden:

- non-loopback endpoint calls;
- `api.finam.ru` order calls;
- real FINAM POST/DELETE;
- runtime/live attachment;
- `LiveReady`;
- stop/SLTP/brackets, replace, or multi-leg order surfaces.

Smoke:

```bash
bash scripts/m3e_command_consumer_redis_smoke.sh
```

Evidence:

```bash
python3 scripts/m3e4_redis_command_consumer_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

Required evidence booleans:

```text
m3e4_redis_consumer_lifecycle_ok = true
xreadgroup_consume_ok = true
xack_after_ack_or_dlq_publish_ok = true
xautoclaim_recovery_ok = true
pending_replay_no_second_endpoint_attempt = true
place_and_cancel_redis_lifecycle_ok = true
external_order_endpoint_allowed = false
local_mock_endpoint_only = true
```
