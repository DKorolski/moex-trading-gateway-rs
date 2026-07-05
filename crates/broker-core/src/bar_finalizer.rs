use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::event::{Bar, MarketDataSourceKind};
use crate::instrument::InstrumentId;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClosedBarStreamKey {
    pub instrument: InstrumentId,
    pub timeframe_sec: u32,
    pub source_kind: MarketDataSourceKind,
}

impl ClosedBarStreamKey {
    pub fn from_bar(bar: &Bar) -> Self {
        Self {
            instrument: bar.instrument.clone(),
            timeframe_sec: bar.timeframe_sec,
            source_kind: bar.source_kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClosedBarFinalizerActionKind {
    BufferedForming,
    UpdatedForming,
    EmittedClosedFromNextBar,
    PassedThroughFinal,
    SuppressedDuplicateFinal,
    DroppedNonMonotonicForming,
    PassedThroughNonLiveSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClosedBarFinalizerAction {
    pub kind: ClosedBarFinalizerActionKind,
    pub emitted: Option<Bar>,
}

#[derive(Debug, Clone, Default)]
pub struct ClosedBarFinalizer {
    forming_by_stream: HashMap<ClosedBarStreamKey, Bar>,
    last_emitted_open_ts_by_stream: HashMap<ClosedBarStreamKey, chrono::DateTime<chrono::Utc>>,
}

impl ClosedBarFinalizer {
    pub fn observe_bar(&mut self, bar: Bar) -> ClosedBarFinalizerAction {
        if bar.source_kind != MarketDataSourceKind::LiveStream {
            return ClosedBarFinalizerAction {
                kind: ClosedBarFinalizerActionKind::PassedThroughNonLiveSource,
                emitted: Some(bar),
            };
        }

        let key = ClosedBarStreamKey::from_bar(&bar);
        if bar.is_final {
            if self
                .forming_by_stream
                .get(&key)
                .is_some_and(|forming| forming.open_ts <= bar.open_ts)
            {
                self.forming_by_stream.remove(&key);
            }
            return self.emit_once(key, bar, ClosedBarFinalizerActionKind::PassedThroughFinal);
        }

        match self.forming_by_stream.get(&key) {
            None => {
                self.forming_by_stream.insert(key, bar);
                ClosedBarFinalizerAction {
                    kind: ClosedBarFinalizerActionKind::BufferedForming,
                    emitted: None,
                }
            }
            Some(previous) if bar.open_ts == previous.open_ts => {
                self.forming_by_stream.insert(key, bar);
                ClosedBarFinalizerAction {
                    kind: ClosedBarFinalizerActionKind::UpdatedForming,
                    emitted: None,
                }
            }
            Some(previous) if bar.open_ts > previous.open_ts => {
                let mut closed = previous.clone();
                closed.is_final = true;
                self.forming_by_stream.insert(key.clone(), bar);
                self.emit_once(
                    key,
                    closed,
                    ClosedBarFinalizerActionKind::EmittedClosedFromNextBar,
                )
            }
            Some(_) => ClosedBarFinalizerAction {
                kind: ClosedBarFinalizerActionKind::DroppedNonMonotonicForming,
                emitted: None,
            },
        }
    }

    fn emit_once(
        &mut self,
        key: ClosedBarStreamKey,
        bar: Bar,
        kind: ClosedBarFinalizerActionKind,
    ) -> ClosedBarFinalizerAction {
        if self
            .last_emitted_open_ts_by_stream
            .get(&key)
            .is_some_and(|last_open_ts| bar.open_ts <= *last_open_ts)
        {
            return ClosedBarFinalizerAction {
                kind: ClosedBarFinalizerActionKind::SuppressedDuplicateFinal,
                emitted: None,
            };
        }

        self.last_emitted_open_ts_by_stream.insert(key, bar.open_ts);
        ClosedBarFinalizerAction {
            kind,
            emitted: Some(bar),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::{Exchange, Market};
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;

    fn bar(minute: u32, close: i64, is_final: bool) -> Bar {
        let open_ts = Utc.with_ymd_and_hms(2026, 7, 5, 15, minute, 0).unwrap();
        Bar {
            instrument: InstrumentId {
                symbol: "IMOEXF".to_string(),
                venue_symbol: Some("IMOEXF@RTSX".to_string()),
                exchange: Exchange::Moex,
                market: Market::Futures,
            },
            source_kind: MarketDataSourceKind::LiveStream,
            timeframe_sec: 60,
            open_ts,
            close_ts: open_ts + chrono::Duration::seconds(60),
            open: Decimal::new(close, 0),
            high: Decimal::new(close, 0),
            low: Decimal::new(close, 0),
            close: Decimal::new(close, 0),
            volume: Decimal::new(1, 0),
            is_final,
        }
    }

    #[test]
    fn closed_bar_finalizer_buffers_and_updates_current_forming_bar() {
        let mut finalizer = ClosedBarFinalizer::default();

        let first = finalizer.observe_bar(bar(55, 2253, false));
        assert_eq!(first.kind, ClosedBarFinalizerActionKind::BufferedForming);
        assert!(first.emitted.is_none());

        let update = finalizer.observe_bar(bar(55, 2252, false));
        assert_eq!(update.kind, ClosedBarFinalizerActionKind::UpdatedForming);
        assert!(update.emitted.is_none());
    }

    #[test]
    fn closed_bar_finalizer_emits_previous_forming_when_next_bar_arrives() {
        let mut finalizer = ClosedBarFinalizer::default();

        finalizer.observe_bar(bar(55, 2252, false));
        let action = finalizer.observe_bar(bar(56, 2253, false));

        assert_eq!(
            action.kind,
            ClosedBarFinalizerActionKind::EmittedClosedFromNextBar
        );
        let emitted = action.emitted.expect("previous bar emitted");
        assert!(emitted.is_final);
        assert_eq!(
            emitted.open_ts,
            Utc.with_ymd_and_hms(2026, 7, 5, 15, 55, 0).unwrap()
        );
        assert_eq!(emitted.close, Decimal::new(2252, 0));
    }

    #[test]
    fn closed_bar_finalizer_passes_direct_final_once_and_suppresses_duplicates() {
        let mut finalizer = ClosedBarFinalizer::default();

        let final_bar = finalizer.observe_bar(bar(55, 2252, true));
        assert_eq!(
            final_bar.kind,
            ClosedBarFinalizerActionKind::PassedThroughFinal
        );
        assert!(final_bar.emitted.expect("final emitted").is_final);

        let duplicate = finalizer.observe_bar(bar(55, 2252, true));
        assert_eq!(
            duplicate.kind,
            ClosedBarFinalizerActionKind::SuppressedDuplicateFinal
        );
        assert!(duplicate.emitted.is_none());
    }

    #[test]
    fn closed_bar_finalizer_keeps_current_forming_after_late_duplicate_final() {
        let mut finalizer = ClosedBarFinalizer::default();

        finalizer.observe_bar(bar(55, 2252, false));
        let inferred = finalizer.observe_bar(bar(56, 2253, false));
        assert_eq!(
            inferred.kind,
            ClosedBarFinalizerActionKind::EmittedClosedFromNextBar
        );

        let late_duplicate_final = finalizer.observe_bar(bar(55, 2252, true));
        assert_eq!(
            late_duplicate_final.kind,
            ClosedBarFinalizerActionKind::SuppressedDuplicateFinal
        );
        assert!(late_duplicate_final.emitted.is_none());

        let next = finalizer.observe_bar(bar(57, 2254, false));
        assert_eq!(
            next.kind,
            ClosedBarFinalizerActionKind::EmittedClosedFromNextBar
        );
        let emitted = next.emitted.expect("current forming still buffered");
        assert_eq!(
            emitted.open_ts,
            Utc.with_ymd_and_hms(2026, 7, 5, 15, 56, 0).unwrap()
        );
    }

    #[test]
    fn closed_bar_finalizer_drops_non_monotonic_forming_updates() {
        let mut finalizer = ClosedBarFinalizer::default();

        finalizer.observe_bar(bar(56, 2253, false));
        let older = finalizer.observe_bar(bar(55, 2252, false));

        assert_eq!(
            older.kind,
            ClosedBarFinalizerActionKind::DroppedNonMonotonicForming
        );
        assert!(older.emitted.is_none());
    }

    #[test]
    fn closed_bar_finalizer_passes_non_live_sources_without_buffering() {
        let mut finalizer = ClosedBarFinalizer::default();
        let mut historical = bar(55, 2252, true);
        historical.source_kind = MarketDataSourceKind::HistoricalPoll;

        let action = finalizer.observe_bar(historical);

        assert_eq!(
            action.kind,
            ClosedBarFinalizerActionKind::PassedThroughNonLiveSource
        );
        assert!(action.emitted.expect("historical emitted").is_final);
    }
}
