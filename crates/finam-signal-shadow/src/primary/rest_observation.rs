//! Bounded, data-only comparisons. No canonical capability or HOLD mutation.
use broker_core::event::Bar;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) enum ComparisonKind {
    Match,
    VolumeRevision,
    PriceDifference,
    InventoryDifference,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Assessment {
    pub label_ms: i64,
    pub known_basis: &'static str,
    pub kind: ComparisonKind,
    pub expected_m1: Vec<Bar>,
    pub observed_m1: Vec<Bar>,
    pub missing_known_m1_ms: Vec<i64>,
    pub additional_rest_m1_ms: Vec<i64>,
    pub price_changed_m1_ms: Vec<i64>,
    pub volume_changed_m1_ms: Vec<i64>,
    pub model_input_rewritten: bool,
    pub model_impact: &'static str,
}

pub(crate) fn assess(
    label_ms: i64,
    basis: &'static str,
    expected: Vec<Bar>,
    actual: Vec<Bar>,
) -> Assessment {
    let mut missing = vec![];
    let mut extra = vec![];
    let mut prices = vec![];
    let mut volumes = vec![];
    for old in &expected {
        match actual.iter().find(|new| new.open_ts == old.open_ts) {
            None => missing.push(old.open_ts.timestamp_millis()),
            Some(new) => {
                if !super::revisions::same_prices(old, new) {
                    prices.push(old.open_ts.timestamp_millis());
                }
                if old.volume != new.volume {
                    volumes.push(old.open_ts.timestamp_millis());
                }
            }
        }
    }
    for new in &actual {
        if !expected.iter().any(|old| old.open_ts == new.open_ts) {
            extra.push(new.open_ts.timestamp_millis());
        }
    }
    let kind = if !missing.is_empty() || !extra.is_empty() {
        ComparisonKind::InventoryDifference
    } else if !prices.is_empty() {
        ComparisonKind::PriceDifference
    } else if !volumes.is_empty() {
        ComparisonKind::VolumeRevision
    } else {
        ComparisonKind::Match
    };
    Assessment {
        label_ms,
        known_basis: basis,
        kind,
        expected_m1: expected,
        observed_m1: actual,
        missing_known_m1_ms: missing,
        additional_rest_m1_ms: extra,
        price_changed_m1_ms: prices,
        volume_changed_m1_ms: volumes,
        model_input_rewritten: false,
        model_impact: "NOT_EVALUATED_REQUIRES_OFFLINE_REPLAY",
    }
}
