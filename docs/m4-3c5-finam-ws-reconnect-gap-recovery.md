# M4-3c5 FINAM WS reconnect/gap-recovery parity contract

M4-3c5 closes the design gap between a simple FINAM WebSocket reconnect loop and
the mature ALOR gateway recovery semantics.

This is source-only hardening. It does not call FINAM, ALOR, Redis, SSH, or any
broker order endpoint. It does not enable runtime-live, command consumption,
POST/DELETE order routes, Stop/SLTP, bracket, or continuous trading.

## Problem

The current FINAM WS shadow loop can reconnect and resubscribe, but reconnect is not the same thing as data recovery.

After a network break the gateway must prove that no final strategy bar was
missed before it can be treated as a live-ready market-data source. ALOR already
has the operational ingredients for this:

- connection generation;
- resubscribe/reconnect commands;
- a backfill plan with cold/warm modes;
- `from` + `skipHistory` subscription semantics;
- `History`, `HistoryGap`, and `Live` origins;
- subscription ACK tracking;
- inactive/unknown GUID filtering;
- data-quality counters and duplicate/old-bar rejection.

FINAM WebSocket subscriptions are different. The FINAM WS `BARS` subscription
shape used by this project carries `symbol` and `timeframe`, not a historical
`from` replay window. Therefore the FINAM recovery path must combine:

1. REST `Bars` replay for the gap window;
2. WebSocket resubscribe for live data;
3. a broker-neutral watermark proof before readiness advances.

## Contract

M4-3c5 adds broker-neutral recovery primitives in `broker-core`:

- `MarketDataRecoveryPlanInput`;
- `MarketDataRecoveryPlan`;
- `MarketDataRecoveryInput`;
- `MarketDataRecoveryReport`;
- `MarketDataRecoveryMode`;
- `MarketDataRecoveryPhase`;
- `MarketDataRecoveryBlocker`;
- `plan_market_data_recovery()`;
- `evaluate_market_data_recovery()`.

The plan step computes the replay window:

- no prior final watermark -> `Cold` recovery from configured history start;
- prior final watermark -> `Warm` recovery from `last_final_bar - overlap`;
- zero timeframe is explicitly invalid and cannot silently produce LiveReady.

The evaluation step allows `LiveReady` only when all of these are true:

- replay window covers the previous final-bar watermark;
- warm replay is present;
- replay is contiguous through at least one bar after the previous watermark;
- no replay gap was detected;
- transport is connected;
- live subscription was sent and confirmed;
- a first live final bar exists;
- first live final bar is not older than the replay tail.

Otherwise the report remains `LoadingHistory`, `SyncingGap`, `LiveSubscribing`,
or `Degraded` with explicit blockers.

## Future FINAM wiring

The next implementation step should wire this source-only contract into
`finam-ws-shadow-loop`:

1. persist last emitted final bar per `(symbol, timeframe)`;
2. on reconnect create `MarketDataRecoveryPlan`;
3. fetch FINAM REST Bars for `replay_from_ts..checked_ts`;
4. pass replay bars through the same finalizer/aggregator path;
5. dedupe overlap and detect non-contiguous gaps;
6. connect WebSocket and subscribe;
7. wait for first fresh live final bar;
8. publish recovery report fields alongside market-data lifecycle/readiness.

Runtime attachment must continue to be blocked until recovery report is clean.

## Readiness impact

During reconnect/recovery, gateway readiness must stay degraded or
reconciliation-only. A new WS connection alone is insufficient.

Expected blockers:

- `ReplayWindowMissing`;
- `ReplayWindowDoesNotCoverWatermark`;
- `ReplayMissing`;
- `ReplayNotContiguousToWatermark`;
- `ReplayGapDetected`;
- `TransportDisconnected`;
- `LiveSubscriptionMissing`;
- `FirstLiveFinalBarMissing`;
- `FirstLiveFinalBeforeReplayEnd`;
- `GapAbsenceNotProven`.

## Boundary

Still forbidden:

- real FINAM order POST/DELETE;
- command-consumer-to-real-FINAM;
- continuous runtime live;
- Stop/SLTP/bracket;
- RI/RTS scale-up;
- market position tests.

This step is only a source-level recovery parity contract.
