# M4-3m active-session ALOR native 10m vs FINAM derived 10m parity

Status: active-session evidence tooling / no-live / no order endpoints.

M4-3m compares the current ALOR strategy-facing oracle bars with the FINAM
shadow bars that are derived from final WebSocket M1 data.

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

FINAM shadow input:

```text
finam_ws_shadow:market_data
schema_version = 2
msg_type       = MarketData
payload.Bar.timeframe_sec = 60
payload.Bar.is_final      = true
```

The FINAM M1 bars are aggregated using the same strict M1-to-M10 contract from
M4-3c4 and the provenance gate from M4-3l-a:

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
- FINAM M1 aggregation is final-only, contiguous, 600-second target;
- missing ALOR oracle stream is reported as pending, not as false success;
- no raw Redis payload is written into evidence;
- no live/order boundary is opened.

Runtime closure requires a reviewed active-session report with at least one
matched ALOR native 10m bar and FINAM derived 10m bar, with zero blocking diffs.
