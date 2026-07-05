use chrono::{DateTime, NaiveDate, Utc};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrokerOrderOrphanReason {
    AccountMismatch,
    MissingCorrelationId,
    MissingInstrumentRegistry,
    UnknownInstrumentIdentity,
    AmbiguousInstrumentIdentity,
    FilledQuantityWithoutMatchingTrade,
    MatchingTradeAccountMismatch,
    MatchingTradeInstrumentMismatch,
    MatchingTradeSideMismatch,
    MatchingTradeQuantityLessThanFilledQuantity,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broker_asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_date: Option<NaiveDate>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broker_asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_initial_margin: Option<Money>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_initial_margin: Option<Money>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broker_asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_date: Option<NaiveDate>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrokerMarginSufficiency {
    Sufficient,
    Insufficient,
    MissingCashSnapshot,
    MissingFreeCash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrokerRequiredMarginFailure {
    MissingInstrumentSpec,
    MissingInitialMargin,
    InvalidQuantity,
    InvalidReferencePrice,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BrokerRequiredMargin {
    Derived { amount: Money },
    Missing(BrokerRequiredMarginFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BrokerOrderMarginSufficiency {
    Sufficient { required_margin: Money },
    Insufficient { required_margin: Money },
    MissingCashSnapshot,
    MissingFreeCash,
    MissingInstrumentSpec,
    MissingInitialMargin,
    InvalidQuantity,
    InvalidReferencePrice,
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

    pub fn has_correlation_id(&self) -> bool {
        self.broker_order_id.is_some() || self.client_order_id.is_some()
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
    pub fn target_position_qty(&self, instrument: &InstrumentId) -> Quantity {
        self.positions
            .iter()
            .filter(|position| position.matches_instrument(instrument))
            .map(|position| position.qty)
            .sum()
    }

    pub fn target_is_flat(&self, instrument: &InstrumentId) -> bool {
        self.target_position_qty(instrument) == Decimal::ZERO
    }

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

    pub fn target_active_orders(&self, instrument: &InstrumentId) -> Vec<&BrokerOrderSnapshot> {
        self.active_orders_for_instrument(instrument)
    }

    pub fn account_active_orders(&self) -> Vec<&BrokerOrderSnapshot> {
        self.orders
            .iter()
            .filter(|order| order.is_active_for_lifecycle())
            .collect()
    }

    pub fn unknown_orders(&self) -> Vec<&BrokerOrderSnapshot> {
        self.orders
            .iter()
            .filter(|order| order.lifecycle == BrokerOrderLifecycle::Unknown)
            .collect()
    }

    pub fn account_wide_active_order_count(&self) -> usize {
        self.orders
            .iter()
            .filter(|order| order.is_active_for_lifecycle())
            .count()
    }

    pub fn account_orphan_orders(&self) -> Vec<&BrokerOrderSnapshot> {
        self.orders
            .iter()
            .filter(|order| !self.orphan_reasons_for_order(order).is_empty())
            .collect()
    }

    pub fn account_orphan_order_count(&self) -> usize {
        self.account_orphan_orders().len()
    }

    pub fn orphan_reasons_for_order(
        &self,
        order: &BrokerOrderSnapshot,
    ) -> Vec<BrokerOrderOrphanReason> {
        let mut reasons = Vec::new();

        if order.account_id != self.account_id {
            reasons.push(BrokerOrderOrphanReason::AccountMismatch);
        }
        if !order.has_correlation_id() {
            reasons.push(BrokerOrderOrphanReason::MissingCorrelationId);
        }
        reasons.extend(self.order_instrument_identity_reasons(order));

        if order.filled_qty > Decimal::ZERO {
            let matching_trades = self.trades_matching_order_identity(order);
            if matching_trades.is_empty() {
                reasons.push(BrokerOrderOrphanReason::FilledQuantityWithoutMatchingTrade);
            } else {
                if matching_trades
                    .iter()
                    .any(|trade| trade.account_id != order.account_id)
                {
                    reasons.push(BrokerOrderOrphanReason::MatchingTradeAccountMismatch);
                }
                if matching_trades
                    .iter()
                    .any(|trade| !order_trade_instrument_identity_matches(order, trade))
                {
                    reasons.push(BrokerOrderOrphanReason::MatchingTradeInstrumentMismatch);
                }
                if matching_trades.iter().any(|trade| trade.side != order.side) {
                    reasons.push(BrokerOrderOrphanReason::MatchingTradeSideMismatch);
                }
                let consistent_trade_qty: Quantity = matching_trades
                    .iter()
                    .filter(|trade| {
                        trade.account_id == order.account_id
                            && order_trade_instrument_identity_matches(order, trade)
                            && trade.side == order.side
                    })
                    .map(|trade| trade.qty)
                    .sum();
                if consistent_trade_qty < order.filled_qty {
                    reasons
                        .push(BrokerOrderOrphanReason::MatchingTradeQuantityLessThanFilledQuantity);
                }
            }
        }

        reasons
    }

    fn order_instrument_identity_reasons(
        &self,
        order: &BrokerOrderSnapshot,
    ) -> Vec<BrokerOrderOrphanReason> {
        if order.instrument.venue_symbol.is_none() {
            return vec![BrokerOrderOrphanReason::UnknownInstrumentIdentity];
        }
        if self.instruments.is_empty() {
            return vec![BrokerOrderOrphanReason::MissingInstrumentRegistry];
        }
        let matching_specs = self
            .instruments
            .iter()
            .filter(|spec| spec.matches_order_identity(order))
            .collect::<Vec<_>>();
        match matching_specs.len() {
            0 => vec![BrokerOrderOrphanReason::UnknownInstrumentIdentity],
            1 => Vec::new(),
            _ => vec![BrokerOrderOrphanReason::AmbiguousInstrumentIdentity],
        }
    }

    fn trades_matching_order_identity<'a>(
        &'a self,
        order: &BrokerOrderSnapshot,
    ) -> Vec<&'a BrokerTradeSnapshot> {
        self.trades
            .iter()
            .filter(|trade| {
                order
                    .broker_order_id
                    .as_ref()
                    .zip(trade.broker_order_id.as_ref())
                    .is_some_and(|(order_id, trade_order_id)| order_id == trade_order_id)
                    || order
                        .client_order_id
                        .as_ref()
                        .zip(trade.client_order_id.as_ref())
                        .is_some_and(|(order_id, trade_order_id)| order_id == trade_order_id)
            })
            .collect()
    }

    pub fn cash_by_currency(&self, currency: &str) -> Option<Money> {
        self.cash
            .as_ref()
            .and_then(|cash| cash.cash_by_currency(currency))
    }

    pub fn margin_sufficiency_for_order(&self, required_margin: Money) -> BrokerMarginSufficiency {
        let Some(cash) = &self.cash else {
            return BrokerMarginSufficiency::MissingCashSnapshot;
        };
        cash.margin_sufficiency_for_required_margin(required_margin)
    }

    pub fn required_margin_for_order(
        &self,
        instrument: &InstrumentId,
        side: OrderSide,
        qty: Quantity,
        reference_price: Price,
    ) -> BrokerRequiredMargin {
        if qty <= Decimal::ZERO {
            return BrokerRequiredMargin::Missing(BrokerRequiredMarginFailure::InvalidQuantity);
        }
        if reference_price <= Decimal::ZERO {
            return BrokerRequiredMargin::Missing(
                BrokerRequiredMarginFailure::InvalidReferencePrice,
            );
        }
        let Some(spec) = self
            .instruments
            .iter()
            .find(|spec| spec.matches_instrument_id(instrument))
        else {
            return BrokerRequiredMargin::Missing(
                BrokerRequiredMarginFailure::MissingInstrumentSpec,
            );
        };
        let initial_margin = match side {
            OrderSide::Buy => spec.long_initial_margin,
            OrderSide::Sell => spec.short_initial_margin,
        };
        let Some(initial_margin) = initial_margin else {
            return BrokerRequiredMargin::Missing(
                BrokerRequiredMarginFailure::MissingInitialMargin,
            );
        };
        let _reference_notional = reference_price * qty;
        BrokerRequiredMargin::Derived {
            amount: initial_margin * qty,
        }
    }

    pub fn margin_sufficiency_for_instrument_order(
        &self,
        instrument: &InstrumentId,
        side: OrderSide,
        qty: Quantity,
        reference_price: Price,
    ) -> BrokerOrderMarginSufficiency {
        let required_margin =
            match self.required_margin_for_order(instrument, side, qty, reference_price) {
                BrokerRequiredMargin::Derived { amount } => amount,
                BrokerRequiredMargin::Missing(
                    BrokerRequiredMarginFailure::MissingInstrumentSpec,
                ) => {
                    return BrokerOrderMarginSufficiency::MissingInstrumentSpec;
                }
                BrokerRequiredMargin::Missing(
                    BrokerRequiredMarginFailure::MissingInitialMargin,
                ) => {
                    return BrokerOrderMarginSufficiency::MissingInitialMargin;
                }
                BrokerRequiredMargin::Missing(BrokerRequiredMarginFailure::InvalidQuantity) => {
                    return BrokerOrderMarginSufficiency::InvalidQuantity;
                }
                BrokerRequiredMargin::Missing(
                    BrokerRequiredMarginFailure::InvalidReferencePrice,
                ) => {
                    return BrokerOrderMarginSufficiency::InvalidReferencePrice;
                }
            };
        match self.margin_sufficiency_for_order(required_margin) {
            BrokerMarginSufficiency::Sufficient => {
                BrokerOrderMarginSufficiency::Sufficient { required_margin }
            }
            BrokerMarginSufficiency::Insufficient => {
                BrokerOrderMarginSufficiency::Insufficient { required_margin }
            }
            BrokerMarginSufficiency::MissingCashSnapshot => {
                BrokerOrderMarginSufficiency::MissingCashSnapshot
            }
            BrokerMarginSufficiency::MissingFreeCash => {
                BrokerOrderMarginSufficiency::MissingFreeCash
            }
        }
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
        let account_orphan_orders_count = self.account_orphan_order_count();
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
            account_orphan_orders_count,
            other_symbol_active_orders_count,
        }
    }
}

impl BrokerCashSnapshot {
    pub fn cash_by_currency(&self, currency: &str) -> Option<Money> {
        self.cash
            .iter()
            .find(|cash| cash.currency.eq_ignore_ascii_case(currency))
            .map(|cash| cash.amount)
    }

    pub fn margin_sufficiency_for_required_margin(
        &self,
        required_margin: Money,
    ) -> BrokerMarginSufficiency {
        let Some(free_cash) = self.free_cash else {
            return BrokerMarginSufficiency::MissingFreeCash;
        };
        if free_cash >= required_margin {
            BrokerMarginSufficiency::Sufficient
        } else {
            BrokerMarginSufficiency::Insufficient
        }
    }
}

impl BrokerInstrumentSpec {
    pub fn instrument_id(&self) -> InstrumentId {
        InstrumentId {
            symbol: self.instrument.internal_symbol.0.clone(),
            venue_symbol: Some(self.instrument.broker_symbol.0.clone()),
            exchange: self.instrument.exchange.clone(),
            market: self.instrument.market.clone(),
        }
    }

    pub fn matches_instrument_id(&self, instrument: &InstrumentId) -> bool {
        instrument_identity_matches(&self.instrument_id(), instrument)
    }

    pub fn matches_order_identity(&self, order: &BrokerOrderSnapshot) -> bool {
        self.matches_instrument_id(&order.instrument)
            && optional_string_identity_matches(&order.broker_asset_id, &self.broker_asset_id)
            && optional_string_identity_matches(&order.board, &self.board)
            && optional_date_identity_matches(
                &order.expiration_date,
                &self.instrument.expiration_date,
            )
    }

    pub fn matches_trade_identity(&self, trade: &BrokerTradeSnapshot) -> bool {
        self.matches_instrument_id(&trade.instrument)
            && optional_string_identity_matches(&trade.broker_asset_id, &self.broker_asset_id)
            && optional_string_identity_matches(&trade.board, &self.board)
            && optional_date_identity_matches(
                &trade.expiration_date,
                &self.instrument.expiration_date,
            )
    }

    pub fn canonical_identity_matches(&self, other: &BrokerInstrumentSpec) -> bool {
        self.instrument.broker == other.instrument.broker
            && self.instrument.broker_symbol == other.instrument.broker_symbol
            && self.instrument.exchange == other.instrument.exchange
            && self.instrument.market == other.instrument.market
            && self.instrument.schedule_id == other.instrument.schedule_id
            && self.board == other.board
            && self.instrument.expiration_date == other.instrument.expiration_date
            && self.broker_asset_id == other.broker_asset_id
    }
}

fn optional_string_identity_matches(actual: &Option<String>, expected: &Option<String>) -> bool {
    match actual.as_deref() {
        Some(actual) => expected.as_deref() == Some(actual),
        None => true,
    }
}

fn optional_date_identity_matches(
    actual: &Option<NaiveDate>,
    expected: &Option<NaiveDate>,
) -> bool {
    match actual {
        Some(actual) => expected == &Some(*actual),
        None => true,
    }
}

fn optional_string_identity_compatible(left: &Option<String>, right: &Option<String>) -> bool {
    match (left.as_deref(), right.as_deref()) {
        (Some(left), Some(right)) => left == right,
        (None, None) => true,
        _ => false,
    }
}

fn optional_date_identity_compatible(left: &Option<NaiveDate>, right: &Option<NaiveDate>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left == right,
        (None, None) => true,
        _ => false,
    }
}

fn order_trade_instrument_identity_matches(
    order: &BrokerOrderSnapshot,
    trade: &BrokerTradeSnapshot,
) -> bool {
    instrument_identity_matches(&trade.instrument, &order.instrument)
        && optional_string_identity_compatible(&trade.broker_asset_id, &order.broker_asset_id)
        && optional_string_identity_compatible(&trade.board, &order.board)
        && optional_date_identity_compatible(&trade.expiration_date, &order.expiration_date)
}

pub fn instrument_identity_matches(left: &InstrumentId, right: &InstrumentId) -> bool {
    if left.venue_symbol.is_some() && right.venue_symbol.is_some() {
        return left.venue_symbol == right.venue_symbol;
    }
    left.symbol == right.symbol && left.exchange == right.exchange && left.market == right.market
}

pub fn instrument_spec_identity_matches(
    left: &BrokerInstrumentSpec,
    right: &BrokerInstrumentSpec,
) -> bool {
    left.canonical_identity_matches(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::BrokerKind;
    use crate::instrument::{BrokerSymbol, InternalSymbol};
    use crate::instrument::{Exchange, Market};
    use chrono::NaiveDate;

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
            broker_order_id: Some(BrokerOrderId::new("BROKER-ORDER-TEST")),
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
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: None,
            received_ts: now,
        }
    }

    fn order_with_broker_order_id(
        account_id: &BrokerAccountId,
        instrument: InstrumentId,
        status: OrderStatus,
        remaining_qty: Option<Decimal>,
        broker_order_id: &str,
    ) -> BrokerOrderSnapshot {
        BrokerOrderSnapshot {
            broker_order_id: Some(BrokerOrderId::new(broker_order_id)),
            ..order(account_id, instrument, status, remaining_qty)
        }
    }

    fn trade(
        account_id: &BrokerAccountId,
        broker_order_id: &str,
        instrument: InstrumentId,
        side: OrderSide,
        qty: Decimal,
    ) -> BrokerTradeSnapshot {
        let now = Utc::now();
        BrokerTradeSnapshot {
            account_id: account_id.clone(),
            broker_trade_id: BrokerTradeId::new(format!("TRADE-{broker_order_id}")),
            broker_order_id: Some(BrokerOrderId::new(broker_order_id)),
            client_order_id: None,
            instrument,
            side,
            qty,
            price: Decimal::new(1000, 0),
            gross_amount: None,
            commission: None,
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: now,
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

    #[test]
    fn orphan_order_truth_is_derived_from_account_instrument_and_correlation() {
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let other_account = BrokerAccountId::new("ACC_TEST_0002");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let unknown_without_venue = instrument("IMOEXF", None);
        let unknown_contract = instrument("RI", Some("RIU6@RTSX"));
        let mut missing_correlation = order(
            &account_id,
            target.clone(),
            OrderStatus::Working,
            Some(Decimal::ONE),
        );
        missing_correlation.broker_order_id = None;
        missing_correlation.client_order_id = None;
        let other_account_order = order(
            &other_account,
            target.clone(),
            OrderStatus::Working,
            Some(Decimal::ONE),
        );
        let missing_instrument_identity = order(
            &account_id,
            unknown_without_venue,
            OrderStatus::Working,
            Some(Decimal::ONE),
        );
        let unknown_instrument_identity = order(
            &account_id,
            unknown_contract,
            OrderStatus::Working,
            Some(Decimal::ONE),
        );
        let truth = BrokerTruthSnapshot {
            account_id: account_id.clone(),
            orders: vec![
                order(
                    &account_id,
                    target.clone(),
                    OrderStatus::Working,
                    Some(Decimal::ONE),
                ),
                missing_correlation.clone(),
                other_account_order.clone(),
                missing_instrument_identity.clone(),
                unknown_instrument_identity.clone(),
            ],
            positions: Vec::new(),
            cash: None,
            trades: Vec::new(),
            instruments: vec![instrument_spec(
                "IMOEXF@RTSX",
                "ASSET-2026-09",
                "RTSX",
                NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"),
            )],
            received_ts: Utc::now(),
        };

        assert!(truth.orphan_reasons_for_order(&truth.orders[0]).is_empty());
        assert!(truth
            .orphan_reasons_for_order(&missing_correlation)
            .contains(&BrokerOrderOrphanReason::MissingCorrelationId));
        assert!(truth
            .orphan_reasons_for_order(&other_account_order)
            .contains(&BrokerOrderOrphanReason::AccountMismatch));
        assert!(truth
            .orphan_reasons_for_order(&missing_instrument_identity)
            .contains(&BrokerOrderOrphanReason::UnknownInstrumentIdentity));
        assert!(truth
            .orphan_reasons_for_order(&unknown_instrument_identity)
            .contains(&BrokerOrderOrphanReason::UnknownInstrumentIdentity));
        assert_eq!(truth.account_orphan_order_count(), 4);
        assert_eq!(
            truth
                .summarize_for_instrument(&target)
                .account_orphan_orders_count,
            4
        );
    }

    #[test]
    fn orphan_order_truth_is_derived_from_order_trade_mismatch() {
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let other_account = BrokerAccountId::new("ACC_TEST_0002");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let ri = instrument("RI", Some("RIU6@RTSX"));
        let clean_order = order_with_broker_order_id(
            &account_id,
            target.clone(),
            OrderStatus::Filled,
            Some(Decimal::ZERO),
            "BROKER-CLEAN",
        );
        let missing_trade_order = order_with_broker_order_id(
            &account_id,
            target.clone(),
            OrderStatus::Filled,
            Some(Decimal::ZERO),
            "BROKER-MISSING-TRADE",
        );
        let mismatched_trade_order = order_with_broker_order_id(
            &account_id,
            target.clone(),
            OrderStatus::Filled,
            Some(Decimal::ZERO),
            "BROKER-BAD-TRADE",
        );
        let truth = BrokerTruthSnapshot {
            account_id: account_id.clone(),
            orders: vec![
                clean_order.clone(),
                missing_trade_order.clone(),
                mismatched_trade_order.clone(),
            ],
            positions: Vec::new(),
            cash: None,
            trades: vec![
                trade(
                    &account_id,
                    "BROKER-CLEAN",
                    target.clone(),
                    OrderSide::Buy,
                    Decimal::ONE,
                ),
                trade(
                    &other_account,
                    "BROKER-BAD-TRADE",
                    ri,
                    OrderSide::Sell,
                    Decimal::new(5, 1),
                ),
            ],
            instruments: vec![instrument_spec(
                "IMOEXF@RTSX",
                "ASSET-2026-09",
                "RTSX",
                NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"),
            )],
            received_ts: Utc::now(),
        };

        assert!(truth.orphan_reasons_for_order(&clean_order).is_empty());
        assert!(truth
            .orphan_reasons_for_order(&missing_trade_order)
            .contains(&BrokerOrderOrphanReason::FilledQuantityWithoutMatchingTrade));
        let mismatched_reasons = truth.orphan_reasons_for_order(&mismatched_trade_order);
        assert!(mismatched_reasons.contains(&BrokerOrderOrphanReason::MatchingTradeAccountMismatch));
        assert!(
            mismatched_reasons.contains(&BrokerOrderOrphanReason::MatchingTradeInstrumentMismatch)
        );
        assert!(mismatched_reasons.contains(&BrokerOrderOrphanReason::MatchingTradeSideMismatch));
        assert!(mismatched_reasons
            .contains(&BrokerOrderOrphanReason::MatchingTradeQuantityLessThanFilledQuantity));
        assert_eq!(truth.account_orphan_order_count(), 2);
    }

    #[test]
    fn orphan_order_truth_blocks_missing_instrument_registry() {
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let order = order(
            &account_id,
            target.clone(),
            OrderStatus::Working,
            Some(Decimal::ONE),
        );
        let truth = empty_truth(account_id, vec![order.clone()]);

        let reasons = truth.orphan_reasons_for_order(&order);
        assert!(reasons.contains(&BrokerOrderOrphanReason::MissingInstrumentRegistry));
        assert_eq!(
            truth
                .summarize_for_instrument(&target)
                .account_orphan_orders_count,
            1
        );
    }

    #[test]
    fn orphan_order_truth_blocks_ambiguous_same_venue_instrument_registry() {
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let order = order(
            &account_id,
            target.clone(),
            OrderStatus::Working,
            Some(Decimal::ONE),
        );
        let truth = BrokerTruthSnapshot {
            account_id,
            orders: vec![order.clone()],
            positions: Vec::new(),
            cash: None,
            trades: Vec::new(),
            instruments: vec![
                instrument_spec(
                    "IMOEXF@RTSX",
                    "ASSET-2026-09",
                    "RTSX",
                    NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"),
                ),
                instrument_spec(
                    "IMOEXF@RTSX",
                    "ASSET-2026-12",
                    "FORTS",
                    NaiveDate::from_ymd_opt(2026, 12, 17).expect("date"),
                ),
            ],
            received_ts: Utc::now(),
        };

        assert!(truth
            .orphan_reasons_for_order(&order)
            .contains(&BrokerOrderOrphanReason::AmbiguousInstrumentIdentity));
        assert_eq!(truth.account_orphan_order_count(), 1);
    }

    #[test]
    fn enriched_order_identity_disambiguates_same_venue_instrument_registry() {
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let mut order = order(
            &account_id,
            target,
            OrderStatus::Working,
            Some(Decimal::ONE),
        );
        order.broker_asset_id = Some("ASSET-2026-09".to_string());
        order.board = Some("RTSX".to_string());
        order.expiration_date = Some(NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"));
        let truth = BrokerTruthSnapshot {
            account_id,
            orders: vec![order.clone()],
            positions: Vec::new(),
            cash: None,
            trades: Vec::new(),
            instruments: vec![
                instrument_spec(
                    "IMOEXF@RTSX",
                    "ASSET-2026-09",
                    "RTSX",
                    NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"),
                ),
                instrument_spec(
                    "IMOEXF@RTSX",
                    "ASSET-2026-12",
                    "FORTS",
                    NaiveDate::from_ymd_opt(2026, 12, 17).expect("date"),
                ),
            ],
            received_ts: Utc::now(),
        };

        assert!(truth.orphan_reasons_for_order(&order).is_empty());
        assert_eq!(truth.account_orphan_order_count(), 0);
    }

    #[test]
    fn enriched_order_trade_identity_blocks_same_venue_different_contract_mismatch() {
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let mut order = order_with_broker_order_id(
            &account_id,
            target.clone(),
            OrderStatus::Filled,
            Some(Decimal::ZERO),
            "BROKER-SAME-VENUE",
        );
        order.broker_asset_id = Some("ASSET-2026-09".to_string());
        order.board = Some("RTSX".to_string());
        order.expiration_date = Some(NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"));
        let mut trade = trade(
            &account_id,
            "BROKER-SAME-VENUE",
            target,
            OrderSide::Buy,
            Decimal::ONE,
        );
        trade.broker_asset_id = Some("ASSET-2026-12".to_string());
        trade.board = Some("FORTS".to_string());
        trade.expiration_date = Some(NaiveDate::from_ymd_opt(2026, 12, 17).expect("date"));
        let truth = BrokerTruthSnapshot {
            account_id,
            orders: vec![order.clone()],
            positions: Vec::new(),
            cash: None,
            trades: vec![trade],
            instruments: vec![instrument_spec(
                "IMOEXF@RTSX",
                "ASSET-2026-09",
                "RTSX",
                NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"),
            )],
            received_ts: Utc::now(),
        };

        let reasons = truth.orphan_reasons_for_order(&order);
        assert!(reasons.contains(&BrokerOrderOrphanReason::MatchingTradeInstrumentMismatch));
        assert!(
            reasons.contains(&BrokerOrderOrphanReason::MatchingTradeQuantityLessThanFilledQuantity)
        );
        assert_eq!(truth.account_orphan_order_count(), 1);
    }

    #[test]
    fn derived_orphan_order_count_blocks_combined_preflight_decision() {
        use crate::operational_config::{
            BrokerCanonicalPreflightBlock, BrokerCanonicalPreflightDecision,
            BrokerLiveEntryDecision,
        };

        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let mut orphan_order = order(
            &account_id,
            target.clone(),
            OrderStatus::Working,
            Some(Decimal::ONE),
        );
        orphan_order.broker_order_id = None;
        orphan_order.client_order_id = None;
        let truth = empty_truth(account_id, vec![orphan_order]);
        let summary = truth.summarize_for_instrument(&target);

        assert_eq!(summary.account_orphan_orders_count, 1);
        let decision = BrokerCanonicalPreflightDecision::from_readiness_margin_and_truth(
            BrokerLiveEntryDecision {
                allowed: true,
                blocks: Vec::new(),
            },
            BrokerOrderMarginSufficiency::Sufficient {
                required_margin: Decimal::ONE,
            },
            summary,
        );

        assert!(!decision.allowed);
        assert!(decision
            .blocks
            .contains(&BrokerCanonicalPreflightBlock::AccountOrphanOrdersPresent));
    }

    #[test]
    fn canonical_truth_exposes_target_flat_and_cash_margin_helpers() {
        let now = Utc::now();
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let truth = BrokerTruthSnapshot {
            account_id: account_id.clone(),
            orders: Vec::new(),
            positions: vec![BrokerPositionSnapshot {
                account_id: account_id.clone(),
                instrument: target.clone(),
                qty: Decimal::ZERO,
                avg_price: None,
                unrealized_pnl: None,
                source_ts: None,
                received_ts: now,
            }],
            cash: Some(BrokerCashSnapshot {
                account_id,
                cash: vec![CashPosition {
                    currency: "RUB".to_string(),
                    amount: Decimal::new(6000, 0),
                }],
                equity: Some(Decimal::new(6000, 0)),
                free_cash: Some(Decimal::new(6000, 0)),
                initial_margin: Some(Decimal::new(5000, 0)),
                maintenance_margin: Some(Decimal::new(4000, 0)),
                source_ts: None,
                received_ts: now,
            }),
            trades: Vec::new(),
            instruments: Vec::new(),
            received_ts: now,
        };

        assert!(truth.target_is_flat(&target));
        assert_eq!(truth.cash_by_currency("rub"), Some(Decimal::new(6000, 0)));
        assert_eq!(
            truth.margin_sufficiency_for_order(Decimal::new(5000, 0)),
            BrokerMarginSufficiency::Sufficient
        );
        assert_eq!(
            truth.margin_sufficiency_for_order(Decimal::new(7000, 0)),
            BrokerMarginSufficiency::Insufficient
        );
    }

    #[test]
    fn missing_cash_or_free_cash_blocks_margin_sufficiency() {
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let now = Utc::now();
        let no_cash = BrokerTruthSnapshot {
            account_id: account_id.clone(),
            orders: Vec::new(),
            positions: Vec::new(),
            cash: None,
            trades: Vec::new(),
            instruments: Vec::new(),
            received_ts: now,
        };
        let no_free_cash = BrokerTruthSnapshot {
            account_id: account_id.clone(),
            orders: Vec::new(),
            positions: Vec::new(),
            cash: Some(BrokerCashSnapshot {
                account_id,
                cash: Vec::new(),
                equity: None,
                free_cash: None,
                initial_margin: None,
                maintenance_margin: None,
                source_ts: None,
                received_ts: now,
            }),
            trades: Vec::new(),
            instruments: Vec::new(),
            received_ts: now,
        };

        assert_eq!(
            no_cash.margin_sufficiency_for_order(Decimal::ONE),
            BrokerMarginSufficiency::MissingCashSnapshot
        );
        assert_eq!(
            no_free_cash.margin_sufficiency_for_order(Decimal::ONE),
            BrokerMarginSufficiency::MissingFreeCash
        );
    }

    fn instrument_spec(
        broker_symbol: &str,
        asset_id: &str,
        board: &str,
        expiration_date: NaiveDate,
    ) -> BrokerInstrumentSpec {
        BrokerInstrumentSpec {
            instrument: InstrumentMapEntry {
                internal_symbol: InternalSymbol("IMOEXF".to_string()),
                broker: BrokerKind::Finam,
                broker_symbol: BrokerSymbol(broker_symbol.to_string()),
                exchange: Exchange::Moex,
                market: Market::Futures,
                price_step: Decimal::new(5, 1),
                qty_step: Decimal::ONE,
                lot_size: Decimal::ONE,
                min_qty: Decimal::ONE,
                step_value: Decimal::new(5, 0),
                currency: "RUB".to_string(),
                schedule_id: board.to_string(),
                expiration_date: Some(expiration_date),
                is_tradable: true,
            },
            broker_asset_id: Some(asset_id.to_string()),
            board: Some(board.to_string()),
            long_initial_margin: Some(Decimal::new(5000, 0)),
            short_initial_margin: Some(Decimal::new(5200, 0)),
        }
    }

    #[test]
    fn instrument_spec_identity_includes_board_expiry_and_asset_id() {
        let base = instrument_spec(
            "IMOEXF@RTSX",
            "ASSET-2026-09",
            "RTSX",
            NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"),
        );
        let same = instrument_spec(
            "IMOEXF@RTSX",
            "ASSET-2026-09",
            "RTSX",
            NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"),
        );
        let different_expiry = instrument_spec(
            "IMOEXF@RTSX",
            "ASSET-2026-09",
            "RTSX",
            NaiveDate::from_ymd_opt(2026, 12, 17).expect("date"),
        );
        let different_asset_id = instrument_spec(
            "IMOEXF@RTSX",
            "ASSET-2026-12",
            "RTSX",
            NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"),
        );
        let different_board = instrument_spec(
            "IMOEXF@RTSX",
            "ASSET-2026-09",
            "FUT",
            NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"),
        );

        assert!(instrument_spec_identity_matches(&base, &same));
        assert!(!instrument_spec_identity_matches(&base, &different_expiry));
        assert!(!instrument_spec_identity_matches(
            &base,
            &different_asset_id
        ));
        assert!(!instrument_spec_identity_matches(&base, &different_board));
    }

    #[test]
    fn instrument_derived_required_margin_uses_side_qty_and_reference_price_guardrails() {
        let now = Utc::now();
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let truth = BrokerTruthSnapshot {
            account_id: account_id.clone(),
            orders: Vec::new(),
            positions: Vec::new(),
            cash: Some(BrokerCashSnapshot {
                account_id,
                cash: vec![CashPosition {
                    currency: "RUB".to_string(),
                    amount: Decimal::new(6000, 0),
                }],
                equity: Some(Decimal::new(6000, 0)),
                free_cash: Some(Decimal::new(6000, 0)),
                initial_margin: Some(Decimal::new(5000, 0)),
                maintenance_margin: Some(Decimal::new(4000, 0)),
                source_ts: None,
                received_ts: now,
            }),
            trades: Vec::new(),
            instruments: vec![instrument_spec(
                "IMOEXF@RTSX",
                "ASSET-2026-09",
                "RTSX",
                NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"),
            )],
            received_ts: now,
        };

        assert_eq!(
            truth.required_margin_for_order(
                &target,
                OrderSide::Buy,
                Decimal::ONE,
                Decimal::new(22275, 1),
            ),
            BrokerRequiredMargin::Derived {
                amount: Decimal::new(5000, 0)
            }
        );
        assert_eq!(
            truth.margin_sufficiency_for_instrument_order(
                &target,
                OrderSide::Sell,
                Decimal::ONE,
                Decimal::new(22275, 1),
            ),
            BrokerOrderMarginSufficiency::Sufficient {
                required_margin: Decimal::new(5200, 0)
            }
        );
        assert_eq!(
            truth.margin_sufficiency_for_instrument_order(
                &target,
                OrderSide::Buy,
                Decimal::new(2, 0),
                Decimal::new(22275, 1),
            ),
            BrokerOrderMarginSufficiency::Insufficient {
                required_margin: Decimal::new(10000, 0)
            }
        );
        assert_eq!(
            truth.required_margin_for_order(&target, OrderSide::Buy, Decimal::ZERO, Decimal::ONE),
            BrokerRequiredMargin::Missing(BrokerRequiredMarginFailure::InvalidQuantity)
        );
        assert_eq!(
            truth.required_margin_for_order(&target, OrderSide::Buy, Decimal::ONE, Decimal::ZERO),
            BrokerRequiredMargin::Missing(BrokerRequiredMarginFailure::InvalidReferencePrice)
        );
    }

    #[test]
    fn missing_instrument_margin_is_explicit_not_silent_sufficient() {
        let target = instrument("IMOEXF", Some("IMOEXF@RTSX"));
        let mut spec = instrument_spec(
            "IMOEXF@RTSX",
            "ASSET-2026-09",
            "RTSX",
            NaiveDate::from_ymd_opt(2026, 9, 17).expect("date"),
        );
        spec.long_initial_margin = None;
        let truth = BrokerTruthSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            orders: Vec::new(),
            positions: Vec::new(),
            cash: None,
            trades: Vec::new(),
            instruments: vec![spec],
            received_ts: Utc::now(),
        };

        assert_eq!(
            truth.required_margin_for_order(&target, OrderSide::Buy, Decimal::ONE, Decimal::ONE),
            BrokerRequiredMargin::Missing(BrokerRequiredMarginFailure::MissingInitialMargin)
        );
        assert_eq!(
            truth.margin_sufficiency_for_instrument_order(
                &target,
                OrderSide::Buy,
                Decimal::ONE,
                Decimal::ONE,
            ),
            BrokerOrderMarginSufficiency::MissingInitialMargin
        );
    }
}
