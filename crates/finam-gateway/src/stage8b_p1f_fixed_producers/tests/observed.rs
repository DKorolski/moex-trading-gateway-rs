use super::*;
use crate::stage8b_p1f_fixed_producers::observed::prepare_with_verifier;
use crate::stage8b_p1f_fixed_producers::observed::*;
use broker_core::observed_m1::OBSERVED_M1_POLICY_V1;
use broker_finam::sparse_m10::{
    AdmittedClosedM1Snapshot, ClosedM1RequestPlan, ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1,
};
use runtime_durable_service::{
    Stage8bP1RedisConfig, Stage8bP1fM10FeederRedisV1, Stage8bP1fRedisRoleV1,
};
use serde_json::json;

fn snapshot(close: DateTime<Utc>, minutes: &[i64]) -> AdmittedClosedM1Snapshot {
    let start = close - Duration::minutes(10);
    let bars: Vec<_> = minutes
        .iter()
        .map(|m| {
            json!({
                "timestamp":(start + Duration::minutes(*m)).to_rfc3339(),
                "open":{"value":"2200"},"high":{"value":"2201"},"low":{"value":"2199"},
                "close":{"value":"2200.5"},"volume":{"value":"1"}
            })
        })
        .collect();
    let raw_body = serde_json::to_string(&json!({"symbol":"IMOEXF@RTSX","bars":bars})).unwrap();
    let plan = ClosedM1RequestPlan::new(start, close, vec![close]).unwrap();
    AdmittedClosedM1Snapshot::admit(
        ClosedM1SnapshotEvidenceV1 {
            policy: OBSERVED_M1_POLICY_V1.into(),
            start,
            end: close,
            parts: vec![ClosedM1RestPartV1 {
                method: "GET".into(),
                endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
                symbol: "IMOEXF@RTSX".into(),
                timeframe: "TIME_FRAME_M1".into(),
                start,
                end: close,
                requested_at: close,
                received_at: close + Duration::seconds(1),
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(raw_body.len() as u64),
                response_sha256: sha256_hex(raw_body.as_bytes()),
                raw_body,
            }],
        },
        &plan,
        close + Duration::seconds(1),
    )
    .unwrap()
}

fn input(source: &AdmittedClosedM1Snapshot, o4: bool) -> Stage8bP1fObservedM10Input<'_> {
    Stage8bP1fObservedM10Input {
        phase: if o4 {
            Stage8bP1fProducerPhaseV1::O4FinamReadOnly
        } else {
            Stage8bP1fProducerPhaseV1::O3Synthetic
        },
        phase_id: if o4 { "o4-observed" } else { "o3-observed" }.into(),
        nominal_close: source.receipt().end(),
        trusted_now: source.receipt().received_at(),
        snapshot: source,
        expected_receipt_sha256: source.receipt().sha256(),
        expected_synthetic_snapshot_sha256: if o4 { None } else { Some(source.sha256()) },
    }
}

fn first() -> Stage8bP1fObservedM10Lineage<'static> {
    Stage8bP1fObservedM10Lineage::First(
        authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap(),
    )
}

fn prepare(
    input: Stage8bP1fObservedM10Input<'_>,
    schedule: &Stage8bP1eSchedulePublisherStateV1,
    signer: &FixtureSigner,
    lineage: Stage8bP1fObservedM10Lineage<'_>,
) -> Result<Stage8bP1fObservedM10Outcome, Stage8bP1fProducerErrorV1> {
    prepare_with_verifier(input, &"4".repeat(64), schedule, lineage, &|s, op, now| {
        s.test_verify_fresh_envelope_with_key(op, now, signer.public_key_ed25519_hex())
    })
}

fn new_state(outcome: Stage8bP1fObservedM10Outcome) -> Stage8bP1fObservedM10State {
    match outcome {
        Stage8bP1fObservedM10Outcome::Prepared(s) => s,
        _ => panic!("new Prepared expected"),
    }
}

