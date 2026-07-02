# M3b-22 controlled real-readonly evidence package

Status: controlled one-shot FINAM real-readonly evidence package. M3b-22 still
does not authorize FINAM order placement/cancel, real command consumption, real
CommandAck lifecycle, runtime attachment, `LiveReady`, live micro, stop/SLTP, or
bracket behavior.

## Operator command

The evidence package is produced by:

```bash
FINAM_SECRET_TOKEN=... \
FINAM_ACCOUNT_ID=... \
FINAM_SYMBOL='TICKER@MIC' \
cargo run -p broker-cli -- finam-real-readonly-evidence \
  --output reports/finam-real-readonly-evidence/redacted-evidence.json
```

The command performs only:

- FINAM auth/session calls required to obtain a JWT and token-details shape;
- one public operator-run broker-truth probe;
- GET-only real-readonly broker-truth source calls;
- redacted report writing.

It does not call FINAM order placement/cancel endpoints.

The token/account preflight is intentionally strict. `token_details.readonly`
must be `true`; a trading-capable token with `readonly=false` blocks before any
broker-truth source request is sent.

## Required controlled-run properties

The generated report must satisfy:

```text
blocking_reasons == []
probe_run_clock_source == OperatorProvided
computed_preflight_age_ms >= 0
computed_preflight_age_ms <= preflight_max_age_ms
actual_http_send_started_count <= max_requests
actual_http_send_completed_count <= actual_http_send_started_count
audit_store_mode == EphemeralEvidenceStore
```

The command hard-codes safe operator-run flags:

```text
retry_disabled == true
background_loop_disabled == true
scheduler_disabled == true
operator_disable_procedure_documented == true
preserve_transport_error_taxonomy == true
```

## Evidence matrix

Each evidence matrix row is redacted and includes:

```text
attempt_id
source
route_template
actual_http_send_started
actual_http_send_completed
http_status
http_body_present
http_body_len
http_body_sha256
mapped_fetch_reason
outcome
transport_error_category
operator_action
audit_record_sha256
```

Raw account ids, order ids, client ids, tokens, rendered paths, query values,
and raw bodies must not appear in the report.

## Artifact policy

Actual evidence reports are runtime/review artifacts under:

```text
reports/finam-real-readonly-evidence/
```

They are intentionally separate from source handoff archives. Source handoff
archives continue to exclude `reports/`.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
