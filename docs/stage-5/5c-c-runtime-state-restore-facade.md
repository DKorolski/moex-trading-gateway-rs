# Stage 5C-c — runtime-state restore facade

Status: review candidate.

Date: 2026-07-11.

## Outcome

The slice opens exactly one additional source lifecycle callback:

```text
strategy + Stage5cPaperHostAdmission
  -> validate provenance envelope
  -> set_state (persisted state)
  -> Stage5cRuntimeStateLoadedPaperStrategy
  -> on_bootstrap_snapshot (exact broker truth)
  -> Stage5cBootstrappedPaperStrategy
  -> on_runtime_state_restored
  -> Stage5cRuntimeStateRestoredPaperStrategy
```

Both type states own the same concrete strategy instance. Profile, high180,
riskgate and all other runtime configuration therefore remain bound by Rust
ownership; callers cannot substitute another configured strategy between
bootstrap, restore and the future warmup gate.

Validation happens before `set_state` and checks envelope/state schema,
strategy kind/identity, account, instrument, tick size, persisted timestamp,
canonical configuration fingerprint and explicit profile/MR/riskgate binding.
Only `HybridIntradayRuntime` state is accepted. Quantity and side must be
consistent with the admitted broker position. Exact broker truth is notified
after persisted state, matching the ALOR lifecycle; postconditions ensure stale
persisted TP/SL IDs cannot survive an empty broker snapshot.

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
