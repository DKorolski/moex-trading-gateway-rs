# M3g-3 own orders/trades stream or polling-equivalent readiness

M3g-3 closes the next streams/readiness slice after the M3g-2 first-live-bar
gate. It is still a readiness/evidence layer only: it does not enable real
FINAM order endpoint calls, runtime live attachment, `LiveReady`, or strategy
execution.

Accepted broker-truth inputs:

- own orders via `StreamFresh`;
- own orders via `PollingFallbackFresh`;
- own trades via `StreamFresh`;
- own trades via `PollingFallbackFresh`;
- fresh positions snapshot.

Polling fallback is accepted only with an explicit SLA:

- max allowed age;
- last successful polling cycle timestamp;
- failure count;
- max allowed failure count.

Stream freshness is accepted only when:

- stream last-seen timestamp is present and fresh;
- connection state is `Connected`;
- reconnect/resubscribe is not in progress;
- snapshot/stream race is not detected.

Snapshot/stream race policy:

- if the first stream event timestamp is older than the startup snapshot
  timestamp, readiness is blocked with `SnapshotStreamRace`;
- the safe recovery path is to refresh broker truth and resubscribe/reconcile
  before any future live arm can be considered.

Restart policy for the first-live-bar gate:

- after gateway restart, the gate resets;
- a new `LiveStream` final bar is required;
- persisted first-live-bar state is not trusted unless a later stage proves
  freshness/session safety separately.

Readiness behavior:

- stale/missing orders block readiness;
- stale/missing trades block readiness;
- stale/missing positions block readiness;
- reconnect/resubscribe/disconnected stream state blocks readiness;
- snapshot/stream race blocks readiness;
- accepted own orders/trades/positions plus accepted first-live-bar still does
  not produce `LiveReady`;
- readiness remains `Reconciliation` with `OperatorLiveArmMissing` until later
  explicit gates.

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
python3 scripts/m3g3_broker_truth_stream_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
