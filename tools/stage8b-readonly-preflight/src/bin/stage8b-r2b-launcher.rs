//! Root-owned R2B launcher with durable nonce admission and sealed-FD handoff.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(target_os = "linux")]
use std::ffi::CString;
#[cfg(target_os = "linux")]
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(target_os = "linux")]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

#[cfg(target_os = "linux")]
const HELPER: &str = "/opt/moex-trading/stage8b-r2b/bin/stage8b-readonly-preflight";
const ACCEPTED_SHA256: &str =
    include_str!("../../../../docs/stage-8/stage8b-p-r2b-accepted-helper-sha256.txt");
const HELPER_ACCEPTANCE_AUTHORITY: &str =
    include_str!("../../../../docs/stage-8/stage8b-p-r2b-helper-acceptance-authority.json");
const R2B_ACCEPTANCE_PUBLIC_KEY_HEX: &str =
    "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct R2bHelperAcceptanceAuthority {
    schema_version: u8,
    stage: String,
    revision: String,
    status: String,
    helper_executable_sha256: String,
    acceptance_key_id: String,
    authority_commitment_sha256: String,
    signature_ed25519_hex: String,
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], Box<dyn std::error::Error>> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("invalid lowercase hex".into());
    }
    let mut output = [0_u8; N];
    for (index, slot) in output.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(output)
}

fn validate_helper_acceptance_authority() -> Result<String, Box<dyn std::error::Error>> {
    let authority: R2bHelperAcceptanceAuthority =
        serde_json::from_str(HELPER_ACCEPTANCE_AUTHORITY)?;
    if authority.schema_version != 1
        || authority.stage != "Stage 8B-P R2B"
        || authority.revision != "R2"
        || authority.status != "ACCEPTED_HELPER_ONLY_R2B_NOT_ISSUED"
        || authority.acceptance_key_id != "stage8b-r2b-helper-acceptance-v1"
        || authority.helper_executable_sha256 != ACCEPTED_SHA256.trim()
    {
        return Err("R2B helper acceptance authority mismatch".into());
    }
    let mut unsigned = authority.clone();
    unsigned.authority_commitment_sha256.clear();
    unsigned.signature_ed25519_hex.clear();
    let mut hasher = Sha256::new();
    hasher.update(b"stage8b-p-r2b-helper-acceptance-authority-v1\0");
    hasher.update(serde_json::to_vec(&unsigned)?);
    let commitment = format!("{:x}", hasher.finalize());
    if authority.authority_commitment_sha256 != commitment {
        return Err("R2B helper acceptance commitment mismatch".into());
    }
    let public = VerifyingKey::from_bytes(&decode_hex::<32>(R2B_ACCEPTANCE_PUBLIC_KEY_HEX)?)?;
    let signature = Signature::from_bytes(&decode_hex::<64>(&authority.signature_ed25519_hex)?);
    public.verify(commitment.as_bytes(), &signature)?;
    Ok(authority.helper_executable_sha256)
}

#[cfg(target_os = "linux")]
fn open_accepted_helper(
    expected_sha256: &str,
) -> Result<std::fs::File, Box<dyn std::error::Error>> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(HELPER)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.gid() != 0
        || metadata.nlink() != 1
        || metadata.mode() & 0o022 != 0
        || metadata.mode() & 0o111 == 0
    {
        return Err("R2B helper custody mismatch".into());
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let observed = format!("{:x}", Sha256::digest(&bytes));
    if observed != expected_sha256 {
        return Err("R2B helper hash mismatch".into());
    }
    file.seek(SeekFrom::Start(0))?;
    // FD 3 is reserved for the sealed admission receipt.  Keep the already
    // verified executable on a distinct descriptor so receipt installation
    // cannot replace the exact inode that will be passed to fexecve().
    if file.as_raw_fd() == stage8b_readonly_preflight::r2a5::R2B_ADMISSION_RECEIPT_FD {
        let duplicate = unsafe {
            libc::fcntl(
                file.as_raw_fd(),
                libc::F_DUPFD_CLOEXEC,
                stage8b_readonly_preflight::r2a5::R2B_ADMISSION_RECEIPT_FD + 1,
            )
        };
        if duplicate == -1 {
            return Err(std::io::Error::last_os_error().into());
        }
        file = unsafe { std::fs::File::from_raw_fd(duplicate) };
    }
    Ok(file)
}

#[cfg(not(target_os = "linux"))]
fn open_accepted_helper(
    _expected_sha256: &str,
) -> Result<std::fs::File, Box<dyn std::error::Error>> {
    Err("R2B launcher is Linux-only".into())
}

