# M3d-2a protected exact-two-route local mock endpoint harness

Status: local mock wire evidence only. This step does not enable real FINAM
order endpoint calls.

## Scope

M3d-2a adds a loopback-only test harness that proves the future exact-two-route
endpoint boundary can observe the actual HTTP wire shape before a real endpoint
implementation is reviewed:

- place order: `POST /v1/accounts/{account_id}/orders`;
- cancel order: `DELETE /v1/accounts/{account_id}/orders/{order_id}`;
- authorization header presence and length only;
- JSON body shape, body length, and body SHA256 only;
- rendered account id, order id, token, and raw body are not exported in
  diagnostics.

The raw socket write/read helpers live only under `#[cfg(test)]`. The public
module exports redacted classification diagnostics and design route templates,
not a production FINAM transport.

## Safety boundary

Still not allowed in M3d-2a:

- real FINAM `POST /orders`;
- real FINAM `DELETE /orders/{order_id}`;
- scanner allowlist mode;
- constructible `EndpointGateApproved`;
- command consumer connected to strategies;
- live ACK lifecycle;
- runtime/live attachment or `LiveReady`;
- SLTP, brackets, replace, or multi-leg orders.

The existing `crates/finam-gateway/src/real_order_endpoint.rs` transition
scanner remains unchanged and continues to require design-only source with no
HTTP send surface.

## Reproducible checks

After creating a clean handoff archive, generate evidence with:

```bash
python3 scripts/m3d2a_local_mock_endpoint_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

The evidence report is written to:

```text
reports/m3d-protected-endpoint/m3d2a-local-mock-evidence.json
reports/m3d-protected-endpoint/m3d2a-local-mock-evidence.json.sha256
```

Required green checks:

- `cargo fmt --all --check`;
- `cargo test --all`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- `bash scripts/forbidden_surface_scan.sh`;
- `bash scripts/forbidden_surface_negative_harness.sh`;
- `bash scripts/order_endpoint_scanner_transition_spec.sh`;
- `bash scripts/redis_shadow_smoke.sh`;
- `bash scripts/runtime_bridge_dry_smoke.sh`.

## M3d-2b strict contract hardening

M3d-2b extends the M3d-2a route/redaction harness into a strict FINAM
plain-order contract harness. It still does not add real FINAM `POST` or
`DELETE` transport.

Request hardening:

- `symbol` must use the pinned broker-symbol shape used by the instrument
  registry policy;
- `quantity.value` must be a decimal-like string;
- `side` must be `SIDE_BUY` or `SIDE_SELL`;
- `type` must be `ORDER_TYPE_MARKET` or `ORDER_TYPE_LIMIT`;
- `time_in_force` must be present and one of the plain-order allowed FINAM
  values;
- `client_order_id` must be present and FINAM-safe;
- LIMIT requires `limit_price.value`;
- MARKET rejects `limit_price`;
- `stop_price`, `stop_condition`, `legs`, `sltp`, `valid_before`, and unknown
  plain-order fields are rejected.

Renderer binding:

- positive place/cancel tests go through preflight approval and the existing
  `broker-finam` request builders before local mock classification;
- hand-written raw strings remain only as negative/wire fixtures.

Response matrix:

- accepted with broker order id;
- accepted without broker order id;
- malformed 2xx body;
- 400 reject;
- 401 unauthorized;
- cancel 404/409/410 reconciliation-required;
- 429 rate limit;
- 500/503 maintenance;
- 504 and local timeout;
- body read failure / closed connection.

Generate M3d-2b evidence with:

```bash
python3 scripts/m3d2b_strict_contract_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

## M3d-2c real transport behind gate, local mock only

M3d-2c introduces the first real `reqwest` order endpoint transport source, but
keeps it disabled by default and impossible to use in production without
`EndpointGateApproved`.

Transport scope:

- only `POST /v1/accounts/{account_id}/orders`;
- only `DELETE /v1/accounts/{account_id}/orders/{order_id}`;
- only existing `FinamPlaceOrderRequestSpec` and `FinamCancelOrderRequestSpec`;
- no SLTP, bracket, replace, or multi-leg routes.

Authorization policy:

- pinned as `Authorization: Bearer <jwt>`;
- diagnostics export only policy/scheme/length facts, never raw JWT;
- policy source is FINAM REST documentation / llms reference:
  `https://api.finam.ru/docs/rest/llms.txt`.

Post-send semantics:

- PLACE 2xx with broker id -> submitted;
- PLACE 2xx without broker id -> submitted pending broker id reconciliation;
- malformed 2xx / body-read-failed after send -> reconciliation required;
- timeout / 504 -> timeout unknown pending;
- CANCEL accepted remains pending reconciliation, never terminal canceled.

The transport is tested only against a loopback local mock server. The test-only
gate constructor is behind `#[cfg(test)]`; production
`EndpointGateApproved::try_from_decision()` remains blocked by
`REAL_ORDER_ENDPOINT_IMPLEMENTATION_REVIEW_ACCEPTED = false`.

