//! Single-threaded observed composition. Never promotes historical data to LIVE.
use crate::{
    feedback::Quote,
    input::ClosedInputAdapter,
    model::ShadowModel,
    observer::{Decision, Observer, Step},
    segment::{ObservationMode, SeededSegment},
    timing::CallbackFence,
    transport::StopHandle,
    ValidatedConfig,
};
use broker_core::HybridRuntimeBarOrigin;
use broker_finam::sparse_m10::AdmittedClosedM1Snapshot;
use chrono::{DateTime, Utc};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum SessionError {
    Closed,
    Input,
    Timing,
    Model,
    Evidence,
    Incomplete,
}

pub struct ObservedSession {
    config: ValidatedConfig,
    model: ShadowModel,
    adapter: ClosedInputAdapter,
    fence: CallbackFence,
    observer: Observer,
    stop: StopHandle,
    next_label_ms: i64,
    closed: bool,
    last_quote_ms: Option<i64>,
    prefix_quote_floor_ms: Option<i64>,
}

impl ObservedSession {
    /// Consume only a fully source-admitted NEW_SEGMENT reconstruction. Replay
    /// evidence stays BACKFILL; only subsequent measured callbacks are LIVE.
    /// Pending reconstruction is retained in evidence but cannot be continued
    /// without a separate proof of quote continuity across the downtime gap.
    pub fn resume(
        mut prefix: crate::prefix::ReconstructedPrefix,
        stop: StopHandle,
        approval: &crate::runner::PrefixRunPlan,
    ) -> Result<Self, SessionError> {
        if approval.validate(&prefix).is_err() {
            stop.stop();
            let _ = prefix
                .observer
                .finish("INCOMPLETE_prefix_coverage_unproven");
            return Err(SessionError::Input);
        }
        let next = prefix.next_label_utc_ms();
        let frontier = prefix.frontier_utc_ms();
        let now = Utc::now().timestamp_millis();
        if stop.is_stopped()
            || prefix.model.pending().is_some()
            || now < frontier
            || now >= next.checked_add(600_000).ok_or(SessionError::Input)?
            || now >= prefix.config.config().run_deadline_utc_ms
        {
            stop.stop();
            let _ = prefix.observer.finish("INCOMPLETE_prefix_handoff");
            return Err(SessionError::Incomplete);
        }
        let binding = prefix.binding().clone();
        let ready = (|| {
            let fence = CallbackFence::new(&prefix.config, stop.clone())
                .map_err(|_| SessionError::Timing)?;
            prefix
                .observer
                .record_run_plan(approval)
                .map_err(|_| SessionError::Evidence)?;
            prefix
                .observer
                .record_transport(&serde_json::json!({
                "event":"prefix_handoff", "mode":ObservationMode::BackfillReplay,
                "prefix_binding":binding,
                "frontier_utc_ms":frontier,"next_label_utc_ms":next,
                "position_lots":prefix.model.position_lots(),"state":prefix.model.observation()
                }))
                .map_err(|_| SessionError::Evidence)?;
            let after = Utc::now().timestamp_millis();
            if after < now
                || after >= next + 600_000
                || stop.is_stopped()
                || after >= prefix.config.config().run_deadline_utc_ms
            {
                return Err(SessionError::Incomplete);
            }
            Ok(fence)
        })();
        let fence = match ready {
            Ok(fence) => fence,
            Err(error) => {
                stop.stop();
                let _ = prefix.observer.finish("INCOMPLETE_prefix_handoff");
                return Err(error);
            }
        };
        prefix.model.select_observed();
        Ok(Self {
            config: prefix.config,
            model: prefix.model,
            adapter: prefix.adapter,
            observer: prefix.observer,
            fence,
            stop,
            next_label_ms: next,
            closed: false,
            // Old prefix quotes must not affect a new live intent. The socket
            // still owns within-live dedup; the cross-prefix boundary is strict.
            last_quote_ms: None,
            prefix_quote_floor_ms: Some(frontier),
        })
    }

