# FINAM bar timestamp/finality golden-test plan

Current shadow mapping treats FINAM REST `bar.timestamp` as `open_ts`, derives
`close_ts = open_ts + timeframe`, and marks historical bars as final. This is a
safe read-only/shadow assumption, but it is not yet sufficient for runtime
execution semantics.

Before a runtime bridge consumes bar events, run and archive a redacted golden
test for each timeframe used by strategies.

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
