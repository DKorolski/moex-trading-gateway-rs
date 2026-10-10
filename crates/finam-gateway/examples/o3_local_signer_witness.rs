//! Disposable network-none Linux witness only. No runtime, Redis, bootstrap,
//! VPS, private schedule-key read or alternate schedule trust key.
//! As a Cargo example this client has dev-dependency features; it is NOT the
//! production signer. The witness builds that binary in a separate invocation.
//! `prepare` creates a LOCAL ephemeral guardian authority; `request` executes
//! the normal publisher -> fixed socket -> production verifier path.
use broker_core::{stage4_bootstrap::*, *};
use broker_finam::sparse_m10::*;
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::SigningKey;
use finam_gateway::{stage8b_p1f_o3_composition::*, stage8b_p1f_o3_signer::*, *};
use runtime_durable_service as d;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};
use strategy_runtime_core::{stage8b_p1e_canonical_json as canonical, *};

#[path = "o3_local_signer_witness/linked.rs"]
mod linked;

const KEY_ID: &str = "stage8b-p1f-o2-authority-generation-1";
const ROOT: &str = "/var/lib/moex-finam-p1-paper-control";
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}
fn phase_stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
fn write(path: &str, bytes: &[u8], mode: u32) {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)
        .unwrap();
    f.write_all(bytes).unwrap();
    f.sync_all().unwrap();
}

fn input(plan: &O3CompositionPlan) -> Stage8bP1eSchedulePublisherInputV1 {
    let now = DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
    let mut truth = plan.schedule.synthetic_truth.clone();
    truth.received_ts = now;
    let mut freshness = Stage4BrokerTruthFreshnessInput::from_broker_truth_received_ts(now, 60_000);
    freshness.schedule = Stage4BrokerTruthFreshnessProbe::fresh(now, 60_000, true);
    let validated = validate_stage4_broker_truth_bootstrap(Stage4BrokerTruthBootstrapInput {
        broker_truth: &truth,
        broker_truth_source_status: Stage4BrokerTruthSourceStatus::Present,
        target_instrument: InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        },
        restored_runtime_state: None,
        freshness,
        schedule_state: BrokerMarketSessionState::Open,
        adoption: Stage4AdoptionDisposition::default(),
        external_issues: vec![],
        safety_boundary: Stage4BrokerTruthSafetyBoundary::closed(),
        checked_ts: now,
    });
    let sections = validated
        .freshness
        .sections
        .iter()
        .map(|s| Stage4BootstrapEvidenceSourceStatusSection {
            section: s.section,
            source_status: Stage4BrokerTruthSourceStatus::Present,
            required_for_bootstrap: s.required_for_bootstrap,
        })
        .collect::<Vec<_>>();
    let evidence = build_stage4_accepted_paper_host_evidence(&validated, &sections).unwrap();
    adapt_stage8b_p1e_readonly_schedule(Stage8bP1eReadonlyScheduleAdapterInputV1 {
        exact_finam_response_bytes: plan
            .schedule
            .exact_synthetic_schedule_json
            .as_bytes()
            .to_vec(),
        schedule: serde_json::from_str(&plan.schedule.exact_synthetic_schedule_json).unwrap(),
        stage4_report: evidence.report().clone(),
        registry_entry: plan.schedule.registry_entry.clone(),
        registry_version: plan.schedule.registry_version.clone(),
        registry_identity_sha256: plan.schedule.registry_identity_sha256.clone(),
        operational_identity_sha256: plan.operational_identity_sha256.clone(),
        runtime_config_fingerprint_sha256: plan.runtime_config_fingerprint_sha256.clone(),
        instrument_map_fingerprint_sha256: plan.instrument_map_fingerprint_sha256.clone(),
        source_observed_at_utc: now,
        published_at_utc: now,
        evidence_kind: Stage8bP1eScheduleEvidenceKindV1::Tradability,
        schedule_state: Stage8bP1eScheduleStateV1::Open,
        boundary_proof: None,
    })
    .unwrap()
}

