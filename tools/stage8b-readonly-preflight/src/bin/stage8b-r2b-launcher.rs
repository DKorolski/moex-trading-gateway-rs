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
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use stage8b_readonly_preflight::r2a5::{
    self, R2bAdmissionReceiptV1, R2bAdmissionState, R2bSupervisorMessageV1, R2bTerminalEvidenceV1,
};

#[cfg(target_os = "linux")]
const HELPER: &str = "/opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight";
#[cfg(any(target_os = "linux", test))]
const ACCEPTED_SHA256: &str =
    include_str!("../../../../docs/stage-8/stage8b-p-r2b-accepted-helper-sha256.txt");
#[cfg(target_os = "linux")]
const CHILD_TIMEOUT_MS: i32 = 120_000;
#[cfg(all(target_os = "linux", feature = "stage8b-r2b-controlled-custody"))]
const CONTROLLED_FAULT_TIMEOUT_MS: i32 = 500;
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
fn verify_runtime_isolation_before_admission() -> Result<(), Box<dyn std::error::Error>> {
    let yama = std::fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope")?;
    let scope: u32 = yama.trim().parse()?;
    if scope < 1 {
        return Err("R2B requires kernel.yama.ptrace_scope >= 1".into());
    }
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let status = match std::fs::read_to_string(entry.path().join("status")) {
            Ok(status) => status,
            Err(_) => continue,
        };
        let uid = status.lines().find_map(|line| {
            line.strip_prefix("Uid:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u32>().ok())
        });
        if uid == Some(r2a5::R2B_HELPER_UID) {
            return Err("R2B dedicated helper UID already has a process".into());
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn close_non_allowlisted_descriptors() -> Result<(), Box<dyn std::error::Error>> {
    // Use the syscall directly: musl targets expose SYS_close_range but not
    // the glibc-only libc::close_range wrapper.  This keeps the accepted
    // launcher buildable as the same static Linux/amd64 artifact exercised by
    // the Phase-6 compatibility proof.
    if unsafe { libc::syscall(libc::SYS_close_range, 8u32, u32::MAX, 0u32) } == 0 {
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
    receipt_model: &R2bAdmissionReceiptV1,
    controlled_custody: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(feature = "stage8b-r2b-controlled-custody"))]
    let _ = receipt_model;
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
    if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    #[cfg(feature = "stage8b-r2b-controlled-custody")]
    if controlled_custody {
        run_controlled_protocol_fault(receipt_model)?;
    }
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
fn send_controlled_bytes(bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut offset = 0;
    while offset < bytes.len() {
        let written = unsafe {
            libc::send(
                r2a5::R2B_TERMINAL_CHANNEL_FD,
                bytes[offset..].as_ptr().cast(),
                bytes.len() - offset,
                libc::MSG_NOSIGNAL,
            )
        };
        if written <= 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        offset += written as usize;
    }
    Ok(())
}

#[cfg(all(target_os = "linux", feature = "stage8b-r2b-controlled-custody"))]
fn send_controlled_message(
    message: &R2bSupervisorMessageV1,
) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::to_vec(message)?;
    send_controlled_bytes(&(payload.len() as u32).to_be_bytes())?;
    send_controlled_bytes(&payload)
}

#[cfg(all(target_os = "linux", feature = "stage8b-r2b-controlled-custody"))]
fn controlled_hang() -> ! {
    loop {
        unsafe { libc::pause() };
    }
}

#[cfg(all(target_os = "linux", feature = "stage8b-r2b-controlled-custody"))]
fn run_controlled_protocol_fault(
    receipt: &R2bAdmissionReceiptV1,
) -> Result<(), Box<dyn std::error::Error>> {
    if controlled_fault("NO_START_FRAME") || controlled_fault("CHILD_IGNORES_CHANNEL") {
        controlled_hang();
    }
    if controlled_fault("PARTIAL_FRAME_HEADER") || controlled_fault("SLOW_DRIP_FRAME") {
        send_controlled_bytes(&[0])?;
        controlled_hang();
    }
    let started = R2bSupervisorMessageV1::HelperProcessStarted {
        schema_version: 1,
        admission_commitment_sha256: receipt.admission_commitment_sha256.clone(),
    };
    if controlled_fault("PARTIAL_FRAME_BODY") {
        let payload = serde_json::to_vec(&started)?;
        send_controlled_bytes(&(payload.len() as u32).to_be_bytes())?;
        send_controlled_bytes(&payload[..payload.len() / 2])?;
        controlled_hang();
    }
    if controlled_fault("NO_TERMINAL_FRAME") || controlled_fault("TERMINAL_THEN_HANG") {
        send_controlled_message(&started)?;
        if controlled_fault("TERMINAL_THEN_HANG") {
            let terminal = R2bSupervisorMessageV1::Terminal {
                schema_version: 1,
                admission_commitment_sha256: receipt.admission_commitment_sha256.clone(),
                evidence: Box::new(r2a5::r2b_supervisor_fallback_terminal(
                    receipt,
                    chrono::Utc::now(),
                    "CONTROLLED_TERMINAL_THEN_HANG",
                )),
            };
            send_controlled_message(&terminal)?;
        }
        controlled_hang();
    }
    Ok(())
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
fn read_exact_before(
    fd: RawFd,
    output: &mut [u8],
    deadline: Instant,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut offset = 0;
    while offset < output.len() {
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLIN | libc::POLLHUP,
            revents: 0,
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("R2B helper terminal channel timeout".into());
        }
        let timeout = i32::try_from(
            remaining
                .as_nanos()
                .div_ceil(1_000_000)
                .min(i32::MAX as u128),
        )?;
        let polled = unsafe { libc::poll(&mut poll, 1, timeout) };
        if polled == 0 {
            continue;
        }
        if polled < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error.into());
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
fn read_frame_before<T: DeserializeOwned>(
    fd: RawFd,
    deadline: Instant,
) -> Result<T, Box<dyn std::error::Error>> {
    let mut size = [0_u8; 4];
    read_exact_before(fd, &mut size, deadline)?;
    let size = u32::from_be_bytes(size) as usize;
    if size == 0 || size > MAX_FRAME_BYTES {
        return Err("R2B helper terminal frame size invalid".into());
    }
    let mut payload = vec![0; size];
    read_exact_before(fd, &mut payload, deadline)?;
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
    terminal: Option<R2bTerminalEvidenceV1>,
    protocol_valid: bool,
    root_error_category: Option<r2a5::R2bTerminalErrorCategory>,
    succeeded: bool,
}

#[cfg(target_os = "linux")]
fn validated_terminal(
    frame: R2bSupervisorMessageV1,
    receipt: &R2bAdmissionReceiptV1,
) -> Option<R2bTerminalEvidenceV1> {
    let R2bSupervisorMessageV1::Terminal {
        schema_version,
        admission_commitment_sha256,
        evidence,
    } = frame
    else {
        return None;
    };
    (schema_version == 1
        && admission_commitment_sha256 == receipt.admission_commitment_sha256
        && r2a5::validate_r2b_helper_terminal(receipt, &evidence))
    .then_some(*evidence)
}

#[cfg(target_os = "linux")]
enum ChildWaitOutcome {
    Reaped,
    TimedOutAndReaped,
    TimedOutUnreaped,
}

#[cfg(target_os = "linux")]
fn wait_child_before(
    child: libc::pid_t,
    deadline: Instant,
    status: &mut i32,
) -> Result<ChildWaitOutcome, Box<dyn std::error::Error>> {
    let pidfd = unsafe { libc::syscall(libc::SYS_pidfd_open, child, 0) } as RawFd;
    if pidfd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let pidfd = unsafe { std::fs::File::from_raw_fd(pidfd) };
    loop {
        let waited = unsafe { libc::waitpid(child, status, libc::WNOHANG) };
        if waited == child {
            return Ok(ChildWaitOutcome::Reaped);
        }
        if waited < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            unsafe { libc::kill(child, libc::SIGKILL) };
            let reap_deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < reap_deadline {
                if unsafe { libc::waitpid(child, status, libc::WNOHANG) } == child {
                    return Ok(ChildWaitOutcome::TimedOutAndReaped);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            return Ok(ChildWaitOutcome::TimedOutUnreaped);
        }
        let mut poll = libc::pollfd {
            fd: pidfd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let timeout = i32::try_from(remaining.as_millis().min(100))?;
        unsafe { libc::poll(&mut poll, 1, timeout.max(1)) };
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
            receipt,
            controlled,
        ) {
            eprintln!("stage8b-r2b-child-exec: {error}");
        }
        unsafe { libc::_exit(126) };
    }
    drop(child_channel_raw);
    drop(child_channel);
    #[cfg(feature = "stage8b-r2b-controlled-custody")]
    let timeout_ms = if controlled
        && [
            "NO_START_FRAME",
            "NO_TERMINAL_FRAME",
            "TERMINAL_THEN_HANG",
            "SLOW_DRIP_FRAME",
            "PARTIAL_FRAME_HEADER",
            "PARTIAL_FRAME_BODY",
            "CHILD_IGNORES_CHANNEL",
        ]
        .iter()
        .any(|fault| controlled_fault(fault))
    {
        CONTROLLED_FAULT_TIMEOUT_MS
    } else {
        CHILD_TIMEOUT_MS
    };
    #[cfg(not(feature = "stage8b-r2b-controlled-custody"))]
    let timeout_ms = CHILD_TIMEOUT_MS;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
    let mut terminal: Option<R2bTerminalEvidenceV1> = None;
    let mut protocol_valid = false;
    let mut lifecycle_state_failed = false;
    let mut channel_timed_out = false;
    let first = read_frame_before::<R2bSupervisorMessageV1>(parent_channel.as_raw_fd(), deadline);
    channel_timed_out |= first.is_err() && Instant::now() >= deadline;
    if let Ok(R2bSupervisorMessageV1::HelperProcessStarted {
        schema_version,
        admission_commitment_sha256,
    }) = first
    {
        if schema_version == 1 && admission_commitment_sha256 == receipt.admission_commitment_sha256
        {
            lifecycle_state_failed |= r2a5::record_r2b_supervisor_state(
                receipt_bytes,
                R2bAdmissionState::HelperProcessStarted,
            )
            .is_err();
            if controlled && controlled_fault("HELPER_CRASH_AFTER_STARTED") {
                unsafe { libc::kill(child, libc::SIGKILL) };
            } else {
                let next = read_frame_before::<R2bSupervisorMessageV1>(
                    parent_channel.as_raw_fd(),
                    deadline,
                );
                channel_timed_out |= next.is_err() && Instant::now() >= deadline;
                if let Ok(frame) = next {
                    if let Some(evidence) = validated_terminal(frame, receipt) {
                        lifecycle_state_failed |= r2a5::record_r2b_supervisor_state(
                            receipt_bytes,
                            R2bAdmissionState::HelperTerminalReceived,
                        )
                        .is_err();
                        terminal = Some(evidence);
                        protocol_valid = true;
                    }
                }
            }
        }
    }
    let mut status = 0;
    if terminal.is_none() {
        unsafe { libc::kill(child, libc::SIGKILL) };
    }
    let wait_outcome = wait_child_before(child, deadline, &mut status);
    let (waited, wait_timed_out) = match wait_outcome {
        Ok(ChildWaitOutcome::Reaped) => (true, false),
        Ok(ChildWaitOutcome::TimedOutAndReaped) => (true, true),
        Ok(ChildWaitOutcome::TimedOutUnreaped) | Err(_) => (false, true),
    };
    let timed_out = channel_timed_out || wait_timed_out;
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
    let exit_consistent = terminal.as_ref().is_some_and(|evidence| {
        matches!(
            (evidence.terminal_outcome, success),
            (r2a5::R2bTerminalOutcome::Success, true) | (r2a5::R2bTerminalOutcome::Failure, false)
        )
    });
    if lifecycle_state_failed || terminal.is_none() || !waited || timed_out || !exit_consistent {
        protocol_valid = false;
    }
    let root_error_category = if timed_out {
        Some(r2a5::R2bTerminalErrorCategory::Timeout)
    } else if lifecycle_state_failed || !waited {
        Some(r2a5::R2bTerminalErrorCategory::InternalInvariantFailure)
    } else if !protocol_valid || !exit_consistent {
        Some(r2a5::R2bTerminalErrorCategory::ContractDrift)
    } else {
        None
    };
    Ok(ChildCompletion {
        child_pid: Some(child),
        wait_status: waited.then_some(status),
        terminal,
        protocol_valid,
        root_error_category,
        succeeded: success
            && !lifecycle_state_failed
            && protocol_valid
            && !timed_out
            && exit_consistent,
    })
}

#[cfg(target_os = "linux")]
fn supervise(controlled: bool) -> Result<(), Box<dyn std::error::Error>> {
    if unsafe { libc::geteuid() } != 0 || unsafe { libc::getegid() } != 0 {
        return Err("R2B supervisor requires root".into());
    }
    let accepted = accepted_helper_sha256()?;
    let helper = open_accepted_helper(&accepted)?;
    verify_runtime_isolation_before_admission()?;
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
    ) {
        Ok(completion) => completion,
        Err(_) => ChildCompletion {
            child_pid: None,
            wait_status: None,
            terminal: Some(r2a5::r2b_supervisor_fallback_terminal(
                &receipt,
                started,
                "SUPERVISOR_POST_ADMISSION_FAILURE",
            )),
            protocol_valid: false,
            root_error_category: Some(r2a5::R2bTerminalErrorCategory::InternalInvariantFailure),
            succeeded: false,
        },
    };
    let root_record = r2a5::r2b_root_terminal_record(
        &receipt,
        completion.child_pid,
        completion.wait_status,
        completion.terminal,
        completion.protocol_valid,
        completion.root_error_category,
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
