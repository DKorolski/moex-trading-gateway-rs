//! The dedicated root signer owns only the schedule credential. Neither this
//! executable nor the paper child has a key-generation or installation command.
use super::*;
use crate::stage8b_p1f_o3_composition::authority::require_active;
use crate::stage8b_p1f_o3_composition::{AdmittedInputs, O3Error};
use runtime_durable_service::Stage8bP1fAuthorityStoreV1;
use std::{
    ffi::{CStr, CString},
    fs::{self, OpenOptions},
    os::unix::{
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        net::UnixListener,
    },
    path::Path,
};
use tokio::signal::unix::{signal, SignalKind};
use zeroize::Zeroizing;

const CREDENTIAL: &str = "/run/credentials/moex-finam-p1f-o3-signer.service/schedule.ed25519";
const SOCKET_ROOT: &str = "/run/moex-finam-p1-o3";

struct OwnedSocket {
    device: u64,
    inode: u64,
}
impl Drop for OwnedSocket {
    fn drop(&mut self) {
        if let Ok(m) = fs::symlink_metadata(O3_SCHEDULE_SIGNER_SOCKET) {
            if m.dev() == self.device && m.ino() == self.inode {
                let _ = fs::remove_file(O3_SCHEDULE_SIGNER_SOCKET);
            }
        }
    }
}

pub async fn run_fixed_signer(manifest_sha256: &str) -> Result<(), O3Error> {
    if unsafe { libc::geteuid() } != 0 || !cfg!(target_os = "linux") {
        return Err(O3Error::Input);
    }
    // Fail closed before touching the credential. Disable core dumps only in
    // this process; no host setting is changed.
    let limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0 {
        return Err(O3Error::Input);
    }
    #[cfg(target_os = "linux")]
    if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0) } != 0 {
        return Err(O3Error::Input);
    }
    let mut term = signal(SignalKind::terminate()).map_err(|_| O3Error::Io)?;
    let mut interrupt = signal(SignalKind::interrupt()).map_err(|_| O3Error::Io)?;
    let inputs = AdmittedInputs::load_fixed(manifest_sha256)?;
    let (uid, gid) = service_identity()?;
    let store = Stage8bP1fAuthorityStoreV1::open_production(gid).map_err(|_| O3Error::Input)?;
    require_active(&store, manifest_sha256, inputs.deadline).await?;
    let remaining = (inputs.deadline - Utc::now())
        .to_std()
        .map_err(|_| O3Error::Deadline)?;
    let monotonic_deadline = Instant::now() + remaining;
    let policy = O3ScheduleSigningPolicy {
        schema_version: 1,
        phase_manifest_sha256: inputs.manifest_sha256.clone(),
        operational_identity_sha256: inputs.plan.operational_identity_sha256.clone(),
        runtime_config_fingerprint_sha256: inputs.plan.runtime_config_fingerprint_sha256.clone(),
        instrument_map_fingerprint_sha256: inputs.plan.instrument_map_fingerprint_sha256.clone(),
        registry_identity_sha256: inputs.plan.schedule.registry_identity_sha256.clone(),
        registry_version: inputs.plan.schedule.registry_version.clone(),
        schedule_semantic_sha256: inputs.plan.schedule_semantic_sha256.clone(),
        not_before: inputs.not_before,
        deadline: inputs.deadline,
        publisher_uid: uid,
    };
    let root = Path::new(SOCKET_ROOT);
    let metadata = fs::symlink_metadata(root).map_err(|_| O3Error::Io)?;
    if !metadata.is_dir()
        || metadata.uid() != 0
        || metadata.gid() != gid
        || metadata.mode() & 0o777 != 0o750
        || root.canonicalize().map_err(|_| O3Error::Io)? != root
    {
        return Err(O3Error::Input);
    }
    // bind() refuses an existing path; no replacement of a stale/foreign socket.
    let listener = UnixListener::bind(O3_SCHEDULE_SIGNER_SOCKET).map_err(|_| O3Error::Io)?;
    let metadata = fs::symlink_metadata(O3_SCHEDULE_SIGNER_SOCKET).map_err(|_| O3Error::Io)?;
    let _owned = OwnedSocket {
        device: metadata.dev(),
        inode: metadata.ino(),
    };
    let socket = CString::new(O3_SCHEDULE_SIGNER_SOCKET).map_err(|_| O3Error::Input)?;
    if unsafe { libc::chown(socket.as_ptr(), 0, gid) } != 0 {
        return Err(O3Error::Io);
    }
    fs::set_permissions(O3_SCHEDULE_SIGNER_SOCKET, fs::Permissions::from_mode(0o660))
        .map_err(|_| O3Error::Io)?;
    listener.set_nonblocking(true).map_err(|_| O3Error::Io)?;
    let service =
        O3ScheduleSigningService::new(policy, read_credential()?).map_err(|_| O3Error::Input)?;
    loop {
        if Instant::now() >= monotonic_deadline || Utc::now() >= inputs.deadline {
            return Ok(());
        }
        // Read-only history validation; no phase claim, lease reset or fresh
        // authority is issued by a signer restart.
        require_active(&store, manifest_sha256, inputs.deadline).await?;
        match listener.accept() {
            Ok((mut stream, _)) => {
                // A malformed or wrong-UID request receives no signature. It
                // cannot terminate another valid connection or extend lifetime.
                let _ = service.serve_connection(&mut stream);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return Err(O3Error::Io),
        }
        tokio::select! {
            _ = term.recv() => return Ok(()),
            _ = interrupt.recv() => return Ok(()),
            _ = tokio::time::sleep(Duration::from_millis(20)) => {},
        }
    }
}

