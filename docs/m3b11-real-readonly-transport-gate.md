# M3b-11 real-readonly broker-truth transport gate

Status: disabled-by-default real-readonly design gate. M3b-11 still does not
authorize FINAM `POST /orders`, FINAM `DELETE /orders/{order_id}`, real command
consumption, real FINAM CommandAck lifecycle, runtime strategy attachment,
`LiveReady`, live micro, stop/SLTP, or bracket behavior.

M3b-11 separates local/mock `/readonly/...` placeholders from documented FINAM
REST read-only route templates. It does not implement a network client and does
not perform HTTP I/O.

M3b-12 follow-up implements the GET-only real-readonly transport behind this
gate, plus query policy, operator guardrails, and redacted SQLite audit. See
`docs/m3b12-real-readonly-broker-truth-transport.md`.

Sources used for route templates:

- FINAM REST docs: https://api.finam.ru/docs/rest/
- local project notes: `docs/finam-api-notes.md`

## Disabled-by-default gate

`GatewayFeatureSet` now has:

```text
real_readonly_broker_truth_enabled = false
```

The real-readonly gate is separate from the real order endpoint gate. It can be
approved only when read-only broker-truth is explicitly enabled and order/runtime
features remain disabled:

- command consumer disabled;
- order placement disabled;
- cancel disabled;
- stop/SLTP/bracket disabled.

The gate policy is:

```text
AsyncReadOnlyGetOnlyNoRawBodyCrossing
```

## FINAM read-only route builder

M3b-10 local mock specs continue to use safe logical placeholders:

```text
/readonly/...
```

M3b-11 adds a separate FINAM route builder for documented REST GET templates:

| Broker-truth source | FINAM route template |
| --- | --- |
| GetOrder | `GET /v1/accounts/{account_id}/orders/{order_id}` |
| OrdersSnapshot | `GET /v1/accounts/{account_id}/orders` |
| TradesSnapshot | `GET /v1/accounts/{account_id}/trades` |
| PositionSnapshot | `GET /v1/accounts/{account_id}` |

Rendered raw paths are private to the route type. Public diagnostics expose only
method, route template, route source, query-key names, and presence/length
metadata for account/order/client/instrument fields.

## Real-readonly async transport boundary

`FinamRealReadonlyBrokerTruthTransport` is an async GET/read-only-only boundary.
It accepts:

```text
RealReadonlyBrokerTruthGateApproved
FinamRealReadonlyRoute
```

and returns:

```text
CancelBrokerTruthReadonlyCapturedResponse
```

The captured response exposes only redacted HTTP diagnostics publicly. Raw body
bytes remain private to the mapper boundary.

## Instrument identity hardening

Broker-truth instrument matching no longer accepts symbol-only equality.

Evidence can pass the scope guard only when:

- full instrument identity matches; or
- symbol, venue symbol, and exchange match, with market either matching or not
  provided by the broker DTO.

Same symbol on a different venue is treated as `InstrumentMismatch`.

## UnknownClientError policy

`UnknownClientError` is treated as manual unknown-pending policy:

```text
UnknownClientError -> UnknownPendingOrder operator signal
```

This allows other truth sources to be attempted by orchestration, but the final
report remains operator-visible and does not silently select client-error truth
as evidence.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
