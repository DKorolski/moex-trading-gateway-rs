use super::*;
use crate::stage8b_p1f_o3_composition::producers;
use broker_core::*;
use broker_finam::sparse_m10::{
    AdmittedClosedM1Snapshot, ClosedM1RequestPlan, ClosedM1RestPartV1, OBSERVED_M1_POLICY_V1,
};
use ed25519_dalek::SigningKey;
use rust_decimal::Decimal;
use serde_json::{json, Value};

struct Case {
    plan: O3CompositionPlan,
    config: Vec<u8>,
    phase: durable::Stage8bP1fPhaseManifestV1,
    key: SigningKey,
    now: DateTime<Utc>,
    root: std::path::PathBuf,
}

impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Case {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("o3-inputs-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let root = root.canonicalize().unwrap();
        let now = parse_time("2026-10-08T09:50:00Z").unwrap();
        let profile = durable::Stage8bP1RuntimeProfileKind::V2;
        let fingerprint = profile.build_hybrid_runtime().unwrap().1;
        let mut config: Value = serde_json::from_slice(include_bytes!(
            "../../../../docs/stage-8/stage8b-p1f-o2-supervisor-template.json"
        ))
        .unwrap();
        config["runtime_profile_id"] = json!(profile.profile_id());
        config["runtime_profile_sha256"] = json!(profile.profile_sha256());
        config["first_boot_source_bundle_sha256"] = json!("9".repeat(64));
        config["bootstrap"]["schema_version"] = json!(2);
        config["bootstrap"]["durable_parent"] = json!(root);
        config["bootstrap"]["market_data_policy_sha256"] =
            json!(durable::STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256);
        config["bootstrap"]["runtime_config_fingerprint_sha256"] = json!(fingerprint);
        let config = serde_json::to_vec(&config).unwrap();
        let validated = durable::validate_stage8b_p1e_supervisor_config_v1(
            durable::parse_stage8b_p1e_supervisor_config_v1(&config).unwrap(),
            [1; 16],
        )
        .unwrap();
        let identity = validated
            .bootstrap()
            .operational_identity_sha256()
            .to_string();
        let entry = InstrumentMapEntry {
            internal_symbol: InternalSymbol("IMOEXF".into()),
            broker: BrokerKind::Finam,
            broker_symbol: BrokerSymbol("IMOEXF@RTSX".into()),
            exchange: Exchange::Moex,
            market: Market::Futures,
            price_step: Decimal::new(5, 1),
            qty_step: Decimal::ONE,
            lot_size: Decimal::ONE,
            min_qty: Decimal::ONE,
            step_value: Decimal::new(5, 0),
            currency: "RUB".into(),
            schedule_id: "RTSX".into(),
            expiration_date: None,
            is_tradable: true,
        };
        let truth = BrokerTruthSnapshot {
            account_id: BrokerAccountId::new("finam-paper-primary"),
            orders: Vec::new(),
            positions: Vec::new(),
            trades: Vec::new(),
            cash: None,
            instruments: vec![BrokerInstrumentSpec {
                instrument: entry.clone(),
                broker_asset_id: Some("SYNTHETIC-O3".into()),
                board: Some("RTSX".into()),
                long_initial_margin: Some(Money::new(5000, 0)),
                short_initial_margin: Some(Money::new(5000, 0)),
            }],
            received_ts: now,
        };
        let source = |end: DateTime<Utc>| {
            let start = now;
            let bars = (0..(end - start).num_minutes())
                .map(|i| {
                    json!({
                        "timestamp": (start + chrono::Duration::minutes(i)).to_rfc3339(),
                        "open":{"value":"2300"}, "high":{"value":"2301"},
                        "low":{"value":"2299"}, "close":{"value":"2300"}, "volume":{"value":"1"}
                    })
                })
                .collect::<Vec<_>>();
            let raw = serde_json::to_string(&json!({"symbol":"IMOEXF@RTSX","bars":bars})).unwrap();
            let evidence = ClosedM1SnapshotEvidenceV1 {
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
                    requested_at: end,
                    received_at: end,
                    status: 200,
                    transport_complete: true,
                    declared_body_bytes: Some(raw.len() as u64),
                    response_sha256: hash(raw.as_bytes()),
                    raw_body: raw,
                }],
            };
            // Historical times are confined to this unit fixture. The deployed
            // code exposes no clock override or synthetic freshness bypass.
            let admitted = AdmittedClosedM1Snapshot::admit(
                evidence.clone(),
                &ClosedM1RequestPlan::new(start, end, vec![end]).unwrap(),
                end,
            )
            .unwrap();
            let bytes = durable::build_stage8b_p1_observed_canonical_m10(
                &identity,
                (end - chrono::Duration::minutes(10)).timestamp_millis(),
                admitted.receipt(),
            )
            .unwrap();
            O3ObservedInput {
                evidence,
                snapshot_sha256: admitted.sha256().into(),
                receipt_sha256: admitted.receipt().sha256().into(),
                canonical_m10_sha256: hash(&bytes),
            }
        };
        let plan = O3CompositionPlan {
            schema_version:1,domain:"moex.stage8b.p1f.o3-composition.v1".into(),
            phase_id:"o3-local-synthetic".into(), operational_identity_sha256:identity.clone(),
            runtime_config_fingerprint_sha256:fingerprint,
            instrument_map_fingerprint_sha256:durable::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            schedule_fixture_sha256:"1".repeat(64),schedule_semantic_sha256:"2".repeat(64),
            schedule:O3ScheduleTemplate {
                synthetic_truth:truth, registry_entry:entry, registry_version:"imoexf-v1".into(),
                registry_identity_sha256:durable::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                exact_synthetic_schedule_json:serde_json::to_string(&json!({
                    "symbol":"IMOEXF@RTSX", "sessions":[{"type":"SESSION_TYPE_MAIN",
                    "interval":{"start_time":"2026-10-08T04:00:00Z","end_time":"2026-10-08T20:50:00Z"}}]
                })).unwrap(),
            },
            decision:source(now+chrono::Duration::minutes(10)),
            successor:source(now+chrono::Duration::minutes(20)),
            catchup_coverage: serde_json::from_value(json!({
                "source_mode":"config-bound-explicit-session-windows-v1",
                "sessions":[{"session_date":"2026-10-08","windows":[{
                    "first_close_time_utc":parse_time("2026-10-08T04:00:00Z").unwrap().timestamp(),
                    "last_close_time_utc":parse_time("2026-10-08T20:40:00Z").unwrap().timestamp()
                }]}]
            })).unwrap(),
        };
        let phase = durable::Stage8bP1fPhaseManifestV1 {
            schema_version: 1,
            domain: "stage8b-p1f-phase-manifest-v1".into(),
            installation_id: "isolated-test".into(),
            target_host_id: durable::STAGE8B_P1F_TARGET_HOST_ID.into(),
            target_host_ssh_ed25519_sha256: durable::STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256
                .into(),
            authority_generation: 1,
            authority_sequence: 27,
            predecessor_event_sha256: "3".repeat(64),
            accepted_source_tree_sha256: "4".repeat(64),
            phase: durable::Stage8bP1fPhaseV1::O3SyntheticPaper,
            materialization_policy_sha256: hash(&stage8b_p1e_canonical_json(&plan).unwrap()),
            config_template_sha256: hash(&config),
            installation_sha256: "5".repeat(64),
            controller_id: "isolated-test".into(),
            not_before_utc: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            deadline_utc: (now + chrono::Duration::minutes(30))
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            issuer_key_id: KEY_ID.into(),
            signature_ed25519_hex: String::new(),
        };
        Self {
            plan,
            config,
            phase,
            key: SigningKey::from_bytes(&[49; 32]),
            now,
            root,
        }
    }
    fn phase_bytes(&self) -> Vec<u8> {
        durable::sign_stage8b_p1f_phase_manifest_v1(self.phase.clone(), &self.key).unwrap()
    }
    fn admit(&self) -> Result<AdmittedInputs, O3Error> {
        let phase = self.phase_bytes();
        AdmittedInputs::admit(
            &phase,
            &hash(&phase),
            &durable::stage8b_p1f_authority_public_key_hex(&self.key),
            &stage8b_p1e_canonical_json(&self.plan).unwrap(),
            &self.config,
            self.now,
            [1; 16],
        )
    }
    fn rebind_plan(&mut self) {
        self.phase.materialization_policy_sha256 =
            hash(&stage8b_p1e_canonical_json(&self.plan).unwrap());
    }
}

