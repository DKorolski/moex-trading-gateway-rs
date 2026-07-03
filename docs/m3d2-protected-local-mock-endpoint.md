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

