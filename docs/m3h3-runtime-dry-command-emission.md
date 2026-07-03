# M3h-3 runtime dry command emission

M3h-3 adds a runtime-shadow dry command emitter between the M3h runtime decision
tick and the M3e command stream. It does not attach live strategies and does
not call FINAM. The boundary is:

```text
M3h StrategyDecisionTick -> approved dry candidate -> Envelope<BrokerCommand> -> M3e command stream
```

Emission requirements:

- the input must be a `StrategyDecisionTick`;
- runtime dry readiness must be `DryReady` and `live_ready = false`;
- the candidate must keep runtime live attachment, `LiveReady`, external order
  endpoints, and real FINAM endpoints disabled;
- only plain market/limit `PlaceOrder` commands are accepted in this stage;
- cancel, stop/SLTP/bracket/replace/multi-leg, comments, and live boundaries are
  rejected;
- request-id idempotency prevents a duplicate `request_id` from publishing a
  second command envelope.

Lifecycle behavior:

- `PendingEmission` is recorded before the Redis publish attempt;
- successful publication becomes `PublishedToM3eCommandStream`;
- blocked or failed emission becomes `NotEmitted`;
- publish failure preserves an explicit lifecycle record with
  `PublishFailed`.

Safety boundary:

- command goes only to the M3e command stream;
- command is serialized as broker-neutral `Envelope<BrokerCommand>`;
- no runtime live attachment;
- no `LiveReady`;
- no external FINAM `POST / DELETE`;
- no command-consumer-to-real-FINAM transport;
- no stop/SLTP/bracket/replace/multi-leg.

Evidence:

```bash
python3 scripts/m3h3_runtime_dry_command_emission_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
