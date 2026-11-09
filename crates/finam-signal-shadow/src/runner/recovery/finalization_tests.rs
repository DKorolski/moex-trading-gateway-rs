//! Owner-level cancellation: exercise the same finalizer used by EOD, not
//! merely restore's own cooperative STOP branch. Network fixtures are localhost.
use super::{
    tests::{current, factory, peer, Peer},
    *,
};
use crate::{
    runner::{failure::Slot, finalize_workers, terminal_segment_reason, terminal_status},
    socket::{LivenessSnapshot, SocketFailure, SocketStage},
};
use serde_json::{json, Value};
use tokio::task::JoinSet;

fn original(code: SocketCode) -> TransportError {
    TransportError::Socket(SocketFailure {
        stage: SocketStage::Receive,
        code,
        timestamp_bounds: None,
        quote_time: None,
        quote_conflict: None,
    })
}
fn diagnostic() -> LivenessSnapshot {
    LivenessSnapshot {
        observed_at_ms: 120_000,
        connected_at_ms: 0,
        last_rx_at_ms: Some(100_000),
        last_live_bars_at_ms: Some(50_000),
        last_ping_at_ms: Some(105_000),
        last_matched_pong_at_ms: Some(90_000),
        rx_age_ms: 20_000,
        bars_data_age_ms: 70_000,
        pending_pong_age_ms: Some(15_000),
        timeout: None,
    }
}
async fn wait_stage(audit: &Audit, stage: &str) {
    tokio::time::timeout(Duration::from_secs(4), async {
        while audit.snapshot()["stage"] != stage {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

// Exercise a real candle session's canonical dedup/finalization too. This cold,
// Flat fixture is not a full operational EOD/readiness claim.
fn session_terminal(end: &Value, result: &Result<(), &'static str>) -> &'static str {
    let (mut session, snapshot, path) = crate::session::tests::composition_fixture_policy(true);
    let labels = [snapshot.evidence().start];
    let received = snapshot.evidence().parts[0].received_at.timestamp_millis();
    session.snapshot(&snapshot, &labels, received).unwrap();
    let once = std::fs::read(&path).unwrap();
    session.snapshot(&snapshot, &labels, received).unwrap();
    assert_eq!(once, std::fs::read(&path).unwrap());
    session.record_transport(end).unwrap();
    let reason = terminal_segment_reason(result);
    session.finish(reason).unwrap();
    let rows: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rows.iter().filter(|r| r["kind"] == "model_step").count(), 1);
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "hypothetical_candle_feedback")
            .count(),
        0
    );
    assert_eq!(rows.last().unwrap()["event"]["reason"], reason);
    std::fs::remove_file(path).unwrap();
    reason
}

fn evidence(name: &str, end: Value, result: &Result<(), &'static str>) {
    let reason = session_terminal(&end, result);
    // Optional test-only export links real Rust coordinator outputs to the
    // unchanged file-based EOD verifier. No such I/O exists in production.
    if let Some(dir) = std::env::var_os("CANDLE_RECOVERY_FINALIZATION_EVIDENCE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&json!({
                "case":name,"origin":"rust_owner_finalization_test",
                "runner_end":end,"segment_reason":reason,
                "canonical_model_steps":1,"duplicate_model_steps":0,
                "virtual_fills":0,"operational_evidence":false
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

async fn cancelled_at(stage: &'static str, code: SocketCode, interrupted: bool) {
    let (addr, _, server) = peer(vec![Peer::Quiet]).await;
    let (stop, f) = factory(addr);
    let audit = Audit::default();
    let worker_audit = audit.clone();
    let error = original(code);
    let expected = error.clone();
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
    let mut jobs = JoinSet::new();
    jobs.spawn({
        let stop = stop.clone();
        async move {
            let _guard = worker_audit.latch_fault(&error, 6, 7, Some(diagnostic()));
            if stage == "fault_observed" {
                std::future::pending::<()>().await;
            }
            let mut run = RunBudget::with_audit(worker_audit);
            restore(
                f,
                || async {
                    if stage == "auth" {
                        std::future::pending::<()>().await;
                    }
                    Ok(current(8))
                },
                stop,
                &tx,
                &mut run,
                7,
                Reason::classify(&error).unwrap(),
                Duration::from_secs(5),
            )
            .await?;
            Ok(())
        }
    });
    wait_stage(&audit, stage).await;
    assert_eq!(audit.unresolved_fault().unwrap().error, expected);
    let slot = Slot::default();
    // Simulate the real EOD owner's Ok branch with all canonical bars already
    // complete, or its explicit SIGINT branch. It then aborts/drains the worker.
    let mut result = if interrupted {
        Err("runner_interrupted")
    } else {
        Ok(())
    };
    assert!(
        finalize_workers(
            &mut jobs,
            &stop,
            &audit,
            &slot,
            &mut result,
            Duration::from_secs(1)
        )
        .await
    );
    assert!(stop.is_stopped());
    assert!(jobs.is_empty());
    assert_eq!(
        result,
        Err(if interrupted {
            "runner_interrupted"
        } else {
            "runner_bars_recovery_incomplete"
        })
    );
    let state = audit.snapshot();
    assert_eq!(state["stage"], "cancelled");
    assert_eq!(state["cancelled_during"], stage);
    assert_eq!(state["fault_resolved"], false);
    assert_eq!(state["resolved_generation"], Value::Null);
    assert_eq!(
        state["fault"]["error"],
        serde_json::to_value(&expected).unwrap()
    );
    assert_eq!(state["fault"]["liveness"]["bars_data_age_ms"], 70_000);
    let receipt = serde_json::to_value(slot.take()).unwrap();
    assert_eq!(receipt["code"], json!({"Socket":code}));
    assert_eq!(receipt["auth_generation"], 6);
    assert_eq!(receipt["connection_generation"], 7);
    assert_eq!(receipt["liveness"]["bars_data_age_ms"], 70_000);
    while let Ok(event) = rx.try_recv() {
        assert!(!matches!(event, Event::Snapshot(..)));
    }
    let name = format!(
        "{stage}-{}-{}",
        if code == SocketCode::Eof {
            "eof"
        } else {
            "close"
        },
        if interrupted { "stop" } else { "eod" }
    );
    evidence(
        &name,
        json!({
            "event":"runner_end","status":terminal_status(&result),
            "terminal_reason":terminal_status(&result),"workers_drained":true,
            "remaining_worker_tasks":jobs.len(),"bars_observe_failure":receipt,
            "bars_recovery":state
        }),
        &result,
    );
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn owner_eod_cancels_backoff_eof() {
    cancelled_at("backoff", SocketCode::Eof, false).await;
}
#[tokio::test]
async fn owner_eod_cancels_backoff_close() {
    cancelled_at("backoff", SocketCode::Close, false).await;
}
#[tokio::test]
async fn owner_eod_cancels_auth_eof() {
    cancelled_at("auth", SocketCode::Eof, false).await;
}
#[tokio::test]
async fn owner_eod_cancels_auth_close() {
    cancelled_at("auth", SocketCode::Close, false).await;
}
#[tokio::test]
async fn owner_eod_cancels_first_data_eof() {
    cancelled_at("first_data", SocketCode::Eof, false).await;
}
#[tokio::test]
async fn owner_eod_cancels_first_data_close() {
    cancelled_at("first_data", SocketCode::Close, false).await;
}
#[tokio::test]
async fn owner_eod_cancels_before_restore_first_poll() {
    cancelled_at("fault_observed", SocketCode::Eof, false).await;
}
#[tokio::test]
async fn owner_stop_drains_and_retains_original_fault() {
    cancelled_at("first_data", SocketCode::Close, true).await;
}

#[tokio::test]
async fn completed_fresh_data_recovery_does_not_poison_eod() {
    let (addr, _, server) = peer(vec![Peer::Fresh]).await;
    let (stop, f) = factory(addr);
    let audit = Audit::default();
    let worker_audit = audit.clone();
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
    let mut jobs = JoinSet::new();
    jobs.spawn({
        let stop = stop.clone();
        async move {
            let _guard = worker_audit.latch_fault(&original(SocketCode::Eof), 6, 7, None);
            let mut run = RunBudget::with_audit(worker_audit);
            let _restored = restore(
                f,
                || async { Ok(current(8)) },
                stop,
                &tx,
                &mut run,
                7,
                Reason::Eof,
                Duration::from_secs(1),
            )
            .await?;
            std::future::pending::<Result<(), &'static str>>().await
        }
    });
    wait_stage(&audit, "ready").await;
    let mut result = Ok(());
    let slot = Slot::default();
    assert!(
        finalize_workers(
            &mut jobs,
            &stop,
            &audit,
            &slot,
            &mut result,
            Duration::from_secs(1)
        )
        .await
    );
    assert_eq!(result, Ok(()));
    assert!(jobs.is_empty() && audit.unresolved_fault().is_none());
    assert!(slot.take().is_none());
    let state = audit.snapshot();
    assert_eq!(state["fault_resolved"], true);
    assert_eq!(state["resolved_generation"], 8);
    assert!(state["resolved_at_ms"].is_i64());
    assert_eq!(state["cancelled_during"], Value::Null);
    let mut successes = 0;
    while let Ok(event) = rx.try_recv() {
        assert!(!matches!(event, Event::Snapshot(..)));
        if matches!(event, Event::Liveness(ref v) if v["event"]=="bars_recovery_succeeded") {
            successes += 1;
        }
    }
    assert_eq!(successes, 1);
    evidence(
        "completed-before-eod",
        json!({
            "event":"runner_end","status":terminal_status(&result),
            "terminal_reason":terminal_status(&result),"workers_drained":true,
            "remaining_worker_tasks":0,"bars_observe_failure":null,"bars_recovery":state
        }),
        &result,
    );
    server.await.unwrap();
}

#[test]
fn old_guard_cannot_cancel_or_resolve_a_later_fault() {
    let audit = Audit::default();
    let first = audit.latch_fault(&original(SocketCode::Eof), 1, 1, None);
    assert!(!audit.resolve(first.id, 1));
    assert!(audit.resolve(first.id, 2));
    let second = audit.latch_fault(&original(SocketCode::Close), 2, 2, None);
    assert!(!audit.resolve(first.id, 3));
    drop(first);
    assert_eq!(audit.snapshot()["stage"], "fault_observed");
    assert_eq!(
        audit.unresolved_fault().unwrap().error,
        original(SocketCode::Close)
    );
    drop(second);
    assert_eq!(audit.snapshot()["stage"], "cancelled");
}

#[tokio::test]
async fn unlatched_restore_is_rejected_before_network() {
    let (stop, f) = factory("127.0.0.1:9".parse().unwrap());
    let (tx, _rx) = tokio::sync::mpsc::channel(128);
    assert!(matches!(
        restore(
            f,
            || async { panic!("no auth without a fault") },
            stop,
            &tx,
            &mut RunBudget::default(),
            1,
            Reason::Eof,
            Duration::from_secs(1)
        )
        .await,
        Err("runner_bars_recovery_unlatched")
    ));
}
