//! Opt-in precommit REST / eligible WS-first. Synthetic clocks/model context;
//! the retained provider REST body + reported WS pair do not reconstruct WS RX.
use super::*;
use crate::primary::{DeliveryPolicy, WsPrimaryConfig};

const NEW: DeliveryPolicy = DeliveryPolicy::ObservedBoRestPrecommitWsFirstV2;

pub(super) fn candidate(
    label: i64,
    subscribed: i64,
    policy: DeliveryPolicy,
    work: impl FnOnce(Coordinator, PathBuf),
) {
    with_fixture_clock(label - 100, || {
        let (session, config, _, path) =
            crate::session::tests::primary_fixture_delivery(label, true, policy);
        let instrument = broker_core::InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: broker_core::Exchange::Moex,
            market: broker_core::Market::Futures,
        };
        let p = PrimaryProcessor::from_fixture(
            &config,
            instrument,
            (0..12).map(|i| label + i * 600000).collect(),
            1,
            subscribed,
        )
        .unwrap();
        work(Coordinator::new(p, session).unwrap(), path);
    });
}

pub(super) fn send(o: &mut Coordinator, row: Value, received: i64) -> String {
    go(received);
    let frame = crate::primary::WireFrame::normalize(
        &json!({"type":"DATA","subscription_type":"BARS",
        "subscription_key":"test-key","payload":{"symbol":"IMOEXF@RTSX","bars":[row]}})
        .to_string(),
        1,
        "test-key",
        at(received),
    )
    .unwrap();
    let hash = frame.sha256().to_owned();
    o.on_frame(frame).unwrap();
    hash
}

pub(super) fn startup(o: &mut Coordinator, boundary: bool) -> RestTicket {
    for i in 0..10 {
        send(o, raw_row(T + i * 60000), T + i * 60000 + 200);
    }
    if boundary {
        send(o, raw_row(T + 600000), T + 600100);
    }
    go(T + 601000);
    let Tick::ProofRequired(ticket) = o.tick().unwrap() else {
        panic!("one proof")
    };
    assert_eq!(ticket.reason(), Some(ExceptionReason::StartupPartial));
    ticket
}

pub(super) fn changes(label: i64) -> Vec<Value> {
    (0..10)
        .map(|i| {
            let mut r = raw_row(label + i * 60000);
            if i == 9 {
                r["low"] = json!({"value":"2199.5"});
                r["close"] = json!({"value":"2200"});
            }
            r
        })
        .collect()
}

#[test]
fn ws_first_policy_is_explicit_and_old_fingerprints_unchanged() {
    let raw = crate::primary::tests::config();
    let old = WsPrimaryConfig::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let mut new = raw.clone();
    new["primary"]["delivery"] = json!(NEW);
    let new = WsPrimaryConfig::parse(&serde_json::to_vec(&new).unwrap()).unwrap();
    assert_ne!(old.fingerprint().unwrap(), new.fingerprint().unwrap());
    assert_ne!(
        old.primary.fingerprint().unwrap(),
        new.primary.fingerprint().unwrap()
    );
    assert_eq!(old.base.model_sha256, new.base.model_sha256);
    assert_eq!(
        old.base.feedback_policy_sha256,
        new.base.feedback_policy_sha256
    );
    assert_eq!(
        serde_json::to_value(old).unwrap()["primary"].get("delivery"),
        None
    );
}

#[test]
fn ws_first_timely_startup_rest_selects_whole_bucket_once_not_mixed_prices() {
    for boundary in [false, true] {
        candidate(T, T + 100, NEW, |mut o, path| {
            let ticket = startup(&mut o, boundary);
            let duplicate = ticket.clone();
            go(T + 601050);
            let source = source(&ticket, changes(T));
            let hash = source.sha256().to_owned();
            assert!(matches!(
                o.complete_proof(ticket, Ok(source)).unwrap(),
                Tick::Published {
                    origin: "REST_EXCEPTION",
                    ..
                }
            ));
            assert_eq!(o.processor().counters().committed, 1);
            assert!(o
                .complete_proof(duplicate, Err(RestFailure::Timeout))
                .is_err());
            let trace = read(&path);
            assert_eq!(
                trace.iter().filter(|r| r["kind"] == "model_step").count(),
                1
            );
            let canonical = trace
                .iter()
                .find(|r| r["kind"] == "canonical_source")
                .unwrap();
            let receipt = &canonical["event"]["receipt"];
            assert_eq!(receipt["rest_snapshot_sha256"], hash);
            assert_eq!(receipt["actual_m1"][9]["low"], "2199.5");
            assert_eq!(receipt["actual_m1"][9]["close"], "2200");
            let select = trace
                .iter()
                .position(|r| r["event"]["event"] == "primary_precommit_rest_selected")
                .unwrap();
            let model = trace
                .iter()
                .position(|r| r["kind"] == "model_step")
                .unwrap();
            assert!(select < model);
            let change = trace
                .iter()
                .find(|r| r["event"]["event"] == "primary_source_change")
                .unwrap();
            assert_eq!(
                change["event"]["change"]["expected_ws_version"]["last_changed_at_ms"],
                T + 540200
            );
            assert_eq!(change["event"]["change"]["model_input_rewritten"], false);
        });
    }
}

