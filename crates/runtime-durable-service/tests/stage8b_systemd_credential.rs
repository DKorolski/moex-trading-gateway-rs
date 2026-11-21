//! Run only through scripts/stage8b_p1_systemd_credential_probe.py on Linux.
//! Uses public synthetic bytes, never an installed lifecycle key or bootstrap.

#[test]
#[ignore = "requires a native systemd service with a synthetic credential"]
fn native_systemd_credential_loader() {
    let expected = match std::env::var("MOEX_CREDENTIAL_PROBE_EXPECT").as_deref() {
        Ok("accept") => true,
        Ok("reject") => false,
        _ => panic!("run only through the synthetic-credential probe"),
    };
    let result = runtime_durable_service::load_stage8b_p1_commitment_key_from_systemd_credential();
    assert_eq!(
        result.is_ok(),
        expected,
        "production credential loader result"
    );
    // Neither key bytes nor a key fingerprint are emitted.
    println!("CREDENTIAL_PROBE_PASS expected_accept={expected}");
}
