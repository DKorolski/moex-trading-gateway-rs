# M3b-9 read-only broker-truth local HTTP mapper

Status: dry / local-readonly fixture contract. M3b-9 still does not authorize
FINAM `POST /orders`, FINAM `DELETE /orders/{order_id}`, real command
consumption, real FINAM CommandAck lifecycle, runtime strategy attachment,
`LiveReady`, live micro, stop/SLTP, or bracket behavior.

M3b-9 prepares the broker-truth fetcher boundary for a future real FINAM
read-only implementation by adding local HTTP-shaped mapping tests. It does not
perform network I/O.

## Async-aware boundary

The synchronous dry contract remains:

```text
CancelBrokerTruthReadonlyFetcher
```

M3b-9 adds an explicit future-facing async boundary:

```text
CancelBrokerTruthAsyncReadonlyFetcher
```

The async boundary uses an owned request snapshot so a future real network
implementation does not need to hide an async runtime behind a synchronous
trait.

## Local HTTP fixture mapper

`CancelBrokerTruthReadonlyHttpResponse` represents a local fixture with:

```text
status
optional body bytes
```

Its debug/diagnostic output contains only:

```text
status
body_present
body_len
body_sha256
```

Raw HTTP body bytes are not serde-exported and are not included in
broker-truth reports.

The local mapper covers:

- GetOrder DTO -> checked get-order truth result;
- OrdersSnapshot DTO -> order snapshot truth result;
- TradesSnapshot DTO -> trade snapshot truth result;
- PositionSnapshot DTO -> portfolio-position truth result.

## HTTP status policy

Read-only HTTP statuses map to typed fetch reasons:

| HTTP outcome | Fetch reason |
| --- | --- |
| 404 | `NotFound404` |
| 401 / 403 | `Unauthorized` |
| 408 | `Timeout` |
| 429 | `RateLimited` |
| 502 / 503 / other 5xx | `Maintenance` |
| 504 | `Timeout` |
| malformed 2xx body | `DecodeError` |

`Timeout` remains an unknown-pending/retry-later condition unless a broader
policy later decides to degrade or disarm. `Maintenance`, `Unauthorized`,
`RateLimited`, and `DecodeError` still feed the existing operator-disarm
priority.

## GetOrder identity strength

GetOrder diagnostics now include redacted identity strength:

```text
BrokerOrderIdExact
ClientOrderIdFallback
```

The value is categorical only. Raw broker order ids and client order ids remain
outside reports.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
