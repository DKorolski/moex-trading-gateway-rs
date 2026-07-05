use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::event::Bar;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BarAggregationRejectReason {
    NonFinalSourceBar,
    SourceTimeframeZero,
    TargetTimeframeZero,
    SourceDoesNotDivideTarget,
    SourceTimeframeNotSmallerThanTarget,
    SourceBarDurationMismatch,
    BucketTimestampOutOfRange,
    InstrumentChangedWithinBucket,
    SourceKindChangedWithinBucket,
    NonContiguousSourceBar,
    NonMonotonicSourceBar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BarAggregationAction {
    Buffered {
        bucket_open_ts: DateTime<Utc>,
        buffered_count: usize,
    },
    Emitted {
        emitted: Bar,
    },
    Rejected {
        reason: BarAggregationRejectReason,
    },
    DroppedIncompleteBucket {
        bucket_open_ts: DateTime<Utc>,
        buffered_count: usize,
    },
}

#[derive(Debug, Clone)]
struct BarAggregationBucket {
    bucket_open_ts: DateTime<Utc>,
    target_timeframe_sec: u32,
    bars: Vec<Bar>,
}

impl BarAggregationBucket {
    fn push(&mut self, bar: Bar) {
        self.bars.push(bar);
    }

    fn expected_count(&self) -> usize {
        usize::try_from(self.target_timeframe_sec / self.bars[0].timeframe_sec)
            .expect("target/source ratio fits usize")
    }

    fn is_complete(&self) -> bool {
        !self.bars.is_empty() && self.bars.len() == self.expected_count()
    }

    fn can_accept(&self, bar: &Bar) -> Result<(), BarAggregationRejectReason> {
        let first = &self.bars[0];
        if first.instrument != bar.instrument {
            return Err(BarAggregationRejectReason::InstrumentChangedWithinBucket);
        }
        if first.source_kind != bar.source_kind {
            return Err(BarAggregationRejectReason::SourceKindChangedWithinBucket);
        }
        let Some(previous) = self.bars.last() else {
            return Ok(());
        };
        if bar.open_ts <= previous.open_ts {
            return Err(BarAggregationRejectReason::NonMonotonicSourceBar);
        }
        if bar.open_ts != previous.close_ts {
            return Err(BarAggregationRejectReason::NonContiguousSourceBar);
        }
        Ok(())
    }

    fn aggregate(&self) -> Bar {
        let first = self.bars.first().expect("complete bucket has first bar");
        let last = self.bars.last().expect("complete bucket has last bar");
        let mut high = first.high;
        let mut low = first.low;
        let mut volume = first.volume;
        for bar in self.bars.iter().skip(1) {
            if bar.high > high {
                high = bar.high;
            }
            if bar.low < low {
                low = bar.low;
            }
            volume += bar.volume;
        }

        Bar {
            instrument: first.instrument.clone(),
            source_kind: first.source_kind,
            timeframe_sec: self.target_timeframe_sec,
            open_ts: self.bucket_open_ts,
            close_ts: self.bucket_open_ts
                + chrono::Duration::seconds(i64::from(self.target_timeframe_sec)),
            open: first.open,
            high,
            low,
            close: last.close,
            volume,
            is_final: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CanonicalBarAggregator {
    target_timeframe_sec: u32,
    current: Option<BarAggregationBucket>,
}

impl CanonicalBarAggregator {
    pub fn new(target_timeframe_sec: u32) -> Self {
        Self {
            target_timeframe_sec,
            current: None,
        }
    }

    pub fn observe_final_source_bar(&mut self, bar: Bar) -> BarAggregationAction {
        match validate_source_bar(&bar, self.target_timeframe_sec) {
            Ok(()) => {}
            Err(reason) => return BarAggregationAction::Rejected { reason },
        }

        let bucket_open_ts = match bucket_open_ts(bar.open_ts, self.target_timeframe_sec) {
            Some(bucket_open_ts) => bucket_open_ts,
            None => {
                return BarAggregationAction::Rejected {
                    reason: BarAggregationRejectReason::BucketTimestampOutOfRange,
                };
            }
        };

        match self.current.take() {
            None => {
                let mut bucket = BarAggregationBucket {
                    bucket_open_ts,
                    target_timeframe_sec: self.target_timeframe_sec,
                    bars: Vec::new(),
                };
                bucket.push(bar);
                let buffered_count = bucket.bars.len();
                self.current = Some(bucket);
                BarAggregationAction::Buffered {
                    bucket_open_ts,
                    buffered_count,
                }
            }
            Some(mut bucket) if bucket.bucket_open_ts == bucket_open_ts => {
                if let Err(reason) = bucket.can_accept(&bar) {
                    self.current = None;
                    return BarAggregationAction::Rejected { reason };
                }
                bucket.push(bar);
                if bucket.is_complete() {
                    BarAggregationAction::Emitted {
                        emitted: bucket.aggregate(),
                    }
                } else {
                    let buffered_count = bucket.bars.len();
                    self.current = Some(bucket);
                    BarAggregationAction::Buffered {
                        bucket_open_ts,
                        buffered_count,
                    }
                }
            }
            Some(bucket) if bucket.bucket_open_ts < bucket_open_ts => {
                let dropped = BarAggregationAction::DroppedIncompleteBucket {
                    bucket_open_ts: bucket.bucket_open_ts,
                    buffered_count: bucket.bars.len(),
                };
                let mut next = BarAggregationBucket {
                    bucket_open_ts,
                    target_timeframe_sec: self.target_timeframe_sec,
                    bars: Vec::new(),
                };
                next.push(bar);
                self.current = Some(next);
                dropped
            }
            Some(_) => BarAggregationAction::Rejected {
                reason: BarAggregationRejectReason::NonMonotonicSourceBar,
            },
        }
    }
}

fn validate_source_bar(
    bar: &Bar,
    target_timeframe_sec: u32,
) -> Result<(), BarAggregationRejectReason> {
    if !bar.is_final {
        return Err(BarAggregationRejectReason::NonFinalSourceBar);
    }
    if bar.timeframe_sec == 0 {
        return Err(BarAggregationRejectReason::SourceTimeframeZero);
    }
    if target_timeframe_sec == 0 {
        return Err(BarAggregationRejectReason::TargetTimeframeZero);
    }
    if bar.timeframe_sec >= target_timeframe_sec {
        return Err(BarAggregationRejectReason::SourceTimeframeNotSmallerThanTarget);
    }
    if target_timeframe_sec % bar.timeframe_sec != 0 {
        return Err(BarAggregationRejectReason::SourceDoesNotDivideTarget);
    }
    if (bar.close_ts - bar.open_ts).num_seconds() != i64::from(bar.timeframe_sec) {
        return Err(BarAggregationRejectReason::SourceBarDurationMismatch);
    }
    Ok(())
}

fn bucket_open_ts(open_ts: DateTime<Utc>, target_timeframe_sec: u32) -> Option<DateTime<Utc>> {
    let timestamp = open_ts.timestamp();
    let target = i64::from(target_timeframe_sec);
    let bucket_timestamp = timestamp - timestamp.rem_euclid(target);
    Utc.timestamp_opt(bucket_timestamp, 0).single()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MarketDataSourceKind;
    use crate::instrument::{Exchange, InstrumentId, Market};
    use chrono::{Duration, TimeZone};
    use rust_decimal::Decimal;

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn m1_bar(minute: u32, open: i64, high: i64, low: i64, close: i64, volume: i64) -> Bar {
        let open_ts = Utc
            .with_ymd_and_hms(2026, 7, 6, 9, minute, 0)
            .single()
            .expect("valid timestamp");
        Bar {
            instrument: instrument(),
            source_kind: MarketDataSourceKind::LiveStream,
            timeframe_sec: 60,
            open_ts,
            close_ts: open_ts + Duration::seconds(60),
            open: Decimal::new(open, 0),
            high: Decimal::new(high, 0),
            low: Decimal::new(low, 0),
            close: Decimal::new(close, 0),
            volume: Decimal::new(volume, 0),
            is_final: true,
        }
    }

    #[test]
    fn canonical_bar_aggregator_emits_complete_m1_to_m10_bucket() {
        let mut aggregator = CanonicalBarAggregator::new(600);
        let mut last_action = None;
        for i in 0..10 {
            last_action = Some(aggregator.observe_final_source_bar(m1_bar(
                i,
                100 + i64::from(i),
                105 + i64::from(i),
                95 - i64::from(i),
                101 + i64::from(i),
                10 + i64::from(i),
            )));
        }

        let BarAggregationAction::Emitted { emitted } = last_action.expect("last action") else {
            panic!("expected emitted complete bucket");
        };
        assert_eq!(emitted.timeframe_sec, 600);
        assert_eq!(
            emitted.open_ts,
            Utc.with_ymd_and_hms(2026, 7, 6, 9, 0, 0).single().unwrap()
        );
        assert_eq!(
            emitted.close_ts,
            Utc.with_ymd_and_hms(2026, 7, 6, 9, 10, 0).single().unwrap()
        );
        assert_eq!(emitted.open, Decimal::new(100, 0));
        assert_eq!(emitted.high, Decimal::new(114, 0));
        assert_eq!(emitted.low, Decimal::new(86, 0));
        assert_eq!(emitted.close, Decimal::new(110, 0));
        assert_eq!(emitted.volume, Decimal::new(145, 0));
        assert!(emitted.is_final);
    }

    #[test]
    fn canonical_bar_aggregator_rejects_forming_source_bar() {
        let mut aggregator = CanonicalBarAggregator::new(600);
        let mut forming = m1_bar(0, 100, 101, 99, 100, 1);
        forming.is_final = false;

        assert_eq!(
            aggregator.observe_final_source_bar(forming),
            BarAggregationAction::Rejected {
                reason: BarAggregationRejectReason::NonFinalSourceBar
            }
        );
    }

    #[test]
    fn canonical_bar_aggregator_rejects_gap_inside_bucket() {
        let mut aggregator = CanonicalBarAggregator::new(600);
        assert!(matches!(
            aggregator.observe_final_source_bar(m1_bar(0, 100, 101, 99, 100, 1)),
            BarAggregationAction::Buffered { .. }
        ));

        assert_eq!(
            aggregator.observe_final_source_bar(m1_bar(2, 100, 101, 99, 100, 1)),
            BarAggregationAction::Rejected {
                reason: BarAggregationRejectReason::NonContiguousSourceBar
            }
        );
    }

    #[test]
    fn canonical_bar_aggregator_drops_incomplete_bucket_on_next_bucket() {
        let mut aggregator = CanonicalBarAggregator::new(600);
        assert!(matches!(
            aggregator.observe_final_source_bar(m1_bar(0, 100, 101, 99, 100, 1)),
            BarAggregationAction::Buffered { .. }
        ));

        assert_eq!(
            aggregator.observe_final_source_bar(m1_bar(10, 100, 101, 99, 100, 1)),
            BarAggregationAction::DroppedIncompleteBucket {
                bucket_open_ts: Utc.with_ymd_and_hms(2026, 7, 6, 9, 0, 0).single().unwrap(),
                buffered_count: 1,
            }
        );
    }

    #[test]
    fn canonical_bar_aggregator_rejects_non_divisible_timeframe() {
        let mut aggregator = CanonicalBarAggregator::new(700);

        assert_eq!(
            aggregator.observe_final_source_bar(m1_bar(0, 100, 101, 99, 100, 1)),
            BarAggregationAction::Rejected {
                reason: BarAggregationRejectReason::SourceDoesNotDivideTarget
            }
        );
    }
}
