use super::*;
use crate::{
    primary::{PrimaryPolicy, PrimaryProcessor},
    runner::primary::{Coordinator, Tick},
    segment::SeededSegment,
    session::ObservedSession,
    timing::{advance_fixture_clock, now_ms, with_fixture_clock},
    transport::ReadOnlyCollector,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(crate) struct Fixture {
    pub config: WsPrimaryConfig,
    pub binding: CalendarBinding,
    pub model: ModelDay,
    pub raw: RawCalendar,
    pub materials: [Vec<u8>; 4],
}
pub(crate) struct Dir(pub PathBuf);
impl Dir {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "primary-calendar-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(crate) fn at(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap()
}
fn date_open(date: NaiveDate) -> i64 {
    (date.and_hms_opt(7, 0, 0).unwrap().and_utc() - chrono::Duration::hours(3)).timestamp_millis()
}
pub(crate) fn instrument() -> broker_core::InstrumentId {
    broker_core::InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: broker_core::Exchange::Moex,
        market: broker_core::Market::Futures,
    }
}
fn raw_interval(kind: &str, start: i64, end: i64) -> Value {
    json!({"type":kind,"interval":{"start_time":at(start).to_rfc3339(),"end_time":at(end).to_rfc3339()}})
}
fn snapshot(sessions: Vec<Value>, received: i64) -> ScheduleSnapshot {
    let body = json!({"symbol":"IMOEXF@RTSX","sessions":sessions}).to_string();
    ScheduleSnapshot {
        method: "GET".into(),
        endpoint: "https://api.finam.ru/v1/assets/IMOEXF@RTSX/schedule".into(),
        requested_at_ms: received - 50,
        received_at_ms: received,
        status: 200,
        transport_complete: true,
        declared_body_bytes: body.len() as u64,
        response_sha256: digest(body.as_bytes()),
        raw_body: body,
    }
}
impl Fixture {
    pub fn new() -> Self {
        Self::from_materials(crate::intraday::tests::primary_restart_materials())
    }
    pub fn historical() -> Self {
        Self::from_materials(crate::intraday::tests::primary_historical_materials())
    }
    fn from_materials((mut base, materials): (crate::ShadowConfig, [Vec<u8>; 4])) -> Self {
        base.lmax_ms = 10000;
        base.clock_uncertainty_ms = 100;
        base.max_cache_bars = 4096;
        let config = WsPrimaryConfig {
            schema_version: 2,
            market_data_mode: "ws_primary_v1".into(),
            source_policy: super::super::WS_PRIMARY_POLICY.into(),
            base,
            primary: PrimaryPolicy {
                audit_every_commits: None,
                ..Default::default()
            },
        };
        let open = config.base.observation_start_utc_ms;
        let date = local_date(open).unwrap();
        let mut f = Self {
            config,
            materials,
            binding: CalendarBinding {
                schema_version: 1,
                config_sha256: String::new(),
                model_day_sha256: String::new(),
                raw_calendar_sha256: String::new(),
                bootstrap_reference_start_ms: 0,
                next_label_ms: open,
                start_not_before_ms: open,
                start_before_ms: open + 540000,
                eod_check_at_ms: open + 101 * 600000 + 9900,
            },
            model: ModelDay {
                schema_version: 1,
                session_date: date.to_string(),
                required_m10_labels_ms: (0..101).map(|i| open + i * 600000).collect(),
            },
            raw: RawCalendar {
                schema_version: 1,
                previous_session_date: date.pred_opt().unwrap().to_string(),
                snapshots: Vec::new(),
            },
        };
        f.default_raw();
        f
    }
    pub fn open(&self) -> i64 {
        self.config.base.observation_start_utc_ms
    }
    pub fn close(&self) -> i64 {
        self.open() + 101 * 600000
    }
    fn default_raw(&mut self) {
        let prev = date_open(
            NaiveDate::parse_from_str(&self.raw.previous_session_date, "%Y-%m-%d").unwrap(),
        );
        self.binding.bootstrap_reference_start_ms = prev;
        self.raw.snapshots = vec![snapshot(
            vec![
                raw_interval("EARLY_TRADING", prev, prev + 101 * 600000),
                raw_interval("CLOSED", prev + 101 * 600000, self.open()),
                raw_interval("EARLY_TRADING", self.open(), self.open() + 2 * 3600000),
                raw_interval(
                    "CORE_TRADING",
                    self.open() + 2 * 3600000,
                    self.open() + 12 * 3600000,
                ),
                raw_interval("LATE_TRADING", self.open() + 12 * 3600000, self.close()),
            ],
            self.open() - 1000,
        )];
    }
    fn monday(&mut self) {
        let mut date = local_date(self.open()).unwrap();
        while date.weekday().number_from_monday() != 1 {
            date = date.succ_opt().unwrap();
        }
        let open = date_open(date);
        self.config.base.observation_start_utc_ms = open;
        self.config.base.run_deadline_utc_ms = open + 17 * 3600000;
        self.model.session_date = date.to_string();
        self.model.required_m10_labels_ms = (0..101).map(|i| open + i * 600000).collect();
        self.binding.next_label_ms = open;
        self.binding.start_not_before_ms = open;
        self.binding.start_before_ms = open + 540000;
        self.binding.eod_check_at_ms = self.close() + 9900;
        self.raw.previous_session_date = date.pred_opt().unwrap().to_string();
        self.default_raw();
    }
    pub fn cap(&self, now: i64) -> Result<AdmittedCalendar, CalendarError> {
        let model = serde_json::to_vec(&self.model).unwrap();
        let raw = serde_json::to_vec(&self.raw).unwrap();
        let mut binding = self.binding.clone();
        binding.config_sha256 = self.config.fingerprint().unwrap();
        binding.model_day_sha256 = digest(&model);
        binding.raw_calendar_sha256 = digest(&raw);
        admit(
            &self.config,
            binding,
            CalendarInputs {
                model_day: &model,
                raw_calendar: &raw,
                mapping: &self.materials[1],
            },
            now,
        )
    }
    pub fn owner(&self, path: &Path, unbound_fixture: bool) -> Coordinator {
        let config = ValidatedConfig::from_ws_primary(&self.config).unwrap();
        let segment = SeededSegment::from_inputs(
            config,
            &self.materials[0],
            &self.materials[1],
            &self.materials[2],
            &self.materials[3],
        )
        .unwrap();
        let (stop, _) = ReadOnlyCollector::new(
            Utc::now() + chrono::Duration::hours(1),
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        let session = ObservedSession::create(segment, path, stop).unwrap();
        let processor = if unbound_fixture {
            PrimaryProcessor::from_fixture(
                &self.config,
                instrument(),
                self.model.required_m10_labels_ms.clone(),
                1,
                now_ms(),
            )
            .unwrap()
        } else {
            PrimaryProcessor::from_config(
                &self.config,
                instrument(),
                self.model.required_m10_labels_ms.clone(),
                1,
                now_ms(),
            )
            .unwrap()
        };
        Coordinator::new(processor, session).unwrap()
    }
    pub fn row(&self, label: i64) -> Value {
        json!({"timestamp":at(label).to_rfc3339(),"isDataSnapshot":false,"open":{"value":"2200"},"high":{"value":"2201"},
        "low":{"value":"2199"},"close":{"value":"2200.5"},"volume":{"value":"2"}})
    }
    pub fn rest(
        &self,
        plan: &broker_finam::sparse_m10::ClosedM1RequestPlan,
    ) -> broker_finam::sparse_m10::AdmittedClosedM1Snapshot {
        use broker_finam::sparse_m10::{
            AdmittedClosedM1Snapshot, ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1,
            OBSERVED_M1_POLICY_V1,
        };
        let rows: Vec<_> = (plan.start().timestamp_millis()..plan.end().timestamp_millis())
            .step_by(60000)
            .map(|t| self.row(t))
            .collect();
        let body = json!({"symbol":"IMOEXF@RTSX","bars":rows}).to_string();
        let now = at(now_ms());
        AdmittedClosedM1Snapshot::admit(
            ClosedM1SnapshotEvidenceV1 {
                policy: OBSERVED_M1_POLICY_V1.into(),
                start: plan.start(),
                end: plan.end(),
                parts: vec![ClosedM1RestPartV1 {
                    method: "GET".into(),
                    endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
                    symbol: "IMOEXF@RTSX".into(),
                    timeframe: "TIME_FRAME_M1".into(),
                    start: plan.start(),
                    end: plan.end(),
                    requested_at: now,
                    received_at: now,
                    status: 200,
                    transport_complete: true,
                    declared_body_bytes: Some(body.len() as u64),
                    response_sha256: digest(body.as_bytes()),
                    raw_body: body,
                }],
            },
            plan,
            now,
        )
        .unwrap()
    }
}
fn go(ms: i64) {
    advance_fixture_clock((ms - now_ms()).try_into().unwrap());
}

#[test]
fn independent_raw_weekend_reference_is_not_model_warmup_or_a_bar_count() {
    let mut f = Fixture::new();
    f.monday();
    let calendar = f.cap(f.open()).unwrap();
    assert_eq!(calendar.required().len(), 101);
    assert_eq!(calendar.last_label_ms(), f.close() - 600000);
    assert_eq!(
        local_date(calendar.reference_start_ms())
            .unwrap()
            .weekday()
            .number_from_monday(),
        7
    );
    assert_ne!(
        local_date(calendar.reference_start_ms())
            .unwrap()
            .to_string(),
        f.config.base.warmup_session_dates[3]
    );
    assert!(calendar.validate_entry(f.open()).is_ok());
    assert!(matches!(
        calendar.validate_entry(f.binding.start_before_ms),
        Err(CalendarError::Window)
    ));
    // Quiet raw days may require an earlier independently pinned M1 range.
    // Do not silently shrink it to the previous day or to received WS labels.
    f.binding.bootstrap_reference_start_ms = f.config.base.warmup_range_start_utc_ms;
    let wide = f.cap(f.open()).unwrap();
    assert_eq!(
        wide.reference_start_ms(),
        f.config.base.warmup_range_start_utc_ms
    );
    f.binding.bootstrap_reference_start_ms = calendar.reference_start_ms() + 60000;
    assert!(matches!(
        f.cap(f.open()),
        Err(CalendarError::PreviousSession)
    ));
}

#[test]
fn current_schedule_expires_at_entry_but_historical_raw_timestamps_are_not_rewritten() {
    let mut f = Fixture::new();
    f.monday();
    let monday = local_date(f.open()).unwrap();
    let friday = monday - chrono::Duration::days(3);
    f.raw.previous_session_date = friday.to_string();
    let prev = date_open(friday);
    f.binding.bootstrap_reference_start_ms = prev;
    f.raw.snapshots = vec![
        snapshot(
            vec![
                raw_interval("CORE_TRADING", prev, prev + 101 * 600000),
                raw_interval("CLOSED", prev + 101 * 600000, f.open()),
            ],
            prev + 101 * 600000 + 1000,
        ),
        snapshot(
            vec![raw_interval("CORE_TRADING", f.open(), f.close())],
            f.open() - 1000,
        ),
    ];
    let saved = f.raw.snapshots[0].clone();
    let cap = f.cap(f.open()).unwrap();
    assert_eq!(cap.raw.snapshots[0].received_at_ms, saved.received_at_ms);
    assert_eq!(cap.raw.snapshots[0].raw_body, saved.raw_body);
    f.default_raw();
    f.raw.snapshots[0].received_at_ms = f.open() - 86400000 + 10000;
    f.raw.snapshots[0].requested_at_ms = f.raw.snapshots[0].received_at_ms - 50;
    let cap = f.cap(f.open()).unwrap();
    assert_eq!(
        cap.validate_entry(f.open() + 20000),
        Err(CalendarError::Stale)
    );
}

#[test]
fn short_closed_unknown_or_conflicting_raw_schedule_is_hold_before_transport() {
    let f = Fixture::new();
    for case in [
        "short",
        "hole",
        "conflict",
        "unknown",
        "hash",
        "future",
        "truncated",
    ] {
        let mut g = Fixture::new();
        let mut body: Value = serde_json::from_str(&g.raw.snapshots[0].raw_body).unwrap();
        match case {
            "short" => {
                body["sessions"][4]["interval"]["end_time"] =
                    json!(at(g.open() + 13 * 3600000).to_rfc3339());
                body["sessions"].as_array_mut().unwrap().push(raw_interval(
                    "CLOSED",
                    g.open() + 13 * 3600000,
                    g.close(),
                ));
            }
            "hole" => {
                body["sessions"][3]["interval"]["start_time"] =
                    json!(at(g.open() + 2 * 3600000 + 60000).to_rfc3339())
            }
            "conflict" => body["sessions"].as_array_mut().unwrap().push(raw_interval(
                "CLOSED",
                g.open() + 60000,
                g.open() + 120000,
            )),
            "unknown" => body["sessions"][3]["type"] = json!("UNKNOWN_PROVIDER_STATE"),
            _ => (),
        }
        let raw = body.to_string();
        let p = &mut g.raw.snapshots[0];
        p.raw_body = raw;
        p.declared_body_bytes = p.raw_body.len() as u64;
        p.response_sha256 = digest(p.raw_body.as_bytes());
        match case {
            "hash" => p.response_sha256 = "0".repeat(64),
            "future" => p.received_at_ms = f.open() + 1,
            "truncated" => p.transport_complete = false,
            _ => (),
        }
        assert!(g.cap(g.open()).is_err(), "{case}");
    }
}

#[test]
fn model_inventory_mapping_and_real_previous_raw_day_must_agree_independently() {
    let mut f = Fixture::new();
    f.model.required_m10_labels_ms.remove(17);
    assert!(matches!(
        f.cap(f.open()),
        Err(CalendarError::ModelInventory)
    ));
    let mut f = Fixture::new();
    let mut mapping: Value = serde_json::from_slice(&f.materials[1]).unwrap();
    mapping["lot_size"] = json!(2);
    f.materials[1] = serde_json::to_vec(&mapping).unwrap();
    f.config.base.instrument_mapping_sha256 = digest(&f.materials[1]);
    assert!(matches!(f.cap(f.open()), Err(CalendarError::Binding)));
    let mut f = Fixture::new();
    f.monday();
    let monday = local_date(f.open()).unwrap();
    let friday = monday - chrono::Duration::days(3);
    let p = date_open(friday);
    f.binding.bootstrap_reference_start_ms = p;
    let saturday = p + 86400000;
    f.raw.previous_session_date = friday.to_string();
    f.raw.snapshots = vec![snapshot(
        vec![
            raw_interval("CORE_TRADING", p, p + 101 * 600000),
            raw_interval("CLOSED", p + 101 * 600000, saturday),
            raw_interval("CORE_TRADING", saturday, saturday + 101 * 600000),
            raw_interval("CLOSED", saturday + 101 * 600000, f.open()),
            raw_interval("CORE_TRADING", f.open(), f.close()),
        ],
        f.open() - 1000,
    )];
    assert!(matches!(
        f.cap(f.open()),
        Err(CalendarError::PreviousSession)
    ));
    let mut f = Fixture::new();
    f.model.session_date = "2026-11-07".into();
    assert!(matches!(f.cap(f.open()), Err(CalendarError::ModelClosed)));
}

#[test]
fn exact_calendar_binding_cannot_replace_a_missing_intraday_model_prefix() {
    let mut f = Fixture::new();
    let d = Dir::new();
    f.binding.next_label_ms = f.open() + 20 * 600000;
    f.binding.start_not_before_ms = f.binding.next_label_ms;
    f.binding.start_before_ms = f.binding.next_label_ms + 540000;
    with_fixture_clock(f.open(), || {
        // Create a legitimate fresh owner at open, then attempt to skip its
        // model prefix. A fresh session created intraday is already refused.
        let mut owner = f.owner(&d.0.join("journal"), true);
        go(f.binding.next_label_ms + 100);
        let cap = f.cap(now_ms()).unwrap();
        assert!(owner.bind_calendar(cap).is_err());
        assert_eq!(owner.processor().next_label_ms(), Some(f.open()));
        assert!(!owner.processor().subscription_data_ready());
    });
}

#[test]
fn duplicate_keys_and_unknown_raw_fields_do_not_enter_public_calendar_evidence() {
    let mut f = Fixture::new();
    let mut value: Value = serde_json::from_str(&f.raw.snapshots[0].raw_body).unwrap();
    value["authorization"] = json!("fixture-not-a-real-token");
    let raw = value.to_string();
    let p = &mut f.raw.snapshots[0];
    p.raw_body = raw;
    p.declared_body_bytes = p.raw_body.len() as u64;
    p.response_sha256 = digest(p.raw_body.as_bytes());
    assert!(matches!(f.cap(f.open()), Err(CalendarError::Decode)));
    let mut f = Fixture::new();
    let p = &mut f.raw.snapshots[0];
    p.raw_body = p.raw_body.replacen(
        "\"symbol\":\"IMOEXF@RTSX\"",
        "\"symbol\":\"IMOEXF@RTSX\",\"symbol\":\"IMOEXF@RTSX\"",
        1,
    );
    p.declared_body_bytes = p.raw_body.len() as u64;
    p.response_sha256 = digest(p.raw_body.as_bytes());
    assert!(matches!(f.cap(f.open()), Err(CalendarError::Decode)));
}

#[test]
fn session_end_uses_one_proof_without_a_next_m1_and_never_becomes_ws_live() {
    let f = Fixture::new();
    let d = Dir::new();
    with_fixture_clock(f.open(), || {
        let cap = f.cap(now_ms()).unwrap();
        let mut owner = f.owner(&d.0.join("journal"), true);
        owner.bind_calendar(cap).unwrap();
        for minute in 0..1010 {
            let label = f.open() + minute * 60000;
            go(label + 100);
            let wire=json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(label)]}}).to_string();
            owner
                .on_frame(
                    crate::primary::WireFrame::normalize(&wire, 1, "fixture", at(now_ms()))
                        .unwrap(),
                )
                .unwrap();
            if minute > 0 && minute % 10 == 0 {
                go(label + 1100);
                assert!(matches!(
                    owner.tick().unwrap(),
                    Tick::Published {
                        origin: "WS_LIVE",
                        ..
                    }
                ));
            }
        }
        go(f.close() + 1000);
        assert!(matches!(owner.tick().unwrap(), Tick::Waiting));
        go(f.close() + 3000);
        let Tick::ProofRequired(ticket) = owner.tick().unwrap() else {
            panic!("SESSION_END proof");
        };
        assert_eq!(
            ticket.reason(),
            Some(super::super::ExceptionReason::SessionEnd)
        );
        assert!(matches!(owner.tick().unwrap(), Tick::AwaitingProof));
        let source = f.rest(&ticket.request_plan().unwrap());
        assert!(matches!(
            owner.complete_proof(ticket, Ok(source)).unwrap(),
            Tick::Published {
                origin: "REST_EXCEPTION",
                ..
            }
        ));
        assert_eq!(owner.processor().next_label_ms(), None);
        assert_eq!(owner.role_counts().proof_scheduled, 1);
        assert_eq!(owner.role_counts().session_end_scheduled, 1);
        let log = std::fs::read_to_string(d.0.join("journal")).unwrap();
        let rows: Vec<Value> = log
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(
            rows.iter()
                .filter(|r| r["kind"] == "canonical_commit")
                .count(),
            101
        );
        let last = rows
            .iter()
            .rfind(|r| r["kind"] == "canonical_source")
            .unwrap();
        assert_eq!(last["event"]["receipt"]["reason"], "SessionEnd");
        assert!(last["event"]["receipt"]["boundary_received_at_ms"].is_null());
    });
}
