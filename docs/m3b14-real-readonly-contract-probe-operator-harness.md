# M3b-14 real-readonly contract probe operator harness

Status: operator-run harness for bounded read-only FINAM contract evidence.
M3b-14 still does not authorize FINAM order placement/cancel, real command
consumption, real CommandAck lifecycle, runtime attachment, `LiveReady`, live
micro, stop/SLTP, or bracket behavior.

## Purpose

M3b-14 wraps the M3b-13 contract probe in an explicit operator-run harness. The
harness is intentionally one-shot and bounded:

- no retry;
- no background loop;
- no scheduler;
- explicit source list;
- explicit max request count;
- exact request timeout / min interval match against the approved run marker;
- single approved account hash;
- redacted output-location descriptor;
- documented disable procedure before the run can proceed.

## Operator-run approval

The harness accepts only a `FinamRealReadonlyBrokerTruthAsyncFetcher` that
already carries `RealReadonlyBrokerTruthRunApproved`. Before any source is
probed it verifies:

```text
request.account_id hash == approved account hash
request_timeout_ms == approved timeout
min_request_interval_ms == approved interval
sources.len() <= max_requests <= 4
```

If any check fails, no route rendering and no HTTP send happens through the
operator harness.

## Evidence output

The operator-run report is redacted:

- account identity is represented by length + SHA-256 only;
- output location is represented by length + SHA-256 only;
- route diagnostics contain templates and query-key names only;
- HTTP diagnostics contain status, body presence/length/SHA-256, and transport
  category only;
- audit records keep safe enum reasons and metadata only.

Raw account ids, order ids, client ids, URLs, rendered paths, query values,
tokens, and raw HTTP bodies must remain absent from handoff artifacts.

## Audit store mode

M3b-14 makes the audit-store mode explicit in the operator-run config:

```text
EphemeralEvidenceStore
PersistentAuditStore
```

Default is `EphemeralEvidenceStore`. Choosing `PersistentAuditStore` for a real
operator run requires a separate operational decision covering permissions,
retention, backup/export policy, and corruption handling.

## Transport error taxonomy

The operator report preserves transport categories and maps them to operator
actions:

```text
DnsOrConnectError -> check DNS/network/broker reachability
TlsError -> check certificates, clock, and proxy
HttpSendError -> check HTTP client/network boundary
BodyReadError -> check body read timeout or truncated response
Timeout -> check timeout budget and broker latency
RequestBuildError -> check route/request builder contract
AccountNotAllowed -> check approved account allowlist
```

## Forbidden-surface CI scan

M3b-14 adds `scripts/forbidden_surface_scan.sh` and runs it from GitHub Actions.
The scan is context-aware:

- real HTTP `DELETE` is forbidden;
- `Method::POST` / `Method::DELETE` are forbidden in gateway/order surfaces;
- `.post(` is allowed only in broker-finam auth/session/token-details code;
- real-readonly transport/operator-probe scopes must not reference order
  endpoint request specs, real order transport, or place/cancel endpoint names.

## Real operator sequence

For a future real read-only evidence run:

1. Keep all order/runtime flags disabled.
2. Enable only real-readonly broker-truth gate.
3. Build operator guardrails for one account and construct
   `RealReadonlyBrokerTruthRunApproved`.
4. Configure explicit sources and `max_requests`.
5. Configure timeout/min interval equal to the approved marker.
6. Configure redacted output-location descriptor.
7. Choose audit mode; prefer `EphemeralEvidenceStore` unless a persistent audit
   plan is approved.
8. Run the one-shot operator contract probe.
9. Save only the redacted report.
10. Disable the probe/read-only run.

## Still not allowed

- FINAM real PlaceOrder endpoint calls;
- FINAM real CancelOrder endpoint calls;
- real command stream consumer connected to strategies;
- real CommandAck lifecycle against FINAM endpoints;
- strategy runtime adaptation;
- `LiveReady`;
- first live micro;
- stop/SLTP/bracket.
