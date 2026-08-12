use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use runtime_durable_service::{
    Stage7bDurablePaths, Stage7bDurableStorageError, Stage7bWritableDurableAuthority,
};
use strategy_runtime_core::{
    authorize_stage6d_first_boot, Stage6dFirstBootConfig, Stage6dOperationalIdentityConfig,
};

fn identity() -> Stage6dOperationalIdentityConfig {
    Stage6dOperationalIdentityConfig {
        broker_id: "paper".to_string(),
        strategy_instance_id: "hybrid-imoexf".to_string(),
        deployment_id: "stage7b-subprocess".to_string(),
        deployment_generation: 1,
        gateway_instance_id: "gateway-test".to_string(),
        instrument_map_fingerprint_sha256: "2".repeat(64),
        market_data_generation: 1,
        command_consumer_generation: 1,
    }
}

fn test_parent() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "stage7b-lock-subprocess-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&path).unwrap();
    fs::canonicalize(path).unwrap()
}

fn create_durable_root(parent: &Path) -> PathBuf {
    let root = parent.join(Stage7bDurablePaths::expected_directory_name(&identity()).unwrap());
    fs::create_dir(&root).unwrap();
    root
}

fn authorization() -> strategy_runtime_core::Stage6dFirstBootAuthorization {
    authorize_stage6d_first_boot(Stage6dFirstBootConfig {
        deployment_id: identity().deployment_id,
        expected_runtime_config_fingerprint_sha256: "3".repeat(64),
        allow_create_missing_journal: true,
    })
    .unwrap()
}

#[test]
#[ignore]
fn stage7b_writer_lock_holder_child() {
    let root = PathBuf::from(std::env::var_os("STAGE7B_CHILD_ROOT").unwrap());
    let ready = PathBuf::from(std::env::var_os("STAGE7B_CHILD_READY").unwrap());
    let paths = Stage7bDurablePaths::validate(root, &identity()).unwrap();
    let _authority = Stage7bWritableDurableAuthority::open_existing(paths, &identity()).unwrap();
    fs::write(ready, b"locked-and-storage-ready").unwrap();
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn stage7b_b_second_process_is_rejected_and_sigkill_releases_kernel_lock() {
    let parent = test_parent();
    let root = create_durable_root(&parent);
    let paths = Stage7bDurablePaths::validate(&root, &identity()).unwrap();
    drop(
        Stage7bWritableDurableAuthority::create_new(paths, &identity(), &authorization()).unwrap(),
    );

    let ready = parent.join("child-ready");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("--ignored")
        .arg("--exact")
        .arg("stage7b_writer_lock_holder_child")
        .arg("--nocapture")
        .env("STAGE7B_CHILD_ROOT", &root)
        .env("STAGE7B_CHILD_READY", &ready)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready.exists() && Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("lock-holder child exited before ready: {status}");
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(ready.exists(), "lock-holder child did not become ready");

    let journal_before_rejected_writer = fs::read(root.join("stage6.journal")).unwrap();
    let second_paths = Stage7bDurablePaths::validate(&root, &identity()).unwrap();
    assert!(matches!(
        Stage7bWritableDurableAuthority::open_existing(second_paths, &identity()),
        Err(Stage7bDurableStorageError::WriterAlreadyHeld)
    ));
    assert_eq!(
        fs::read(root.join("stage6.journal")).unwrap(),
        journal_before_rejected_writer
    );

    child.kill().unwrap();
    child.wait().unwrap();
    let recovered_paths = Stage7bDurablePaths::validate(&root, &identity()).unwrap();
    let recovered =
        Stage7bWritableDurableAuthority::open_existing(recovered_paths, &identity()).unwrap();
    drop(recovered);
    assert!(root.join("stage6.writer.lock").exists());
    fs::remove_dir_all(parent).unwrap();
}
