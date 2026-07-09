# Stage 3 — market-data parity to strategy input level

Status: Stage 3A accepted; Stage 3B comparator foundation accepted after
Stage 3B-1 hardening; Stage 3C redacted report-generator accepted after
Stage 3C-1 duplicate bucket hardening; Stage 3D is next.

Date: 2026-07-09.

## Goal

Stage 3 must prove that the FINAM market-data contour can produce the same
strategy-facing closed 10-minute bar contract that the ALOR runtime/gateway
uses today.

The target path is:

```text
FINAM WS final M1 bars
  -> strict canonical M1-to-M10 aggregation
  -> strategy/model input candidate
  -> compared with ALOR native M10 oracle
```

The accepted ALOR oracle remains the strategy-facing native 10-minute stream
from `BarsGetAndSubscribe(tf=600)`.

## Stage 3A scope

Stage 3A is planning and evidence-schema only. It defines:

- accepted inputs;
- strategy-input bar provenance;
- timestamp and OHLCV comparison policy;
- reconnect/gap recovery expectations;
- redacted evidence artifact shape;
- acceptance gates for the later active-session report.

Stage 3A does not collect live evidence and does not attach the real strategy
runtime.

Review status: accepted after review of `6755998`.

## Inputs

### ALOR oracle

```text
md.bars.<portfolio>.10m
source_mode = AlorNativeBarsGetAndSubscribeTf600
timeframe_sec = 600
```

ALOR v1 bar field `payload.close_time_utc` is treated as the 10-minute bucket
open timestamp for the existing active 10-minute strategy stream:

```text
bucket_open_ts  = payload.close_time_utc
bucket_close_ts = payload.close_time_utc + 600s
```

The event is emitted after the closed bucket is available. Strategy parity is
judged on the closed bar contract, not on when Redis delivered the row.

### Optional ALOR assembled cross-check

If an isolated ALOR 1-minute stand exists, it may be assembled to M10 as an
internal diagnostic:

```text
source_mode = AlorStandDerivedM1ToM10
```

This is useful for validating assembly semantics, but it does not replace ALOR
native M10 as the oracle.

### FINAM candidate

FINAM strategy input candidate must be derived from final WebSocket M1 bars:

```text
source_mode = FinamDerivedM1ToM10
source_timeframe_sec = 60
target_timeframe_sec = 600
aggregation_complete = true
gap_absence_proven = true
```

Raw FINAM M1 bars are diagnostic/aggregation inputs only. They must never be
fed to strategy/model input as a model bar.

FINAM-native M10 remains blocked for strategy-facing input until separately
characterized and accepted.

## Strategy-input gate

A candidate bar may become strategy/model input only if all conditions hold:

- `is_final = true`;
- `target_timeframe_sec = 600`;
- `source_mode = FinamDerivedM1ToM10` or accepted ALOR oracle mode;
- source M1 bucket contains ten contiguous final M1 bars;
- no missing or conflicting source bars exist inside the bucket;
- duplicate source bars are idempotent if exact and blocking if conflicting;
- `gap_absence_proven = true`;
- session filter says the bucket belongs to a valid trading session;
- first fresh final live bar after restart/replay has been observed.

Rejected candidate bars must be diagnostic only and must not advance strategy
decision watermarks.

## Comparison policy

Stage 3 active-session evidence must compare overlapping ALOR native M10 and
FINAM derived M10 buckets on:

- instrument identity;
- session date;
- timeframe;
- finality;
- bucket open timestamp;
- bucket close timestamp;
- OHLCV.

The default policy is strict:

```text
timestamp_tolerance_sec = 0
price_tolerance = exact_decimal
volume_tolerance = exact_decimal
ohlcv_diff_policy = blocking_on_any_nonzero_diff
```

Any relaxation requires an explicit later review decision and must not be hidden
inside the comparator.

## Reconnect and gap recovery

After a disconnect/reconnect or silence interval:

1. load the last final strategy-bar watermark;
2. compute warm replay window with overlap;
3. fetch/replay missing final bars from read-only history;
4. dedupe overlap;
5. prove contiguity through the previous watermark and replay tail;
6. resubscribe to FINAM WS;
7. wait for the first fresh final live bar after replay;
8. only then allow candidate strategy bars.

If the gap cannot be proven closed:

- Entry must be blocked;
- Exit/Cancel/Repair must remain possible in later runtime stages where those
  paths exist;
- no runtime-live permission is implied by Stage 3.

## Session filtering

Stage 3 must distinguish:

- valid active trading buckets;
- expected weekend/holiday/closed-session silence;
- clearing/session breaks;
- unknown schedule state.

Unknown schedule is blocking. Expected closed/break intervals are not market
data failure by themselves, but they also do not produce fresh strategy bars.

## Evidence artifact

The future active-session report should write a redacted summary under:

```text
reports/parity/finam-vs-alor-m10/YYYY-MM-DD.json
```

Clean source handoff archives must not include generated reports. The schema is
documented in
[`stage-3/3a-market-data-parity-evidence-schema.md`](stage-3/3a-market-data-parity-evidence-schema.md).

Raw Redis payloads, secrets, account ids, broker tokens, and unbounded market
data dumps must not be exported.

## Stage 3 acceptance

Stage 3 cannot be accepted until a reviewed active-session evidence package
proves:

- at least one matched ALOR native M10 and FINAM derived M10 bucket;
- zero blocking OHLCV/timestamp/finality/timeframe/instrument diffs;
- raw M1 bars did not become strategy/model bars;
- duplicate source bars did not create duplicate model bars;
- reconnect recovery replayed or explicitly proved no gap before fresh live
  strategy bars resumed;
- stale backlog did not advance strategy input;
- weekend/session filtering matched the ALOR strategy-input contract;
- evidence is redacted and source-bound.

## Still forbidden

Stage 3 does not authorize:

- runtime-live;
- real FINAM command consumer;
- strategy-driven real FINAM orders;
- real FINAM `POST`/`DELETE` from runtime;
- Stop/SLTP/bracket/replace/multi-leg live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `i64` surrogate adapter;
- BO/MR trading logic changes.

## Next implementation slices

Recommended next slices after accepted Stage 3A:

1. Stage 3B — source-only comparator contract and synthetic fixture tests
   (accepted after Stage 3B-1 hardening).
2. Stage 3C — redacted report generator for ALOR native M10 vs FINAM derived
   M10 evidence (accepted after Stage 3C-1 duplicate bucket hardening).
3. Stage 3D — controlled active-session evidence collection (next).
4. Stage 3E — reconnect/gap-recovery evidence for strategy-input bars.
5. Stage 3 acceptance report.