#[test]
fn ws_first_old_observed_policy_still_holds_same_price_conflict() {
    candidate(
        T,
        T + 100,
        DeliveryPolicy::ObservedBo1sVolumeAuditV1,
        |mut o, path| {
            let ticket = startup(&mut o, true);
            go(T + 601050);
            let response = source(&ticket, changes(T));
            assert!(matches!(
                o.complete_proof(ticket, Ok(response)),
                Err(CoordinatorError::Source(PrimaryError::CanonicalConflict))
            ));
            assert_eq!(o.processor().counters().committed, 0);
            assert!(!read(&path).iter().any(|r| r["kind"] == "model_step"));
        },
    );
}

#[test]
fn ws_first_missing_known_minute_is_not_price_priority() {
    candidate(T, T + 100, NEW, |mut o, path| {
        let ticket = startup(&mut o, true);
        go(T + 601050);
        let rows = (0..10)
            .filter(|i| *i != 5)
            .map(|i| raw_row(T + i * 60000))
            .collect();
        let response = source(&ticket, rows);
        assert!(o.complete_proof(ticket, Ok(response)).is_err());
        assert_eq!(o.processor().counters().committed, 0);
        assert!(!read(&path).iter().any(|r| r["kind"] == "model_step"));
    });
}

#[test]
fn ws_first_no_eligible_ws_never_mints_late_callback() {
    for delay in [1900, 1901, 3000] {
        candidate(T, T + 100, NEW, |mut o, path| {
            let ticket = startup(&mut o, true);
            go(T + 600000 + delay);
            let response = source(&ticket, changes(T));
            assert!(matches!(
                o.complete_proof(ticket, Ok(response)).unwrap(),
                Tick::AwaitingProof
            ));
            assert_eq!(o.processor().counters().committed, 0);
            assert!(!read(&path).iter().any(|r| r["kind"] == "model_step"));
            if delay > 1900 {
                assert!(matches!(o.tick().unwrap(), Tick::Degraded));
            }
        });
    }
}

#[test]
fn ws_first_rest_just_before_deadline_is_eligible_but_never_retimes_source() {
    candidate(T, T + 100, NEW, |mut o, path| {
        let ticket = startup(&mut o, true);
        go(T + 601899);
        let response = source(&ticket, changes(T));
        assert!(matches!(
            o.complete_proof(ticket, Ok(response)).unwrap(),
            Tick::Published {
                origin: "REST_EXCEPTION",
                ..
            }
        ));
        let trace = read(&path);
        assert_eq!(
            trace.iter().filter(|r| r["kind"] == "model_step").count(),
            1
        );
        let source = trace
            .iter()
            .find(|r| r["kind"] == "canonical_source")
            .unwrap();
        assert_eq!(
            source["event"]["receipt"]["proof_received_at_ms"],
            T + 601899
        );
    });
}

#[test]
fn ws_first_sparse_proof_is_not_general_price_override() {
    candidate(T, T - 100, NEW, |mut o, path| {
        for i in 0..9 {
            send(&mut o, raw_row(T + i * 60000), T + i * 60000 + 100);
        }
        send(&mut o, raw_row(T + 600000), T + 600100);
        go(T + 601000);
        let Tick::ProofRequired(ticket) = o.tick().unwrap() else {
            panic!("proof")
        };
        assert_eq!(ticket.reason(), Some(ExceptionReason::SparseUnproven));
        go(T + 601050);
        let mut rows: Vec<_> = (0..10).map(|i| raw_row(T + i * 60000)).collect();
        rows[0]["low"] = json!({"value":"2199.5"});
        let response = source(&ticket, rows);
        assert!(matches!(
            o.complete_proof(ticket, Ok(response)),
            Err(CoordinatorError::Source(PrimaryError::CanonicalConflict))
        ));
        assert!(!read(&path).iter().any(|r| r["kind"] == "model_step"));
    });
}

