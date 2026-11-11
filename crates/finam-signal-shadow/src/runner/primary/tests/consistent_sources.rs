//! V5 source selection, real model callbacks; all clocks are synthetic.
use super::*;
use super::{
    admitted_rest::{pending, revised, rows},
    ws_first::{candidate, send, ws_overtakes},
};
use crate::primary::DeliveryPolicy;
const V5: DeliveryPolicy = DeliveryPolicy::ObservedBoConsistentSourcesV5;

#[test]
fn consistent_v5_known_pair_across_generation_does_not_reopen_live_readiness() {
    candidate(T, T - 100, V5, |mut o, path| {
        let ticket = pending(&mut o, false);
        go(T + 601040);
        o.complete_proof(ticket.clone(), Ok(source(&ticket, rows())))
            .unwrap();
        go(T + 601200);
        o.processor.begin_generation(2, T + 601200).unwrap();
        let row = revised(T + 540000, "ws");
        let frame=crate::primary::WireFrame::normalize(&json!({"type":"DATA","subscription_type":"BARS","subscription_key":"test-key","payload":{"symbol":"IMOEXF@RTSX","bars":[row]}}).to_string(),2,"test-key",at(T+601200)).unwrap();
        o.on_frame(frame).unwrap();
        assert!(o.processor.last_live_data().is_none());
        assert!(o.processor.gap_unresolved());
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
fn consistent_v5_late_or_missing_known_rest_never_gains_callback() {
    for late in [false, true] {
        candidate(T, T - 100, V5, |mut o, path| {
            let ticket = pending(&mut o, false);
            go(T + if late { 601901 } else { 601100 });
            let mut values = rows();
            if !late {
                values.remove(5);
            }
            let result = o
                .complete_proof(ticket.clone(), Ok(source(&ticket, values)))
                .unwrap();
            assert!(!matches!(result, Tick::Published { .. }));
            assert_eq!(o.processor.counters().committed, 0);
            assert!(!read(&path).iter().any(|r| r["kind"] == "model_step"));
        });
    }
}

#[test]
fn consistent_v5_known_superseded_repeat_does_not_rewrite_or_refresh_readiness() {
    candidate(T, T - 100, V5, |mut o, path| {
        let ticket = pending(&mut o, false);
        go(T + 601040);
        assert!(matches!(
            o.complete_proof(ticket.clone(), Ok(source(&ticket, rows())))
                .unwrap(),
            Tick::Published {
                origin: "REST_EXCEPTION",
                ..
            }
        ));
        let before: Vec<_> = read(&path)
            .into_iter()
            .filter(|r| {
                matches!(
                    r["kind"].as_str(),
                    Some("canonical_source" | "model_step" | "canonical_commit")
                )
            })
            .collect();
        let readiness = o.processor.last_live_data().unwrap().received_at_ms;
        let cache = o.processor.cached_m1();
        send(&mut o, revised(T + 540000, "ws"), T + 601200);
        assert_eq!(
            o.processor.last_live_data().unwrap().received_at_ms,
            readiness
        );
        assert_eq!(o.processor.cached_m1(), cache);
        let after: Vec<_> = read(&path)
            .into_iter()
            .filter(|r| {
                matches!(
                    r["kind"].as_str(),
                    Some("canonical_source" | "model_step" | "canonical_commit")
                )
            })
            .collect();
        assert_eq!(before, after);
        let trace = read(&path);
        let c = &trace
            .iter()
            .find(|r| r["event"]["change"]["context"] == "WS_SUPERSEDED_REPEAT_DISCARDED")
            .unwrap()["event"]["change"];
        assert_eq!(c["superseded_ws_version"]["generation"], 1);
        assert!(c["original_canonical_source_sha256"].as_str().is_some());
        send(&mut o, raw_row(T + 600000), T + 602000);
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
fn consistent_v5_unknown_postcommit_price_is_still_terminal() {
    candidate(T, T - 100, V5, |mut o, _| {
        let ticket = pending(&mut o, false);
        go(T + 601040);
        o.complete_proof(ticket.clone(), Ok(source(&ticket, rows())))
            .unwrap();
        let mut row = revised(T + 540000, "ws");
        row["close"] = json!({"value":"2451"});
        go(T + 601200);
        let frame=crate::primary::WireFrame::normalize(&json!({"type":"DATA","subscription_type":"BARS","subscription_key":"test-key","payload":{"symbol":"IMOEXF@RTSX","bars":[row]}}).to_string(),1,"test-key",at(T+601200)).unwrap();
        assert!(matches!(
            o.on_frame(frame),
            Err(CoordinatorError::Source(
                crate::primary::PrimaryError::CanonicalConflict
            ))
        ));
        assert_eq!(o.processor.counters().committed, 1);
    });
}

#[test]
fn consistent_v5_boundary_with_pending_grace_can_select_whole_rest_before_deadline() {
    for boundary in [950, 1200] {
        candidate(T, T - 100, V5, |mut o, path| {
            for i in 0..10 {
                send(&mut o, raw_row(T + i * 60000), T + i * 60000 + 100);
            }
            let ticket = if boundary == 950 {
                send(&mut o, raw_row(T + 600000), T + 600950);
                go(T + 601000);
                let Tick::ProofRequired(t) = o.tick().unwrap() else {
                    panic!("grace proof")
                };
                assert_eq!(t.reason(), Some(ExceptionReason::WsGracePending));
                t
            } else {
                go(T + 601000);
                let Tick::ProofRequired(t) = o.tick().unwrap() else {
                    panic!("boundary proof")
                };
                send(&mut o, raw_row(T + 600000), T + 601200);
                t
            };
            go(T + 601250);
            assert!(matches!(
                o.complete_proof(
                    ticket.clone(),
                    Ok(source(
                        &ticket,
                        (0..10).map(|i| raw_row(T + i * 60000)).collect()
                    ))
                )
                .unwrap(),
                Tick::Published {
                    origin: "REST_EXCEPTION",
                    ..
                }
            ));
            let trace = read(&path);
            let r = &trace
                .iter()
                .find(|r| r["kind"] == "canonical_source")
                .unwrap()["event"]["receipt"];
            assert_eq!(r["reason"], "WsGracePending");
            assert_eq!(r["boundary_received_at_ms"], T + 600000 + boundary);
            assert_eq!(r["canonical_ready_at_ms"], T + 601250);
        });
    }
    candidate(T, T - 100, V5, |mut o, _| {
        let ticket = ws_overtakes(&mut o);
        go(T + 601800);
        o.complete_proof(ticket.clone(), Ok(source(&ticket, rows())))
            .unwrap();
        assert_eq!(o.processor.counters().committed, 1);
    });
}

#[test]
fn consistent_v5_nonmonotonic_valid_provisional_is_quarantined_until_whole_rest() {
    for (field, value) in [
        ("open", "2200.5"),
        ("high", "2200.5"),
        ("low", "2200"),
        ("volume", "1"),
    ] {
        candidate(T, T - 100, V5, |mut o, path| {
            send(&mut o, raw_row(T), T + 100);
            let mut revision = raw_row(T);
            revision[field] = json!({"value":value});
            if field == "volume" {
                revision["close"] = json!({"value":"2200"});
            }
            send(&mut o, revision, T + 200);
            for i in 1..=10 {
                send(&mut o, raw_row(T + i * 60000), T + i * 60000 + 100);
            }
            go(T + 601100);
            let Tick::ProofRequired(ticket) = o.tick().unwrap() else {
                panic!("quarantined WS must not call model")
            };
            assert_eq!(ticket.reason(), Some(ExceptionReason::ProvisionalRevision));
            assert_eq!(o.processor.counters().committed, 0);
            go(T + 601150);
            let admitted = source(&ticket, (0..10).map(|i| raw_row(T + i * 60000)).collect());
            assert!(matches!(
                o.complete_proof(ticket, Ok(admitted)).unwrap(),
                Tick::Published {
                    origin: "REST_EXCEPTION",
                    ..
                }
            ));
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
}
