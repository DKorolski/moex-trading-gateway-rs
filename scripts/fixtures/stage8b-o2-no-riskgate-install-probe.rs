// Container-only read-only input validation against accepted release rlibs.
use finam_gateway::Stage8bP1fO2MaterializedSourceV1;
use runtime_durable_service::{
    validate_stage8b_p1_bootstrap_config, Stage8bP1BootstrapConfig, Stage8bP1RuntimeProfileV2,
    STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V3_SHA256,
};

fn main() {
    let source = std::fs::read("/package/payload/source-template.json").unwrap();
    let profile = Stage8bP1fO2MaterializedSourceV1::validate_template_profile(&source).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&source).unwrap();
    let supervisor: serde_json::Value = serde_json::from_slice(
        &std::fs::read("/package/payload/supervisor.template.json").unwrap(),
    )
    .unwrap();
    let bootstrap: Stage8bP1BootstrapConfig =
        serde_json::from_value(supervisor["bootstrap"].clone()).unwrap();
    std::fs::create_dir_all(&bootstrap.durable_parent).unwrap(); // disposable container only
    let validated = validate_stage8b_p1_bootstrap_config(bootstrap).unwrap();
    assert_eq!(
        value["operational_identity_sha256"],
        validated.operational_identity_sha256()
    );
    let (_, fingerprint) = Stage8bP1RuntimeProfileV2::build_hybrid_runtime().unwrap();
    assert_eq!(
        supervisor["bootstrap"]["runtime_config_fingerprint_sha256"],
        fingerprint
    );
    let policy: serde_json::Value = serde_json::from_slice(
        &std::fs::read("/package/payload/materialization-policy.json").unwrap(),
    )
    .unwrap();
    Stage8bP1fO2MaterializedSourceV1::validate_bars_interval(
        profile,
        policy["bars_start_utc"].as_str().unwrap().parse().unwrap(),
        policy["bars_end_utc"].as_str().unwrap().parse().unwrap(),
    )
    .unwrap();
    assert_eq!(
        STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V3_SHA256,
        "d722d70a897578ce93217f34c82dff2a7ed6c6c402a12914b7b862c2d95b693c"
    );
    println!(
        "PASS accepted-rlib source-v3 calendar transport operational-identity={} runtime-fingerprint={}",
        validated.operational_identity_sha256(), fingerprint
    );
}
