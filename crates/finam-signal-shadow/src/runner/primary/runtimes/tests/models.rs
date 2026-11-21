//! Bounded source composition only. Synthetic caps, no network/credentials.
use super::*;
use crate::primary::{models_bridge::ImoexfModelBridge, DeliveryPolicy, WsPrimaryConfig};
use crate::socket::primary::Subscription;
const T: i64 = 1_793_592_000_000; // 02 November, 07:00 MSK

fn manifest() -> finam_shadow_models::local::contract::Manifest {
    crate::primary::tests::models_bridge::manifest()
}
fn create(d: &Dir) -> (Coordinator, StopHandle, PathBuf, WsPrimaryConfig) {
    let (session, config, _, path) = crate::session::tests::primary_fixture_delivery_input_limit(
        T,
        false,
        DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8,
        4 * 1024 * 1024,
    );
    let root = session.primary_runtime().unwrap().1;
    let processor = PrimaryProcessor::from_fixture(
        &config,
        crate::primary::calendar::tests::instrument(),
        (0..101).map(|i| T + i * 600000).collect(),
        1,
        T - 100,
    )
    .unwrap();
    let models =
        ImoexfModelBridge::create(&d.0.join("models.sqlite"), manifest(), &config).unwrap();
    (
        Coordinator::new_with_imoexf_models(processor, session, models).unwrap(),
        root,
        path,
        config,
    )
}
fn bind(owner: &mut Coordinator) -> Subscription {
    let sub = crate::socket::primary::fixture_subscription(1, T - 100);
    owner.bind_subscription(&sub).unwrap();
    sub
}
fn ingest(owner: &mut Coordinator, sub: &Subscription, label: i64) {
    let mut frame = crate::session::primary_tests::wire(label, now_ms());
    frame.owner = Some(sub.owner.clone());
    owner.on_frame(frame).unwrap();
}
fn dense(owner: &mut Coordinator, sub: &Subscription) {
    let label = owner.processor().next_label_ms().unwrap();
    for i in if now_ms() > label + 100 { 1 } else { 0 }..=10 {
        go(label + i * 60000 + 100);
        ingest(owner, sub, label + i * 60000);
        ingest(owner, sub, label + i * 60000); // retransmissions are not callbacks
    }
    go(label + 601100);
}
fn exported(owner: &Coordinator) -> Value {
    owner.imoexf_model_export().unwrap().unwrap()
}

#[test]
fn v8_models_shared_canonical_commit_and_duplicate_are_two_arm_only() {
    let d = Dir::new();
    with_fixture_clock(T, || {
        let (mut owner, root, path, config) = create(&d);
        let sub = bind(&mut owner);
        let initial = exported(&owner);
        assert_eq!(
            initial["states"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|s| s["arm"].as_str().unwrap().starts_with("IMOEXF/") && s["ready"] == true)
                .count(),
            2
        );
        for slot in 0..2 {
            dense(&mut owner, &sub);
            assert!(matches!(
                owner.tick().unwrap(),
                Tick::Published {
                    origin: "WS_LIVE",
                    ..
                }
            ));
            let e = exported(&owner);
            assert_eq!(e["revision"], slot + 1);
            assert_eq!(e["results"].as_array().unwrap().len(), (slot + 1) * 2);
            assert!(matches!(owner.tick().unwrap(), Tick::Waiting));
            assert_eq!(exported(&owner), e);
        }
        assert!(!root.is_stopped());
        assert_eq!(owner.processor().counters().committed, 2);
        let e = exported(&owner);
        for (before, after) in initial["states"]
            .as_array()
            .unwrap()
            .iter()
            .zip(e["states"].as_array().unwrap())
        {
            if !before["arm"].as_str().unwrap().starts_with("IMOEXF/") {
                assert_eq!(before, after);
            }
        }
        let rows = trace(&path);
        let bound = rows
            .iter()
            .find(|r| r["event"]["event"] == "primary_runtime_cohort_bound")
            .unwrap();
        assert_eq!(
            bound["event"]["manifest_sha256"],
            manifest().sha256().unwrap()
        );
        assert_eq!(count(&path, "canonical_commit"), 2);
        assert_eq!(
            rows.iter()
                .filter(|r| r["event"]["event"] == "primary_imoexf_models_commit")
                .count(),
            2
        );
        assert!(!owner.session.eod_model_ready());
        assert!(owner.session.journal_seal().is_none());
        // Even a valid old observer journal cannot attest the SQL cohort.
        owner
            .session
            .finish("BOUNDED_MODELS_NOT_AGGREGATE_EOD")
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mut successor = config.clone();
        successor.base.previous_segment_id = Some(config.base.segment_id.clone());
        successor.base.segment_id = "models-successor".into();
        let required: Vec<_> = (0..101).map(|i| T + i * 600000).collect();
        let binding = crate::primary::restart::RestartBinding {
            schema_version: 1,
            config_sha256: successor.fingerprint().unwrap(),
            previous_config_sha256: config.fingerprint().unwrap(),
            previous_journal_sha256: digest(&bytes),
            previous_segment_id: config.base.segment_id,
            required_labels_sha256: crate::primary::restart::inventory_sha256(&required),
            committed_labels: 2,
            max_reconstruction_ms: 10000,
        };
        assert!(matches!(
            crate::primary::restart::admit(&successor, binding, &bytes, &required, now_ms()),
            Err(crate::primary::restart::RestartError::Incomplete)
        ));
        std::fs::remove_file(path).unwrap();
    });
}

