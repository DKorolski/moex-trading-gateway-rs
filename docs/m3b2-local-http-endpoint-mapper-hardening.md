# M3b-2 local HTTP endpoint mapper hardening

Status: dry/non-network. M3b-2 does not authorize FINAM `POST /orders`,
FINAM `DELETE /orders/{order_id}`, real command consumption, real FINAM
CommandAck lifecycle, runtime strategy attachment, `LiveReady`, live micro,
stop/SLTP, or bracket behavior.

M3b-2 hardens the endpoint response layer one step closer to a future real
transport while still using only local/mock HTTP-shaped responses. The goal is
to prove that post-network decode/map failures cannot bypass durable order-path
attempt recording.

## Local HTTP response classifier

`broker-finam` now exposes a redacted local response classifier:

```text
FinamOrderEndpointLocalHttpResponse
FinamOrderEndpointClassifiedResponse
classify_order_endpoint_local_http_response(...)
```

The local response type can represent:

- HTTP status + body + optional retry-after milliseconds;
- timeout.

It has a custom `Debug` implementation that records only status, body length,
body kind, and retry-after presence. Raw body text and broker order ids are not
printed.

M3b-3 follow-up: internal endpoint mapped/classified results also have redacted
`Debug` output and are no longer serde export objects. See
`docs/m3b3-redacted-endpoint-result-status-policy.md`.

## Status and body mapping

M3b-2 maps local HTTP-shaped outcomes to endpoint results as follows:

| Local response | Endpoint outcome |
| --- | --- |
| 2xx JSON with non-empty broker order id | `Accepted` |
| 2xx JSON without broker order id | `Accepted` without id, requiring reconciliation |
| 2xx JSON with empty broker order id | `DecodeError` |
| malformed JSON | `DecodeError` |
| 400-class broker validation/rejection except 401/403/429 | `BrokerRejected` |
| 401 / 403 | `Unauthorized` |
| 429 | `RateLimited` with optional `retry_after_ms` |
| 500 | `Maintenance(Unknown)` |
| 503 | `Maintenance(ServiceInterval)` |
| timeout | `TimeoutUnknownPending` path |

The classifier diagnostics contain safe body kinds such as `object`,
`malformed_json`, or `empty`; they do not carry raw broker body text.

## Post-network ordering invariant

M3b-2 adds gateway helpers:

```text
simulate_place_order_endpoint_local_http_response(...)
simulate_cancel_order_endpoint_local_http_response(...)
```

These helpers deliberately classify the local HTTP response after durable
attempt recording:

```text
build approved request spec
load order-path record
persist BeginSubmit / RequestCancel
classify local HTTP response
persist final state + ACK/disarm
```

This is the important safety proof from review #32: an empty accepted
`broker_order_id` or malformed JSON cannot return an early mapper error before
the local store records that an endpoint attempt began.

## New safe categories

M3b-2 adds broker-neutral safe categories:

```text
CommandAckReasonCode::Unauthorized
OrderPathErrorKind::Unauthorized
OperatorDisarmSignal::OrderEndpointUnauthorized
```

Existing M3b-1 categories continue to handle rate-limit, maintenance, and
decode-error responses.

## SQLite and ACK proofs

Tests prove:

- successful local HTTP accepted response publishes redacted runtime ACK;
- empty broker order id records `BeginSubmit` before `ResponseDecodeError`;
- malformed cancel response records `RequestCancel` before
  `ResponseDecodeError`;
- 401 and 403 disarm with `OrderEndpointUnauthorized`;
- Redis ACK payloads contain only safe reason codes and do not leak raw account,
  client order id, broker order id, or broker body text.

## Endpoint gate status

`EndpointGateApproved` remains unconstructible. The real endpoint gate still
blocks on `M3a11PreEndpointReviewRequired`, and the post-review approval
constant remains false.

No real broker base URL is used by M3b-2 order tests.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
