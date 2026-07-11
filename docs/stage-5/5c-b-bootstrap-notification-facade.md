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

The admission is consumed by value and is no longer `Clone`. The receipt is
also non-serializable and non-cloneable, and is intended to be consumed by the
next lifecycle gate.

Notification uses only `admission.bootstrap_snapshot()`. It does not reread
broker truth or reconstruct state from report summaries. Expiry is checked
again immediately before callback invocation; `now == expires_at` remains
valid and `now > expires_at` is blocked before state mutation.

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
`NotifyRuntimeStateRestored`, consuming the bootstrap receipt and preserving the
Stage 4G lifecycle order. It may not open warmup or semantic bars in the same
slice.
