use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::ids::BrokerAccountId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrokerOrderIntentKind {
    Entry,
    Exit,
    Cancel,
    Replace,
    StopOrBracket,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerTimeoutConfig {
    pub connect_timeout_ms: u64,
    pub request_timeout_ms: u64,
    pub order_submit_timeout_ms: u64,
    pub cancel_timeout_ms: u64,
    pub reconcile_timeout_ms: u64,
    pub stream_heartbeat_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerFreshnessConfig {
    pub account_snapshot_max_age_ms: u64,
    pub positions_max_age_ms: u64,
    pub orders_max_age_ms: u64,
    pub trades_max_age_ms: u64,
    pub quotes_max_age_ms: u64,
    pub instrument_spec_max_age_ms: u64,
    pub schedule_max_age_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerRiskLimitConfig {
    pub max_orders_per_run: u32,
    pub max_position_qty: Decimal,
    pub max_position_lifetime_sec: u64,
    pub require_cash_margin_sufficiency: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerScopeConfig {
    pub allowed_accounts: Vec<BrokerAccountId>,
    pub allowed_symbols: Vec<String>,
    pub allowed_order_types: Vec<String>,
    pub allowed_sessions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerLifecycleConfig {
    pub begin_submit_persistence_required: bool,
    pub request_cancel_persistence_required: bool,
    pub idempotency_marker_required: bool,
    pub one_shot_marker_required: bool,
    pub crash_recovery_state_required: bool,
    pub blind_retry_after_ambiguous_send_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerOperationalConfig {
    pub timeouts: BrokerTimeoutConfig,
    pub freshness: BrokerFreshnessConfig,
    pub risk_limits: BrokerRiskLimitConfig,
    pub scope: BrokerScopeConfig,
    pub lifecycle: BrokerLifecycleConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerCapabilityMatrix {
    pub supports_market_order: bool,
    pub supports_limit_order: bool,
    pub supports_cancel: bool,
    pub supports_replace: bool,
    pub supports_stop_sltp: bool,
    pub supports_brackets: bool,
    pub supports_multi_leg: bool,
    pub supports_readonly_orders: bool,
    pub supports_readonly_trades: bool,
    pub supports_readonly_positions: bool,
    pub supports_streaming_order_updates: bool,
    pub supports_streaming_position_updates: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerFeedFreshness {
    pub observed_ts: Option<DateTime<Utc>>,
    pub max_age_ms: u64,
}

impl BrokerFeedFreshness {
    pub fn is_fresh_at(&self, now: DateTime<Utc>) -> bool {
        let Some(observed_ts) = self.observed_ts else {
            return false;
        };
        let age_ms = now.signed_duration_since(observed_ts).num_milliseconds();
        age_ms >= 0 && age_ms as u64 <= self.max_age_ms
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrokerMarketSessionState {
    Open,
    Closed,
    Break,
    Maintenance,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerReadinessSnapshot {
    pub account: BrokerFeedFreshness,
    pub positions: BrokerFeedFreshness,
    pub orders: BrokerFeedFreshness,
    pub trades: BrokerFeedFreshness,
    pub quotes: BrokerFeedFreshness,
    pub instrument_spec: BrokerFeedFreshness,
    pub schedule: BrokerFeedFreshness,
    pub market_session: BrokerMarketSessionState,
    pub unknown_order_count: usize,
    pub cash_margin_present: bool,
    pub instrument_spec_validated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrokerLiveEntryBlock {
    AccountSnapshotStale,
    PositionsStale,
    OrdersStale,
    TradesStale,
    QuotesStale,
    InstrumentSpecStale,
    ScheduleStale,
    MarketNotOpen,
    UnknownOrdersPresent,
    CashMarginMissing,
    InstrumentSpecNotValidated,
    MarketOrdersUnsupported,
    LimitOrdersUnsupported,
    OrderTypeNotAllowed,
    CancelUnsupported,
    AccountNotAllowed,
    SymbolNotAllowed,
    LifecyclePersistenceMissing,
    BlindRetryAfterAmbiguousSendAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerLiveEntryScope {
    pub account_id: BrokerAccountId,
    pub symbol: String,
    pub order_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerLiveEntryDecision {
    pub allowed: bool,
    pub blocks: Vec<BrokerLiveEntryBlock>,
}

impl BrokerReadinessSnapshot {
    pub fn broker_truth_is_fresh(&self, now: DateTime<Utc>) -> bool {
        self.account.is_fresh_at(now)
            && self.positions.is_fresh_at(now)
            && self.orders.is_fresh_at(now)
            && self.trades.is_fresh_at(now)
            && self.quotes.is_fresh_at(now)
            && self.instrument_spec.is_fresh_at(now)
            && self.schedule.is_fresh_at(now)
    }

    pub fn live_entry_allowed(
        &self,
        now: DateTime<Utc>,
        config: &BrokerOperationalConfig,
        capabilities: &BrokerCapabilityMatrix,
        scope: &BrokerLiveEntryScope,
    ) -> BrokerLiveEntryDecision {
        let mut blocks = Vec::new();
        if !self.account.is_fresh_at(now) {
            blocks.push(BrokerLiveEntryBlock::AccountSnapshotStale);
        }
        if !self.positions.is_fresh_at(now) {
            blocks.push(BrokerLiveEntryBlock::PositionsStale);
        }
        if !self.orders.is_fresh_at(now) {
            blocks.push(BrokerLiveEntryBlock::OrdersStale);
        }
        if !self.trades.is_fresh_at(now) {
            blocks.push(BrokerLiveEntryBlock::TradesStale);
        }
        if !self.quotes.is_fresh_at(now) {
            blocks.push(BrokerLiveEntryBlock::QuotesStale);
        }
        if !self.instrument_spec.is_fresh_at(now) {
            blocks.push(BrokerLiveEntryBlock::InstrumentSpecStale);
        }
        if !self.schedule.is_fresh_at(now) {
            blocks.push(BrokerLiveEntryBlock::ScheduleStale);
        }
        if self.market_session != BrokerMarketSessionState::Open {
            blocks.push(BrokerLiveEntryBlock::MarketNotOpen);
        }
        if self.unknown_order_count > 0 {
            blocks.push(BrokerLiveEntryBlock::UnknownOrdersPresent);
        }
        if config.risk_limits.require_cash_margin_sufficiency && !self.cash_margin_present {
            blocks.push(BrokerLiveEntryBlock::CashMarginMissing);
        }
        if !self.instrument_spec_validated {
            blocks.push(BrokerLiveEntryBlock::InstrumentSpecNotValidated);
        }
        if scope.order_type.eq_ignore_ascii_case("market") && !capabilities.supports_market_order {
            blocks.push(BrokerLiveEntryBlock::MarketOrdersUnsupported);
        }
        if scope.order_type.eq_ignore_ascii_case("limit") && !capabilities.supports_limit_order {
            blocks.push(BrokerLiveEntryBlock::LimitOrdersUnsupported);
        }
        if !config.scope.allowed_accounts.contains(&scope.account_id) {
            blocks.push(BrokerLiveEntryBlock::AccountNotAllowed);
        }
        if !config.scope.allowed_symbols.contains(&scope.symbol) {
            blocks.push(BrokerLiveEntryBlock::SymbolNotAllowed);
        }
        if !config
            .scope
            .allowed_order_types
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&scope.order_type))
        {
            blocks.push(BrokerLiveEntryBlock::OrderTypeNotAllowed);
        }
        if !config.lifecycle.begin_submit_persistence_required
            || !config.lifecycle.idempotency_marker_required
            || !config.lifecycle.one_shot_marker_required
            || !config.lifecycle.crash_recovery_state_required
        {
            blocks.push(BrokerLiveEntryBlock::LifecyclePersistenceMissing);
        }
        if config.lifecycle.blind_retry_after_ambiguous_send_allowed {
            blocks.push(BrokerLiveEntryBlock::BlindRetryAfterAmbiguousSendAllowed);
        }

        BrokerLiveEntryDecision {
            allowed: blocks.is_empty(),
            blocks,
        }
    }

    pub fn emergency_cancel_allowed(
        &self,
        capabilities: &BrokerCapabilityMatrix,
        config: &BrokerOperationalConfig,
        scope: &BrokerLiveEntryScope,
    ) -> BrokerLiveEntryDecision {
        let mut blocks = Vec::new();
        if !capabilities.supports_cancel {
            blocks.push(BrokerLiveEntryBlock::CancelUnsupported);
        }
        if !config.scope.allowed_accounts.contains(&scope.account_id) {
            blocks.push(BrokerLiveEntryBlock::AccountNotAllowed);
        }
        if !config.scope.allowed_symbols.contains(&scope.symbol) {
            blocks.push(BrokerLiveEntryBlock::SymbolNotAllowed);
        }
        BrokerLiveEntryDecision {
            allowed: blocks.is_empty(),
            blocks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn freshness(now: DateTime<Utc>, max_age_ms: u64) -> BrokerFeedFreshness {
        BrokerFeedFreshness {
            observed_ts: Some(now),
            max_age_ms,
        }
    }

    fn config() -> BrokerOperationalConfig {
        BrokerOperationalConfig {
            timeouts: BrokerTimeoutConfig {
                connect_timeout_ms: 5_000,
                request_timeout_ms: 10_000,
                order_submit_timeout_ms: 10_000,
                cancel_timeout_ms: 10_000,
                reconcile_timeout_ms: 30_000,
                stream_heartbeat_timeout_ms: 70_000,
            },
            freshness: BrokerFreshnessConfig {
                account_snapshot_max_age_ms: 120_000,
                positions_max_age_ms: 120_000,
                orders_max_age_ms: 120_000,
                trades_max_age_ms: 120_000,
                quotes_max_age_ms: 30_000,
                instrument_spec_max_age_ms: 86_400_000,
                schedule_max_age_ms: 86_400_000,
            },
            risk_limits: BrokerRiskLimitConfig {
                max_orders_per_run: 1,
                max_position_qty: Decimal::ONE,
                max_position_lifetime_sec: 60,
                require_cash_margin_sufficiency: true,
            },
            scope: BrokerScopeConfig {
                allowed_accounts: vec![BrokerAccountId::new("ACC_TEST_0001")],
                allowed_symbols: vec!["IMOEXF@RTSX".to_string()],
                allowed_order_types: vec!["market".to_string(), "limit".to_string()],
                allowed_sessions: vec!["main".to_string()],
            },
            lifecycle: BrokerLifecycleConfig {
                begin_submit_persistence_required: true,
                request_cancel_persistence_required: true,
                idempotency_marker_required: true,
                one_shot_marker_required: true,
                crash_recovery_state_required: true,
                blind_retry_after_ambiguous_send_allowed: false,
            },
        }
    }

    fn capabilities() -> BrokerCapabilityMatrix {
        BrokerCapabilityMatrix {
            supports_market_order: true,
            supports_limit_order: true,
            supports_cancel: true,
            supports_replace: false,
            supports_stop_sltp: false,
            supports_brackets: false,
            supports_multi_leg: false,
            supports_readonly_orders: true,
            supports_readonly_trades: true,
            supports_readonly_positions: true,
            supports_streaming_order_updates: false,
            supports_streaming_position_updates: false,
        }
    }

    fn readiness(now: DateTime<Utc>) -> BrokerReadinessSnapshot {
        BrokerReadinessSnapshot {
            account: freshness(now, 120_000),
            positions: freshness(now, 120_000),
            orders: freshness(now, 120_000),
            trades: freshness(now, 120_000),
            quotes: freshness(now, 30_000),
            instrument_spec: freshness(now, 86_400_000),
            schedule: freshness(now, 86_400_000),
            market_session: BrokerMarketSessionState::Open,
            unknown_order_count: 0,
            cash_margin_present: true,
            instrument_spec_validated: true,
        }
    }

    fn scope() -> BrokerLiveEntryScope {
        BrokerLiveEntryScope {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            symbol: "IMOEXF@RTSX".to_string(),
            order_type: "market".to_string(),
        }
    }

    #[test]
    fn fresh_canonical_readiness_allows_scoped_market_entry() {
        let now = Utc::now();
        let decision = readiness(now).live_entry_allowed(now, &config(), &capabilities(), &scope());

        assert!(decision.allowed);
        assert!(decision.blocks.is_empty());
    }

    #[test]
    fn stale_positions_orders_quote_or_account_blocks_live_entry() {
        let now = Utc::now();
        let mut readiness = readiness(now);
        readiness.positions.observed_ts = Some(now - Duration::milliseconds(120_001));
        readiness.orders.observed_ts = Some(now - Duration::milliseconds(120_001));
        readiness.quotes.observed_ts = Some(now - Duration::milliseconds(30_001));
        readiness.account.observed_ts = None;

        let decision = readiness.live_entry_allowed(now, &config(), &capabilities(), &scope());

        assert!(!decision.allowed);
        assert!(decision
            .blocks
            .contains(&BrokerLiveEntryBlock::AccountSnapshotStale));
        assert!(decision
            .blocks
            .contains(&BrokerLiveEntryBlock::PositionsStale));
        assert!(decision.blocks.contains(&BrokerLiveEntryBlock::OrdersStale));
        assert!(decision.blocks.contains(&BrokerLiveEntryBlock::QuotesStale));
    }

    #[test]
    fn unknown_orders_missing_cash_and_unvalidated_instrument_block_live_entry() {
        let now = Utc::now();
        let mut readiness = readiness(now);
        readiness.unknown_order_count = 1;
        readiness.cash_margin_present = false;
        readiness.instrument_spec_validated = false;

        let decision = readiness.live_entry_allowed(now, &config(), &capabilities(), &scope());

        assert!(!decision.allowed);
        assert!(decision
            .blocks
            .contains(&BrokerLiveEntryBlock::UnknownOrdersPresent));
        assert!(decision
            .blocks
            .contains(&BrokerLiveEntryBlock::CashMarginMissing));
        assert!(decision
            .blocks
            .contains(&BrokerLiveEntryBlock::InstrumentSpecNotValidated));
    }

    #[test]
    fn market_closed_blocks_entry_but_not_emergency_cancel() {
        let now = Utc::now();
        let mut readiness = readiness(now);
        readiness.market_session = BrokerMarketSessionState::Break;

        let entry = readiness.live_entry_allowed(now, &config(), &capabilities(), &scope());
        let cancel = readiness.emergency_cancel_allowed(&capabilities(), &config(), &scope());

        assert!(!entry.allowed);
        assert!(entry.blocks.contains(&BrokerLiveEntryBlock::MarketNotOpen));
        assert!(cancel.allowed);
    }

    #[test]
    fn lifecycle_persistence_and_blind_retry_policy_block_live_entry() {
        let now = Utc::now();
        let mut config = config();
        config.lifecycle.begin_submit_persistence_required = false;
        config.lifecycle.blind_retry_after_ambiguous_send_allowed = true;

        let decision = readiness(now).live_entry_allowed(now, &config, &capabilities(), &scope());

        assert!(!decision.allowed);
        assert!(decision
            .blocks
            .contains(&BrokerLiveEntryBlock::LifecyclePersistenceMissing));
        assert!(decision
            .blocks
            .contains(&BrokerLiveEntryBlock::BlindRetryAfterAmbiguousSendAllowed));
    }

    #[test]
    fn order_type_scope_blocks_live_entry_even_when_capability_exists() {
        let now = Utc::now();
        let mut config = config();
        config.scope.allowed_order_types = vec!["limit".to_string()];

        let decision = readiness(now).live_entry_allowed(now, &config, &capabilities(), &scope());

        assert!(!decision.allowed);
        assert!(decision
            .blocks
            .contains(&BrokerLiveEntryBlock::OrderTypeNotAllowed));
    }
}
