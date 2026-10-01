# P2 — short-history calendar / transport range alignment

Date: 2026-10-01. Status: SOURCE CORRECTION REVIEW CANDIDATE; no activation.
Baseline: `c8fe58b7b8b8945d248e69108f46e62d0c068330`, PR #11.
Finding: [review thread](https://github.com/DKorolski/moex-trading-gateway-rs/pull/11#discussion_r4148517519).

## Scope and reproduced defect

The accepted source, build fix and NRG01 test-harness acceptance stand. Required
CI run `36765599251` passed `rust` and `redis-smoke` for c8fe58b. Merge did not
occur: the new P2 thread blocks it. Those results are not fresh CI for this patch.
The owner explicitly authorized this narrow source correction on 2026-10-01.

Template/runtime source admission accepts four prior sessions of calendar age
1..=14. The old policy and collector capped elapsed fetch duration at 14*24h.
The first session's M1 begins before its first 07:10 MSK M10 close, and the
candidate day's data is also needed. For example, September 14 04:00 UTC through
September 28 20:50 UTC spans 14 days 16:50, although the session date age is 14.
The added exact CLI regression failed with exit 101 before the production fix:
template validation passed; policy validation rejected the interval.

## Narrow correction

The existing materializer type exposes one pure shared `validate_bars_interval`
method, used by both short policy validation and the collector before client
construction/GETs. The positive transport interval is capped at
`(SHORT_HISTORY_MAX_DAYS + 1) * 24h = 15 days`, inclusive. Age 14 spans up to 15
calendar dates including the current date. This finite transport envelope covers
their intraday endpoints; it does not add a strategy history session or authorize
an age-15 source. It is a ceiling, not a required lookback or automatic retry rule.
Actual artifact requests should cover only required reviewed session windows.

Unchanged:

- Four complete prior sessions, age 1..=14, plus exact closed current prefix.
- Runtime admission, profile/source-plan JSON bytes and their hashes.
- Missing M1/freshness rejection, canonical timestamps and disabled riskgate.
- Legacy policy minimum 180 / maximum 400 days; legacy collector positive
  interval with a 400-day maximum; chunk size and GET route inventory.
- Strategy, guardian, process supervision, Redis, installation and CI workflow.

Only the materializer library and CLI have Rust changes. Authority is refreshed
normally for these two files and status/roadmap documentation, without changing
closed flags, required checks or accepted replay references.

## Focused evidence and next boundary

Tests cover the actual age-14 template/policy combination, elapsed cap boundaries
(zero, reversed, 14 days, 14 days + 1 second, full intraday tail, exact 15 days,
15 days + 1 second), legacy limits, rejection before GETs, age 15 on weekdays,
and complete fixture materialization through the actual runtime source validator
with four prior sessions and empty riskgate observations. No live FINAM is used.
Retain the initial RED and final exact-tree gate separately; only actual final
results count. fmt, full canonical Clippy, gateway regression, affected runtime
source tests and authority + 45 negatives accompany the immutable review ZIP.

Push to the same PR for fresh CI. Independent narrow source acceptance and
required green checks precede one history-preserving merge. Then prepare the
new exact-baseline O2 artifact for review, not another recovery framework.
No VPS, operational Redis, FINAM writes, installation or O2 execution is
authorized here; all terminal history must remain intact.
