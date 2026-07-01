# M3b-0 design / fixture gate

Status: dry/non-network. This document does not authorize FINAM
`POST /orders`, FINAM `DELETE /orders/{order_id}`, real command consumption,
runtime strategy attachment, `LiveReady`, live micro, stop/SLTP, or bracket
behavior.

M3b-0 turns the final pre-endpoint review comments into code-level contracts
without opening the real FINAM order transport.

## Endpoint gate marker design

`finam-gateway` now defines:

```text
EndpointGateApproved
EndpointGateApprovalError
FinamRealOrderEndpointTransport
```

The future real transport trait requires `&EndpointGateApproved` for both
place and cancel endpoint signatures. The marker has no public constructor.
`EndpointGateApproved::try_from_decision()` returns an error while the current
gate decision is blocked by `M3a11PreEndpointReviewRequired`. M3b-0 also keeps
a private post-review approval constant set to false, so a manually forged
`RealOrderEndpointGateDecision` with no blockers still cannot produce the
marker before the endpoint implementation review is accepted.

Current invariant:

```text
endpoint_calls_allowed = false
blocking_reasons includes M3a11PreEndpointReviewRequired
config cannot override this
```

Tests cover both default config and manually enabled adjacent flags.
They also cover a manually constructed allow-looking decision.

## Synthetic/redacted FINAM endpoint fixtures

`broker-finam` now has synthetic fixture DTOs and mapper classification for
future order endpoint response characterization:

```text
FinamOrderEndpointFixture
FinamOrderEndpointAcceptedDto
FinamOrderEndpointMappedResult
FinamOrderEndpointResponseDiagnostic
```

Covered fixture classes:

- accepted with broker order id;
- accepted without broker order id;
- rejected;
- timeout / transport unknown;
- rate limited;
- maintenance / market closed style response;
- decode error shape.

Fixtures and diagnostics are safe for review by default. Raw broker order ids
inside accepted DTOs are redacted from `Debug` and from diagnostics. Mapping an
accepted response with an empty broker order id is rejected.

## Future real transport signature

The dry compile-contract test proves a future real transport implementation
must accept request specs plus the endpoint gate marker:

```text
place_order_endpoint(&EndpointGateApproved, FinamPlaceOrderRequestSpec)
cancel_order_endpoint(&EndpointGateApproved, FinamCancelOrderRequestSpec)
```

The trait returns mapped endpoint results. It still has no HTTP implementation
and no real endpoint caller.

## SQLite deployment checks

`broker-core` now exposes:

```text
inspect_sqlite_runtime_directory(path, workspace_root)
SqliteRuntimeDirectoryIssue
```

This is a future startup/deployment gate helper. It can flag:

- missing runtime directory;
- path that is not a directory;
- group/world-accessible directory on Unix;
- runtime directory inside the workspace tree;
- runtime directory inside workspace artifact areas such as `reports`, `tmp`,
  or `handoff`.

M3b-0 does not enforce this on dry tests, but it provides the check needed for
future endpoint-capable startup blocking.

## Transition audit contract matrix

The safe event-name contract is now table-tested for key transitions:

```text
IntentRecorded -> SubmitInFlight = BeginSubmit
SubmitInFlight -> Submitted = SubmitAccepted
SubmitInFlight -> TimeoutUnknownPending = SubmitTimedOut
Submitted -> CancelRequested = RequestCancel
CancelRequested -> CancelSubmitted = CancelAccepted
CancelRequested -> CancelTimeoutUnknownPending = CancelTimedOut
CancelRequested -> ManualInterventionRequired + BrokerRejected = CancelRejected
CancelRequested -> ManualInterventionRequired + ReconciliationRequired = RequireManualIntervention
```

## Operator raw diagnostics policy

Raw local order-path records remain operator/internal only. Future CLI support,
if added, must require an explicit operator-only mode and must not be used by
runtime logs, Redis streams, review exports, or handoff archives.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
