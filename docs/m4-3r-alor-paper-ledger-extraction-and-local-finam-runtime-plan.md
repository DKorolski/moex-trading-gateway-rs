# M4-3r ALOR paper/ledger extraction and local FINAM runtime integration plan

Status: accepted work plan / no-send / no live orders.

M4-3r changes the next step after M4-3q. Instead of treating the FINAM
paper-shadow stand as a loose harness, the new contour must first absorb the
mature ALOR paper/replay/ledger semantics into broker-neutral runtime contracts.

## Current operational boundary

- ALOR `<ALOR_LIVE_ORACLE_PORTFOLIO>` remains the live oracle.
- ALOR is not flat, so no FINAM live/order-path work is allowed.
- FINAM WS shadow may be used only as market-data input.
- No FINAM `POST /orders`.
- No FINAM `DELETE /orders/{id}`.
- No command-consumer-to-real-FINAM.
- No runtime `LiveReady`.
- No Stop/SLTP/bracket/replace/multi-leg.

The M4-3q FINAM WS once check proved that the gateway-side market-data contour
can connect and write to `finam_imoexf_paper:*`, but runtime state and intents
remain empty until the executable paper runtime adapter exists.

## Why this route is better

The remaining parity risk is no longer only market data. The hard part is:

- strategy warmup and state restore;
- decision/no-decision parity;
- intent idempotency;
- paper execution semantics;
- paper order/trade/position ledger;
- risk-gate ledger/state;
- health/readiness/snapshot surfaces.

ALOR already solved much of this in `strategy-runtime`; FINAM should reuse the
semantics through broker-neutral contracts instead of inventing a separate
gateway-local paper mode.

## ALOR sources to extract

Primary source files:

```text
strategy-runtime/src/strategy_host.rs
strategy-runtime/src/runtime.rs
strategy-runtime/src/redis_transport.rs
strategy-runtime/src/trade_ledger.rs
strategy-runtime/src/risk_gate_store.rs
strategy-runtime/src/state.rs
strategy-runtime/src/config.rs
strategy-runtime/src/strategies/hybrid_intraday_runtime.rs
```

Primary configs:

```text
configs/runtime.hybrid.paper.canary.<ALOR_CANARY_PORTFOLIO>.toml
configs/runtime.hybrid.paper.<ALOR_PAPER_PORTFOLIO>.toml
configs/runtime.hybrid.live.<ALOR_LIVE_ORACLE_PORTFOLIO>.riskgate-shadow.toml
```

Primary docs:

```text
docs/live-runtime-service-patterns-anti-regression-checklist-2026-05-07.md
docs/imoexf-primary-runtime-integration-review-handoff-2026-04-26.md
docs/redis-runtime-state-and-snapshots.md
```

## Broker-neutral contracts to add

M4-3r should introduce broker-neutral paper/runtime contracts, not FINAM-specific
strategy logic:

```text
PaperIntent
PaperOrder
PaperTrade
PaperPosition
PaperAck
PaperLedgerSnapshot
PaperRuntimeState
RuntimeBarInput
RuntimeDecisionRecord
RuntimeSuppressionRecord
RiskGatePaperLedgerRecord
RiskGatePaperState
```

The FINAM gateway remains responsible for:

- market data;
- broker truth read-only;
- readiness/health;
- broker-neutral event publication.

The runtime/paper adapter is responsible for:

- consuming canonical final 10-minute strategy bars;
- warming/restoring strategy state;
- producing paper intents;
- applying deterministic paper acks/fills;
- maintaining paper positions/trades/orders;
- publishing runtime state and paper ledger snapshots.

## Strategy-facing input rules

Only final canonical 10-minute bars may reach the strategy:

```text
bar_source_mode       = FinamDerivedM1ToM10
source_timeframe_sec  = 60
target_timeframe_sec  = 600
aggregation_complete  = true
gap_absence_proven    = true
source_kind           = LiveStream
```

Reject:

- raw FINAM M1;
- FINAM-native M10 while characterization is pending;
- historical/read-only bars as live decision input;
- recovery bars unless they are explicitly classified as warmup-only;
- stale/degraded market data;
- duplicate decision ids.

