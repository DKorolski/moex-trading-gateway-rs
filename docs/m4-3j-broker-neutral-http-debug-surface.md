# M4-3j broker-neutral HTTP/debug health surface

M4-3j defines the FINAM-side broker-neutral shape for ALOR-style health/debug
observability.

This step is design/report-only:

- no actual HTTP listener is started;
- no runtime-live attachment is enabled;
- no command-consumer-to-real-FINAM transport is enabled;
- no FINAM order POST/DELETE is added;
- no Stop/SLTP/bracket/replace/multi-leg behavior is enabled.

## ALOR parity target

The ALOR gateway exposes:

```text
GET /liveness
GET /readiness
GET /debug/cws
```

The FINAM broker-neutral shape maps this to:

```text
GET /liveness
GET /readiness
GET /debug/transport
```

`/debug/transport` is intentionally broker-neutral. For FINAM it can describe
WebSocket market-data, read-only HTTP, Redis publication, and future command
consumer state without exposing raw secrets, raw tokens, raw account IDs, raw
broker payloads, or raw request bodies.

## Readiness HTTP status contract

The status mapping follows the ALOR operational convention:

```text
ReadinessPhase::LiveReady -> HTTP 200
any other readiness phase -> HTTP 503
```

This does not grant live trading. In the current FINAM shadow path the expected
state is still typically `Reconciliation / OperatorLiveArmMissing`, therefore
HTTP readiness remains `503` even when market data itself is healthy.

## Boundary

The M4-3j report must keep these invariants true:

```text
design_only = true
actual_http_server_enabled = false
no_live_boundary_expansion = true
no_order_post_delete = true
no_command_consumer_to_real_broker = true
route.enabled = false for all route shapes
debug_transport.redacted = true
raw_secrets_exported = false
raw_tokens_exported = false
raw_account_ids_exported = false
```

Actual HTTP server wiring is a later implementation step and should require a
separate review. M4-3j only fixes the canonical route/response contract so FINAM
and ALOR can converge on the same observability surface.
