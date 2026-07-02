# M3b-23 real-readonly evidence closeout hardening

Status: evidence-package hardening after the accepted M3b-22 controlled
real-readonly run. M3b-23 still does not authorize FINAM order
placement/cancel, real command consumption, real CommandAck lifecycle, runtime
attachment, `LiveReady`, live micro, stop/SLTP, or bracket behavior.

## Self-contained evidence metadata

`broker-cli finam-real-readonly-evidence` now writes an `evidence_metadata`
section into the redacted evidence JSON:

```text
source_commit_full_sha
source_archive_name
source_archive_sha256
broker_cli_package_version
broker_cli_build_profile
forbidden_surface_scan.status
forbidden_surface_scan.script_sha256
runbook_doc
runbook_doc_version
```

The command runs `scripts/forbidden_surface_scan.sh` before the FINAM evidence
run and fails before any broker-truth source request if the scan fails.

## Per-attempt timing evidence

Each evidence matrix row includes redacted timing fields:

```text
attempt_started_at
attempt_completed_at
attempt_elapsed_ms
inter_attempt_gap_ms
min_request_interval_ms
actual_http_send_started_at
actual_http_send_completed_at
actual_http_send_elapsed_ms
```

`inter_attempt_gap_ms` is measured between actual HTTP send starts when those
timestamps exist. This keeps the report focused on the real GET-only transport
boundary rather than local loop bookkeeping alone.

## Parsed-count reconciliation summary

Each evidence matrix row distinguishes route/HTTP contract evidence from parsed
reconciliation truth evidence:

```text
parsed_orders_count
matched_orders_count
parsed_trades_count
matched_trades_count
position_items_count
position_identity_match_count
snapshot_complete
snapshot_incomplete_reason
```

These fields are redacted counts only. They do not expose raw account ids,
order ids, client order ids, symbols, paths, query values, tokens, or raw
response bodies.

## GetOrder 200 follow-up plan

M3b-23 does not create real orders. `GetOrder -> 200` evidence remains a future
read-only characterization task and must be satisfied by one of:

- a controlled read-only probe against an already-existing broker order id that
  was not created by this project during the evidence run; or
- a redacted real-shape fixture supplied out-of-band and checked through the
  existing mapper/identity tests.

The follow-up must cover:

```text
GetOrder -> 200 / identity exact / parsed order DTO
GetOrder -> 200 / identity mismatch / MismatchedOrderIdentity
```

It must still avoid FINAM POST/DELETE order endpoints.

M3b-24 follow-up adds a checked synthetic real-shape GetOrder 200 fixture for
exact identity and mismatch, plus M3c pre-order gate policy. See
`docs/m3b24-m3c0-pre-order-readiness-closeout.md`.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
