//! Independently pinned model inventory + retained raw FINAM schedule. Never
//! derive required M10 or the previous raw session from observed bar presence.
//! Local pins are trusted inputs, not provider authentication. R0 supports the
//! ordinary baseline07 day; unknown/short/contradictory calendars fail closed.
use super::WsPrimaryConfig;
use crate::{seed::digest, ValidatedConfig};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
#[cfg(feature = "models-bridge")]
mod additional;
#[cfg(feature = "models-bridge")]
pub(crate) use additional::admit_additional;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarBinding {
    pub schema_version: u32,
    pub config_sha256: String,
    pub model_day_sha256: String,
    pub raw_calendar_sha256: String,
    /// Explicit range fixed BEFORE WS: may include earlier seeded sessions when
    /// a quiet previous raw day cannot cover a 300-row bootstrap. Never narrow
    /// it to current WS timestamps or assume that "previous day" is enough.
    pub bootstrap_reference_start_ms: i64,
    pub next_label_ms: i64,
    pub start_not_before_ms: i64,
    pub start_before_ms: i64,
    pub eod_check_at_ms: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDay {
    pub schema_version: u32,
    pub session_date: String,
    pub required_m10_labels_ms: Vec<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCalendar {
    pub schema_version: u32,
    /// Independently selected actual raw-session date. May be a weekend even
    /// when the model's last warmup session is Friday. Not a "last N hours" hint.
    pub previous_session_date: String,
    pub snapshots: Vec<ScheduleSnapshot>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleSnapshot {
    pub method: String,
    pub endpoint: String,
    pub requested_at_ms: i64,
    pub received_at_ms: i64,
    pub status: u16,
    pub transport_complete: bool,
    pub declared_body_bytes: u64,
    pub response_sha256: String,
    pub raw_body: String,
}
pub struct CalendarInputs<'a> {
    pub model_day: &'a [u8],
    pub raw_calendar: &'a [u8],
    pub mapping: &'a [u8],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CalendarError {
    Binding,
    Bounds,
    Decode,
    Clock,
    ModelClosed,
    ModelInventory,
    RawCoverage,
    RawConflict,
    PreviousSession,
    Stale,
    Window,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Schedule {
    symbol: String,
    sessions: Vec<Session>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Session {
    #[serde(rename = "type")]
    kind: String,
    interval: Interval,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Interval {
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Trading,
    Auction,
    Closed,
}
#[derive(Debug, Clone, Copy)]
struct Window {
    start: i64,
    end: i64,
    class: Class,
}

/// No Deserialize/constructor outside full admission. It binds calendar facts,
/// not DATA readiness, source admission or authorization to open real orders.
/// ```compile_fail
/// use finam_signal_shadow::primary::calendar::AdmittedCalendar;
/// let forged: AdmittedCalendar = serde_json::from_str("{}").unwrap();
/// ```
pub struct AdmittedCalendar {
    binding: CalendarBinding,
    model: ModelDay,
    raw: RawCalendar,
    snapshots: Vec<(i64, Vec<Window>)>,
    reference_start_ms: i64,
    sha256: String,
    admitted_at_ms: i64,
    exact_inputs: [String; 3],
}
impl AdmittedCalendar {
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn required(&self) -> &[i64] {
        &self.model.required_m10_labels_ms
    }
    pub fn reference_start_ms(&self) -> i64 {
        self.reference_start_ms
    }
    pub fn last_label_ms(&self) -> i64 {
        *self.required().last().expect("admitted inventory")
    }
    pub fn close_ms(&self) -> i64 {
        self.last_label_ms() + 600000
    }
    pub fn eod_check_at_ms(&self) -> i64 {
        self.binding.eod_check_at_ms
    }
    pub(crate) fn matches(&self, config: &str, next: i64) -> bool {
        self.binding.config_sha256 == config && self.binding.next_label_ms == next
    }
    pub(crate) fn validate_entry(&self, now: i64) -> Result<(), CalendarError> {
        if now < self.admitted_at_ms {
            return Err(CalendarError::Clock);
        }
        if now < self.binding.start_not_before_ms || now >= self.binding.start_before_ms {
            return Err(CalendarError::Window);
        }
        self.fresh(now)
    }
    fn fresh(&self, now: i64) -> Result<(), CalendarError> {
        let windows: Vec<_> = self
            .snapshots
            .iter()
            .filter(|(received, _)| {
                now.checked_sub(*received)
                    .is_some_and(|age| (0..=86400000).contains(&age))
            })
            .flat_map(|(_, windows)| windows.iter().copied())
            .collect();
        let windows = normalize(windows)?;
        if !covers(&windows, self.required()[0], self.close_ms()) {
            return Err(CalendarError::Stale);
        }
        Ok(())
    }
    /// Market schedule facts only: no headers, account ID or credential fields.
    pub(crate) fn evidence(&self) -> serde_json::Value {
        serde_json::json!({"binding":self.binding,"model_day_raw_json":self.exact_inputs[0],
            "raw_calendar_raw_json":self.exact_inputs[1],"mapping_raw_json":self.exact_inputs[2],
            "calendar_sha256":self.sha256,"reference_start_ms":self.reference_start_ms,"admitted_at_ms":self.admitted_at_ms,
            "raw_snapshot_count":self.raw.snapshots.len(),
            "freshness_policy":"current-model-window-schedule-within-24h-at-entry;historical-reference-times-unchanged",
            "history_proves_live_readiness":false})
    }
}

pub fn admit(
    config: &WsPrimaryConfig,
    binding: CalendarBinding,
    inputs: CalendarInputs<'_>,
    now: i64,
) -> Result<AdmittedCalendar, CalendarError> {
    let validated = ValidatedConfig::from_ws_primary(config).map_err(|_| CalendarError::Binding)?;
    let c = validated.config();
    if binding.schema_version != 1
        || binding.config_sha256 != validated.fingerprint()
        || binding.model_day_sha256 != digest(inputs.model_day)
        || binding.raw_calendar_sha256 != digest(inputs.raw_calendar)
        || c.instrument_mapping_sha256 != digest(inputs.mapping)
    {
        return Err(CalendarError::Binding);
    }
    let total = inputs
        .model_day
        .len()
        .checked_add(inputs.raw_calendar.len())
        .and_then(|n| n.checked_add(inputs.mapping.len()))
        .ok_or(CalendarError::Bounds)?;
    if total as u64 > c.max_input_bytes.min(512 * 1024)
        || inputs.model_day.len() > 16384
        || inputs.mapping.len() > 16384
        || now < 0
    {
        return Err(CalendarError::Bounds);
    }
    let mapping: crate::seed::InstrumentMapping = decode(inputs.mapping)?;
    if mapping.instrument.symbol != "IMOEXF"
        || mapping.instrument.venue_symbol.as_deref() != Some("IMOEXF@RTSX")
        || mapping.instrument.exchange != broker_core::Exchange::Moex
        || mapping.instrument.market != broker_core::Market::Futures
        || mapping.tick_size != "0.5"
        || mapping.lot_size != 1
    {
        return Err(CalendarError::Binding);
    }
    let model: ModelDay = decode(inputs.model_day)?;
    let date = NaiveDate::parse_from_str(&model.session_date, "%Y-%m-%d")
        .map_err(|_| CalendarError::Decode)?;
    let open = (date
        .and_hms_opt(7, 0, 0)
        .ok_or(CalendarError::Clock)?
        .and_utc()
        - chrono::Duration::hours(3))
    .timestamp_millis();
    let close = open.checked_add(101 * 600000).ok_or(CalendarError::Clock)?;
    if date.weekday().number_from_monday() > 5 || model.required_m10_labels_ms.is_empty() {
        return Err(CalendarError::ModelClosed);
    }
    if model.schema_version != 1
        || model.required_m10_labels_ms.len() != 101
        || open != c.observation_start_utc_ms
        || model
            .required_m10_labels_ms
            .iter()
            .enumerate()
            .any(|(i, label)| *label != open + i as i64 * 600000)
    {
        return Err(CalendarError::ModelInventory);
    }
    if !model
        .required_m10_labels_ms
        .contains(&binding.next_label_ms)
        || binding.start_not_before_ms < binding.next_label_ms
        || binding.start_not_before_ms >= binding.start_before_ms
        || binding.start_before_ms > binding.next_label_ms + 600000
        || binding.start_before_ms > close
        || binding.eod_check_at_ms <= close
        || binding.eod_check_at_ms >= c.run_deadline_utc_ms
        || binding.eod_check_at_ms - close + c.clock_uncertainty_ms as i64 > c.lmax_ms as i64
        || now >= binding.eod_check_at_ms
    {
        return Err(CalendarError::Window);
    }
    let raw: RawCalendar = decode(inputs.raw_calendar)?;
    let previous = NaiveDate::parse_from_str(&raw.previous_session_date, "%Y-%m-%d")
        .map_err(|_| CalendarError::Decode)?;
    if raw.schema_version != 1 || raw.snapshots.is_empty() || raw.snapshots.len() > 8 {
        return Err(CalendarError::Bounds);
    }
    let days = (date - previous).num_days();
    let last_model_day = NaiveDate::parse_from_str(&c.warmup_session_dates[3], "%Y-%m-%d")
        .map_err(|_| CalendarError::Decode)?;
    if !(1..=14).contains(&days) || previous < last_model_day {
        return Err(CalendarError::PreviousSession);
    }
    let mut snapshots = Vec::new();
    for source in &raw.snapshots {
        if source.method != "GET"
            || !matches!(
                source.endpoint.as_str(),
                "https://api.finam.ru/v1/assets/IMOEXF@RTSX/schedule"
                    | "https://api.finam.ru/v1/assets/IMOEXF%40RTSX/schedule"
            )
            || source.status != 200
            || !source.transport_complete
            || source.requested_at_ms < 0
            || source.received_at_ms < source.requested_at_ms
            || source.received_at_ms > now
            || source.received_at_ms - source.requested_at_ms > 5000
            || source.declared_body_bytes != source.raw_body.len() as u64
            || source.raw_body.len() > 65536
            || digest(source.raw_body.as_bytes()) != source.response_sha256
        {
            return Err(CalendarError::Binding);
        }
        let schedule: Schedule = decode(source.raw_body.as_bytes())?;
        if schedule.symbol != "IMOEXF@RTSX"
            || schedule.sessions.is_empty()
            || schedule.sessions.len() > 512
        {
            return Err(CalendarError::RawCoverage);
        }
        let mut windows = Vec::new();
        for session in schedule.sessions {
            let class = match session.kind.as_str() {
                "EARLY_TRADING" | "CORE_TRADING" | "LATE_TRADING" => Class::Trading,
                "OPENING_AUCTION" | "CLOSING_AUCTION" => Class::Auction,
                "CLOSED" | "CLEARING" => Class::Closed,
                _ => return Err(CalendarError::Decode),
            };
            let start = session.interval.start_time;
            let end = session.interval.end_time;
            if start.timestamp() < 0
                || start >= end
                || start.timestamp_subsec_nanos() != 0
                || end.timestamp_subsec_nanos() != 0
                || (end - start).num_days() > 15
            {
                return Err(CalendarError::Clock);
            }
            if class != Class::Closed
                && local_date(start.timestamp_millis())? != local_date(end.timestamp_millis() - 1)?
            {
                return Err(CalendarError::RawConflict);
            }
            windows.push(Window {
                start: start.timestamp_millis(),
                end: end.timestamp_millis(),
                class,
            });
        }
        snapshots.push((source.received_at_ms, windows));
    }
    let windows = normalize(
        snapshots
            .iter()
            .flat_map(|(_, v)| v.iter().copied())
            .collect(),
    )?;
    if !covers(&windows, open, close)
        || model.required_m10_labels_ms.iter().any(|label| {
            !windows
                .iter()
                .any(|w| w.class == Class::Trading && w.start < *label + 600000 && w.end > *label)
        })
    {
        return Err(CalendarError::RawCoverage);
    }
    let prior: Vec<_> = windows
        .iter()
        .filter(|w| w.class != Class::Closed && local_date(w.start).ok() == Some(previous))
        .collect();
    let previous_start = prior
        .iter()
        .map(|w| w.start)
        .min()
        .ok_or(CalendarError::PreviousSession)?;
    let previous_end = prior
        .iter()
        .map(|w| w.end)
        .max()
        .ok_or(CalendarError::PreviousSession)?;
    if !covers(&windows, previous_end, open)
        || windows.iter().any(|w| {
            w.class != Class::Closed && local_date(w.start).is_ok_and(|d| d > previous && d < date)
        })
    {
        return Err(CalendarError::PreviousSession);
    }
    if binding.bootstrap_reference_start_ms < c.warmup_range_start_utc_ms
        || binding.bootstrap_reference_start_ms % 60000 != 0
        || binding.bootstrap_reference_start_ms > previous_start.div_euclid(60000) * 60000
    {
        return Err(CalendarError::PreviousSession);
    }
    let reference_start_ms = binding.bootstrap_reference_start_ms;
    let sha256 = digest(
        &serde_json::to_vec(&(&binding, &c.instrument_mapping_sha256))
            .map_err(|_| CalendarError::Binding)?,
    );
    let admitted = AdmittedCalendar {
        binding,
        model,
        raw,
        snapshots,
        reference_start_ms,
        sha256,
        admitted_at_ms: now,
        exact_inputs: [inputs.model_day, inputs.raw_calendar, inputs.mapping]
            .map(|b| String::from_utf8(b.to_vec()).expect("validated UTF8")),
    };
    admitted.fresh(now)?;
    Ok(admitted)
}
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, CalendarError> {
    let text = std::str::from_utf8(bytes).map_err(|_| CalendarError::Decode)?;
    serde_json::from_value(super::json::parse(text).map_err(|_| CalendarError::Decode)?)
        .map_err(|_| CalendarError::Decode)
}
fn local_date(ms: i64) -> Result<NaiveDate, CalendarError> {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .and_then(|v| v.checked_add_signed(chrono::Duration::hours(3)))
        .map(|v| v.date_naive())
        .ok_or(CalendarError::Clock)
}
fn normalize(mut windows: Vec<Window>) -> Result<Vec<Window>, CalendarError> {
    windows.sort_by_key(|w| (w.start, w.end, w.class));
    let mut out: Vec<Window> = Vec::new();
    for w in windows {
        if let Some(last) = out.last_mut() {
            if w.start < last.end && w.class != last.class {
                return Err(CalendarError::RawConflict);
            }
            if w.start <= last.end && w.class == last.class {
                last.end = last.end.max(w.end);
                continue;
            }
        }
        out.push(w);
    }
    Ok(out)
}
fn covers(windows: &[Window], start: i64, end: i64) -> bool {
    if start > end {
        return false;
    }
    if start == end {
        return true;
    }
    let mut at = start;
    for w in windows {
        if w.end <= at {
            continue;
        }
        if w.start > at {
            return false;
        }
        at = at.max(w.end);
        if at >= end {
            return true;
        }
    }
    at >= end
}

#[cfg(test)]
pub(crate) mod tests;
