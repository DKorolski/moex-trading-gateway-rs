# M4-2d Canonical BrokerTruthSnapshot integration into M4 preflight/evidence

Status: canonical integration package only. No live calls.

M4-2d follows the accepted M4-2c operational parity specification. The goal is
to stop relying on ad-hoc M4 local counters where canonical broker truth is
available, and to enrich FINAM read-only mapping so that trades, instrument
specification, and readiness can participate in the same broker-neutral model.

## Trading boundary

This stage must not:

- send live orders;
- open or close a position;
- enable runtime live;
- connect command-consumer to real FINAM;
- enable Stop/SLTP/bracket/replace/multi-leg;
- authorize additional live position tests.

Live expansion remains blocked after M4-2d.

## Implemented scope

### FINAM canonical aggregate mapper

`broker-finam` now exposes an enriched aggregate mapper:

```rust
map_finam_broker_truth_snapshot_with_readonly_artifacts(
    account,
    orders,
    trades,
    instruments,
    received_ts,
) -> Result<BrokerTruthSnapshot, FinamMapperError>
```

It preserves the existing M4-2b function:

```rust
map_finam_broker_truth_snapshot(account, orders, received_ts)
```

as a compatibility wrapper.

The enriched mapper adds:

- FINAM account trades -> `BrokerTradeSnapshot`;
- FINAM asset + asset_params + schedule -> `BrokerInstrumentSpec`;
- account cash/positions/orders remain mapped through the canonical
  `BrokerTruthSnapshot` aggregate.

### FINAM canonical readiness mapper

`broker-finam` now exposes:

```rust
map_finam_broker_readiness_snapshot(...)
```

It produces `BrokerReadinessSnapshot` from read-only artifacts:

- account freshness;
- positions freshness;
- orders freshness;
- trades freshness;
- quote freshness;
- instrument spec freshness;
- schedule freshness;
- `BrokerMarketSessionState` from FINAM schedule/session;
- unknown order count;
- cash/margin presence;
- instrument spec validation flag.

### M4-1c preflight/evidence canonical migration

`broker-cli` M4-1c tiny position market path now derives the pre-boundary flat
and active-order truth from:

- `map_finam_broker_truth_snapshot(...)`;
- `BrokerTruthSnapshot::summarize_for_instrument(...)`;
- `BrokerTruthSnapshot::target_is_flat(...)`;
- `BrokerTruthSnapshot::target_position_qty(...)`.

The old JSON fields such as `positions_count` and `active_orders_count` remain
for report compatibility, but the report now includes:

```json
"truth_source": "BrokerTruthSnapshot",
"canonical_summary": { ... }
```

The M4-1c evidence script now requires this canonical truth source before it can
mark evidence ready.

## Tests added

`broker-finam` includes M4-2d tests for:

1. enriched `BrokerTruthSnapshot` includes trades and instrument spec;
2. FINAM schedule/session maps to `BrokerReadinessSnapshot.market_session`;
3. canonical readiness/freshness is fresh for the synthetic read-only artifact
   set;
4. same ticker but different MIC is not the same instrument;
5. buy/sell trade pair explains a flat round-trip net quantity.

Existing M4-2b tests continue to pass.

## Margin sufficiency waiver

M4-2d maps instrument price step, lot size, step value, tradability, schedule,
cash, free cash, and margin fields into canonical structures. It does **not**
yet implement full instrument-derived required margin calculation from:

```text
instrument + side + qty + price
```

This is an explicit M4-2d waiver, not a live authorization. The gap remains P0
before any additional live-position tests. Until that follow-up is accepted,
margin sufficiency can only compare already-supplied required margin against
canonical free cash.

## Remaining P0 blockers after M4-2d

M4-2d reduces the canonical integration gap, but does not close operational
parity. Remaining P0 blockers include:

1. instrument-derived required margin calculation;
2. canonical readiness as the single runtime/live preflight source;
3. quote/order/trade/instrument freshness from real read-only artifacts in the
   next pre-live package;
4. runtime command consumer still not connected to real FINAM;
5. Stop/SLTP/bracket/replace/multi-leg still blocked;
6. further live position tests still blocked pending review.

## Acceptance

M4-2d is ready for review when:

- `cargo test -p broker-finam m4_2d` passes;
- `cargo test -p broker-finam m4_2b` still passes;
- `cargo test -p broker-core operational` passes;
- `cargo test -p broker-cli m4_1c --no-default-features` compiles the M4-1c
  canonical report path;
- M4-2d evidence reports no live calls and live expansion blocked;
- forbidden-surface scanners remain green.
