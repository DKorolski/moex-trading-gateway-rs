# M4-3k-a readiness HTTP semantics strictness

M4-3k-a hardens the ALOR ↔ FINAM observability parity comparator.

## Problem closed

The M4-3k comparator originally treated this as acceptable:

```text
ReadinessPhase != LiveReady
HTTP status = 200
```

That is unsafe for a future systemd/supervisor health input. The listener itself
was already protected, but the parity model also needs to reject a malformed
surface.

## Strict rule

The comparator now requires:

```text
ReadinessPhase::LiveReady -> HTTP 200
any other readiness phase -> HTTP 503
```

## Negative tests

M4-3k-a adds explicit regression tests:

```text
Reconciliation + HTTP 200 -> parity fails on ReadinessHttpStatusRule
LiveReady      + HTTP 503 -> parity fails on ReadinessHttpStatusRule
```

## Boundary

No trading boundary changed:

```text
live_orders_performed = false
post_delete_calls_performed = false
runtime_live_attachment_allowed = false
command_consumer_to_real_finam_enabled = false
```
