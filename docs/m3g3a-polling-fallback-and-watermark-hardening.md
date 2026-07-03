# M3g-3a polling fallback and snapshot/stream watermark hardening

M3g-3a is a small hardening patch after accepted M3g-3. It closes the
ambiguity around polling fallback and strengthens the snapshot/stream race
model before the broader M3g-4 readiness simulation package.

Polling fallback policy:

- policy is `AuxiliaryRequiresConnectedStream`;
- `PollingFallbackFresh` can satisfy broker-truth readiness only when the
  stream connection state is also `Connected`;
- if the stream is `Disconnected`, `Reconnecting`, or `Resubscribing`, polling
  evidence is reported but readiness remains blocked;
- this is the safer Option A from review: polling is auxiliary evidence, not a
  full substitute for a healthy own orders/trades stream.

Snapshot/stream watermark policy:

- `stream_subscribed_ts`;
- `snapshot_started_ts`;
- `snapshot_completed_ts`;
- `first_stream_event_ts`;
- `gap_absence_proven`.

Accepted watermark status is only `OrderedNoGap`. Any missing or inconsistent
watermark blocks readiness with `SnapshotStreamRace`.

Operator diagnostics:

- readiness reports now include `operator_blocker_summary`;
- the summary preserves blocker kind and count for operator-facing reporting;
- generic readiness reasons remain available for broker-neutral state machines.

Still forbidden:

- real FINAM `POST /orders`;
- real FINAM `DELETE /orders/{id}`;
- non-loopback order endpoint;
- command-consumer-to-real-FINAM transport;
- runtime live attachment;
- `LiveReady`;
- stop/SLTP/brackets/replace/multi-leg.

Evidence:

```bash
python3 scripts/m3g3a_polling_fallback_watermark_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
