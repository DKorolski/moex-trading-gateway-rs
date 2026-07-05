# M4-3f FINAM WS data-quality ledger

M4-3f ports the ALOR gateway data-quality accounting pattern into the FINAM WS
shadow report.

This step is diagnostic/source hardening only:

- no live orders;
- no FINAM order POST/DELETE;
- no command-consumer-to-real-FINAM;
- no runtime-live attachment;
- no Stop/SLTP/bracket.

## ALOR oracle

ALOR tracks bar quality with the invariant:

```text
received = emitted + dropped + ignored + pending
```

and reports imbalances explicitly. This made it easy to see whether bars were
lost, emitted, suppressed, ignored, or still buffered.

## FINAM WS ledger

The FINAM WS shadow report now includes a `data_quality` section with the same
core invariant for bars and quotes.

For bars:

- `received` = final bar events + forming bar events;
- `emitted` = fresh final strategy bars published to market-data stream;
- `dropped` = stale WS final bars suppressed + duplicate final suppressed +
  non-monotonic forming dropped;
- `ignored` = unknown-source bar events;
- `pending` = buffered forming bars not yet finalized.

For quotes:

- `received` = quote events;
- `emitted` = quote events published as diagnostic market data;
- `dropped/ignored/pending` = 0 for the current shadow path.

If the equation does not balance, the report exposes an `imbalances` array.

## Expected local stale-backlog shape

For the observed IMOEXF backlog run:

```json
{
  "bars": {
    "received": 300,
    "emitted": 0,
    "dropped": 300,
    "ignored": 0,
    "pending": 0,
    "balanced": true
  }
}
```

That means the gateway saw 300 final WS bars, classified all as stale backlog,
and correctly kept them out of the strategy market-data stream.

## Future work

The next ALOR parity pieces are:

1. WS generation and stale-generation ignoring;
2. subscription confirmation/pending stats;
3. warm/cold REST replay implementation;
4. bar silence watchdog with session awareness;
5. HTTP/debug health surface.