pub(super) fn ws_overtakes(o: &mut Coordinator) -> RestTicket {
    for i in 0..9 {
        send(o, raw_row(T + i * 60000), T + i * 60000 + 100);
    }
    send(o, raw_row(T + 600000), T + 600100);
    go(T + 601000);
    let Tick::ProofRequired(ticket) = o.tick().unwrap() else {
        panic!("one sparse proof")
    };
    assert_eq!(ticket.reason(), Some(ExceptionReason::SparseUnproven));
    send(o, raw_row(T + 540000), T + 601200);
    assert!(matches!(
        o.tick().unwrap(),
        Tick::Published {
            origin: "WS_LIVE",
            ..
        }
    ));
    ticket
}

#[test]
fn ws_first_complete_ws_overtakes_pending_rest_timeout_and_continues() {
    candidate(T, T - 100, NEW, |mut o, path| {
        let ticket = ws_overtakes(&mut o);
        assert_eq!(o.processor().counters().committed, 1);
        go(T + 601900);
        o.complete_proof(ticket, Err(RestFailure::Timeout)).unwrap();
        assert!(matches!(
            window(&mut o, T + 600000),
            Tick::Published {
                origin: "WS_LIVE",
                ..
            }
        ));
        assert_eq!(o.processor().counters().committed, 2);
        assert_eq!(
            read(&path)
                .iter()
                .filter(|r| r["kind"] == "model_step")
                .count(),
            2
        );
        assert_eq!(o.role_counts().proof_scheduled, 1);
    });
}

#[test]
fn ws_first_late_conflicting_rest_neither_rewrites_ws_nor_replays_model() {
    candidate(T, T - 100, NEW, |mut o, path| {
        let ticket = ws_overtakes(&mut o);
        let before: Vec<_> = read(&path)
            .into_iter()
            .filter(|r| {
                r["kind"] == "canonical_source"
                    || r["kind"] == "model_step"
                    || r["kind"] == "canonical_commit"
            })
            .collect();
        go(T + 601901);
        let response = source(&ticket, changes(T));
        o.complete_proof(ticket, Ok(response)).unwrap();
        let after: Vec<_> = read(&path)
            .into_iter()
            .filter(|r| {
                r["kind"] == "canonical_source"
                    || r["kind"] == "model_step"
                    || r["kind"] == "canonical_commit"
            })
            .collect();
        assert_eq!(before, after);
        assert!(read(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_rest_proof_late_ignored"
                && r["event"]["ws_already_committed"] == true));
        assert!(matches!(
            window(&mut o, T + 600000),
            Tick::Published {
                origin: "WS_LIVE",
                ..
            }
        ));
    });
}

#[test]
fn ws_first_timely_postcommit_price_audit_remains_strict() {
    candidate(T, T - 100, NEW, |mut o, path| {
        let ticket = ws_overtakes(&mut o);
        go(T + 601300);
        let response = source(&ticket, changes(T));
        assert!(matches!(
            o.complete_proof(ticket, Ok(response)),
            Err(CoordinatorError::Source(PrimaryError::CanonicalConflict))
        ));
        assert_eq!(
            read(&path)
                .iter()
                .filter(|r| r["kind"] == "model_step")
                .count(),
            1
        );
    });
}

#[test]
fn ws_first_timely_response_processed_after_deadline_cannot_rewrite_ws() {
    candidate(T, T - 100, NEW, |mut o, path| {
        for i in 0..9 {
            send(&mut o, raw_row(T + i * 60000), T + i * 60000 + 100);
        }
        send(&mut o, raw_row(T + 600000), T + 600100);
        go(T + 601000);
        let Tick::ProofRequired(ticket) = o.tick().unwrap() else {
            panic!("proof");
        };
        go(T + 601050);
        let response = source(&ticket, changes(T));
        send(&mut o, raw_row(T + 540000), T + 601200);
        assert!(matches!(
            o.tick().unwrap(),
            Tick::Published {
                origin: "WS_LIVE",
                ..
            }
        ));
        go(T + 601901);
        o.complete_proof(ticket, Ok(response)).unwrap();
        assert_eq!(
            read(&path)
                .iter()
                .filter(|r| r["kind"] == "model_step")
                .count(),
            1
        );
        assert!(matches!(
            window(&mut o, T + 600000),
            Tick::Published {
                origin: "WS_LIVE",
                ..
            }
        ));
    });
}

