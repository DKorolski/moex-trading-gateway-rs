# Stage 5C-b — one-shot bootstrap-notification facade

Status: review candidate.

Date: 2026-07-11.

## Outcome

Stage 5C-b opens exactly one source callback:

```text
Stage5cPaperHostAdmission
  -> notification-time expiry check
  -> exact admitted RuntimeHostBootstrapSnapshot
  -> source on_bootstrap_snapshot
  -> Stage5cBootstrapNotificationReceipt
```

The Stage 4 evidence is consumed into admission and is no longer `Clone`.
Admission binds `strategy_id`. Bootstrap consumes both admission and the
concrete `HybridIntradayRuntimeStrategy` by value and returns
`Stage5cBootstrappedPaperStrategy`, which owns the same mutated strategy plus
its receipt. Neither the type-state nor receipt is serializable or cloneable.

Notification uses only `admission.bootstrap_snapshot()`. It does not reread
broker truth or reconstruct state from report summaries. Expiry is checked
again immediately before callback invocation; `now == expires_at` remains
valid and `now > expires_at` is blocked before state mutation.

Before callback invocation, the strategy instance is checked against admission:

- configured strategy symbol equals admission target symbol;
- configured strategy tick size equals the admitted tick size.

Binding failures occur before state mutation. The notification API no longer
accepts a free strategy ID.

## Mapping policy

- exact target/account identity is rechecked;
- target position quantity is mapped from the admitted aggregate quantity;
- the admitted snapshot timestamp becomes the source bootstrap timestamp;
- context is fixed to paper, live orders disabled and `SyncingHistory`;
- active target orders are fail-closed until an ownership/attribution-complete
  bootstrap mapping is accepted;
- stop-order bootstrap remains empty in this slice;
- source bootstrap must emit zero intents.

## Still closed

- runtime-state-restored notification;
- history warmup;
- pending-stream recovery;
- first semantic bar;
- paper intent sink;
- command consumer and all broker sends;
- runtime-live and real POST/DELETE;
- broker-side Stop/SLTP/bracket execution.

## Next gate

After acceptance, the next Stage 5C slice may add
`NotifyRuntimeStateRestored`, consuming `Stage5cBootstrappedPaperStrategy` and
returning the next type-state while preserving Stage 4G lifecycle order. It may
not open warmup or semantic bars in the same slice.
