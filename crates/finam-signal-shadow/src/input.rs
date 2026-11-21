//! No IO. Consumes existing closed-REST admission, never provisional WS bars.
use broker_core::{event::Bar, HybridRuntimeBarEvent, HybridRuntimeBarOrigin, InstrumentId};
use broker_finam::sparse_m10::AdmittedClosedM1Snapshot;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

/// Model data identity, independent of replay/live delivery mode.
pub fn canonical_digest(bar: &HybridRuntimeBarEvent) -> Result<String, InputError> {
    let bytes = serde_json::to_vec(&(
        &bar.instrument,
        bar.close_time_utc,
        bar.timeframe_sec,
        bar.is_final,
        bar.open,
        bar.high,
        bar.low,
        bar.close,
        bar.volume,
    ))
    .map_err(|_| InputError::Conversion)?;
    Ok(crate::seed::digest(&bytes))
}

#[derive(Debug, PartialEq, Eq)]
pub enum InputError {
    Bounds,
    RequiredRange,
    Identity,
    Conflict,
    Conversion,
}

#[derive(Debug)]
pub struct ClosedInput {
    pub bar: HybridRuntimeBarEvent,
    pub snapshot_sha256: String,
    pub minute_bitmap: u16,
}

/// Bounded segment-local overlap inventory. No eviction, authority or live READY.
pub struct ClosedInputAdapter {
    instrument: InstrumentId,
    limit: usize,
    seen: BTreeMap<DateTime<Utc>, Vec<Bar>>,
    held: bool,
}

impl ClosedInputAdapter {
    pub fn new(instrument: InstrumentId, limit: usize) -> Result<Self, InputError> {
        if limit == 0 || limit > 4096 {
            return Err(InputError::Bounds);
        }
        Ok(Self {
            instrument,
            limit,
            seen: BTreeMap::new(),
            held: false,
        })
    }

    /// Required labels must come from the caller's independently fixed plan.
    /// Successful conversion is BACKFILL_REPLAY, never an implicit live promotion.
    /// All labels validate before overlap state advances.
    pub fn prepare(
        &mut self,
        snapshot: &AdmittedClosedM1Snapshot,
        required: &[DateTime<Utc>],
    ) -> Result<Vec<ClosedInput>, InputError> {
        if self.held {
            return Err(InputError::RequiredRange);
        }
        let result = self.checked(snapshot, required);
        if result.is_err() {
            self.held = true;
        }
        result
    }

    fn checked(
        &mut self,
        snapshot: &AdmittedClosedM1Snapshot,
        required: &[DateTime<Utc>],
    ) -> Result<Vec<ClosedInput>, InputError> {
        if required.is_empty()
            || required.len() > self.limit
            || required.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(InputError::Bounds);
        }
        let mut pending = Vec::new();
        let mut output = Vec::new();
        for &label in required {
            let bucket = snapshot
                .bucket(label)
                .map_err(|_| InputError::RequiredRange)?;
            let source = bucket.bar();
            if source.instrument != self.instrument {
                return Err(InputError::Identity);
            }
            if let Some(old) = self.seen.get(&label) {
                if old.as_slice() != bucket.actual_m1() {
                    return Err(InputError::Conflict);
                }
                continue;
            }
            // An unrecognized older label cannot be silently replayed after advancement.
            if self
                .seen
                .last_key_value()
                .is_some_and(|(last, _)| label <= *last)
            {
                return Err(InputError::RequiredRange);
            }
            let number = |s: String| -> Result<f64, InputError> {
                let n: f64 = s.parse().map_err(|_| InputError::Conversion)?;
                if !n.is_finite() {
                    return Err(InputError::Conversion);
                }
                Ok(n)
            };
            output.push(ClosedInput {
                bar: HybridRuntimeBarEvent {
                    instrument: source.instrument.clone(),
                    close_time_utc: source.open_ts.timestamp(),
                    open: number(source.open.to_string())?,
                    high: number(source.high.to_string())?,
                    low: number(source.low.to_string())?,
                    close: number(source.close.to_string())?,
                    volume: number(source.volume.to_string())?,
                    origin: HybridRuntimeBarOrigin::Replay,
                    timeframe_sec: 600,
                    is_final: true,
                },
                snapshot_sha256: bucket.snapshot_sha256().into(),
                minute_bitmap: bucket.minute_bitmap(),
            });
            pending.push((label, bucket.actual_m1().to_vec()));
        }
        if self.seen.len() + pending.len() > self.limit {
            return Err(InputError::Bounds);
        }
        self.seen.extend(pending);
        Ok(output)
    }
}
