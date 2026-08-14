use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static TEST_DIRECTORY_COUNTER: AtomicU64 = AtomicU64::new(1);

use runtime_durable_service::{
    Stage7bDurableRootAuthority, Stage7bDurableStorageError, Stage7bStorageOpenPhase,
    Stage7bWritableDurableAuthority,
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
        "stage7b-lock-subprocess-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        TEST_DIRECTORY_COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::canonicalize(path).unwrap()
}

fn create_durable_root(parent: &Path) -> PathBuf {
    let root =
        parent.join(Stage7bDurableRootAuthority::expected_directory_name(&identity()).unwrap());
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
    let paths = Stage7bDurableRootAuthority::validate(root, &identity()).unwrap();
    let _authority = Stage7bWritableDurableAuthority::open_existing(paths, &identity()).unwrap();
    fs::write(ready, b"locked-and-storage-ready").unwrap();
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

#[test]
#[ignore]
fn stage7b_root_replacement_barrier_child() {
    let root = PathBuf::from(std::env::var_os("STAGE7B_CHILD_ROOT").unwrap());
    let ready = PathBuf::from(std::env::var_os("STAGE7B_CHILD_READY").unwrap());
    let resume = PathBuf::from(std::env::var_os("STAGE7B_CHILD_RESUME").unwrap());
    let paths = Stage7bDurableRootAuthority::validate(root, &identity()).unwrap();
    let result = Stage7bWritableDurableAuthority::open_existing_with_phase_observer(
        paths,
        &identity(),
        |phase| {
            if phase == Stage7bStorageOpenPhase::WriterLockAcquired {
                fs::write(&ready, b"writer-lock-acquired").unwrap();
                let deadline = Instant::now() + Duration::from_secs(15);
                while !resume.exists() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(10));
                }
                assert!(resume.exists(), "parent did not release phase barrier");
            }
        },
    );
    assert!(matches!(
        result,
        Err(Stage7bDurableStorageError::RootIdentityDrift)
    ));
}

#[test]
#[ignore]
fn stage7b_e_x02_header_written_before_sync_child() {
    let root = PathBuf::from(std::env::var_os("STAGE7B_CHILD_ROOT").unwrap());
    let barrier = PathBuf::from(std::env::var_os("STAGE7B_CHILD_READY").unwrap());
    let escaped = PathBuf::from(std::env::var_os("STAGE7B_CHILD_RESUME").unwrap());
    let paths = Stage7bDurableRootAuthority::validate(root, &identity()).unwrap();
    let _authority = Stage7bWritableDurableAuthority::create_new_with_pre_journal_sync_observer(
        paths,
        &identity(),
        &authorization(),
        || {
            fs::write(&barrier, b"complete-journal-header-before-first-sync").unwrap();
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        },
    )
    .unwrap();
    fs::write(escaped, b"storage-ready-escaped").unwrap();
}

fn wait_until_ready(child: &mut std::process::Child, ready: &Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready.exists() && Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("child exited before ready: {status}");
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(ready.exists(), "child did not become ready");
}

