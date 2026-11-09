//! File-only executable tests. Future validation and fixed historical EOD
//! fixtures are SYNTHETIC, never FINAM or operational readiness. No observe run.
use broker_core::*;
use chrono::{Datelike, Duration, Utc};
use finam_signal_shadow::{
    feedback,
    primary::{calendar::*, entry::*, PrimaryPolicy, WsPrimaryConfig},
    profile,
    seed::*,
    Reference, RestartMode, ShadowConfig,
};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    path::PathBuf,
    process::{Command, Output},
};

struct Fixture {
    path: PathBuf,
    files: Vec<OsString>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
impl Fixture {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "primary-cli-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let now = Utc::now().timestamp_millis();
        let mut date = (Utc::now() + Duration::days(2)).date_naive();
        while date.weekday().number_from_monday() > 5 {
            date = date.succ_opt().unwrap();
        }
        let open = date
            .and_hms_opt(4, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let close = open + 101 * 600000;
        let mut days = vec![];
        let mut d = date;
        while days.len() < 4 {
            d = d.pred_opt().unwrap();
            if d.weekday().number_from_monday() <= 5 {
                days.push(d);
            }
        }
        days.reverse();
        let instrument = InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        };
        let sessions: Vec<_> = days
            .iter()
            .map(|d| SessionInventory {
                date: d.to_string(),
                required_m10_labels_utc: (0..101)
                    .map(|i| d.and_hms_opt(4, 0, 0).unwrap().and_utc().timestamp() + i * 600)
                    .collect(),
            })
            .collect();
        let bars: Vec<_> = sessions
            .iter()
            .flat_map(|s| s.required_m10_labels_utc.iter())
            .map(|&t| HybridRuntimeBarEvent {
                instrument: instrument.clone(),
                close_time_utc: t,
                open: 2200.,
                high: 2201.,
                low: 2199.,
                close: 2200.5,
                volume: 10.,
                origin: HybridRuntimeBarOrigin::History,
                is_final: true,
                timeframe_sec: 600,
            })
            .collect();
        let reference = b"synthetic reference, not current ALOR".to_vec();
        let mapping = serde_json::to_vec(&InstrumentMapping {
            instrument,
            tick_size: "0.5".into(),
            lot_size: 1,
        })
        .unwrap();
        let seed_calendar = serde_json::to_vec(&CalendarInventory {
            sessions: sessions.try_into().unwrap(),
        })
        .unwrap();
        let seed = serde_json::to_vec(&bars).unwrap();
        let config = WsPrimaryConfig {
            schema_version: 2,
            market_data_mode: "ws_primary_v1".into(),
            source_policy: finam_signal_shadow::primary::WS_PRIMARY_POLICY.into(),
            primary: PrimaryPolicy::default(),
            base: ShadowConfig {
                schema_version: 1,
                reference: Reference::AlorFreeze20260927,
                restart_mode: RestartMode::NewSegment,
                model_profile: "imoexf-baseline07-bo-only-no-riskgate".into(),
                model_sha256: strategy_runtime_core::HybridIntradayRuntimeStrategy::new(
                    profile::baseline07_config().unwrap(),
                )
                .stage5c_config_fingerprint(),
                reference_sha256: digest(&reference),
                feedback_policy_sha256: feedback::candle_policy_fingerprint(),
                instrument_mapping_sha256: digest(&mapping),
                calendar_sha256: digest(&seed_calendar),
                seed_sha256: digest(&seed),
                instrument: "IMOEXF@RTSX".into(),
                run_id: "primary-cli-test".into(),
                segment_id: "fresh".into(),
                previous_segment_id: None,
                warmup_session_dates: days
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
                warmup_range_start_utc_ms: bars.first().unwrap().close_time_utc * 1000,
                warmup_range_end_utc_ms: (bars.last().unwrap().close_time_utc + 600) * 1000,
                observation_start_utc_ms: open,
                run_deadline_utc_ms: open + 17 * 3600000,
                lmax_ms: 10000,
                max_quote_age_ms: 1000,
                clock_uncertainty_ms: 100,
                ws_silence_budget_ms: 1000,
                reconnect_budget_ms: 1000,
                stop_timeout_ms: 1000,
                max_input_bytes: 2_000_000,
                max_cache_bars: 4096,
                max_output_bytes: 20_000_000,
            },
        };
        let model = serde_json::to_vec(&ModelDay {
            schema_version: 1,
            session_date: date.to_string(),
            required_m10_labels_ms: (0..101).map(|i| open + i * 600000).collect(),
        })
        .unwrap();
        let prev = days[3]
            .and_hms_opt(4, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let iso = |ms| {
            chrono::DateTime::from_timestamp_millis(ms)
                .unwrap()
                .to_rfc3339()
        };
        let interval = |kind: &str, a, b| json!({"type":kind,"interval":{"start_time":iso(a),"end_time":iso(b)}});
        let raw_body=json!({"symbol":"IMOEXF@RTSX","sessions":[interval("CORE_TRADING",prev,prev+101*600000),interval("CLOSED",prev+101*600000,open),interval("CORE_TRADING",open,close)]}).to_string();
        let raw = serde_json::to_vec(&RawCalendar {
            schema_version: 1,
            previous_session_date: days[3].to_string(),
            snapshots: vec![ScheduleSnapshot {
                method: "GET".into(),
                endpoint: "https://api.finam.ru/v1/assets/IMOEXF@RTSX/schedule".into(),
                requested_at_ms: now - 100,
                received_at_ms: now - 50,
                status: 200,
                transport_complete: true,
                declared_body_bytes: raw_body.len() as u64,
                response_sha256: digest(raw_body.as_bytes()),
                raw_body,
            }],
        })
        .unwrap();
        let plan = Plan {
            schema_version: 1,
            config_sha256: config.fingerprint().unwrap(),
            account_id: "ACCOUNT_CLI_DO_NOT_PRINT".into(),
            read_only_credentials: true,
            auth_policy: "provider_expiry_readonly_rtsx_v1".into(),
            calendar: CalendarBinding {
                schema_version: 1,
                config_sha256: config.fingerprint().unwrap(),
                model_day_sha256: digest(&model),
                raw_calendar_sha256: digest(&raw),
                bootstrap_reference_start_ms: prev,
                next_label_ms: open,
                start_not_before_ms: open,
                start_before_ms: open + 540000,
                eod_check_at_ms: close + 9900,
            },
            startup: Startup::FreshSession,
        };
        let mut files = vec![];
        for (name, body) in [
            ("config", serde_json::to_vec(&config).unwrap()),
            ("reference", reference),
            ("mapping", mapping),
            ("seed-calendar", seed_calendar),
            ("seed", seed),
            ("model-day", model),
            ("raw-calendar", raw),
            ("plan", serde_json::to_vec(&plan).unwrap()),
        ] {
            let file = path.join(name);
            std::fs::write(&file, body).unwrap();
            files.push(file.into_os_string());
        }
        Self { path, files }
    }
    fn command(&self, command: &str, extra: &[OsString]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_finam-signal-shadow"))
            .arg(command)
            .args(&self.files)
            .args(extra)
            .env_remove("FINAM_SECRET")
            .env_remove("FINAM_TOKEN")
            .output()
            .unwrap()
    }
}
#[test]
fn executable_verifies_historical_fixture_and_rejects_rebound_unjoined_completion() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/ws-primary-file-only-20260928");
    let metadata: Value =
        serde_json::from_slice(&std::fs::read(root.join("fixture.json")).unwrap()).unwrap();
    assert_eq!(
        metadata["scope"],
        "SYNTHETIC_HISTORICAL_FILE_ONLY_NOT_FINAM_NOT_LIVE_PARITY"
    );
    for (name, pin) in metadata["files_sha256"].as_object().unwrap() {
        assert_eq!(*pin, digest(&std::fs::read(root.join(name)).unwrap()));
    }
    let originals: Vec<_> = [
        "config.json",
        "evidence-binding.json",
        "journal.jsonl",
        "completion.json",
        "fixture.json",
    ]
    .map(|n| (n, std::fs::read(root.join(n)).unwrap()))
    .into();
    let invoke =
        |binding: &std::path::Path, journal: &std::path::Path, completion: &std::path::Path| {
            Command::new(env!("CARGO_BIN_EXE_finam-signal-shadow"))
                .arg("verify-ws-primary")
                .arg(root.join("config.json"))
                .arg(binding)
                .arg(journal)
                .arg(completion)
                .env_remove("FINAM_SECRET")
                .env_remove("FINAM_TOKEN")
                .output()
                .unwrap()
        };
    let output = invoke(
        &root.join("evidence-binding.json"),
        &root.join("journal.jsonl"),
        &root.join("completion.json"),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["semantic_eod_passed"], true);
    assert_eq!(value["canonical_labels"], 101);
    assert_eq!(value["ws_live_labels"], 100);
    assert_eq!(value["rest_exception_labels"], 1);
    for key in ["network_qualified", "cgroup_qualified", "parity_accepted"] {
        assert_eq!(value[key], false);
    }
    let temp = Fixture::new();
    for field in ["writer_drained", "model_prepared", "network_qualified"] {
        let mut completion: Value =
            serde_json::from_slice(&std::fs::read(root.join("completion.json")).unwrap()).unwrap();
        completion[field] = json!(field == "network_qualified");
        completion["semantic_eod_passed"] = json!(true);
        let bytes = serde_json::to_vec(&completion).unwrap();
        let completion_path = temp.path.join(format!("{field}-completion.json"));
        std::fs::write(&completion_path, &bytes).unwrap();
        let mut binding: Value =
            serde_json::from_slice(&std::fs::read(root.join("evidence-binding.json")).unwrap())
                .unwrap();
        binding["completion_sha256"] = json!(digest(&bytes));
        let binding_path = temp.path.join(format!("{field}-binding.json"));
        std::fs::write(&binding_path, serde_json::to_vec(&binding).unwrap()).unwrap();
        let output = invoke(&binding_path, &root.join("journal.jsonl"), &completion_path);
        assert!(!output.status.success(), "{field}");
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr).unwrap().trim(),
            "primary_verification_refused"
        );
    }
    for (name, bytes) in originals {
        assert_eq!(bytes, std::fs::read(root.join(name)).unwrap());
    }
    assert!(!temp.path.join("NO_OUTPUT").exists());
}

