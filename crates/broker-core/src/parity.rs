use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::event::{Bar, MarketDataSourceKind};
use crate::instrument::{InstrumentId, Quantity};
use crate::operational_snapshot::{BrokerTruthInstrumentSummary, BrokerTruthSnapshot};
use crate::{instrument_identity_matches, BrokerInstrumentSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrokerParityIssueKind {
    TargetPositionQtyMismatch,
    TargetFlatMismatch,
    TargetActiveOrdersMismatch,
    TargetUnknownOrdersMismatch,
    AccountActiveOrdersMismatch,
    AccountUnknownOrdersMismatch,
    AccountOrphanOrdersMismatch,
    OtherSymbolActiveOrdersMismatch,
    ReceivedTimestampSkewExceeded,
    MissingTargetInstrumentSpec,
    AmbiguousTargetInstrumentSpec,
    InstrumentSpecMismatch,
    BarInstrumentMismatch,
    BarFinalityMismatch,
    BarTimeframeMismatch,
    BarOpenTimestampMismatch,
    BarCloseTimestampMismatch,
    BarOhlcvMismatch,
    BarSourceKindMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerParityIssue {
    pub kind: BrokerParityIssueKind,
    pub blocking: bool,
    pub left: String,
    pub right: String,
}

impl BrokerParityIssue {
    fn blocking(kind: BrokerParityIssueKind, left: impl ToString, right: impl ToString) -> Self {
        Self {
            kind,
            blocking: true,
            left: left.to_string(),
            right: right.to_string(),
        }
    }

    fn diagnostic(kind: BrokerParityIssueKind, left: impl ToString, right: impl ToString) -> Self {
        Self {
            kind,
            blocking: false,
            left: left.to_string(),
            right: right.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrokerTruthParityReport {
    pub parity_stage: String,
    pub left_label: String,
    pub right_label: String,
    pub target_instrument: InstrumentId,
    pub left_target_position_qty: Quantity,
    pub right_target_position_qty: Quantity,
    pub left_target_is_flat: bool,
    pub right_target_is_flat: bool,
    pub left_summary: BrokerTruthInstrumentSummary,
    pub right_summary: BrokerTruthInstrumentSummary,
    pub received_ts_skew_ms: i64,
    pub max_received_ts_skew_ms: i64,
    pub issues: Vec<BrokerParityIssue>,
    pub blocking_issue_count: usize,
    pub diagnostic_issue_count: usize,
    pub cutover_safe: bool,
    pub live_order_authorized: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrokerBarParityReport {
    pub parity_stage: String,
    pub left_label: String,
    pub right_label: String,
    pub target_instrument: InstrumentId,
    pub left_source_kind: MarketDataSourceKind,
    pub right_source_kind: MarketDataSourceKind,
    pub open_ts: Option<DateTime<Utc>>,
    pub close_ts: Option<DateTime<Utc>>,
    pub issues: Vec<BrokerParityIssue>,
    pub blocking_issue_count: usize,
    pub diagnostic_issue_count: usize,
    pub bars_synchronized: bool,
    pub live_order_authorized: bool,
}

pub fn compare_broker_truth_for_instrument(
    left_label: impl Into<String>,
    left: &BrokerTruthSnapshot,
    right_label: impl Into<String>,
    right: &BrokerTruthSnapshot,
    target_instrument: &InstrumentId,
    max_received_ts_skew_ms: i64,
) -> BrokerTruthParityReport {
    let left_label = left_label.into();
    let right_label = right_label.into();
    let left_summary = left.summarize_for_instrument(target_instrument);
    let right_summary = right.summarize_for_instrument(target_instrument);
    let left_target_position_qty = left.target_position_qty(target_instrument);
    let right_target_position_qty = right.target_position_qty(target_instrument);
    let left_target_is_flat = left.target_is_flat(target_instrument);
    let right_target_is_flat = right.target_is_flat(target_instrument);
    let received_ts_skew_ms = (left.received_ts - right.received_ts)
        .num_milliseconds()
        .abs();

    let mut issues = Vec::new();
    if left_target_position_qty != right_target_position_qty {
        issues.push(BrokerParityIssue::blocking(
            BrokerParityIssueKind::TargetPositionQtyMismatch,
            left_target_position_qty,
            right_target_position_qty,
        ));
    }
    if left_target_is_flat != right_target_is_flat {
        issues.push(BrokerParityIssue::blocking(
            BrokerParityIssueKind::TargetFlatMismatch,
            left_target_is_flat,
            right_target_is_flat,
        ));
    }
    push_count_mismatch(
        &mut issues,
        BrokerParityIssueKind::TargetActiveOrdersMismatch,
        left_summary.target_active_orders_count,
        right_summary.target_active_orders_count,
        true,
    );
    push_count_mismatch(
        &mut issues,
        BrokerParityIssueKind::TargetUnknownOrdersMismatch,
        left_summary.target_unknown_orders_count,
        right_summary.target_unknown_orders_count,
        true,
    );
    push_count_mismatch(
        &mut issues,
        BrokerParityIssueKind::AccountActiveOrdersMismatch,
        left_summary.account_active_orders_count,
        right_summary.account_active_orders_count,
        true,
    );
    push_count_mismatch(
        &mut issues,
        BrokerParityIssueKind::AccountUnknownOrdersMismatch,
        left_summary.account_unknown_orders_count,
        right_summary.account_unknown_orders_count,
        true,
    );
    push_count_mismatch(
        &mut issues,
        BrokerParityIssueKind::AccountOrphanOrdersMismatch,
        left_summary.account_orphan_orders_count,
        right_summary.account_orphan_orders_count,
        true,
    );
    push_count_mismatch(
        &mut issues,
        BrokerParityIssueKind::OtherSymbolActiveOrdersMismatch,
        left_summary.other_symbol_active_orders_count,
        right_summary.other_symbol_active_orders_count,
        true,
    );
    if received_ts_skew_ms > max_received_ts_skew_ms {
        issues.push(BrokerParityIssue::blocking(
            BrokerParityIssueKind::ReceivedTimestampSkewExceeded,
            received_ts_skew_ms,
            max_received_ts_skew_ms,
        ));
    }
    issues.extend(compare_target_instrument_specs(
        left,
        right,
        target_instrument,
    ));

    let blocking_issue_count = issues.iter().filter(|issue| issue.blocking).count();
    let diagnostic_issue_count = issues.len() - blocking_issue_count;
    BrokerTruthParityReport {
        parity_stage: "M4-3aDualBrokerShadowParity".to_string(),
        left_label,
        right_label,
        target_instrument: target_instrument.clone(),
        left_target_position_qty,
        right_target_position_qty,
        left_target_is_flat,
        right_target_is_flat,
        left_summary,
        right_summary,
        received_ts_skew_ms,
        max_received_ts_skew_ms,
        issues,
        blocking_issue_count,
        diagnostic_issue_count,
        cutover_safe: blocking_issue_count == 0,
        live_order_authorized: false,
    }
}

pub fn compare_final_bars_for_instrument(
    left_label: impl Into<String>,
    left: &Bar,
    right_label: impl Into<String>,
    right: &Bar,
    target_instrument: &InstrumentId,
) -> BrokerBarParityReport {
    let mut issues = Vec::new();

    if !instrument_identity_matches(&left.instrument, target_instrument)
        || !instrument_identity_matches(&right.instrument, target_instrument)
    {
        issues.push(BrokerParityIssue::blocking(
            BrokerParityIssueKind::BarInstrumentMismatch,
            instrument_label(&left.instrument),
            instrument_label(&right.instrument),
        ));
    }
    if !left.is_final || !right.is_final || left.is_final != right.is_final {
        issues.push(BrokerParityIssue::blocking(
            BrokerParityIssueKind::BarFinalityMismatch,
            left.is_final,
            right.is_final,
        ));
    }
    if left.timeframe_sec != right.timeframe_sec {
        issues.push(BrokerParityIssue::blocking(
            BrokerParityIssueKind::BarTimeframeMismatch,
            left.timeframe_sec,
            right.timeframe_sec,
        ));
    }
    if left.open_ts != right.open_ts {
        issues.push(BrokerParityIssue::blocking(
            BrokerParityIssueKind::BarOpenTimestampMismatch,
            left.open_ts.to_rfc3339(),
            right.open_ts.to_rfc3339(),
        ));
    }
    if left.close_ts != right.close_ts {
        issues.push(BrokerParityIssue::blocking(
            BrokerParityIssueKind::BarCloseTimestampMismatch,
            left.close_ts.to_rfc3339(),
            right.close_ts.to_rfc3339(),
        ));
    }
    if left.open != right.open
        || left.high != right.high
        || left.low != right.low
        || left.close != right.close
        || left.volume != right.volume
    {
        issues.push(BrokerParityIssue::blocking(
            BrokerParityIssueKind::BarOhlcvMismatch,
            format!(
                "{}/{}/{}/{}/{}",
                left.open, left.high, left.low, left.close, left.volume
            ),
            format!(
                "{}/{}/{}/{}/{}",
                right.open, right.high, right.low, right.close, right.volume
            ),
        ));
    }
    if left.source_kind != right.source_kind {
        issues.push(BrokerParityIssue::diagnostic(
            BrokerParityIssueKind::BarSourceKindMismatch,
            format!("{:?}", left.source_kind),
            format!("{:?}", right.source_kind),
        ));
    }

    let blocking_issue_count = issues.iter().filter(|issue| issue.blocking).count();
    let diagnostic_issue_count = issues.len() - blocking_issue_count;
    BrokerBarParityReport {
        parity_stage: "M4-3aDualBrokerShadowParity".to_string(),
        left_label: left_label.into(),
        right_label: right_label.into(),
        target_instrument: target_instrument.clone(),
        left_source_kind: left.source_kind,
        right_source_kind: right.source_kind,
        open_ts: (left.open_ts == right.open_ts).then_some(left.open_ts),
        close_ts: (left.close_ts == right.close_ts).then_some(left.close_ts),
        issues,
        blocking_issue_count,
        diagnostic_issue_count,
        bars_synchronized: blocking_issue_count == 0,
        live_order_authorized: false,
    }
}

fn push_count_mismatch(
    issues: &mut Vec<BrokerParityIssue>,
    kind: BrokerParityIssueKind,
    left: usize,
    right: usize,
    blocking: bool,
) {
    if left != right {
        let issue = if blocking {
            BrokerParityIssue::blocking(kind, left, right)
        } else {
            BrokerParityIssue::diagnostic(kind, left, right)
        };
        issues.push(issue);
    }
}

fn compare_target_instrument_specs(
    left: &BrokerTruthSnapshot,
    right: &BrokerTruthSnapshot,
    target_instrument: &InstrumentId,
) -> Vec<BrokerParityIssue> {
    let mut issues = Vec::new();
    let left_specs = specs_for_target(left, target_instrument);
    let right_specs = specs_for_target(right, target_instrument);

    match (left_specs.len(), right_specs.len()) {
        (1, 1) => {
            let left_spec = left_specs[0];
            let right_spec = right_specs[0];
            if !cross_broker_instrument_specs_match(left_spec, right_spec) {
                issues.push(BrokerParityIssue::blocking(
                    BrokerParityIssueKind::InstrumentSpecMismatch,
                    redacted_spec_label(left_spec),
                    redacted_spec_label(right_spec),
                ));
            }
        }
        (0, _) | (_, 0) => {
            issues.push(BrokerParityIssue::blocking(
                BrokerParityIssueKind::MissingTargetInstrumentSpec,
                left_specs.len(),
                right_specs.len(),
            ));
        }
        _ => {
            issues.push(BrokerParityIssue::blocking(
                BrokerParityIssueKind::AmbiguousTargetInstrumentSpec,
                left_specs.len(),
                right_specs.len(),
            ));
        }
    }

    issues
}

fn specs_for_target<'a>(
    snapshot: &'a BrokerTruthSnapshot,
    target_instrument: &InstrumentId,
) -> Vec<&'a BrokerInstrumentSpec> {
    snapshot
        .instruments
        .iter()
        .filter(|spec| spec.matches_instrument_id(target_instrument))
        .collect()
}

fn cross_broker_instrument_specs_match(
    left: &BrokerInstrumentSpec,
    right: &BrokerInstrumentSpec,
) -> bool {
    left.instrument.internal_symbol == right.instrument.internal_symbol
        && left.instrument.exchange == right.instrument.exchange
        && left.instrument.market == right.instrument.market
        && left.instrument.price_step == right.instrument.price_step
        && left.instrument.qty_step == right.instrument.qty_step
        && left.instrument.lot_size == right.instrument.lot_size
        && left.instrument.min_qty == right.instrument.min_qty
        && left.instrument.step_value == right.instrument.step_value
        && left.instrument.currency == right.instrument.currency
        && left.instrument.expiration_date == right.instrument.expiration_date
        && left.instrument.is_tradable == right.instrument.is_tradable
        && left.long_initial_margin == right.long_initial_margin
        && left.short_initial_margin == right.short_initial_margin
}

fn redacted_spec_label(spec: &BrokerInstrumentSpec) -> String {
    format!(
        "{}|{:?}|{:?}|step={}|lot={}|expiry={:?}|tradable={}",
        spec.instrument.internal_symbol.0,
        spec.instrument.exchange,
        spec.instrument.market,
        spec.instrument.price_step,
        spec.instrument.lot_size,
        spec.instrument.expiration_date,
        spec.instrument.is_tradable,
    )
}

fn instrument_label(instrument: &InstrumentId) -> String {
    format!(
        "{}|venue={}|{:?}|{:?}",
        instrument.symbol,
        instrument.venue_symbol.as_deref().unwrap_or("<none>"),
        instrument.exchange,
        instrument.market
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::BrokerKind;
    use crate::ids::BrokerAccountId;
    use crate::instrument::{
        BrokerSymbol, Exchange, InstrumentMapEntry, InternalSymbol, Market, Price,
    };
    use crate::operational_snapshot::{BrokerInstrumentSpec, BrokerPositionSnapshot};
    use rust_decimal::Decimal;

    fn target() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: None,
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn venue_instrument(symbol: &str, venue_symbol: &str) -> InstrumentId {
        InstrumentId {
            symbol: symbol.to_string(),
            venue_symbol: Some(venue_symbol.to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn spec(broker: BrokerKind, broker_symbol: &str) -> BrokerInstrumentSpec {
        BrokerInstrumentSpec {
            instrument: InstrumentMapEntry {
                internal_symbol: InternalSymbol("IMOEXF".to_string()),
                broker,
                broker_symbol: BrokerSymbol(broker_symbol.to_string()),
                exchange: Exchange::Moex,
                market: Market::Futures,
                price_step: dec("0.5"),
                qty_step: Decimal::ONE,
                lot_size: Decimal::ONE,
                min_qty: Decimal::ONE,
                step_value: dec("5"),
                currency: "RUB".to_string(),
                schedule_id: "moex-forts".to_string(),
                expiration_date: None,
                is_tradable: true,
            },
            broker_asset_id: None,
            board: None,
            long_initial_margin: Some(dec("5000")),
            short_initial_margin: Some(dec("5000")),
        }
    }

    fn snapshot(
        account_id: &str,
        instrument: InstrumentId,
        qty: Decimal,
        spec: BrokerInstrumentSpec,
        received_ts: DateTime<Utc>,
    ) -> BrokerTruthSnapshot {
        BrokerTruthSnapshot {
            account_id: BrokerAccountId::new(account_id),
            orders: Vec::new(),
            positions: vec![BrokerPositionSnapshot {
                account_id: BrokerAccountId::new(account_id),
                instrument,
                qty,
                avg_price: None,
                unrealized_pnl: None,
                source_ts: Some(received_ts),
                received_ts,
            }],
            cash: None,
            trades: Vec::new(),
            instruments: vec![spec],
            received_ts,
        }
    }

    fn bar(source_kind: MarketDataSourceKind, close: Price) -> Bar {
        let open_ts = DateTime::parse_from_rfc3339("2026-07-05T09:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        Bar {
            instrument: target(),
            source_kind,
            timeframe_sec: 600,
            open_ts,
            close_ts: open_ts + chrono::Duration::seconds(600),
            open: dec("2247"),
            high: dec("2249.5"),
            low: dec("2247"),
            close,
            volume: dec("815"),
            is_final: true,
        }
    }

    fn dec(value: &str) -> Decimal {
        value.parse().expect("decimal")
    }

    #[test]
    fn broker_truth_parity_accepts_cross_broker_flat_same_target() {
        let now = Utc::now();
        let alor = snapshot(
            "ACC_ALOR_TEST",
            venue_instrument("IMOEXF", "IMOEXF"),
            Decimal::ZERO,
            spec(BrokerKind::Alor, "IMOEXF"),
            now,
        );
        let finam = snapshot(
            "ACC_FINAM_TEST",
            venue_instrument("IMOEXF", "IMOEXF@RTSX"),
            Decimal::ZERO,
            spec(BrokerKind::Finam, "IMOEXF@RTSX"),
            now,
        );

        let report =
            compare_broker_truth_for_instrument("alor", &alor, "finam", &finam, &target(), 30_000);

        assert!(report.cutover_safe);
        assert!(!report.live_order_authorized);
        assert_eq!(report.blocking_issue_count, 0);
    }

    #[test]
    fn broker_truth_parity_blocks_target_qty_mismatch() {
        let now = Utc::now();
        let alor = snapshot(
            "ACC_ALOR_TEST",
            venue_instrument("IMOEXF", "IMOEXF"),
            Decimal::ZERO,
            spec(BrokerKind::Alor, "IMOEXF"),
            now,
        );
        let finam = snapshot(
            "ACC_FINAM_TEST",
            venue_instrument("IMOEXF", "IMOEXF@RTSX"),
            Decimal::ONE,
            spec(BrokerKind::Finam, "IMOEXF@RTSX"),
            now,
        );

        let report =
            compare_broker_truth_for_instrument("alor", &alor, "finam", &finam, &target(), 30_000);

        assert!(!report.cutover_safe);
        assert!(report.issues.iter().any(|issue| {
            issue.kind == BrokerParityIssueKind::TargetPositionQtyMismatch && issue.blocking
        }));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.kind == BrokerParityIssueKind::TargetFlatMismatch));
    }

    #[test]
    fn broker_truth_parity_blocks_received_ts_skew() {
        let now = Utc::now();
        let alor = snapshot(
            "ACC_ALOR_TEST",
            venue_instrument("IMOEXF", "IMOEXF"),
            Decimal::ZERO,
            spec(BrokerKind::Alor, "IMOEXF"),
            now,
        );
        let finam = snapshot(
            "ACC_FINAM_TEST",
            venue_instrument("IMOEXF", "IMOEXF@RTSX"),
            Decimal::ZERO,
            spec(BrokerKind::Finam, "IMOEXF@RTSX"),
            now - chrono::Duration::seconds(60),
        );

        let report =
            compare_broker_truth_for_instrument("alor", &alor, "finam", &finam, &target(), 30_000);

        assert!(!report.cutover_safe);
        assert!(report.issues.iter().any(|issue| {
            issue.kind == BrokerParityIssueKind::ReceivedTimestampSkewExceeded && issue.blocking
        }));
    }

    #[test]
    fn final_bar_parity_accepts_source_kind_as_diagnostic_only() {
        let alor = bar(MarketDataSourceKind::LiveStream, dec("2249"));
        let finam = bar(MarketDataSourceKind::HistoricalPoll, dec("2249"));

        let report = compare_final_bars_for_instrument("alor", &alor, "finam", &finam, &target());

        assert!(report.bars_synchronized);
        assert_eq!(report.blocking_issue_count, 0);
        assert_eq!(report.diagnostic_issue_count, 1);
        assert!(!report.live_order_authorized);
    }

    #[test]
    fn final_bar_parity_blocks_price_mismatch() {
        let alor = bar(MarketDataSourceKind::LiveStream, dec("2249"));
        let finam = bar(MarketDataSourceKind::HistoricalPoll, dec("2248.5"));

        let report = compare_final_bars_for_instrument("alor", &alor, "finam", &finam, &target());

        assert!(!report.bars_synchronized);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.kind == BrokerParityIssueKind::BarOhlcvMismatch));
    }
}
