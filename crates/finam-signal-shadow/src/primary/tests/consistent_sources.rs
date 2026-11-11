use super::*;
use crate::primary::DeliveryPolicy;

#[test]
fn consistent_v5_reference_versions_do_not_rewrite_inventory_or_seed() {
    for variant in ["price", "missing", "extra"] {
        let mut raw = config();
        raw["primary"]["delivery"] = json!(DeliveryPolicy::ObservedBoConsistentSourcesV5);
        let config = WsPrimaryConfig::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
        let mut p =
            PrimaryProcessor::from_fixture(&config, instrument(), vec![T, C], 1, T - 100).unwrap();
        let old = reference(vec![row(T - 120000)], T - 120000, T, T + 100);
        p.add_reference(&old).unwrap();
        let first_hash = old.sha256().to_owned();
        let raw_original = old.evidence().clone();
        let mut revised = row(T - 120000);
        revised["close"] = json!({"value":"2200"});
        let values = match variant {
            "missing" => vec![row(T - 60000)],
            "extra" => vec![revised, row(T - 60000)],
            _ => vec![revised],
        };
        let new = reference(values, T - 120000, T, T + 200);
        let result = p.add_reference(&new);
        if variant != "price" {
            assert_eq!(result, Err(PrimaryError::ReferenceConflict));
            continue;
        }
        result.unwrap();
        assert_eq!(new.bars().next().unwrap().close, Decimal::from(2200));
        assert_eq!(old.sha256(), first_hash);
        assert_eq!(
            serde_json::to_value(old.evidence()).unwrap(),
            serde_json::to_value(raw_original).unwrap()
        );
        assert_eq!(p.reference_hashes().len(), 2);
        assert_eq!(p.cached_m1(), 0);
        assert!(!p.subscription_data_ready());
        assert_eq!(
            p.take_source_changes()[0].context,
            "REST_REFERENCE_VERSIONED"
        );
    }
}

#[test]
fn consistent_v5_is_opt_in_and_binds_new_fingerprint_without_model_change() {
    let raw = config();
    let old = WsPrimaryConfig::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let mut selected = raw;
    selected["primary"]["delivery"] = json!(DeliveryPolicy::ObservedBoConsistentSourcesV5);
    let new = WsPrimaryConfig::parse(&serde_json::to_vec(&selected).unwrap()).unwrap();
    assert!(new.primary.consistent_sources());
    assert!(!old.primary.consistent_sources());
    assert_ne!(old.fingerprint().unwrap(), new.fingerprint().unwrap());
    assert_eq!(old.base.model_sha256, new.base.model_sha256);
    assert_eq!(
        old.base.feedback_policy_sha256,
        new.base.feedback_policy_sha256
    );
    assert_eq!(new.primary.close_grace_ms, 1000);
    assert_eq!(new.primary.max_ingress_age_ms, 2000);
}
