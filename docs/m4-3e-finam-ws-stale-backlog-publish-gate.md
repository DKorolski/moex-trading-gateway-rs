# M4-3e FINAM WS stale backlog publish gate

M4-3e turns the M4-3d freshness diagnostics into a safer publication policy.

FINAM WS can deliver final bars through the WebSocket transport that are old
relative to the receive time. Those bars are useful diagnostics/backlog evidence,
but they must not be published as strategy-ready market-data bars.

This step keeps the trading boundary closed:

- no live orders;
- no FINAM order POST/DELETE;
- no command-consumer-to-real-FINAM;
- no runtime-live attachment;
- no Stop/SLTP/bracket.

## Policy

The FINAM WS shadow loop still:

- decodes WS `BARS`;
- counts all inbound/final/live bars;
- runs the closed-bar finalizer;
- records stale/fresh/gap diagnostics;
- publishes quotes as diagnostic market data.

But it publishes a final bar to the strategy market-data stream only if:

```text
close_ts <= observed_ts
and observed_ts - close_ts <= max(3 * timeframe_sec, 60)
```

For M1, this threshold is 180 seconds.

Stale WS final bars are suppressed from strategy publication and counted in:

- `stale_live_final_bar_count`;
- `stale_ws_final_bar_suppressed_count`.

Fresh published strategy bars are counted in:

- `published_strategy_bar_count`.

## Why this matters

Before this gate, a local IMOEXF run showed 300 final M1 bars arriving through
FINAM WS, but all were stale backlog ending around 19:00 MSK. Readiness stayed
degraded, but the bars were still written to the market-data stream with
`source_kind = LiveStream`.

After this gate, the same shape remains visible in stdout diagnostics, but stale
backlog bars do not enter the strategy market-data stream.

## Future recovery path

M4-3e does not replace the M4-3c5 recovery contract. The future reconnect/gap
implementation still needs:

1. REST Bars replay for the recovery window;
2. overlap dedupe;
3. gap absence proof;
4. WS resubscribe;
5. first fresh live final bar proof;
6. active-session evidence.
