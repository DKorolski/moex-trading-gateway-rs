use super::*;

fn observed(subscribed: i64) -> PrimaryProcessor {
    let mut c = config();
    c["primary"]["delivery"] = json!("ObservedBo1sVolumeAuditV1");
    let c = WsPrimaryConfig::parse(&serde_json::to_vec(&c).unwrap()).unwrap();
    PrimaryProcessor::from_fixture(&c, instrument(), vec![T, C], 1, subscribed).unwrap()
}
fn feed(p: &mut PrimaryProcessor, boundary: bool) {
    for i in 0..if boundary { 11 } else { 10 } {
        let t = T + i * 60000;
        p.ingest(frame(vec![row(t)], t + 100), t + 100).unwrap();
    }
}

#[test]
fn observed_policy_is_explicit_and_old_configuration_stays_strict() {
    let old = config();
    assert!(old["primary"].get("delivery").is_none());
    let a = WsPrimaryConfig::parse(&serde_json::to_vec(&old).unwrap()).unwrap();
    let mut new = old.clone();
    new["primary"]["delivery"] = json!("ObservedBo1sVolumeAuditV1");
    let b = WsPrimaryConfig::parse(&serde_json::to_vec(&new).unwrap()).unwrap();
    assert_ne!(a.fingerprint().unwrap(), b.fingerprint().unwrap());
    assert_eq!(a.base.model_sha256, b.base.model_sha256);
    for bad in [json!(true), json!(null), json!("ignore_all_errors")] {
        new["primary"]["delivery"] = bad;
        assert!(WsPrimaryConfig::parse(&serde_json::to_vec(&new).unwrap()).is_err());
    }
}

#[test]
fn observed_dense_is_ready_in_one_second_without_any_rest() {
    let mut p = observed(T - 100);
    feed(&mut p, true);
    assert!(matches!(
        p.prepare(C + 1099).unwrap(),
        CanonicalStatus::Waiting
    ));
    let input = ready(&mut p, C + 1100);
    assert_eq!(input.receipt().origin, "WS_LIVE");
    assert!(input.rest_evidence().is_none());
    p.committed(*input, C + 1100).unwrap();
    assert_eq!(p.counters().committed, 1);
}

#[test]
fn observed_quiet_boundary_uses_one_fast_proof_not_invented_zero_m1() {
    let mut p = observed(T - 100);
    feed(&mut p, false);
    assert!(matches!(
        p.prepare(C + 999).unwrap(),
        CanonicalStatus::PendingProof {
            may_request: false,
            ..
        }
    ));
    assert!(matches!(
        p.prepare(C + 1000).unwrap(),
        CanonicalStatus::PendingProof {
            reason: ExceptionReason::BoundaryUnproven,
            may_request: true,
            ..
        }
    ));
    let source = reference(
        (0..10).map(|i| row(T + i * 60000)).collect(),
        T,
        C,
        C + 1050,
    );
    let input = p
        .prepare_rest_exception(&source, ExceptionReason::BoundaryUnproven, C + 1050)
        .unwrap()
        .unwrap();
    assert_eq!(input.receipt().origin, "REST_EXCEPTION");
    assert_eq!(input.receipt().minute_bitmap, 1023);
    p.committed(*input, C + 1050).unwrap();
}

#[test]
fn observed_late_source_never_becomes_a_live_callback() {
    let mut p = observed(T - 100);
    feed(&mut p, true);
    assert!(!matches!(
        p.prepare(C + 1901).unwrap(),
        CanonicalStatus::Ready(_)
    ));
    assert!(p.gap_unresolved());
    assert_eq!(p.counters().committed, 0);
}

#[test]
fn observed_volume_after_commit_is_audited_without_rewriting_or_readiness_refresh() {
    let mut p = observed(T - 100);
    feed(&mut p, true);
    let input = ready(&mut p, C + 1100);
    let original_digest = input.receipt().canonical_digest.clone();
    p.committed(*input, C + 1100).unwrap();
    let stamp = p.last_live_data().unwrap().received_at_ms;
    for volume in ["1548", "1543"] {
        let mut r = row(T);
        r["volume"] = json!({"value":volume});
        p.ingest(frame(vec![r], C + 1200), C + 1200).unwrap();
    }
    assert_eq!(p.counters().committed, 1);
    assert_eq!(p.next_label_ms(), Some(C));
    assert_eq!(p.last_live_data().unwrap().received_at_ms, stamp);
    assert_eq!(p.take_source_changes().len(), 2);
    let original = reference(
        (0..10).map(|i| row(T + i * 60000)).collect(),
        T,
        C,
        C + 1300,
    );
    p.compare_rest_audit(&original).unwrap();
    assert!(p.take_source_changes().is_empty());
    assert!(!original_digest.is_empty());
}

