# Stage 5B-2b — broker-neutral wrapper mechanical import

Status: mechanical import accepted; boundary-hardening review candidate.

Date: 2026-07-11.

## Outcome

The accepted `6e15ab1b...` Hybrid runtime oracle is now compiled as the single
approved `strategy-runtime-core::hybrid_intraday_runtime` library module.
Trading formulas, constants, BO/MR/high180 orchestration, riskgate decisions,
timeout clocks and state transitions remain source-derived.

Mechanical migration changes are limited to:

- `Uuid` request identities -> `broker_core::StrategyRequestId`;
- numeric ALOR order/exchange ids -> `broker_core::BrokerOrderId`;
- stop-order strings -> `broker_core::BrokerStopOrderId`;
- host/protocol compatibility types moved behind a broker-neutral local seam;
- the six Stage 5B-2 callback contracts exposed through
  `BrokerNeutralHybridStrategy` without attaching a runtime host.

The imported oracle test matrix is compiled and executed. Additional adapter
tests freeze account namespace, strategy/event clocks, final M10 admission and
`TradingWindowClosed` deferred-entry behavior.

## Safety boundary

This stage is paper/no-send only:

- runtime host attachment: false;
- FINAM command consumer attachment: false;
- strategy-driven real orders: false;
- runtime `LiveReady` enablement: false;
- real POST/DELETE expansion: false;
- real Stop/SLTP/bracket execution: false.

The source-compatible host seam is crate-private. The public callback adapter
returns typed `Result<Vec<BrokerNeutralHybridIntent>,
HybridRuntimeCallbackValidationError>` values in memory. It validates the
context instrument, payload instrument, configured target symbol and canonical
final M10 admission before entering any source callback. No component publishes
accepted intents to Redis, a gateway or a broker.

## Correspondence

Normative source remains:

```text
source-oracles/alor-stage5/hybrid_intraday_runtime.rs
SHA256 6e15ab1b7212c56d3ecd8397b2d8991c1feccbde8eaa5e3d0051aec82a55f0aa
```

Compiled target:

```text
crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs
```

The source-region hashes remain unchanged in
`stage5b2-source-correspondence.toml`; target adaptations are classified only
as `BrokerNeutralTypeMigration` or `HostBoundaryExtraction`.

## Next gate

Stage 5C must review runtime-host policy and generic callback seams separately.
It may not infer live authorization from this compile milestone.
