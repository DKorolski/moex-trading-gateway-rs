// Disposable network-none container, linked to the accepted release rlibs.
use chrono::{DateTime, Utc};
use finam_gateway::{Stage8bP1fO2MaterializedSourceV1, Stage8bP1fObservedM10Plan};
use runtime_durable_service as d;
use serde_json::Value;
use std::fs;

fn read(name: &str) -> Value {
    serde_json::from_slice(&fs::read(format!("/package/payload/{name}")).unwrap()).unwrap()
}
fn time(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}
fn main() {
    let source = fs::read("/package/payload/source-template.json").unwrap();
    let template: Value = serde_json::from_slice(&source).unwrap();
    let supervisor = read("supervisor.template.json");
    // The actual packaged bytes must cross the guardian's parse_canonical
    // boundary, not merely deserialize successfully as a config.
    assert_eq!(
        fs::read("/package/payload/supervisor.template.json").unwrap(),
        serde_json::to_vec(&supervisor).unwrap(),
        "packaged supervisor template must be canonical serde_json without LF"
    );
    let bootstrap: d::Stage8bP1BootstrapConfig =
        serde_json::from_value(supervisor["bootstrap"].clone()).unwrap();
    fs::create_dir_all(&bootstrap.durable_parent).unwrap();
    let validated = d::validate_stage8b_p1_bootstrap_config(bootstrap).unwrap();
    let identity = validated.operational_identity_sha256();
    assert_eq!(template["operational_identity_sha256"], identity);
    let policy = read("materialization-policy.json");
    assert_eq!(policy["schema_version"], 3);
    assert_eq!(policy["operational_identity_sha256"], identity);
    assert_eq!(supervisor["bootstrap"]["schema_version"], 2);
    assert_eq!(
        policy["market_data_policy_sha256"],
        d::STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256
    );
    assert_eq!(
        supervisor["bootstrap"]["market_data_policy_sha256"],
        policy["market_data_policy_sha256"]
    );
    let profile = Stage8bP1fO2MaterializedSourceV1::validate_template_profile(&source).unwrap();
    assert!(profile.no_riskgate());
    assert_eq!(
        supervisor["bootstrap"]["runtime_config_fingerprint_sha256"],
        profile.build_hybrid_runtime().unwrap().1
    );
    Stage8bP1fO2MaterializedSourceV1::validate_bars_interval(
        profile,
        time(policy["bars_start_utc"].as_str().unwrap()),
        time(policy["bars_end_utc"].as_str().unwrap()),
    )
    .unwrap();
    let spec: Value = serde_json::from_slice(&fs::read("/package/spec.json").unwrap()).unwrap();
    for boundary in ["execution_window_start_utc", "execution_window_end_utc"] {
        let now = time(spec["calendar"][boundary].as_str().unwrap());
        let plan =
            Stage8bP1fObservedM10Plan::from_calendar_template(&source, identity, now).unwrap();
        assert_eq!(
            plan.request_start(),
            time(policy["bars_start_utc"].as_str().unwrap())
        );
        assert_eq!(plan.request_end(), now);
        println!(
            "PASS calendar-boundary={boundary} digest={}",
            plan.calendar_sha256()
        );
    }
    let start = time(
        spec["calendar"]["execution_window_start_utc"]
            .as_str()
            .unwrap(),
    );
    let end = time(
        spec["calendar"]["execution_window_end_utc"]
            .as_str()
            .unwrap(),
    );
    for now in [
        start - chrono::Duration::seconds(1),
        end + chrono::Duration::days(1),
    ] {
        assert!(Stage8bP1fObservedM10Plan::from_calendar_template(&source, identity, now).is_err());
    }
    let mut old: d::Stage8bP1BootstrapConfig =
        serde_json::from_value(supervisor["bootstrap"].clone()).unwrap();
    old.schema_version = 1;
    old.market_data_policy_sha256 = None;
    let strict = d::validate_stage8b_p1_bootstrap_config(old).unwrap();
    assert_ne!(identity, strict.operational_identity_sha256());
    assert!(Stage8bP1fObservedM10Plan::from_calendar_template(
        &source,
        strict.operational_identity_sha256(),
        start
    )
    .is_err());
    println!("PASS accepted-rlib source-v4 contract; template-v3 calendar-only; schema2 policy3 identity={identity}; no root migration");
}
