# M3b-12 real-readonly broker-truth transport

Status: GET-only real-readonly implementation behind the disabled-by-default
broker-truth gate. M3b-12 still does not authorize FINAM `POST /orders`,
FINAM `DELETE /orders/{order_id}`, real command consumption, real FINAM
CommandAck lifecycle, runtime strategy attachment, `LiveReady`, live micro,
stop/SLTP, or bracket behavior.

M3b-12 turns the M3b-11 route gate into an executable read-only transport path:

- `ReqwestFinamRealReadonlyBrokerTruthTransport` performs FINAM REST `GET`
  requests only;
- `FinamRealReadonlyBrokerTruthAsyncFetcher` builds FINAM GET routes behind
  `RealReadonlyBrokerTruthGateApproved`, captures raw status/body privately,
  maps through the existing broker-truth DTO classifiers, and emits redacted
  route/HTTP/audit diagnostics;
- local tests use `LocalMockFinamRealReadonlyBrokerTruthTransport`, not live
  FINAM I/O.

M3b-13 follow-up makes read-only enablement stricter: transport/fetcher
construction requires `RealReadonlyBrokerTruthRunApproved`, transport errors
carry redacted categories, full trades pages become incomplete evidence, and a
disabled-by-default contract-probe harness is available. See
`docs/m3b13-real-readonly-enable-runbook.md`.

## Captured-response invariant

The implementation preserves the review invariant:

```text
FINAM GET route + private raw status/body capture
-> redacted captured-response diagnostic
-> typed fetch reason / observation mapping
-> orchestration/audit record
```

Status mapping remains explicit:

| HTTP / failure | Broker-truth reason |
| --- | --- |
| 404 | `NotFound404` |
| 401 / 403 | `Unauthorized` |
| 408 / 504 / transport timeout | `Timeout` |
| 429 | `RateLimited` |
| 400 / 422 | `InvalidRequest` |
| 405 | `UnsupportedEndpoint` |
| 409 / 410 / other 4xx | `UnknownClientError` |
| 5xx / transport send/read failure | `Maintenance` |
| malformed 2xx body | `DecodeError` |

Raw body bytes remain private to the mapper boundary. Public diagnostics expose
only status, body presence/length/hash, route templates, query-key names, and
presence/length metadata for account/order/client/instrument fields.

## Private route request parts

Rendered FINAM paths are still not public API. The route type exposes only a
crate-private request-parts method used by the transport implementation.
Debug/serde diagnostics do not include raw account ids, order ids, client order
ids, query values, or rendered paths.

## Query policy

`FinamRealReadonlyBrokerTruthQueryPolicy` defines the real-readonly snapshot
query semantics:

- orders snapshot: broker account snapshot, then client-side
  account/instrument/order-identity filtering;
- trades snapshot: single-page `GET /v1/accounts/{account_id}/trades`;
- default trades limit: `1000`;
- default trades window: 24 hours ending at `request.requested_at`;
- maximum trades window: 7 days;
- pagination policy: `SinglePageNoCursor`.

Invalid zero/oversized limits or windows fail as `InvalidRequest` before any
HTTP request is sent.

## Operator guardrails

`evaluate_finam_real_readonly_operator_guardrails()` provides a redacted
operator/runbook decision for enabling read-only broker truth. It checks:

- the real-readonly gate is approved;
- base URL is HTTPS;
- account id is present;
- account allowlist is non-empty and contains the requested account;
- request timeout is bounded;
- minimum request interval is not below the safety floor;
- command consumer, placement, cancel, and stop/SLTP/bracket flags are disabled.

The guardrail diagnostic includes only lengths/counts/hashes, not raw account
ids or base URL values.

## SQLite audit

`SqliteFinamRealReadonlyBrokerTruthAuditStore` persists redacted real-readonly
attempt records. Audit rows include:

- timestamp;
- source and route template;
- query-key names;
- id presence/length metadata;
- HTTP status and body presence/length/hash;
- mapped fetch reason when present;
- constant safe details marker:
  `finam_real_readonly_broker_truth`.

The audit table does not store raw account/order/client ids, raw paths, raw
query values, or raw HTTP bodies.

## Source-scan guard

M3b-12 adds tests proving the real-readonly transport source stays GET-only and
does not introduce:

- `.post(`;
- `.delete(`;
- FINAM place/cancel request specs;
- real order endpoint transport;
- public raw path/body/response fields.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
