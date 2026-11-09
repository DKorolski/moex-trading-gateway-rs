//! Actual managed async driver + local WS codec + stub HTTP executor. No FINAM.
use super::*;
use crate::{
    primary::calendar::tests::{Dir, Fixture},
    runner::primary::runtimes::tests as fleet,
};
use std::path::{Path, PathBuf};

async fn all_committed(paths: &[PathBuf], n: usize) {
    until(|| {
        paths.iter().all(|p| {
            fleet::trace(p)
                .iter()
                .filter(|r| r["event"]["event"] == "primary_runtime_cohort_barrier")
                .count()
                == n
        })
    })
    .await;
}
async fn feed_group(
    peer: &mut Peer,
    progress: &mut watch::Receiver<Option<LiveDataStamp>>,
    paths: &[PathBuf],
    base: i64,
    start: i64,
    end: i64,
    initial: usize,
) {
    for i in start..=end {
        let stamp = base + i * 60000 + 100;
        go(stamp);
        peer.send(data(stamp - 100)).await.unwrap();
        accepted(progress, stamp).await;
        if i > 0 && i % 10 == 0 {
            go(stamp + 1000);
            peer.send(data(stamp - 100)).await.unwrap();
            all_committed(paths, initial + ((i - start) / 10) as usize).await;
            progress.changed().await.unwrap();
            progress.borrow_and_update();
        }
    }
}
fn compare_members(paths: &[PathBuf], origins: &[&str]) {
    let model_rows: Vec<_> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let rows = fleet::trace(path);
            assert_eq!(rows.iter().filter(|r| r["kind"] == "seed").count(), 1);
            let bound = rows
                .iter()
                .find(|r| r["event"]["event"] == "primary_runtime_cohort_bound")
                .unwrap();
            assert_eq!(bound["event"]["runtime_index"], index);
            let actual: Vec<_> = rows
                .iter()
                .filter(|r| r["kind"] == "canonical_source")
                .map(|r| r["event"]["receipt"]["origin"].as_str().unwrap())
                .collect();
            assert_eq!(actual, origins);
            assert_eq!(fleet::count(path, "canonical_commit"), origins.len());
            rows.into_iter()
                .filter(|r| r["kind"] == "model_step")
                .map(|r| r["event"].clone())
                .collect::<Vec<_>>()
        })
        .collect();
    for rows in &model_rows[1..] {
        assert_eq!(rows, &model_rows[0]);
    }
}
fn emit(
    scenario: &str,
    f: &Fixture,
    paths: &[PathBuf],
    report: &RunReport,
    connections: usize,
    http: usize,
) {
    if let Some(base) = std::env::var_os("WS_PRIMARY_T38_EVIDENCE_DIR") {
        use std::{
            io::Write,
            sync::atomic::{AtomicU64, Ordering},
        };
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = Path::new(&base).join(format!(
            "run-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
        std::fs::create_dir(&dir).unwrap();
        for (i, path) in paths.iter().enumerate() {
            std::fs::copy(path, dir.join(format!("runtime-{i}.jsonl"))).unwrap();
        }
        let result = json!({"scenario":scenario,"runtime_count":paths.len(),"socket_factory_calls":connections,
            "stub_http_executor_calls":http,"single_shared_admission_frontier":true,
            "model_callbacks_per_runtime":report.canonical_commits,"report":report,
            "network":"LOCAL_WS_PEER_AND_STUB_HTTP_NOT_FINAM",
            "aggregate_cohort_eod_qualified":false,"aggregate_cohort_restart_qualified":false,
            "operational_qualified":false});
        for (name, value) in [
            ("result.json", result),
            ("config.json", serde_json::to_value(&f.config).unwrap()),
        ] {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir.join(name))
                .unwrap();
            file.write_all(&serde_json::to_vec_pretty(&value).unwrap())
                .unwrap();
            file.sync_all().unwrap();
        }
        println!("T38_EVIDENCE_DIR={}", dir.display());
    }
}

#[test]
fn managed_shared_feed_uses_one_socket_one_proof_one_audit_for_three_models() {
    let mut f = Fixture::new();
    f.config.primary.audit_every_commits = Some(6);
    let d = Dir::new();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let t = f.open();
            let (mut owner, root, paths) = fleet::group(&f, &d, 3);
            let calendar = f.cap(crate::timing::now_ms()).unwrap();
            let reference = calendar.reference_start_ms();
            owner.bind_calendar(calendar).unwrap();
            let (factory, mut peers, mut progress, connections) = setup().await;
            let calls = Arc::new(AtomicUsize::new(0));
            let count = calls.clone();
            let fetcher = move |job, stop| {
                count.fetch_add(1, Ordering::SeqCst);
                fetch(job, stop)
            };
            let task = tokio::spawn(drive(
                owner,
                DateTime::from_timestamp_millis(reference).unwrap(),
                factory,
                fetcher,
            ));
            let mut p = progress.recv().await.unwrap();
            handshake(&mut peers[0]).await;
            until(|| {
                fleet::trace(&paths[0])
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_subscription_sent")
            })
            .await;
            feed_group(&mut peers[0], &mut p, &paths, t, 0, 60, 0).await;
            all_committed(&paths, 6).await;
            until(|| {
                fleet::trace(&paths[0]).iter().any(|r| {
                    r["event"]["event"] == "primary_audit_result" && r["event"]["status"] == "Match"
                })
            })
            .await;
            for i in 61..70 {
                go(t + i * 60000 + 100);
                peers[0].send(data(t + i * 60000)).await.unwrap();
                accepted(&mut p, t + i * 60000 + 100).await;
            }
            go(t + 70 * 60000 + 3100);
            peers[0].send(data(t + 69 * 60000)).await.unwrap();
            all_committed(&paths, 7).await;
            // Barrier rows precede the common frontier by design. This next
            // newly observed minute is processed by the SAME blocking writer
            // after the proof operation; its accepted DATA ack is our drain
            // rendezvous. Do not STOP while only per-consumer fsync is known.
            go(t + 70 * 60000 + 3101);
            peers[0].send(data(t + 70 * 60000)).await.unwrap();
            accepted(&mut p, t + 70 * 60000 + 3101).await;
            root.stop();
            let report = task.await.unwrap().unwrap();
            assert!(report.all_workers_drained, "{report:?}");
            assert!(matches!(report.cause, Cause::RequestedStop), "{report:?}");
            assert_eq!(report.canonical_commits, 7);
            assert_eq!(connections.load(Ordering::SeqCst), 1);
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert_eq!(report.http.proof.known_get_send_attempts, 1);
            assert_eq!(report.http.audit.known_get_send_attempts, 1);
            assert_eq!(report.http.bootstrap.scheduled, 0);
            assert_eq!(report.http.recovery.scheduled, 0);
            assert!(!report.full_session_qualified);
            assert!(!report.completion.as_ref().unwrap().semantic_eod_passed);
            compare_members(
                &paths,
                &[
                    "WS_LIVE",
                    "WS_LIVE",
                    "WS_LIVE",
                    "WS_LIVE",
                    "WS_LIVE",
                    "WS_LIVE",
                    "REST_EXCEPTION",
                ],
            );
            emit("dense-proof-audit", &f, &paths, &report, 1, 2);
        })
    });
}