fn service_identity() -> Result<(u32, u32), O3Error> {
    let name = CString::new(runtime_durable_service::STAGE8B_P1F_SERVICE_USER)
        .map_err(|_| O3Error::Input)?;
    // This dedicated single-thread startup copies values before the next libc
    // lookup. No borrowed passwd storage escapes.
    let p = unsafe { libc::getpwnam(name.as_ptr()) };
    if p.is_null() {
        return Err(O3Error::Input);
    }
    let p = unsafe { &*p };
    if p.pw_uid == 0
        || p.pw_gid == 0
        || p.pw_name.is_null()
        || unsafe { CStr::from_ptr(p.pw_name) } != name.as_c_str()
    {
        return Err(O3Error::Input);
    }
    Ok((p.pw_uid, p.pw_gid))
}

fn read_credential() -> Result<SigningKey, O3Error> {
    read_credential_at(Path::new(CREDENTIAL), 0)
}

fn read_credential_at(path: &Path, owner: u32) -> Result<SigningKey, O3Error> {
    let parent = path.parent().ok_or(O3Error::Input)?;
    if parent.canonicalize().map_err(|_| O3Error::Io)? != parent {
        return Err(O3Error::Input);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| O3Error::Io)?;
    let m = file.metadata().map_err(|_| O3Error::Io)?;
    if !m.is_file() || m.uid() != owner || m.nlink() != 1 || m.mode() & 0o077 != 0 || m.len() > 65 {
        return Err(O3Error::Input);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    (&mut file)
        .take(66)
        .read_to_end(&mut bytes)
        .map_err(|_| O3Error::Io)?;
    let after = file.metadata().map_err(|_| O3Error::Io)?;
    if bytes.len() as u64 != m.len()
        || after.uid() != m.uid()
        || after.mode() != m.mode()
        || after.nlink() != 1
        || after.len() != m.len()
        || after.mtime() != m.mtime()
        || after.mtime_nsec() != m.mtime_nsec()
        || after.ctime() != m.ctime()
        || after.ctime_nsec() != m.ctime_nsec()
    {
        return Err(O3Error::Input);
    }
    let mut seed = Zeroizing::new([0; 32]);
    if bytes.len() == 32 {
        seed.copy_from_slice(&bytes);
    } else {
        let text = std::str::from_utf8(&bytes).map_err(|_| O3Error::Input)?;
        let text = text.strip_suffix('\n').unwrap_or(text);
        if text.len() != 64 {
            return Err(O3Error::Input);
        }
        for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
            let nibble = |b| match b {
                b'0'..=b'9' => Ok(b - b'0'),
                b'a'..=b'f' => Ok(b - b'a' + 10),
                _ => Err(O3Error::Input),
            };
            seed[index] = nibble(pair[0])? * 16 + nibble(pair[1])?;
        }
    }
    Ok(SigningKey::from_bytes(&seed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn o3_credential_custody_rejects_permissions_links_fifo_and_malformed_key() {
        let root = std::env::temp_dir().join(format!("o3-credential-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let root = root.canonicalize().unwrap();
        let owner = unsafe { libc::geteuid() };
        let path = root.join("credential");
        let write = |bytes: &[u8]| {
            let mut f = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&path)
                .unwrap();
            f.write_all(bytes).unwrap();
            f.sync_all().unwrap();
        };
        write(&[11u8; 32]);
        let expected = SigningKey::from_bytes(&[11u8; 32]).verifying_key();
        assert_eq!(
            read_credential_at(&path, owner).unwrap().verifying_key(),
            expected
        );
        assert!(read_credential_at(&path, owner + 1).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(read_credential_at(&path, owner).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let link = root.join("link");
        fs::hard_link(&path, &link).unwrap();
        assert!(read_credential_at(&path, owner).is_err());
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_credential_at(&link, owner).is_err());
        fs::remove_file(&link).unwrap();
        fs::remove_file(&path).unwrap();
        write(format!("{}\n", "0b".repeat(32)).as_bytes());
        assert_eq!(
            read_credential_at(&path, owner).unwrap().verifying_key(),
            expected
        );
        fs::remove_file(&path).unwrap();
        for bad in [
            vec![],
            vec![0; 31],
            vec![0; 33],
            vec![0; 66],
            "0B".repeat(32).into_bytes(),
            "gg".repeat(32).into_bytes(),
        ] {
            write(&bad);
            assert!(read_credential_at(&path, owner).is_err());
            fs::remove_file(&path).unwrap();
        }
        let fifo = CString::new(path.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        // O_NONBLOCK + regular-file validation: this cannot wait for a writer.
        assert!(read_credential_at(&path, owner).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