#[test]
fn o3_fixed_inputs_bind_signed_phase_plan_config_identity_and_window() {
    assert!(Case::new().admit().is_ok());
    let mut fractional = Case::new();
    fractional.phase.not_before_utc = fractional
        .now
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    assert!(fractional.admit().is_err());
    let mut c = Case::new();
    c.phase.phase = durable::Stage8bP1fPhaseV1::O2MaterializeBootstrap;
    assert!(c.admit().is_err());
    let mut c = Case::new();
    c.now -= chrono::Duration::seconds(1);
    assert!(c.admit().is_err());
    let mut c = Case::new();
    c.now += chrono::Duration::seconds(1800);
    assert!(c.admit().is_err());
    let mut c = Case::new();
    c.phase.deadline_utc = (c.now + chrono::Duration::seconds(1801)).to_rfc3339();
    assert!(c.admit().is_err());
    let mut c = Case::new();
    c.plan.operational_identity_sha256 = "1".repeat(64);
    c.rebind_plan();
    assert!(c.admit().is_err());
    let mut c = Case::new();
    c.plan.schedule.registry_version = "wrong".into();
    c.rebind_plan();
    assert!(c.admit().is_err());
    let mut c = Case::new();
    c.plan.successor.evidence.end += chrono::Duration::minutes(10);
    c.rebind_plan();
    assert!(c.admit().is_err());
    let mut c = Case::new();
    c.plan.phase_id = "changed-without-signature".into();
    assert!(c.admit().is_err());
    let mut c = Case::new();
    c.config.push(b' ');
    assert!(c.admit().is_err());
    let c = Case::new();
    let mut phase = c.phase_bytes();
    phase[10] ^= 1;
    assert!(AdmittedInputs::admit(
        &phase,
        &hash(&phase),
        &durable::stage8b_p1f_authority_public_key_hex(&c.key),
        &stage8b_p1e_canonical_json(&c.plan).unwrap(),
        &c.config,
        c.now,
        [1; 16]
    )
    .is_err());
}

