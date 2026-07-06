# M4-3h-c controlled recovery acceptance evidence

M4-3h-c proves the FINAM WebSocket shadow loop can complete a controlled no-live
market-data recovery sequence.

The test intentionally starts the loop with a synthetic final-bar watermark that
is behind the current stream and bounds REST replay to an older tail. This
creates a real warm recovery window:

```text
older final watermark
→ REST Bars replay with overlap
→ replay contiguity/gap absence proof
→ first fresh WebSocket final bar at or after replay tail
→ recovery.phase = LiveReady
```

Recovery bars remain diagnostic only:

```text
recovery_replay.published_to_redis = false
recovery_replay.published_as_strategy_live = false
recovery.recovery_bars_publishable_as_strategy_live = false
```

## Required evidence

The generated evidence must show:

```text
recovery.phase = LiveReady
recovery.gap_absence_proven = true
recovery.blockers = []
recovery_replay.mode = Warm
recovery_replay.fetch_ok = true
recovery_replay.gap_detected_count = 0
recovery_replay.overlap_dedup_bar_count > 0
recovery_replay.recovery_not_strategy_live_bar_count > 0
first_live_final_at_or_after_replay_tail = true
```

## Boundary

Allowed:

```text
auth POST /v1/sessions
GET /v1/instruments/{symbol}/bars
FINAM WebSocket market data
Redis health/readiness/market-data writes for normal WS shadow output
```

Forbidden:

```text
order POST/DELETE
live orders
runtime-live attachment
command-consumer-to-real-FINAM
stop/SLTP/bracket
```

M4-3h-c is recovery acceptance only. It does not authorize runtime cutover.
