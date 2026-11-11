//! Retained numeric pair, synthetic clocks and real coordinator/model, no network.
use super::ws_first::{candidate, send, ws_overtakes};
use super::*;
use crate::primary::{DeliveryPolicy, WsPrimaryConfig};

const NEW: DeliveryPolicy = DeliveryPolicy::ObservedBoAdmittedRestV4;

fn revised(label: i64, which: &str) -> Value {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/ws-primary-rest-authority-20261111/closed-m1-pair.json"
    )))
    .unwrap();
    let mut row = raw_row(label);
    for field in ["open", "high", "low", "close", "volume"] {
        row[field] = json!({"value":fixture[which][field]});
    }
    row
}

fn pending(o: &mut Coordinator, sparse: bool) -> RestTicket {
    for i in 0..10 {
        if sparse && i == 5 {
            continue;
        }
        let stamp = T + i * 60000;
        send(
            o,
            if i == 9 {
                revised(stamp, "ws")
            } else {
                raw_row(stamp)
            },
            stamp + 100,
        );
    }
    go(T + 601000);
    let Tick::ProofRequired(ticket) = o.tick().unwrap() else {
        panic!("proof required")
    };
    assert_eq!(
        ticket.reason(),
        Some(if sparse {
            ExceptionReason::SparseUnproven
        } else {
            ExceptionReason::BoundaryUnproven
        })
    );
    ticket
}

fn rows() -> Vec<Value> {
    (0..10)
        .map(|i| {
            if i == 9 {
                revised(T + i * 60000, "rest")
            } else {
                raw_row(T + i * 60000)
            }
        })
        .collect()
}

#[test]
fn admitted_rest_v4_is_opt_in_with_distinct_fingerprints_and_unchanged_model() {
    let mut raw = crate::primary::tests::config();
    let old = WsPrimaryConfig::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    raw["primary"]["delivery"] = json!(NEW);
    let new = WsPrimaryConfig::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
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
    for policy in [
        DeliveryPolicy::StrictBoundaryV1,
        DeliveryPolicy::ObservedBo1sVolumeAuditV1,
        DeliveryPolicy::ObservedBoRestPrecommitWsFirstV2,
        DeliveryPolicy::ObservedBoNonBlockingRestV3,
    ] {
        raw["primary"]["delivery"] = json!(policy);
        let previous = WsPrimaryConfig::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(!previous.primary.admitted_rest_source());
        assert_ne!(previous.fingerprint().unwrap(), new.fingerprint().unwrap());
    }
}

#[test]
fn admitted_rest_v4_retained_close_volume_pair_uses_one_whole_source_without_reconnect() {
    for policy in [DeliveryPolicy::ObservedBoNonBlockingRestV3, NEW] {
        for sparse in [false, true] {
            candidate(T, T - 100, policy, |mut o, path| {
                let ticket = pending(&mut o, sparse);
                let duplicate = ticket.clone();
                go(T + 601040);
                let response = source(&ticket, rows());
                let hash = response.sha256().to_owned();
                let result = o.complete_proof(ticket, Ok(response)).unwrap();
                if policy != NEW {
                    assert!(matches!(result, Tick::AwaitingProof));
                    go(T + 601903);
                    assert!(matches!(o.tick().unwrap(), Tick::Degraded));
                    assert_eq!(o.processor().counters().committed, 0);
                    return;
                }
                assert!(matches!(
                    result,
                    Tick::Published {
                        origin: "REST_EXCEPTION",
                        ..
                    }
                ));
                assert!(o
                    .complete_proof(duplicate, Err(RestFailure::Timeout))
                    .is_err());
                let before = read(&path);
                let receipt = &before
                    .iter()
                    .find(|r| r["kind"] == "canonical_source")
                    .unwrap()["event"]["receipt"];
                assert_eq!(receipt["actual_m1"].as_array().unwrap().len(), 10);
                assert_eq!(receipt["actual_m1"][9]["close"], "2450.5");
                assert_eq!(receipt["actual_m1"][9]["volume"], "2238");
                assert_eq!(receipt["rest_snapshot_sha256"], hash);
                assert_eq!(receipt["source_frame_sha256"], json!([]));
                assert_eq!(
                    before.iter().filter(|r| r["kind"] == "model_step").count(),
                    1
                );
                go(T + 601903);
                assert!(matches!(o.tick().unwrap(), Tick::Waiting));
                assert!(!o.processor().gap_unresolved());
                send(&mut o, raw_row(T + 600000), T + 602000);
                assert!(matches!(
                    window(&mut o, T + 600000),
                    Tick::Published {
                        origin: "WS_LIVE",
                        ..
                    }
                ));
                assert_eq!(o.processor().counters().committed, 2);
                let after = read(&path);
                assert_eq!(
                    after.iter().filter(|r| r["kind"] == "model_step").count(),
                    2
                );
                assert!(!after
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_episode_started"));
            });
        }
    }
}

#[test]
fn admitted_rest_v4_missing_late_or_failed_source_never_calls_model() {
    for case in ["missing", "late", "integrity", "unavailable"] {
        candidate(T, T - 100, NEW, |mut o, path| {
            let ticket = pending(&mut o, false);
            go(T + if case == "late" { 601900 } else { 601040 });
            let result = match case {
                "integrity" => Err(RestFailure::Integrity),
                "unavailable" => Err(RestFailure::Unavailable),
                _ => {
                    let mut values = rows();
                    if case == "missing" {
                        values.remove(5);
                    }
                    Ok(source(&ticket, values))
                }
            };
            assert!(matches!(
                o.complete_proof(ticket, result).unwrap(),
                Tick::AwaitingProof
            ));
            assert_eq!(o.processor().counters().committed, 0);
            assert!(!read(&path).iter().any(|r| r["kind"] == "model_step"));
            go(T + 601903);
            assert!(matches!(o.tick().unwrap(), Tick::Degraded));
        });
    }
}

#[test]
fn admitted_rest_v4_ready_ws_stays_first_and_later_rest_cannot_rewrite_it() {
    candidate(T, T - 100, NEW, |mut o, path| {
        let ticket = ws_overtakes(&mut o);
        let before: Vec<_> = read(&path)
            .into_iter()
            .filter(|r| {
                matches!(
                    r["kind"].as_str(),
                    Some("canonical_source" | "model_step" | "canonical_commit")
                )
            })
            .collect();
        go(T + 601800);
        let response = source(&ticket, rows());
        assert!(matches!(
            o.complete_proof(ticket, Ok(response)).unwrap(),
            Tick::AwaitingProof
        ));
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
        assert_eq!(o.processor().counters().committed, 1);
        assert!(matches!(
            window(&mut o, T + 600000),
            Tick::Published {
                origin: "WS_LIVE",
                ..
            }
        ));
    });
}
