//! Offline config regression linked to the unchanged production library.
//! No Redis, HMAC/ceremony access, bootstrap, restored-root edits or signing.
use runtime_durable_service as d;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{env, fs, path::Path};

fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn validate(raw: &[u8], parent: &Path, boot: u8) -> d::Stage8bP1eValidatedSupervisorConfigV1 {
    let mut c = d::parse_stage8b_p1e_supervisor_config_v1(raw).unwrap();
    // Only a local existing directory for filesystem validation. Neither this
    // path nor the consumer boot ID contributes to manifest/strategy identity.
    c.bootstrap.durable_parent = parent.into();
    d::validate_stage8b_p1e_supervisor_config_v1(c, [boot; 16]).unwrap()
}

fn main() {
    let a: Vec<String> = env::args().collect();
    let before = fs::read(Path::new(&a[1]).join("supervisor.before.json")).unwrap();
    let after = fs::read(Path::new(&a[1]).join("supervisor.after.json")).unwrap();
    let manifest = fs::read(Path::new(&a[1]).join("redis-manifest.json")).unwrap();
    let parent = Path::new(&a[2]);
    assert_eq!(
        hash(&before),
        "2d28d3c2d9fb3e4933a2f89e3b751dfe67d56b14d1e4f542bbc838e0513c56d6"
    );
    assert_eq!(
        hash(&after),
        "3a031f24ac91990bb4b00308fad93d48b36af09d135ab5d4ecc62e4702e5c3e8"
    );
    let old: Value = serde_json::from_slice(&before).unwrap();
    let mut changed = old.clone();
    changed["redis_deployment_manifest_sha256"] = Value::String(hash(&manifest));
    assert_eq!(serde_json::to_vec(&changed).unwrap(), after);
    println!("PASS exact-one-field-config-change");
    for boot in [1, 2] {
        let b = validate(&before, parent, boot);
        let n = validate(&after, parent, boot);
        let bm = d::stage8b_p1f_o3_redis_manifest_v1(&b).unwrap();
        let nm = d::stage8b_p1f_o3_redis_manifest_v1(&n).unwrap();
        assert_eq!(bm, manifest);
        assert_eq!(nm, manifest);
        assert_eq!(
            hash(&nm),
            "4b9162f0f87d55f4cca3b8efac9d33491f0d3f421eb090e8cdd78fcb0f3e86b3"
        );
        assert_ne!(
            hash(&bm),
            old["redis_deployment_manifest_sha256"].as_str().unwrap()
        );
        assert_eq!(
            b.bootstrap().operational_identity_sha256(),
            n.bootstrap().operational_identity_sha256()
        );
        assert_eq!(
            n.bootstrap().operational_identity_sha256(),
            "fbd9ceb7ee964944e79db559c4e4d5cc7af54578b653ce218e183d01fa1444c3"
        );
        assert_eq!(
            b.first_boot_source_bundle_sha256(),
            n.first_boot_source_bundle_sha256()
        );
        println!("PASS production-constructor-old-reject-new-match-boot-{boot}-same-root-source");
    }
    let mut c = d::parse_stage8b_p1e_supervisor_config_v1(&after).unwrap();
    c.bootstrap.durable_parent = parent.into();
    c.bootstrap.deployment_generation += 1;
    let drift = d::validate_stage8b_p1e_supervisor_config_v1(c, [1; 16]).unwrap();
    assert_ne!(
        hash(&d::stage8b_p1f_o3_redis_manifest_v1(&drift).unwrap()),
        hash(&manifest)
    );
    println!("PASS foreign-generation-not-bound-by-new-pin");
    let mut c = d::parse_stage8b_p1e_supervisor_config_v1(&after).unwrap();
    c.bootstrap.durable_parent = parent.into();
    c.redis_url = "redis://127.0.0.1:6379/0".into();
    assert!(d::validate_stage8b_p1e_supervisor_config_v1(c, [1; 16]).is_err());
    println!("PASS DB0-rejected");
    println!("PASS CONFIG_ONLY_REGRESSION_NO_RUNTIME_OR_REDIS_EXECUTION");
}
