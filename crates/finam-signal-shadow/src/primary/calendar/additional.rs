//! USD/RI calendar admission uses the existing window normalization/freshness
//! rules, but never calls or widens the IMOEXF mapping/seed entry.
use super::*;
use broker_finam::ShadowInstrument;

pub(crate) fn admit_additional(
    instrument: ShadowInstrument,
    binding: CalendarBinding,
    model: ModelDay,
    raw: RawCalendar,
    mapping: &[u8],
    now: i64,
) -> Result<AdmittedCalendar, CalendarError> {
    if instrument == ShadowInstrument::Imoexf
        || binding.schema_version != 1
        || model.schema_version != 1
        || raw.schema_version != 1
        || raw.snapshots.is_empty()
        || raw.snapshots.len() > 16
    {
        return Err(CalendarError::Binding);
    }
    let day = NaiveDate::parse_from_str(&model.session_date, "%Y-%m-%d")
        .map_err(|_| CalendarError::Clock)?;
    let open = (day.and_hms_opt(7, 0, 0).ok_or(CalendarError::Clock)? - chrono::Duration::hours(3))
        .and_utc()
        .timestamp_millis();
    let required: Vec<_> = (0..101).map(|i| open + i * 600000).collect();
    if day.weekday().num_days_from_monday() >= 5
        || model.session_date == "2026-11-04"
        || model.required_m10_labels_ms != required
        || binding.next_label_ms != open
        || binding.start_not_before_ms < open
        || binding.start_not_before_ms >= binding.start_before_ms
        || binding.start_before_ms > open + 540000
        || now >= binding.start_before_ms
        || binding.eod_check_at_ms != open + 101 * 600000 + 9900
    {
        return Err(CalendarError::ModelInventory);
    }
    let previous = NaiveDate::parse_from_str(&raw.previous_session_date, "%Y-%m-%d")
        .map_err(|_| CalendarError::PreviousSession)?;
    if previous >= day
        || day.signed_duration_since(previous).num_days() > 7
        || binding.bootstrap_reference_start_ms >= open
        || binding.bootstrap_reference_start_ms < open - 7 * 86400000
    {
        return Err(CalendarError::PreviousSession);
    }
    let model_bytes = serde_json::to_vec(&model).map_err(|_| CalendarError::Decode)?;
    let raw_bytes = serde_json::to_vec(&raw).map_err(|_| CalendarError::Decode)?;
    if binding.model_day_sha256 != digest(&model_bytes)
        || binding.raw_calendar_sha256 != digest(&raw_bytes)
        || mapping.len() > 65536
        || binding.config_sha256.len() != 64
    {
        return Err(CalendarError::Binding);
    }
    let mut snapshots = Vec::new();
    for source in &raw.snapshots {
        let expected = format!(
            "https://api.finam.ru/v1/assets/{}/schedule",
            instrument.venue_symbol()
        );
        let encoded = expected.replace('@', "%40");
        if source.method != "GET"
            || ![expected, encoded].contains(&source.endpoint)
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
        if schedule.symbol != instrument.venue_symbol()
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
            .flat_map(|(_, w)| w.iter().copied())
            .collect(),
    )?;
    let close = open + 101 * 600000;
    if !covers(&windows, open, close)
        || required.iter().any(|label| {
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
            w.class != Class::Closed && local_date(w.start).is_ok_and(|d| d > previous && d < day)
        })
        || binding.bootstrap_reference_start_ms % 60000 != 0
        || binding.bootstrap_reference_start_ms > previous_start
    {
        return Err(CalendarError::PreviousSession);
    }
    let reference_start_ms = binding.bootstrap_reference_start_ms;
    let exact_inputs = [
        String::from_utf8(model_bytes).map_err(|_| CalendarError::Decode)?,
        String::from_utf8(raw_bytes).map_err(|_| CalendarError::Decode)?,
        std::str::from_utf8(mapping)
            .map_err(|_| CalendarError::Decode)?
            .into(),
    ];
    let sha256 =
        digest(&serde_json::to_vec(&(&binding, &exact_inputs)).map_err(|_| CalendarError::Decode)?);
    let calendar = AdmittedCalendar {
        binding,
        model,
        raw,
        snapshots,
        reference_start_ms,
        sha256,
        admitted_at_ms: now,
        exact_inputs,
    };
    calendar.fresh(now)?;
    Ok(calendar)
}
