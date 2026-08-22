use broker_cli::invoke_stage8b_no_send_from_cli;
use finam_gateway::Stage8bOperatorInvocationRequest;
use std::fs;

#[test]
fn broker_cli_reaches_only_the_public_redacted_no_send_facade() {
    let root = std::env::temp_dir().join(format!(
        "stage8b-i-cli-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let package = root.join("accepted.zip");
    fs::write(&package, b"accepted-stage8b-package").unwrap();
    fs::write(
        root.join("stage8b-run-manifest.json"),
        b"{\"schema_version\":1}",
    )
    .unwrap();

    let diagnostic = invoke_stage8b_no_send_from_cli(Stage8bOperatorInvocationRequest::new(
        "INVOCATION_CLI_TEST_0001",
        package,
        &root,
    ))
    .unwrap();
    assert!(diagnostic.no_send);
    assert!(!diagnostic.authority_constructed);
    assert_eq!(diagnostic.evidence_files_pinned, 2);
    fs::remove_dir_all(root).unwrap();
}
