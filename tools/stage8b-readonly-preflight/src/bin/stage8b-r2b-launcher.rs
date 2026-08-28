//! Root-owned R2B admission supervisor and immutable terminal finalizer.

#[cfg(target_os = "linux")]
use serde::de::DeserializeOwned;
#[cfg(target_os = "linux")]
use sha2::{Digest, Sha256};
#[cfg(target_os = "linux")]
use std::ffi::CString;
#[cfg(target_os = "linux")]
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
#[cfg(target_os = "linux")]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

#[cfg(target_os = "linux")]
use stage8b_readonly_preflight::r2a5::{self, R2bAdmissionReceiptV1, R2bAdmissionState};

#[cfg(target_os = "linux")]
const HELPER: &str = "/opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight";
#[cfg(any(target_os = "linux", test))]
const ACCEPTED_SHA256: &str =
    include_str!("../../../../docs/stage-8/stage8b-p-r2b-accepted-helper-sha256.txt");
#[cfg(target_os = "linux")]
const CHILD_TIMEOUT_MS: i32 = 120_000;
#[cfg(target_os = "linux")]
const MAX_FRAME_BYTES: usize = 512 * 1024;

#[cfg(any(target_os = "linux", test))]
fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(any(target_os = "linux", test))]
fn accepted_helper_sha256() -> Result<String, Box<dyn std::error::Error>> {
    let value = ACCEPTED_SHA256.trim();
    if !valid_sha256(value) {
        return Err("invalid accepted R2B helper SHA-256".into());
    }
    Ok(value.to_owned())
}

#[cfg(target_os = "linux")]
fn duplicate_high(fd: RawFd) -> Result<std::fs::File, Box<dyn std::error::Error>> {
    let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 20) };
    if duplicate == -1 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { std::fs::File::from_raw_fd(duplicate) })
}

#[cfg(target_os = "linux")]
fn open_accepted_helper(
    expected_sha256: &str,
) -> Result<std::fs::File, Box<dyn std::error::Error>> {
    let mut opened = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(HELPER)?;
    let metadata = opened.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.gid() != 0
        || metadata.nlink() != 1
        || metadata.mode() & 0o022 != 0
        || metadata.mode() & 0o111 == 0
        || metadata.mode() & (libc::S_ISUID | libc::S_ISGID) != 0
    {
        return Err("R2B helper custody or privilege-bit mismatch".into());
    }
    let capability_name = CString::new("security.capability")?;
    let capability_size = unsafe {
        libc::fgetxattr(
            opened.as_raw_fd(),
            capability_name.as_ptr(),
            std::ptr::null_mut(),
            0,
        )
    };
    let capability_errno = std::io::Error::last_os_error().raw_os_error();
    if capability_size >= 0
        || (capability_errno != Some(libc::ENODATA) && capability_errno != Some(libc::ENOTSUP))
    {
        return Err("R2B helper file capability present or unverifiable".into());
    }
    let mut bytes = Vec::new();
    opened.read_to_end(&mut bytes)?;
    if format!("{:x}", Sha256::digest(&bytes)) != expected_sha256 {
        return Err("R2B helper hash mismatch".into());
    }
    opened.seek(SeekFrom::Start(0))?;
    duplicate_high(opened.as_raw_fd())
}

#[cfg(target_os = "linux")]
fn current_executable_sha256() -> Result<String, Box<dyn std::error::Error>> {
    Ok(format!(
        "{:x}",
        Sha256::digest(std::fs::read("/proc/self/exe")?)
    ))
}

#[cfg(target_os = "linux")]
fn socket_pair() -> Result<(std::fs::File, std::fs::File), Box<dyn std::error::Error>> {
    let mut pair = [-1; 2];
    if unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_CLOEXEC,
            0,
            pair.as_mut_ptr(),
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe {
        (
            std::fs::File::from_raw_fd(pair[0]),
            std::fs::File::from_raw_fd(pair[1]),
        )
    })
}

#[cfg(target_os = "linux")]
fn sealed_receipt(receipt: &[u8]) -> Result<std::fs::File, Box<dyn std::error::Error>> {
    let name = CString::new("stage8b-r2b-root-admission-v1")?;
    let fd =
        unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_ALLOW_SEALING | libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.write_all(receipt)?;
    file.sync_all()?;
    file.set_permissions(std::fs::Permissions::from_mode(0o400))?;
    file.seek(SeekFrom::Start(0))?;
    let seals = libc::F_SEAL_SEAL | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_WRITE;
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_ADD_SEALS, seals) } == -1 {
        return Err(std::io::Error::last_os_error().into());
    }
    duplicate_high(file.as_raw_fd())
}

