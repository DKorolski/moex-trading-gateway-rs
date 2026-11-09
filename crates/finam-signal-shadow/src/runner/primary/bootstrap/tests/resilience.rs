use super::*;
use crate::{
    primary::{DeliveryPolicy, PrimaryProcessor},
    timing::with_fixture_clock,
};
use std::path::PathBuf;

fn delivery(
    mode: DeliveryPolicy,
    work: impl FnOnce(Coordinator, PathBuf, crate::socket::primary::Subscription),
) {
    with_fixture_clock(T - 100, || {
        let (session, config, _, path) =
            crate::session::tests::primary_fixture_delivery(T, false, mode);
        let instrument = broker_core::InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: broker_core::Exchange::Moex,
            market: broker_core::Market::Futures,
        };
        let p = PrimaryProcessor::from_config(&config, instrument, vec![T, T + 600000], 1, T - 100)
            .unwrap();
        let mut owner = Coordinator::new(p, session).unwrap();
        let sub = crate::socket::primary::fixture_subscription(1, T - 100);
        owner.bind_subscription(&sub).unwrap();
        work(owner, path, sub);
    });
}
fn owned(rows: Vec<Value>, sub: &crate::socket::primary::Subscription) -> WireFrame {
    let raw=json!({"type":"DATA","subscription_type":"BARS","subscription_key":"test-key","payload":{"symbol":"IMOEXF@RTSX","bars":rows}}).to_string();
    let mut frame = WireFrame::normalize(&raw, 1, "test-key", at(now_ms())).unwrap();
    frame.owner = Some(sub.owner.clone());
    frame
}
fn volume_row(stamp: i64, volume: &str) -> Value {
    let mut row = raw_row(stamp);
    row["volume"] = json!({"value":volume});
    row
}
fn trace(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn res1_delayed_missing_volume_compatibility_keeps_original_stamps_and_no_model() {
    for (ws, rest) in [("1548", "1543"), ("1543", "1548")] {
        for age in [3000, 1999, 2000, 2001] {
            delivery(
                DeliveryPolicy::ObservedBo1sVolumeAuditV1,
                |mut owner, path, sub| {
                    let mut b = bridge(&owner);
                    go(T + 100);
                    let frame = owned(vec![volume_row(T - 600000, ws)], &sub);
                    let original_hash = frame.sha256().to_owned();
                    let ticket = b.on_frame(&mut owner, frame).unwrap().unwrap();
                    assert_eq!(b.pending.as_ref().unwrap().0.received_at_ms(), T + 100);
                    assert_eq!(ticket.deadline_ms(), T + 5100);
                    go(T + 100 + age);
                    let evidence = source(&ticket, vec![volume_row(T - 600000, rest)]);
                    let source_hash = evidence.sha256().to_owned();
                    eprintln!("RES1_CASE age_ms={age} ws_volume={ws} rest_volume={rest} received_ms={} response_ms={} frame_sha256={original_hash}",T+100,now_ms());
                    b.complete(&mut owner, ticket, Ok(evidence)).unwrap();
                    assert!(!b.pending());
                    assert!(!owner.processor.subscription_data_ready());
                    assert!(owner.processor.last_live_data().is_none());
                    assert_eq!(owner.processor.cached_m1(), 0);
                    assert_eq!(owner.processor.counters().discarded_history, 1);
                    assert_eq!(owner.processor.counters().valid_updates, 0);
                    assert_eq!(owner.processor.counters().committed, 0);
                    let records = trace(&path);
                    let required = records
                        .iter()
                        .find(|r| r["event"]["event"] == "primary_bootstrap_proof_required")
                        .unwrap();
                    assert_eq!(required["event"]["frame_sha256"], original_hash);
                    assert_eq!(required["event"]["original_received_at_ms"], T + 100);
                    let applied = records
                        .iter()
                        .find(|r| r["event"]["event"] == "primary_bootstrap_proof_applied")
                        .unwrap();
                    assert_eq!(applied["event"]["source_sha256"], source_hash);
                    assert_eq!(applied["event"]["history_proves_data_readiness"], false);
                    let change = records
                        .iter()
                        .find(|r| {
                            r["event"]["event"] == "primary_source_change"
                                && r["event"]["change"]["context"] == "WS_BOOTSTRAP_REFERENCE"
                        })
                        .unwrap();
                    assert_eq!(change["event"]["change"]["kind"], "VOLUME_ONLY");
                    let expected: broker_core::event::Bar =
                        serde_json::from_value(change["event"]["change"]["expected"].clone())
                            .unwrap();
                    let observed: broker_core::event::Bar =
                        serde_json::from_value(change["event"]["change"]["observed"].clone())
                            .unwrap();
                    assert_eq!(expected.volume, rest.parse().unwrap());
                    assert_eq!(observed.volume, ws.parse().unwrap());
                    no_model(&path);
                    go(now_ms() + 100);
                    b.on_frame(&mut owner, owned(vec![raw_row(T)], &sub))
                        .unwrap();
                    assert!(owner.processor.subscription_data_ready());
                    assert_eq!(
                        owner.processor.last_live_data().unwrap().received_at_ms,
                        now_ms()
                    );
                    assert_eq!(owner.processor.cached_m1(), 1);
                    no_model(&path);
                },
            );
        }
    }
}

#[test]
fn res1_strict_volume_mismatch_stays_rejected_on_both_sides_of_ingress_limit() {
    for (ws, rest) in [("1548", "1543"), ("1543", "1548")] {
        for age in [1999, 2000, 2001, 3000] {
            delivery(DeliveryPolicy::StrictBoundaryV1, |mut owner, path, sub| {
                let mut b = bridge(&owner);
                go(T + 100);
                let ticket = b
                    .on_frame(&mut owner, owned(vec![volume_row(T - 600000, ws)], &sub))
                    .unwrap()
                    .unwrap();
                go(T + 100 + age);
                let evidence = source(&ticket, vec![volume_row(T - 600000, rest)]);
                assert!(b.complete(&mut owner, ticket, Ok(evidence)).is_err());
                assert!(owner.processor.last_live_data().is_none());
                assert_eq!(owner.processor.cached_m1(), 0);
                assert_eq!(owner.tick().unwrap_err(), CoordinatorError::Held);
                no_model(&path);
            });
        }
    }
}

#[test]
fn res1_prices_identity_and_missing_reference_still_reject_without_partial_effects() {
    for age in [2000, 2001, 3000] {
        for case in ["open", "high", "low", "close", "identity", "missing"] {
            delivery(
                DeliveryPolicy::ObservedBo1sVolumeAuditV1,
                |mut owner, path, sub| {
                    let mut b = bridge(&owner);
                    go(T + 100);
                    let ticket = b
                        .on_frame(
                            &mut owner,
                            owned(vec![volume_row(T - 600000, "1548")], &sub),
                        )
                        .unwrap()
                        .unwrap();
                    let mut row = volume_row(T - 600000, "1543");
                    match case {
                        "identity" => {
                            b.pending.as_mut().unwrap().0.rows[0].bar.instrument.symbol =
                                "OTHER".into()
                        }
                        "missing" => row = volume_row(T - 540000, "1543"),
                        field => {
                            row[field] = json!({"value":match field {"open"=>"2200.5","high"=>"2201.5","low"=>"2198.5",_=>"2201"}})
                        }
                    }
                    go(T + 100 + age);
                    let evidence = source(&ticket, vec![row]);
                    assert!(
                        b.complete(&mut owner, ticket, Ok(evidence)).is_err(),
                        "{case}/{age}"
                    );
                    assert!(owner.processor.last_live_data().is_none());
                    assert_eq!(owner.processor.cached_m1(), 0);
                    assert_eq!(owner.tick().unwrap_err(), CoordinatorError::Held);
                    no_model(&path);
                },
            );
        }
    }
}

#[test]
fn res1_mixed_frame_does_not_gain_a_retimed_live_tail() {
    for age in [1999, 2000, 2001, 3000] {
        delivery(
            DeliveryPolicy::ObservedBo1sVolumeAuditV1,
            |mut owner, path, sub| {
                let mut b = bridge(&owner);
                go(T + 100);
                let frame = owned(vec![volume_row(T - 600000, "1548"), raw_row(T)], &sub);
                let ticket = b.on_frame(&mut owner, frame).unwrap().unwrap();
                go(T + 100 + age);
                let evidence = source(&ticket, vec![volume_row(T - 600000, "1543")]);
                let result = b.complete(&mut owner, ticket, Ok(evidence));
                if age <= 2000 {
                    result.unwrap();
                    assert_eq!(owner.processor.cached_m1(), 1);
                    assert_eq!(
                        owner.processor.last_live_data().unwrap().received_at_ms,
                        T + 100
                    );
                } else {
                    assert_eq!(
                        result,
                        Err(CoordinatorError::Source(PrimaryError::Continuity))
                    );
                    assert!(owner.processor.last_live_data().is_none());
                    assert_eq!(owner.processor.cached_m1(), 0);
                    assert_eq!(owner.tick().unwrap_err(), CoordinatorError::Held);
                }
                no_model(&path);
            },
        );
    }
}

#[test]
fn res1_foreign_owner_and_generation_stay_fenced() {
    for age in [2000, 2001, 3000] {
        for alien_owner in [false, true] {
            delivery(
                DeliveryPolicy::ObservedBo1sVolumeAuditV1,
                |mut owner, path, sub| {
                    let mut b = bridge(&owner);
                    go(T + 100);
                    let ticket = b
                        .on_frame(
                            &mut owner,
                            owned(vec![volume_row(T - 600000, "1548")], &sub),
                        )
                        .unwrap()
                        .unwrap();
                    let pending = &mut b.pending.as_mut().unwrap().0;
                    if alien_owner {
                        pending.owner = Some(Arc::new(()));
                    } else {
                        pending.generation = 2;
                    }
                    go(T + 100 + age);
                    let evidence = source(&ticket, vec![volume_row(T - 600000, "1543")]);
                    assert_eq!(
                        b.complete(&mut owner, ticket, Ok(evidence)),
                        Err(CoordinatorError::Source(PrimaryError::Generation))
                    );
                    assert!(owner.processor.last_live_data().is_none());
                    assert_eq!(owner.processor.cached_m1(), 0);
                    no_model(&path);
                },
            );
        }
    }
}

#[test]
fn res1_volume_compatibility_does_not_extend_bootstrap_deadline() {
    delivery(
        DeliveryPolicy::ObservedBo1sVolumeAuditV1,
        |mut owner, path, sub| {
            let mut b = bridge(&owner);
            go(T + 100);
            let ticket = b
                .on_frame(
                    &mut owner,
                    owned(vec![volume_row(T - 600000, "1548")], &sub),
                )
                .unwrap()
                .unwrap();
            go(ticket.deadline_ms() + 1);
            let evidence = source(&ticket, vec![volume_row(T - 600000, "1543")]);
            assert_eq!(
                b.complete(&mut owner, ticket, Ok(evidence)),
                Err(CoordinatorError::Rest(RestFailure::Integrity))
            );
            assert!(owner.processor.last_live_data().is_none());
            assert_eq!(owner.processor.cached_m1(), 0);
            no_model(&path);
        },
    );
}
