//! Local model/feedback composition; not feed admission or operational readiness.
//! Uses only existing broker-neutral callbacks. No Stage6/7 owner is attached.

use broker_core::{
    HybridRuntimeBarEvent, HybridRuntimeCallbackInput, HybridRuntimePositionEvent,
    HybridRuntimeStrategyContext, HybridRuntimeTradeMode,
};
use strategy_runtime_core::{
    BrokerNeutralHybridIntent as Intent, BrokerNeutralHybridStrategy,
    BrokerNeutralOrderSide as Side, HybridIntradayRuntimeStrategy,
};

use crate::feedback::{Action, CandleFill, Fill, Quote, Request, VirtualBook, CANDLE_POLICY};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Configuration,
    Input,
    Pending,
    Hold,
    Callback,
    UnsupportedIntent,
    Feedback,
    Timing,
}

/// A caller must bind the freshly constructed model/profile and seed before use.
/// This bridge alone grants neither source trust nor comparison eligibility.
pub struct ShadowModel {
    model: HybridIntradayRuntimeStrategy,
    context: HybridRuntimeStrategyContext,
    book: VirtualBook,
    sequence: u64,
    last_bar: Option<HybridRuntimeBarEvent>,
    held: bool,
    candle_only: bool,
}

impl ShadowModel {
    pub(crate) fn select_candle_only(&mut self) {
        self.candle_only = true;
    }
    pub(crate) fn select_observed(&mut self) {
        self.context.paper_execution_mode = broker_core::HybridRuntimePaperExecutionMode::LiveOnly;
        self.context.gateway_phase = broker_core::HybridRuntimeGatewayPhase::LiveReady;
    }
    pub(crate) fn select_backfill(&mut self) {
        self.context.paper_execution_mode =
            broker_core::HybridRuntimePaperExecutionMode::HistorySim;
        self.context.gateway_phase = broker_core::HybridRuntimeGatewayPhase::CatchingUp;
    }
    /// Seed from four explicit, completed model-session dates. Dataset hash
    /// binds the admitted input inventory, not independent provider truth.
    pub fn baseline07_warmed(
        mut context: HybridRuntimeStrategyContext,
        history: &[HybridRuntimeBarEvent],
        sessions: [chrono::NaiveDate; 4],
        expected_history_sha256: &str,
        max_quote_age_ms: i64,
    ) -> Result<Self, Error> {
        use chrono::{Datelike, Timelike};
        use sha2::{Digest, Sha256};
        if history.is_empty() || history.len() > 4096 || sessions.windows(2).any(|p| p[0] >= p[1]) {
            return Err(Error::Input);
        }
        let bytes = serde_json::to_vec(history).map_err(|_| Error::Input)?;
        if format!("{:x}", Sha256::digest(&bytes)) != expected_history_sha256 {
            return Err(Error::Input);
        }
        let mut first = [None; 4];
        let mut last = [None; 4];
        let start = chrono::DateTime::from_timestamp(context.event_ts_utc, 0)
            .and_then(|t| t.checked_add_signed(chrono::Duration::hours(3)))
            .ok_or(Error::Input)?;
        for bar in history {
            let local = chrono::DateTime::from_timestamp(bar.close_time_utc, 0)
                .and_then(|t| t.checked_add_signed(chrono::Duration::hours(3)))
                .ok_or(Error::Input)?;
            let date = local.date_naive();
            let i = sessions
                .iter()
                .position(|s| *s == date)
                .ok_or(Error::Input)?;
            if date >= start.date_naive()
                || local.weekday().number_from_monday() > 5
                || local.second() != 0
                || local.minute() % 10 != 0
            {
                return Err(Error::Input);
            }
            let minute = local.hour() * 60 + local.minute();
            if !(420..=1420).contains(&minute) {
                return Err(Error::Input);
            }
            if first[i].is_none() {
                first[i] = Some(minute);
            }
            last[i] = Some(minute);
        }
        if first != [Some(420); 4] || last != [Some(1420); 4] {
            return Err(Error::Input);
        }
        let config = crate::profile::baseline07_config().map_err(|_| Error::Configuration)?;
        if context.tick_size != config.tick_size {
            return Err(Error::Configuration);
        }
        context.last_bar_ts_utc = history.last().map(|bar| bar.close_time_utc);
        let model = HybridIntradayRuntimeStrategy::new_signal_shadow_from_history(
            config,
            context.clone(),
            history,
        )
        .map_err(|_| Error::Input)?;
        let mut host = Self::new(model, context, 1, max_quote_age_ms)?;
        host.last_bar = history.last().cloned();
        Ok(host)
    }

