# Stage 5C-c — runtime-state restore facade

Status: review candidate.

Date: 2026-07-11.

## Outcome

The slice opens exactly one additional source lifecycle callback:

```text
Stage5cBootstrappedPaperStrategy
  -> validate persisted-state envelope
  -> set_state
  -> on_runtime_state_restored
  -> Stage5cRuntimeStateRestoredPaperStrategy
```

Both type states own the same concrete strategy instance. Profile, high180,
riskgate and all other runtime configuration therefore remain bound by Rust
ownership; callers cannot substitute another configured strategy between
bootstrap, restore and the future warmup gate.

Validation happens before `set_state` and checks schema, strategy, account,
instrument and tick-size identity. Only `HybridIntradayRuntime` state is
accepted. Its persisted position must equal the exact broker-truth position
already admitted and notified at bootstrap, so persisted state cannot overwrite
broker truth.

Broker order IDs remain `BrokerOrderId(String)`. Legacy numeric ALOR IDs require
the explicit `convert_positive_alor_numeric` policy; the `reject` policy fails
closed before deserialization. Non-positive legacy values remain invalid under
the broker-core ID contract.

The callback context is fixed to paper mode, live orders disabled and
`SyncingHistory`. The source restore callback must emit no intents.

## Still closed

- history warmup;
- pending-stream recovery;
- semantic bars and timers;
- paper intent sink;
- command consumer and all broker sends;
- runtime-live and real POST/DELETE;
- broker-side Stop/SLTP/bracket execution.