#[tokio::test]
async fn observed_producer_strict_version_binding_schedule_and_pending_lineage() {
    let signer = FixtureSigner::new(91);
    let source = snapshot(timestamp(12, 10, 0), &[1, 4, 7]);
    let schedule = o3_published_schedule(source.receipt().received_at(), &signer).await;
    let state = new_state(prepare(input(&source, false), &schedule, &signer, first()).unwrap());
    assert_eq!(state.publication_sequence(), 1);
    assert!(matches!(
        prepare(
            input(&source, false),
            &schedule,
            &signer,
            Stage8bP1fObservedM10Lineage::Resume(&state)
        )
        .unwrap(),
        Stage8bP1fObservedM10Outcome::ReplayPrepared(_)
    ));
    let path = state_path("observed-binding");
    state.persist(&path).unwrap();
    assert!(load_stage8b_p1f_m10_producer_state(&path).is_err());
    assert_eq!(
        Stage8bP1fObservedM10State::load(&path, state.binding().clone())
            .unwrap()
            .canonical_bytes()
            .unwrap(),
        state.canonical_bytes().unwrap()
    );
    let changed = snapshot(timestamp(12, 10, 0), &[1, 7]);
    let changed_state =
        new_state(prepare(input(&changed, false), &schedule, &signer, first()).unwrap());
    assert!(Stage8bP1fObservedM10State::load(&path, changed_state.binding().clone()).is_err());
    assert!(prepare(
        input(&changed, false),
        &schedule,
        &signer,
        Stage8bP1fObservedM10Lineage::Resume(&state)
    )
    .is_err());
    let later = snapshot(timestamp(12, 20, 0), &[0, 8]);
    let later_schedule = o3_published_schedule(later.receipt().received_at(), &signer).await;
    assert!(matches!(
        prepare(
            input(&later, true),
            &later_schedule,
            &signer,
            Stage8bP1fObservedM10Lineage::Resume(&state)
        ),
        Err(Stage8bP1fProducerErrorV1::PreparedPublicationPending)
    ));
    assert!(prepare(input(&source, true), &schedule, &signer, first()).is_err());
    let mut missing_pin = input(&source, false);
    missing_pin.expected_synthetic_snapshot_sha256 = None;
    assert!(prepare(missing_pin, &schedule, &signer, first()).is_err());
    let mut stale = input(&source, false);
    stale.trusted_now += Duration::seconds(10);
    assert!(prepare(stale, &schedule, &signer, first()).is_err());
    let empty = snapshot(timestamp(12, 10, 0), &[]);
    assert!(prepare(input(&empty, false), &schedule, &signer, first()).is_err());
    let legacy = super::prepared(
        fixture_prepare_m10(
            &signer,
            o3_batch(
                observations(
                    timestamp(12, 0, 0),
                    source.receipt().received_at(),
                    MarketDataSourceKind::ReadOnlyPoll,
                    0,
                ),
                source.receipt().received_at(),
            ),
            &"4".repeat(64),
            &schedule,
            Stage8bP1fM10ProducerLineageV1::First(
                authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap(),
            ),
        )
        .unwrap(),
    );
    let legacy_path = path.with_file_name("legacy.json");
    persist_stage8b_p1f_m10_producer_state(&legacy_path, &legacy).unwrap();
    let before = fs::read(&legacy_path).unwrap();
    assert!(Stage8bP1fObservedM10State::load(&legacy_path, state.binding().clone()).is_err());
    assert!(state.persist(&legacy_path).is_err());
    assert_eq!(fs::read(&legacy_path).unwrap(), before);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[tokio::test]
async fn observed_producer_real_redis_response_loss_exact_replay_and_o3_o4_sequence() {
    let redis = RedisServer::start().await;
    runtime_durable_service::initialize_stage8b_p1_redis_namespace(
        &redis.url,
        Stage8bP1RedisConfig::paper_default_auto(),
    )
    .await
    .unwrap();
    let signer = FixtureSigner::new(91);
    let source = snapshot(timestamp(12, 10, 0), &[1, 4, 7]);
    let schedule = o3_published_schedule(source.receipt().received_at(), &signer).await;
    let state = new_state(prepare(input(&source, false), &schedule, &signer, first()).unwrap());
    let path = state_path("observed-response-loss");
    state.persist(&path).unwrap();
    let before = fs::read(&path).unwrap();
    let mut feeder = Stage8bP1fM10FeederRedisV1::connect_observed_local_evidence(
        &redis.url,
        Stage8bP1RedisConfig::paper_default_auto(),
        state.binding().clone(),
        Stage8bP1fRedisRoleV1::SyntheticM10Feeder,
    )
    .await
    .unwrap();
    // Real effect succeeds, but the producer never receives a completion and
    // therefore cannot persist Published. Reconstruct from disk on the next run.
    feeder
        .publish_and_reread_exact_m10(
            state.redis_id(),
            &state.canonical_bytes().unwrap(),
            state.binding().operational_identity_sha256(),
        )
        .await
        .unwrap();
    drop(feeder);
    assert_eq!(fs::read(&path).unwrap(), before);
    let disk_binding =
        runtime_durable_service::Stage8bP1ObservedM10Binding::restore_retained_receipt(
            path.parent().unwrap(),
            state.binding().operational_identity_sha256(),
            source.receipt().sha256(),
        )
        .unwrap();
    let restored = Stage8bP1fObservedM10State::load(&path, disk_binding).unwrap();
    let mut feeder = Stage8bP1fM10FeederRedisV1::connect_observed_local_evidence(
        &redis.url,
        Stage8bP1RedisConfig::paper_default_auto(),
        restored.binding().clone(),
        Stage8bP1fRedisRoleV1::SyntheticM10Feeder,
    )
    .await
    .unwrap();
    let published = publish_stage8b_p1f_observed_m10(&path, restored, &mut feeder)
        .await
        .unwrap();
    assert_eq!(published.phase(), Stage8bP1fM10ProducerPhaseV1::Published);
    assert_eq!(published.publication_sequence(), 1);
    assert!(state.persist(&path).is_err()); // stale Prepared cannot roll back Published
    assert!(feeder
        .audit_records()
        .iter()
        .any(|r| r.result
            == runtime_durable_service::Stage8bP1fRedisAuditResultV1::IdempotentExisting));
    assert!(matches!(
        prepare(
            input(&source, false),
            &schedule,
            &signer,
            Stage8bP1fObservedM10Lineage::Resume(&published)
        )
        .unwrap(),
        Stage8bP1fObservedM10Outcome::IdempotentPublished(_)
    ));
    let next = snapshot(timestamp(12, 20, 0), &[0, 8]);
    let next_schedule = o3_published_schedule(next.receipt().received_at(), &signer).await;
    let next = new_state(
        prepare(
            input(&next, true),
            &next_schedule,
            &signer,
            Stage8bP1fObservedM10Lineage::Resume(&published),
        )
        .unwrap(),
    );
    assert_eq!(next.publication_sequence(), 2);
    assert!(
        publish_stage8b_p1f_observed_m10(&path, next.clone(), &mut feeder)
            .await
            .is_err()
    ); // wrong role, no overwrite
    let mut next_feeder = Stage8bP1fM10FeederRedisV1::connect_observed_local_evidence(
        &redis.url,
        Stage8bP1RedisConfig::paper_default_auto(),
        next.binding().clone(),
        Stage8bP1fRedisRoleV1::FinamBarsFeeder,
    )
    .await
    .unwrap();
    let next = publish_stage8b_p1f_observed_m10(&path, next, &mut next_feeder)
        .await
        .unwrap();
    assert_eq!(next.phase(), Stage8bP1fM10ProducerPhaseV1::Published);
    let final_bytes = fs::read(&path).unwrap();
    assert!(published.persist(&path).is_err()); // sequence 2 cannot roll back to 1
    assert_eq!(fs::read(&path).unwrap(), final_bytes);
    assert_eq!(
        Stage8bP1fObservedM10State::load(&path, next.binding().clone())
            .unwrap()
            .publication_sequence(),
        2
    );
    // Published high-water now names the successor, but the prior pending
    // candle must still be recoverable from its original immutable receipt.
    let prior = runtime_durable_service::Stage8bP1ObservedM10Binding::restore_retained_receipt(
        path.parent().unwrap(),
        state.binding().operational_identity_sha256(),
        source.receipt().sha256(),
    )
    .unwrap();
    prior
        .parse_exact(
            &state.canonical_bytes().unwrap(),
            state.binding().operational_identity_sha256(),
        )
        .unwrap();
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[tokio::test]
async fn observed_producer_corrupt_retained_receipt_blocks_prepared_and_redis_effect() {
    let redis = RedisServer::start().await;
    let mut probe = runtime_durable_service::initialize_stage8b_p1_redis_namespace(
        &redis.url,
        Stage8bP1RedisConfig::paper_default_auto(),
    )
    .await
    .unwrap();
    let signer = FixtureSigner::new(91);
    let source = snapshot(timestamp(12, 10, 0), &[1, 4, 7]);
    let schedule = o3_published_schedule(source.receipt().received_at(), &signer).await;
    let state = new_state(prepare(input(&source, false), &schedule, &signer, first()).unwrap());
    let path = state_path("observed-receipt-refusal");
    let receipt_file = path.parent().unwrap().join(format!(
        "stage8b-observed-receipt-{}.json",
        source.receipt().sha256()
    ));
    fs::write(&receipt_file, b"corrupt").unwrap();
    let mut feeder = Stage8bP1fM10FeederRedisV1::connect_observed_local_evidence(
        &redis.url,
        Stage8bP1RedisConfig::paper_default_auto(),
        state.binding().clone(),
        Stage8bP1fRedisRoleV1::SyntheticM10Feeder,
    )
    .await
    .unwrap();
    let audit_before = feeder.audit_records().clone(); // verify-only attach is already recorded
    assert!(publish_stage8b_p1f_observed_m10(&path, state, &mut feeder)
        .await
        .is_err());
    assert!(!path.exists());
    assert_eq!(fs::read(&receipt_file).unwrap(), b"corrupt");
    assert_eq!(probe.retained_m10_count().await.unwrap(), 0);
    assert_eq!(feeder.audit_records(), &audit_before);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
