# M4-3j-a local HTTP/debug listener

M4-3j-a turns the accepted M4-3j design shape into a local debug listener.

Scope:

- bind is local by default: `127.0.0.1`;
- optional private RFC1918 bind requires an explicit flag;
- public bind addresses are rejected;
- only GET routes are exposed;
- no FINAM order POST/DELETE is added;
- no live orders are performed;
- runtime-live attachment remains disabled;
- command-consumer-to-real-FINAM remains disabled;
- Stop/SLTP/bracket/replace/multi-leg remains blocked.

## Routes

```text
GET /liveness
GET /readiness
GET /debug/transport
```

`/readiness` follows the ALOR-compatible status rule:

```text
ReadinessPhase::LiveReady -> HTTP 200
any other phase           -> HTTP 503
```

The safe default response is still:

```text
ReadinessPhase::Reconciliation
reason = OperatorLiveArmMissing
HTTP status = 503
```

## Debug transport payload

`/debug/transport` exposes redacted broker-neutral transport state:

- transport connected/authorized;
- WebSocket generation identifier;
- desired/active/pending subscription counts;
- data-quality ledger;
- recovery phase/blockers;
- session watchdog state;
- `runtime_live_attachment_allowed = false`;
- `command_consumer_to_real_finam_enabled = false`;
- `order_post_delete_allowed = false`.

It must not expose raw secrets, raw tokens, raw account IDs, raw broker payloads,
or raw request/response bodies.

## Operator command

```bash
cargo run -p broker-cli -- \
  finam-local-debug-http \
  --bind 127.0.0.1:18081 \
  --max-requests 3
```

This is a diagnostic listener, not a trading service readiness grant.
