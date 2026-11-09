//! Volume is non-decision metadata only for the explicit BO-only delivery policy.
//! Source bytes, canonical inputs and model state are never rewritten by audit.
use super::{wire::same_values, PrimaryError};
use broker_core::event::Bar;
use serde::Serialize;

pub(crate) fn same_prices(a: &Bar, b: &Bar) -> bool {
    a.instrument == b.instrument
        && a.timeframe_sec == b.timeframe_sec
        && a.open_ts == b.open_ts
        && a.close_ts == b.close_ts
        && (a.open, a.high, a.low, a.close) == (b.open, b.high, b.low, b.close)
}

pub(crate) fn compatible(a: &Bar, b: &Bar, observed_bo: bool) -> bool {
    same_values(a, b) || (observed_bo && same_prices(a, b))
}

#[derive(Debug, Serialize)]
pub(crate) struct SourceChange {
    pub context: &'static str,
    pub kind: &'static str,
    pub minute_ms: i64,
    pub expected: Option<Bar>,
    pub observed: Option<Bar>,
    pub observed_source_sha256: Option<String>,
    pub model_input_rewritten: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_ws_version: Option<WsVersionWitness>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WsVersionWitness {
    pub generation: u64,
    pub first_received_at_ms: i64,
    pub last_changed_at_ms: i64,
    pub frame_sha256: String,
}

impl super::PrimaryProcessor {
    pub(crate) fn pending_volume_revision(&self) -> bool {
        self.source_changes.iter().any(|c| c.kind == "VOLUME_ONLY")
    }
    pub(crate) fn take_source_changes(&mut self) -> Vec<SourceChange> {
        std::mem::take(&mut self.source_changes)
    }

    pub(super) fn note_change(
        &mut self,
        context: &'static str,
        expected: Option<&Bar>,
        observed: Option<&Bar>,
        source: Option<&str>,
    ) -> Result<(), PrimaryError> {
        if expected
            .zip(observed)
            .is_some_and(|(a, b)| same_values(a, b))
        {
            return Ok(());
        }
        if self.source_changes.len() >= self.policy.max_cache_m1 {
            return self.hold(PrimaryError::CacheBudget);
        }
        let volume_only = expected
            .zip(observed)
            .is_some_and(|(a, b)| same_prices(a, b));
        let expected_ws_version = if context == "REST_PROOF" {
            expected.and_then(|bar| self.rest_proof_ws_witness(bar))
        } else {
            None
        };
        self.source_changes.push(SourceChange {
            context,
            kind: if volume_only {
                "VOLUME_ONLY"
            } else {
                "PRICE_OR_INVENTORY_DIFFERENCE"
            },
            minute_ms: expected
                .or(observed)
                .ok_or(PrimaryError::Numeric)?
                .open_ts
                .timestamp_millis(),
            expected: expected.cloned(),
            observed: observed.cloned(),
            observed_source_sha256: source.map(str::to_owned),
            model_input_rewritten: false,
            expected_ws_version,
        });
        Ok(())
    }
}