History/replay bars may warm indicators and rebuild state, but must not emit
paper/live broker actions unless the execution mode explicitly says
`history_sim`.

## Paper fill policy

The first implementation must document and enforce one deterministic policy:

```text
market paper entry/exit = next final 10m bar open proxy
limit paper order       = working until deterministic bar-touch rule
cancel paper order      = terminal canceled if still working
```

No implicit broker fill assumptions are allowed. Every paper fill must explain:

- source decision id;
- source bar bucket;
- fill policy;
- paper order id;
- resulting paper position delta.

## Riskgate parity

The IMOEXF current ALOR baseline uses:

```text
profile       = imoexf_primary_riskgate_high180_lb120
mr_variant    = high180
mr_gate_policy = shadow_pnl_lb120_positive
risk_gate_mode = normal_append
```

`normal_append` is not enforced filtering. M4-3r must preserve this:

- maintain or import a paper risk-gate ledger;
- expose materialized risk-gate state;
- keep enforced MR blocking disabled;
- compare against ALOR `runtime.riskgate.sessions.hybrid_imoexf.imoexf_primary_high180_lb120`.

Long history belongs in the ledger, not in the strategy snapshot.

## Local integration target

Before VPS deployment, run locally:

```text
FINAM WS shadow
  -> finam_imoexf_paper:ws:market_data
  -> canonical M1-to-M10 publisher
  -> finam_imoexf_paper:md:bars:10m
  -> IMOEXF hybrid paper runtime adapter
  -> finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf
  -> finam_imoexf_paper:runtime:intents
  -> finam_imoexf_paper:runtime:paper_acks
  -> finam_imoexf_paper:runtime:orders_paper_only
  -> finam_imoexf_paper:runtime:trades_paper_only
  -> finam_imoexf_paper:runtime:positions_paper_only
```

Local success means:

- runtime state is populated;
- paper ledger streams are populated;
- health/readiness/snapshot surfaces are populated;
- restart restores/dedupes state;
- no live/order surface is enabled.

## ALOR comparison target

Compare local FINAM paper runtime against the live ALOR oracle:

```text
md.bars.<ALOR_LIVE_ORACLE_PORTFOLIO>.10m
runtime.state.hybrid_intraday.live.riskgate_shadow.imoexf.<ALOR_LIVE_ORACLE_PORTFOLIO>
cmd.orders.<ALOR_LIVE_ORACLE_PORTFOLIO>
cmd.acks.<ALOR_LIVE_ORACLE_PORTFOLIO>
broker.trades.<ALOR_LIVE_ORACLE_PORTFOLIO>
broker.positions.<ALOR_LIVE_ORACLE_PORTFOLIO>
broker.snapshots.<ALOR_LIVE_ORACLE_PORTFOLIO>
runtime.riskgate.sessions.hybrid_imoexf.imoexf_primary_high180_lb120
```

The comparison should cover:

- health/readiness shape;
- runtime state shape;
- strategy owner/sleeve/side/qty/reason;
- no-decision/suppression reasons;
- paper positions/trades/orders;
- riskgate ledger/materialized state;
- no unexplained one-bar drift.

## Freeze replay dataset

The freeze replay dataset is optional for strategy logic parity because the
strategy is not being changed. It is still useful as a smoke/regression suite
for the new broker-neutral runtime adapter:

- serialization/deserialization;
- warmup/replay vs live-decision boundary;
- idempotency;
- ledger restore;
- riskgate bootstrap/normal append behavior.

Do not make freeze replay a blocker for the first local FINAM paper runtime if
active-session ALOR-vs-FINAM paper parity is clean.

## VPS promotion gate

Only after local success:

1. package clean handoff;
2. review acceptance;
3. deploy paper-shadow runtime to VPS;
4. keep ALOR live as oracle;
5. compare for 1-2 days;
6. no FINAM live orders.

Cutover/live remains blocked until a separate pre-live gate is reviewed and
accepted.
