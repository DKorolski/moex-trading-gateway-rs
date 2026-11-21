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
mod primary_recovery;
mod primary_restart;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
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

/// Only the session writer can mint successful model/feedback publication.
/// The coordinator consumes the data capability after receiving this result.
#[derive(Debug)]
pub struct CanonicalPublication {
    pub(crate) label_ms: i64,
    pub(crate) source_sha256: String,
    pub(crate) committed_at_ms: i64,
    pub(crate) model_result_at_ms: i64,
}

impl ObservedSession {
    /// A shared-feed barrier includes every consumer's callback/feedback/fsync.
    /// Rechecking an earlier consumer never calls its model a second time.
    pub(crate) fn check_shared_publication(
        &mut self,
        close_ms: i64,
        replay_deadline_ms: Option<i64>,
    ) -> Result<(), SessionError> {
        self.check()?;
        if replay_deadline_ms.is_some() {
            self.fence.check_now().map_err(|_| SessionError::Timing)?;
        } else {
            self.fence.check_live().map_err(|_| SessionError::Timing)?;
        }
        let now = crate::timing::now_ms();
        match replay_deadline_ms {
            Some(deadline) if now > deadline => Err(SessionError::Timing),
            Some(_) => Ok(()),
            None => self.check_publication_latency(close_ms, now),
        }
    }
    pub(crate) fn eod_model_ready(&self) -> bool {
        !self.closed
            && !self.model.held()
            && self.model.pending().is_none()
            && self.model.position_lots() == 0
            && self.config.primary_policy().is_some()
    }
    pub(crate) fn journal_seal(&self) -> Option<crate::primary::eod::JournalSeal> {
        self.observer.seal()
    }
    pub(crate) fn eod_snapshot(&self) -> serde_json::Value {
        serde_json::json!({"position_lots":self.model.position_lots(),
        "pending":self.model.pending().is_some(),"held":self.model.held(),"next_label_ms":self.next_label_ms})
    }
    pub(crate) fn bind_live_continuity(&mut self, gate: crate::primary::ContinuityGate) {
        self.fence.attach_live_continuity(gate);
    }
    pub(crate) fn primary_runtime(&self) -> Option<(&ValidatedConfig, StopHandle)> {
        self.config
            .primary_policy()
            .map(|_| (&self.config, self.stop.clone()))
    }
    pub(crate) fn primary_binding(&self) -> Option<(&str, i64, &crate::primary::PrimaryPolicy)> {
        self.config
            .primary_policy()
            .map(|policy| (self.config.fingerprint(), self.next_label_ms, policy))
    }
    pub(crate) fn resume_intraday(
        day: crate::intraday::ReconstructedCandleDay,
        stop: StopHandle,
        approval: &crate::intraday::IntradayRunPlan,
    ) -> Result<Self, SessionError> {
        Self::resume_intraday_clock(day, stop, approval, || Utc::now().timestamp_millis())
    }

