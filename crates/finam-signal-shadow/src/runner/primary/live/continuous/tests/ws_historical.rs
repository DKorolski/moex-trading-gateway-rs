//! V8 synthetic incident-shaped continuation. The real result proves only the
//! pre-readiness snapshot, NOT these later cache/REST/network developments.
use super::*;
use crate::primary::restart::{RestartBinding, RestartError};
use crate::primary::{DeliveryPolicy, WsPrimaryConfig};

fn restart_check(
    config: &WsPrimaryConfig,
    rows: &[Value],
    committed: usize,
) -> Result<(), RestartError> {
    let mut successor = config.clone();
    successor.base.previous_segment_id = Some(config.base.segment_id.clone());
    successor.base.segment_id = "v8-synthetic-successor".into();
    let bytes: Vec<u8> = rows
        .iter()
        .flat_map(|r| {
            let mut b = serde_json::to_vec(r).unwrap();
            b.push(b'\n');
            b
        })
        .collect();
    let required: Vec<_> = (0..12).map(|i| T + i * 600000).collect();
    let binding = RestartBinding {
        schema_version: 1,
        config_sha256: successor.fingerprint().unwrap(),
        previous_config_sha256: config.fingerprint().unwrap(),
        previous_journal_sha256: crate::seed::digest(&bytes),
        previous_segment_id: config.base.segment_id.clone(),
        required_labels_sha256: crate::primary::restart::inventory_sha256(&required),
        committed_labels: committed,
        max_reconstruction_ms: 10000,
    };
    crate::primary::restart::admit(&successor, binding, &bytes, &required, T + 1840000).map(|_| ())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Winner {
    Ws,
    Rest,
    Near,
    Stop,
    ExhaustWs,
    ExhaustNone,
    Deadline,
}
async fn incident(winner: Winner) {
    let delivery = DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8;
    let (session, config, _, path) = crate::session::tests::primary_fixture_delivery_input_limit(
        T,
        false,
        delivery,
        4 * 1024 * 1024,
    );
    let root = session.primary_runtime().unwrap().1;
    let instrument = crate::session::primary_tests::wire(T, T + 100).rows[0]
        .bar
        .instrument
        .clone();
    let processor = crate::primary::PrimaryProcessor::from_config(
        &config,
        instrument,
        (0..12).map(|i| T + i * 600000).collect(),
        1,
        T - 100,
    )
    .unwrap();
    let owner = Coordinator::new(processor, session).unwrap();
    let (factory, mut peers, mut progress, calls) = setup().await;
    let reads = Arc::new(AtomicUsize::new(0));
    let count = reads.clone();
    let fetch = move |job: Job, _stop: StopHandle| {
        let count = count.clone();
        async move {
            let plan = job.plan().unwrap();
            let mut rows: Vec<_> = (plan.start().timestamp_millis()..plan.end().timestamp_millis())
                .step_by(60000)
                .map(raw_row)
                .collect();
            if matches!(job, Job::Recovery(_)) {
                count.fetch_add(1, Ordering::SeqCst);
                if winner == Winner::Near {
                    tokio::time::sleep(Duration::from_millis(470)).await;
                } else if winner == Winner::Ws {
                    // A sent GET reply is drained after WS selection, not dropped.
                    tokio::time::sleep(Duration::from_millis(650)).await;
                }
                if !matches!(winner, Winner::Rest | Winner::Near) {
                    rows.truncate(22);
                }
            } else {
                rows.truncate(2);
            }
            source(&job, rows)
        }
    };
    let task = tokio::spawn(drive(
        owner,
        DateTime::from_timestamp_millis(T - 3600000).unwrap(),
        factory,
        fetch,
    ));
    let mut ack = progress.recv().await.unwrap();
    handshake(&mut peers[0]).await;
    until(|| {
        trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_subscription_sent")
    })
    .await;
    feed(&mut peers[0], &mut ack, &path, 0, 29, 0).await;
    let close = T + 1800000;
    go(close + 1000);
    peers[0].send(data(close - 60000)).await.unwrap();
    until(|| {
        trace(&path).iter().any(|r| {
            r["event"]["event"] == "primary_rest_observation"
                && r["event"]["label_ms"] == close - 600000
        })
    })
    .await;
    if !matches!(
        winner,
        Winner::ExhaustWs | Winner::ExhaustNone | Winner::Deadline
    ) {
        go(close + 1364);
        peers[0].send(data(close)).await.unwrap();
        accepted(&mut ack, close + 1364).await;
    }
    go(close + 1904);
    peers[0].send(data(close - 60000)).await.unwrap();
    until(|| reads.load(Ordering::SeqCst) > 0).await;
    let _clock = crate::timing::run_fixture_clock();
    if winner == Winner::Stop {
        root.stop();
    } else if matches!(
        winner,
        Winner::ExhaustWs | Winner::ExhaustNone | Winner::Deadline
    ) {
        tokio::time::timeout(Duration::from_secs(12), async {
            while reads.load(Ordering::SeqCst) < 4 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            !root.is_stopped(),
            "four GETs do not terminate V8 data-wait"
        );
        if winner == Winner::ExhaustWs {
            peers[0].send(data(close)).await.unwrap();
        } else if winner == Winner::Deadline {
            tokio::time::pause();
            go(close + 47000);
            tokio::time::advance(Duration::from_secs(45)).await;
            tokio::time::resume();
        } else {
            root.stop();
        }
    }
    let success = !matches!(
        winner,
        Winner::Stop | Winner::ExhaustNone | Winner::Deadline
    );
    if success {
        until(|| {
            trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
        })
        .await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "healthy socket must never reconnect"
        );
        root.stop();
    }
    let report = tokio::time::timeout(Duration::from_secs(7), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(report.all_workers_drained, "{report:?}");
    assert_eq!(
        report.canonical_commits,
        if success { 3 } else { 2 },
        "{report:?}"
    );
    let rows = trace(&path);
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "model_step").count(),
        if success { 3 } else { 2 }
    );
    assert_eq!(
        reads.load(Ordering::SeqCst),
        if matches!(
            winner,
            Winner::ExhaustWs | Winner::ExhaustNone | Winner::Deadline
        ) {
            4
        } else {
            1
        }
    );
    if winner == Winner::Deadline {
        assert!(
            matches!(
                report.cause,
                Cause::Recovery(reconnect::Error::NoFreshData | reconnect::Error::EpisodeDeadline)
            ),
            "{report:?}"
        );
    }
    if success {
        let event = rows
            .iter()
            .find(|r| {
                r["kind"] == "canonical_source"
                    && r["event"]["receipt"]["label_ms"] == close - 600000
            })
            .unwrap();
        let r = &event["event"]["receipt"];
        if matches!(winner, Winner::Ws | Winner::ExhaustWs) {
            assert_eq!(r["origin"], "WS_RECOVERY_BACKFILL");
            assert!(r["canonical_ready_at_ms"].as_i64().unwrap() >= close + 2364);
            assert_eq!(r["processing_mode"], "BACKFILL_REPLAY");
            assert!(rows
                .iter()
                .any(|r| r["event"]["event"] == "primary_ws_historical_source_selected"));
        } else if winner == Winner::Rest {
            assert_eq!(r["origin"], "RECOVERY_BACKFILL");
        }
        restart_check(&config, &rows, 3).unwrap();
        if r["origin"] == "WS_RECOVERY_BACKFILL" {
            let old_hash = event["event"]["source_sha256"].as_str().unwrap().to_owned();
            for field in [
                "generation",
                "boundary_received_at_ms",
                "canonical_ready_at_ms",
                "data_gap_at_ms",
                "continuity_revision",
                "origin",
            ] {
                let mut corrupted = rows.clone();
                let receipt = &mut corrupted
                    .iter_mut()
                    .find(|r| {
                        r["kind"] == "canonical_source" && r["event"]["source_sha256"] == old_hash
                    })
                    .unwrap()["event"]["receipt"];
                receipt[field] = match field {
                    "generation" => json!(2),
                    "boundary_received_at_ms" => json!(close + 9000),
                    "canonical_ready_at_ms" => json!(close + 1364),
                    "data_gap_at_ms" => json!(close + 1900),
                    "continuity_revision" => json!(999),
                    "origin" => json!("WS_LIVE"),
                    _ => unreachable!(),
                };
                let new_hash = crate::primary::restart::journal::test_ws_hash(receipt.clone());
                for row in &mut corrupted {
                    for key in ["source_sha256", "raw_provenance_sha256"] {
                        if row["event"][key] == old_hash {
                            row["event"][key] = json!(new_hash);
                        }
                    }
                }
                assert!(
                    restart_check(&config, &corrupted, 3).is_err(),
                    "resealed mutation {field}"
                );
            }
        }
    }
    let _ = std::fs::remove_file(path);
}
#[test]
fn v8_driver_incident_ws_wins_and_late_rest_reply_is_drained_once() {
    with_fixture_clock(T - 100, || runtime(incident(Winner::Ws)));
}
#[test]
fn v8_driver_rest_wins_before_ws_readiness_once() {
    with_fixture_clock(T - 100, || runtime(incident(Winner::Rest)));
}
#[test]
fn v8_driver_near_simultaneous_sources_publish_once() {
    with_fixture_clock(T - 100, || runtime(incident(Winner::Near)));
}
#[test]
fn v8_driver_stop_before_readiness_never_publishes() {
    with_fixture_clock(T - 100, || runtime(incident(Winner::Stop)));
}
#[test]
fn v8_driver_four_get_cap_preserves_bounded_ws_path() {
    with_fixture_clock(T - 100, || runtime(incident(Winner::ExhaustWs)));
}
#[test]
fn v8_driver_four_get_cap_then_stop_has_no_fifth_get_or_callback() {
    with_fixture_clock(T - 100, || runtime(incident(Winner::ExhaustNone)));
}
#[test]
fn v8_driver_four_get_cap_no_source_expires_at_original_episode_deadline() {
    with_fixture_clock(T - 100, || runtime(incident(Winner::Deadline)));
}
