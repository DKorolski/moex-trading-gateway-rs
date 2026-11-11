use super::*;
use crate::{
    session::primary_tests::{raw_row, wire},
    timing::{advance_fixture_clock, now_ms, with_fixture_clock},
};
use broker_finam::sparse_m10::{
    ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1, OBSERVED_M1_POLICY_V1,
};
use serde_json::{json, Value};
use std::path::PathBuf;

/// One-shot, per-owner test seam after the actual durable waiting record.
/// Only the blocking writer's sample advances here; no production clock API.
pub(super) struct ProofWaitIo {
    elapsed_ms: u64,
    pub before_ms: std::sync::atomic::AtomicI64,
    pub after_ms: std::sync::atomic::AtomicI64,
    pub entered: std::sync::atomic::AtomicBool,
    pub release: std::sync::atomic::AtomicBool,
}
impl ProofWaitIo {
    pub fn new(elapsed_ms: u64) -> Self {
        Self {
            elapsed_ms,
            before_ms: 0.into(),
            after_ms: 0.into(),
            entered: false.into(),
            release: false.into(),
        }
    }
    pub fn advance_and_wait(&self, stop: &crate::transport::StopHandle) {
        use std::sync::atomic::Ordering::SeqCst;
        self.before_ms.store(now_ms(), SeqCst);
        advance_fixture_clock(self.elapsed_ms);
        self.after_ms.store(now_ms(), SeqCst);
        self.entered.store(true, SeqCst);
        let started = std::time::Instant::now();
        while !self.release.load(SeqCst) && !stop.is_stopped() {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(5),
                "test IO release missing"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

pub(super) const T: i64 = 1793678400000; // synthetic fixture time; not operational evidence
mod admitted_rest;
mod nonblocking_rest;
mod resilience;
mod ws_first;
fn at(ms: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_millis(ms).unwrap()
}
pub(super) fn go(ms: i64) {
    assert!(ms >= now_ms());
    advance_fixture_clock((ms - now_ms()) as u64);
}
pub(super) fn fixture<TOut>(audit: bool, work: impl FnOnce(Coordinator, PathBuf) -> TOut) -> TOut {
    with_fixture_clock(T - 100, || {
        let (session, config, _, path) = crate::session::tests::primary_fixture_mode(T, audit);
        let instrument = broker_core::InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: broker_core::Exchange::Moex,
            market: broker_core::Market::Futures,
        };
        let p = PrimaryProcessor::from_fixture(
            &config,
            instrument,
            (0..12).map(|i| T + i * 600000).collect(),
            1,
            T - 100,
        )
        .unwrap();
        work(Coordinator::new(p, session).unwrap(), path)
    })
}
fn read(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
fn window(owner: &mut Coordinator, label: i64) -> Tick {
    for i in 0..=10 {
        let stamp = label + i * 60000;
        if stamp + 100 > now_ms() {
            go(stamp + 100);
            owner.on_frame(wire(stamp, stamp + 100)).unwrap();
        }
    }
    go(label + 601100);
    owner.tick().unwrap()
}
fn source(ticket: &RestTicket, rows: Vec<Value>) -> AdmittedClosedM1Snapshot {
    let start = at(ticket.label_ms);
    let end = at(ticket.label_ms + 600000);
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
        &ticket.request_plan().unwrap(),
        now,
    )
    .unwrap()
}
fn proof(owner: &mut Coordinator, missing: Option<i64>) -> RestTicket {
    for i in 0..10 {
        if Some(i) == missing {
            continue;
        }
        let t = T + i * 60000;
        go(t + 100);
        owner.on_frame(wire(t, t + 100)).unwrap();
    }
    if missing.is_some() {
        go(T + 600100);
        owner.on_frame(wire(T + 600000, T + 600100)).unwrap();
    }
    go(T + 603000);
    let Tick::ProofRequired(ticket) = owner.tick().unwrap() else {
        panic!("proof expected")
    };
    ticket
}

#[test]
fn two_dense_actual_model_steps_need_no_regular_rest_ticket() {
    fixture(false, |mut owner, path| {
        for i in 0..2 {
            assert!(matches!(
                window(&mut owner, T + i * 600000),
                Tick::Published {
                    origin: "WS_LIVE",
                    ..
                }
            ));
            assert!(owner.take_audit().unwrap().is_none());
        }
        assert_eq!(owner.role_counts().proof_scheduled, 0);
        assert_eq!(owner.role_counts().audit_scheduled, 0);
        assert_eq!(owner.processor().counters().committed, 2);
        assert_eq!(
            read(&path)
                .iter()
                .filter(|r| r["kind"] == "model_step")
                .count(),
            2
        );
    });
}

#[test]
fn timely_sparse_proof_uses_same_model_entry_with_rest_origin() {
    fixture(true, |mut owner, path| {
        let ticket = proof(&mut owner, Some(1));
        assert_eq!(ticket.reason(), Some(ExceptionReason::SparseUnproven));
        go(T + 603050);
        let rows = (0..10)
            .filter(|i| *i != 1)
            .map(|i| raw_row(T + i * 60000))
            .collect();
        let response = source(&ticket, rows);
        assert!(matches!(
            owner.complete_proof(ticket, Ok(response)).unwrap(),
            Tick::Published {
                origin: "REST_EXCEPTION",
                ..
            }
        ));
        let trace = read(&path);
        let receipt = &trace
            .iter()
            .find(|r| r["kind"] == "canonical_source")
            .unwrap()["event"]["receipt"];
        assert_eq!(receipt["minute_bitmap"], 1021);
        assert_eq!(receipt["origin"], "REST_EXCEPTION");
        assert_eq!(
            trace.iter().filter(|r| r["kind"] == "model_step").count(),
            1
        );
    });
}

#[test]
fn proof_failure_is_not_retried_each_tick_and_never_advances_model() {
    fixture(true, |mut owner, path| {
        let ticket = proof(&mut owner, Some(1));
        let duplicate = ticket.clone();
        assert!(matches!(
            owner
                .complete_proof(ticket, Err(RestFailure::Timeout))
                .unwrap(),
            Tick::AwaitingProof
        ));
        for _ in 0..3 {
            assert!(matches!(owner.tick().unwrap(), Tick::AwaitingProof));
        }
        assert!(matches!(
            owner.complete_proof(duplicate, Err(RestFailure::Timeout)),
            Err(CoordinatorError::Ticket)
        ));
        assert_eq!(owner.role_counts().proof_scheduled, 1);
        assert_eq!(owner.processor().next_label_ms(), Some(T));
        assert!(!read(&path).iter().any(|r| r["kind"] == "model_step"));
        go(T + 611000);
        assert!(matches!(owner.tick().unwrap(), Tick::Degraded));
    });
}

#[test]
fn ws_wins_while_boundary_proof_is_in_flight_without_duplicate_callback() {
    fixture(true, |mut owner, path| {
        let ticket = proof(&mut owner, None);
        go(T + 603100);
        owner.on_frame(wire(T + 600000, T + 603100)).unwrap();
        go(T + 604100);
        assert!(matches!(
            owner.tick().unwrap(),
            Tick::Published {
                origin: "WS_LIVE",
                ..
            }
        ));
        go(T + 604200);
        let response = source(&ticket, (0..10).map(|i| raw_row(T + i * 60000)).collect());
        assert!(matches!(
            owner.complete_proof(ticket, Ok(response)).unwrap(),
            Tick::Waiting
        ));
        assert_eq!(
            read(&path)
                .iter()
                .filter(|r| r["kind"] == "model_step")
                .count(),
            1
        );
        assert_eq!(owner.processor().counters().committed, 1);
    });
}

#[test]
fn r2_rest_before_grace_is_retained_and_compared_before_one_ws_callback() {
    for mismatch in [false, true] {
        fixture(false, |mut owner, path| {
            let ticket = proof(&mut owner, None);
            go(T + 603100);
            owner.on_frame(wire(T + 600000, T + 603100)).unwrap();
            go(T + 603200);
            let mut rows: Vec<_> = (0..10).map(|i| raw_row(T + i * 60000)).collect();
            if mismatch {
                rows[0]["volume"] = json!({"value":"3"});
            }
            let response = source(&ticket, rows);
            assert!(matches!(
                owner.complete_proof(ticket, Ok(response)).unwrap(),
                Tick::Waiting
            ));
            assert_eq!(owner.processor().counters().committed, 0);
            assert!(matches!(owner.tick().unwrap(), Tick::Waiting));
            go(T + 604100);
            let result = owner.tick();
            if mismatch {
                assert!(matches!(
                    result,
                    Err(CoordinatorError::Source(PrimaryError::CanonicalConflict))
                ));
                assert!(owner.tick().is_err());
            } else {
                assert!(matches!(
                    result.unwrap(),
                    Tick::Published {
                        origin: "WS_LIVE",
                        ..
                    }
                ));
                assert!(matches!(owner.tick().unwrap(), Tick::Waiting));
            }
            let rows = read(&path);
            assert_eq!(
                rows.iter().filter(|r| r["kind"] == "model_step").count(),
                usize::from(!mismatch)
            );
            assert!(
                rows.iter().filter(|r| r["kind"] == "virtual_fill").count()
                    <= usize::from(!mismatch)
            );
            assert_eq!(owner.counts.proof_scheduled, 1);
            assert_eq!(owner.counts.proof_completed, 1);
        });
    }
}

#[test]
fn r2_sparse_reason_can_improve_to_boundary_without_another_get() {
    fixture(false, |mut owner, path| {
        for i in 0..9 {
            go(T + i * 60000 + 100);
            owner.on_frame(wire(T + i * 60000, now_ms())).unwrap();
        }
        go(T + 603000);
        let Tick::ProofRequired(ticket) = owner.tick().unwrap() else {
            panic!("proof")
        };
        assert_eq!(ticket.reason(), Some(ExceptionReason::SparseUnproven));
        go(T + 603100);
        owner.on_frame(wire(T + 540000, now_ms())).unwrap();
        go(T + 603200);
        let response = source(&ticket, (0..10).map(|i| raw_row(T + i * 60000)).collect());
        assert!(matches!(
            owner.complete_proof(ticket, Ok(response)).unwrap(),
            Tick::Published {
                origin: "REST_EXCEPTION",
                ..
            }
        ));
        assert_eq!(owner.counts.proof_scheduled, 1);
        let rows = read(&path);
        let receipt = &rows
            .iter()
            .find(|r| r["kind"] == "canonical_source")
            .unwrap()["event"]["receipt"];
        assert_eq!(receipt["reason"], "BoundaryUnproven");
    });
}

#[test]
fn r3_timer_tracks_actual_grace_and_uncertainty_without_relaxing_lmax() {
    fixture(false, |mut owner, _| {
        let ticket = proof(&mut owner, None);
        go(T + 603050);
        owner
            .complete_proof(ticket, Err(RestFailure::Unavailable))
            .unwrap();
        go(T + 603100);
        owner.on_frame(wire(T + 600000, now_ms())).unwrap();
        assert_eq!(owner.processor.next_action_ms(), Some(T + 604100));
        go(T + 604100);
        assert!(matches!(
            owner.tick().unwrap(),
            Tick::Published {
                origin: "WS_LIVE",
                ..
            }
        ));
    });
    fixture(false, |mut owner, _| {
        let _ticket = proof(&mut owner, None);
        go(T + 609000);
        owner.on_frame(wire(T + 600000, now_ms())).unwrap();
        assert_eq!(owner.processor.next_action_ms(), Some(T + 609901));
        go(T + 609901);
        assert!(matches!(owner.tick().unwrap(), Tick::Degraded));
        assert_eq!(owner.processor.counters().committed, 0);
    });
}

#[test]
fn r3_io_crossing_keeps_due_until_the_next_readiness_evaluation() {
    for elapsed in [1, 2, 5] {
        fixture(false, |mut owner, _| {
            let _ticket = proof(&mut owner, None);
            go(T + 603100);
            owner.on_frame(wire(T + 600000, now_ms())).unwrap();
            go(T + 604099);
            assert!(matches!(owner.tick().unwrap(), Tick::Waiting));
            go(T + 604099 + elapsed);
            // IO crossed due, but no readiness evaluation has consumed it.
            assert_eq!(owner.processor.next_action_ms(), Some(T + 604100));
            assert!(matches!(
                owner.tick().unwrap(),
                Tick::Published {
                    origin: "WS_LIVE",
                    ..
                }
            ));
            assert!(owner.processor.next_action_ms().unwrap() > now_ms());
            assert_eq!(owner.processor.counters().committed, 1);
        });
    }
}

#[test]
fn r3_processed_thresholds_do_not_spin_without_boundary_or_while_proof_is_pending() {
    fixture(false, |mut owner, _| {
        let _ticket = proof(&mut owner, None);
        go(T + 604101);
        for _ in 0..20 {
            assert!(matches!(owner.tick().unwrap(), Tick::AwaitingProof));
            assert_eq!(owner.processor.next_action_ms(), Some(T + 609901));
        }
        assert_eq!(owner.counts.proof_scheduled, 1);
        assert_eq!(owner.processor.counters().committed, 0);
    });
}

#[test]
fn unavailable_audit_does_not_stop_two_following_ws_m10() {
    fixture(true, |mut owner, path| {
        for i in 0..6 {
            window(&mut owner, T + i * 600000);
        }
        let ticket = owner.take_audit().unwrap().unwrap();
        assert!(owner.take_audit().unwrap().is_none());
        assert_eq!(
            owner
                .complete_audit(ticket, Err(RestFailure::Timeout))
                .unwrap(),
            AuditStatus::Unavailable
        );
        for i in 6..8 {
            assert!(matches!(
                window(&mut owner, T + i * 600000),
                Tick::Published {
                    origin: "WS_LIVE",
                    ..
                }
            ));
        }
        assert_eq!(owner.processor().counters().committed, 8);
        assert_eq!(owner.audit_status(), AuditStatus::Unavailable);
        assert_eq!(owner.role_counts().audit_scheduled, 1);
        assert_eq!(owner.role_counts().proof_scheduled, 0);
        assert_eq!(
            read(&path)
                .iter()
                .filter(|r| r["kind"] == "model_step")
                .count(),
            8
        );
    });
}

#[test]
fn pending_audit_never_blocks_the_canonical_writer() {
    fixture(true, |mut owner, _| {
        for i in 0..6 {
            window(&mut owner, T + i * 600000);
        }
        let _pending = owner.take_audit().unwrap().unwrap();
        assert!(matches!(
            window(&mut owner, T + 6 * 600000),
            Tick::Published {
                origin: "WS_LIVE",
                ..
            }
        ));
        assert!(owner.take_audit().unwrap().is_none());
        assert_eq!(owner.audit_status(), AuditStatus::Pending);
    });
}

#[test]
fn auth_denial_is_not_an_unavailable_audit() {
    fixture(true, |mut owner, _| {
        for i in 0..6 {
            window(&mut owner, T + i * 600000);
        }
        let ticket = owner.take_audit().unwrap().unwrap();
        assert!(matches!(
            owner.complete_audit(ticket, Err(RestFailure::Authentication)),
            Err(CoordinatorError::Rest(RestFailure::Authentication))
        ));
        assert!(matches!(owner.tick(), Err(CoordinatorError::Held)));
    });
}

#[test]
fn equal_m10_with_changed_minute_distribution_is_audit_conflict() {
    fixture(true, |mut owner, _| {
        for i in 0..6 {
            window(&mut owner, T + i * 600000);
        }
        let ticket = owner.take_audit().unwrap().unwrap();
        go(now_ms() + 50);
        let mut rows: Vec<_> = (0..10)
            .map(|i| raw_row(ticket.label_ms + i * 60000))
            .collect();
        rows[0]["volume"] = json!({"value":"3"});
        rows[1]["volume"] = json!({"value":"1"});
        let response = source(&ticket, rows);
        assert_eq!(
            owner.complete_audit(ticket, Ok(response)).unwrap(),
            AuditStatus::Conflict
        );
        assert!(matches!(owner.tick(), Err(CoordinatorError::Held)));
        assert_eq!(owner.processor().counters().committed, 6);
    });
}

#[test]
fn available_conflicting_proof_blocks_ws_before_actual_callback() {
    fixture(true, |mut owner, path| {
        let ticket = proof(&mut owner, None);
        go(T + 603100);
        owner.on_frame(wire(T + 600000, T + 603100)).unwrap();
        go(T + 604100);
        let mut rows: Vec<_> = (0..10).map(|i| raw_row(T + i * 60000)).collect();
        rows[0]["volume"] = json!({"value":"3"});
        let response = source(&ticket, rows);
        assert!(matches!(
            owner.complete_proof(ticket, Ok(response)),
            Err(CoordinatorError::Source(PrimaryError::CanonicalConflict))
        ));
        assert!(!read(&path).iter().any(|r| r["kind"] == "model_step"));
        assert!(matches!(owner.tick(), Err(CoordinatorError::Held)));
    });
}

#[test]
fn ws_primary_config_cannot_enter_legacy_rest_runner() {
    with_fixture_clock(T - 100, || {
        let (_session, primary, _, _path) = crate::session::tests::primary_fixture_at(T);
        let config = crate::ValidatedConfig::from_ws_primary(&primary).unwrap();
        let plan = crate::runner::RunPlan {
            config_sha256: config.fingerprint().into(),
            account_id: "fixture".into(),
            read_only_credentials: true,
            auth_policy: "provider_expiry_readonly_rtsx_v1".into(),
            eod_check_at: at(T + 600000),
            reconnect_after_data_frames: None,
            requests: vec![],
        };
        assert_eq!(plan.validate(&config), Err("runner_legacy_mode_required"));
    });
}
