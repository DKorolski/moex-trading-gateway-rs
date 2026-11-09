use super::super::tests::{fixture, go, T};
use super::*;
use crate::{
    session::primary_tests::{raw_row, wire},
    timing::now_ms,
};
use broker_finam::sparse_m10::{
    ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1, OBSERVED_M1_POLICY_V1,
};
use serde_json::{json, Value};
mod resilience;

fn at(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap()
}
fn bridge(owner: &Coordinator) -> BootstrapBridge {
    BootstrapBridge::new(
        owner.processor.config_sha256().into(),
        1,
        at(T - 3600000),
        T + 3600000,
    )
    .unwrap()
}
fn source(ticket: &BootstrapTicket, rows: Vec<Value>) -> AdmittedClosedM1Snapshot {
    let start = ticket.plan.start();
    let end = ticket.plan.end();
    let now = at(now_ms());
    let raw = json!({"symbol":"IMOEXF@RTSX","bars":rows}).to_string();
    AdmittedClosedM1Snapshot::admit(
        ClosedM1SnapshotEvidenceV1 {
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
                requested_at: at(ticket.issued_at_ms),
                received_at: now,
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(raw.len() as u64),
                response_sha256: crate::seed::digest(raw.as_bytes()),
                raw_body: raw,
            }],
        },
        &ticket.plan,
        now,
    )
    .unwrap()
}
fn request(owner: &mut Coordinator, bridge: &mut BootstrapBridge) -> BootstrapTicket {
    go(T + 100);
    bridge
        .on_frame(owner, wire(T - 600000, T + 100))
        .unwrap()
        .unwrap()
}
fn no_model(path: &std::path::Path) {
    assert!(!std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .any(|line| { serde_json::from_str::<Value>(line).unwrap()["kind"] == "model_step" }));
}

#[test]
fn exact_reference_discards_history_without_readiness_then_fresh_data_qualifies() {
    fixture(false, |mut owner, path| {
        let mut b = bridge(&owner);
        let ticket = request(&mut owner, &mut b);
        assert_eq!(ticket.plan.start(), at(T - 3600000));
        assert_eq!(ticket.plan.end(), at(T));
        go(T + 150);
        let evidence = source(&ticket, vec![raw_row(T - 600000)]);
        b.complete(&mut owner, ticket, Ok(evidence)).unwrap();
        assert!(!b.pending());
        assert!(!owner.processor.subscription_data_ready());
        assert!(owner.processor.last_live_data().is_none());
        assert_eq!(owner.processor.cached_m1(), 0);
        go(T + 200);
        b.on_frame(&mut owner, wire(T, T + 200)).unwrap();
        assert_eq!(
            owner.processor.last_live_data().unwrap().received_at_ms,
            T + 200
        );
        no_model(&path);
    });
}
#[test]
fn delayed_verified_history_keeps_original_stamp_and_never_becomes_live() {
    fixture(false, |mut owner, path| {
        let mut b = bridge(&owner);
        let ticket = request(&mut owner, &mut b);
        let original_hash = b.pending.as_ref().unwrap().0.sha256().to_owned();
        go(T + 3100);
        let evidence = source(&ticket, vec![raw_row(T - 600000)]);
        b.complete(&mut owner, ticket, Ok(evidence)).unwrap();
        assert!(!owner.processor.subscription_data_ready());
        assert!(owner.processor.last_live_data().is_none());
        let trace = std::fs::read_to_string(&path).unwrap();
        assert!(trace.contains(&original_hash));
        assert!(trace.contains(&format!("\"original_received_at_ms\":{}", T + 100)));
        no_model(&path);
    });
}
#[test]
fn delayed_mixed_frame_does_not_retime_live_tail_or_apply_partial_frame() {
    fixture(false, |mut owner, path| {
        let mut b = bridge(&owner);
        go(T + 100);
        let text = json!({"type":"DATA","subscription_type":"BARS","subscription_key":"test-key",
            "payload":{"symbol":"IMOEXF@RTSX","bars":[raw_row(T-600000),raw_row(T)]}})
        .to_string();
        let frame = WireFrame::normalize(&text, 1, "test-key", at(T + 100)).unwrap();
        let ticket = b.on_frame(&mut owner, frame).unwrap().unwrap();
        go(T + 3100);
        let evidence = source(&ticket, vec![raw_row(T - 600000)]);
        assert_eq!(
            b.complete(&mut owner, ticket, Ok(evidence)),
            Err(CoordinatorError::Source(PrimaryError::Continuity))
        );
        assert_eq!(owner.processor.cached_m1(), 0);
        assert!(owner.processor.last_live_data().is_none());
        assert!(owner.processor.gap_unresolved());
        assert_eq!(owner.tick().unwrap_err(), CoordinatorError::Held);
        no_model(&path);
    });
}
#[test]
fn mismatching_reference_latches_owner_and_does_not_retry() {
    fixture(false, |mut owner, path| {
        let mut b = bridge(&owner);
        let ticket = request(&mut owner, &mut b);
        go(T + 150);
        let mut row = raw_row(T - 600000);
        row["volume"] = json!({"value":"3"});
        let evidence = source(&ticket, vec![row]);
        assert_eq!(
            b.complete(&mut owner, ticket, Ok(evidence)),
            Err(CoordinatorError::Source(PrimaryError::ReferenceConflict))
        );
        assert_eq!(
            b.on_frame(&mut owner, wire(T, T + 150)).err(),
            Some(CoordinatorError::Held)
        );
        assert_eq!(owner.tick().unwrap_err(), CoordinatorError::Held);
        no_model(&path);
    });
}
#[test]
fn reference_range_is_not_widened_to_the_received_rows() {
    fixture(false, |mut owner, path| {
        let mut b = bridge(&owner);
        go(T + 100);
        let ticket = b
            .on_frame(&mut owner, wire(T - 7200000, T + 100))
            .unwrap()
            .unwrap();
        assert_eq!(ticket.plan.start(), at(T - 3600000));
        go(T + 150);
        let evidence = source(&ticket, vec![raw_row(T - 600000)]);
        assert_eq!(
            b.complete(&mut owner, ticket, Ok(evidence)),
            Err(CoordinatorError::Source(PrimaryError::ReferenceUnproven))
        );
        assert!(owner.processor.gap_unresolved());
        no_model(&path);
    });
}
#[test]
fn failed_reference_is_terminal_for_this_bridge_and_never_second_get() {
    fixture(false, |mut owner, path| {
        let mut b = bridge(&owner);
        let ticket = request(&mut owner, &mut b);
        assert_eq!(
            b.complete(&mut owner, ticket, Err(RestFailure::Unavailable)),
            Err(CoordinatorError::Rest(RestFailure::Unavailable))
        );
        assert_eq!(
            b.on_frame(&mut owner, wire(T, T + 100)).err(),
            Some(CoordinatorError::Held)
        );
        assert!(owner.processor.gap_unresolved());
        no_model(&path);
    });
}
#[test]
fn foreign_ticket_does_not_consume_pending_proof() {
    fixture(false, |mut owner, path| {
        let mut b = bridge(&owner);
        let ticket = request(&mut owner, &mut b);
        let mut foreign = ticket.clone();
        foreign.owner = Arc::new(());
        assert_eq!(
            b.complete(&mut owner, foreign, Err(RestFailure::Timeout)),
            Err(CoordinatorError::Ticket)
        );
        assert!(b.pending());
        go(T + 150);
        let evidence = source(&ticket, vec![raw_row(T - 600000)]);
        b.complete(&mut owner, ticket, Ok(evidence)).unwrap();
        no_model(&path);
    });
}