    pub fn observation(&self) -> serde_json::Value {
        self.model.signal_shadow_observation()
    }

    pub fn baseline07(
        context: HybridRuntimeStrategyContext,
        max_quote_age_ms: i64,
    ) -> Result<Self, Error> {
        let config = crate::profile::baseline07_config().map_err(|_| Error::Configuration)?;
        if context.instrument.symbol != config.symbol || context.tick_size != config.tick_size {
            return Err(Error::Configuration);
        }
        Self::new(
            HybridIntradayRuntimeStrategy::new(config),
            context,
            1,
            max_quote_age_ms,
        )
    }

    fn new(
        model: HybridIntradayRuntimeStrategy,
        context: HybridRuntimeStrategyContext,
        max_lots: u32,
        max_quote_age_ms: i64,
    ) -> Result<Self, Error> {
        if context.trade_mode != HybridRuntimeTradeMode::Paper
            || context.allow_live_orders
            || context.position_qty != Some(0.0)
            || !context.tick_size.is_finite()
            || context.tick_size <= 0.0
        {
            return Err(Error::Configuration);
        }
        let book = VirtualBook::new(
            context.instrument.symbol.clone(),
            max_lots,
            max_quote_age_ms,
        )
        .map_err(|_| Error::Configuration)?;
        Ok(Self {
            model,
            context,
            book,
            sequence: 0,
            last_bar: None,
            held: false,
            candle_only: false,
        })
    }

    pub fn position_lots(&self) -> i64 {
        self.book.position_lots()
    }
    pub fn pending(&self) -> Option<&Request> {
        self.book.pending()
    }
    pub fn held(&self) -> bool {
        self.held
    }
    pub fn model_config_fingerprint(&self) -> String {
        self.model.stage5c_config_fingerprint()
    }

    /// Measured callback path. Feed admission and configuration identity must be
    /// established by the composition before calling this method.
    /// A late/error callback may have mutated the model: the segment is terminal.
    pub fn on_bar_measured(
        &mut self,
        bar: HybridRuntimeBarEvent,
        received_ms: i64,
        ready_ms: i64,
        fence: &mut crate::timing::CallbackFence,
    ) -> Result<(Option<Request>, crate::timing::CallbackTiming), Error> {
        if self.held {
            return Err(Error::Hold);
        }
        let close = match bar
            .close_time_utc
            .checked_add(600)
            .and_then(|v| v.checked_mul(1000))
        {
            Some(v) => v,
            None => return self.stop(Error::Input),
        };
        // Poison first so panic unwinding cannot leave a reusable model.
        self.held = true;
        let result = fence.execute(close, received_ms, ready_ms, |start| {
            self.on_bar_inner(bar, start)
        });
        match result {
            Ok((request, timing)) => {
                let request = match request {
                    Some(request) => match self.book.seal_result(&request, timing.result_at_ms) {
                        Ok(sealed) => Some(sealed),
                        Err(_) => return self.stop(Error::Feedback),
                    },
                    None => None,
                };
                self.held = false;
                Ok((request, timing))
            }
            Err(_) => self.stop(Error::Timing),
        }
    }

    /// Exact last-input replay is a no-op. Older/conflicting inputs stop this segment.
    /// The future input adapter must supply closed-range/session evidence.
    pub fn on_bar(
        &mut self,
        bar: HybridRuntimeBarEvent,
        result_at_ms: i64,
    ) -> Result<Option<Request>, Error> {
        if self.held {
            return Err(Error::Hold);
        }
        self.on_bar_inner(bar, result_at_ms)
    }

