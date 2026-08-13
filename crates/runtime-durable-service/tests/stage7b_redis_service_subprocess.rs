use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use runtime_durable_service::Stage7bRedisServiceConfig;

fn scratch_directory() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "stage7b-d-c-boot-identity-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must follow the Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&path).expect("create subprocess scratch directory");
    path
}

#[test]
#[ignore]
fn stage7b_d_c_b068_boot_identity_child() {
    let output = PathBuf::from(
        std::env::var_os("STAGE7B_D_C_BOOT_IDENTITY_OUTPUT")
            .expect("child output path must be supplied"),
    );
    let config = Stage7bRedisServiceConfig::paper_default_auto("subprocess-boot")
        .expect("child must construct a valid paper config");
    fs::write(output, config.consumer_name).expect("child must persist its consumer identity");
}

fn run_boot_child(output: &Path) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("stage7b_d_c_b068_boot_identity_child")
        .arg("--nocapture")
        .env("STAGE7B_D_C_BOOT_IDENTITY_OUTPUT", output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn Stage 7B-d-c boot child");
    assert!(status.success(), "boot identity child failed: {status}");
}

#[test]
fn stage7b_d_c_b068_new_process_boot_uuid_is_unique() {
    let scratch = scratch_directory();
    let first_path = scratch.join("first-consumer");
    let second_path = scratch.join("second-consumer");

    run_boot_child(&first_path);
    run_boot_child(&second_path);

    let first = fs::read_to_string(first_path).expect("first child identity");
    let second = fs::read_to_string(second_path).expect("second child identity");
    assert!(first.starts_with("stage7b-boot-"));
    assert!(second.starts_with("stage7b-boot-"));
    assert_ne!(
        first, second,
        "process boots must not reuse consumer identity"
    );

    fs::remove_dir_all(scratch).expect("remove subprocess scratch directory");
}