    pub(crate) fn resume_intraday_clock(
        mut day: crate::intraday::ReconstructedCandleDay,
        stop: StopHandle,
        approval: &crate::intraday::IntradayRunPlan,
        mut clock: impl FnMut() -> i64,
    ) -> Result<Self, SessionError> {
        let began = clock();
        let ready = (|| {
            approval
                .validate_reconstructed(&day)
                .map_err(|_| SessionError::Input)?;
            approval
                .validate_now(&day.config, began)
                .map_err(|_| SessionError::Timing)?;
            if stop.is_stopped() || day.model.pending().is_some() {
                return Err(SessionError::Incomplete);
            }
            let fence =
                CallbackFence::new(&day.config, stop.clone()).map_err(|_| SessionError::Timing)?;
            day.observer.record_transport(&serde_json::json!({
                "event":"intraday_handoff","at_ms":began,"intraday_plan_sha256":day.plan_sha256,
                "next_label_utc_ms":day.next_label_ms,"last_processed_label_utc_ms":day.next_label_ms-600_000,
                "position_lots":day.model.position_lots(),"state":day.model.observation(),
                "historical_mode":ObservationMode::BackfillReplay,
                "subsequent_mode":ObservationMode::LiveObserved,
                "ws_connected":false,"history_proves_live_readiness":false,
            })).map_err(|_| SessionError::Evidence)?;
            let after = clock();
            approval
                .validate_now(&day.config, after)
                .map_err(|_| SessionError::Timing)?;
            if after < began || stop.is_stopped() {
                return Err(SessionError::Timing);
            }
            Ok(fence)
        })();
        let fence = match ready {
            Ok(fence) => fence,
            Err(error) => {
                stop.stop();
                let _ = day.observer.finish("INCOMPLETE_intraday_handoff");
                return Err(error);
            }
        };
        day.model.select_observed();
        Ok(Self {
            config: day.config,
            model: day.model,
            adapter: day.adapter,
            observer: day.observer,
            fence,
            stop,
            next_label_ms: day.next_label_ms,
            closed: false,
            last_quote_ms: None,
            prefix_quote_floor_ms: None,
        })
    }

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
    pub(crate) fn record_primary_reference(
        &mut self,
        source: &AdmittedClosedM1Snapshot,
    ) -> Result<(), SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        if self.config.primary_policy().is_none() {
            return self.fail(SessionError::Input);
        }
        if self.observer.record_source(source).is_err() {
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
        let now = crate::timing::now_ms();
        // Same production UTC clock as CallbackFence; its private cfg(test)
        // sample permits historical component tests, never executable overrides.
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
        if self.config.primary_policy().is_some() {
            return Err(SessionError::Input);
        }
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
            let publication =
                self.apply_model_input(input.bar, &input.snapshot_sha256, received_ms)?;
            self.next_label_ms = next_label(publication.label_ms).ok_or(SessionError::Input)?;
        }
        Ok(())
    }

    /// WS and bounded REST exceptions have one model entry. A caller cannot
    /// replace this capability with a bare Bar or a deserialized receipt.
    pub fn canonical(
        &mut self,
        input: &crate::primary::AdmittedCanonicalM10,
    ) -> Result<CanonicalPublication, SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        let result = self.canonical_inner(input);
        match result {
            Ok(value) => Ok(value),
            Err(error) => self.fail(error),
        }
    }

    fn canonical_inner(
        &mut self,
        input: &crate::primary::AdmittedCanonicalM10,
    ) -> Result<CanonicalPublication, SessionError> {
        self.check()?;
        let policy = self.config.primary_policy().ok_or(SessionError::Input)?;
        if self.model.pending().is_some() {
            return Err(SessionError::Incomplete);
        }
        let receipt = input.receipt();
        if receipt.config_sha256 != self.config.fingerprint()
            || receipt.policy_sha256 != policy.fingerprint().map_err(|_| SessionError::Input)?
            || receipt.label_ms != self.next_label_ms
            || receipt.canonical_digest
                != crate::input::canonical_digest(input.model_bar())
                    .map_err(|_| SessionError::Input)?
            || receipt.canonical_ready_at_ms > crate::timing::now_ms()
            || input.received_at_ms() < receipt.close_ms
        {
            return Err(SessionError::Input);
        }
        let source_sha256 = self
            .observer
            .record_canonical_source(input)
            .map_err(|_| SessionError::Evidence)?;
        let mut publication = self.apply_model_input(
            input.model_bar().clone(),
            &source_sha256,
            input.received_at_ms(),
        )?;
        if self.model.pending().is_some() {
            return Err(SessionError::Incomplete);
        }
        self.observer
            .record_canonical_commit(
                publication.label_ms,
                &source_sha256,
                publication.model_result_at_ms,
                publication.committed_at_ms,
            )
            .map_err(|_| SessionError::Evidence)?;
        // Commit fsync is included in the same original live fence. Failure is
        // terminal; neither this input nor its partially changed model is retried.
        self.fence.check_live().map_err(|_| SessionError::Timing)?;
        let committed = crate::timing::now_ms();
        self.check_publication_latency(receipt.close_ms, committed)?;
        publication.committed_at_ms = committed;
        self.next_label_ms = next_label(receipt.label_ms).ok_or(SessionError::Input)?;
        Ok(publication)
    }

    fn check_publication_latency(&self, close: i64, published_ms: i64) -> Result<(), SessionError> {
        let latency = published_ms
            .checked_sub(close)
            .and_then(|v| u64::try_from(v).ok())
            .and_then(|v| v.checked_add(self.config.config().clock_uncertainty_ms));
        let limit = self.config.config().lmax_ms.min(
            if self
                .config
                .primary_policy()
                .is_some_and(|p| p.observed_bo())
            {
                2000
            } else {
                u64::MAX
            },
        );
        if latency.map_or(true, |v| v > limit) {
            return Err(SessionError::Timing);
        }
        Ok(())
    }

    fn apply_model_input(
        &mut self,
        mut bar: broker_core::HybridRuntimeBarEvent,
        provenance: &str,
        received_ms: i64,
    ) -> Result<CanonicalPublication, SessionError> {
        let label = bar
            .close_time_utc
            .checked_mul(1000)
            .ok_or(SessionError::Input)?;
        if label != self.next_label_ms {
            return Err(SessionError::Incomplete);
        }
        let ready = crate::timing::now_ms();
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
                raw_provenance_sha256: provenance.to_owned(),
                before,
                after,
                decision,
            })
            .map_err(|_| SessionError::Evidence)?;
        if self.config.candle_only() {
            if let Some(request) = &request {
                self.fence.check_live().map_err(|_| SessionError::Timing)?;
                let fill = self
                    .model
                    .on_candle_close(request, received_ms, crate::timing::now_ms(), provenance)
                    .map_err(|_| SessionError::Model)?;
                self.observer
                    .record_candle_feedback(
                        ObservationMode::LiveObserved,
                        &fill,
                        &self.model.observation(),
                    )
                    .map_err(|_| SessionError::Evidence)?;
            }
        }
        let published_ms = crate::timing::now_ms();
        self.observer
            .record_transport(&serde_json::json!({
                "event": "model_step_durable_publication",
                "model_label_utc_ms": label,
                "published_at_ms": published_ms,
            }))
            .map_err(|_| SessionError::Evidence)?;
        self.fence.check_live().map_err(|_| SessionError::Timing)?;
        let durable_at = crate::timing::now_ms();
        self.check_publication_latency(close, durable_at)?;
        Ok(CanonicalPublication {
            label_ms: label,
            source_sha256: provenance.to_owned(),
            committed_at_ms: durable_at,
            model_result_at_ms: timing.result_at_ms,
        })
    }

    /// Persist raw source before feedback, including empty deduplicated frames.
    /// This does not assert complete coverage across process downtime.
    pub fn quote_frame(&mut self, frame: crate::ws::QuoteFrame) -> Result<(), SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        if self.config.candle_only() {
            return self.fail(SessionError::Input);
        }
        if let Err(error) = self.check() {
            return self.fail(error);
        }
        if frame.initial_discard.is_some()
            || frame.raw_frame.len() > 256 * 1024
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
        if self.config.candle_only() {
            return self.fail(SessionError::Input);
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
            || crate::timing::now_ms() >= self.config.config().run_deadline_utc_ms
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
                close <= crate::timing::now_ms().min(self.config.config().run_deadline_utc_ms)
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
pub(crate) mod primary_tests;

#[cfg(test)]
pub(crate) mod tests {
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
    pub(crate) fn composition_fixture() -> (
        ObservedSession,
        AdmittedClosedM1Snapshot,
        std::path::PathBuf,
    ) {
        composition_fixture_policy(false)
    }

    pub(crate) fn composition_fixture_policy(
        candle_only: bool,
    ) -> (
        ObservedSession,
        AdmittedClosedM1Snapshot,
        std::path::PathBuf,
    ) {
        use crate::{seed::digest, segment::SeedObservation};
        use broker_core::*;
        use broker_finam::sparse_m10::*;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let now = DateTime::from_timestamp_millis(crate::timing::now_ms()).unwrap();
        let close_ms = now.timestamp_millis().div_euclid(600_000) * 600_000;
        let start = DateTime::from_timestamp_millis(close_ms - 600_000).unwrap();
        let end = DateTime::from_timestamp_millis(close_ms).unwrap();
        let instrument = InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        };
        let mut model = ShadowModel::baseline07(
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
        if candle_only {
            model.select_candle_only();
        }
        let policy = if candle_only {
            crate::feedback::candle_policy_fingerprint()
        } else {
            crate::feedback::policy_fingerprint()
        };
        let config = ValidatedConfig::parse(&serde_json::to_vec(&serde_json::json!({
            "schema_version":1, "reference":"alor_freeze_2026_09_27", "restart_mode":"new_segment",
            "model_profile":"imoexf-baseline07-bo-only-no-riskgate",
            "model_sha256":model.model_config_fingerprint(), "reference_sha256":"a".repeat(64),
            "feedback_policy_sha256":policy,
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
            Utc::now() + chrono::Duration::seconds(60),
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

    /// Actual cold model + test-only clock. This is not warmup, network or
    /// operational evidence. The production constructor remains unchanged.
    pub(crate) fn primary_fixture() -> (
        ObservedSession,
        crate::primary::WsPrimaryConfig,
        AdmittedClosedM1Snapshot,
        std::path::PathBuf,
    ) {
        let label = crate::timing::now_ms().div_euclid(600000) * 600000 - 600000;
        primary_fixture_at(label)
    }

    pub(crate) fn primary_fixture_at(
        label: i64,
    ) -> (
        ObservedSession,
        crate::primary::WsPrimaryConfig,
        AdmittedClosedM1Snapshot,
        std::path::PathBuf,
    ) {
        primary_fixture_mode(label, true)
    }
    pub(crate) fn primary_fixture_mode(
        label: i64,
        audit: bool,
    ) -> (
        ObservedSession,
        crate::primary::WsPrimaryConfig,
        AdmittedClosedM1Snapshot,
        std::path::PathBuf,
    ) {
        primary_fixture_delivery(
            label,
            audit,
            crate::primary::DeliveryPolicy::StrictBoundaryV1,
        )
    }
    pub(crate) fn primary_fixture_delivery(
        label: i64,
        audit: bool,
        delivery: crate::primary::DeliveryPolicy,
    ) -> (
        ObservedSession,
        crate::primary::WsPrimaryConfig,
        AdmittedClosedM1Snapshot,
        std::path::PathBuf,
    ) {
        primary_fixture_delivery_input_limit(label, audit, delivery, 100_000)
    }
    pub(crate) fn primary_fixture_delivery_input_limit(
        label: i64,
        audit: bool,
        delivery: crate::primary::DeliveryPolicy,
        input_limit: u64,
    ) -> (
        ObservedSession,
        crate::primary::WsPrimaryConfig,
        AdmittedClosedM1Snapshot,
        std::path::PathBuf,
    ) {
        let (mut session, source, old_path) = composition_fixture_policy(true);
        let mut base = session.config.config().clone();
        base.max_input_bytes = input_limit;
        base.observation_start_utc_ms = label;
        session.next_label_ms = base.observation_start_utc_ms;
        base.lmax_ms = 10000;
        base.clock_uncertainty_ms = 100;
        base.max_cache_bars = 4096;
        base.run_deadline_utc_ms = base.observation_start_utc_ms + 17 * 60 * 60 * 1000;
        base.max_output_bytes = 4 * 1024 * 1024;
        let primary = crate::primary::WsPrimaryConfig {
            schema_version: 2,
            market_data_mode: "ws_primary_v1".into(),
            source_policy: crate::primary::WS_PRIMARY_POLICY.into(),
            base,
            primary: crate::primary::PrimaryPolicy {
                delivery,
                audit_every_commits: audit.then_some(6),
                ..Default::default()
            },
        };
        let config = ValidatedConfig::from_ws_primary(&primary).unwrap();
        let c = config.config();
        let header = crate::segment::SeedObservation {
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
            state: session.model.observation(),
        };
        let path = old_path.with_extension("primary.jsonl");
        session.observer = Observer::create(&path, header, c.max_output_bytes, 1_048_576).unwrap();
        session.fence = CallbackFence::new(&config, session.stop.clone()).unwrap();
        session.config = config;
        std::fs::remove_file(old_path).unwrap();
        (session, primary, source, path)
    }

    #[test]
    fn candle_observed_sparse_snapshot_dedup_and_quote_rejection() {
        let (mut session, snapshot, path) = composition_fixture_policy(true);
        let labels = [snapshot.evidence().start];
        let received = snapshot.evidence().parts[0].received_at.timestamp_millis();
        session.snapshot(&snapshot, &labels, received).unwrap();
        let before = std::fs::read(&path).unwrap();
        session.snapshot(&snapshot, &labels, received).unwrap();
        assert_eq!(before, std::fs::read(&path).unwrap());
        let text = String::from_utf8(before).unwrap();
        assert!(text.contains("source_snapshot"));
        assert!(text.contains("model_step"));
        assert!(text.contains(&crate::feedback::candle_policy_fingerprint()));
        assert_eq!(
            session.quote(Quote {
                instrument: "IMOEXF".into(),
                source_id: "rejected".into(),
                price_ticks: 4400,
                event_at_ms: received,
                received_at_ms: received,
            }),
            Err(SessionError::Input)
        );
        assert!(session.stop.is_stopped());
        assert!(session.closed);
        drop(session);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn candle_transports_cannot_connect_quotes() {
        let (session, _, path) = composition_fixture_policy(true);
        let mut transports = crate::transport::ConfiguredTransports::new(&session.config).unwrap();
        let lease = broker_finam::AccessTokenLease::new(
            broker_finam::AccessToken::new("never-sent"),
            tokio::time::Instant::now().into_std(),
            broker_finam::FinamAuthPolicy::default(),
        );
        let result = transports.connect_quotes(lease).await;
        assert!(matches!(
            result,
            Err(crate::transport::TransportError::Held)
        ));
        assert!(!transports.stop_handle().is_stopped());
        drop(transports);
        drop(session);
        std::fs::remove_file(path).unwrap();
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
                        initial_discard: None,
                        auxiliary_total: 0,
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
                initial_discard: None,
                auxiliary_total: 0,
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
                initial_discard: None,
                auxiliary_total: 0,
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
    fn diagnostic_initial_frame_cannot_enter_model_or_feedback() {
        let (mut session, _, path) = composition_fixture();
        let before = session.model.observation();
        let mut observations = crate::ws::initial_quote_tests::observations();
        let crate::ws::Observation::Quotes(mut frame) = observations.remove(1) else {
            panic!("expected diagnostic discard");
        };
        frame.received_at_ms = Utc::now().timestamp_millis();
        assert_eq!(session.quote_frame(frame), Err(SessionError::Input));
        assert_eq!(session.model.observation(), before);
        assert!(session.closed && session.stop.is_stopped());
        drop(session);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn projected_auxiliary_frame_preserves_raw_without_model_effect() {
        let (mut session, _, path) = composition_fixture();
        let now = Utc::now();
        let mut input = crate::quote::QuoteInput::new(5000, 4096).unwrap();
        let row = |aux: bool| {
            let mut r = serde_json::json!({"symbol":"IMOEXF@RTSX","timestamp":now.to_rfc3339(),"last":{"value":"2535.5"}});
            if aux {
                r["bid"] = serde_json::json!({"value":"2535"});
            }
            serde_json::json!({"type":"DATA","subscriptionType":"QUOTES","payload":{"quote":[r]}})
                .to_string()
        };
        assert_eq!(input.receive(&row(false), now).unwrap().len(), 1);
        let raw = row(true);
        let quotes = input.receive(&raw, now).unwrap();
        assert!(quotes.is_empty());
        let before = session.model.observation();
        session
            .quote_frame(crate::ws::QuoteFrame {
                initial_discard: None,
                auxiliary_total: input.auxiliary_total(),
                raw_frame: raw.clone(),
                received_at_ms: now.timestamp_millis(),
                quotes,
            })
            .unwrap();
        assert_eq!(session.model.observation(), before);
        let text = std::fs::read_to_string(&path).unwrap();
        let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(last["event"]["raw_frame"], raw);
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
