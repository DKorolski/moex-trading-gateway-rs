# M4-3c2 ALOR-style market-data lifecycle

Status: source-only / no-live / no order endpoints.

M4-3c2 brings the FINAM WebSocket market-data shadow closer to the mature ALOR
gateway semantics. The goal is not just to receive bars, but to make the data
lifecycle visible in the same operational language that ALOR already exposes:

```text
history / gap sync -> live subscription -> first final live bar -> data LiveReady
```

The broker gateway as a whole still must not publish continuous runtime
`LiveReady`, because operator live arm, command consumer to real FINAM, runtime
attachment, Stop/SLTP/bracket, and cutover remain blocked.

## ALOR parity source

The ALOR gateway uses `BarsGetAndSubscribe` with `from` and `skipHistory`.
That means its bootstrap/backfill data and live stream arrive through the WS
subscription, but the gateway still marks every bar with an origin:

- `History`;
- `HistoryGap`;
- `Live`.

ALOR readiness then waits for live bars plus broker-truth sync before entering
`GatewayPhase::LiveReady`.

FINAM keeps its own API shape, but follows the same neutral lifecycle:

- historical/read-only bars are bootstrap or diagnostics only;
- recovery bars are gap-sync only;
- strategy parity requires `MarketDataSourceKind::LiveStream`;
- strategy parity readiness requires a final live bar, not merely a quote or a
  forming bar.

## Added broker-neutral contract

M4-3c2 adds:

```text
BrokerMarketDataLifecycleInput
BrokerMarketDataLifecycleSnapshot
MarketDataLifecyclePhase
MarketDataLifecycleBlocker
evaluate_market_data_lifecycle()
```

Lifecycle phases:

```text
LoadingHistory
SyncingGap
LiveSubscribing
LiveReady
Degraded
```

Blockers:

```text
BarsSubscriptionDisabled
TransportDisconnected
NoLiveBarsObserved
NoFinalLiveBarsObserved
MarketDataStale
RestDataNotStrategySource
```

## FINAM WS shadow behavior

`finam-ws-shadow-loop` now records:

```text
history_bar_event_count
read_only_bar_event_count
recovery_bar_event_count
live_bar_event_count
final_live_bar_event_count
forming_live_bar_event_count
first_live_bar_seen
first_live_final_bar_seen
first_live_final_bar_close_ts
last_live_bar_close_ts
last_final_live_bar_close_ts
market_data_lifecycle.phase
market_data_lifecycle.blockers
```

The readiness rule is intentionally stricter than M4-3c1:

```text
no BARS subscription     -> Degraded / MarketDataNotLive
no live final bar yet    -> Degraded / FirstLiveBarMissing
live final bar observed  -> Reconciliation / OperatorLiveArmMissing
```

It still must not publish:

```text
ReadinessPhase::LiveReady
```

for the whole gateway, because continuous runtime live remains disabled.

## Boundary

M4-3c2 must not:

- send FINAM `POST /orders`;
- send FINAM `DELETE /orders/{id}`;
- enable command-consumer-to-real-FINAM;
- enable continuous runtime live;
- open or close a position;
- enable Stop/SLTP/bracket/replace/multi-leg;
- make cutover automatic.

## Acceptance

M4-3c2 is ready for review when:

- `BrokerMarketDataLifecycleSnapshot` is exported from `broker-core`;
- FINAM WS reports ALOR-style lifecycle diagnostics;
- forming live bars do not satisfy readiness;
- historical/read-only/recovery bars do not satisfy the first-live-final gate;
- final `LiveStream` bars move market-data lifecycle to `LiveReady`;
- broker readiness remains no-live / operator-arm blocked;
- forbidden-surface scanners remain green;
- no broker order endpoints are enabled or called.