#[test]
fn v8_models_rest_exception_retains_exact_sparse_source() {
    let d = Dir::new();
    with_fixture_clock(T, || {
        let (mut owner, _, path, _) = create(&d);
        let sub = bind(&mut owner);
        for i in [0, 2, 9] {
            go(T + i * 60000 + 100);
            ingest(&mut owner, &sub, T + i * 60000);
        }
        go(T + 601100);
        let Tick::ProofRequired(ticket) = owner.tick().unwrap() else {
            panic!("proof")
        };
        let source = crate::primary::tests::reference(
            [0, 2, 9]
                .iter()
                .map(|i| crate::session::primary_tests::raw_row(T + i * 60000))
                .collect(),
            T,
            T + 600000,
            now_ms(),
        );
        let raw = serde_json::to_value(source.evidence()).unwrap();
        assert!(matches!(
            owner.complete_proof(ticket, Ok(source)).unwrap(),
            Tick::Published {
                origin: "REST_EXCEPTION",
                ..
            }
        ));
        let e = exported(&owner);
        assert_eq!(e["revision"], 1);
        let c = &e["inputs"][0]["candle"];
        assert_eq!(c["construction"]["provenance"]["rest_evidence"], raw);
        assert_eq!(
            c["construction"]["provenance"]["canonical_receipt"]["actual_m1"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(c["start_ms"], T);
        std::fs::remove_file(path).unwrap();
    });
}

#[test]
fn v8_models_ws_historical_uses_native_owner_and_no_rest_invention() {
    let d = Dir::new();
    with_fixture_clock(T, || {
        let (mut owner, _, path, _) = create(&d);
        let sub = bind(&mut owner);
        for i in 0..10 {
            go(T + i * 60000 + 100);
            ingest(&mut owner, &sub, T + i * 60000);
        }
        go(T + 601364);
        ingest(&mut owner, &sub, T + 600000);
        go(T + 601904);
        assert!(matches!(owner.tick().unwrap(), Tick::Degraded));
        owner.latch_data_recovery().unwrap();
        go(T + 602364);
        assert!(owner.try_ws_recovery(1, T + 646904, 1).unwrap());
        assert_eq!(owner.processor().counters().recovery_committed, 1);
        let e = exported(&owner);
        assert_eq!(e["revision"], 1);
        let c = &e["inputs"][0]["candle"];
        let p = &c["construction"]["provenance"];
        assert_eq!(p["canonical_receipt"]["origin"], "WS_RECOVERY_BACKFILL");
        assert!(p["rest_evidence"].is_null());
        assert_eq!(
            p["recovery_receipt"]["versions"].as_array().unwrap().len(),
            10
        );
        assert_eq!(
            p["source_receipt_sha256"],
            digest(p["source_receipt_json"].as_str().unwrap().as_bytes())
        );
        assert_eq!(
            p["canonical_receipt_sha256"],
            digest(p["canonical_receipt_json"].as_str().unwrap().as_bytes())
        );
        assert_eq!(c["received_ms"], T + 601364);
        assert_eq!(c["source_processed_ms"], T + 602364);
        assert_eq!(e["observed_clean_session"], false);
        assert!(!owner.try_ws_recovery(1, T + 646904, 1).unwrap());
        std::fs::remove_file(path).unwrap();
    });
}

#[test]
fn v8_models_post_sql_deadline_or_stop_never_advances_or_retries() {
    for (delay, stop) in [(900, false), (0, true)] {
        let d = Dir::new();
        with_fixture_clock(T, || {
            let (mut owner, root, path, _) = create(&d);
            let sub = bind(&mut owner);
            dense(&mut owner, &sub);
            owner.session.barrier_fault = Some((delay, stop));
            assert!(owner.tick().is_err());
            let e = exported(&owner);
            assert_eq!(e["revision"], 1);
            assert_eq!(e["results"].as_array().unwrap().len(), 2);
            assert_eq!(owner.processor().counters().committed, 0);
            assert_eq!(owner.processor().next_label_ms(), Some(T));
            assert!(root.is_stopped());
            assert!(matches!(owner.tick(), Err(CoordinatorError::Held)));
            assert_eq!(exported(&owner), e);
            std::fs::remove_file(path).unwrap();
        });
    }
}

#[test]
fn v8_models_nonempty_or_wrong_source_attachment_is_rejected_without_callback() {
    let d = Dir::new();
    with_fixture_clock(T, || {
        let (mut owner, _, path, config) = create(&d);
        let sub = bind(&mut owner);
        dense(&mut owner, &sub);
        owner.tick().unwrap();
        drop(owner);
        let models =
            ImoexfModelBridge::open(&d.0.join("models.sqlite"), manifest(), &config).unwrap();
        let native = PrimaryProcessor::from_fixture(
            &config,
            crate::primary::calendar::tests::instrument(),
            (0..101).map(|i| T + i * 600000).collect(),
            1,
            T - 100,
        )
        .unwrap();
        assert!(models.validate_frontier(&native).is_err());
        assert_eq!(models.export().unwrap()["revision"], 1);
        std::fs::remove_file(path).unwrap();
    });
}

#[test]
fn v8_models_rest_historical_batch_preserves_clock_and_source_evidence() {
    let d = Dir::new();
    with_fixture_clock(T, || {
        let (mut owner, _, path, _) = create(&d);
        let sub = bind(&mut owner);
        dense(&mut owner, &sub);
        owner.tick().unwrap();
        go(T + 1800100);
        owner.latch_recovery().unwrap();
        let plan = owner
            .issue_recovery_request(
                T + 1800000,
                crate::primary::RecoveryRequestScope {
                    episode: 1,
                    ordinal: 1,
                    deadline_ms: now_ms() + 45000,
                },
                None,
            )
            .unwrap();
        let source = crate::primary::tests::reference(
            (plan.request().start().timestamp_millis()..plan.request().end().timestamp_millis())
                .step_by(60000)
                .map(crate::session::primary_tests::raw_row)
                .collect(),
            plan.request().start().timestamp_millis(),
            plan.request().end().timestamp_millis(),
            now_ms(),
        );
        let source_sha = source.sha256().to_string();
        let raw_sha = digest(&serde_json::to_vec(source.evidence()).unwrap());
        assert!(matches!(
            owner.complete_recovery_request(&plan, &source).unwrap(),
            RecoveryProgress::Ready(2)
        ));
        assert_eq!(owner.processor().counters().committed, 3);
        assert_eq!(owner.processor().counters().recovery_committed, 2);
        let e = exported(&owner);
        assert_eq!(e["revision"], 3);
        assert_eq!(e["results"].as_array().unwrap().len(), 6);
        for row in &e["inputs"].as_array().unwrap()[1..] {
            let c = &row["candle"];
            let p = &c["construction"]["provenance"];
            assert_eq!(p["canonical_receipt"]["origin"], "RECOVERY_BACKFILL");
            assert_eq!(p["rest_evidence"]["snapshot_sha256"], source_sha);
            assert_eq!(p["rest_evidence"]["evidence_sha256"], raw_sha);
            assert_eq!(p["recovery_receipt"]["recovery_plan_sha256"], plan.sha256());
            assert_eq!(c["received_ms"], now_ms());
            assert_eq!(c["source_processed_ms"], now_ms());
            assert_eq!(
                p["source_receipt_sha256"],
                digest(p["source_receipt_json"].as_str().unwrap().as_bytes())
            );
        }
        assert!(matches!(
            owner.complete_recovery_request(&plan, &source).unwrap(),
            RecoveryProgress::Retired
        ));
        assert_eq!(exported(&owner), e);
        std::fs::remove_file(path).unwrap();
    });
}

#[test]
fn v8_models_sql_failure_is_atomic_and_stops_shared_writer_without_retry() {
    let d = Dir::new();
    with_fixture_clock(T, || {
        let (mut owner, root, path, _) = create(&d);
        let before = exported(&owner);
        let status=std::process::Command::new("python3").arg("-c")
            .arg("import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute(\"CREATE TRIGGER reject_arm BEFORE INSERT ON outcomes BEGIN SELECT RAISE(ABORT, 'synthetic failure'); END\"); c.commit()")
            .arg(d.0.join("models.sqlite")).status().unwrap();
        assert!(status.success());
        let sub = bind(&mut owner);
        dense(&mut owner, &sub);
        assert!(owner.tick().is_err());
        assert!(root.is_stopped());
        assert_eq!(owner.processor().counters().committed, 0);
        assert_eq!(owner.processor().next_label_ms(), Some(T));
        assert_eq!(exported(&owner), before);
        assert!(matches!(owner.tick(), Err(CoordinatorError::Held)));
        std::fs::remove_file(path).unwrap();
    });
}

#[test]
fn v8_models_fresh_binding_rejects_wrong_freeze_policy_or_source_before_sql() {
    let d = Dir::new();
    with_fixture_clock(T, || {
        let (owner, _, path, config) = create(&d);
        drop(owner);
        let mut shifted = config.clone();
        shifted.base.observation_start_utc_ms += 600000;
        assert!(
            ImoexfModelBridge::create(&d.0.join("bad-freeze.sqlite"), manifest(), &shifted)
                .is_err()
        );
        assert!(!d.0.join("bad-freeze.sqlite").exists());
        let mut policy = config.clone();
        policy.primary.delivery = DeliveryPolicy::ObservedBoDataWaitRecoveryV7;
        assert!(
            ImoexfModelBridge::create(&d.0.join("bad-policy.sqlite"), manifest(), &policy).is_err()
        );
        assert!(!d.0.join("bad-policy.sqlite").exists());
        let models =
            ImoexfModelBridge::open(&d.0.join("models.sqlite"), manifest(), &config).unwrap();
        let mut foreign = config.clone();
        foreign.base.run_id = "different-run".into();
        let source = PrimaryProcessor::from_fixture(
            &foreign,
            crate::primary::calendar::tests::instrument(),
            vec![T],
            1,
            T - 100,
        )
        .unwrap();
        assert!(models.validate_frontier(&source).is_err());
        let source = PrimaryProcessor::from_fixture(
            &config,
            crate::primary::calendar::tests::instrument(),
            vec![T + 600000],
            1,
            T - 100,
        )
        .unwrap();
        assert!(models.validate_frontier(&source).is_err());
        assert_eq!(models.export().unwrap()["revision"], 0);
        std::fs::remove_file(path).unwrap();
    });
}

#[test]
fn v8_models_maximum_rest_gap_keeps_each_projection_bounded_and_raw_retained_once() {
    let d = Dir::new();
    with_fixture_clock(T, || {
        let (mut owner, _, path, _) = create(&d);
        let sub = bind(&mut owner);
        dense(&mut owner, &sub);
        owner.tick().unwrap();
        let cutover = T + 37 * 600000; // committed 07:00 + maximum six-hour gap
        go(cutover + 100);
        owner.latch_recovery().unwrap();
        let plan = owner
            .issue_recovery_request(
                cutover,
                crate::primary::RecoveryRequestScope {
                    episode: 1,
                    ordinal: 1,
                    deadline_ms: now_ms() + 45000,
                },
                None,
            )
            .unwrap();
        let source = crate::primary::tests::reference(
            (plan.request().start().timestamp_millis()..cutover)
                .step_by(60000)
                .map(crate::session::primary_tests::raw_row)
                .collect(),
            plan.request().start().timestamp_millis(),
            cutover,
            now_ms(),
        );
        assert!(
            serde_json::to_vec(source.evidence()).unwrap().len() > 65536,
            "this regression must exceed the per-candle bound before hash reference"
        );
        assert!(matches!(
            owner.complete_recovery_request(&plan, &source).unwrap(),
            RecoveryProgress::Ready(36)
        ));
        assert_eq!(owner.processor().counters().committed, 37);
        let e = exported(&owner);
        assert_eq!(e["revision"], 37);
        assert_eq!(e["results"].as_array().unwrap().len(), 74);
        for row in e["inputs"].as_array().unwrap() {
            assert!(serde_json::to_vec(&row["candle"]).unwrap().len() <= 65536);
        }
        assert_eq!(count(&path, "source_snapshot"), 1);
        assert_eq!(count(&path, "canonical_commit"), 37);
        std::fs::remove_file(path).unwrap();
    });
}

#[test]
fn v8_models_not_ready_context_cannot_attach_as_successful_empty_outcomes() {
    let d = Dir::new();
    with_fixture_clock(T, || {
        let (owner, _, path, config) = create(&d);
        drop(owner);
        let mut unready = manifest();
        unready
            .seeds
            .iter_mut()
            .find(|s| s.binding.instrument == "IMOEXF")
            .unwrap()
            .history
            .truncate(101);
        let models =
            ImoexfModelBridge::create(&d.0.join("unready.sqlite"), unready, &config).unwrap();
        let source = PrimaryProcessor::from_fixture(
            &config,
            crate::primary::calendar::tests::instrument(),
            (0..101).map(|i| T + i * 600000).collect(),
            1,
            T - 100,
        )
        .unwrap();
        assert!(models
            .validate_frontier(&source)
            .unwrap_err()
            .to_string()
            .contains("contexts must be ready"));
        assert_eq!(models.export().unwrap()["revision"], 0);
        std::fs::remove_file(path).unwrap();
    });
}
