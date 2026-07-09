# Stage 3B — source-only market-data parity comparator contract

Status: implemented for review.

Date: 2026-07-09.

## Purpose

Stage 3B adds a source-only contract for comparing the existing ALOR
strategy-facing M10 oracle with FINAM-derived canonical M10 bars:

```text
ALOR native BarsGetAndSubscribe(tf=600)
  vs
FINAM final M1 bars -> strict M1-to-M10 aggregation
```

This patch is intentionally synthetic-fixture backed. It does not connect Redis,
runtime-live, real FINAM command consumers, or order execution.

## Contract

The accepted strategy-input candidate must satisfy:

- source mode is `FinamDerivedM1ToM10`;
- source timeframe is 60 seconds;
- target timeframe is 600 seconds;
- exactly ten contiguous final M1 bars create one final M10 bar;
- exact duplicate M1 bars are idempotent;
- conflicting duplicate M1 bars are blocking;
- missing M1 buckets are incomplete/blocking;
- FINAM native M10 is rejected until separately characterized;
- raw FINAM M1 can be aggregation input only and must not advance a strategy
  watermark.

The ALOR oracle normalization keeps the existing runtime convention:

```text
bucket_open_ts  = payload.close_time_utc
bucket_close_ts = payload.close_time_utc + 600s
```

## Comparator policy

The comparator is strict by default:

- timestamp tolerance: zero;
- price tolerance: exact decimal;
- volume tolerance: exact decimal;
- missing bar policy: blocking;
- finality policy: final-only;
- instrument/timeframe/timestamp/OHLCV mismatch: `BlockedDiff`.

Diagnostic source-kind differences may be counted separately, but they do not
relax blocking business parity rules.

## Evidence fields introduced before active session

The Stage 3 report contract now includes explicit publication counters:

```text
raw_m1_published_as_model_bar_count
finam_derived_m10_published_as_model_bar_count
alor_native_m10_oracle_bars_seen
candidate_bars_rejected_before_strategy_count
```

It also distinguishes reconnect/gap recovery states:

```text
recovery_required
recovery_status = NotRequired | NotAttempted | AttemptedAndComplete | AttemptedAndFailed
```

This avoids conflating “no recovery was needed” with “recovery was needed but
not attempted.”

## Synthetic acceptance coverage

Stage 3B source tests cover:

- synchronized ALOR native M10 vs FINAM-derived M10 passes;
- ALOR `close_time_utc` is treated as `bucket_open_ts`;
- ten contiguous final FINAM M1 bars assemble exactly one M10;
- missing M1 bucket is incomplete/blocking;
- exact duplicate M1 is idempotent;
- conflicting duplicate M1 is blocking;
- raw M1 cannot advance the strategy watermark;
- FINAM native M10 is rejected/blocked;
- OHLCV mismatch produces `BlockedDiff`;
- timestamp mismatch produces `BlockedDiff`;
- raw payloads are not exported;
- the safety boundary remains closed.

## Still forbidden

Stage 3B does not authorize:

- runtime-live;
- real FINAM command consumer;
- strategy-driven real FINAM orders;
- real FINAM `POST`/`DELETE` from runtime;
- Stop/SLTP/bracket/replace live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `i64` surrogate adapter;
- BO/MR trading logic changes.

## Next expected patch

Stage 3C should add a redacted report generator around the accepted comparator
shape. It should still remain source/report-only until active-session evidence
collection is explicitly authorized and reviewed.
