use std::str::FromStr;

use broker_core::{instrument::InstrumentMapEntry, readiness::ReadinessReason};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::dto::{AssetParamsResponse, AssetResponse, AssetScheduleResponse, DecimalLike};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinamInstrumentRegistryBlock {
    UnknownSymbol,
    MissingMic,
    MissingPriceStep,
    PriceStepMismatch,
    MissingQtyStep,
    QtyStepMismatch,
    MissingLotSize,
    ExpiredContract,
    NotTradable,
    ScheduleMissing,
    SessionClosed,
    CurrencyMismatch,
    ManualOverrideRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamInstrumentRegistryValidation {
    pub instrument_map_validated: bool,
    pub schedule_loaded: bool,
    pub blocks: Vec<FinamInstrumentRegistryBlock>,
    pub readiness_reasons: Vec<ReadinessReason>,
    pub fingerprint_sha256: String,
}

#[derive(Debug, Clone, Copy)]
pub struct FinamInstrumentRegistryValidator {
    pub require_open_session: bool,
}

impl Default for FinamInstrumentRegistryValidator {
    fn default() -> Self {
        Self {
            require_open_session: true,
        }
    }
}

impl FinamInstrumentRegistryValidator {
    pub fn validate(
        &self,
        entry: &InstrumentMapEntry,
        asset: &AssetResponse,
        params: &AssetParamsResponse,
        schedule: &AssetScheduleResponse,
        checked_at: DateTime<Utc>,
    ) -> FinamInstrumentRegistryValidation {
        let mut blocks = Vec::new();

        if asset_venue_symbol(asset).as_deref() != Some(entry.broker_symbol.0.as_str())
            || params.symbol != entry.broker_symbol.0
        {
            blocks.push(FinamInstrumentRegistryBlock::UnknownSymbol);
        }
        if schedule.symbol != entry.broker_symbol.0 {
            blocks.push(FinamInstrumentRegistryBlock::ScheduleMissing);
        }

        let expected_mic = entry.broker_symbol.0.split_once('@').map(|(_, mic)| mic);
        match (expected_mic, asset.mic.as_deref()) {
            (None, _) | (_, None) => blocks.push(FinamInstrumentRegistryBlock::MissingMic),
            (Some(expected), Some(actual)) if expected != actual => {
                blocks.push(FinamInstrumentRegistryBlock::MissingMic)
            }
            _ => {}
        }

        match asset_price_step(asset) {
            None => blocks.push(FinamInstrumentRegistryBlock::MissingPriceStep),
            Some(price_step) if price_step != entry.price_step => {
                blocks.push(FinamInstrumentRegistryBlock::PriceStepMismatch)
            }
            _ => {}
        }

        if entry.qty_step <= Decimal::ZERO {
            blocks.push(FinamInstrumentRegistryBlock::MissingQtyStep);
        }
        if entry.lot_size <= Decimal::ZERO {
            blocks.push(FinamInstrumentRegistryBlock::MissingLotSize);
        }
        match asset_lot_size(asset) {
            None => blocks.push(FinamInstrumentRegistryBlock::MissingLotSize),
            Some(lot_size) if lot_size != entry.lot_size => {
                blocks.push(FinamInstrumentRegistryBlock::QtyStepMismatch)
            }
            _ => {}
        }

        if !params
            .is_tradable
            .unwrap_or(params.tradeable.unwrap_or(false))
            || !entry.is_tradable
        {
            blocks.push(FinamInstrumentRegistryBlock::NotTradable);
        }

        match asset.quote_currency.as_deref() {
            Some(currency) if currency == entry.currency => {}
            _ => blocks.push(FinamInstrumentRegistryBlock::CurrencyMismatch),
        }

        if is_expired(entry.expiration_date, asset, checked_at.date_naive()) {
            blocks.push(FinamInstrumentRegistryBlock::ExpiredContract);
        }

        if schedule.sessions.is_empty() {
            blocks.push(FinamInstrumentRegistryBlock::ScheduleMissing);
        } else if self.require_open_session && !schedule_has_open_session(schedule, checked_at) {
            blocks.push(FinamInstrumentRegistryBlock::SessionClosed);
        }

        dedup_blocks(&mut blocks);
        let schedule_loaded = !blocks.contains(&FinamInstrumentRegistryBlock::ScheduleMissing);
        let instrument_map_validated = blocks.is_empty();
        let readiness_reasons =
            readiness_reasons_for_instrument_registry(instrument_map_validated, schedule_loaded);
        let fingerprint_sha256 = instrument_registry_fingerprint(entry, asset, params, schedule);

        FinamInstrumentRegistryValidation {
            instrument_map_validated,
            schedule_loaded,
            blocks,
            readiness_reasons,
            fingerprint_sha256,
        }
    }
}

fn readiness_reasons_for_instrument_registry(
    instrument_map_validated: bool,
    schedule_loaded: bool,
) -> Vec<ReadinessReason> {
    let mut reasons = Vec::new();
    if !instrument_map_validated {
        reasons.push(ReadinessReason::InstrumentMapNotValidated);
    }
    if !schedule_loaded {
        reasons.push(ReadinessReason::ScheduleNotLoaded);
    }
    reasons
}

fn asset_price_step(asset: &AssetResponse) -> Option<Decimal> {
    asset
        .future_details
        .as_ref()
        .and_then(|future| future.min_step.as_ref())
        .or(asset.min_step.as_ref())
        .and_then(decimal_like_value)
}

fn asset_venue_symbol(asset: &AssetResponse) -> Option<String> {
    Some(format!(
        "{}@{}",
        asset.ticker.as_deref()?,
        asset.mic.as_deref()?
    ))
}

fn asset_lot_size(asset: &AssetResponse) -> Option<Decimal> {
    asset
        .future_details
        .as_ref()
        .and_then(|future| future.lot_size.as_ref())
        .or(asset.lot_size.as_ref())
        .and_then(|value| Decimal::from_str(&value.value).ok())
}

fn decimal_like_value(value: &DecimalLike) -> Option<Decimal> {
    match value {
        DecimalLike::Value(value) => Decimal::from_str(&value.value).ok(),
        DecimalLike::String(value) => Decimal::from_str(value).ok(),
    }
}

fn is_expired(
    entry_expiration_date: Option<NaiveDate>,
    asset: &AssetResponse,
    checked_date: NaiveDate,
) -> bool {
    let asset_last_trade_date = asset
        .future_details
        .as_ref()
        .and_then(|future| future.last_trade_date.as_deref())
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok());
    entry_expiration_date
        .or(asset_last_trade_date)
        .is_some_and(|expiration| expiration < checked_date)
}

