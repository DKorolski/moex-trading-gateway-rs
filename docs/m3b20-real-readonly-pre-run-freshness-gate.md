# M3b-20 real-readonly pre-run freshness gate

Status: final freshness/evidence-shape hardening before a future controlled
FINAM real-readonly evidence run. M3b-20-pre still does not authorize FINAM
order placement/cancel, real command consumption, real CommandAck lifecycle,
runtime attachment, `LiveReady`, live micro, stop/SLTP, or bracket behavior.

## Preflight freshness / TTL

`FinamRealReadonlyTokenAccountPreflightApproved` now carries explicit freshness
metadata through its redacted diagnostic:

```text
preflight_checked_at
preflight_max_age_ms
```

The operator run blocks before any attempt if the marker is stale at
`probe_run_started_at`:

```text
TokenAccountPreflightExpired
```

This prevents a request-bound marker from being reused after token/account
permissions could have changed. The marker must be created in the same
controlled flow or with a bounded max age.

## Per-row actual-send evidence

Evidence matrix rows now include per-source transport boundary flags:

```text
actual_http_send_started
actual_http_send_completed
```

The aggregate report counters remain:

```text
actual_http_send_started_count
actual_http_send_completed_count
actual_send_count
```

`actual_send_count` remains a compatibility alias for started sends. Reviewers
can now reconcile both aggregate counts and each individual GET-only source row.

## Still not the actual evidence run

M3b-20-pre is a code/docs safety gate, not a real FINAM probe execution. The
actual controlled read-only evidence package must be a separate artifact with:

```text
probe_run_id / probe_run_fingerprint
request_snapshot_fingerprint
ordered_sources_sha256
max_requests
actual send counters
per-row send started/completed flags
route templates
statuses
body len/hash
mapped fetch reason/outcome
transport category/action
audit record fingerprint
```

and no raw account/order/client/body/token/path values.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