fn sealed_receipt_fd(_receipt: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(target_os = "linux"))]
    return Err("R2B launcher is Linux-only".into());
    #[cfg(target_os = "linux")]
    {
        let name = CString::new("stage8b-r2b-admission-v1")?;
        let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_ALLOW_SEALING) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        file.write_all(_receipt)?;
        file.sync_all()?;
        file.seek(SeekFrom::Start(0))?;
        let seals =
            libc::F_SEAL_SEAL | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_WRITE;
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_ADD_SEALS, seals) } == -1 {
            return Err(std::io::Error::last_os_error().into());
        }
        let source_fd = file.as_raw_fd();
        if source_fd != stage8b_readonly_preflight::r2a5::R2B_ADMISSION_RECEIPT_FD
            && unsafe {
                libc::dup2(
                    source_fd,
                    stage8b_readonly_preflight::r2a5::R2B_ADMISSION_RECEIPT_FD,
                )
            } == -1
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let receipt_fd = stage8b_readonly_preflight::r2a5::R2B_ADMISSION_RECEIPT_FD;
        let flags = unsafe { libc::fcntl(receipt_fd, libc::F_GETFD) };
        if flags == -1
            || unsafe { libc::fcntl(receipt_fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) } == -1
        {
            return Err(std::io::Error::last_os_error().into());
        }
        if source_fd == receipt_fd {
            std::mem::forget(file);
        } else {
            drop(file);
        }
        Ok(())
    }
}

fn drop_to_helper_identity() -> Result<(), Box<dyn std::error::Error>> {
    if unsafe { libc::geteuid() } != 0 || unsafe { libc::getegid() } != 0 {
        return Err("R2B launcher requires root admission identity".into());
    }
    if unsafe { libc::setgroups(0, std::ptr::null()) } != 0
        || unsafe { libc::setgid(stage8b_readonly_preflight::r2a5::R2B_EVIDENCE_GID) } != 0
        || unsafe { libc::setuid(stage8b_readonly_preflight::r2a5::R2B_HELPER_UID) } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    if unsafe { libc::geteuid() } != stage8b_readonly_preflight::r2a5::R2B_HELPER_UID
        || unsafe { libc::getegid() } != stage8b_readonly_preflight::r2a5::R2B_EVIDENCE_GID
    {
        return Err("R2B privilege drop did not stick".into());
    }
    Ok(())
}

fn fd_bound_exec(
    helper: &std::fs::File,
    controlled_custody: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = helper;
    let _ = controlled_custody;
    #[cfg(not(target_os = "linux"))]
    return Err("R2B launcher is Linux-only".into());
    #[cfg(target_os = "linux")]
    {
        let flags = unsafe { libc::fcntl(helper.as_raw_fd(), libc::F_GETFD) };
        if flags == -1
            || unsafe { libc::fcntl(helper.as_raw_fd(), libc::F_SETFD, flags & !libc::FD_CLOEXEC) }
                == -1
        {
            return Err(std::io::Error::last_os_error().into());
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
            libc::fexecve(helper.as_raw_fd(), argv.as_ptr(), envp.as_ptr());
        }
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(not(feature = "stage8b-r2b-controlled-custody"))]
fn controlled_custody_requested() -> Result<bool, Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("R2B launcher accepts no arguments".into());
    }
    Ok(false)
}

#[cfg(feature = "stage8b-r2b-controlled-custody")]
fn controlled_custody_requested() -> Result<bool, Box<dyn std::error::Error>> {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    match arguments.as_slice() {
        [_program, mode] if mode == "--controlled-custody" => Ok(true),
        _ => Err("qualification launcher requires --controlled-custody".into()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let controlled_custody = controlled_custody_requested()?;
    let accepted = validate_helper_acceptance_authority()?;
    let helper = open_accepted_helper(&accepted)?;
    let receipt = if controlled_custody {
        stage8b_readonly_preflight::r2a5::prepare_r2b_controlled_custody_admission(&accepted)?
    } else {
        stage8b_readonly_preflight::r2a5::prepare_r2b_privileged_admission(&accepted)?
    };
    sealed_receipt_fd(&receipt)?;
    stage8b_readonly_preflight::r2a5::record_r2b_helper_started(&receipt)?;
    drop_to_helper_identity()?;
    fd_bound_exec(&helper, controlled_custody)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_helper_acceptance_signature_and_hash_are_valid() {
        assert_eq!(
            validate_helper_acceptance_authority().unwrap(),
            ACCEPTED_SHA256.trim()
        );
    }
}