#[test]
fn managed_cohort_reconnect_repairs_once_then_resumes_each_model_without_duplicates() {
    let f = Fixture::new();
    let d = Dir::new();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let t = f.open();
            let (mut owner, root, paths) = fleet::group(&f, &d, 3);
            let calendar = f.cap(crate::timing::now_ms()).unwrap();
            let reference = calendar.reference_start_ms();
            owner.bind_calendar(calendar).unwrap();
            let (factory, mut peers, mut progress, connections) = setup().await;
            let calls = Arc::new(AtomicUsize::new(0));
            let count = calls.clone();
            let fetcher = move |job, stop| {
                count.fetch_add(1, Ordering::SeqCst);
                fetch(job, stop)
            };
            let task = tokio::spawn(drive(
                owner,
                DateTime::from_timestamp_millis(reference).unwrap(),
                factory,
                fetcher,
            ));
            let mut p = progress.recv().await.unwrap();
            handshake(&mut peers[0]).await;
            until(|| {
                fleet::trace(&paths[0])
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_subscription_sent")
            })
            .await;
            feed_group(&mut peers[0], &mut p, &paths, t, 0, 20, 0).await;
            all_committed(&paths, 2).await;
            go(t + 3 * 600000 + 100);
            peers[0].close(None).await.unwrap();
            let mut p2 = tokio::time::timeout(Duration::from_secs(5), progress.recv())
                .await
                .unwrap()
                .unwrap();
            handshake(&mut peers[1]).await;
            until(|| {
                fleet::trace(&paths[0]).iter().any(|r| {
                    r["event"]["event"] == "primary_subscription_sent"
                        && r["event"]["generation"] == 2
                })
            })
            .await;
            go(t + 3 * 600000 + 200);
            peers[1].send(data(t + 3 * 600000)).await.unwrap();
            accepted(&mut p2, t + 3 * 600000 + 200).await;
            until(|| {
                fleet::trace(&paths[0])
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
            })
            .await;
            all_committed(&paths, 3).await;
            p2.changed().await.unwrap();
            p2.borrow_and_update();
            for i in 31..=40 {
                go(t + i * 60000 + 100);
                peers[1].send(data(t + i * 60000)).await.unwrap();
                accepted(&mut p2, t + i * 60000 + 100).await;
            }
            go(t + 4 * 600000 + 3100);
            peers[1].send(data(t + 4 * 600000)).await.unwrap();
            all_committed(&paths, 4).await;
            p2.changed().await.unwrap();
            p2.borrow_and_update();
            feed_group(&mut peers[1], &mut p2, &paths, t, 41, 50, 5).await;
            all_committed(&paths, 5).await;
            root.stop();
            let report = task.await.unwrap().unwrap();
            assert!(report.all_workers_drained, "{report:?}");
            assert!(matches!(report.cause, Cause::RequestedStop), "{report:?}");
            assert_eq!(report.canonical_commits, 5);
            assert_eq!(connections.load(Ordering::SeqCst), 2);
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert_eq!(report.http.recovery.known_get_send_attempts, 1);
            assert_eq!(report.http.proof.known_get_send_attempts, 1);
            assert_eq!(report.recovery.len(), 1);
            assert!(
                !report.recovery[0].transport_unresolved
                    && !report.recovery[0].canonical_unresolved
            );
            assert!(!report.full_session_qualified);
            compare_members(
                &paths,
                &[
                    "WS_LIVE",
                    "WS_LIVE",
                    "RECOVERY_BACKFILL",
                    "REST_EXCEPTION",
                    "WS_LIVE",
                ],
            );
            emit("forced-close-gap", &f, &paths, &report, 2, 2);
        })
    });
}
