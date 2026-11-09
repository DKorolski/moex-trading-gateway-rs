//! Source-neutral Decimal math only: neither finality nor publication authority.
//! REST and WS admission must supply their own checked source receipts.
use crate::{event::Bar, InstrumentId, MarketDataSourceKind};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregationError {
    Range,
    Empty,
    Bar,
}

#[derive(Debug)]
pub struct M10Values {
    pub provisional_bar: Bar,
    pub actual_m1: Vec<Bar>,
    pub minute_bitmap: u16,
}

/// Exactly [open,open+600s). Sparse boundaries do not shift. Input must be
/// ordered and unique; a duplicate is not extra traded volume.
pub fn aggregate_m1_window(
    instrument: &InstrumentId,
    open: DateTime<Utc>,
    rows: &[Bar],
) -> Result<M10Values, AggregationError> {
    let close = open
        .checked_add_signed(Duration::minutes(10))
        .ok_or(AggregationError::Range)?;
    if open.timestamp() < 0 || open.timestamp() % 600 != 0 || open.timestamp_subsec_nanos() != 0 {
        return Err(AggregationError::Range);
    }
    if rows.is_empty() {
        return Err(AggregationError::Empty);
    }
    if rows.len() > 10 {
        return Err(AggregationError::Range);
    }
    let mut prior = None;
    for row in rows {
        if &row.instrument != instrument
            || row.timeframe_sec != 60
            || row.open_ts < open
            || row.open_ts >= close
            || row.open_ts.timestamp() % 60 != 0
            || row.open_ts.timestamp_subsec_nanos() != 0
            || row.open_ts.checked_add_signed(Duration::minutes(1)) != Some(row.close_ts)
            || prior.is_some_and(|t| row.open_ts <= t)
            || row.low <= Decimal::ZERO
            || row.low > row.high
            || row.low > row.open.min(row.close)
            || row.high < row.open.max(row.close)
            || row.volume < Decimal::ZERO
        {
            return Err(AggregationError::Bar);
        }
        prior = Some(row.open_ts);
    }
    let mut bar = rows[0].clone();
    bar.open_ts = open;
    bar.close_ts = close;
    bar.timeframe_sec = 600;
    bar.source_kind = MarketDataSourceKind::Unknown;
    bar.is_final = false;
    bar.close = rows.last().ok_or(AggregationError::Empty)?.close;
    bar.volume = Decimal::ZERO;
    let mut minute_bitmap = 0;
    for row in rows {
        bar.high = bar.high.max(row.high);
        bar.low = bar.low.min(row.low);
        bar.volume = bar
            .volume
            .checked_add(row.volume)
            .ok_or(AggregationError::Bar)?;
        minute_bitmap |= 1 << ((row.open_ts - open).num_seconds() / 60);
    }
    Ok(M10Values {
        provisional_bar: bar,
        actual_m1: rows.to_vec(),
        minute_bitmap,
    })
}
