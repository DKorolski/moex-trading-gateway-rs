# M4-2a parity test gap matrix

Status: no-live test inventory for ALOR -> canonical model -> FINAM parity.

The matrix is a gate before any new live position test. Each row names the
broker-neutral invariant, the ALOR oracle source, current coverage, and the
required next action.

| ID | Invariant | ALOR oracle source | Current coverage | Status | Next action |
| --- | --- | --- | --- | --- | --- |
| P0-1 | Active order = target instrument + active lifecycle + blocking remaining qty | `OrdersManager::active_orders_for`, command lifecycle logs | `BrokerTruthSnapshot::active_orders_for_instrument`, `active_order_truth_is_quantity_aware` | implemented in core | Wire FINAM order mapper to canonical snapshot |
| P0-2 | Unknown order status blocks readiness/lifecycle | ALOR non-terminal status is not terminal | `unknown_order_status_is_separate_blocking_truth` | implemented in core | Bind readiness to canonical unknown counters |
| P0-3 | Target flat ignores zero rows and other instruments | `PositionsSnapshot: HashMap<String, PositionSnapshot>` | `position_truth_is_target_instrument_and_nonzero_qty_scoped`, FINAM zero-qty mapper test | partial | Add ALOR/FINAM fixture converter tests |
| P0-4 | Account-wide active orders remain safety blocker | ALOR account streams + target scoped active orders | `target_lifecycle_and_account_wide_safety_counts_are_structured_separately` | implemented in core | Use summary in M4/M5 preflights |
| P0-5 | Instrument identity includes MIC/board/contract, not ticker only | ALOR symbol/class/secCode + FINAM ticker@MIC | instrument registry validator tests | partial | Add cross-broker instrument fixture suite |
| P0-6 | Cash/margin sufficiency explicit before entry | ALOR portfolio/cash readiness and operator runbooks | `BrokerCashSnapshot` skeleton only | missing | Map FINAM account/asset params into canonical cash/margin |
| P0-7 | Trade/fill reconciliation explains position delta | ALOR trade/order/position streams | trade mapper exists, no canonical chain | missing | Add order->trade->position parity fixtures |
| P0-8 | Stale orders/positions/quote/account snapshots block live | ALOR synced feed readiness | M3g/M3j readiness docs, not canonical-bound | partial | Bind readiness to `BrokerTruthSnapshot` freshness |
| P0-9 | Market closed blocks entry but not emergency cancel policy | ALOR scheduler and command validation | scheduler docs and FINAM schedule checks | partial | Add canonical live-admission decision tests |
| P0-10 | FINAM mapper produces `BrokerTruthSnapshot`, not local counters | New M4-2 broker-neutral model | model exists, not wired | missing | Implement `map_finam_broker_truth_snapshot` |

## Current no-live conclusion

M4-1c is operationally reconciled flat, but live expansion remains blocked until
P0 gaps for orders, positions, instruments, cash/margin, trades, and readiness
are either implemented or explicitly waived by review.
