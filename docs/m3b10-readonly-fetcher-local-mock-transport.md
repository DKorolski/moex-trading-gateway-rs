# M3b-10 read-only broker-truth local mock transport

Status: dry / local-readonly integration design. M3b-10 still does not
authorize FINAM `POST /orders`, FINAM `DELETE /orders/{order_id}`, real command
consumption, real FINAM CommandAck lifecycle, runtime strategy attachment,
`LiveReady`, live micro, stop/SLTP, or bracket behavior.

M3b-10 extends the M3b-9 local DTO mapper with a read-only mock transport
boundary. It is still local and does not perform network I/O.

## Read-only request specs

Broker-truth request builders now create redacted GET-only request specs for:

- GetOrder;
- OrdersSnapshot;
- TradesSnapshot;
- PositionSnapshot.

The specs expose endpoint shape, query-key names, and presence/length
diagnostics for account/order/client/instrument fields. They do not serialize
raw account ids, broker order ids, client order ids, or symbols.

## Local mock HTTP client

`CancelBrokerTruthReadonlyHttpClient` is an async read-only client boundary.
`LocalMockCancelBrokerTruthReadonlyHttpClient` records redacted request specs
and returns local fixture responses.

The client returns `CancelBrokerTruthReadonlyCapturedResponse`, whose public
surface exposes only source and redacted HTTP diagnostic. Raw body bytes remain
private to the gateway mapper boundary.

## Refined read-only client error policy

Read-only HTTP 4xx policy is now typed:

| HTTP outcome | Fetch reason |
| --- | --- |
| 400 / 422 | `InvalidRequest` |
| 405 | `UnsupportedEndpoint` |
| 409 / 410 / other 4xx | `UnknownClientError` |

This separates invalid request/endpoint problems from malformed 2xx body
decode failures.

## Identity strength policy

`ClientOrderIdFallback` is no longer strong evidence by default during
orchestration. The default policy downgrades it to unknown with
`WeakIdentityNeedsConfirmation`.

The policy can explicitly opt in to treating client-id fallback as strong:

```text
identity.accept_client_order_id_fallback_as_strong = true
```

Default remains `false`.

## Account and instrument scope checks

Read-only broker-truth mappers now reject evidence when the broker DTO matches
the requested order/client id but belongs to a different expected account or
instrument. These cases become typed fetch reasons:

- `AccountMismatch`;
- `InstrumentMismatch`.

The diagnostics remain categorical and do not expose raw ids.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
