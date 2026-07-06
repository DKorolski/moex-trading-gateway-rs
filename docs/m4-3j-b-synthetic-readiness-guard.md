# M4-3j-b synthetic LiveReady guard

M4-3j-b hardens the local HTTP/debug listener before it can be used as an
operational/systemd health input.

## P1 closed

The normal `finam-local-debug-http` CLI no longer exposes a `--live-ready`
operator flag. The listener's default readiness remains:

```text
ReadinessPhase::Reconciliation
reason = OperatorLiveArmMissing
HTTP /readiness = 503
HTTP /debug/transport = 503
```

Synthetic `LiveReady` can only be constructed through an internal test helper.
When it is constructed, the response is explicitly marked:

```text
synthetic_readiness = true
not_for_systemd_readiness = true
```

This prevents an operator or systemd unit from accidentally treating synthetic
readiness as real FINAM readiness.

## Real WS-bound debug report

FINAM WS shadow reports now include a broker-neutral debug surface generated from
the actual `FinamWsShadowIterationReport`:

```text
broker_neutral_debug_surface
```

This report binds `/debug/transport`-compatible data to real WS-loop state:

- WebSocket generation;
- desired/active/pending subscriptions;
- data-quality ledger;
- recovery phase/blockers;
- session watchdog;
- readiness phase/reasons;
- no runtime-live attachment;
- no command-consumer-to-real-FINAM;
- no order POST/DELETE.

## Boundary

M4-3j-b does not add trading capabilities:

```text
live_orders_performed = false
post_delete_calls_performed = false
runtime_live_attachment_allowed = false
command_consumer_to_real_finam_enabled = false
```
