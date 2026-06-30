# FINAM bar timestamp/finality golden-test plan

Current shadow mapping treats FINAM REST `bar.timestamp` as `open_ts`, derives
`close_ts = open_ts + timeframe`, and marks historical bars as final. This is a
safe read-only/shadow assumption, but it is not yet sufficient for runtime
execution semantics.

Before a runtime bridge consumes bar events, run and archive a redacted golden
test for each timeframe used by strategies.

M2h adds a read-only harness for collecting the first evidence bundle:

```bash
FINAM_SECRET_TOKEN=... FINAM_SYMBOL=TICKER@MIC \
  cargo run -p broker-cli -- finam-bar-finality-golden-check \
  --timeframe TIME_FRAME_M1 \
  --start-time 2026-06-29T09:00:00Z \
  --end-time 2026-06-29T10:00:00Z \
  --output tmp/finam-bar-finality-golden-redacted.json
```

The harness uses only auth and `bars_typed`; it does not call account, order,
cancel, or placement endpoints. Its output is redacted: it records presence
flags, counts, timestamp deltas, first/last bar timestamps, derived close/open
consistency, and `acceptance_status =
unproven_operator_review_required`.

## Required checks

1. Request a bounded historical window that contains only fully closed bars.
2. Verify whether `bar.timestamp` matches bar open or bar close by comparing:
   - requested `start_time`;
   - consecutive returned timestamps;
   - expected exchange session boundaries;
   - quote/latest-trade timestamps around the same period when available.
3. Request a window ending near the current wall-clock time and determine
   whether FINAM returns the still-forming current bar.
4. Repeat around a session gap or evening/session boundary.
5. Save only redacted shape/count/timestamp-convention fixtures, not account,
order, trade, or secret-bearing payloads.

M2k recorded the first redacted M1 evidence summary in
`docs/finam-bar-finality-evidence-2026-06-30.md`. The evidence supports
open-timestamp mapping for the checked windows and shows that exact
minute-aligned `end_time` may be inclusive. Runtime consumption remains blocked
until the finality/drop and durable dedupe/watermark policies are implemented.

## Acceptance rule

Runtime consumption is allowed only after the convention is documented as one of
these explicit policies:

- timestamp is open time and the last forming bar is not returned;
- timestamp is open time and the gateway drops the still-forming last bar;
- timestamp is close time and the mapper is changed accordingly;
- FINAM behavior is inconsistent, so runtime must consume a live stream instead
  of historical polling for execution bars.

Until this is proven, historical bars published by the shadow runner remain
read-only observability data and must not drive live order decisions.
