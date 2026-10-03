//! Content-addressed receipt files beside existing producer state. These are
//! retained evidence, NOT authority and NOT a registry. No discovery, overwrite
//! of conflicting bytes, pruning, or implicit acceptance of a Redis hash.
use super::*;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_RECEIPT_BYTES: u64 = 16 * 1024 * 1024;
static TEMP: AtomicU64 = AtomicU64::new(1);

fn path(parent: &Path, hash: &str) -> Result<PathBuf, Stage8bP1CanonicalM10Error> {
    if !is_sha256(hash) {
        return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
    }
    let meta = fs::symlink_metadata(parent).map_err(|_| Stage8bP1CanonicalM10Error::Decode)?;
    if !meta.is_dir() || meta.permissions().mode() & 0o022 != 0 {
        return Err(Stage8bP1CanonicalM10Error::Decode);
    }
    Ok(parent.join(format!("stage8b-observed-receipt-{hash}.json")))
}

fn read(path: &Path) -> Result<Vec<u8>, Stage8bP1CanonicalM10Error> {
    let invalid = Stage8bP1CanonicalM10Error::Decode;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| invalid)?;
    let before = file.metadata().map_err(|_| invalid)?;
    if !before.is_file()
        || before.nlink() != 1
        || before.len() > MAX_RECEIPT_BYTES
        || before.permissions().mode() & 0o022 != 0
    {
        return Err(invalid);
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_RECEIPT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid)?;
    let after = file.metadata().map_err(|_| invalid)?;
    if bytes.len() as u64 != before.len()
        || (
            before.len(),
            before.mtime(),
            before.mtime_nsec(),
            before.ctime(),
            before.ctime_nsec(),
        ) != (
            after.len(),
            after.mtime(),
            after.mtime_nsec(),
            after.ctime(),
            after.ctime_nsec(),
        )
    {
        return Err(invalid);
    }
    Ok(bytes)
}

impl Stage8bP1ObservedM10Binding {
    /// Retain once per admitted range BEFORE Prepared or Redis publication.
    /// Uses the existing single-writer protected-parent contract. Interrupted
    /// temp files are not lookup candidates; retry writes the same final bytes.
    /// There is deliberately no GC: a receipt cannot be deleted while pending.
    pub fn persist_retained_receipt(
        &self,
        parent: &Path,
    ) -> Result<(), Stage8bP1CanonicalM10Error> {
        let target = path(parent, self.source().sha256())?;
        let invalid = Stage8bP1CanonicalM10Error::Decode;
        match fs::symlink_metadata(&target) {
            Ok(_) => {
                if read(&target)? != self.source().retained_bytes() {
                    return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let temp = parent.join(format!(
                    ".stage8b-observed-receipt-{}-{}.tmp",
                    std::process::id(),
                    TEMP.fetch_add(1, Ordering::Relaxed)
                ));
                let mut file = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                    .open(&temp)
                    .map_err(|_| invalid)?;
                file.write_all(self.source().retained_bytes())
                    .map_err(|_| invalid)?;
                file.sync_all().map_err(|_| invalid)?;
                fs::rename(&temp, &target).map_err(|_| invalid)?;
            }
            Err(_) => return Err(invalid),
        }
        // Also sync idempotent retries after possible rename response loss.
        File::open(&target)
            .and_then(|f| f.sync_all())
            .map_err(|_| invalid)?;
        File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|_| invalid)?;
        if read(&target)? != self.source().retained_bytes() {
            return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
        }
        Ok(())
    }

    /// The expected hash must come from an admitted source/producer record or
    /// authenticated durable binding; filename presence alone grants nothing.
    pub fn restore_retained_receipt(
        parent: &Path,
        identity: &str,
        expected_hash: &str,
    ) -> Result<Self, Stage8bP1CanonicalM10Error> {
        let bytes = read(&path(parent, expected_hash)?)?;
        let receipt = ObservedM1Receipt::restore(&bytes, expected_hash)
            .map_err(|_| Stage8bP1CanonicalM10Error::DigestMismatch)?;
        Self::new(identity, receipt, expected_hash)
    }

    /// Resolve the receipt referenced by one EXACT sealed M10. Expected IDs
    /// and payload hash must come from the durable pending owner, not Redis.
    /// Rebuilding with another snapshot is forbidden even for equal OHLCV.
    pub fn restore_for_exact_m10(
        parent: &Path,
        bytes: &[u8],
        identity: &str,
        redis_id: &str,
        semantic_sha256: &str,
        payload_sha256: &str,
    ) -> Result<Self, Stage8bP1CanonicalM10Error> {
        if bytes.len() > 16 * 1024 {
            return Err(Stage8bP1CanonicalM10Error::Decode);
        }
        let wire: EnvelopeV2 =
            serde_json::from_slice(bytes).map_err(|_| Stage8bP1CanonicalM10Error::Decode)?;
        if wire.redis_id != redis_id
            || wire.m10_semantic_id_sha256 != semantic_sha256
            || wire.m10_payload_sha256 != payload_sha256
        {
            return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
        }
        let binding =
            Self::restore_retained_receipt(parent, identity, &wire.payload.source_receipt_sha256)?;
        // This recomputes the hashes and binds every byte to the retained full
        // receipt; copying sealed outer hash strings cannot bypass validation.
        binding.parse_exact(bytes, identity)?;
        Ok(binding)
    }
}
