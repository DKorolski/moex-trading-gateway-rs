# M3e-3a cancel local-mock endpoint boundary

M3e-3a extends the accepted M3e-3 place-only local-mock endpoint slice to
`CancelOrder`.

Scope:

- decode `CancelOrder` from `Envelope<BrokerCommand>`;
- persist `CommandReceived` before cancel endpoint-bound work;
- load the existing broker-order mapping from order-path store;
- run cancel preflight/local validation before endpoint attempt;
- persist `RequestCancel` before local-mock classified cancel endpoint call;
- publish redacted ACK only after local endpoint result integration;
- enforce `request_id` idempotency for cancel commands;
- preserve the M3e-3 `AckPublishPlanned -> ACK -> AckPublished -> modeled XACK`
  discipline.

Cancel policy:

- accepted cancel ACK is not terminal canceled; it is a submitted cancel state;
- `404/409/410` map to reconciliation/manual policy, not blind success;
- duplicate cancel request does not create a second endpoint attempt;
- preflight rejection happens before endpoint attempt.

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
python3 scripts/m3e3a_cancel_local_mock_endpoint_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

Required evidence booleans:

```text
m3e3a_cancel_local_mock_endpoint_boundary_ok = true
cancel_command_received_persisted_before_endpoint = true
cancel_preflight_local_reject_before_endpoint = true
request_cancel_persisted_before_endpoint = true
cancel_accepted_not_terminal = true
cancel_404_409_410_reconciliation_not_success = true
duplicate_cancel_request_no_second_endpoint_attempt = true
local_mock_endpoint_only = true
non_loopback_endpoint_allowed = false
external_order_endpoint_allowed = false
```