#[test]
fn o3_synthetic_schedule_uses_ordinary_report_and_stable_semantic_pin() {
    let mut c = Case::new();
    let first = producers::build_schedule_input(&c.plan, c.now).unwrap();
    c.plan.schedule_fixture_sha256 =
        crate::stage8b_p1f_synthetic_schedule_fixture_sha256(&c.plan.phase_id, &first).unwrap();
    let first = producers::schedule_input(&c.plan, c.now).unwrap();
    let second = producers::schedule_input(&c.plan, c.now + chrono::Duration::seconds(1)).unwrap();
    assert_ne!(
        first.payload.stage4_evidence.report_sha256,
        second.payload.stage4_evidence.report_sha256
    );
    assert_eq!(
        first.payload.normalized_schedule.normalized_payload_sha256,
        second.payload.normalized_schedule.normalized_payload_sha256
    );
    assert_eq!(first.payload.registry, second.payload.registry);
    assert!(second.payload.stage4_evidence.boundary_proof.is_none());
    let submicro = c.now + chrono::Duration::nanoseconds(123_456_789);
    let precise = producers::schedule_input(&c.plan, submicro).unwrap();
    let declared =
        DateTime::parse_from_rfc3339(&precise.payload.stage4_evidence.source_observed_at_utc)
            .unwrap()
            .with_timezone(&Utc);
    let report_hex = &precise.payload.stage4_evidence.report_canonical_json_hex;
    let report_bytes = report_hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    let report: Stage4BootstrapEvidenceReport = serde_json::from_slice(&report_bytes).unwrap();
    assert_eq!(report.checked_ts, declared);
    assert_eq!(
        declared,
        DateTime::from_timestamp_micros(submicro.timestamp_micros()).unwrap()
    );
    assert_eq!(precise.published_at_utc, declared);
    assert_eq!(
        declared.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        precise.payload.stage4_evidence.source_observed_at_utc
    );
    c.plan.schedule_fixture_sha256 = "0".repeat(64);
    assert!(producers::schedule_input(&c.plan, c.now).is_err());
    c.plan.schedule.synthetic_truth.instruments.clear();
    assert!(producers::build_schedule_input(&c.plan, c.now).is_err());
}