    /// Historical deterministic reconstruction uses the recorded callback and
    /// result clocks, NOT a fake current UTC clock or a live timing exception.
    pub(crate) fn on_recorded_bar(
        &mut self,
        bar: HybridRuntimeBarEvent,
        started_ms: i64,
        result_ms: i64,
    ) -> Result<Option<Request>, Error> {
        if self.held || result_ms < started_ms {
            return self.stop(Error::Timing);
        }
        self.held = true;
        let result = self
            .on_bar_inner(bar, started_ms)
            .and_then(|request| match request {
                Some(request) => self
                    .book
                    .seal_result(&request, result_ms)
                    .map(Some)
                    .map_err(|_| Error::Feedback),
                None => Ok(None),
            });
        match result {
            Ok(value) => {
                self.held = false;
                Ok(value)
            }
            Err(e) => self.stop(e),
        }
    }

    fn on_bar_inner(
        &mut self,
        bar: HybridRuntimeBarEvent,
        result_at_ms: i64,
    ) -> Result<Option<Request>, Error> {
        if let Some(last) = &self.last_bar {
            if last == &bar {
                return Ok(None);
            }
            if bar.close_time_utc <= last.close_time_utc {
                return self.stop(Error::Input);
            }
        }
        let close_ms = bar
            .close_time_utc
            .checked_add(600)
            .and_then(|v| v.checked_mul(1000));
        if bar.instrument != self.context.instrument
            || !bar.is_final
            || bar.timeframe_sec != 600
            || close_ms.map_or(true, |t| t < 0 || result_at_ms < t)
            || [bar.open, bar.high, bar.low, bar.close, bar.volume]
                .iter()
                .any(|v| !v.is_finite())
            || bar.low <= 0.0
            || bar.high < bar.low
            || bar.open < bar.low
            || bar.open > bar.high
            || bar.close < bar.low
            || bar.close > bar.high
            || bar.volume < 0.0
        {
            return self.stop(Error::Input);
        }
        if self.book.pending().is_some() {
            return Err(Error::Pending);
        }
        self.context.position_qty = Some(self.book.position_lots() as f64);
        self.context.event_ts_utc = result_at_ms / 1000;
        self.context.strategy_now_ts_utc = result_at_ms / 1000;
        self.context.last_bar_ts_utc = Some(bar.close_time_utc);
        let intents = match self.model.on_broker_bar(HybridRuntimeCallbackInput {
            context: self.context.clone(),
            payload: bar.clone(),
        }) {
            Ok(v) => v,
            Err(_) => return self.stop(Error::Callback),
        };
        self.last_bar = Some(bar);
        if intents.is_empty() {
            return Ok(None);
        }
        if intents.len() != 1 {
            return self.stop(Error::UnsupportedIntent);
        }
        let (qty, side) = match market(&intents[0], &self.context.instrument.symbol, 0) {
            Some(v) => v,
            None => return self.stop(Error::UnsupportedIntent),
        };
        if !qty.is_finite() || qty <= 0.0 || qty > u32::MAX as f64 || qty.fract() != 0.0 {
            return self.stop(Error::UnsupportedIntent);
        }
        let action = match (self.book.position_lots(), side) {
            (0, Side::Buy) => Action::EnterLong,
            (0, Side::Sell) => Action::EnterShort,
            (p, Side::Sell) if p > 0 => Action::Exit,
            (p, Side::Buy) if p < 0 => Action::Exit,
            _ => return self.stop(Error::UnsupportedIntent),
        };
        let Some(sequence) = self.sequence.checked_add(1) else {
            return self.stop(Error::Hold);
        };
        let request = Request {
            sequence,
            instrument: self.context.instrument.symbol.clone(),
            action,
            quantity_lots: qty as u32,
            decision_at_ms: result_at_ms,
        };
        if self.book.submit(request.clone()).is_err() {
            return self.stop(Error::Feedback);
        }
        self.sequence = sequence;
        Ok(Some(request))
    }

