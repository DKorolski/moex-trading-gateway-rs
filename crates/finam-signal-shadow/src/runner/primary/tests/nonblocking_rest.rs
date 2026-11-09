//! Synthetic real-model regressions; no provider/soak/latency claim.
use super::ws_first::{candidate, changes, send, startup, ws_overtakes};
use super::*;
use crate::primary::DeliveryPolicy;
const NEW: DeliveryPolicy = DeliveryPolicy::ObservedBoNonBlockingRestV3;

#[test]
fn nonblocking_rest_preserves_nonstartup_source_admission_without_poisoning_future_eligible_ws() {
    candidate(T, T - 100, NEW, |mut o, path| {
        for i in 0..9 {
            send(&mut o, raw_row(T + i * 60000), T + i * 60000 + 100);
        }
        send(&mut o, raw_row(T + 600000), T + 600100);
        go(T + 601000);
        let Tick::ProofRequired(ticket) = o.tick().unwrap() else {
            panic!("sparse proof")
        };
        assert_eq!(ticket.reason(), Some(ExceptionReason::SparseUnproven));
        go(T + 601050);
        let mut rows: Vec<_> = (0..10).map(|i| raw_row(T + i * 60000)).collect();
        rows[0]["open"] = json!({"value":"2200.5"});
        let response = source(&ticket, rows);
        assert!(matches!(
            o.complete_proof(ticket, Ok(response)).unwrap(),
            Tick::AwaitingProof
        ));
        assert_eq!(o.processor().counters().committed, 0);
        assert!(read(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_rest_source_price_rejected"));
        send(&mut o, raw_row(T + 540000), T + 601200);
        assert!(matches!(
            o.tick().unwrap(),
            Tick::Published {
                origin: "WS_LIVE",
                ..
            }
        ));
        assert_eq!(o.processor().counters().committed, 1);
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
fn nonblocking_rest_preserves_timely_startup_source_and_one_callback() {
    candidate(T, T + 100, NEW, |mut o, path| {
        let ticket = startup(&mut o, true);
        go(T + 601050);
        let response = source(&ticket, changes(T));
        assert!(matches!(
            o.complete_proof(ticket, Ok(response)).unwrap(),
            Tick::Published {
                origin: "REST_EXCEPTION",
                ..
            }
        ));
        let rows = read(&path);
        assert_eq!(rows.iter().filter(|r| r["kind"] == "model_step").count(), 1);
        assert!(rows
            .iter()
            .any(|r| r["event"]["event"] == "primary_rest_observation"
                && r["event"]["assessment"]["kind"] == "PriceDifference"));
        assert_eq!(o.processor().counters().committed, 1);
    });
}

#[test]
fn nonblocking_rest_data_failures_and_disagreements_do_not_rewrite_ws_or_stop_next_bucket() {
    for case in [
        "price",
        "inventory",
        "Timeout",
        "Unavailable",
        "Integrity",
        "Authentication",
        "Unknown",
        "Cancelled",
    ] {
        candidate(T, T - 100, NEW, |mut o, path| {
            let ticket = ws_overtakes(&mut o);
            let before: Vec<_> = read(&path)
                .into_iter()
                .filter(|r| r["kind"] == "model_step" || r["kind"] == "canonical_source")
                .collect();
            go(T + 601900);
            let response = match case {
                "price" => Ok(source(&ticket, changes(T))),
                "inventory" => Ok(source(
                    &ticket,
                    (0..9).map(|i| raw_row(T + i * 60000)).collect(),
                )),
                "Timeout" => Err(RestFailure::Timeout),
                "Unavailable" => Err(RestFailure::Unavailable),
                "Integrity" => Err(RestFailure::Integrity),
                "Authentication" => Err(RestFailure::Authentication),
                "Unknown" => Err(RestFailure::Unknown),
                _ => Err(RestFailure::Cancelled),
            };
            assert!(matches!(
                o.complete_proof(ticket, response).unwrap(),
                Tick::AwaitingProof
            ));
            let after: Vec<_> = read(&path)
                .into_iter()
                .filter(|r| r["kind"] == "model_step" || r["kind"] == "canonical_source")
                .collect();
            assert_eq!(before, after, "{case}");
            assert!(matches!(
                window(&mut o, T + 600000),
                Tick::Published {
                    origin: "WS_LIVE",
                    ..
                }
            ));
            assert_eq!(o.processor().counters().committed, 2);
            let rows = read(&path);
            assert_eq!(rows.iter().filter(|r| r["kind"] == "model_step").count(), 2);
            assert!(rows.iter().any(|r| matches!(
                r["event"]["event"].as_str(),
                Some("primary_rest_observation" | "primary_rest_nonblocking_failed")
            )));
            if case == "inventory" {
                assert!(rows.iter().any(
                    |r| r["event"]["assessment"]["missing_known_m1_ms"] == json!([T + 540000])
                ));
            }
        });
    }
}

#[test]
fn nonblocking_rest_never_admits_missing_known_minute_or_missing_mandatory_source() {
    for late in [false, true] {
        for failure in [false, true] {
            candidate(T, T + 100, NEW, |mut o, path| {
                let ticket = startup(&mut o, true);
                go(T + if late { 601900 } else { 601050 });
                let response = if failure {
                    Err(RestFailure::Integrity)
                } else {
                    Ok(source(
                        &ticket,
                        (0..9).map(|i| raw_row(T + i * 60000)).collect(),
                    ))
                };
                assert!(matches!(
                    o.complete_proof(ticket, response).unwrap(),
                    Tick::AwaitingProof
                ));
                assert_eq!(o.processor().counters().committed, 0);
                assert!(!read(&path).iter().any(|r| r["kind"] == "model_step"));
                go(T + 601901);
                assert!(matches!(o.tick().unwrap(), Tick::Degraded));
            });
        }
    }
}

#[test]
fn ws_first_r2_late_missing_known_inventory_still_blocks_strict_v2_before_and_after_commit() {
    for committed in [false, true] {
        candidate(
            T,
            if committed { T - 100 } else { T + 100 },
            DeliveryPolicy::ObservedBoRestPrecommitWsFirstV2,
            |mut o, path| {
                let ticket = if committed {
                    ws_overtakes(&mut o)
                } else {
                    startup(&mut o, true)
                };
                go(T + 601900);
                let response = source(&ticket, (0..9).map(|i| raw_row(T + i * 60000)).collect());
                assert!(matches!(
                    o.complete_proof(ticket, Ok(response)),
                    Err(CoordinatorError::Source(PrimaryError::CanonicalConflict))
                ));
                assert_eq!(
                    read(&path)
                        .iter()
                        .filter(|r| r["kind"] == "model_step")
                        .count(),
                    usize::from(committed)
                );
            },
        );
    }
}

#[test]
fn nonblocking_rest_audits_record_values_and_errors_then_continue_without_model_replay() {
    for mode in ["match", "price", "inventory", "Integrity"] {
        candidate(T, T - 100, NEW, |mut o, path| {
            // This fixture enables the unchanged hourly audit cadence.
            for i in 0..6 {
                assert!(matches!(
                    window(&mut o, T + i * 600000),
                    Tick::Published { .. }
                ));
            }
            let ticket = o.take_audit().unwrap().unwrap();
            let label = ticket.label_ms();
            let result = match mode {
                "Integrity" => Err(RestFailure::Integrity),
                "inventory" => Ok(source(
                    &ticket,
                    (0..9).map(|i| raw_row(label + i * 60000)).collect(),
                )),
                "price" => Ok(source(&ticket, changes(label))),
                _ => Ok(source(
                    &ticket,
                    (0..10).map(|i| raw_row(label + i * 60000)).collect(),
                )),
            };
            let status = o.complete_audit(ticket, result).unwrap();
            assert_eq!(
                status,
                if mode == "Integrity" {
                    AuditStatus::EvidenceUnavailable
                } else {
                    AuditStatus::EvidenceOnly
                }
            );
            assert_eq!(o.processor().counters().committed, 6);
            assert!(matches!(
                window(&mut o, T + 3600000),
                Tick::Published { .. }
            ));
            assert_eq!(
                read(&path)
                    .iter()
                    .filter(|r| r["kind"] == "model_step")
                    .count(),
                7
            );
            assert_eq!(o.role_counts().audit_completed, 1);
        });
    }
}
