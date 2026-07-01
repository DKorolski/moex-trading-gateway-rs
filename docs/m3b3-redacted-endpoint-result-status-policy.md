# M3b-3 redacted endpoint result boundary and status policy

Status: dry/non-network. M3b-3 does not authorize FINAM `POST /orders`,
FINAM `DELETE /orders/{order_id}`, real command consumption, real FINAM
CommandAck lifecycle, runtime strategy attachment, `LiveReady`, live micro,
stop/SLTP, or bracket behavior.

M3b-3 hardens the local endpoint-response layer before any real transport is
introduced. It closes the internal-result redaction boundary and makes local
HTTP-shaped status mapping endpoint-context-aware.

## Redacted internal result boundary

The following internal endpoint result types are no longer serde-export types:

```text
FinamOrderExecutionOutcome
FinamOrderEndpointMappedResult
FinamOrderEndpointClassifiedResponse
```

They are still usable inside the gateway/order-path pipeline, but they should
not be used as review/export/reporting payloads. Their `Debug` implementations
are custom and redacted:

- accepted broker order id is represented as presence + length;
- raw broker order id is not printed;
- local HTTP body text is never printed;
- diagnostics remain the export boundary.

The safe export/reporting object remains:

```text
FinamOrderEndpointResponseDiagnostic
```

## Context-aware local status policy

`broker-finam` now exposes:

```text
FinamOrderEndpointContext
classify_order_endpoint_local_http_response_for_context(context, response)
```

The previous context-free classifier remains as a place-oriented convenience
wrapper for existing dry tests.

Shared policy:

| Local response | Outcome |
| --- | --- |
| 2xx valid accepted body | accepted |
| 2xx empty broker order id | decode error |
| malformed JSON | decode error |
| body read failure | decode error |
| 401 / 403 | unauthorized |
| 429 | rate-limited |
| 408 / 504 | timeout / unknown pending |
| 500 / 502 / 503 | maintenance |

Place-specific policy:

- generic 4xx broker errors, including 404/409/410/422, are local
  `BrokerRejected` in this dry classifier.

Cancel-specific policy:

- 404 / 409 / 410 are not treated as ordinary broker rejection;
- they map to `ReconciliationRequired`;
- gateway integration records `ManualInterventionRequired` with
  `UnknownPending` ACK and `UnknownPendingOrder` disarm.

This avoids treating an uncertain cancel response as a safe terminal fact.

## Post-begin body failure proof

M3b-3 adds local body-read-failure modeling:

```text
FinamOrderEndpointLocalHttpResponse::BodyReadFailed
```

Gateway tests prove the same ordering invariant as M3b-2:

```text
BeginSubmit / RequestCancel persisted
-> body read / decode / status classification fails
-> ManualInterventionRequired + safe ACK/disarm persisted
```

## Operator disarm matrix

The local HTTP integration tests cover these disarm paths:

| Outcome | Disarm signal |
| --- | --- |
| unauthorized | `OrderEndpointUnauthorized` |
| rate-limited | `OrderEndpointRateLimited` |
| maintenance | `OrderEndpointMaintenance` |
| decode/body-read failure | `OrderEndpointDecodeError` |
| timeout/ambiguous | `UnknownPendingOrder` |

## Endpoint gate status

`EndpointGateApproved` remains unconstructible. The real endpoint gate still
blocks on `M3a11PreEndpointReviewRequired`, and the post-review approval
constant remains false.

No real broker base URL is used by M3b-3 order tests.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
