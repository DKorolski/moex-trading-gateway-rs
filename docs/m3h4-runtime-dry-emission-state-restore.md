# M3h-4 runtime dry emission state restore

M3h-4 hardens the M3h-3 dry command emitter with a durable runtime-side
emission lifecycle store. This is still a dry/shadow boundary: no live runtime
attachment, no `LiveReady`, and no external FINAM order endpoint is enabled.

Lifecycle states:

- `PendingEmission`;
- `PublishedToM3eCommandStream`;
- `NotEmitted`.

Durable store behavior:

- lifecycle records are keyed by `StrategyRequestId`;
- `PendingEmission` is persisted before attempting to publish into the M3e
  command stream;
- `PublishedToM3eCommandStream` is persisted after publish success;
- `NotEmitted` is persisted for blocked/dropped/failed candidates;
- JSON-backed restore is available for M3h evidence and restart tests.

Restart policy:

- restart after `PublishedToM3eCommandStream` does not publish the same
  `request_id` again;
- restart after `PendingEmission` is conservative and does not publish again;
- restart after `NotEmitted` keeps rollback/dropped-intent state terminal for
  that `request_id`;
- retry/replay must use an explicit new `request_id`.

Rollback policy:

`NotEmitted` records are terminal runtime-side rollback markers for these
reasons:

- `NoStrategyDecisionTick`;
- `ReadinessNotDryReady`;
- `LiveReadyForbidden`;
- `UnsafeLiveBoundary`;
- `UnsupportedOrderShape`;
- `PublishFailed`.

Safety boundary:

- only M3e command stream output is allowed;
- no direct endpoint stream;
- no FINAM-specific stream;
- no runtime live attachment;
- no `LiveReady`;
- no external FINAM `POST / DELETE`;
- no command-consumer-to-real-FINAM transport;
- no stop/SLTP/bracket/replace/multi-leg.

Evidence:

```bash
python3 scripts/m3h4_runtime_dry_emission_state_restore_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
