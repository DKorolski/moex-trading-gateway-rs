# M3b-24 / M3c-0 pre-order readiness closeout

Status: pre-order readiness closeout and M3c gate design preparation. This
document is not an implementation of FINAM order placement/cancel, real command
consumption, real CommandAck lifecycle, runtime attachment, `LiveReady`, live
micro, stop/SLTP, or bracket behavior.

## Scope

M3b-24 closes the remaining M3b evidence/readiness items before an M3c order
endpoint gate can be reviewed:

- GetOrder 200 real-shape fixture coverage;
- positive reconciliation fixture for exact identity;
- MismatchedOrderIdentity fixture for GetOrder 200 mismatch;
- release-profile evidence policy;
- FINAM route-template recheck policy;
- M3c feature-flag/off-by-default order transport plan;
- operator arming, durable store, and retry/backoff readiness matrix.

No FINAM POST/DELETE order endpoint is called or added.

## GetOrder 200 fixture closeout

`finam-gateway` includes a redacted real-shape fixture test:

```text
m3b24_get_order_200_real_shape_fixture_covers_exact_and_mismatch_redacted
```

It covers:

```text
GetOrder -> 200 / identity exact / parsed order DTO
GetOrder -> 200 / identity mismatch / MismatchedOrderIdentity
```

The exact fixture proves:

```text
parsed_orders_count = 1
matched_orders_count = 1
identity_strength = BrokerOrderIdExact
broker_truth = Terminal
```

The mismatch fixture proves:

```text
parsed_orders_count = 1
matched_orders_count = 0
reason = MismatchedOrderIdentity
```

The fixture report is serialization-checked for redaction: raw broker order id,
client order id, alternate order id, alternate client id, and raw broker
comments must not appear.

## Release-profile evidence policy

M3b-23 evidence was collected with `broker_cli_build_profile = debug`. That is
accepted only for API-contract validation.

Before any M3c order endpoint gate can be accepted, the operator must provide
one of:

- a release-profile real-readonly evidence package using the same
  `finam-real-readonly-evidence` command; or
- an explicit reviewer-accepted waiver stating why debug-profile evidence is
  sufficient for the specific gate being reviewed.

This policy applies before order endpoints are enabled. It does not authorize
order endpoint calls.

## FINAM route-template recheck policy

Current real-readonly route templates are tagged as:

```text
FinamRestDocs20260701
```

Immediately before an M3c order endpoint gate, the reviewer/operator must
recheck:

```text
GET /v1/accounts/{account_id}/orders/{order_id}
GET /v1/accounts/{account_id}/orders
GET /v1/accounts/{account_id}/trades
GET /v1/accounts/{account_id}
```

against current FINAM REST documentation/API behavior. If FINAM docs drift, the
route source enum/documentation and evidence package must be updated before the
gate can proceed.

## M3c order endpoint gate design only

M3c design may specify a future order endpoint transport, but it must remain
disabled by default and unreachable without all of:

```text
feature flag: real_order_endpoint_enabled = false by default
command consumer disabled by default
operator arm one-shot and TTL
account allowlist
instrument allowlist
quantity/notional/rate limits
SQLite durable id mapping store open and healthy
forbidden unknown active broker order check
no blind retry after ambiguous submit
rate-limit/backoff policy
manual intervention state
forbidden_surface_scan green until explicit gate revision
```

## Pre-order readiness matrix

| Area | Required before endpoint gate | M3b-24 status |
|---|---|---|
| GetOrder 200 exact | Parsed DTO + exact identity evidence | synthetic real-shape fixture covered |
| GetOrder 200 mismatch | MismatchedOrderIdentity evidence | synthetic real-shape fixture covered |
| Positive reconciliation | exact identity maps to terminal/still-working truth | fixture covered for terminal |
| Release profile | release evidence or accepted waiver | policy documented, evidence not collected |
| Route templates | recheck against current FINAM docs/API | policy documented |
| Order feature flags | off by default, separate from readonly | design requirement only |
| Operator arming | one-shot, TTL, account/instrument limits | design requirement only |
| Durable store | SQLite id mapping and recovery healthy | existing dry path, gate requirement |
| Retry/backoff | no blind retry, bounded read retries | existing dry policy, gate requirement |
| Runtime/live | remain detached | not implemented |

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