#[test]
fn observed_recovery_overlap_ignores_only_volume_without_replaying_committed_input() {
    for price_conflict in [false, true] {
        let mut p = observed(T - 100);
        feed(&mut p, true);
        let input = ready(&mut p, C + 1100);
        p.committed(*input, C + 1100).unwrap();
        p.break_continuity();
        let plan = p.plan_recovery(C, C + 2100).unwrap();
        let rows = (0..10)
            .map(|i| {
                let mut r = row(T + i * 60000);
                if i == 9 {
                    r["volume"] = json!({"value":"1548"});
                    if price_conflict {
                        r["close"] = json!({"value":"2201"});
                    }
                }
                r
            })
            .collect();
        let source = reference(rows, T, C, C + 2200);
        let result = p.admit_recovery(&plan, &source, C + 2200);
        if price_conflict {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap().steps(), 0);
        }
        assert_eq!(p.counters().committed, 1);
        assert_eq!(p.next_label_ms(), Some(C));
    }
}

#[test]
fn observed_volume_revisions_do_not_hide_price_or_inventory_changes() {
    for field in ["open", "high", "low", "close"] {
        let mut p = observed(T - 100);
        feed(&mut p, true);
        let input = ready(&mut p, C + 1100);
        p.committed(*input, C + 1100).unwrap();
        let mut r = row(T);
        r[field] = json!({"value":match field {"open"=>"2200.5","high"=>"2201.5","low"=>"2198.5",_=>"2201"}});
        r["volume"] = json!({"value":"1548"});
        assert_eq!(
            p.ingest(frame(vec![r], C + 1200), C + 1200),
            Err(PrimaryError::CanonicalConflict)
        );
        assert_eq!(p.counters().committed, 1);
        assert_eq!(
            p.take_source_changes()[0].kind,
            "PRICE_OR_INVENTORY_DIFFERENCE"
        );
    }
    let mut p = observed(T - 100);
    feed(&mut p, true);
    let input = ready(&mut p, C + 1100);
    p.committed(*input, C + 1100).unwrap();
    let sparse = reference(
        (1..10).map(|i| row(T + i * 60000)).collect(),
        T,
        C,
        C + 1200,
    );
    assert_eq!(
        p.compare_rest_audit(&sparse),
        Err(PrimaryError::CanonicalConflict)
    );
}

#[test]
fn n1_overlap_diagnostic_names_the_actual_price_conflict_not_the_first_row() {
    for first_volume_revision in [false, true] {
        let mut p = observed(T - 100);
        feed(&mut p, true);
        let input = ready(&mut p, C + 1100);
        p.committed(*input, C + 1100).unwrap();
        p.break_continuity();
        let plan = p.plan_recovery(C, C + 2100).unwrap();
        let rows = (0..10)
            .map(|i| {
                let mut r = row(T + i * 60000);
                if i == 0 && first_volume_revision {
                    r["volume"] = json!({"value":"1548"});
                }
                if i == 8 {
                    r["close"] = json!({"value":"2201"});
                }
                r
            })
            .collect();
        let source = reference(rows, T, C, C + 2200);
        assert!(matches!(
            p.admit_recovery(&plan, &source, C + 2200),
            Err(PrimaryError::CanonicalConflict)
        ));
        let changes = p.take_source_changes();
        let cause = changes
            .iter()
            .find(|c| c.context == "RECOVERY_OVERLAP" && c.kind == "PRICE_OR_INVENTORY_DIFFERENCE")
            .expect("actual conflict must be retained");
        assert_eq!(cause.minute_ms, T + 8 * 60000);
        assert_eq!(
            cause.expected.as_ref().unwrap().close,
            Decimal::new(22005, 1)
        );
        assert_eq!(cause.observed.as_ref().unwrap().close, Decimal::from(2201));
        assert_eq!(
            cause.expected.as_ref().unwrap().open_ts,
            cause.observed.as_ref().unwrap().open_ts
        );
        assert_eq!(
            cause.observed_source_sha256.as_deref(),
            Some(source.sha256())
        );
        assert!(!cause.model_input_rewritten);
        assert_eq!(p.counters().committed, 1);
        assert_eq!(p.next_label_ms(), Some(C));
    }
}

#[test]
fn n1_overlap_diagnostic_names_the_missing_reference_minute() {
    let mut p = observed(T - 100);
    feed(&mut p, true);
    let input = ready(&mut p, C + 1100);
    p.committed(*input, C + 1100).unwrap();
    p.break_continuity();
    let plan = p.plan_recovery(C, C + 2100).unwrap();
    let rows = (0..10)
        .filter(|i| *i != 8)
        .map(|i| row(T + i * 60000))
        .collect();
    let source = reference(rows, T, C, C + 2200);
    assert!(matches!(
        p.admit_recovery(&plan, &source, C + 2200),
        Err(PrimaryError::CanonicalConflict)
    ));
    let changes = p.take_source_changes();
    let cause = changes
        .iter()
        .find(|c| c.context == "RECOVERY_OVERLAP")
        .expect("missing minute must be retained");
    assert_eq!(cause.minute_ms, T + 8 * 60000);
    assert_eq!(cause.expected.as_ref().unwrap().open_ts, at(T + 8 * 60000));
    assert!(cause.observed.is_none());
    assert_eq!(
        cause.observed_source_sha256.as_deref(),
        Some(source.sha256())
    );
    assert_eq!(p.counters().committed, 1);
}
