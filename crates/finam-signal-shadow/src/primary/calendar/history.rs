//! Calendar proof for the complete historical query horizon, independent of
//! which M1 rows arrived. No weekday/weekend closure is inferred from silence.
use super::*;
use broker_finam::ShadowInstrument;

pub(crate) fn history_windows(
    instrument: ShadowInstrument,
    raw: &RawCalendar,
    start: i64,
    end: i64,
    now: i64,
) -> Result<Vec<(i64, i64)>, CalendarError> {
    if instrument == ShadowInstrument::Imoexf
        || raw.schema_version != 1
        || raw.snapshots.is_empty()
        || raw.snapshots.len() > 16
        || start < 0
        || start >= end
    {
        return Err(CalendarError::Bounds);
    }
    let mut windows = Vec::new();
    for source in &raw.snapshots {
        let endpoint = format!(
            "https://api.finam.ru/v1/assets/{}/schedule",
            instrument.venue_symbol()
        );
        if source.method != "GET"
            || ![endpoint.clone(), endpoint.replace('@', "%40")].contains(&source.endpoint)
            || source.status != 200
            || !source.transport_complete
            || source.requested_at_ms < 0
            || source.received_at_ms < source.requested_at_ms
            || source.received_at_ms > now
            || source.received_at_ms - source.requested_at_ms > 5000
            || source.raw_body.len() > 65536
            || source.declared_body_bytes != source.raw_body.len() as u64
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
        for session in schedule.sessions {
            let class = match session.kind.as_str() {
                "EARLY_TRADING" | "CORE_TRADING" | "LATE_TRADING" => Class::Trading,
                "OPENING_AUCTION" | "CLOSING_AUCTION" => Class::Auction,
                "CLOSED" | "CLEARING" => Class::Closed,
                _ => return Err(CalendarError::Decode),
            };
            let a = session.interval.start_time;
            let b = session.interval.end_time;
            if a.timestamp() < 0
                || a >= b
                || a.timestamp_subsec_nanos() != 0
                || b.timestamp_subsec_nanos() != 0
                || (b - a).num_days() > 15
            {
                return Err(CalendarError::Clock);
            }
            if class != Class::Closed
                && local_date(a.timestamp_millis())? != local_date(b.timestamp_millis() - 1)?
            {
                return Err(CalendarError::RawConflict);
            }
            windows.push(Window {
                start: a.timestamp_millis(),
                end: b.timestamp_millis(),
                class,
            });
        }
    }
    let windows = normalize(windows)?;
    if !covers(&windows, start, end) {
        return Err(CalendarError::RawCoverage);
    }
    Ok(windows
        .into_iter()
        .filter(|w| w.class != Class::Closed && w.start < end && w.end > start)
        .map(|w| (w.start.max(start), w.end.min(end)))
        .collect())
}