Generate M3d-2c evidence with:

```bash
python3 scripts/m3d2c_real_transport_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

## M3d-2d durable real-transport lifecycle, local mock only

M3d-2d binds the M3d-2c real `reqwest` transport to the durable order-path
state machine and SQLite transition audit. This is still not live trading:
every transport test uses a loopback local mock server, while production
`EndpointGateApproved` remains unconstructible.

Lifecycle scope:

- PLACE persists `IntentRecorded` and then `BeginSubmit` before the HTTP send;
- CANCEL persists `RequestCancel` before the HTTP send;
- successful PLACE with broker id becomes `Submitted`;
- PLACE 2xx without broker id becomes `SubmittedPendingBrokerOrderId` and
  schedules reconciliation;
- timeout and ambiguous post-send outcomes become unknown-pending /
  reconciliation-required, with no blind retry;
- CANCEL accepted is only a submitted cancel candidate pending broker-truth
  reconciliation, never terminal canceled;
- cancel conflict / 404-like / ambiguous outcomes require broker-truth
  reconciliation or manual intervention.

The exported report is redacted: it exposes only ACK status/reason and id
presence/length diagnostics. Raw JWT, path, body, account id, client order id,
and broker order id are not exported from the review report.

Crash-window tests cover:

- crash before send: durable intent remains `IntentRecorded`;
- crash after `BeginSubmit` before send: restart recovers conservatively to
  `TimeoutUnknownPending`;
- crash after send before classified outcome persistence: restart also recovers
  conservatively, without retrying the order;
- crash after classified outcome before external ACK publication: durable
  terminal/intermediate state is already recoverable from SQLite.

Additional scanner/evidence hardening:

- exact send surface remains unchanged: one `.post(`, one `.delete(`, one
  `.send(` in the reviewed M3d-2c transport module;
- direct `EndpointGateApproved { _private: () }` construction is forbidden by
  scanner;
- tests must pass dynamic loopback mock base URLs.

Generate M3d-2d evidence with:

```bash
python3 scripts/m3d2d_lifecycle_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

M3d-2d explicitly still does not allow:

- real external FINAM order endpoint calls;
- command consumer connected to strategies;
- runtime/live attachment or `LiveReady`;
- SLTP, bracket, replace, or multi-leg order surfaces;
- blind retry after timeout or ambiguous post-send outcome.

## M3d-2e protected endpoint closure

M3d-2e closes the protected M3d-2 endpoint stage before any M3e command
consumer work. It does not add command consumption or live trading. It adds
two final hardening pieces:

1. A lifecycle outcome matrix proving durable state/ACK/reconciliation mapping
   for the required PLACE and CANCEL endpoint outcomes.
2. An explicit external endpoint enablement firewall around the real transport
   config.

Lifecycle matrix coverage:

- PLACE accepted with broker id -> `Submitted`;
- PLACE accepted without broker id -> `SubmittedPendingBrokerOrderId` +
  reconciliation;
- PLACE 400 -> `BrokerRejected`, no retry;
- PLACE 401 / 429 / 500 / 503 -> manual/disarm class, no blind retry;
- PLACE malformed 2xx / body read failure -> reconciliation required;
- PLACE timeout / 504 -> `TimeoutUnknownPending`;
- PLACE send error after possible send -> reconciliation required.

CANCEL matrix coverage:

- CANCEL 200 / 202 / 204 accepted -> `CancelSubmitted`, reconciliation
  scheduled;
- CANCEL accepted with same broker id -> `CancelSubmitted`;
- CANCEL accepted with different broker id -> manual intervention;
- CANCEL 404 / 409 / 410 -> reconciliation/manual path;
- CANCEL 401 / 429 / 503 -> manual/disarm class, no blind retry;
- CANCEL timeout / 504 -> `CancelTimeoutUnknownPending`;
- CANCEL decode / body read failure -> reconciliation required.

External endpoint firewall:

- `M3d2RealOrderEndpointTransportConfig::default()` remains
  `https://api.finam.ru`, but its default endpoint mode is `LocalMockOnly`, so
  constructing the real transport with default config is blocked.
- Loopback local mock URLs are allowed only through `LocalMockOnly`.
- Explicit `ExternalFinamDisabled` blocks `api.finam.ru`.
- `FutureExternalFinamRequiresLiveGate` is still blocked in M3d-2e.
- Scanner/evidence forbid real transport construction/default config usage
  outside approved test modules.
- The gateway/command-consumer surface does not instantiate the real transport.

Generate M3d-2e evidence with:

```bash
python3 scripts/m3d2e_closure_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

If the evidence is green, M3d-2 is closed as a protected local-mock endpoint
stage. Even then, M3e must still remain non-live: no external FINAM order calls,
no strategy runtime attachment, no `LiveReady`, and no stop/SLTP/bracket.
