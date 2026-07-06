# M4-3n ALOR native 10m vs ALOR assembled 1m-to-10m stand evidence

Status: diagnostic evidence tooling / no strategy runtime / no order endpoints.

M4-3n closes the ALOR-internal timeframe question separately from FINAM parity:
given the production ALOR native 10m stream and a separate diagnostic ALOR
gateway stand publishing 1m bars, does strict M1-to-M10 assembly reproduce the
native 10m bars?

This stage is evidence-only. It must not attach strategy runtime, consume or
emit order commands, place/cancel orders, enable Stop/SLTP/bracket, or change
the production contour.

## Stand topology

The intended VPS diagnostic stand is isolated from the production hybrid stack:

```text
/opt/trading-stand/alor-imoexf-1m
docker compose project: trading-hybrid-1m-stand
services: redis + alor-gateway only
runtime: absent
gateway tf_sec: 60
stand stream: md.bars.<portfolio>.1m
```

Production oracle input:

```text
container: trading-hybrid-redis-1
stream:    md.bars.<portfolio>.10m
```

Stand assembled input:

```text
container: trading-hybrid-1m-stand-redis-1
stream:    md.bars.<portfolio>.1m
```

## Timestamp and aggregation contract

ALOR v1 bar envelopes keep the historical field name `close_time_utc`. For the
active MOEX streams used here, it is normalized as the bucket timestamp/open:

```text
open_ts  = payload.close_time_utc
close_ts = payload.close_time_utc + timeframe_sec
```

The stand M1 stream is assembled to M10 only when all ten contiguous final 1m
bars are present. Exact duplicate M1 rows are deduped; conflicting duplicates
are counted explicitly.

## Comparator

M4-3n compares overlapping native and assembled M10 buckets on:

- symbol identity;
- finality;
- timeframe;
- open timestamp;
- close timestamp;
- OHLCV.

Runtime closure requires:

```text
runtime_status = Closed
comparison.status = Synchronized
blocking_issue_count = 0
stand_command_safety.orders_len = 0
stand_command_safety.acks_len = 0 or missing
```

## Boundary

M4-3n must not:

- start strategy runtime in the stand;
- write to production Redis;
- call FINAM or ALOR order endpoints;
- send/cancel/replace orders;
- enable continuous FINAM runtime-live;
- emit `LiveReady` for FINAM;
- perform cutover.

