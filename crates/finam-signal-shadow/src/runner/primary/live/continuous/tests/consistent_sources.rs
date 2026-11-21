//! Actual managed socket, ingress, serial writer and reconnect routing; no provider.
use super::*;
#[test]
fn consistent_v5_stale_mixed_bootstrap_uses_bounded_recovery_without_retimestamping() {
    with_fixture_clock(T - 100, || {
        runtime(async {
            let (owner, _, root, path) = crate::runner::primary::live::tests::context_delivery(
                false,
                crate::primary::DeliveryPolicy::ObservedBoConsistentSourcesV5,
            );
            let (factory, mut peers, mut progress, calls) = setup().await;
            let release = Arc::new(tokio::sync::Notify::new());
            let fetch_release = release.clone();
            let fetch = move |job: Job, _stop: StopHandle| {
                let release = fetch_release.clone();
                async move {
                    assert!(matches!(&job, Job::Bootstrap(_)));
                    release.notified().await;
                    source(&job, vec![raw_row(T - 60000)])
                }
            };
            let task = tokio::spawn(drive(
                owner,
                DateTime::from_timestamp_millis(T - 3600000).unwrap(),
                factory,
                fetch,
            ));
            let _ack = progress.recv().await.unwrap();
            let mut peer = peers.remove(0);
            handshake(&mut peer).await;
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_subscription_sent")
            })
            .await;
            go(T + 100);
            peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[raw_row(T-60000),raw_row(T)]}}).to_string())).await.unwrap();
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_bootstrap_proof_required")
            })
            .await;
            go(T + 2201);
            release.notify_one();
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_episode_started")
            })
            .await;
            let _new_feedback = tokio::time::timeout(Duration::from_secs(5), progress.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            root.stop();
            let report = task.await.unwrap().unwrap();
            assert!(report.all_workers_drained);
            assert_eq!(report.canonical_commits, 0);
            assert_eq!(report.recovery.len(), 1);
            let rows = trace(&path);
            assert!(!rows.iter().any(|r| r["kind"] == "model_step"));
            let record = &rows
                .iter()
                .find(|r| r["event"]["event"] == "primary_bootstrap_proof_required")
                .unwrap()["event"];
            assert_eq!(record["original_received_at_ms"], T + 100);
            assert!(!rows
                .iter()
                .any(|r| r["event"]["event"] == "primary_bootstrap_proof_applied"));
        })
    });
}