    pub fn on_quote(&mut self, quote: Quote, applied_at_ms: i64) -> Result<Option<Fill>, Error> {
        if self.held {
            return Err(Error::Hold);
        }
        if self.candle_only {
            return self.stop(Error::Feedback);
        }
        // Stage the book effect; on a failed callback this segment becomes terminal.
        let mut next = self.book.clone();
        let fill = match next.observe(quote, applied_at_ms) {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(None),
            Err(_) => return self.stop(Error::Feedback),
        };
        self.commit_position(next, applied_at_ms)?;
        Ok(Some(fill))
    }

    /// Settle only the exact last accepted closed bar and pending intent.
    /// Applied time is not shifted to manufacture a post-intent source quote.
    pub(crate) fn on_candle_close(
        &mut self,
        request: &Request,
        received_at_ms: i64,
        applied_at_ms: i64,
        raw_provenance_sha256: &str,
    ) -> Result<CandleFill, Error> {
        if self.held {
            return Err(Error::Hold);
        }
        self.held = true;
        let result = self.candle_close_inner(
            request,
            received_at_ms,
            applied_at_ms,
            raw_provenance_sha256,
        );
        match result {
            Ok(fill) => {
                self.held = false;
                Ok(fill)
            }
            Err(error) => self.stop(error),
        }
    }

    fn candle_close_inner(
        &mut self,
        request: &Request,
        received_at_ms: i64,
        applied_at_ms: i64,
        raw_provenance_sha256: &str,
    ) -> Result<CandleFill, Error> {
        let bar = self.last_bar.as_ref().ok_or(Error::Feedback)?;
        let label = bar.close_time_utc.checked_mul(1000).ok_or(Error::Input)?;
        let close = label.checked_add(600_000).ok_or(Error::Input)?;
        let ticks = bar.close / self.context.tick_size;
        if !self.candle_only
            || !bar.is_final
            || bar.timeframe_sec != 600
            || label < 0
            || label % 600_000 != 0
            || received_at_ms < close
            || request.decision_at_ms < received_at_ms
            || applied_at_ms < request.decision_at_ms
            || request.quantity_lots != 1
            || !ticks.is_finite()
            || ticks <= 0.0
            || ticks >= i64::MAX as f64
            || ticks.fract() != 0.0
            || raw_provenance_sha256.len() != 64
            || !raw_provenance_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Feedback);
        }
        let mut next = self.book.clone();
        next.settle_close(request, ticks as i64, applied_at_ms)
            .map_err(|_| Error::Feedback)?;
        let fill = CandleFill {
            request: request.clone(),
            price_ticks: ticks as i64,
            model_label_utc_ms: label,
            nominal_close_utc_ms: close,
            received_at_ms,
            applied_at_ms,
            canonical_input_sha256: crate::input::canonical_digest(bar)
                .map_err(|_| Error::Input)?,
            raw_provenance_sha256: raw_provenance_sha256.into(),
            position_lots: next.position_lots(),
            average_price_ticks: next.average_price_ticks(),
            policy: CANDLE_POLICY.into(),
        };
        self.commit_position(next, applied_at_ms)?;
        Ok(fill)
    }

    fn commit_position(&mut self, next: VirtualBook, applied_at_ms: i64) -> Result<(), Error> {
        let avg_price = next.average_price_ticks().unwrap_or(0) as f64 * self.context.tick_size;
        if !avg_price.is_finite() {
            return self.stop(Error::Feedback);
        }
        let mut context = self.context.clone();
        context.position_qty = Some(next.position_lots() as f64);
        context.event_ts_utc = applied_at_ms / 1000;
        context.strategy_now_ts_utc = applied_at_ms / 1000;
        let result = self.model.on_broker_position(HybridRuntimeCallbackInput {
            context: context.clone(),
            payload: HybridRuntimePositionEvent {
                instrument: context.instrument.clone(),
                qty: next.position_lots() as f64,
                avg_price,
                existing: false,
                source_ts_utc: applied_at_ms / 1000,
            },
        });
        match result {
            Ok(intents) if intents.is_empty() => {
                self.book = next;
                self.context = context;
                Ok(())
            }
            _ => self.stop(Error::Callback),
        }
    }

    fn stop<T>(&mut self, error: Error) -> Result<T, Error> {
        self.held = true;
        Err(error)
    }
}

