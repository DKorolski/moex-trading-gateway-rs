# M3b-17 real-readonly evidence package hardening

Status: controlled real-readonly evidence package hardening. M3b-17 still does
not authorize FINAM order placement/cancel, real command consumption, real
CommandAck lifecycle, runtime attachment, `LiveReady`, live micro, stop/SLTP, or
bracket behavior.

M3b-18 follow-up makes the token/account approval marker non-serializable,
splits request/captured/actual-send counters, and adds probe run identity for
audit correlation. See `docs/m3b18-real-readonly-pre-evidence-gate.md`.

## Token readonly/scope diagnostic

The redacted token/account preflight diagnostic now records token scope shape:

```text
token_readonly_flag_present
token_readonly_flag_value
md_permissions_count
```

Controlled operator probes require:

```text
token_details_checked == true
token_readonly_flag_value == Some(true)
token_account_hash_match == Some(true)
no_order_feature_flags_enabled == true
```

This is a defense-in-depth check. The code boundary still blocks order
POST/DELETE independently.

## Attempt-id evidence alignment

The evidence package no longer relies on positional alignment between separate
route/captured/audit/source arrays. Each source attempt now has an attempt
record:

```text
attempt_id
source
source_diagnostic
route_diagnostic
captured_diagnostic
audit_record
```

The evidence matrix is built from those attempt records.

## Request/send counters

The operator report includes:

```text
requested_sources_count
actual_send_count
max_requests
```

Tests assert:

```text
actual_send_count <= max_requests
```

For a controlled real-readonly evidence run, this makes the package auditable
without inspecting raw network logs.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