    pub fn record_transport(&mut self, event: &impl serde::Serialize) -> Result<(), SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        if self.observer.record_transport(event).is_err() {
            return self.fail(SessionError::Evidence);
        }
        Ok(())
    }
    pub fn record_run_plan(&mut self, plan: &impl serde::Serialize) -> Result<(), SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        if self.observer.record_run_plan(plan).is_err() {
            return self.fail(SessionError::Evidence);
        }
        Ok(())
    }
    pub fn create(
        segment: SeededSegment,
        output: &Path,
        stop: StopHandle,
    ) -> Result<Self, SessionError> {
        let header = segment.seed_observation();
        let (config, mut model) = segment.into_replay_parts();
        let now = Utc::now().timestamp_millis();
        // This source entry is a fresh session-boundary start. Mid-session start
        // is refused rather than silently discarding the missing prefix.
        let start = config.config().observation_start_utc_ms;
        if stop.is_stopped() || now >= start.checked_add(600_000).ok_or(SessionError::Input)? {
            return Err(SessionError::Incomplete);
        }
        let fence = CallbackFence::new(&config, stop.clone()).map_err(|_| SessionError::Timing)?;
        let instrument = broker_core::InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: broker_core::Exchange::Moex,
            market: broker_core::Market::Futures,
        };
        let adapter = ClosedInputAdapter::new(instrument, config.config().max_cache_bars as usize)
            .map_err(|_| SessionError::Input)?;
        model.select_observed();
        let observer =
            Observer::create(output, header, config.config().max_output_bytes, 1_048_576)
                .map_err(|_| SessionError::Evidence)?;
        Ok(Self {
            config,
            model,
            adapter,
            fence,
            observer,
            stop,
            next_label_ms: start,
            closed: false,
            last_quote_ms: None,
            prefix_quote_floor_ms: None,
        })
    }

    pub fn snapshot(
        &mut self,
        snapshot: &AdmittedClosedM1Snapshot,
        required: &[DateTime<Utc>],
        received_ms: i64,
    ) -> Result<(), SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        let result = self.snapshot_inner(snapshot, required, received_ms);
        if let Err(error) = result {
            return self.fail(error);
        }
        Ok(())
    }

    fn snapshot_inner(
        &mut self,
        snapshot: &AdmittedClosedM1Snapshot,
        required: &[DateTime<Utc>],
        received_ms: i64,
    ) -> Result<(), SessionError> {
        self.check()?;
        // Receive frontier is taken from the admitted REST evidence, never WS
        // heartbeat or a caller's nominal candle time.
        let actual_received = snapshot
            .evidence()
            .parts
            .iter()
            .map(|p| p.received_at.timestamp_millis())
            .max()
            .ok_or(SessionError::Input)?;
        if received_ms != actual_received {
            return Err(SessionError::Input);
        }
        let inputs = self
            .adapter
            .prepare(snapshot, required)
            .map_err(|_| SessionError::Input)?;
        if !inputs.is_empty() {
            self.observer
                .record_source(snapshot)
                .map_err(|_| SessionError::Evidence)?;
        }
        for input in inputs {
            let mut bar = input.bar;
            let label = bar
                .close_time_utc
                .checked_mul(1000)
                .ok_or(SessionError::Input)?;
            if label != self.next_label_ms {
                return Err(SessionError::Incomplete);
            }
            let ready = Utc::now().timestamp_millis();
            let close = label.checked_add(600_000).ok_or(SessionError::Input)?;
            let encoded = serde_json::to_vec(&bar).map_err(|_| SessionError::Input)?;
            if encoded.len() as u64 > self.config.config().max_input_bytes {
                return Err(SessionError::Input);
            }
            let before = self.model.observation();
            bar.origin = HybridRuntimeBarOrigin::Live;
            let (request, timing) = self
                .model
                .on_bar_measured(bar.clone(), received_ms, ready, &mut self.fence)
                .map_err(|_| SessionError::Model)?;
            let after = self.model.observation();
            let decision = Decision::from_model_observation(
                request.as_ref().map(|r| r.action),
                request.as_ref().map_or(0, |r| r.quantity_lots),
                &after,
            )
            .map_err(|_| SessionError::Model)?;
            self.observer
                .record_step(&Step {
                    mode: ObservationMode::LiveObserved,
                    instrument: bar.instrument.symbol.clone(),
                    model_label_utc_ms: label,
                    nominal_close_utc_ms: close,
                    provider_at_ms: None,
                    received_at_ms: timing.received_at_ms,
                    canonical_ready_at_ms: timing.canonical_ready_at_ms,
                    model_start_at_ms: timing.model_start_at_ms,
                    result_at_ms: timing.result_at_ms,
                    canonical_input_sha256: crate::input::canonical_digest(&bar)
                        .map_err(|_| SessionError::Input)?,
                    raw_provenance_sha256: input.snapshot_sha256,
                    before,
                    after,
                    decision,
                })
                .map_err(|_| SessionError::Evidence)?;
            let published_ms = Utc::now().timestamp_millis();
            self.observer
                .record_transport(&serde_json::json!({
                    "event": "model_step_durable_publication",
                    "model_label_utc_ms": label,
                    "published_at_ms": published_ms,
                }))
                .map_err(|_| SessionError::Evidence)?;
            self.fence.check_now().map_err(|_| SessionError::Timing)?;
            let latency = published_ms
                .checked_sub(close)
                .and_then(|v| u64::try_from(v).ok())
                .and_then(|v| v.checked_add(self.config.config().clock_uncertainty_ms));
            if latency.map_or(true, |v| v > self.config.config().lmax_ms) {
                return Err(SessionError::Timing);
            }
            self.next_label_ms = next_label(label).ok_or(SessionError::Input)?;
        }
        Ok(())
    }

    /// Persist raw source before feedback, including empty deduplicated frames.
    /// This does not assert complete coverage across process downtime.
    pub fn quote_frame(&mut self, frame: crate::ws::QuoteFrame) -> Result<(), SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        if let Err(error) = self.check() {
            return self.fail(error);
        }
        if frame.raw_frame.len() > 256 * 1024
            || frame.received_at_ms < 0
            || frame.received_at_ms > Utc::now().timestamp_millis()
            || frame
                .quotes
                .iter()
                .any(|q| q.received_at_ms != frame.received_at_ms)
        {
            return self.fail(SessionError::Input);
        }
        if self
            .observer
            .record_transport(&serde_json::json!({
                "event":"quote_source", "mode":ObservationMode::LiveObserved,
                "raw_frame":frame.raw_frame,
                "raw_frame_sha256":crate::seed::digest(frame.raw_frame.as_bytes()),
                "received_at_ms":frame.received_at_ms,
            }))
            .is_err()
        {
            return self.fail(SessionError::Evidence);
        }
        for quote in frame.quotes {
            self.quote(quote)?;
        }
        Ok(())
    }

    pub fn quote(&mut self, quote: Quote) -> Result<(), SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        if let Err(e) = self.check() {
            return self.fail(e);
        }
        if self.fence.check_now().is_err() {
            return self.fail(SessionError::Timing);
        }
        let now = Utc::now().timestamp_millis();
        if quote.event_at_ms > quote.received_at_ms
            || quote.received_at_ms > now
            || self
                .last_quote_ms
                .is_some_and(|last| quote.event_at_ms < last)
            || self
                .prefix_quote_floor_ms
                .is_some_and(|floor| quote.event_at_ms <= floor)
        {
            return self.fail(SessionError::Input);
        }
        self.last_quote_ms = Some(quote.event_at_ms);
        if self.model.pending().is_none() {
            return Ok(());
        }
        let result = self.model.on_quote(quote, now);
        if self.fence.check_now().is_err() {
            return self.fail(SessionError::Timing);
        }
        match result {
            Ok(Some(fill)) => {
                if self
                    .observer
                    .record_feedback(ObservationMode::LiveObserved, &fill)
                    .is_err()
                {
                    return self.fail(SessionError::Evidence);
                }
            }
            Ok(None) => {}
            Err(_) => return self.fail(SessionError::Model),
        }
        Ok(())
    }

    fn check(&self) -> Result<(), SessionError> {
        if self.stop.is_stopped()
            || Utc::now().timestamp_millis() >= self.config.config().run_deadline_utc_ms
        {
            return Err(SessionError::Closed);
        }
        Ok(())
    }

    pub fn finish(&mut self, reason: &str) -> Result<(), SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        self.closed = true;
        self.stop.stop();
        let pending = self.model.pending().is_some();
        let nonflat = self.model.position_lots() != 0;
        let missing = self
            .next_label_ms
            .checked_add(600_000)
            .map_or(true, |close| {
                close
                    <= Utc::now()
                        .timestamp_millis()
                        .min(self.config.config().run_deadline_utc_ms)
            });
        let result = if pending {
            "INCOMPLETE_virtual_pending"
        } else if nonflat {
            "INCOMPLETE_virtual_nonflat"
        } else if missing {
            "INCOMPLETE_required_bar_missing"
        } else {
            reason
        };
        self.observer
            .finish(result)
            .map_err(|_| SessionError::Evidence)?;
        if pending || nonflat || missing {
            return Err(SessionError::Incomplete);
        }
        Ok(())
    }

    fn fail<T>(&mut self, error: SessionError) -> Result<T, SessionError> {
        self.closed = true;
        self.stop.stop();
        let _ = self.observer.finish("INCOMPLETE_observed_path_failure");
        Err(error)
    }
}

