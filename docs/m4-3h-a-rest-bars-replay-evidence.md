# M4-3h-a REST Bars replay evidence

M4-3h-a is a controlled GET-only evidence slice for the FINAM warm/cold resync plan.

It does not enable runtime-live, order placement, cancel, command consumer, stop/SLTP/bracket, or continuous trading.

## Purpose

The evidence proves that a warm replay window can be built from the latest Redis final-bar watermark and fetched through FINAM REST Bars:

```text
watermark = latest final M1 LiveStream bar in finam:market-data
replay_from_close_ts = watermark - overlap_bars * timeframe
rest_query_start_open_ts = replay_from_close_ts - timeframe
rest_query_end_ts = now
```

FINAM REST bar timestamps are treated as bar open timestamps. Canonical replay close timestamp is:

```text
close_ts = rest_bar.timestamp + timeframe
```

## Required evidence

The script checks:

```text
auth_http = 200
REST Bars GET succeeds
bars_count > 0
mapped close timestamps are monotonic
replay first close <= watermark
replay last close >= watermark
replay has at least one bar after watermark
no timestamp gaps inside returned sequence
overlap_dedup_bar_count >= 1
gap_absence_proven = true
```

The default HTTP timeout is intentionally 60 seconds because FINAM REST Bars can be slow during active market periods.

## Boundary

Allowed in this stage:

```text
FINAM auth POST /v1/sessions
FINAM GET /v1/instruments/{symbol}/bars
Redis read of market-data watermark
```

Still forbidden:

```text
order POST/DELETE
live orders
runtime-live attachment
command-consumer-to-real-FINAM
stop/SLTP/bracket
```

M4-3h-a is replay evidence only. It does not publish recovery bars into the strategy stream.