#[test]
fn ws_first_timely_rest_plus_slow_evidence_io_cannot_publish_late() {
    candidate(T, T + 100, NEW, |mut o, path| {
        let ticket = startup(&mut o, true);
        go(T + 601850);
        let response = source(&ticket, changes(T));
        let io = Arc::new(ProofWaitIo::new(60));
        io.release.store(true, std::sync::atomic::Ordering::SeqCst);
        o.proof_wait_io = Some(io.clone());
        assert!(o.complete_proof(ticket, Ok(response)).is_err());
        assert!(io.entered.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(now_ms(), T + 601910);
        assert!(!read(&path).iter().any(|r| r["kind"] == "model_step"));
        assert_eq!(o.processor().counters().committed, 0);
    });
}

#[test]
fn ws_first_late_wrong_range_is_integrity_failure_not_timeout_fallback() {
    candidate(T, T - 100, NEW, |mut o, _| {
        let ticket = ws_overtakes(&mut o);
        go(T + 601901);
        let mut wrong = ticket.clone();
        wrong.label_ms -= 600000;
        let response = source(
            &wrong,
            (0..10).map(|i| raw_row(T - 600000 + i * 60000)).collect(),
        );
        assert_eq!(
            o.complete_proof(ticket, Ok(response)).unwrap_err(),
            CoordinatorError::Rest(RestFailure::Integrity)
        );
        assert!(o.tick().is_err());
    });
}

#[test]
fn ws_first_auth_integrity_unknown_cancellation_still_fail_closed() {
    for failure in [
        RestFailure::Authentication,
        RestFailure::Integrity,
        RestFailure::Unknown,
        RestFailure::Cancelled,
    ] {
        candidate(T, T + 100, NEW, |mut o, path| {
            let ticket = startup(&mut o, true);
            go(T + 601050);
            assert_eq!(
                o.complete_proof(ticket, Err(failure)).unwrap_err(),
                CoordinatorError::Rest(failure)
            );
            assert!(o.tick().is_err());
            assert!(!read(&path).iter().any(|r| r["kind"] == "model_step"));
        });
    }
}

#[test]
fn ws_first_nov9_real_conflict_pair_uses_original_rest_body_not_forged_ws_time() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/ws-primary-rest-wsfirst-20261109/source-conflict.json"
    )))
    .unwrap();
    let evidence: ClosedM1SnapshotEvidenceV1 =
        serde_json::from_value(fixture["rest_evidence"].clone()).unwrap();
    let label = evidence.start.timestamp_millis();
    assert_eq!(fixture["ws_receive_time_known"], false);
    for policy in [
        NEW,
        DeliveryPolicy::ObservedBoNonBlockingRestV3,
        DeliveryPolicy::ObservedBo1sVolumeAuditV1,
    ] {
        candidate(label, label + 6960, policy, |mut o, path| {
            let body: Value = serde_json::from_str(&evidence.parts[0].raw_body).unwrap();
            let bars = body["bars"].as_array().unwrap();
            let reported = &fixture["ws_disputed_bar"];
            for (i, row) in bars.iter().enumerate() {
                let mut row = row.clone();
                if i == 9 {
                    for key in ["open", "high", "low", "close", "volume"] {
                        row[key] = json!({"value":reported[key]});
                    }
                }
                // These RX times are SYNTHETIC: no original per-minute WS RX
                // timestamp/frame exists in the retained provider evidence.
                send(
                    &mut o,
                    row,
                    label + i as i64 * 60000 + if i == 0 { 7000 } else { 200 },
                );
            }
            send(&mut o, raw_row(label + 600000), label + 600100);
            go(label + 601001);
            let Tick::ProofRequired(ticket) = o.tick().unwrap() else {
                panic!("startup proof")
            };
            go(label + 601031);
            let source = AdmittedClosedM1Snapshot::admit(
                evidence.clone(),
                &ticket.request_plan().unwrap(),
                at(now_ms()),
            )
            .unwrap();
            let outcome = o.complete_proof(ticket, Ok(source));
            let count = read(&path)
                .iter()
                .filter(|r| r["kind"] == "model_step")
                .count();
            if policy != DeliveryPolicy::ObservedBo1sVolumeAuditV1 {
                assert!(matches!(
                    outcome.unwrap(),
                    Tick::Published {
                        origin: "REST_EXCEPTION",
                        ..
                    }
                ));
                assert_eq!(count, 1);
            } else {
                assert!(matches!(
                    outcome,
                    Err(CoordinatorError::Source(PrimaryError::CanonicalConflict))
                ));
                assert_eq!(count, 0);
            }
        });
    }
}
