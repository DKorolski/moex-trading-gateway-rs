# M4-3m active-session ALOR native 10m vs FINAM derived 10m parity

Status: active-session evidence tooling / no-live / no order endpoints.

M4-3m compares the current ALOR strategy-facing oracle bars with the FINAM
shadow bars that are derived from final WebSocket M1 data. It also has an
optional ALOR M1-to-M10 cross-check mode for environments where both ALOR M1
and native ALOR 10m are available.

This stage is evidence-only. It does not authorize runtime-live, command
consumer to real FINAM, order placement, order cancel, Stop/SLTP/bracket,
replace, multi-leg, or cutover.

## Inputs

ALOR oracle input:

```text
md.bars.<portfolio>.10m
schema_version = 1
msg_type       = bar
payload.close_time_utc
payload.o/h/l/c/v
```

For active MOEX 10m streams, `payload.close_time_utc` is normalized as the
bucket timestamp/open. The event is emitted after that 10-minute bucket has
closed, so the canonical comparison interval is:

```text
open_ts  = payload.close_time_utc
close_ts = payload.close_time_utc + 600
```

Optional ALOR assembled input:

```text
md.bars.<portfolio>.1m
schema_version = 1
msg_type       = bar
```

When provided, M4-3m derives `AlorDerivedM1ToM10` and compares it to the native
ALOR 10m oracle. This is a diagnostic cross-check, not a replacement for the
strategy-facing native 10m stream.

FINAM shadow input:

```text
finam_ws_shadow:market_data
schema_version = 2
msg_type       = MarketData
payload.Bar.timeframe_sec = 60
payload.Bar.is_final      = true
```

The FINAM M1 bars are aggregated using the same strict M1-to-M10 contract from
M4-3c4 and the provenance gate from M4-3l-a. Exact duplicate M1 bars with the
same open timestamp and same OHLCV are deduped before aggregation; conflicting
duplicates are counted explicitly and the latest observed row wins for the
diagnostic report.

```text
bar_source_mode       = FinamDerivedM1ToM10
source_timeframe_sec  = 60
target_timeframe_sec  = 600
aggregation_complete  = true
gap_absence_proven    = true
```

## Comparator

For each matching 10-minute bucket, M4-3m compares:

- instrument/symbol identity;
- finality;
- timeframe;
- open timestamp;
- close timestamp;
- OHLCV.

Diffs are classified as:

- `MissingAlorBar`;
- `MissingFinamDerivedBar`;
- `TimestampMismatch`;
- `OhlcvMismatch`;
- `TimeframeMismatch`;
- `FinalityMismatch`;
- `InstrumentMismatch`;
- `SourceKindDiagnostic`.

If the ALOR native 10m stream is absent, the report must be explicit:

```text
runtime_status = Pending
pending_reason = MissingAlorOracleStream
```

That is not a failure of the code package; it means the active-session runtime
evidence cannot be closed until the oracle stream is available.

If ALOR M1 is not provided, native-vs-assembled ALOR evidence remains omitted;
that does not block ALOR-native-vs-FINAM-derived closure. If ALOR M1 is
provided but cannot produce complete 10m buckets, the report records
`NoCompleteAlorDerivedM10Bucket`.

## Boundary

M4-3m must not:

- call FINAM `POST /orders`;
- call FINAM `DELETE /orders/{id}`;
- enable command-consumer-to-real-FINAM;
- enable continuous runtime live;
- emit `LiveReady`;
- attach live strategy runtime;
- open or close a position;
- enable Stop/SLTP/bracket/replace/multi-leg;
- perform automatic cutover.

## Acceptance

Source/tooling acceptance requires:

- Redis reader handles ALOR v1 bar envelopes and FINAM v2 `MarketData::Bar`
  envelopes;
- ALOR v1 active 10m timestamps are normalized as bucket-open timestamps;
- FINAM M1 aggregation is final-only, contiguous, 600-second target;
- exact duplicate FINAM M1 bars are deduped before aggregation;
- missing ALOR oracle stream is reported as pending, not as false success;
- no raw Redis payload is written into evidence;
- no live/order boundary is opened.

Runtime closure requires a reviewed active-session report with at least one
matched ALOR native 10m bar and FINAM derived 10m bar, with zero blocking diffs.
