# M3b-21 real-readonly operator clock gate

Status: final operator-clock/evidence-row coverage hardening before a future
controlled FINAM real-readonly evidence run. M3b-21-pre still does not authorize
FINAM order placement/cancel, real command consumption, real CommandAck
lifecycle, runtime attachment, `LiveReady`, live micro, stop/SLTP, or bracket
behavior.

## Explicit operator-run clock

Enabled operator-run probes now require an explicit operator-provided clock:

```text
probe_run_started_at: Some(...)
```

If an enabled run omits that value, the run blocks before any attempts:

```text
ProbeRunClockMissing
```

The report records:

```text
probe_run_started_at
probe_run_clock_source
```

`MissingFallbackToFetcherObserved` remains available only as a blocked-report
diagnostic fallback. Controlled enabled runs must use `OperatorProvided`.

## Computed preflight age

Operator reports now include:

```text
computed_preflight_age_ms
```

This is computed from:

```text
probe_run_started_at - preflight_checked_at
```

The value is exported as redacted timing evidence and lets reviewers validate
TTL behavior without recomputing it from timestamps.

## Per-row send flag coverage

M3b-21-pre adds transport-like fixture coverage where evidence matrix rows show:

```text
actual_http_send_started == true
actual_http_send_completed == true
```

and separately:

```text
actual_http_send_started == true
actual_http_send_completed == false
```

This proves the per-row flags and aggregate counters are wired together before
any real FINAM probe is run.

## Still not the actual evidence run

M3b-21-pre is still code/docs/test hardening only. The actual controlled
read-only FINAM evidence package must be produced as a separate artifact and
must remain GET-only, one-shot, no-retry, no-background-loop, and
`EphemeralEvidenceStore` only.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
