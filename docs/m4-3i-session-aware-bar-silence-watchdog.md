# M4-3i session-aware bar silence watchdog

M4-3i adds a session-aware watchdog for FINAM WebSocket final bars.

The watchdog uses FINAM read-only schedule data to distinguish two very
different situations:

- active market session with missing/stale fresh final bars;
- closed/break/maintenance session where bar silence is expected.

This avoids treating legitimate session breaks as feed failures while still
blocking market-data readiness when the live final-bar stream is silent during
an open session.

## Report fields

`finam-ws-shadow-*` reports include:

```text
session_silence_watchdog.schema = m4_3i_session_aware_bar_silence_watchdog
session_silence_watchdog.schedule_fetch_ok
session_silence_watchdog.session_state
session_silence_watchdog.silence_threshold_sec
session_silence_watchdog.last_fresh_live_final_bar_close_ts
session_silence_watchdog.seconds_since_last_fresh_final
session_silence_watchdog.silence_alert
session_silence_watchdog.alert_reason
session_silence_watchdog.session_closed_no_silence_alert
```

## Acceptance rules

Inside an open session:

```text
last fresh final bar missing or older than threshold -> silence_alert = true
```

Outside an open session:

```text
Closed/Break/Maintenance -> silence_alert = false
```

If schedule cannot be fetched or parsed:

```text
session_state = Unknown
silence_alert = false
alert_reason = ScheduleFetchFailed | SessionUnknown
```

Unknown does not authorize live readiness; it is only a diagnostic no-false-alert
state for this watchdog.

## Boundary

Allowed:

```text
auth POST /v1/sessions
GET /v1/assets/{symbol}/schedule
FINAM WebSocket market data
FINAM REST Bars recovery GET when the WS shadow loop is used
Redis health/readiness/market-data writes for normal WS shadow output
```

Forbidden:

```text
order POST/DELETE
live orders
runtime-live attachment
command-consumer-to-real-FINAM
stop/SLTP/bracket
```

M4-3i is watchdog/readiness hardening only. It does not authorize runtime cutover.
