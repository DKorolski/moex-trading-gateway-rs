# M4-3q FINAM IMOEXF hybrid paper-shadow stand

Status: stand conversion plan / source-only / no live orders.

M4-3q introduces a dedicated FINAM paper-shadow stand for the IMOEXF hybrid
strategy. The purpose is to run the new FINAM market-data contour next to the
current ALOR live oracle and compare strategy-facing behavior before any runtime
cutover.

## Why this is a separate stand

The existing ALOR diagnostic stand from M4-3n is still useful: it proves that
strict ALOR M1-to-M10 assembly can be compared against the production ALOR
native 10-minute stream. M4-3q does not replace that stand.

Instead, M4-3q adds a FINAM-specific paper-shadow contour:

```text
FINAM WS M1 bars
  -> CanonicalBarAggregator(target = 600s)
  -> FINAM-derived final 10m bars
  -> IMOEXF hybrid paper-shadow runtime boundary
  -> paper intents / paper acks / paper runtime state
  -> parity comparison against ALOR live oracle
```

## Config

The stand configuration is:

```text
config/finam-imoexf-hybrid-paper-shadow.vps.example.json
```

The executable FINAM WS shadow runner config for the first no-send connection
check is:

```text
config/finam-imoexf-ws-shadow-paper.vps.example.json
```

It uses isolated Redis streams under:

```text
finam_imoexf_paper:*
```

This namespace must not overlap with the production ALOR streams:

```text
md.bars.<ALOR_LIVE_ORACLE_PORTFOLIO>.10m
cmd.orders.<ALOR_LIVE_ORACLE_PORTFOLIO>
cmd.acks.<ALOR_LIVE_ORACLE_PORTFOLIO>
broker.*
runtime.state.hybrid_intraday.live.riskgate_shadow.imoexf.<ALOR_LIVE_ORACLE_PORTFOLIO>
runtime.riskgate.sessions.hybrid_imoexf.imoexf_primary_high180_lb120
```

## Strategy contract

The strategy target mirrors the current ALOR live baseline:

```text
strategy_id   = hybrid_imoexf
strategy_kind = hybrid_intraday
profile       = imoexf_primary_riskgate_high180_lb120
symbol        = IMOEXF
qty           = 3
```

The FINAM stand must not use raw 1-minute bars as strategy input. Strategy-facing
bars are accepted only after canonical M1-to-M10 aggregation with sidecar
provenance:

```text
bar_source_mode       = FinamDerivedM1ToM10
source_timeframe_sec  = 60
target_timeframe_sec  = 600
aggregation_complete  = true
gap_absence_proven    = true
```

FINAM-native M10 bars remain blocked for strategy-facing input until separately
characterized.

## Riskgate contract

The current ALOR IMOEXF contour uses:

```text
mr_variant       = high180
mr_gate_policy   = shadow_pnl_lb120_positive
risk_gate_mode   = normal_append
```

`normal_append` is a shadow/ledger mode, not enforced filtering. The FINAM paper
stand must preserve that distinction:

- shadow risk-gate ledger/state may be built and compared;
- enforced MR gating remains disabled;
- risk-gate state is part of runtime parity, not a live-order permission.

## ALOR oracle

ALOR remains the live operational oracle while it has active positions. M4-3q is
read-only against ALOR and uses `XRANGE`/`XREVRANGE`, not `XREADGROUP`.

The relevant ALOR streams are:

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

The current live case discovered on 2026-07-06 is useful parity input:

```text
IMOEXF short -3
owner = intraday_breakout
market sell qty = 3
fill price = 2194.0
```

It must be treated as oracle evidence only; it does not authorize FINAM live
orders.

## Boundary

M4-3q must not:

- send FINAM `POST /orders`;
- send FINAM `DELETE /orders/{id}`;
- enable command-consumer-to-real-FINAM;
- emit runtime `LiveReady`;
- attach continuous live runtime;
- enable Stop/SLTP/bracket/replace/multi-leg;
- write to production ALOR streams;
- consume ALOR Redis with consumer groups;
- cut over automatically from ALOR to FINAM.

Allowed:

- run FINAM WS shadow;
- publish FINAM shadow market data to `finam_imoexf_paper:*`;
- assemble final 10-minute bars from FINAM M1;
- run paper/shadow strategy boundary once implemented;
- emit paper-only intents/acks/state into isolated FINAM streams;
- compare FINAM paper outputs against ALOR read-only oracle.

## Operator run order

1. Keep ALOR live contour untouched.
2. Start FINAM WS shadow with the M4-3q executable WS config namespace:

   ```bash
   cargo run -p broker-cli -- \
     finam-ws-shadow-once \
     --config config/finam-imoexf-ws-shadow-paper.vps.example.json \
     --secret-env FINAM_SECRET_TOKEN \
     --symbol IMOEXF@RTSX \
     --timeframe TIME_FRAME_M1 \
     --subscribe-bars \
     --max-messages 20 \
     --max-duration-seconds 60
   ```

3. Wait for fresh final M1 bars and gap-safe recovery.
4. Build/publish canonical final 10-minute bars.
5. Run IMOEXF hybrid paper-shadow runtime boundary, no live orders.
6. Compare:
   - ALOR native 10m vs FINAM derived 10m;
   - ALOR runtime decision/state vs FINAM paper runtime decision/state;
   - ALOR riskgate shadow ledger/state vs FINAM paper riskgate state;
   - emitted ALOR command shape vs paper-only FINAM intent shape.

## Acceptance

The stand is acceptable when:

- config is valid JSON;
- all FINAM streams are isolated under `finam_imoexf_paper:*`;
- live/order surfaces are explicitly disabled;
- canonical M1-to-M10 provenance is required;
- raw M1 and FINAM-native M10 strategy input are blocked;
- riskgate mode is shadow/normal-append, not enforced;
- ALOR oracle streams are read-only references;
- no order endpoint or command-consumer-to-real-FINAM is introduced.