fn market(intent: &Intent, symbol: &str, depth: u8) -> Option<(f64, Side)> {
    if depth > 4 {
        return None;
    }
    match intent {
        Intent::Classified { intent, .. } => market(intent, symbol, depth + 1),
        Intent::Routed {
            intent,
            symbol: target,
        } if target == symbol => market(intent, symbol, depth + 1),
        Intent::Market {
            qty,
            side,
            fill_price: None,
            ..
        } => Some((*qty, *side)),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod candle_tests {
    use super::*;
    use broker_core::*;

    fn empty_model() -> ShadowModel {
        let instrument = InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        };
        let mut model = ShadowModel::baseline07(
            HybridRuntimeStrategyContext {
                strategy_id: "candle-negative".into(),
                request_namespace_account: BrokerAccountId("shadow-test".into()),
                instrument: instrument.clone(),
                tick_size: 0.5,
                trade_mode: HybridRuntimeTradeMode::Paper,
                paper_execution_mode: HybridRuntimePaperExecutionMode::LiveOnly,
                allow_live_orders: false,
                gateway_phase: HybridRuntimeGatewayPhase::LiveReady,
                position_qty: Some(0.0),
                event_ts_utc: 0,
                strategy_now_ts_utc: 0,
                last_bar_ts_utc: None,
            },
            1000,
        )
        .unwrap();
        model.select_candle_only();
        model
    }

    fn pending() -> (ShadowModel, Request, i64) {
        let mut model = empty_model();
        let instrument = model.context.instrument.clone();
        let raw = include_bytes!("../../../fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv");
        for r in csv::Reader::from_reader(raw.as_slice()).records() {
            let r = r.unwrap();
            let n = |i| r.get(i).unwrap().parse().unwrap();
            let bar = HybridRuntimeBarEvent {
                instrument: instrument.clone(),
                close_time_utc: chrono::DateTime::parse_from_str(&r[1], "%Y-%m-%d %H:%M:%S%:z")
                    .unwrap()
                    .timestamp(),
                open: n(3),
                high: n(4),
                low: n(5),
                close: n(6),
                volume: n(7),
                origin: HybridRuntimeBarOrigin::Live,
                is_final: true,
                timeframe_sec: 600,
            };
            let now = (bar.close_time_utc + 600) * 1000 + 17;
            if let Some(request) = model.on_bar(bar, now).unwrap() {
                return (model, request, now);
            }
        }
        panic!("fixture must produce a BO request");
    }

    // Test-only existing frozen-input host. It returns the real model/book,
    // never an imported position value or production state-rebind API.
    pub(crate) fn nonflat(side: i64) -> (ShadowModel, HybridRuntimeBarEvent) {
        let (mut model, request, now) = pending();
        model
            .on_candle_close(&request, now, now, &"a".repeat(64))
            .unwrap();
        if model.position_lots() == side {
            let last = model.last_bar.clone().unwrap();
            return (model, last);
        }
        let raw = include_bytes!("../../../fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv");
        for record in csv::Reader::from_reader(raw.as_slice()).records() {
            let record = record.unwrap();
            let label = chrono::DateTime::parse_from_str(&record[1], "%Y-%m-%d %H:%M:%S%:z")
                .unwrap()
                .timestamp();
            if label <= model.last_bar.as_ref().unwrap().close_time_utc {
                continue;
            }
            let number = |i| record.get(i).unwrap().parse().unwrap();
            let bar = HybridRuntimeBarEvent {
                instrument: model.context.instrument.clone(),
                close_time_utc: label,
                timeframe_sec: 600,
                is_final: true,
                origin: HybridRuntimeBarOrigin::Live,
                open: number(3),
                high: number(4),
                low: number(5),
                close: number(6),
                volume: number(7),
            };
            let now = (label + 600) * 1000 + 17;
            if let Some(request) = model.on_bar(bar.clone(), now).unwrap() {
                model
                    .on_candle_close(&request, now, now, &"a".repeat(64))
                    .unwrap();
            }
            if model.position_lots() == side {
                return (model, bar);
            }
        }
        panic!("frozen fixture must cover both position sides");
    }

    #[test]
    fn resilience_bo_decisions_and_feedback_are_invariant_to_valid_volume() {
        let mut models = [empty_model(), empty_model(), empty_model()];
        let raw = include_bytes!("../../../fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv");
        let mut count = 0;
        let mut fills = 0;
        for r in csv::Reader::from_reader(raw.as_slice()).records() {
            let r = r.unwrap();
            let n = |i| r.get(i).unwrap().parse().unwrap();
            let bar = HybridRuntimeBarEvent {
                instrument: models[0].context.instrument.clone(),
                close_time_utc: chrono::DateTime::parse_from_str(&r[1], "%Y-%m-%d %H:%M:%S%:z")
                    .unwrap()
                    .timestamp(),
                open: n(3),
                high: n(4),
                low: n(5),
                close: n(6),
                volume: n(7),
                origin: HybridRuntimeBarOrigin::Live,
                is_final: true,
                timeframe_sec: 600,
            };
            let now = (bar.close_time_utc + 600) * 1000 + 17;
            let volumes = [bar.volume, 0.0, 1_000_000.0 + count as f64];
            let mut requests = Vec::new();
            for (model, volume) in models.iter_mut().zip(volumes) {
                let mut input = bar.clone();
                input.volume = volume;
                requests.push(model.on_bar(input, now).unwrap());
            }
            assert_eq!(requests[0], requests[1]);
            assert_eq!(requests[0], requests[2]);
            assert_eq!(models[0].observation(), models[1].observation());
            assert_eq!(models[0].observation(), models[2].observation());
            for (model, request) in models.iter_mut().zip(&requests) {
                if let Some(request) = request {
                    model
                        .on_candle_close(request, now, now, &"a".repeat(64))
                        .unwrap();
                }
            }
            fills += usize::from(requests[0].is_some());
            assert_eq!(models[0].observation(), models[1].observation());
            assert_eq!(models[0].observation(), models[2].observation());
            count += 1;
        }
        assert!(count >= 505);
        assert!(fills >= 10);
        eprintln!("BO_VOLUME_INVARIANCE bars={count} feedback={fills} variants=3");
    }

    #[test]
    fn candle_exact_request_and_causal_times_or_terminal_no_book_effect() {
        for case in [
            "sequence",
            "instrument",
            "action",
            "qty",
            "received",
            "applied",
            "hash",
            "policy",
        ] {
            let (mut model, original, now) = pending();
            let mut request = original.clone();
            let mut received = now;
            let mut applied = now;
            let mut hash = "a".repeat(64);
            match case {
                "sequence" => request.sequence += 1,
                "instrument" => request.instrument = "OTHER".into(),
                "action" => request.action = Action::Exit,
                "qty" => request.quantity_lots = 2,
                "received" => received = now + 1,
                "applied" => applied = now - 1,
                "hash" => hash = "not-a-hash".into(),
                "policy" => model.candle_only = false,
                _ => unreachable!(),
            }
            let before = model.observation();
            assert!(
                model
                    .on_candle_close(&request, received, applied, &hash)
                    .is_err(),
                "{case}"
            );
            assert_eq!(model.pending(), Some(&original), "{case}");
            assert_eq!(model.position_lots(), 0);
            assert_eq!(model.observation(), before);
            assert!(model.held());
        }
    }

    #[test]
    fn candle_exact_fill_once_and_no_fake_quote_or_backdated_time() {
        let (mut model, request, now) = pending();
        let price = model.last_bar.as_ref().unwrap().close;
        let fill = model
            .on_candle_close(&request, now, now + 2, &"a".repeat(64))
            .unwrap();
        assert_eq!(fill.price_ticks as f64 * 0.5, price);
        assert_eq!(fill.applied_at_ms, now + 2);
        assert_eq!(fill.request.decision_at_ms, now);
        assert!(model.pending().is_none());
        let before = model.observation();
        assert!(model
            .on_candle_close(&request, now, now + 3, &"a".repeat(64))
            .is_err());
        assert_eq!(before, model.observation());
        assert!(model.held());
    }
}