#[test]
fn executable_validates_v2_without_credentials_outputs_or_network_activation() {
    let f = Fixture::new();
    let output = f.command("validate-ws-primary", &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["required_labels"], 101);
    assert_eq!(receipt["entry_window_open_now"], false);
    assert_eq!(receipt["history_proves_live_readiness"], false);
    assert!(!String::from_utf8(output.stdout)
        .unwrap()
        .contains("ACCOUNT_CLI_DO_NOT_PRINT"));
    assert_eq!(std::fs::read_dir(&f.path).unwrap().count(), 8);
    let output = f.command(
        "observe-ws-primary",
        &[
            f.path.join("NO_SECRET").into_os_string(),
            f.path.join("NO_OUTPUT").into_os_string(),
        ],
    );
    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "primary_entry_window"
    );
    assert!(!f.path.join("NO_OUTPUT").exists());
}
#[test]
fn executable_rejects_legacy_config_restart_mode_and_unverified_completion() {
    let f = Fixture::new();
    let output = f.command(
        "validate-ws-primary-restart",
        &[f.path.join("MISSING_PREFIX").into_os_string()],
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "primary_startup_mode"
    );
    let binding = f.path.join("binding");
    let journal = f.path.join("journal");
    let completion = f.path.join("completion");
    std::fs::write(&binding, b"{}").unwrap();
    std::fs::write(&journal, b"{}").unwrap();
    std::fs::write(&completion, b"{\"semantic_eod_passed\":true}").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_finam-signal-shadow"))
        .arg("verify-ws-primary")
        .arg(&f.files[0])
        .arg(binding)
        .arg(journal)
        .arg(completion)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&std::fs::read(&f.files[0]).unwrap()).unwrap();
    std::fs::write(&f.files[0], serde_json::to_vec(&value["base"]).unwrap()).unwrap();
    let output = f.command("validate-ws-primary", &[]);
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "primary_config_invalid"
    );
}
