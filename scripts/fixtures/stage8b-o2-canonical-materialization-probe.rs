// Disposable --network=none Linux container only. No production key or authority.
// Exact accepted release rlibs; retained source bytes; explicit historical clock.
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use runtime_durable_service as d;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

const OPERATOR: &str = "/accepted-operator";
const ISSUER: &str = "offline-canonical-probe";
const HOST_KEY: &str = "SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo";
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn timestamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}
fn read(name: &str) -> Vec<u8> {
    fs::read(format!("/package/payload/{name}")).unwrap()
}
fn directory(path: &str, mode: u32) {
    assert!(
        !Path::new(path).exists(),
        "probe requires a fresh disposable root"
    );
    fs::create_dir_all(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn signed(kind: &str, value: Value) -> Vec<u8> {
    let input = format!("/tmp/{kind}.input.json");
    let output = format!("/tmp/{kind}.signed.json");
    fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
    let status = Command::new(OPERATOR)
        .args([kind, "/tmp/fixture.key", &input, &output])
        .status()
        .unwrap();
    assert!(status.success(), "fixture signing failed");
    fs::read(output).unwrap()
}
fn main() {
    assert!(Path::new("/.dockerenv").is_file());
    assert_eq!(std::env::var("O2_DISPOSABLE_PROBE").unwrap(), "1");
    assert_eq!(std::env::consts::OS, "linux");
    let staged = fs::read("/private-input/retained-staged.json").unwrap();
    assert_eq!(
        sha(&staged),
        "5c7ba133f673d9f46916ee092781613e342044be7d5381c37aeb011bc2e307b7"
    );
    let envelope: Value = serde_json::from_slice(&staged).unwrap();
    let manifest = envelope["manifest_sha256"].as_str().unwrap();
    let template = read("supervisor.template.json");
    let original = fs::read("/original-template.json").unwrap();
    assert_eq!(
        sha(&original),
        "04aed2dc5cdf5bdc44d48cf28c91986bd71e266f2965f76200caf796d87e191e"
    );
    assert_eq!(original, [template.as_slice(), b"\n"].concat());
    let value: Value = serde_json::from_slice(&template).unwrap();
    assert_eq!(template, serde_json::to_vec(&value).unwrap());
    directory(
        value["bootstrap"]["durable_parent"].as_str().unwrap(),
        0o700,
    );
    let source_template = read("source-template.json");
    let policy = read("materialization-policy.json");
    let now = DateTime::parse_from_rfc3339("2026-10-06T04:15:05.318837Z")
        .unwrap()
        .with_timezone(&Utc);
    let checked = d::check_stage8b_p1f_staged_source(
        &staged,
        manifest,
        &policy,
        &template,
        &source_template,
        now,
    )
    .unwrap();
    assert_eq!(
        sha(checked.source_bytes()),
        "e669f6a1c6632e953a5292af1c958fb14f7a3cc99c54c6ac462bf3a30185adaa"
    );
    assert_eq!(
        checked.source_bytes(),
        envelope["exact_source_json"].as_str().unwrap().as_bytes()
    );
    assert!(d::check_stage8b_p1f_staged_source(
        &staged,
        manifest,
        &policy,
        &template,
        &source_template,
        now + Duration::seconds(301),
    )
    .is_err());
    println!("PASS exact retained V4 staged admission; stale source still rejected");

    directory(d::STAGE8B_P1F_AUTHORITY_CONTROL_ROOT, 0o750);
    directory(d::STAGE8B_P1F_CONFIG_ROOT, 0o750);
    directory("/etc/moex-finam-p1-paper/bootstrap", 0o750);
    let store = d::Stage8bP1fAuthorityStoreV1::open_production(0).unwrap();
    // Deliberately public fixture seed. Never derived from a real ceremony.
    fs::write("/tmp/fixture.key", [42_u8; 32]).unwrap();
    fs::set_permissions("/tmp/fixture.key", fs::Permissions::from_mode(0o600)).unwrap();
    let key = Command::new(OPERATOR)
        .args(["public-key", "/tmp/fixture.key"])
        .output()
        .unwrap();
    assert!(key.status.success());
    let key = String::from_utf8(key.stdout).unwrap().trim().to_string();
    let nonce = "1".repeat(64);
    let installation_id = "offline-canonical-regression";
    let mut head = b"moex.stage8b.p1f.authority-genesis.v1\0".to_vec();
    for s in [
        installation_id,
        d::STAGE8B_P1F_TARGET_HOST_ID,
        d::STAGE8B_P1F_AUTHORITY_CONTROL_ROOT,
    ] {
        head.extend(s.as_bytes());
        head.push(0);
    }
    head.extend(1_u64.to_be_bytes());
    head.extend(nonce.as_bytes());
    let genesis = signed(
        "sign-genesis",
        json!({
            "schema_version":1, "domain":"stage8b-p1f-genesis-manifest-v1",
            "installation_id":installation_id, "target_host_id":d::STAGE8B_P1F_TARGET_HOST_ID,
            "target_host_ssh_ed25519_sha256":HOST_KEY,
            "control_root":d::STAGE8B_P1F_AUTHORITY_CONTROL_ROOT, "authority_generation":1,
            "ceremony_nonce_sha256":nonce, "genesis_head_sha256":sha(&head),
            "not_before_utc":timestamp(now-Duration::seconds(5)), "expires_at_utc":timestamp(now+Duration::hours(1)),
            "issuer_key_id":ISSUER, "signature_ed25519_hex":""
        }),
    );
    let receipt = store
        .initialize_authority(&genesis, &key, ISSUER, now)
        .unwrap();
    let activation = signed(
        "sign-activation",
        json!({
            "schema_version":1,"domain":"stage8b-p1f-activation-certificate-v1",
            "installation_id":installation_id,"target_host_id":d::STAGE8B_P1F_TARGET_HOST_ID,
            "control_root":d::STAGE8B_P1F_AUTHORITY_CONTROL_ROOT,"authority_generation":1,
            "ceremony_nonce_sha256":nonce,"genesis_manifest_sha256":sha(&genesis),
            "genesis_receipt_sha256":sha(&serde_json::to_vec(&receipt).unwrap()),"genesis_head_sha256":sha(&head),
            "activated_at_utc":timestamp(now),"expires_at_utc":timestamp(now+Duration::hours(1)),
            "issuer_key_id":ISSUER,"signature_ed25519_hex":""
        }),
    );
    store
        .activate_authority(&activation, &key, ISSUER, now)
        .unwrap();
    let predecessor = store.inspect().unwrap();
    let phase = signed(
        "sign-phase",
        json!({
            "schema_version":1,"domain":"stage8b-p1f-phase-manifest-v1",
            "installation_id":installation_id,"target_host_id":d::STAGE8B_P1F_TARGET_HOST_ID,
            "target_host_ssh_ed25519_sha256":HOST_KEY,
            "authority_generation":1,"authority_sequence":predecessor.latest_sequence+1,
            "predecessor_event_sha256":predecessor.latest_event_sha256,"accepted_source_tree_sha256":"7".repeat(64),
            "phase":"O2_MATERIALIZE_BOOTSTRAP","materialization_policy_sha256":sha(&policy),
            "config_template_sha256":sha(&template),"installation_sha256":"8".repeat(64),
            "controller_id":ISSUER,"not_before_utc":timestamp(now),"deadline_utc":timestamp(now+Duration::minutes(10)),
            "issuer_key_id":ISSUER,"signature_ed25519_hex":""
        }),
    );
    store.claim_phase(&phase, &key, ISSUER, now).unwrap();
    let phase_sha = sha(&phase);
    let before = store.inspect().unwrap();
    for wrong in [
        original,
        [template.as_slice(), b"\r\n"].concat(),
        serde_json::to_vec_pretty(&value).unwrap(),
    ] {
        assert_eq!(
            store
                .materialize_o2(
                    &phase_sha,
                    &policy,
                    &wrong,
                    checked.source_bytes(),
                    checked.broker_truth_checked_at_utc(),
                    now
                )
                .unwrap_err(),
            d::Stage8bP1fAuthorityErrorV1::InvalidDocument
        );
        assert_eq!(store.inspect().unwrap(), before);
        assert!(!Path::new("/etc/moex-finam-p1-paper/supervisor.json").exists());
        assert_eq!(
            fs::read_dir("/etc/moex-finam-p1-paper/bootstrap")
                .unwrap()
                .count(),
            0
        );
    }
    println!("PASS real guardian rejects LF/CRLF/pretty templates without materialization writes");
    let receipt = store
        .materialize_o2(
            &phase_sha,
            &policy,
            &template,
            checked.source_bytes(),
            checked.broker_truth_checked_at_utc(),
            now,
        )
        .unwrap();
    assert_eq!(receipt.state, "ReadyForBootstrap");
    let reread_source =
        fs::read("/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json")
            .unwrap();
    let reread_config = fs::read("/etc/moex-finam-p1-paper/supervisor.json").unwrap();
    assert_eq!(reread_source, checked.source_bytes());
    assert_eq!(sha(&reread_source), receipt.source_sha256);
    assert_eq!(sha(&reread_config), receipt.final_config_sha256);
    let final_config: Value = serde_json::from_slice(&reread_config).unwrap();
    assert_eq!(
        final_config["first_boot_source_bundle_sha256"],
        receipt.source_sha256
    );
    for p in [
        "/etc/moex-finam-p1-paper/supervisor.json",
        "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json",
    ] {
        assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o440);
    }
    let after = store.inspect().unwrap();
    assert_eq!(after.latest_sequence, before.latest_sequence + 1);
    assert_eq!(
        store
            .materialize_o2(
                &phase_sha,
                &policy,
                &template,
                checked.source_bytes(),
                checked.broker_truth_checked_at_utc(),
                now
            )
            .unwrap(),
        receipt
    );
    assert_eq!(store.inspect().unwrap(), after);
    assert_eq!(
        fs::read_dir(value["bootstrap"]["durable_parent"].as_str().unwrap())
            .unwrap()
            .count(),
        0
    );
    println!("PASS accepted release guardian: ReadyForBootstrap, exact source/config reread, mode0440, idempotent replay");
    println!("PASS offline fixture only: no bootstrap process, Redis, FINAM, systemd or production authority");
}