impl Drop for ObservedSession {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.finish("INCOMPLETE_owner_dropped");
        }
        self.stop.stop();
    }
}

/// Fixed baseline07 nominal schedule, not evidence of exchange availability.
/// A missing scheduled bucket remains INCOMPLETE, never fabricated or skipped.
fn next_label(label_ms: i64) -> Option<i64> {
    use chrono::{Datelike, Timelike};
    let local = DateTime::from_timestamp_millis(label_ms)? + chrono::Duration::hours(3);
    if local.hour() == 23 && local.minute() == 40 {
        let mut date = local.date_naive().succ_opt()?;
        while date.weekday().number_from_monday() > 5 {
            date = date.succ_opt()?;
        }
        Some((date.and_hms_opt(7, 0, 0)?.and_utc() - chrono::Duration::hours(3)).timestamp_millis())
    } else {
        label_ms.checked_add(600_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nominal_eod_advances_to_next_start_without_inventing_night_bars() {
        let label = DateTime::parse_from_rfc3339("2026-10-23T23:40:00+03:00")
            .unwrap()
            .timestamp_millis();
        let next = DateTime::parse_from_rfc3339("2026-10-26T07:00:00+03:00")
            .unwrap()
            .timestamp_millis();
        assert_eq!(next_label(label), Some(next));
        assert_eq!(next_label(label - 600_000), Some(label));
    }

    // Inject the composition directly to test a recently closed real-clock
    // bucket at any time of day. This intentionally does NOT bypass production
    // create() or claim warmup/readiness acceptance: the fixture model is cold.
    fn composition_fixture() -> (
        ObservedSession,
        AdmittedClosedM1Snapshot,
        std::path::PathBuf,
    ) {
        use crate::{seed::digest, segment::SeedObservation};
        use broker_core::*;
        use broker_finam::sparse_m10::*;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let now = Utc::now();
        let close_ms = now.timestamp_millis().div_euclid(600_000) * 600_000;
        let start = DateTime::from_timestamp_millis(close_ms - 600_000).unwrap();
        let end = DateTime::from_timestamp_millis(close_ms).unwrap();
        let instrument = InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        };
        let model = ShadowModel::baseline07(
            HybridRuntimeStrategyContext {
                strategy_id: "composition-test".into(),
                request_namespace_account: BrokerAccountId("shadow-test".into()),
                instrument: instrument.clone(),
                tick_size: 0.5,
                trade_mode: HybridRuntimeTradeMode::Paper,
                paper_execution_mode: HybridRuntimePaperExecutionMode::LiveOnly,
                allow_live_orders: false,
                gateway_phase: HybridRuntimeGatewayPhase::LiveReady,
                position_qty: Some(0.0),
                event_ts_utc: now.timestamp(),
                strategy_now_ts_utc: now.timestamp(),
                last_bar_ts_utc: None,
            },
            1000,
        )
        .unwrap();
        let config = ValidatedConfig::parse(&serde_json::to_vec(&serde_json::json!({
            "schema_version":1, "reference":"alor_freeze_2026_09_27", "restart_mode":"new_segment",
            "model_profile":"imoexf-baseline07-bo-only-no-riskgate",
            "model_sha256":model.model_config_fingerprint(), "reference_sha256":"a".repeat(64),
            "feedback_policy_sha256":crate::feedback::policy_fingerprint(),
            "instrument_mapping_sha256":"b".repeat(64), "calendar_sha256":"c".repeat(64), "seed_sha256":"d".repeat(64),
            "instrument":"IMOEXF@RTSX", "run_id":"composition-test", "segment_id":"s1", "previous_segment_id":null,
            "warmup_session_dates":["2026-09-22","2026-09-23","2026-09-24","2026-09-25"],
            "warmup_range_start_utc_ms":1, "warmup_range_end_utc_ms":2,
            "observation_start_utc_ms":start.timestamp_millis(), "run_deadline_utc_ms":now.timestamp_millis()+60_000,
            "lmax_ms":1_200_000, "clock_uncertainty_ms":1000, "max_quote_age_ms":1000,
            "ws_silence_budget_ms":1000,"reconnect_budget_ms":1000,"stop_timeout_ms":1000,
            "max_input_bytes":100_000,"max_cache_bars":10,"max_output_bytes":100_000
        })).unwrap()).unwrap();
        let c = config.config();
        let header = SeedObservation {
            mode: ObservationMode::Warmup,
            run_id: c.run_id.clone(),
            segment_id: c.segment_id.clone(),
            previous_segment_id: None,
            config_sha256: config.fingerprint().into(),
            semantic_config_sha256: config.semantic_fingerprint().into(),
            seed_sha256: c.seed_sha256.clone(),
            model_sha256: c.model_sha256.clone(),
            reference_sha256: c.reference_sha256.clone(),
            feedback_policy_sha256: c.feedback_policy_sha256.clone(),
            last_history_label_utc: 0,
            observation_start_utc_ms: c.observation_start_utc_ms,
            virtual_position_lots: 0,
            live_observation_count: 0,
            state: model.observation(),
        };
        let path = std::env::temp_dir().join(format!(
            "shadow-session-{}-{}.jsonl",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let observer = Observer::create(&path, header, 100_000, 20_000).unwrap();
        let (stop, _) = crate::transport::ReadOnlyCollector::new(
            now + chrono::Duration::seconds(60),
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        let fence = CallbackFence::new(&config, stop.clone()).unwrap();
        let session = ObservedSession {
            config,
            model,
            adapter: ClosedInputAdapter::new(instrument, 10).unwrap(),
            fence,
            observer,
            stop,
            next_label_ms: start.timestamp_millis(),
            closed: false,
            last_quote_ms: None,
            prefix_quote_floor_ms: None,
        };
        let raw = serde_json::json!({"symbol":"IMOEXF@RTSX", "bars":[{
            "timestamp":start.to_rfc3339(), "open":{"value":"2200"}, "high":{"value":"2200"},
            "low":{"value":"2200"}, "close":{"value":"2200"}, "volume":{"value":"1"}
        }]})
        .to_string();
        let snapshot = AdmittedClosedM1Snapshot::admit(
            ClosedM1SnapshotEvidenceV1 {
                policy: OBSERVED_M1_POLICY_V1.into(),
                start,
                end,
                parts: vec![ClosedM1RestPartV1 {
                    method: "GET".into(),
                    endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
                    symbol: "IMOEXF@RTSX".into(),
                    timeframe: "TIME_FRAME_M1".into(),
                    start,
                    end,
                    requested_at: now,
                    received_at: now,
                    status: 200,
                    transport_complete: true,
                    declared_body_bytes: Some(raw.len() as u64),
                    response_sha256: digest(raw.as_bytes()),
                    raw_body: raw,
                }],
            },
            &ClosedM1RequestPlan::new(start, end, vec![end]).unwrap(),
            now,
        )
        .unwrap();
        (session, snapshot, path)
    }

    #[test]
    fn normalized_submillisecond_quotes_survive_flat_and_pending_composition() {
        use crate::quote::QuoteInput;
        use broker_core::*;
        for pending in [false, true] {
            let (mut session, _, path) = composition_fixture();
            if pending {
                let mut csv = csv::Reader::from_reader(
                    include_bytes!(
                        "../../../fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv"
                    )
                    .as_slice(),
                );
                for row in csv.records() {
                    let row = row.unwrap();
                    let ts = DateTime::parse_from_str(&row[1], "%Y-%m-%d %H:%M:%S%:z")
                        .unwrap()
                        .timestamp();
                    let number = |i| row.get(i).unwrap().parse::<f64>().unwrap();
                    let bar = HybridRuntimeBarEvent {
                        instrument: InstrumentId {
                            symbol: "IMOEXF".into(),
                            venue_symbol: Some("IMOEXF@RTSX".into()),
                            exchange: Exchange::Moex,
                            market: Market::Futures,
                        },
                        close_time_utc: ts,
                        open: number(3),
                        high: number(4),
                        low: number(5),
                        close: number(6),
                        volume: number(7),
                        origin: HybridRuntimeBarOrigin::Live,
                        is_final: true,
                        timeframe_sec: 600,
                    };
                    if session
                        .model
                        .on_bar(bar, (ts + 600) * 1000)
                        .unwrap()
                        .is_some()
                    {
                        break;
                    }
                }
                assert!(session.model.pending().is_some());
            }
            let mut normalizer = QuoteInput::new(1000, 4096).unwrap();
            let mut send_pair = |session: &mut ObservedSession, ms: i64| {
                let received = DateTime::from_timestamp_millis(ms + 1).unwrap();
                let rows: Vec<_> = [100_000, 200_000].into_iter().map(|nanos| {
                    let timestamp = DateTime::from_timestamp_millis(ms).unwrap()
                        + chrono::Duration::nanoseconds(nanos);
                    serde_json::json!({"symbol":"IMOEXF@RTSX","timestamp":timestamp.to_rfc3339(),"last":{"value":"2205"}})
                }).collect();
                let raw = serde_json::json!({"type":"DATA","subscriptionType":"QUOTES","payload":{"quote":rows}}).to_string();
                let quotes = normalizer.receive(&raw, received).unwrap();
                assert_eq!(quotes.len(), 2);
                assert!(quotes.iter().all(|q| q.event_at_ms == ms));
                assert_ne!(quotes[0].source_id, quotes[1].source_id);
                assert!(normalizer.receive(&raw, received).unwrap().is_empty());
                session
                    .quote_frame(crate::ws::QuoteFrame {
                        raw_frame: raw,
                        received_at_ms: received.timestamp_millis(),
                        quotes,
                    })
                    .unwrap();
            };
            if pending {
                let decision = session.model.pending().unwrap().decision_at_ms;
                send_pair(&mut session, decision);
                assert!(session.model.pending().is_some());
                assert_eq!(session.model.position_lots(), 0);
            }
            send_pair(&mut session, Utc::now().timestamp_millis() - 10);
            assert!(!session.closed);
            assert!(session.model.pending().is_none());
            assert_eq!(session.model.position_lots().abs(), i64::from(pending));
            let rows = std::fs::read_to_string(&path).unwrap();
            let fills = rows
                .lines()
                .filter(|line| {
                    serde_json::from_str::<serde_json::Value>(line).unwrap()["kind"]
                        == "hypothetical_feedback"
                })
                .count();
            assert_eq!(fills, usize::from(pending));
            drop(session);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn strict_prefix_floor_is_not_the_live_millisecond_frontier() {
        for offset in [-1, 0, 1] {
            let (mut session, _, path) = composition_fixture();
            let floor = Utc::now().timestamp_millis() - 10;
            session.prefix_quote_floor_ms = Some(floor);
            let quote = Quote {
                instrument: "IMOEXF".into(),
                source_id: "fixture".into(),
                price_ticks: 4400,
                event_at_ms: floor + offset,
                received_at_ms: floor + 2,
            };
            let result = session.quote(quote.clone());
            if offset <= 0 {
                assert_eq!(result, Err(SessionError::Input));
                assert!(session.closed);
            } else {
                result.unwrap();
                // Production normalizer filters duplicates; the session floor
                // must not become strict after admitting a live millisecond.
                session
                    .quote(Quote {
                        source_id: "distinct-admitted".into(),
                        ..quote
                    })
                    .unwrap();
                assert_eq!(session.prefix_quote_floor_ms, Some(floor));
            }
            drop(session);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn raw_quote_frames_are_durable_even_when_dedup_is_empty_and_failure_is_terminal() {
        use crate::ws::QuoteFrame;
        let (mut session, _, path) = composition_fixture();
        let received = Utc::now().timestamp_millis();
        let raw =
            " {\"type\":\"DATA\",\"subscriptionType\":\"QUOTES\",\"payload\":{\"quote\":[]}}\n";
        // The protocol owns normalization; this is its duplicate-only result.
        session
            .quote_frame(QuoteFrame {
                raw_frame: raw.into(),
                received_at_ms: received,
                quotes: vec![],
            })
            .unwrap();
        let rows: Vec<serde_json::Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let event = &rows.last().unwrap()["event"];
        assert_eq!(event["event"], "quote_source");
        assert_eq!(event["raw_frame"], raw);
        assert_eq!(
            event["raw_frame_sha256"],
            crate::seed::digest(raw.as_bytes())
        );
        assert_eq!(event["received_at_ms"], received);
        // A frame whose source row cannot fit must never enter feedback.
        let before = session.model.observation();
        assert_eq!(
            session.quote_frame(QuoteFrame {
                raw_frame: "x".repeat(21_000),
                received_at_ms: received,
                quotes: vec![Quote {
                    instrument: "IMOEXF".into(),
                    source_id: "test".into(),
                    price_ticks: 4400,
                    event_at_ms: received,
                    received_at_ms: received
                }]
            }),
            Err(SessionError::Evidence)
        );
        assert_eq!(session.model.observation(), before);
        assert!(session.closed && session.stop.is_stopped());
        drop(session);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn admitted_snapshot_runs_composition_once_and_retains_source() {
        let (mut session, snapshot, path) = composition_fixture();
        let required = [snapshot.evidence().start];
        let received = snapshot.evidence().parts[0].received_at.timestamp_millis();
        session.snapshot(&snapshot, &required, received).unwrap();
        let after = session.model.observation();
        let next = session.next_label_ms;
        let output = std::fs::read(&path).unwrap();
        session.snapshot(&snapshot, &required, received).unwrap();
        assert_eq!(session.model.observation(), after);
        assert_eq!(session.next_label_ms, next);
        assert_eq!(std::fs::read(&path).unwrap(), output);
        let rows: Vec<serde_json::Value> = std::str::from_utf8(&output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows.iter().filter(|r| r["kind"] == "model_step").count(), 1);
        let source = rows
            .iter()
            .position(|r| r["kind"] == "source_snapshot")
            .unwrap();
        let model = rows.iter().position(|r| r["kind"] == "model_step").unwrap();
        assert!(source < model);
        assert_eq!(
            rows[model]["event"]["raw_provenance_sha256"],
            snapshot.sha256()
        );
        assert_eq!(
            rows[model]["event"]["decision"]["reason"],
            after["no_intent_trace"]["reason"]
                .as_str()
                .unwrap_or("model_returned_no_intent")
        );
        assert!(!session.model.held());
        drop(session);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn stopped_or_missing_predecessor_prevents_model_callback() {
        for stopped in [true, false] {
            let (mut session, snapshot, path) = composition_fixture();
            if stopped {
                session.stop.stop();
            } else {
                session.next_label_ms -= 600_000;
            }
            let before = session.model.observation();
            let frontier = session.next_label_ms;
            let result = session.snapshot(
                &snapshot,
                &[snapshot.evidence().start],
                snapshot.evidence().parts[0].received_at.timestamp_millis(),
            );
            assert_eq!(
                result,
                Err(if stopped {
                    SessionError::Closed
                } else {
                    SessionError::Incomplete
                })
            );
            assert_eq!(session.model.observation(), before);
            assert_eq!(session.next_label_ms, frontier);
            assert!(!session.model.held());
            assert!(session.closed && session.stop.is_stopped());
            let output = std::fs::read_to_string(&path).unwrap();
            assert!(!output
                .lines()
                .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                .any(|row| row["kind"] == "model_step"));
            drop(session);
            std::fs::remove_file(path).unwrap();
        }
    }
}