#[test]
fn stage7b_b_second_process_is_rejected_and_sigkill_releases_kernel_lock() {
    let parent = test_parent();
    let root = create_durable_root(&parent);
    let paths = Stage7bDurableRootAuthority::validate(&root, &identity()).unwrap();
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

    wait_until_ready(&mut child, &ready);

    let journal_before_rejected_writer = fs::read(root.join("stage6.journal")).unwrap();
    let second_paths = Stage7bDurableRootAuthority::validate(&root, &identity()).unwrap();
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
    let recovered_paths = Stage7bDurableRootAuthority::validate(&root, &identity()).unwrap();
    let recovered =
        Stage7bWritableDurableAuthority::open_existing(recovered_paths, &identity()).unwrap();
    drop(recovered);
    assert!(root.join("stage6.writer.lock").exists());
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn stage7b_e_x01_sigkill_after_lock_before_journal_open_releases_lock() {
    let parent = test_parent();
    let root = create_durable_root(&parent);
    let paths = Stage7bDurableRootAuthority::validate(&root, &identity()).unwrap();
    drop(
        Stage7bWritableDurableAuthority::create_new(paths, &identity(), &authorization()).unwrap(),
    );
    let journal_before = fs::read(root.join("stage6.journal")).unwrap();

    let ready = parent.join("x01-lock-acquired");
    let never_resume = parent.join("x01-never-resume");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("--ignored")
        .arg("--exact")
        .arg("stage7b_root_replacement_barrier_child")
        .arg("--nocapture")
        .env("STAGE7B_CHILD_ROOT", &root)
        .env("STAGE7B_CHILD_READY", &ready)
        .env("STAGE7B_CHILD_RESUME", &never_resume)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_until_ready(&mut child, &ready);
    child.kill().unwrap();
    child.wait().unwrap();

    let recovered = Stage7bDurableRootAuthority::validate(&root, &identity()).unwrap();
    drop(Stage7bWritableDurableAuthority::open_existing(recovered, &identity()).unwrap());
    assert_eq!(
        fs::read(root.join("stage6.journal")).unwrap(),
        journal_before
    );
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn stage7b_e_x02_sigkill_after_new_journal_header_before_sync_is_conservative() {
    let parent = test_parent();
    let root = create_durable_root(&parent);
    let barrier = parent.join("x02-header-written");
    let escaped = parent.join("x02-storage-ready-escaped");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("--ignored")
        .arg("--exact")
        .arg("stage7b_e_x02_header_written_before_sync_child")
        .arg("--nocapture")
        .env("STAGE7B_CHILD_ROOT", &root)
        .env("STAGE7B_CHILD_READY", &barrier)
        .env("STAGE7B_CHILD_RESUME", &escaped)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_until_ready(&mut child, &barrier);
    assert!(
        !escaped.exists(),
        "writable authority escaped before first sync"
    );
    child.kill().unwrap();
    child.wait().unwrap();

    assert!(
        !escaped.exists(),
        "StorageReady escaped across the X02 crash"
    );
    let journal = root.join("stage6.journal");
    assert!(journal.is_file());
    let recovered = Stage7bDurableRootAuthority::validate(&root, &identity()).unwrap();
    match Stage7bWritableDurableAuthority::open_existing(recovered, &identity()) {
        Ok(authority) => {
            assert!(strategy_runtime_core::Stage6JournalBackend::records(&authority).is_empty());
            drop(authority);
        }
        Err(Stage7bDurableStorageError::Journal(_)) => {
            // A filesystem that did not retain a complete header is handled by
            // the explicit fail-closed restart policy.
        }
        Err(error) => panic!("unexpected X02 restart result: {error}"),
    }
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn stage7b_b_root_replacement_between_lock_and_journal_fails_closed() {
    let parent = test_parent();
    let root = create_durable_root(&parent);
    let paths = Stage7bDurableRootAuthority::validate(&root, &identity()).unwrap();
    drop(
        Stage7bWritableDurableAuthority::create_new(paths, &identity(), &authorization()).unwrap(),
    );

    let ready = parent.join("root-race-ready");
    let resume = parent.join("root-race-resume");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("--ignored")
        .arg("--exact")
        .arg("stage7b_root_replacement_barrier_child")
        .arg("--nocapture")
        .env("STAGE7B_CHILD_ROOT", &root)
        .env("STAGE7B_CHILD_READY", &ready)
        .env("STAGE7B_CHILD_RESUME", &resume)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_until_ready(&mut child, &ready);

    let renamed = parent.join("original-root-renamed-after-lock");
    fs::rename(&root, &renamed).unwrap();
    fs::create_dir(&root).unwrap();
    fs::write(&resume, b"continue").unwrap();
    assert!(child.wait().unwrap().success());
    assert!(!root.join("stage6.journal").exists());
    assert!(renamed.join("stage6.journal").is_file());
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn stage7b_b_replaced_lock_path_cannot_admit_second_writer() {
    let parent = test_parent();
    let root = create_durable_root(&parent);
    let paths = Stage7bDurableRootAuthority::validate(&root, &identity()).unwrap();
    drop(
        Stage7bWritableDurableAuthority::create_new(paths, &identity(), &authorization()).unwrap(),
    );

    let ready = parent.join("lock-replacement-ready");
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
    wait_until_ready(&mut child, &ready);

    fs::remove_file(root.join("stage6.writer.lock")).unwrap();
    fs::write(root.join("stage6.writer.lock"), b"replacement").unwrap();
    let second = Stage7bDurableRootAuthority::validate(&root, &identity()).unwrap();
    assert!(matches!(
        Stage7bWritableDurableAuthority::open_existing(second, &identity()),
        Err(Stage7bDurableStorageError::WriterAlreadyHeld)
    ));

    child.kill().unwrap();
    child.wait().unwrap();
    let recovered = Stage7bDurableRootAuthority::validate(&root, &identity()).unwrap();
    drop(Stage7bWritableDurableAuthority::open_existing(recovered, &identity()).unwrap());
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn stage7b_b_replaced_root_after_ready_cannot_admit_second_writer() {
    let parent = test_parent();
    let root = create_durable_root(&parent);
    let paths = Stage7bDurableRootAuthority::validate(&root, &identity()).unwrap();
    drop(
        Stage7bWritableDurableAuthority::create_new(paths, &identity(), &authorization()).unwrap(),
    );

    let ready = parent.join("ready-root-replacement-ready");
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
    wait_until_ready(&mut child, &ready);

    let renamed = parent.join("ready-original-root-renamed");
    fs::rename(&root, &renamed).unwrap();
    fs::create_dir(&root).unwrap();
    let replacement = Stage7bDurableRootAuthority::validate(&root, &identity()).unwrap();
    assert!(matches!(
        Stage7bWritableDurableAuthority::open_existing(replacement, &identity()),
        Err(Stage7bDurableStorageError::WriterAlreadyHeld)
    ));
    assert!(!root.join("stage6.writer.lock").exists());
    assert!(!root.join("stage6.journal").exists());

    child.kill().unwrap();
    child.wait().unwrap();
    fs::remove_dir_all(parent).unwrap();
}
