# M4-1c-a closure + ALOR parity gap audit

Status: live expansion stopped.

M4-1c actual market round-trip was reconciled flat by broker journal and by
post-run read-only preflight. It also exposed a semantic gap: position/order
truth must be broker-neutral, instrument-scoped, and quantity/status aware.
Further live position tests are blocked until the ALOR operational oracle is
extracted and FINAM mapping is aligned to the same domain model.

## M4-1c observed issue

- The actual FINAM round-trip created a buy and sell for `IMOEXF` qty `1`.
- Broker journal showed net qty `0`.
- The immediate FINAM account snapshot still carried a position row.
- A local counter treated row presence as open position.
- The mapper was hardened to ignore zero-quantity positions.
- M4-1c lifecycle reporting was further hardened to use target-instrument
  non-zero position truth, with account-wide row counts retained only as
  diagnostics.

This is accepted as a reconciled micro result, but it is not enough for further
live expansion.

## ALOR operational oracle extraction

Source reviewed:

- `alor-types/src/strategy.rs`
- `alor-gateway/src/models.rs`
- `alor-gateway/src/state/positions_manager.rs`
- `alor-gateway/src/state/orders_manager.rs`
- `alor-gateway/src/services/command_consumer.rs`
- `alor-gateway/src/supervisor.rs`

Extracted mature semantics:

1. Position truth is instrument-keyed.
   - ALOR `PositionsSnapshot` is a `HashMap<String, PositionSnapshot>`.
   - `PositionsManager` stores updates by `event.symbol`.
   - A symbol row is not enough; open risk must be derived from quantity.

2. Order truth has terminal classification.
   - ALOR `OrdersManager::active_orders_for(symbol)` filters by target symbol.
   - Terminal statuses are `filled`, `cancelled`, `canceled`, `rejected`.
   - Non-terminal statuses are active for lifecycle purposes.

3. Account-wide safety and target-symbol lifecycle are different checks.
   - Target-symbol active orders are lifecycle truth.
   - Account-wide active/unknown/orphan orders remain a live safety guard.

4. Readiness depends on synced broker truth feeds.
   - Gateway readiness waits for positions/orders/stop-orders sync.
   - Trading admission requires `LiveReady`.
   - Non-cancel commands require open trading window.

5. Command validation is broker-neutral before broker transport.
   - Price and quantity are normalized by configured tick/volume step.
   - Invalid non-positive quantity/price is blocked before transport.

## Broker-neutral domain model

M4-2 introduces canonical operational truth types in `broker-core`:

- `BrokerOrderSnapshot`
- `BrokerPositionSnapshot`
- `BrokerCashSnapshot`
- `BrokerInstrumentSpec`
- `BrokerTradeSnapshot`
- `BrokerTruthSnapshot`

The model separates:

- order lifecycle: `Active`, `Terminal`, `Unknown`;
- target-instrument lifecycle truth;
- account-wide safety guard;
- zero-quantity position rows versus open positions;
- broker-native identity versus broker-neutral instrument identity.

## FINAM mapper alignment requirement

FINAM must map broker responses into `BrokerTruthSnapshot` instead of ad-hoc
local counters before additional live position testing.

Required invariants:

- flat target position = no non-zero position for target instrument;
- zero-quantity position rows are diagnostics, not open risk;
- target-symbol active orders are lifecycle truth;
- account-wide active/unknown/orphan orders are safety guard;
- unknown order status blocks live readiness or requires explicit policy;
- instrument matching prefers venue symbol when available, otherwise falls back
  to broker-neutral symbol + exchange + market.

## Parity tests to add

M4-2a/M4-2b must add fixtures and tests:

1. ALOR fixture -> canonical `BrokerTruthSnapshot`.
2. FINAM fixture -> canonical `BrokerTruthSnapshot`.
3. Same business state produces same invariants:
   - target flat with zero row;
   - other-instrument position does not make target non-flat;
   - target terminal order does not block lifecycle;
   - other-instrument active order is visible account-wide;
   - unknown status is not silently terminal.

## Revised plan

Old next step:

- M4-1c-a closure / mapper-hardening acceptance.

New sequence:

1. M4-1c-a closure + ALOR parity gap audit.
2. M4-2a ALOR operational oracle extraction.
3. M4-2b BrokerNeutralOperationalSnapshot mapping and parity tests.
4. Only after M4-2b review: discuss additional live position tests.

## Trading boundary

Until M4-2b is accepted:

- no new live position tests;
- no command-consumer-to-real-FINAM;
- no runtime-live expansion;
- no stop/SLTP/bracket;
- no multi-instrument live test;
- no market entry/exit repeats.
