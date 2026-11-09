use super::*;
use crate::{seed::*, Reference, RestartMode, ShadowConfig};
use chrono::{Datelike, Timelike};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use std::str::FromStr;

pub(super) const CSV: &[u8] =
    include_bytes!("../../../../../fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv");
pub(super) const ORACLE: &[u8] =
    include_bytes!("../../../../../fixtures/signal-shadow-state-parity/python-steps.json");
pub(super) const CSV_SHA: &str = "e4a30fb81a0abf5711a3702d19b918cad681820380f24422f6ba476b74f92cec";
pub(super) const ORACLE_SHA: &str =
    "eea5e55a289ce4d6dcc998cafe8dcfcbdc77f9a6442edec0f0d12836ae31f4aa";
#[derive(Clone)]
pub(super) struct GoldBar {
    pub bar: broker_core::HybridRuntimeBarEvent,
    pub values: [Decimal; 5],
}
impl GoldBar {
    pub fn label(&self) -> i64 {
        self.bar.close_time_utc * 1000
    }
    pub fn minute(&self, i: i64) -> Value {
        let [o, h, l, c, v] = self.values;
        let values = if i == 0 {
            [o, o, o, o, Decimal::ZERO]
        } else if i == 1 {
            [o, h, l, c, v]
        } else {
            [c, c, c, c, Decimal::ZERO]
        };
        json!({"timestamp":at(self.label()+i*60000).to_rfc3339(),"isDataSnapshot":false,
            "open":{"value":values[0].normalize().to_string()},"high":{"value":values[1].normalize().to_string()},
            "low":{"value":values[2].normalize().to_string()},"close":{"value":values[3].normalize().to_string()},"volume":{"value":values[4].normalize().to_string()}})
    }
    pub fn local_label(&self) -> String {
        (at(self.label()) + chrono::Duration::hours(3))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string()
    }
    pub fn rows(&self, sparse: bool) -> Vec<Value> {
        (0..10)
            .filter(|i| !sparse || *i != 4)
            .map(|i| self.minute(i))
            .collect()
    }
}
pub(super) fn instrument() -> broker_core::InstrumentId {
    broker_core::InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: broker_core::Exchange::Moex,
        market: broker_core::Market::Futures,
    }
}
pub(super) fn at(ms: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_millis(ms).unwrap()
}
pub(super) struct Fixture {
    pub config: WsPrimaryConfig,
    pub material: [Vec<u8>; 4],
    pub selected: Vec<GoldBar>,
    pub oracle: Value,
}
impl Fixture {
    pub fn new(mode: Mode) -> Self {
        assert_eq!(digest(CSV), CSV_SHA);
        assert_eq!(digest(ORACLE), ORACLE_SHA);
        let all: Vec<_> = csv::Reader::from_reader(CSV)
            .records()
            .map(|r| {
                let r = r.unwrap();
                let values: [Decimal; 5] = (3..8)
                    .map(|i| Decimal::from_str(&r[i]).unwrap())
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap();
                GoldBar {
                    bar: broker_core::HybridRuntimeBarEvent {
                        instrument: instrument(),
                        close_time_utc: chrono::DateTime::parse_from_str(
                            &r[1],
                            "%Y-%m-%d %H:%M:%S%:z",
                        )
                        .unwrap()
                        .timestamp(),
                        open: values[0].to_f64().unwrap(),
                        high: values[1].to_f64().unwrap(),
                        low: values[2].to_f64().unwrap(),
                        close: values[3].to_f64().unwrap(),
                        volume: values[4].to_f64().unwrap(),
                        origin: broker_core::HybridRuntimeBarOrigin::Live,
                        is_final: true,
                        timeframe_sec: 600,
                    },
                    values,
                }
            })
            .collect();
        let local = |b: &GoldBar| at(b.label()) + chrono::Duration::hours(3);
        let dates = ["2026-09-15", "2026-09-16", "2026-09-17", "2026-09-18"];
        let history: Vec<_> = all
            .iter()
            .filter(|b| {
                dates.contains(&local(b).date_naive().to_string().as_str()) && local(b).hour() >= 7
            })
            .map(|b| {
                let mut bar = b.bar.clone();
                bar.origin = broker_core::HybridRuntimeBarOrigin::History;
                bar
            })
            .collect();
        assert_eq!(history.len(), 404);
        let mapping = serde_json::to_vec(&InstrumentMapping {
            instrument: instrument(),
            tick_size: "0.5".into(),
            lot_size: 1,
        })
        .unwrap();
        let calendar = serde_json::to_vec(&CalendarInventory {
            sessions: dates.map(|date| SessionInventory {
                date: date.into(),
                required_m10_labels_utc: history
                    .iter()
                    .filter(|b| {
                        (at(b.close_time_utc * 1000) + chrono::Duration::hours(3))
                            .date_naive()
                            .to_string()
                            == date
                    })
                    .map(|b| b.close_time_utc)
                    .collect(),
            }),
        })
        .unwrap();
        let seed = serde_json::to_vec(&history).unwrap();
        let start = at(chrono::DateTime::parse_from_rfc3339("2026-09-21T04:00:00Z")
            .unwrap()
            .timestamp_millis())
        .timestamp_millis();
        let config = WsPrimaryConfig {
            schema_version: 2,
            market_data_mode: "ws_primary_v1".into(),
            source_policy: WS_PRIMARY_POLICY.into(),
            primary: PrimaryPolicy {
                audit_every_commits: None,
                ..Default::default()
            },
            base: ShadowConfig {
                schema_version: 1,
                reference: Reference::AlorFreeze20260927,
                restart_mode: RestartMode::NewSegment,
                model_profile: "imoexf-baseline07-bo-only-no-riskgate".into(),
                model_sha256: strategy_runtime_core::HybridIntradayRuntimeStrategy::new(
                    crate::profile::baseline07_config().unwrap(),
                )
                .stage5c_config_fingerprint(),
                reference_sha256: ORACLE_SHA.into(),
                feedback_policy_sha256: crate::feedback::candle_policy_fingerprint(),
                instrument_mapping_sha256: digest(&mapping),
                calendar_sha256: digest(&calendar),
                seed_sha256: digest(&seed),
                instrument: "IMOEXF@RTSX".into(),
                run_id: format!("t37-{}", mode.name()),
                segment_id: "five-day-component".into(),
                previous_segment_id: None,
                warmup_session_dates: dates.map(str::to_owned),
                warmup_range_start_utc_ms: history[0].close_time_utc * 1000,
                warmup_range_end_utc_ms: (history.last().unwrap().close_time_utc + 600) * 1000,
                observation_start_utc_ms: start,
                run_deadline_utc_ms: start + 5 * 86400000,
                lmax_ms: 10000,
                max_quote_age_ms: 1000,
                clock_uncertainty_ms: 100,
                ws_silence_budget_ms: 1000,
                reconnect_budget_ms: 1000,
                stop_timeout_ms: 1000,
                max_input_bytes: 4_000_000,
                max_cache_bars: 4096,
                max_output_bytes: 20_000_000,
            },
        };
        let selected = all
            .into_iter()
            .filter(|b| {
                let t = local(b);
                t.year() == 2026 && t.month() == 9 && (21..=25).contains(&t.day()) && t.hour() >= 7
            })
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 505);
        Self {
            config,
            material: [ORACLE.to_vec(), mapping, calendar, seed],
            selected,
            oracle: serde_json::from_slice(ORACLE).unwrap(),
        }
    }
    pub fn segment(&self) -> crate::segment::SeededSegment {
        crate::segment::SeededSegment::from_inputs(
            crate::ValidatedConfig::from_ws_primary(&self.config).unwrap(),
            &self.material[0],
            &self.material[1],
            &self.material[2],
            &self.material[3],
        )
        .unwrap()
    }
}

pub(super) fn rest(
    bar: &GoldBar,
    sparse: bool,
) -> broker_finam::sparse_m10::AdmittedClosedM1Snapshot {
    use broker_finam::sparse_m10::*;
    let start = at(bar.label());
    let end = at(bar.label() + 600000);
    let plan = ClosedM1RequestPlan::new(start, end, vec![end]).unwrap();
    let body = json!({"symbol":"IMOEXF@RTSX","bars":bar.rows(sparse)}).to_string();
    let now = crate::timing::now_ms();
    AdmittedClosedM1Snapshot::admit(
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
                requested_at: at(now - 50),
                received_at: at(now),
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(body.len() as u64),
                response_sha256: digest(body.as_bytes()),
                raw_body: body,
            }],
        },
        &plan,
        at(now),
    )
    .unwrap()
}
