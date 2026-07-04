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
                order.lifecycle == BrokerOrderLifecycle::Active
                    && instrument_identity_matches(&order.instrument, instrument)
            })
            .collect()
    }

    pub fn account_wide_active_order_count(&self) -> usize {
        self.orders
            .iter()
            .filter(|order| order.lifecycle == BrokerOrderLifecycle::Active)
            .count()
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
        let order = |instrument: InstrumentId, status: OrderStatus| BrokerOrderSnapshot {
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
            filled_qty: Decimal::ZERO,
            remaining_qty: Some(Decimal::ONE),
            limit_price: None,
            source_ts: None,
            received_ts: now,
        };
        let snapshot = BrokerTruthSnapshot {
            account_id: account_id.clone(),
            orders: vec![
                order(target.clone(), OrderStatus::Filled),
                order(ri, OrderStatus::Working),
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
}