#[cfg(target_os = "linux")]
fn open_root_record(
    path: &std::path::Path,
    device: u64,
    inode: u64,
) -> Result<std::fs::File, Box<dyn std::error::Error>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.gid() != 0
        || metadata.mode() & 0o777 != 0o400
        || metadata.nlink() != 1
        || metadata.dev() != device
        || metadata.ino() != inode
    {
        return Err("root admission record identity mismatch".into());
    }
    duplicate_high(file.as_raw_fd())
}

#[cfg(target_os = "linux")]
fn dup_to(source: RawFd, target: RawFd) -> Result<(), Box<dyn std::error::Error>> {
    if unsafe { libc::dup3(source, target, 0) } == -1 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn child_capability_sets_are_empty() -> Result<(), Box<dyn std::error::Error>> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    for name in ["CapInh:", "CapPrm:", "CapEff:", "CapAmb:"] {
        let value = status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .ok_or("missing Linux capability status")?
            .trim();
        if value != "0000000000000000" {
            return Err("child retained a Linux capability set".into());
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn close_non_allowlisted_descriptors() -> Result<(), Box<dyn std::error::Error>> {
    if unsafe { libc::close_range(8, u32::MAX, 0) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() != Some(libc::ENOSYS) {
        return Err(error.into());
    }

    // Older kernels and some Linux emulators do not implement close_range.
    // This child is single-threaded after fork, so snapshotting /proc/self/fd
    // and closing every descriptor above the exact 0..=7 allowlist is an
    // equivalent fail-closed fallback without an FD-creation race.
    let mut descriptors = Vec::new();
    for entry in std::fs::read_dir("/proc/self/fd")? {
        let name = entry?.file_name();
        let Some(name) = name.to_str() else {
            return Err("non-UTF8 descriptor entry".into());
        };
        let descriptor: RawFd = name.parse()?;
        if descriptor >= 8 {
            descriptors.push(descriptor);
        }
    }
    for descriptor in descriptors {
        unsafe {
            libc::close(descriptor);
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn child_exec(
    helper: RawFd,
    receipt: RawFd,
    terminal: RawFd,
    admission: RawFd,
    nonce: RawFd,
    controlled_custody: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    dup_to(receipt, r2a5::R2B_ADMISSION_RECEIPT_FD)?;
    dup_to(terminal, r2a5::R2B_TERMINAL_CHANNEL_FD)?;
    dup_to(admission, r2a5::R2B_ADMISSION_RECORD_FD)?;
    dup_to(nonce, r2a5::R2B_NONCE_MARKER_FD)?;
    dup_to(helper, r2a5::R2B_HELPER_EXECUTABLE_FD)?;
    close_non_allowlisted_descriptors()?;
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0
        || unsafe {
            libc::prctl(
                libc::PR_CAP_AMBIENT,
                libc::PR_CAP_AMBIENT_CLEAR_ALL,
                0,
                0,
                0,
            )
        } != 0
        || unsafe { libc::setgroups(0, std::ptr::null()) } != 0
        || unsafe {
            libc::setresgid(
                r2a5::R2B_EVIDENCE_GID,
                r2a5::R2B_EVIDENCE_GID,
                r2a5::R2B_EVIDENCE_GID,
            )
        } != 0
        || unsafe {
            libc::setresuid(
                r2a5::R2B_HELPER_UID,
                r2a5::R2B_HELPER_UID,
                r2a5::R2B_HELPER_UID,
            )
        } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut real_uid = 0;
    let mut effective_uid = 0;
    let mut saved_uid = 0;
    let mut real_gid = 0;
    let mut effective_gid = 0;
    let mut saved_gid = 0;
    if unsafe { libc::getresuid(&mut real_uid, &mut effective_uid, &mut saved_uid) } != 0
        || unsafe { libc::getresgid(&mut real_gid, &mut effective_gid, &mut saved_gid) } != 0
        || [real_uid, effective_uid, saved_uid] != [r2a5::R2B_HELPER_UID; 3]
        || [real_gid, effective_gid, saved_gid] != [r2a5::R2B_EVIDENCE_GID; 3]
        || unsafe { libc::prctl(libc::PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0) } != 1
    {
        return Err("R2B irreversible identity drop did not stick".into());
    }
    child_capability_sets_are_empty()?;
    #[cfg(feature = "stage8b-r2b-controlled-custody")]
    if controlled_custody && controlled_fault("FEXECVE_FAILURE") {
        return Err("controlled qualification fexecve failure".into());
    }
    let executable = CString::new(HELPER)?;
    let mode = CString::new(if controlled_custody {
        "--r2b-controlled-custody-one-shot"
    } else {
        "--r2b-one-shot"
    })?;
    let argv = [executable.as_ptr(), mode.as_ptr(), std::ptr::null()];
    let envp = [std::ptr::null()];
    unsafe {
        libc::fexecve(r2a5::R2B_HELPER_EXECUTABLE_FD, argv.as_ptr(), envp.as_ptr());
    }
    Err(std::io::Error::last_os_error().into())
}

#[cfg(all(target_os = "linux", feature = "stage8b-r2b-controlled-custody"))]
fn controlled_fault(expected: &str) -> bool {
    std::env::var("STAGE8B_R2B_CONTROLLED_FAULT").as_deref() == Ok(expected)
}

#[cfg(all(target_os = "linux", not(feature = "stage8b-r2b-controlled-custody")))]
fn controlled_fault(_expected: &str) -> bool {
    false
}

#[cfg(target_os = "linux")]
fn read_exact_timeout(fd: RawFd, output: &mut [u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut offset = 0;
    while offset < output.len() {
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLIN | libc::POLLHUP,
            revents: 0,
        };
        if unsafe { libc::poll(&mut poll, 1, CHILD_TIMEOUT_MS) } <= 0 {
            return Err("R2B helper terminal channel timeout".into());
        }
        let read = unsafe {
            libc::recv(
                fd,
                output[offset..].as_mut_ptr().cast(),
                output.len() - offset,
                0,
            )
        };
        if read <= 0 {
            return Err("R2B helper terminal channel closed".into());
        }
        offset += read as usize;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn read_frame<T: DeserializeOwned>(fd: RawFd) -> Result<T, Box<dyn std::error::Error>> {
    let mut size = [0_u8; 4];
    read_exact_timeout(fd, &mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    if size == 0 || size > MAX_FRAME_BYTES {
        return Err("R2B helper terminal frame size invalid".into());
    }
    let mut payload = vec![0; size];
    read_exact_timeout(fd, &mut payload)?;
    Ok(serde_json::from_slice(&payload)?)
}

#[cfg(feature = "stage8b-r2b-controlled-custody")]
fn controlled_custody_requested() -> Result<bool, Box<dyn std::error::Error>> {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    match arguments.as_slice() {
        [_program, mode] if mode == "--controlled-custody" => Ok(true),
        _ => Err("qualification supervisor requires --controlled-custody".into()),
    }
}

#[cfg(not(feature = "stage8b-r2b-controlled-custody"))]
fn controlled_custody_requested() -> Result<bool, Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("R2B production supervisor accepts no arguments".into());
    }
    Ok(false)
}

#[cfg(target_os = "linux")]
struct ChildCompletion {
    child_pid: Option<i32>,
    wait_status: Option<i32>,
    terminal: serde_json::Value,
    succeeded: bool,
}

#[cfg(target_os = "linux")]
fn validated_terminal(
    frame: serde_json::Value,
    receipt: &R2bAdmissionReceiptV1,
) -> Option<serde_json::Value> {
    let evidence = frame.get("evidence").cloned();
    let string_matches = |name: &str, expected: &str| {
        evidence
            .as_ref()
            .and_then(|value| value.get(name))
            .and_then(serde_json::Value::as_str)
            == Some(expected)
    };
    let closed = |name: &str| {
        evidence
            .as_ref()
            .and_then(|value| value.get(name))
            .and_then(serde_json::Value::as_bool)
            == Some(false)
    };
    if frame
        .get("message_type")
        .and_then(serde_json::Value::as_str)
        == Some("TERMINAL")
        && frame
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            == Some(1)
        && frame
            .get("admission_commitment_sha256")
            .and_then(serde_json::Value::as_str)
            == Some(receipt.admission_commitment_sha256.as_str())
        && string_matches("run_nonce_sha256", &receipt.run_nonce_sha256)
        && string_matches(
            "signed_run_package_sha256",
            &receipt.signed_run_package_sha256,
        )
        && string_matches(
            "helper_executable_sha256",
            &receipt.helper_executable_sha256,
        )
        && closed("operator_arm_issued")
        && closed("dispatch_attempt_recorded")
        && closed("effect_transport_entered")
        && closed("order_post_sent")
        && closed("order_delete_sent")
        && closed("raw_body_exported")
        && closed("credential_exported")
        && closed("account_id_exported")
    {
        evidence
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
#[allow(clippy::too_many_arguments)]
fn run_admitted_child(
    controlled: bool,
    helper: &std::fs::File,
    parent_channel: &std::fs::File,
    child_channel_raw: std::fs::File,
    child_channel: std::fs::File,
    receipt_bytes: &[u8],
    receipt: &R2bAdmissionReceiptV1,
    started: chrono::DateTime<chrono::Utc>,
) -> Result<ChildCompletion, Box<dyn std::error::Error>> {
    let sealed = sealed_receipt(receipt_bytes)?;
    let admission_path = std::path::Path::new(r2a5::PRODUCTION_ROOT)
        .join("admissions")
        .join(format!("{}.durable", receipt.run_nonce_sha256));
    let nonce_path = std::path::Path::new(r2a5::PRODUCTION_ROOT)
        .join("used-run-nonces")
        .join(&receipt.run_nonce_sha256);
    let admission = open_root_record(
        &admission_path,
        receipt.admission_record_device,
        receipt.admission_record_inode,
    )?;
    let nonce = open_root_record(
        &nonce_path,
        receipt.nonce_marker_device,
        receipt.nonce_marker_inode,
    )?;
    r2a5::record_r2b_supervisor_state(receipt_bytes, R2bAdmissionState::HelperExecAttempted)?;
    let child = unsafe { libc::fork() };
    if child == -1 {
        return Err(std::io::Error::last_os_error().into());
    }
    if child == 0 {
        unsafe { libc::close(parent_channel.as_raw_fd()) };
        if let Err(error) = child_exec(
            helper.as_raw_fd(),
            sealed.as_raw_fd(),
            child_channel.as_raw_fd(),
            admission.as_raw_fd(),
            nonce.as_raw_fd(),
            controlled,
        ) {
            eprintln!("stage8b-r2b-child-exec: {error}");
        }
        unsafe { libc::_exit(126) };
    }
    drop(child_channel_raw);
    drop(child_channel);
    let mut terminal: Option<serde_json::Value> = None;
    let mut lifecycle_state_failed = false;
    let first = read_frame::<serde_json::Value>(parent_channel.as_raw_fd());
    if let Ok(first) = first {
        if first
            .get("message_type")
            .and_then(serde_json::Value::as_str)
            == Some("HELPER_PROCESS_STARTED")
            && first
                .get("schema_version")
                .and_then(serde_json::Value::as_u64)
                == Some(1)
            && first
                .get("admission_commitment_sha256")
                .and_then(serde_json::Value::as_str)
                == Some(receipt.admission_commitment_sha256.as_str())
        {
            lifecycle_state_failed |= r2a5::record_r2b_supervisor_state(
                receipt_bytes,
                R2bAdmissionState::HelperProcessStarted,
            )
            .is_err();
            if controlled && controlled_fault("HELPER_CRASH_AFTER_STARTED") {
                unsafe { libc::kill(child, libc::SIGKILL) };
            } else if let Ok(frame) = read_frame::<serde_json::Value>(parent_channel.as_raw_fd()) {
                if let Some(evidence) = validated_terminal(frame, receipt) {
                    lifecycle_state_failed |= r2a5::record_r2b_supervisor_state(
                        receipt_bytes,
                        R2bAdmissionState::HelperTerminalReceived,
                    )
                    .is_err();
                    terminal = Some(evidence);
                }
            }
        }
    }
    let mut status = 0;
    if terminal.is_none() {
        unsafe { libc::kill(child, libc::SIGKILL) };
    }
    let waited = unsafe { libc::waitpid(child, &mut status, 0) } == child;
    let success = waited && libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0;
    lifecycle_state_failed |= r2a5::record_r2b_supervisor_state(
        receipt_bytes,
        if success {
            R2bAdmissionState::HelperExitedSuccess
        } else {
            R2bAdmissionState::HelperExitedFailure
        },
    )
    .is_err();
    if lifecycle_state_failed || terminal.is_none() || !waited {
        terminal = Some(serde_json::to_value(
            r2a5::r2b_supervisor_fallback_terminal(
                receipt,
                started,
                if lifecycle_state_failed {
                    "SUPERVISOR_STATE_PERSISTENCE_FAILURE"
                } else if !waited {
                    "SUPERVISOR_WAIT_FAILURE"
                } else {
                    "SUPERVISOR_CHILD_FAILURE"
                },
            ),
        )?);
    }
    Ok(ChildCompletion {
        child_pid: Some(child),
        wait_status: waited.then_some(status),
        terminal: terminal.ok_or("R2B terminal construction failed")?,
        succeeded: success && !lifecycle_state_failed,
    })
}

#[cfg(target_os = "linux")]
fn supervise(controlled: bool) -> Result<(), Box<dyn std::error::Error>> {
    if unsafe { libc::geteuid() } != 0 || unsafe { libc::getegid() } != 0 {
        return Err("R2B supervisor requires root".into());
    }
    let accepted = accepted_helper_sha256()?;
    let helper = open_accepted_helper(&accepted)?;
    let launcher_sha256 = current_executable_sha256()?;
    let (parent_channel, child_channel_raw) = socket_pair()?;
    let child_channel = duplicate_high(child_channel_raw.as_raw_fd())?;
    let channel_metadata = child_channel.metadata()?;
    let receipt_bytes = if controlled {
        r2a5::prepare_r2b_controlled_custody_admission(
            &accepted,
            &launcher_sha256,
            channel_metadata.dev(),
            channel_metadata.ino(),
        )?
    } else {
        r2a5::prepare_r2b_privileged_admission(
            &accepted,
            &launcher_sha256,
            channel_metadata.dev(),
            channel_metadata.ino(),
        )?
    };
    let receipt: R2bAdmissionReceiptV1 = serde_json::from_slice(&receipt_bytes)?;
    let started = chrono::Utc::now();
    let completion = match run_admitted_child(
        controlled,
        &helper,
        &parent_channel,
        child_channel_raw,
        child_channel,
        &receipt_bytes,
        &receipt,
        started,
    ) {
        Ok(completion) => completion,
        Err(_) => ChildCompletion {
            child_pid: None,
            wait_status: None,
            terminal: serde_json::to_value(r2a5::r2b_supervisor_fallback_terminal(
                &receipt,
                started,
                "SUPERVISOR_POST_ADMISSION_FAILURE",
            ))?,
            succeeded: false,
        },
    };
    let root_record = r2a5::r2b_root_terminal_record(
        &receipt,
        completion.child_pid,
        completion.wait_status,
        completion.terminal,
    );
    let terminal_persistence = if controlled && controlled_fault("FINALIZER_FSYNC_FAILURE") {
        Err(stage8b_readonly_preflight::r2a3::R2a3Error::EvidencePersistence)
    } else {
        r2a5::persist_r2b_root_terminal_json(
            &receipt.run_nonce_sha256,
            &serde_json::to_value(root_record)?,
        )
    };
    if terminal_persistence.is_err() {
        let _ = r2a5::record_r2b_supervisor_state(
            &receipt_bytes,
            R2bAdmissionState::TerminalPersistenceFailure,
        );
        return Err("R2B root terminal persistence failed".into());
    }
    if r2a5::record_r2b_supervisor_state(&receipt_bytes, R2bAdmissionState::TerminalEvidenceDurable)
        .is_err()
    {
        let _ = r2a5::record_r2b_supervisor_state(
            &receipt_bytes,
            R2bAdmissionState::TerminalPersistenceFailure,
        );
        return Err("R2B terminal state persistence failed after publication".into());
    }
    if completion.succeeded {
        Ok(())
    } else {
        Err("R2B helper completed with root-persisted failure evidence".into())
    }
}

#[cfg(not(target_os = "linux"))]
fn supervise(_controlled: bool) -> Result<(), Box<dyn std::error::Error>> {
    Err("R2B supervisor is Linux-only".into())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    supervise(controlled_custody_requested()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_helper_hash_is_valid() {
        assert!(valid_sha256(&accepted_helper_sha256().unwrap()));
    }
}
