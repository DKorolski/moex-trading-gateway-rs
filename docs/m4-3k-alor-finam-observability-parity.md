# M4-3k ALOR ↔ FINAM observability parity report

M4-3k compares the ALOR gateway observability surface and the FINAM gateway
observability surface through a broker-neutral schema.

This step is report/contract only:

- no FINAM order POST/DELETE;
- no live orders;
- no runtime-live attachment;
- no command-consumer-to-real-FINAM;
- no Stop/SLTP/bracket/replace/multi-leg.

## Parity target

ALOR source:

```text
GET /liveness
GET /readiness
GET /debug/cws
```

FINAM source:

```text
GET /liveness
GET /readiness
GET /debug/transport
```

`/debug/cws` and `/debug/transport` are compared through neutral capability
buckets instead of broker-specific field names.

## Critical capability buckets

```text
LivenessRoute
ReadinessRoute
DebugTransportRoute
ReadinessHttpStatusRule
TransportConnected
WsGeneration
SubscriptionCounts
DataQualityLedger
RecoveryState
SessionWatchdog
RedactedDebug
RuntimeLiveDisabledFlag
CommandConsumerToRealBrokerDisabledFlag
OrderPostDeleteDisabledFlag
```

The report is accepted only when every critical bucket has parity.

## Output

The canonical report schema is:

```text
m4_3k_alor_finam_observability_parity
```

It contains:

- ALOR source shape;
- FINAM source shape derived from `BrokerNeutralHttpDebugSurfaceReport`;
- per-capability parity items;
- `critical_missing_count`;
- `parity_ok`;
- no-live/no-order boundary flags;
- next-stage recommendation.

## Next stage

After M4-3k is accepted, the next natural step is runtime shadow parity:

```text
M4-3l dry runtime attach / M1-M10 parity, no-live
```

That should compare actual strategy-facing behavior, while M4-3k only closes the
observability surface parity contract.
