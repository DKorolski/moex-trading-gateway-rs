# M3b-18 real-readonly pre-evidence gate

Status: final pre-evidence safety gate for a future controlled FINAM
real-readonly probe. M3b-18 still does not authorize FINAM order
placement/cancel, real command consumption, real CommandAck lifecycle, runtime
attachment, `LiveReady`, live micro, stop/SLTP, or bracket behavior.

## Non-serializable token/account approval marker

The token/account preflight remains available as a redacted diagnostic for
operator reports, but it is no longer the capability object used by the
operator probe.

The capability boundary is now:

```text
FinamRealReadonlyTokenAccountPreflightApproved
```

That marker is constructed only from checked token details, account hash
matching, readonly token scope, and disabled order features. The operator run
accepts the marker, not a serializable diagnostic. This prevents a stored or
hand-edited JSON report shape from being confused with approval to send
real-readonly HTTP requests.

## Probe run identity

Each operator probe report now records:

```text
probe_run_started_at
probe_run_id
probe_run_fingerprint
```

The fingerprint is derived from the run start timestamp, approved account/base
URL hashes, timeout/rate settings, selected source list, and request cap. The
same fingerprint is copied into attempt records and evidence-matrix rows, so a
reviewer can correlate the summary, per-attempt evidence, and audit rows without
raw broker identifiers.

## Request/response/send counters

M3b-18 separates synthetic/captured evidence volume from actual HTTP transport
activity:

```text
requested_sources_count
attempt_count
captured_response_count
actual_http_send_started_count
actual_http_send_completed_count
actual_send_count
max_requests
```

`actual_send_count` remains as a backward-compatible alias for
`actual_http_send_started_count`. New reviews should prefer the explicit
started/completed counters.

For local/mock captured responses, actual HTTP send counters stay zero. For a
future controlled real-readonly probe, the counters show whether the transport
started a FINAM GET and whether an HTTP response boundary was reached.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
