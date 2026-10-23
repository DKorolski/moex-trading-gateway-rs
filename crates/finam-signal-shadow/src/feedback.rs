//! Deterministic hypothetical full-fill book. No broker IDs, IO or dispatch.
//! Quote eligibility/finality/provenance must be established by the data adapter.
//! Prices are integer ticks, quantities are integer lots; no implicit rounding.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Explicit comparison assumption, NOT the ALOR broker execution policy.
pub const POLICY: &str = "moex.shadow.first-eligible-post-intent-quote.full-fill.zero-cost.v1";

pub fn policy_fingerprint() -> String {
    format!("{:x}", Sha256::digest(POLICY.as_bytes()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    EnterLong,
    EnterShort,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Strictly increasing within the segment, never a broker order ID.
    pub sequence: u64,
    pub instrument: String,
    pub action: Action,
    pub quantity_lots: u32,
    /// Actual model result time, not the candle label.
    pub decision_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    pub instrument: String,
    pub source_id: String,
    pub price_ticks: i64,
    pub event_at_ms: i64,
    pub received_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fill {
    pub request: Request,
    pub quote: Quote,
    pub applied_at_ms: i64,
    pub position_lots: i64,
    pub average_price_ticks: Option<i64>,
    pub policy: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidRequest,
    InvalidQuote,
    Busy,
    InvalidPositionTransition,
    ReplayedSequence,
    NoPending,
    Clock,
    Bound,
}

/// One pending intent, no unbounded order history. The observer owns the log.
/// Losing this book requires NEW_SEGMENT, never a silent cold-flat continuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualBook {
    instrument: String,
    max_lots: u32,
    max_quote_age_ms: i64,
    position_lots: i64,
    average_price_ticks: Option<i64>,
    pending: Option<Request>,
    last_sequence: u64,
    last_effect_at_ms: Option<i64>,
}

impl VirtualBook {
    pub fn new(instrument: String, max_lots: u32, max_quote_age_ms: i64) -> Result<Self, Error> {
        if !valid_id(&instrument) || max_lots == 0 || max_quote_age_ms <= 0 {
            return Err(Error::Bound);
        }
        Ok(Self {
            instrument,
            max_lots,
            max_quote_age_ms,
            position_lots: 0,
            average_price_ticks: None,
            pending: None,
            last_sequence: 0,
            last_effect_at_ms: None,
        })
    }

    pub fn pending(&self) -> Option<&Request> {
        self.pending.as_ref()
    }

    /// Seal a just-created intent to measured callback completion. Crate-private:
    /// callers cannot move the causal boundary backwards or replace an intent.
    pub(crate) fn seal_result(
        &mut self,
        expected: &Request,
        result_ms: i64,
    ) -> Result<Request, Error> {
        if self.pending.as_ref() != Some(expected) || result_ms < expected.decision_at_ms {
            return Err(Error::Clock);
        }
        let mut sealed = expected.clone();
        sealed.decision_at_ms = result_ms;
        self.pending = Some(sealed.clone());
        Ok(sealed)
    }
    pub fn position_lots(&self) -> i64 {
        self.position_lots
    }
    pub fn average_price_ticks(&self) -> Option<i64> {
        self.average_price_ticks
    }

    pub fn submit(&mut self, request: Request) -> Result<(), Error> {
        if request.instrument != self.instrument
            || request.quantity_lots == 0
            || request.quantity_lots > self.max_lots
            || request.decision_at_ms < 0
        {
            return Err(Error::InvalidRequest);
        }
        if request.sequence <= self.last_sequence {
            return Err(Error::ReplayedSequence);
        }
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        if self
            .last_effect_at_ms
            .is_some_and(|t| request.decision_at_ms < t)
        {
            return Err(Error::Clock);
        }
        match request.action {
            Action::EnterLong | Action::EnterShort if self.position_lots != 0 => {
                return Err(Error::InvalidPositionTransition)
            }
            Action::Exit
                if self.position_lots == 0
                    || self.position_lots.unsigned_abs() != u64::from(request.quantity_lots) =>
            {
                return Err(Error::InvalidPositionTransition)
            }
            _ => {}
        }
        self.last_sequence = request.sequence;
        self.pending = Some(request);
        Ok(())
    }

    /// None means the quote predates the intent. No effect, no fabricated fill.
    /// Caller must submit eligible quotes in source order; a gap/conflict is HOLD.
    pub fn observe(&mut self, quote: Quote, now_ms: i64) -> Result<Option<Fill>, Error> {
        let request = self.pending.as_ref().ok_or(Error::NoPending)?;
        if quote.instrument != self.instrument
            || !valid_id(&quote.source_id)
            || quote.price_ticks <= 0
            || quote.event_at_ms < 0
        {
            return Err(Error::InvalidQuote);
        }
        if quote.received_at_ms < quote.event_at_ms || now_ms < quote.received_at_ms {
            return Err(Error::Clock);
        }
        if quote.event_at_ms <= request.decision_at_ms {
            return Ok(None);
        }
        if now_ms - quote.event_at_ms > self.max_quote_age_ms {
            return Err(Error::Clock);
        }
        let position_lots = match request.action {
            Action::EnterLong => i64::from(request.quantity_lots),
            Action::EnterShort => -i64::from(request.quantity_lots),
            Action::Exit => 0,
        };
        let fill = Fill {
            request: request.clone(),
            quote,
            applied_at_ms: now_ms,
            position_lots,
            average_price_ticks: None,
            policy: POLICY.into(),
        };
        let fill = Fill {
            average_price_ticks: if position_lots == 0 {
                None
            } else {
                Some(fill.quote.price_ticks)
            },
            ..fill
        };
        self.position_lots = fill.position_lots;
        self.average_price_ticks = fill.average_price_ticks;
        self.last_effect_at_ms = Some(now_ms);
        self.pending = None;
        Ok(Some(fill))
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:@".contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn book() -> VirtualBook {
        VirtualBook::new("IMOEXF".into(), 1, 100).unwrap()
    }
    fn request(sequence: u64, action: Action, decision_at_ms: i64) -> Request {
        Request {
            sequence,
            instrument: "IMOEXF".into(),
            action,
            quantity_lots: 1,
            decision_at_ms,
        }
    }
    fn quote(event_at_ms: i64) -> Quote {
        Quote {
            instrument: "IMOEXF".into(),
            source_id: format!("q-{event_at_ms}"),
            price_ticks: 4401,
            event_at_ms,
            received_at_ms: event_at_ms + 1,
        }
    }
    #[test]
    fn entry_pending_feedback_exit_and_one_effect() {
        let mut b = book();
        b.submit(request(1, Action::EnterLong, 100)).unwrap();
        assert_eq!(b.position_lots(), 0);
        assert!(b.observe(quote(100), 102).unwrap().is_none());
        let fill = b.observe(quote(101), 103).unwrap().unwrap();
        assert_eq!(
            (fill.position_lots, fill.average_price_ticks),
            (1, Some(4401))
        );
        assert_eq!(b.observe(quote(101), 103), Err(Error::NoPending));
        assert_eq!(
            b.submit(request(1, Action::EnterLong, 104)),
            Err(Error::ReplayedSequence)
        );
        assert_eq!(
            b.submit(request(2, Action::EnterShort, 104)),
            Err(Error::InvalidPositionTransition)
        );
        b.submit(request(2, Action::Exit, 104)).unwrap();
        assert_eq!(b.position_lots(), 1); // An exit intent alone is not flat.
        assert_eq!(
            b.observe(quote(105), 107).unwrap().unwrap().position_lots,
            0
        );
        assert_eq!(b.average_price_ticks(), None);
    }
    #[test]
    fn short_fill_and_unresolved_eod_do_not_fake_flat() {
        let mut b = book();
        b.submit(request(1, Action::EnterShort, 100)).unwrap();
        b.observe(quote(101), 103).unwrap().unwrap();
        b.submit(request(2, Action::Exit, 104)).unwrap();
        assert_eq!(b.position_lots(), -1);
        assert!(b.observe(quote(102), 105).unwrap().is_none());
        assert!(b.pending().is_some());
    }
    #[test]
    fn wrong_instrument_future_and_stale_quotes_keep_pending() {
        let mut b = book();
        b.submit(request(1, Action::EnterLong, 100)).unwrap();
        let mut q = quote(101);
        q.instrument = "OTHER".into();
        assert_eq!(b.observe(q, 103), Err(Error::InvalidQuote));
        assert_eq!(b.observe(quote(101), 101), Err(Error::Clock));
        assert_eq!(b.observe(quote(101), 202), Err(Error::Clock));
        assert_eq!(b.position_lots(), 0);
        assert_eq!(b.pending().unwrap().sequence, 1);
    }
    #[test]
    fn bounded_requests_no_reversal_or_implicit_rounding() {
        let mut b = book();
        assert_eq!(
            b.submit(request(1, Action::Exit, 100)),
            Err(Error::InvalidPositionTransition)
        );
        let mut r = request(1, Action::EnterLong, 100);
        r.quantity_lots = 2;
        assert_eq!(b.submit(r), Err(Error::InvalidRequest));
        b.submit(request(1, Action::EnterLong, 100)).unwrap();
        assert_eq!(
            b.submit(request(2, Action::EnterLong, 101)),
            Err(Error::Busy)
        );
        b.observe(quote(101), 103).unwrap().unwrap();
        assert_eq!(b.submit(request(2, Action::Exit, 102)), Err(Error::Clock));
    }
    #[test]
    fn policy_is_explicit_and_repeatable() {
        assert_eq!(policy_fingerprint().len(), 64);
        assert_eq!(policy_fingerprint(), policy_fingerprint());
        assert!(!POLICY.contains("current-close"));
    }

    #[test]
    fn measured_result_excludes_quotes_arriving_during_callback() {
        let mut b = book();
        let r = request(1, Action::EnterLong, 100);
        b.submit(r.clone()).unwrap();
        let sealed = b.seal_result(&r, 110).unwrap();
        assert_eq!(sealed.decision_at_ms, 110);
        assert!(b.observe(quote(105), 112).unwrap().is_none());
        assert!(b.observe(quote(110), 112).unwrap().is_none());
        let fill = b.observe(quote(111), 113).unwrap().unwrap();
        assert_eq!(fill.request, sealed);
        assert_eq!(fill.position_lots, 1);
    }

    #[test]
    fn result_seal_rejects_backwards_clock_and_wrong_intent_without_mutation() {
        let mut b = book();
        let r = request(1, Action::EnterLong, 100);
        b.submit(r.clone()).unwrap();
        assert_eq!(b.seal_result(&r, 99), Err(Error::Clock));
        assert_eq!(
            b.seal_result(&request(2, Action::EnterLong, 100), 110),
            Err(Error::Clock)
        );
        assert_eq!(b.pending(), Some(&r));
    }
}
