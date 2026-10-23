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

use crate::feedback::{Action, Fill, Quote, Request, VirtualBook};

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
}

impl ShadowModel {
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
        // Stage the book effect; on a failed callback this segment becomes terminal.
        let mut next = self.book.clone();
        let fill = match next.observe(quote, applied_at_ms) {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(None),
            Err(_) => return self.stop(Error::Feedback),
        };
        let avg_price = fill.average_price_ticks.unwrap_or(0) as f64 * self.context.tick_size;
        if !avg_price.is_finite() {
            return self.stop(Error::Feedback);
        }
        let mut context = self.context.clone();
        context.position_qty = Some(fill.position_lots as f64);
        context.event_ts_utc = applied_at_ms / 1000;
        context.strategy_now_ts_utc = applied_at_ms / 1000;
        let result = self.model.on_broker_position(HybridRuntimeCallbackInput {
            context: context.clone(),
            payload: HybridRuntimePositionEvent {
                instrument: context.instrument.clone(),
                qty: fill.position_lots as f64,
                avg_price,
                existing: false,
                source_ts_utc: applied_at_ms / 1000,
            },
        });
        match result {
            Ok(intents) if intents.is_empty() => {
                self.book = next;
                self.context = context;
                Ok(Some(fill))
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
