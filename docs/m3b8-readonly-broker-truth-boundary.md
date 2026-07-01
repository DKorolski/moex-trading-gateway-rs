# M3b-8 read-only broker-truth boundary

Status: dry / real-readonly contract. M3b-8 still does not authorize FINAM
`POST /orders`, FINAM `DELETE /orders/{order_id}`, real command consumption,
real FINAM CommandAck lifecycle, runtime strategy attachment, `LiveReady`, live
micro, stop/SLTP, or bracket behavior.

M3b-8 hardens the M3b-7 orchestration simulator at the boundary where future
real read-only broker-truth calls will feed observations.

## Checked get-order truth

The real-readonly get-order builder now checks returned identity before treating
a returned order as evidence:

```text
returned broker_order_id matches requested order id -> evidence
returned broker_order_id absent and client_order_id matches -> evidence
returned broker_order_id/client_order_id mismatch -> MismatchedOrderIdentity
missing/404 get-order -> NotFound404
```

Mismatch is exported only as a typed reason. The report still does not include
raw broker order ids or client order ids.

## Position truth guard hardening

Position-derived terminal truth can no longer become strong evidence merely
because direct sources were excluded from precedence policy.

For position-derived terminal truth to participate, the direct order/trade
sources must be actually attempted:

```text
GetOrder
OrdersSnapshot
TradesSnapshot
```

Those attempted direct sources must then be missing, stale, or unknown. If the
direct sources are skipped by policy, position truth is downgraded to unknown
with `PositionGuardRejected`.

## Read-only fetcher contract

M3b-8 introduces an explicit read-only broker-truth fetcher boundary:

```text
CancelBrokerTruthReadonlyFetcher
```

The existing mock fetcher implements this contract and remains dry. The
boundary is guarded by source scans so it does not reference order endpoint
approval markers, FINAM order request specs, order endpoint methods, `.post(`,
or `.delete(`.

M3b-9 extends this boundary with an async-aware read-only fetcher contract and
local HTTP-shaped DTO mappers. See
`docs/m3b9-readonly-fetcher-local-http.md`.

## Error mapping contract

Read-only transport and HTTP outcomes map to typed fetch reasons:

| Read-only outcome | Fetch reason |
| --- | --- |
| HTTP 404 | `NotFound404` |
| HTTP 401 / 403 | `Unauthorized` |
| HTTP 429 | `RateLimited` |
| HTTP 5xx | `Maintenance` |
| transport timeout | `Timeout` |
| malformed body / decode failure | `DecodeError` |
| dry missing fixture | `MissingFixture` |

`MissingFixture` is a dry/test reason and is not intended for a future real
read-only fetcher.

## Config and policy fingerprint

`GatewayConfig` now includes a defaulted broker-truth policy section:

```text
broker_truth.cancel_reconciliation
```

The policy includes:

- precedence version;
- per-source `max_age_ms`;
- source precedence order;
- position guard settings.

Every orchestration report includes a redacted policy snapshot and a SHA-256
policy fingerprint. The shadow config parser accepts the same broker-truth
policy section without enabling order endpoints.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
