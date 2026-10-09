//! Retain the predecessor BEFORE advancing the producer high-water. A single
//! high-water alone cannot reconstruct the accepted two-entry Published window.
//! These files are evidence, not authority: both bindings and exact expected
//! canonical bytes must be supplied independently by the fixed composition.

use super::*;
use std::{
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
};

const MAX_STATE_BYTES: u64 = 128 * 1024;

fn protected_parent(path: &Path) -> Result<&Path, Stage8bP1fProducerErrorV1> {
    let parent = path
        .parent()
        .ok_or(Stage8bP1fProducerErrorV1::DurableStateConflict)?;
    let meta = fs::symlink_metadata(parent)?;
    if !meta.is_dir()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.permissions().mode() & 0o077 != 0
        || parent.canonicalize()? != parent
    {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    Ok(parent)
}

fn read_state(path: &Path) -> Result<Vec<u8>, Stage8bP1fProducerErrorV1> {
    protected_parent(path)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)?;
    let before = file.metadata()?;
    if !before.is_file()
        || before.nlink() != 1
        || before.uid() != unsafe { libc::geteuid() }
        || before.permissions().mode() & 0o077 != 0
        || before.len() > MAX_STATE_BYTES
    {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_STATE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    let stamp = |m: &fs::Metadata| {
        (
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    };
    if stamp(&before) != stamp(&after) || bytes.len() as u64 != before.len() {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    Ok(bytes)
}

/// Explicit single-writer checkpoint, not a second mutable high-water. Only a
/// checked Published predecessor can be retained. Exact retry is idempotent;
/// different existing bytes and symlinks fail closed. The
/// caller must finish this checkpoint before preparing the successor.
pub fn retain_published_predecessor(
    path: &Path,
    predecessor: &Stage8bP1fObservedM10State,
) -> Result<(), Stage8bP1fProducerErrorV1> {
    predecessor.validate()?;
    if predecessor.phase() != Stage8bP1fM10ProducerPhaseV1::Published {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    let parent = protected_parent(path)?;
    let bytes = predecessor.retained_bytes()?;
    predecessor.binding.persist_retained_receipt(parent)?;
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        // Reuse the accepted fsync/temp/rename protocol under the same
        // single-writer lease. An interrupted temporary file is never read.
        Err(e) if e.kind() == ErrorKind::NotFound => persist_m10_bytes(path, &bytes)?,
        Err(e) => return Err(e.into()),
    }
    if read_state(path)? != bytes {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    File::open(path)?.sync_all()?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

/// Restore explicit checkpoints with independently admitted source bindings
/// and canonical bytes; no filesystem scan, Redis-derived hash or fallback.
/// Both states must be Published and consecutive. The returned Rust value
/// still requires the runtime's exact Redis reread before callback admission.
pub fn restore_published_window(
    predecessor_path: &Path,
    successor_high_water_path: &Path,
    predecessor_binding: Stage8bP1ObservedM10Binding,
    successor_binding: Stage8bP1ObservedM10Binding,
    expected_predecessor_bytes: &[u8],
    expected_successor_bytes: &[u8],
) -> Result<runtime_durable_service::Stage8bP1ObservedPublishedWindow, Stage8bP1fProducerErrorV1> {
    if predecessor_path == successor_high_water_path {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    let predecessor = Stage8bP1fObservedM10State::from_retained_bytes(
        &read_state(predecessor_path)?,
        predecessor_binding,
    )?;
    let successor = Stage8bP1fObservedM10State::from_retained_bytes(
        &read_state(successor_high_water_path)?,
        successor_binding,
    )?;
    if predecessor.canonical_bytes()? != expected_predecessor_bytes
        || successor.canonical_bytes()? != expected_successor_bytes
    {
        return Err(Stage8bP1fProducerErrorV1::DurableStateConflict);
    }
    observed_published_window(&predecessor, &successor)
}
