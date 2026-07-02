# M3b-13 real-readonly enablement runbook / contract probe

Status: read-only enablement hardening. M3b-13 still does not authorize FINAM
`POST /orders`, FINAM `DELETE /orders/{order_id}`, real command consumption,
real FINAM CommandAck lifecycle, runtime strategy attachment, `LiveReady`, live
micro, stop/SLTP, or bracket behavior.

M3b-13 hardens the M3b-12 real-readonly GET foundation so it cannot be enabled
from only a token/base URL/gate. A read-only run now requires an explicit
operator-scope approval marker.

M3b-14 follow-up adds the bounded operator-run harness for collecting redacted
contract-probe evidence. See
`docs/m3b14-real-readonly-contract-probe-operator-harness.md`.

## Mandatory run approval marker

`RealReadonlyBrokerTruthRunApproved` is constructible only from:

```text
RealReadonlyBrokerTruthGateApproved
+ FinamRealReadonlyOperatorGuardrailDecision.allowed == true
```

The marker stores only redacted account identity metadata:

```text
account_id_len
account_id_sha256
timeout/rate-limit values
```

It does not store raw account ids. The real-readonly transport constructor and
the real-readonly fetcher require this marker. Fetch attempts whose account hash
does not match the approved account are rejected before route rendering / HTTP
send and are audited as redacted failed attempts.

## Transport error categories

Real-readonly HTTP diagnostics now distinguish redacted transport categories:

```text
DnsOrConnectError
TlsError
HttpSendError
BodyReadError
Timeout
RequestBuildError
AccountNotAllowed
```

These categories are safe enum labels only. They do not expose URL, token,
request path, query values, or body text.

## Trades snapshot incomplete semantics

The bounded single-page trades policy remains intentionally conservative. If a
trades response reaches the configured `trades_limit` and does not contain exact
order/client evidence for the requested cancel reconciliation, the source maps
to:

```text
TradesSnapshotIncomplete
```

This is an operator-visible unknown-pending condition, not strong absence
evidence.

## Contract probe harness

`FinamRealReadonlyContractProbeConfig` is disabled by default:

```text
enabled = false
```

When explicitly enabled in tests/operator tooling, the probe can run selected
read-only broker-truth sources through the already approved real-readonly
fetcher and returns a redacted report:

- attempted source names;
- route diagnostics with templates/key names only;
- captured HTTP diagnostics with status/body hash/category only;
- redacted audit records;
- typed fetch reasons.

The harness does not know how to place or cancel orders.

## SQLite audit hardening

M3b-13 extends audit records for failed attempts:

- route-build/account-scope failures can be recorded without HTTP status;
- transport error categories are stored as safe enum labels;
- raw ids, rendered paths, query values, URL, tokens, and raw bodies remain
  absent.

## Operator sequence before any real read-only use

1. Keep all order/runtime flags disabled.
2. Enable only `real_readonly_broker_truth_enabled`.
3. Configure HTTPS FINAM base URL.
4. Configure a non-empty account allowlist.
5. Build guardrail decision for the exact requested account.
6. Construct `RealReadonlyBrokerTruthRunApproved`.
7. Construct the GET-only real-readonly transport/fetcher.
8. Run the disabled-by-default contract probe only when explicitly enabled.
9. Inspect redacted route/HTTP/audit diagnostics.
10. Disable the probe/read-only run after evidence collection.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady` publication;
- first live micro;
- stop/SLTP/bracket.