fn schedule_has_open_session(schedule: &AssetScheduleResponse, checked_at: DateTime<Utc>) -> bool {
    schedule.sessions.iter().any(|session| {
        let Some(interval) = &session.interval else {
            return false;
        };
        let Some(start_time) = interval.start_time.as_deref() else {
            return false;
        };
        let Some(end_time) = interval.end_time.as_deref() else {
            return false;
        };
        let Ok(start) = DateTime::parse_from_rfc3339(start_time) else {
            return false;
        };
        let Ok(end) = DateTime::parse_from_rfc3339(end_time) else {
            return false;
        };
        checked_at >= start.with_timezone(&Utc) && checked_at < end.with_timezone(&Utc)
    })
}

fn instrument_registry_fingerprint(
    entry: &InstrumentMapEntry,
    asset: &AssetResponse,
    params: &AssetParamsResponse,
    schedule: &AssetScheduleResponse,
) -> String {
    let payload = serde_json::json!({
        "internal_symbol_len": entry.internal_symbol.0.len(),
        "broker_symbol_len": entry.broker_symbol.0.len(),
        "price_step": entry.price_step.to_string(),
        "qty_step": entry.qty_step.to_string(),
        "lot_size": entry.lot_size.to_string(),
        "currency": entry.currency.as_str(),
        "schedule_id_len": entry.schedule_id.len(),
        "expiration_date": entry.expiration_date.map(|date| date.to_string()),
        "asset_symbol_len": asset_venue_symbol(asset).as_ref().map(|value| value.len()),
        "asset_mic_len": asset.mic.as_ref().map(|value| value.len()),
        "asset_quote_currency": asset.quote_currency.as_deref(),
        "params_symbol_len": params.symbol.len(),
        "schedule_symbol_len": schedule.symbol.len(),
        "schedule_session_count": schedule.sessions.len(),
    });
    crate::sha256_hex(payload.to_string().as_bytes())
}

