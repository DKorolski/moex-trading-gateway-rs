# M4-3g FINAM WS generation + subscription confirmation

M4-3g adds a source-only observability contract for FINAM WebSocket shadow runs.

The goal is to move FINAM closer to the mature ALOR gateway lifecycle model:

- every WS connection/iteration has a `ws_generation_id`;
- desired subscriptions are explicit;
- subscriptions become active only after `EVENT` or first `DATA`;
- unconfirmed desired subscriptions become timeout-degraded when the run ends by duration;
- old-generation messages are counted and kept out of strategy emission.

No live orders are enabled. No POST/DELETE order endpoints are added. Runtime-live remains disabled.

## Report contract

`finam-ws-shadow-*` JSON reports now include:

```text
ws_generation.ws_generation_id
ws_generation.desired_subscriptions
ws_generation.active_subscriptions
ws_generation.pending_subscriptions
ws_generation.timeout_degraded_subscriptions
ws_generation.old_generation_message_count
ws_generation.confirmations[]
```

Each confirmation has:

```text
subscription_type
desired
active
pending
status
confirmation_source
```

Allowed statuses:

```text
Disabled
Pending
EventConfirmed
DataConfirmed
TimeoutDegraded
```

## Confirmation rules

For each desired subscription:

```text
DATA > EVENT > Pending/TimeoutDegraded
```

`DATA` confirmation is stronger than `EVENT`, because it proves that the stream is delivering the requested data shape.

If the run stops by `max_duration` while a desired subscription has no `EVENT` and no `DATA`, the subscription is reported as `TimeoutDegraded`.

## Data-quality interaction

M4-3f introduced:

```text
received = emitted + dropped + ignored + pending
```

M4-3g keeps that invariant and adds first-class reason bucket visibility:

```text
DroppedStaleBacklog
DuplicateOrOld
NonMonotonic
UnknownSource
PendingForming
OldGeneration
```

`OldGeneration` is tracked separately because a stale WS generation should never become strategy-eligible market data.

## Boundary

This is still a source-only stage:

```text
live_orders_performed = false
post_delete_calls_performed = false
runtime_live_attachment_allowed = false
command_consumer_to_real_finam_enabled = false
continuous_runtime_live_enabled = false
```

M4-3g does not close the active-session runtime evidence slot. It prepares the observable contract required for that evidence.
