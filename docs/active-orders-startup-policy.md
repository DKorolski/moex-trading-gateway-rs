# Active orders startup policy draft

This is a draft for M2d/M3 review. It is not an order-emitting implementation.

The FINAM shadow runner currently remains read-only and never publishes
`LiveReady`. Active orders therefore do not create live trading risk in M2d, but
they must be classified before any future M3 order mode.

## Startup classes

Allowed to continue reconciliation:

- no active broker orders;
- active broker order is expected by durable local state;
- active broker order is terminal by the time the next broker-truth snapshot is
  loaded.

Must block live readiness and require operator action:

- active order exists for a strategy symbol but no local runtime state owns it;
- active order has a broker-native client order id that cannot be correlated to
  a local request id or accepted manual override;
- active order has unknown broker status;
- active order belongs to a stop/SLTP/bracket lifecycle while those features are
  disabled;
- active order is on an unexpected account, symbol, side, or quantity.

Close-only / manual-only candidate:

- active order is broker/manual owned and intentionally outside strategy scope;
- operator explicitly marks it as external before any strategy runtime bridge is
  allowed to arm.

## Required before M3

- durable `StrategyRequestId -> ClientOrderId -> BrokerOrderId` mapping;
- explicit owner attribution for broker-truth orders;
- active-order policy encoded in readiness;
- operator-visible report listing active orders by redacted/fingerprinted ids;
- no `LiveReady` if active ownership is unknown.
