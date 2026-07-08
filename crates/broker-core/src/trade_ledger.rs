use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::ids::{
    deserialize_broker_order_id_legacy_numeric_or_string, BrokerOrderId, BrokerTradeId,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeRecord {
    pub ts_utc: i64,
    pub trade_id: Option<BrokerTradeId>,
    #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
    pub order_id: BrokerOrderId,
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub price: f64,
    pub commission: f64,
    pub owned: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderRecord {
    #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
    pub order_id: BrokerOrderId,
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub filled: f64,
    pub price: f64,
    pub status: String,
    pub ts_utc: i64,
    pub owned: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClosedTradeRecord {
    pub entry_ts_utc: i64,
    pub exit_ts_utc: i64,
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub entry_price: f64,
    pub exit_price: f64,
    pub commission_total: f64,
    pub pnl_gross: f64,
    pub pnl_net: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerSummary {
    pub strategy_id: String,
    pub symbol: String,
    pub trades_total: usize,
    pub win_rate: f64,
    pub pnl_gross_total: f64,
    pub pnl_net_total: f64,
    pub commission_total: f64,
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub avg_pnl: f64,
    pub max_pnl: f64,
    pub min_pnl: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeLedgerBlockerKind {
    PendingExactBrokerOrderMatch,
    ObservedOrderNotStrategyOwned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeLedgerBlocker {
    pub kind: TradeLedgerBlockerKind,
    pub order_id: BrokerOrderId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeLedgerFillDisposition {
    StrategyAttributed,
    ObservedOnly,
    PendingExactBrokerOrderMatch,
    DuplicateIdempotent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeLedgerFillApplyOutcome {
    pub order_id: BrokerOrderId,
    pub disposition: TradeLedgerFillDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeLedgerOrderApplyOutcome {
    pub order_id: BrokerOrderId,
    pub adopted_pending_strategy_trades: usize,
    pub observed_pending_trades: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct TradeLedgerTradeKey {
    trade_id: BrokerTradeId,
    order_id: BrokerOrderId,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeLedger {
    orders: HashMap<BrokerOrderId, OrderRecord>,
    trades: Vec<TradeRecord>,
    observed_trades: Vec<TradeRecord>,
    pending_trades_by_order_id: HashMap<BrokerOrderId, Vec<TradeRecord>>,
    active_blockers: Vec<TradeLedgerBlocker>,
    blocker_history: Vec<TradeLedgerBlocker>,
    seen_trade_keys: HashSet<TradeLedgerTradeKey>,
    closed_trades: Vec<ClosedTradeRecord>,
    realized_pnl: f64,
    position_qty: f64,
    position_cost: f64,
    entry_ts_utc: Option<i64>,
    entry_price: f64,
    entry_side: Option<String>,
    entry_symbol: Option<String>,
    open_commission_total: f64,
}

impl TradeLedger {
    pub fn record_order(&mut self, record: OrderRecord) -> TradeLedgerOrderApplyOutcome {
        let order_id = record.order_id.clone();
        let owned = record.owned;
        self.orders.insert(order_id.clone(), record);

        let pending = self
            .pending_trades_by_order_id
            .remove(&order_id)
            .unwrap_or_default();
        if !pending.is_empty() {
            self.resolve_active_blocker(
                TradeLedgerBlockerKind::PendingExactBrokerOrderMatch,
                &order_id,
            );
        }

        let mut adopted_pending_strategy_trades = 0;
        let mut observed_pending_trades = 0;
        for mut trade in pending {
            trade.owned = owned;
            if owned {
                self.apply_owned_fill(trade);
                adopted_pending_strategy_trades += 1;
            } else {
                self.observed_trades.push(trade);
                self.activate_blocker(
                    TradeLedgerBlockerKind::ObservedOrderNotStrategyOwned,
                    order_id.clone(),
                );
                observed_pending_trades += 1;
            }
        }

        TradeLedgerOrderApplyOutcome {
            order_id,
            adopted_pending_strategy_trades,
            observed_pending_trades,
        }
    }

    pub fn record_fill(&mut self, mut trade: TradeRecord) -> TradeLedgerFillApplyOutcome {
        let order_id = trade.order_id.clone();
        if self.mark_duplicate_if_seen(&trade) {
            return TradeLedgerFillApplyOutcome {
                order_id,
                disposition: TradeLedgerFillDisposition::DuplicateIdempotent,
            };
        }

        match self.orders.get(&order_id) {
            Some(order) if order.owned => {
                trade.owned = true;
                self.apply_owned_fill(trade);
                TradeLedgerFillApplyOutcome {
                    order_id,
                    disposition: TradeLedgerFillDisposition::StrategyAttributed,
                }
            }
            Some(_) => {
                trade.owned = false;
                self.observed_trades.push(trade);
                self.activate_blocker(
                    TradeLedgerBlockerKind::ObservedOrderNotStrategyOwned,
                    order_id.clone(),
                );
                TradeLedgerFillApplyOutcome {
                    order_id,
                    disposition: TradeLedgerFillDisposition::ObservedOnly,
                }
            }
            None => {
                self.pending_trades_by_order_id
                    .entry(order_id.clone())
                    .or_default()
                    .push(trade);
                self.activate_blocker(
                    TradeLedgerBlockerKind::PendingExactBrokerOrderMatch,
                    order_id.clone(),
                );
                TradeLedgerFillApplyOutcome {
                    order_id,
                    disposition: TradeLedgerFillDisposition::PendingExactBrokerOrderMatch,
                }
            }
        }
    }

    pub fn summary(&self, strategy_id: &str, symbol: &str) -> LedgerSummary {
        let trades_total = self.closed_trades.len();
        let mut gross_profit = 0.0;
        let mut gross_loss = 0.0;
        let mut max_pnl = 0.0;
        let mut min_pnl = 0.0;
        let mut pnl_gross_total = 0.0;
        let mut pnl_net_total = 0.0;
        let mut commission_total = 0.0;
        for (idx, trade) in self.closed_trades.iter().enumerate() {
            pnl_gross_total += trade.pnl_gross;
            pnl_net_total += trade.pnl_net;
            commission_total += trade.commission_total;
            if trade.pnl_gross >= 0.0 {
                gross_profit += trade.pnl_gross;
            } else {
                gross_loss += trade.pnl_gross.abs();
            }
            if idx == 0 || trade.pnl_net > max_pnl {
                max_pnl = trade.pnl_net;
            }
            if idx == 0 || trade.pnl_net < min_pnl {
                min_pnl = trade.pnl_net;
            }
        }
        let avg_pnl = if trades_total == 0 {
            0.0
        } else {
            pnl_net_total / trades_total as f64
        };
        let wins = self
            .closed_trades
            .iter()
            .filter(|trade| trade.pnl_net > 0.0)
            .count();
        let win_rate = if trades_total == 0 {
            0.0
        } else {
            wins as f64 / trades_total as f64
        };
        LedgerSummary {
            strategy_id: strategy_id.to_string(),
            symbol: symbol.to_string(),
            trades_total,
            win_rate,
            pnl_gross_total,
            pnl_net_total,
            commission_total,
            gross_profit,
            gross_loss,
            avg_pnl,
            max_pnl,
            min_pnl,
        }
    }

    pub fn order(&self, order_id: &BrokerOrderId) -> Option<&OrderRecord> {
        self.orders.get(order_id)
    }

    pub fn orders_total(&self) -> usize {
        self.orders.len()
    }

    pub fn trades(&self) -> &[TradeRecord] {
        &self.trades
    }

    pub fn observed_trades(&self) -> &[TradeRecord] {
        &self.observed_trades
    }

    pub fn pending_trades(&self, order_id: &BrokerOrderId) -> Option<&[TradeRecord]> {
        self.pending_trades_by_order_id
            .get(order_id)
            .map(Vec::as_slice)
    }

    pub fn pending_trades_total(&self) -> usize {
        self.pending_trades_by_order_id.values().map(Vec::len).sum()
    }

    pub fn blockers(&self) -> &[TradeLedgerBlocker] {
        &self.active_blockers
    }

    pub fn active_blockers(&self) -> &[TradeLedgerBlocker] {
        &self.active_blockers
    }

    pub fn blocker_history(&self) -> &[TradeLedgerBlocker] {
        &self.blocker_history
    }

    pub fn closed_trades(&self) -> &[ClosedTradeRecord] {
        &self.closed_trades
    }

    fn apply_owned_fill(&mut self, trade: TradeRecord) {
        self.apply_fill(&trade);
        self.trades.push(trade);
    }

    fn mark_duplicate_if_seen(&mut self, trade: &TradeRecord) -> bool {
        let Some(trade_id) = trade.trade_id.clone() else {
            return false;
        };
        let key = TradeLedgerTradeKey {
            trade_id,
            order_id: trade.order_id.clone(),
        };
        !self.seen_trade_keys.insert(key)
    }

    fn activate_blocker(&mut self, kind: TradeLedgerBlockerKind, order_id: BrokerOrderId) {
        if self
            .active_blockers
            .iter()
            .any(|blocker| blocker.kind == kind && blocker.order_id == order_id)
        {
            return;
        }
        let blocker = TradeLedgerBlocker { kind, order_id };
        self.active_blockers.push(blocker.clone());
        self.blocker_history.push(blocker);
    }

    fn resolve_active_blocker(&mut self, kind: TradeLedgerBlockerKind, order_id: &BrokerOrderId) {
        self.active_blockers
            .retain(|blocker| !(blocker.kind == kind && &blocker.order_id == order_id));
    }

    fn apply_fill(&mut self, trade: &TradeRecord) {
        let before_qty = self.position_qty;
        let before_avg_price = if before_qty.abs() <= f64::EPSILON {
            0.0
        } else {
            self.position_cost / before_qty
        };
        let qty = trade.qty;
        let price = trade.price;
        let trade_commission = trade.commission;
        match trade.side.as_str() {
            "buy" => {
                if self.position_qty < 0.0 {
                    let avg_price = self.average_price();
                    let cover_qty = qty.min(self.position_qty.abs());
                    self.realized_pnl += (avg_price - price) * cover_qty;
                    self.position_qty += cover_qty;
                    self.position_cost += avg_price * cover_qty;
                    let remaining = qty - cover_qty;
                    if remaining > 0.0 {
                        self.position_qty += remaining;
                        self.position_cost += remaining * price;
                    }
                } else {
                    self.position_qty += qty;
                    self.position_cost += qty * price;
                }
            }
            "sell" => {
                if self.position_qty > 0.0 {
                    let avg_price = self.average_price();
                    let close_qty = qty.min(self.position_qty);
                    self.realized_pnl += (price - avg_price) * close_qty;
                    self.position_qty -= close_qty;
                    self.position_cost -= avg_price * close_qty;
                    let remaining = qty - close_qty;
                    if remaining > 0.0 {
                        self.position_qty -= remaining;
                        self.position_cost -= remaining * price;
                    }
                } else {
                    self.position_qty -= qty;
                    self.position_cost -= qty * price;
                }
            }
            _ => {}
        }
        let is_flat = self.position_qty.abs() <= f64::EPSILON;
        let was_flat = before_qty.abs() <= f64::EPSILON;
        let flipped = !was_flat && !is_flat && before_qty.signum() != self.position_qty.signum();
        let close_ratio = if flipped && qty > 0.0 {
            (before_qty.abs() / qty).min(1.0)
        } else {
            1.0
        };

        if !was_flat && (is_flat || flipped) {
            let entry_price = if self.entry_price > 0.0 {
                self.entry_price
            } else {
                before_avg_price.abs()
            };
            let entry_side = self.entry_side.clone().unwrap_or_else(|| {
                if before_qty > 0.0 {
                    "buy".to_string()
                } else {
                    "sell".to_string()
                }
            });
            let symbol = self
                .entry_symbol
                .clone()
                .unwrap_or_else(|| trade.symbol.clone());
            let close_qty = before_qty.abs();
            let pnl_gross = if entry_side == "buy" {
                (price - entry_price) * close_qty
            } else {
                (entry_price - price) * close_qty
            };
            let commission_total = if flipped {
                self.open_commission_total + (trade_commission * close_ratio)
            } else {
                self.open_commission_total + trade_commission
            };
            let pnl_net = pnl_gross - commission_total;
            if entry_price > 0.0 {
                self.closed_trades.push(ClosedTradeRecord {
                    entry_ts_utc: self.entry_ts_utc.unwrap_or(trade.ts_utc),
                    exit_ts_utc: trade.ts_utc,
                    symbol,
                    side: entry_side,
                    qty: close_qty,
                    entry_price,
                    exit_price: price,
                    commission_total,
                    pnl_gross,
                    pnl_net,
                });
            }
            self.entry_ts_utc = None;
            self.entry_side = None;
            self.entry_symbol = None;
            self.entry_price = 0.0;
            self.open_commission_total = 0.0;

            if flipped {
                self.entry_ts_utc = Some(trade.ts_utc);
                self.entry_side = Some(if self.position_qty > 0.0 {
                    "buy".to_string()
                } else {
                    "sell".to_string()
                });
                self.entry_symbol = Some(trade.symbol.clone());
                self.entry_price = self.average_price().abs();
                self.open_commission_total = trade_commission * (1.0 - close_ratio);
            }
        } else if was_flat && !is_flat {
            self.entry_ts_utc = Some(trade.ts_utc);
            self.entry_side = Some(trade.side.clone());
            self.entry_symbol = Some(trade.symbol.clone());
            self.entry_price = self.average_price().abs();
            self.open_commission_total = trade_commission;
        } else if !is_flat {
            self.entry_price = self.average_price().abs();
            self.open_commission_total += trade_commission;
        }
    }

    fn average_price(&self) -> f64 {
        if self.position_qty.abs() <= f64::EPSILON {
            0.0
        } else {
            self.position_cost / self.position_qty
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(order_id: &str, owned: bool) -> OrderRecord {
        OrderRecord {
            order_id: BrokerOrderId::new(order_id),
            symbol: "IMOEXF".to_string(),
            side: "buy".to_string(),
            qty: 1.0,
            filled: 0.0,
            price: 2210.0,
            status: "filled".to_string(),
            ts_utc: 1,
            owned,
        }
    }

    fn trade(order_id: &str, side: &str, price: f64) -> TradeRecord {
        TradeRecord {
            ts_utc: 2,
            trade_id: Some(BrokerTradeId::new(format!("TRADE-{order_id}-{side}"))),
            order_id: BrokerOrderId::new(order_id),
            symbol: "IMOEXF".to_string(),
            side: side.to_string(),
            qty: 1.0,
            price,
            commission: 0.0,
            owned: false,
        }
    }

    #[test]
    fn trade_ledger_preserves_broker_order_id_string() {
        let mut ledger = TradeLedger::default();
        let order_id = BrokerOrderId::new("FINAM/ORDER:EXACT_Ё");

        ledger.record_order(order("FINAM/ORDER:EXACT_Ё", true));

        assert_eq!(
            ledger.order(&order_id).expect("order").order_id.as_str(),
            "FINAM/ORDER:EXACT_Ё"
        );
    }

    #[test]
    fn trade_ledger_records_order_and_fill_with_string_id() {
        let mut ledger = TradeLedger::default();
        ledger.record_order(order("FINAM-ORDER-2B6-A", true));
        let outcome = ledger.record_fill(trade("FINAM-ORDER-2B6-A", "buy", 2210.0));

        assert_eq!(
            outcome.disposition,
            TradeLedgerFillDisposition::StrategyAttributed
        );
        assert_eq!(ledger.trades().len(), 1);
        assert_eq!(ledger.trades()[0].order_id.as_str(), "FINAM-ORDER-2B6-A");
        assert_eq!(
            ledger.trades()[0]
                .trade_id
                .as_ref()
                .expect("trade id")
                .as_str(),
            "TRADE-FINAM-ORDER-2B6-A-buy"
        );
    }

    #[test]
    fn trade_ledger_string_order_id_roundtrip() {
        let mut ledger = TradeLedger::default();
        ledger.record_order(order("2033126385648208390", true));

        let json = serde_json::to_string(&ledger).expect("serialize ledger");
        let restored: TradeLedger = serde_json::from_str(&json).expect("deserialize ledger");

        assert!(restored
            .order(&BrokerOrderId::new("2033126385648208390"))
            .is_some());
    }

    #[test]
    fn legacy_numeric_alor_order_id_imports_as_decimal_string() {
        let json = r#"{
            "order_id": 42,
            "symbol": "IMOEXF",
            "side": "buy",
            "qty": 1.0,
            "filled": 1.0,
            "price": 2210.0,
            "status": "filled",
            "ts_utc": 1,
            "owned": true
        }"#;

        let record: OrderRecord = serde_json::from_str(json).expect("legacy order");

        assert_eq!(record.order_id.as_str(), "42");
    }

    #[test]
    fn trade_with_observed_order_not_strategy_attributed_without_ownership() {
        let mut ledger = TradeLedger::default();
        ledger.record_order(order("OBSERVED-ORDER-2B6", false));
        let outcome = ledger.record_fill(trade("OBSERVED-ORDER-2B6", "buy", 2210.0));

        assert_eq!(
            outcome.disposition,
            TradeLedgerFillDisposition::ObservedOnly
        );
        assert!(ledger.trades().is_empty());
        assert_eq!(ledger.observed_trades().len(), 1);
        assert_eq!(
            ledger.blockers()[0].kind,
            TradeLedgerBlockerKind::ObservedOrderNotStrategyOwned
        );
    }

    #[test]
    fn trade_before_order_stays_pending_until_exact_broker_order_id_match() {
        let mut ledger = TradeLedger::default();
        let outcome = ledger.record_fill(trade("FINAM-ORDER-PENDING-2B6", "buy", 2210.0));

        assert_eq!(
            outcome.disposition,
            TradeLedgerFillDisposition::PendingExactBrokerOrderMatch
        );
        assert!(ledger.trades().is_empty());
        assert_eq!(ledger.pending_trades_total(), 1);

        let miss = ledger.record_order(order("FINAM-ORDER-OTHER-2B6", true));
        assert_eq!(miss.adopted_pending_strategy_trades, 0);
        assert_eq!(ledger.pending_trades_total(), 1);

        let matched = ledger.record_order(order("FINAM-ORDER-PENDING-2B6", true));
        assert_eq!(matched.adopted_pending_strategy_trades, 1);
        assert_eq!(ledger.pending_trades_total(), 0);
        assert_eq!(ledger.trades().len(), 1);
    }

    #[test]
    fn unknown_or_orphan_trade_sets_blocker_or_manual_intervention() {
        let mut ledger = TradeLedger::default();
        ledger.record_fill(trade("UNKNOWN-ORDER-2B6", "buy", 2210.0));

        assert_eq!(ledger.blockers().len(), 1);
        assert_eq!(
            ledger.blockers()[0].kind,
            TradeLedgerBlockerKind::PendingExactBrokerOrderMatch
        );
        assert_eq!(ledger.blockers()[0].order_id.as_str(), "UNKNOWN-ORDER-2B6");
    }

    #[test]
    fn pending_trade_blocker_resolves_after_exact_owned_order_match() {
        let mut ledger = TradeLedger::default();
        ledger.record_fill(trade("PENDING-OWNED-2B6A", "buy", 2210.0));

        assert_eq!(
            ledger.active_blockers()[0].kind,
            TradeLedgerBlockerKind::PendingExactBrokerOrderMatch
        );

        let outcome = ledger.record_order(order("PENDING-OWNED-2B6A", true));

        assert_eq!(outcome.adopted_pending_strategy_trades, 1);
        assert!(ledger.active_blockers().is_empty());
        assert_eq!(ledger.blocker_history().len(), 1);
        assert_eq!(ledger.pending_trades_total(), 0);
    }

    #[test]
    fn pending_trade_blocker_turns_into_observed_blocker_after_exact_observed_order_match() {
        let mut ledger = TradeLedger::default();
        ledger.record_fill(trade("PENDING-OBSERVED-2B6A", "buy", 2210.0));

        let outcome = ledger.record_order(order("PENDING-OBSERVED-2B6A", false));

        assert_eq!(outcome.observed_pending_trades, 1);
        assert_eq!(ledger.pending_trades_total(), 0);
        assert_eq!(ledger.active_blockers().len(), 1);
        assert_eq!(
            ledger.active_blockers()[0].kind,
            TradeLedgerBlockerKind::ObservedOrderNotStrategyOwned
        );
        assert_eq!(ledger.blocker_history().len(), 2);
    }

    #[test]
    fn duplicate_trade_id_for_owned_order_is_idempotent_and_does_not_double_count_pnl() {
        let mut ledger = TradeLedger::default();
        ledger.record_order(order("ENTRY-DUP-2B6A", true));
        ledger.record_fill(trade("ENTRY-DUP-2B6A", "buy", 100.0));
        ledger.record_order(order("EXIT-DUP-2B6A", true));

        let exit = trade("EXIT-DUP-2B6A", "sell", 110.0);
        let first = ledger.record_fill(exit.clone());
        let duplicate = ledger.record_fill(exit);

        assert_eq!(
            first.disposition,
            TradeLedgerFillDisposition::StrategyAttributed
        );
        assert_eq!(
            duplicate.disposition,
            TradeLedgerFillDisposition::DuplicateIdempotent
        );
        assert_eq!(ledger.trades().len(), 2);
        assert_eq!(ledger.closed_trades().len(), 1);
        assert_eq!(ledger.closed_trades()[0].pnl_gross, 10.0);
    }

    #[test]
    fn duplicate_trade_id_for_pending_trade_is_idempotent() {
        let mut ledger = TradeLedger::default();
        let pending = trade("PENDING-DUP-2B6A", "buy", 2210.0);

        let first = ledger.record_fill(pending.clone());
        let duplicate = ledger.record_fill(pending);

        assert_eq!(
            first.disposition,
            TradeLedgerFillDisposition::PendingExactBrokerOrderMatch
        );
        assert_eq!(
            duplicate.disposition,
            TradeLedgerFillDisposition::DuplicateIdempotent
        );
        assert_eq!(ledger.pending_trades_total(), 1);
        assert_eq!(ledger.active_blockers().len(), 1);

        let matched = ledger.record_order(order("PENDING-DUP-2B6A", true));
        assert_eq!(matched.adopted_pending_strategy_trades, 1);
        assert_eq!(ledger.trades().len(), 1);
        assert!(ledger.active_blockers().is_empty());
    }

    #[test]
    fn duplicate_trade_id_for_observed_order_is_idempotent() {
        let mut ledger = TradeLedger::default();
        ledger.record_order(order("OBSERVED-DUP-2B6A", false));
        let observed = trade("OBSERVED-DUP-2B6A", "buy", 2210.0);

        let first = ledger.record_fill(observed.clone());
        let duplicate = ledger.record_fill(observed);

        assert_eq!(first.disposition, TradeLedgerFillDisposition::ObservedOnly);
        assert_eq!(
            duplicate.disposition,
            TradeLedgerFillDisposition::DuplicateIdempotent
        );
        assert!(ledger.trades().is_empty());
        assert_eq!(ledger.observed_trades().len(), 1);
        assert_eq!(ledger.active_blockers().len(), 1);
    }

    #[test]
    fn trade_before_order_then_exact_order_has_no_active_pending_exact_blocker() {
        let mut ledger = TradeLedger::default();
        ledger.record_fill(trade("NO-ACTIVE-PENDING-2B6A", "buy", 2210.0));
        ledger.record_order(order("NO-ACTIVE-PENDING-2B6A", true));

        assert!(!ledger.active_blockers().iter().any(|blocker| {
            blocker.kind == TradeLedgerBlockerKind::PendingExactBrokerOrderMatch
                && blocker.order_id.as_str() == "NO-ACTIVE-PENDING-2B6A"
        }));
        assert!(ledger.blockers().is_empty());
        assert_eq!(ledger.blocker_history().len(), 1);
    }

    #[test]
    fn trade_ledger_owned_round_trip_keeps_alor_pnl_semantics() {
        let mut ledger = TradeLedger::default();
        ledger.record_order(order("ENTRY-ORDER-2B6", true));
        ledger.record_fill(trade("ENTRY-ORDER-2B6", "buy", 100.0));
        ledger.record_order(order("EXIT-ORDER-2B6", true));
        ledger.record_fill(trade("EXIT-ORDER-2B6", "sell", 110.0));

        assert_eq!(ledger.closed_trades().len(), 1);
        assert_eq!(ledger.closed_trades()[0].pnl_gross, 10.0);
    }
}
