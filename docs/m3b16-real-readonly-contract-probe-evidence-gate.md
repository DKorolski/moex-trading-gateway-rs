# M3b-16 real-readonly contract probe evidence gate

Status: controlled real-readonly evidence gate preparation. M3b-16 still does
not authorize FINAM order placement/cancel, real command consumption, real
CommandAck lifecycle, runtime attachment, `LiveReady`, live micro, stop/SLTP, or
bracket behavior.

## Base URL bound to RunApproved

`RealReadonlyBrokerTruthRunApproved` now stores redacted identity for the exact
approved FINAM base URL:

```text
rest_base_url_len
rest_base_url_sha256
```

`ReqwestFinamRealReadonlyBrokerTruthTransport::try_new(...)` rejects a transport
config whose base URL hash differs from the approved marker. This preserves:

```text
operator guardrails approved this exact HTTPS base URL
-> transport can only run against this exact base URL
```

## Token/account preflight diagnostic

The operator-run config can carry a redacted token/account preflight diagnostic:

```text
token_details_checked
token_account_ids_count
requested_account_id_len
requested_account_id_sha256
token_account_hash_match
no_order_feature_flags_enabled
```

Controlled operator probes require this preflight to pass before the one-shot
read-only probe loop can run.

## Evidence matrix

The operator-run report now includes a redacted evidence matrix for every
requested read-only source:

```text
source
route_template
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

The matrix is designed for the controlled FINAM read-only routes:

```text
GET /v1/accounts/{account_id}/orders/{order_id}
GET /v1/accounts/{account_id}/orders
GET /v1/accounts/{account_id}/trades
GET /v1/accounts/{account_id}
```

It contains no raw account id, raw order id, raw client order id, token, rendered
URL/path, query value, or raw response body.

## Ephemeral evidence only

For M3b-16 controlled probes, `PersistentAuditStore` is blocked at operator-run
validation time. Use:

```text
EphemeralEvidenceStore
```

Persistent mode still requires a separate hardening ticket.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