fn dedup_blocks(blocks: &mut Vec<FinamInstrumentRegistryBlock>) {
    let mut deduped = Vec::new();
    for block in blocks.drain(..) {
        if !deduped.contains(&block) {
            deduped.push(block);
        }
    }
    *blocks = deduped;
}

#[cfg(test)]
mod tests {
    use broker_core::{
        broker::BrokerKind,
        instrument::{BrokerSymbol, Exchange, InternalSymbol, Market},
    };
    use chrono::TimeZone;

    use super::*;
    use crate::dto::{DecimalValue, FutureDetails, ScheduleSession, TimeInterval};

    fn entry() -> InstrumentMapEntry {
        InstrumentMapEntry {
            internal_symbol: InternalSymbol("TESTFUT".to_string()),
            broker: BrokerKind::Finam,
            broker_symbol: BrokerSymbol("TESTFUT@TEST".to_string()),
            exchange: Exchange::Other("TEST".to_string()),
            market: Market::Futures,
            price_step: Decimal::new(1, 2),
            qty_step: Decimal::ONE,
            lot_size: Decimal::ONE,
            min_qty: Decimal::ONE,
            step_value: Decimal::ONE,
            currency: "RUB".to_string(),
            schedule_id: "TEST_SCHEDULE".to_string(),
            expiration_date: Some(NaiveDate::from_ymd_opt(2026, 9, 17).expect("date")),
            is_tradable: true,
        }
    }

    fn asset() -> AssetResponse {
        AssetResponse {
            board: Some("TEST".to_string()),
            decimals: Some(2),
            future_details: Some(FutureDetails {
                contract_size: Some(DecimalValue {
                    value: "1".to_string(),
                }),
                expiration_date: Some("2026-09-17".to_string()),
                first_trade_date: Some("2026-06-01".to_string()),
                last_trade_date: Some("2026-09-17".to_string()),
                lot_size: Some(DecimalValue {
                    value: "1".to_string(),
                }),
                min_step: Some(DecimalLike::String("0.01".to_string())),
                step_price: Some(DecimalLike::String("1".to_string())),
            }),
            id: Some("ASSET_TEST".to_string()),
            isin: None,
            lot_size: Some(DecimalValue {
                value: "1".to_string(),
            }),
            mic: Some("TEST".to_string()),
            min_step: Some(DecimalLike::String("0.01".to_string())),
            name: Some("Synthetic test future".to_string()),
            quote_currency: Some("RUB".to_string()),
            ticker: Some("TESTFUT".to_string()),
            asset_type: Some("ASSET_TYPE_FUTURES".to_string()),
        }
    }

    fn params() -> AssetParamsResponse {
        AssetParamsResponse {
            account_id: Some("ACC_TEST_0001".to_string()),
            is_tradable: Some(true),
            long_collateral: None,
            long_initial_margin: None,
            long_risk_rate: None,
            longable: None,
            price_type: Some("PRICE_TYPE_PRICE".to_string()),
            short_collateral: None,
            short_initial_margin: None,
            short_risk_rate: None,
            shortable: None,
            symbol: "TESTFUT@TEST".to_string(),
            tradeable: Some(true),
        }
    }