fn prepare(linked_runtime: bool) {
    assert_eq!(unsafe { libc::geteuid() }, 0);
    let now = DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
    let deadline = now + Duration::minutes(30);
    let profile = d::Stage8bP1RuntimeProfileKind::V2;
    let fingerprint = profile.build_hybrid_runtime().unwrap().1;
    let map = d::stage8b_p1_imoexf_instrument_map_fingerprint_sha256();
    let mut config: Value = serde_json::from_slice(include_bytes!(
        "../../../docs/stage-8/stage8b-p1f-o2-supervisor-template.json"
    ))
    .unwrap();
    config["runtime_profile_id"] = json!(profile.profile_id());
    config["runtime_profile_sha256"] = json!(profile.profile_sha256());
    config["bootstrap"]["schema_version"] = json!(2);
    config["bootstrap"]["market_data_policy_sha256"] =
        json!(d::STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256);
    config["bootstrap"]["runtime_config_fingerprint_sha256"] = json!(fingerprint);
    // Deliberately NOT the accepted operational identity or a VPS phase.
    let installation = if linked_runtime {
        "o3-local-runtime-witness"
    } else {
        "o3-local-signer-witness"
    };
    config["bootstrap"]["deployment_id"] = json!(installation);
    let initial_config = serde_json::to_vec(&config).unwrap();
    let boot = uuid::Uuid::parse_str(
        fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .unwrap()
            .trim(),
    )
    .unwrap();
    let validated = d::validate_stage8b_p1e_supervisor_config_v1(
        d::parse_stage8b_p1e_supervisor_config_v1(&initial_config).unwrap(),
        *boot.as_bytes(),
    )
    .unwrap();
    let identity = validated
        .bootstrap()
        .operational_identity_sha256()
        .to_owned();
    config["redis_deployment_manifest_sha256"] = json!(hash(
        &d::stage8b_p1f_o3_redis_manifest_v1(&validated).unwrap()
    ));
    let linked_inputs = if linked_runtime {
        Some(linked::inputs(&mut config, &identity, now))
    } else {
        None
    };
    let config = serde_json::to_vec(&config).unwrap();
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
        orders: vec![],
        positions: vec![],
        trades: vec![],
        cash: None,
        instruments: vec![BrokerInstrumentSpec {
            instrument: entry.clone(),
            broker_asset_id: Some("LOCAL-SYNTHETIC".into()),
            board: Some("RTSX".into()),
            long_initial_margin: Some(Money::new(5000, 0)),
            short_initial_margin: Some(Money::new(5000, 0)),
        }],
        received_ts: now,
    };
    let close = DateTime::from_timestamp((now.timestamp() / 600 + 1) * 600, 0).unwrap();
    let source = |end: DateTime<Utc>| {
        let start = close - Duration::minutes(10);
        let bars = (0..(end-start).num_minutes()).map(|m| json!({
            "timestamp":(start+Duration::minutes(m)).to_rfc3339(),
            "open":{"value":"2300"},"high":{"value":"2301"},"low":{"value":"2299"},"close":{"value":"2300"},"volume":{"value":"1"}
        })).collect::<Vec<_>>();
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
        // Offline fixture PIN calculation, not runtime freshness authority.
        // No producer or runtime is called by this signer-only witness.
        let admitted = AdmittedClosedM1Snapshot::admit(
            evidence.clone(),
            &ClosedM1RequestPlan::new(start, end, vec![end]).unwrap(),
            end,
        )
        .unwrap();
        let bytes = d::build_stage8b_p1_observed_canonical_m10(
            &identity,
            (end - Duration::minutes(10)).timestamp_millis(),
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
    let session_start = (now + Duration::hours(3))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        - Duration::hours(3);
    let (decision, successor) =
        linked_inputs.unwrap_or_else(|| (source(close), source(close + Duration::minutes(10))));
    let (schedule_start, schedule_end) = if linked_runtime {
        (
            session_start + Duration::hours(7),
            session_start + Duration::hours(23) + Duration::minutes(50),
        )
    } else {
        (
            session_start,
            session_start + Duration::hours(24) - Duration::seconds(1),
        )
    };
    let mut plan = O3CompositionPlan {
        schema_version:1,domain:"moex.stage8b.p1f.o3-composition.v1".into(),phase_id:installation.into(),
        operational_identity_sha256:identity.clone(),runtime_config_fingerprint_sha256:fingerprint,instrument_map_fingerprint_sha256:map.clone(),
        schedule_fixture_sha256:"0".repeat(64),schedule_semantic_sha256:"0".repeat(64),
        schedule:O3ScheduleTemplate {synthetic_truth:truth,registry_entry:entry,registry_version:"imoexf-v1".into(),registry_identity_sha256:map,
            exact_synthetic_schedule_json:serde_json::to_string(&json!({"symbol":"IMOEXF@RTSX","sessions":[{"type":"SESSION_TYPE_MAIN","interval":{"start_time":stamp(schedule_start),"end_time":stamp(schedule_end)}}]})).unwrap()},
        decision,successor,
        // This disposable witness explicitly declares its synthetic session;
        // operational plans must supply approved History coverage instead.
        catchup_coverage:serde_json::from_value(json!({
            "source_mode":"config-bound-explicit-session-windows-v1",
            "sessions":[{"session_date":(schedule_start+Duration::hours(3)).date_naive().to_string(),
                "windows":[{"first_close_time_utc":schedule_start.timestamp()/600*600,
                "last_close_time_utc":schedule_end.timestamp()/600*600-600}]}]
        })).unwrap(),
    };
    let input = input(&plan);
    plan.schedule_fixture_sha256 =
        stage8b_p1f_synthetic_schedule_fixture_sha256(&plan.phase_id, &input).unwrap();
    plan.schedule_semantic_sha256 =
        stage8b_p1e_schedule_semantic_sha256(&Stage8bP1eScheduleSemanticIdentityV1 {
            domain: "moex.stage8b.p1e.schedule-semantic-identity.v1".into(),
            schema_version: 1,
            instrument: input.payload.instrument.clone(),
            registry: input.payload.registry.clone(),
            sessions: input.payload.normalized_schedule.sessions.clone(),
            timeframe_sec: 600,
            timezone: input.payload.timezone.clone(),
            trading_day: input.payload.trading_day.clone(),
            stage4_semantic_state: Stage8bP1eStage4SemanticStateV1 {
                boundary_proof: input.payload.stage4_evidence.boundary_proof.clone(),
                evidence_kind: input.payload.stage4_evidence.evidence_kind,
                schedule_state: input.payload.stage4_evidence.schedule_state,
            },
        })
        .unwrap();
    let plan_bytes = canonical(&plan).unwrap();
    write("/etc/moex-finam-p1-paper/supervisor.json", &config, 0o444);
    write(
        "/etc/moex-finam-p1-paper/o3/composition.json",
        &plan_bytes,
        0o444,
    );

    // Ephemeral guardian key for THIS disposable container. It is not the
    // schedule credential or a replacement/rebind of the production ceremony.
    let mut seed = zeroize::Zeroizing::new([0u8; 32]);
    fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(seed.as_mut())
        .unwrap();
    let key = SigningKey::from_bytes(&seed);
    let public = d::stage8b_p1f_authority_public_key_hex(&key);
    write(
        "/etc/moex-finam-p1-paper/o2/authority-public-key.hex",
        public.as_bytes(),
        0o440,
    );
    let nonce = hash(uuid::Uuid::new_v4().as_bytes());
    let mut digest = Sha256::new();
    digest.update(b"moex.stage8b.p1f.authority-genesis.v1\0");
    for part in [installation, d::STAGE8B_P1F_TARGET_HOST_ID, ROOT] {
        digest.update(part.as_bytes());
        digest.update([0]);
    }
    digest.update(1u64.to_be_bytes());
    digest.update(nonce.as_bytes());
    let head = format!("{:x}", digest.finalize());
    let genesis = d::Stage8bP1fGenesisManifestV1 {
        schema_version: 1,
        domain: "stage8b-p1f-genesis-manifest-v1".into(),
        installation_id: installation.into(),
        target_host_id: d::STAGE8B_P1F_TARGET_HOST_ID.into(),
        target_host_ssh_ed25519_sha256: d::STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256.into(),
        control_root: ROOT.into(),
        authority_generation: 1,
        ceremony_nonce_sha256: nonce.clone(),
        genesis_head_sha256: head.clone(),
        not_before_utc: phase_stamp(now),
        expires_at_utc: phase_stamp(deadline),
        issuer_key_id: KEY_ID.into(),
        signature_ed25519_hex: String::new(),
    };
    let signed = d::sign_stage8b_p1f_genesis_manifest_v1(genesis, &key).unwrap();
    let store = d::Stage8bP1fAuthorityStoreV1::open_production(987).unwrap();
    let receipt = store
        .initialize_authority(&signed, &public, KEY_ID, Utc::now())
        .unwrap();
    let activation = d::Stage8bP1fActivationCertificateV1 {
        schema_version: 1,
        domain: "stage8b-p1f-activation-certificate-v1".into(),
        installation_id: installation.into(),
        target_host_id: d::STAGE8B_P1F_TARGET_HOST_ID.into(),
        control_root: ROOT.into(),
        authority_generation: 1,
        ceremony_nonce_sha256: nonce,
        genesis_manifest_sha256: hash(&signed),
        genesis_receipt_sha256: hash(&serde_json::to_vec(&receipt).unwrap()),
        genesis_head_sha256: head,
        activated_at_utc: phase_stamp(now),
        expires_at_utc: phase_stamp(deadline),
        issuer_key_id: KEY_ID.into(),
        signature_ed25519_hex: String::new(),
    };
    store
        .activate_authority(
            &d::sign_stage8b_p1f_activation_certificate_v1(activation, &key).unwrap(),
            &public,
            KEY_ID,
            Utc::now(),
        )
        .unwrap();
    let head = store.inspect().unwrap();
    let phase = d::Stage8bP1fPhaseManifestV1 {
        schema_version: 1,
        domain: "stage8b-p1f-phase-manifest-v1".into(),
        installation_id: installation.into(),
        target_host_id: d::STAGE8B_P1F_TARGET_HOST_ID.into(),
        target_host_ssh_ed25519_sha256: d::STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256.into(),
        authority_generation: 1,
        authority_sequence: head.latest_sequence + 1,
        predecessor_event_sha256: head.latest_event_sha256,
        accepted_source_tree_sha256: hash(b"local-witness-not-operational"),
        phase: d::Stage8bP1fPhaseV1::O3SyntheticPaper,
        materialization_policy_sha256: hash(&plan_bytes),
        config_template_sha256: hash(&config),
        installation_sha256: hash(installation.as_bytes()),
        controller_id: "local-only".into(),
        not_before_utc: phase_stamp(now),
        deadline_utc: phase_stamp(deadline),
        issuer_key_id: KEY_ID.into(),
        signature_ed25519_hex: String::new(),
    };
    let bytes = d::sign_stage8b_p1f_phase_manifest_v1(phase, &key).unwrap();
    write(O3_PHASE_PATH, &bytes, 0o440);
    store
        .claim_o3_phase(&bytes, &public, KEY_ID, Utc::now())
        .unwrap();
    write("/run/o3-local-phase.sha256", hash(&bytes).as_bytes(), 0o444);
    println!("PASS isolated authority and exact fixed input preparation; operational=false");
}

fn request(after_restart: bool) {
    assert_eq!(unsafe { libc::geteuid() }, 987);
    let plan: O3CompositionPlan =
        serde_json::from_slice(&fs::read("/etc/moex-finam-p1-paper/o3/composition.json").unwrap())
            .unwrap();
    let publisher_input = input(&plan);
    assert_eq!(
        stage8b_p1f_synthetic_schedule_fixture_sha256(&plan.phase_id, &publisher_input).unwrap(),
        plan.schedule_fixture_sha256
    );
    assert!(O3UnixScheduleSigner.sign_raw_prehash(&[0u8; 32]).is_err());
    let state = prepare_stage8b_p1f_o3_synthetic_schedule(
        Stage8bP1fO3ScheduleInputV1 {
            phase_id: plan.phase_id,
            expected_synthetic_fixture_sha256: plan.schedule_fixture_sha256,
            publisher_input,
            trusted_now_utc: Utc::now(),
        },
        Stage8bP1eSchedulePublisherLineage::First(
            authorize_stage8b_p1e_first_publication("AUTHORIZE-STAGE8B-P1E-FIRST-PUBLICATION")
                .unwrap(),
        ),
        &O3UnixScheduleSigner,
    )
    .unwrap();
    let bytes = state.exact_envelope_bytes().unwrap();
    if !after_restart {
        persist_stage8b_p1e_schedule_publisher_state(
            Path::new("/run/o3-public/schedule-prepared.json"),
            &state,
        )
        .unwrap();
    }
    let mut unsigned: Stage8bP1eScheduleEnvelopeV3 = serde_json::from_slice(&bytes).unwrap();
    unsigned.signature_ed25519_hex.clear();
    let mut wrong_identity = unsigned.clone();
    wrong_identity.operational_identity_sha256 = "c".repeat(64);
    assert!(O3UnixScheduleSigner
        .sign_schedule_envelope(&wrong_identity)
        .is_err());
    unsigned.published_at_utc = stamp(Utc::now() - Duration::seconds(3));
    assert!(O3UnixScheduleSigner
        .sign_schedule_envelope(&unsigned)
        .is_err());
    for frame in [
        vec![0, 2, 0, 1],
        [64u32.to_be_bytes().to_vec(), vec![b'a'; 64]].concat(),
    ] {
        use std::os::unix::net::UnixStream;
        let mut socket = UnixStream::connect(O3_SCHEDULE_SIGNER_SOCKET).unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        socket.write_all(&frame).unwrap();
        let mut response = [0; 64];
        assert!(socket.read_exact(&mut response).is_err());
    }
    let verified = verify_stage8b_p1e_schedule_envelope_v3(
        &bytes,
        &Stage8bP1eScheduleVerificationContextV1 {
            expected_instrument_map_fingerprint_sha256: plan.instrument_map_fingerprint_sha256,
            expected_operational_identity_sha256: plan.operational_identity_sha256,
            expected_registry_identity_sha256: plan.schedule.registry_identity_sha256,
            expected_registry_version: plan.schedule.registry_version,
            expected_runtime_config_fingerprint_sha256: plan.runtime_config_fingerprint_sha256,
            high_water: None,
            trusted_now: Utc::now(),
        },
    )
    .unwrap();
    let _ = verified;
    // Public signed evidence only; never request or serialize private bytes.
    write(
        if after_restart {
            "/run/o3-public/signed-envelope-after-restart.json"
        } else {
            "/run/o3-public/signed-envelope.json"
        },
        &bytes,
        0o644,
    );
    println!("PASS real-clock generation-2 AF_UNIX signature and production verification; paper_outcome=false");
    println!("PASS foreign identity, stale envelope, raw digest and oversized frame rejected");
}

async fn replay_publish_and_read() {
    assert_eq!(unsafe { libc::geteuid() }, 987);
    let plan: O3CompositionPlan =
        serde_json::from_slice(&fs::read("/etc/moex-finam-p1-paper/o3/composition.json").unwrap())
            .unwrap();
    // The production preparation command, not this fixture example, retained
    // this checkpoint before the production initializer created its Redis row.
    let path = Path::new("/var/lib/moex-finam-p1-paper-o3-producer/schedule-high-water.json");
    let prepared = load_stage8b_p1e_schedule_publisher_state(path).unwrap();
    assert_eq!(
        prepared.phase(),
        Stage8bP1eSchedulePublisherPhaseV1::Prepared
    );
    let initial = prepared.exact_envelope_bytes().unwrap();
    let mut writer = Stage8bP1fSchedulePublisherRedisV1::connect(d::STAGE8B_P1E_REDIS_URL_IPV4)
        .await
        .unwrap();
    let published = publish_stage8b_p1e_prepared_schedule(path, prepared, &mut writer)
        .await
        .unwrap();
    assert_eq!(published.exact_envelope_bytes().unwrap(), initial);
    let next = prepare_stage8b_p1f_o3_synthetic_schedule(
        Stage8bP1fO3ScheduleInputV1 {
            phase_id: plan.phase_id.clone(),
            expected_synthetic_fixture_sha256: plan.schedule_fixture_sha256.clone(),
            publisher_input: input(&plan),
            trusted_now_utc: Utc::now(),
        },
        Stage8bP1eSchedulePublisherLineage::Resume(&published),
        &O3UnixScheduleSigner,
    )
    .unwrap();
    let next = publish_stage8b_p1e_prepared_schedule(path, next, &mut writer)
        .await
        .unwrap();
    let envelope: Stage8bP1eScheduleEnvelopeV3 =
        serde_json::from_slice(&next.exact_envelope_bytes().unwrap()).unwrap();
    assert_eq!(envelope.publication_sequence, "2");
    assert_eq!(envelope.semantic_revision, "1");
    let mut context = Stage8bP1eScheduleVerificationContextV1 {
        expected_instrument_map_fingerprint_sha256: plan.instrument_map_fingerprint_sha256,
        expected_operational_identity_sha256: plan.operational_identity_sha256,
        expected_registry_identity_sha256: plan.schedule.registry_identity_sha256,
        expected_registry_version: plan.schedule.registry_version,
        expected_runtime_config_fingerprint_sha256: plan.runtime_config_fingerprint_sha256,
        high_water: None,
        trusted_now: Utc::now(),
    };
    let latch = d::Stage8bP1eShutdownLatchV1::default();
    let mut reader = d::Stage8bP1eRedisScheduleReader::connect(d::STAGE8B_P1E_REDIS_URL_IPV4)
        .await
        .unwrap();
    let d::Stage8bP1eGuardedScheduleReadV1::Read(d::Stage8bP1eNewestScheduleReadV1::Verified(read)) =
        reader.read_newest_guarded(&context, &latch).await.unwrap()
    else {
        panic!("verified reader required")
    };
    assert_eq!(read.high_water().publication_sequence(), 2);
    assert_eq!(read.high_water().semantic_revision(), 1);
    let retained = serde_json::to_vec(read.high_water()).unwrap();
    write(
        "/run/o3-public/schedule-reader-high-water.json",
        &retained,
        0o600,
    );
    drop(reader);
    // Exact high-water retained across a new normal reader, no fixture trust.
    context.high_water = Some(
        serde_json::from_slice(
            &fs::read("/run/o3-public/schedule-reader-high-water.json").unwrap(),
        )
        .unwrap(),
    );
    context.trusted_now = Utc::now();
    let mut reader = d::Stage8bP1eRedisScheduleReader::connect(d::STAGE8B_P1E_REDIS_URL_IPV4)
        .await
        .unwrap();
    let d::Stage8bP1eGuardedScheduleReadV1::Read(d::Stage8bP1eNewestScheduleReadV1::Verified(read)) =
        reader.read_newest_guarded(&context, &latch).await.unwrap()
    else {
        panic!("verified restarted reader required")
    };
    assert_eq!(serde_json::to_vec(read.high_water()).unwrap(), retained);
    println!("PASS exact Prepared replay -> sequence 2/revision 1 -> ordinary Redis reader -> retained high-water readmission; paper_outcome=false");
}

fn main() {
    assert!(
        cfg!(target_os = "linux") && Path::new("/.dockerenv").exists(),
        "disposable Linux Docker only"
    );
    assert!(
        fs::read_to_string("/proc/net/route")
            .unwrap()
            .lines()
            .count()
            == 1,
        "network-none required"
    );
    // Harness passes only the two operations; no arbitrary paths or trust key.
    match std::env::args().nth(1).as_deref() {
        Some("prepare") => prepare(false),
        Some("prepare-linked") => prepare(true),
        Some("request") => request(false),
        Some("request-after-restart") => request(true),
        Some("replay-publish-and-read") => tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
            .block_on(replay_publish_and_read()),
        Some("wrong-peer") => {
            use std::os::unix::net::UnixStream;
            assert_eq!(unsafe { libc::geteuid() }, 0);
            let mut socket = UnixStream::connect(O3_SCHEDULE_SIGNER_SOCKET).unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut response = [0; 64];
            assert!(socket.read_exact(&mut response).is_err());
            println!("PASS connected root peer rejected; only publisher uid admitted");
        }
        _ => panic!("prepare|request"),
    }
}
