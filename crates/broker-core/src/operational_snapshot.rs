use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::account::CashPosition;
use crate::ids::{BrokerAccountId, BrokerOrderId, BrokerTradeId, ClientOrderId};
use crate::instrument::{InstrumentId, InstrumentMapEntry, Money, Price, Quantity};
use crate::order::{OrderSide, OrderStatus, OrderType, TimeInForce};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrokerOrderLifecycle {
    Active,
    Terminal,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrokerOrderQuantityTruth {
    RemainingPositive,
    RemainingZero,
    RemainingUnknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrokerOrderSnapshot {
    pub account_id: BrokerAccountId,
    pub broker_order_id: Option<BrokerOrderId>,
    pub client_order_id: Option<ClientOrderId>,
    pub instrument: InstrumentId,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub time_in_force: Option<TimeInForce>,
    pub status: OrderStatus,
    pub lifecycle: BrokerOrderLifecycle,
    pub qty: Quantity,
    pub filled_qty: Quantity,
    pub remaining_qty: Option<Quantity>,
    pub limit_price: Option<Price>,
    pub source_ts: Option<DateTime<Utc>>,
    pub received_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrokerPositionSnapshot {
    pub account_id: BrokerAccountId,
    pub instrument: InstrumentId,
    pub qty: Quantity,
    pub avg_price: Option<Price>,
    pub unrealized_pnl: Option<Money>,
    pub source_ts: Option<DateTime<Utc>>,
    pub received_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrokerCashSnapshot {
    pub account_id: BrokerAccountId,
    pub cash: Vec<CashPosition>,
    pub equity: Option<Money>,
    pub free_cash: Option<Money>,
    pub initial_margin: Option<Money>,
    pub maintenance_margin: Option<Money>,
    pub source_ts: Option<DateTime<Utc>>,
    pub received_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrokerInstrumentSpec {
    pub instrument: InstrumentMapEntry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrokerTradeSnapshot {
    pub account_id: BrokerAccountId,
    pub broker_trade_id: BrokerTradeId,
    pub broker_order_id: Option<BrokerOrderId>,
    pub client_order_id: Option<ClientOrderId>,
    pub instrument: InstrumentId,
    pub side: OrderSide,
    pub qty: Quantity,
    pub price: Price,
    pub gross_amount: Option<Money>,
    pub commission: Option<Money>,
    pub source_ts: DateTime<Utc>,
    pub received_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrokerTruthSnapshot {
    pub account_id: BrokerAccountId,
    pub orders: Vec<BrokerOrderSnapshot>,
    pub positions: Vec<BrokerPositionSnapshot>,
    pub cash: Option<BrokerCashSnapshot>,
    pub trades: Vec<BrokerTradeSnapshot>,
    pub instruments: Vec<BrokerInstrumentSpec>,
    pub received_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BrokerTruthInstrumentSummary {
    pub target_open_positions_count: usize,
    pub account_open_positions_count: usize,
    pub target_active_orders_count: usize,
    pub target_unknown_orders_count: usize,
    pub target_terminal_orders_count: usize,
    pub target_inconsistent_orders_count: usize,
    pub account_active_orders_count: usize,
    pub account_unknown_orders_count: usize,
    pub account_orphan_orders_count: usize,
    pub other_symbol_active_orders_count: usize,
}

impl BrokerOrderSnapshot {
    pub fn lifecycle_for(status: &OrderStatus) -> BrokerOrderLifecycle {
        match status {
            OrderStatus::New | OrderStatus::Working | OrderStatus::PartiallyFilled => {
                BrokerOrderLifecycle::Active
            }
            OrderStatus::Filled
            | OrderStatus::Canceled
            | OrderStatus::Rejected
            | OrderStatus::Expired => BrokerOrderLifecycle::Terminal,
            OrderStatus::Unknown(_) => BrokerOrderLifecycle::Unknown,
        }
    }

    pub fn quantity_truth(&self) -> BrokerOrderQuantityTruth {
        match self.remaining_qty {
            Some(remaining_qty) if remaining_qty == Decimal::ZERO => {
                BrokerOrderQuantityTruth::RemainingZero
            }
            Some(_) => BrokerOrderQuantityTruth::RemainingPositive,
            None => BrokerOrderQuantityTruth::RemainingUnknown,
        }
    }

    pub fn has_blocking_active_quantity(&self) -> bool {
        matches!(
            self.quantity_truth(),
            BrokerOrderQuantityTruth::RemainingPositive
                | BrokerOrderQuantityTruth::RemainingUnknown
        )
    }

    pub fn is_active_for_lifecycle(&self) -> bool {
        self.lifecycle == BrokerOrderLifecycle::Active && self.has_blocking_active_quantity()
    }

    pub fn is_inconsistent_active_zero_remaining(&self) -> bool {
        self.lifecycle == BrokerOrderLifecycle::Active
            && self.quantity_truth() == BrokerOrderQuantityTruth::RemainingZero
    }
}

impl BrokerPositionSnapshot {
    pub fn is_open(&self) -> bool {
        self.qty != Decimal::ZERO
    }

    pub fn matches_instrument(&self, instrument: &InstrumentId) -> bool {
        instrument_identity_matches(&self.instrument, instrument)
    }
}

impl BrokerTruthSnapshot {
    pub fn open_positions_for_instrument(
        &self,
        instrument: &InstrumentId,
    ) -> Vec<&BrokerPositionSnapshot> {
        self.positions
            .iter()
            .filter(|position| position.is_open() && position.matches_instrument(instrument))
            .collect()
    }

    pub fn active_orders_for_instrument(
        &self,
        instrument: &InstrumentId,
    ) -> Vec<&BrokerOrderSnapshot> {
        self.orders
            .iter()
            .filter(|order| {
                order.is_active_for_lifecycle()
                    && instrument_identity_matches(&order.instrument, instrument)
            })
            .collect()
    }

    pub fn unknown_orders_for_instrument(
        &self,
        instrument: &InstrumentId,
    ) -> Vec<&BrokerOrderSnapshot> {
        self.orders
            .iter()
            .filter(|order| {
                order.lifecycle == BrokerOrderLifecycle::Unknown
                    && instrument_identity_matches(&order.instrument, instrument)
            })
            .collect()
    }

    pub fn account_wide_active_order_count(&self) -> usize {
        self.orders
            .iter()
            .filter(|order| order.is_active_for_lifecycle())
            .count()
    }

    pub fn summarize_for_instrument(
        &self,
        instrument: &InstrumentId,
    ) -> BrokerTruthInstrumentSummary {
        let target_open_positions_count = self.open_positions_for_instrument(instrument).len();
        let account_open_positions_count = self
            .positions
            .iter()
            .filter(|position| position.is_open())
            .count();
        let target_active_orders_count = self.active_orders_for_instrument(instrument).len();
        let target_unknown_orders_count = self.unknown_orders_for_instrument(instrument).len();
        let target_terminal_orders_count = self
            .orders
            .iter()
            .filter(|order| {
                order.lifecycle == BrokerOrderLifecycle::Terminal
                    && instrument_identity_matches(&order.instrument, instrument)
            })
            .count();
        let target_inconsistent_orders_count = self
            .orders
            .iter()
            .filter(|order| {
                order.is_inconsistent_active_zero_remaining()
                    && instrument_identity_matches(&order.instrument, instrument)
            })
            .count();
        let account_active_orders_count = self.account_wide_active_order_count();
        let account_unknown_orders_count = self
            .orders
            .iter()
            .filter(|order| order.lifecycle == BrokerOrderLifecycle::Unknown)
            .count();
        let other_symbol_active_orders_count = self
            .orders
            .iter()
            .filter(|order| {
                order.is_active_for_lifecycle()
                    && !instrument_identity_matches(&order.instrument, instrument)
            })
            .count();

        BrokerTruthInstrumentSummary {
            target_open_positions_count,
            account_open_positions_count,
            target_active_orders_count,
            target_unknown_orders_count,
            target_terminal_orders_count,
            target_inconsistent_orders_count,
            account_active_orders_count,
            account_unknown_orders_count,
            account_orphan_orders_count: 0,
            other_symbol_active_orders_count,
        }
    }
}

pub fn instrument_identity_matches(left: &InstrumentId, right: &InstrumentId) -> bool {
    if left.venue_symbol.is_some() && right.venue_symbol.is_some() {
        return left.venue_symbol == right.venue_symbol;
    }
    left.symbol == right.symbol && left.exchange == right.exchange && left.market == right.market
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::{Exchange, Market};

    fn instrument(symbol: &str, venue_symbol: Option<&str>) -> InstrumentId {
        InstrumentId {
            symbol: symbol.to_string(),
            venue_symbol: venue_symbol.map(str::to_string),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn order(
        account_id: &BrokerAccountId,
        instrument: InstrumentId,
        status: OrderStatus,
        remaining_qty: Option<Decimal>,
    ) -> BrokerOrderSnapshot {
        let now = Utc::now();
        BrokerOrderSnapshot {
            account_id: account_id.clone(),
            broker_order_id: None,
            client_order_id: None,
            instrument,
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            time_in_force: None,
            status: status.clone(),
            lifecycle: BrokerOrderSnapshot::lifecycle_for(&status),
            qty: Decimal::ONE,
            filled_qty: remaining_qty
                .map(|remaining_qty| Decimal::ONE - remaining_qty)
                .unwrap_or(Decimal::ZERO),
            remaining_qty,
            limit_price: None,
            source_ts: None,
            received_ts: now,
        }
    }

    fn empty_truth(
        account_id: BrokerAccountId,
        orders: Vec<BrokerOrderSnapshot>,
    ) -> BrokerTruthSnapshot {
        BrokerTruthSnapshot {
            account_id,
            orders,
            positions: Vec::new(),
            cash: None,
            trades: Vec::new(),
            instruments: Vec::new(),
            received_ts: Utc::now(),
        }
    }

    #[test]
    fn position_truth_is_target_instrument_and_nonzero_qty_scoped() {
        let now = Utc::now();
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let snapshot = BrokerTruthSnapshot {
            account_id: account_id.clone(),
            orders: Vec::new(),
            positions: vec![
                BrokerPositionSnapshot {
                    account_id: account_id.clone(),
                    instrument: target.clone(),
                    qty: Decimal::ZERO,
                    avg_price: None,
                    unrealized_pnl: None,
                    source_ts: None,
                    received_ts: now,
                },
                BrokerPositionSnapshot {
                    account_id,
                    instrument: instrument("RI", Some("RIU6@RTSX")),
                    qty: Decimal::ONE,
                    avg_price: None,
                    unrealized_pnl: None,
                    source_ts: None,
                    received_ts: now,
                },
            ],
            cash: None,
            trades: Vec::new(),
            instruments: Vec::new(),
            received_ts: now,
        };

        assert!(snapshot.open_positions_for_instrument(&target).is_empty());
        assert_eq!(
            snapshot
                .open_positions_for_instrument(&instrument("RI", Some("RIU6@RTSX")))
                .len(),
            1
        );
    }

    #[test]
    fn active_order_truth_is_target_instrument_scoped_but_account_guard_remains_available() {
        let now = Utc::now();
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let ri = instrument("RI", Some("RIU6@RTSX"));
        let snapshot = BrokerTruthSnapshot {
            account_id: account_id.clone(),
            orders: vec![
                order(
                    &account_id,
                    target.clone(),
                    OrderStatus::Filled,
                    Some(Decimal::ONE),
                ),
                order(&account_id, ri, OrderStatus::Working, Some(Decimal::ONE)),
            ],
            positions: Vec::new(),
            cash: None,
            trades: Vec::new(),
            instruments: Vec::new(),
            received_ts: now,
        };

        assert!(snapshot.active_orders_for_instrument(&target).is_empty());
        assert_eq!(snapshot.account_wide_active_order_count(), 1);
    }

    #[test]
    fn active_order_truth_is_quantity_aware() {
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let snapshot = empty_truth(
            account_id.clone(),
            vec![
                order(
                    &account_id,
                    target.clone(),
                    OrderStatus::Working,
                    Some(Decimal::ZERO),
                ),
                order(
                    &account_id,
                    target.clone(),
                    OrderStatus::PartiallyFilled,
                    Some(Decimal::new(5, 1)),
                ),
                order(&account_id, target.clone(), OrderStatus::New, None),
            ],
        );

        let active = snapshot.active_orders_for_instrument(&target);

        assert_eq!(active.len(), 2);
        assert_eq!(
            snapshot
                .summarize_for_instrument(&target)
                .target_inconsistent_orders_count,
            1
        );
    }

    #[test]
    fn terminal_order_with_remaining_quantity_is_not_active_but_stays_diagnostic() {
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let snapshot = empty_truth(
            account_id.clone(),
            vec![order(
                &account_id,
                target.clone(),
                OrderStatus::Filled,
                Some(Decimal::ONE),
            )],
        );
        let summary = snapshot.summarize_for_instrument(&target);

        assert!(snapshot.active_orders_for_instrument(&target).is_empty());
        assert_eq!(summary.target_terminal_orders_count, 1);
        assert_eq!(summary.target_active_orders_count, 0);
    }

    #[test]
    fn unknown_order_status_is_separate_blocking_truth() {
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let snapshot = empty_truth(
            account_id.clone(),
            vec![order(
                &account_id,
                target.clone(),
                OrderStatus::Unknown("ORDER_STATUS_FOLLOW_NEW".to_string()),
                Some(Decimal::ONE),
            )],
        );
        let summary = snapshot.summarize_for_instrument(&target);

        assert!(snapshot.active_orders_for_instrument(&target).is_empty());
        assert_eq!(summary.target_unknown_orders_count, 1);
        assert_eq!(summary.account_unknown_orders_count, 1);
    }

    #[test]
    fn target_lifecycle_and_account_wide_safety_counts_are_structured_separately() {
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let ri = instrument("RI", Some("RIU6@RTSX"));
        let snapshot = empty_truth(
            account_id.clone(),
            vec![
                order(
                    &account_id,
                    target.clone(),
                    OrderStatus::Filled,
                    Some(Decimal::ZERO),
                ),
                order(&account_id, ri, OrderStatus::Working, Some(Decimal::ONE)),
            ],
        );
        let summary = snapshot.summarize_for_instrument(&target);

        assert_eq!(summary.target_active_orders_count, 0);
        assert_eq!(summary.target_terminal_orders_count, 1);
        assert_eq!(summary.account_active_orders_count, 1);
        assert_eq!(summary.other_symbol_active_orders_count, 1);
    }
}