    fn schedule() -> AssetScheduleResponse {
        AssetScheduleResponse {
            symbol: "TESTFUT@TEST".to_string(),
            sessions: vec![ScheduleSession {
                interval: Some(TimeInterval {
                    start_time: Some("2026-07-03T06:00:00Z".to_string()),
                    end_time: Some("2026-07-03T16:00:00Z".to_string()),
                }),
                session_type: Some("SESSION_TYPE_MAIN".to_string()),
            }],
        }
    }

    #[test]
    fn instrument_registry_validator_accepts_matching_reference_data() {
        let checked_at = Utc
            .with_ymd_and_hms(2026, 7, 3, 9, 10, 0)
            .single()
            .expect("timestamp");
        let validation = FinamInstrumentRegistryValidator::default().validate(
            &entry(),
            &asset(),
            &params(),
            &schedule(),
            checked_at,
        );

        assert!(validation.instrument_map_validated);
        assert!(validation.schedule_loaded);
        assert!(validation.blocks.is_empty());
        assert!(validation.readiness_reasons.is_empty());
        assert_eq!(validation.fingerprint_sha256.len(), 64);
    }

    #[test]
    fn instrument_registry_validator_blocks_tick_lot_schedule_and_tradability_drift() {
        let mut entry = entry();
        entry.price_step = Decimal::new(5, 2);
        entry.lot_size = Decimal::new(10, 0);
        entry.currency = "USD".to_string();
        let mut params = params();
        params.is_tradable = Some(false);
        let empty_schedule = AssetScheduleResponse {
            symbol: "TESTFUT@TEST".to_string(),
            sessions: Vec::new(),
        };
        let checked_at = Utc
            .with_ymd_and_hms(2026, 7, 3, 9, 10, 0)
            .single()
            .expect("timestamp");

        let validation = FinamInstrumentRegistryValidator::default().validate(
            &entry,
            &asset(),
            &params,
            &empty_schedule,
            checked_at,
        );

        assert!(!validation.instrument_map_validated);
        assert!(!validation.schedule_loaded);
        assert!(validation
            .blocks
            .contains(&FinamInstrumentRegistryBlock::PriceStepMismatch));
        assert!(validation
            .blocks
            .contains(&FinamInstrumentRegistryBlock::QtyStepMismatch));
        assert!(validation
            .blocks
            .contains(&FinamInstrumentRegistryBlock::NotTradable));
        assert!(validation
            .blocks
            .contains(&FinamInstrumentRegistryBlock::ScheduleMissing));
        assert!(validation
            .blocks
            .contains(&FinamInstrumentRegistryBlock::CurrencyMismatch));
        assert!(validation
            .readiness_reasons
            .contains(&ReadinessReason::InstrumentMapNotValidated));
        assert!(validation
            .readiness_reasons
            .contains(&ReadinessReason::ScheduleNotLoaded));
    }

    #[test]
    fn instrument_registry_validator_blocks_closed_session_and_expired_contract() {
        let mut entry = entry();
        entry.expiration_date = Some(NaiveDate::from_ymd_opt(2026, 6, 1).expect("date"));
        let checked_at = Utc
            .with_ymd_and_hms(2026, 7, 3, 20, 0, 0)
            .single()
            .expect("timestamp");

        let validation = FinamInstrumentRegistryValidator::default().validate(
            &entry,
            &asset(),
            &params(),
            &schedule(),
            checked_at,
        );

        assert!(!validation.instrument_map_validated);
        assert!(validation
            .blocks
            .contains(&FinamInstrumentRegistryBlock::ExpiredContract));
        assert!(validation
            .blocks
            .contains(&FinamInstrumentRegistryBlock::SessionClosed));
        assert!(validation
            .readiness_reasons
            .contains(&ReadinessReason::InstrumentMapNotValidated));
    }
}
