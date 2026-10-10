//! Exact pre-append attempt beside the already protected observed receipts.
//! No discovery, overwrite, refresh of an expired attempt, or new journal.
use super::*;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
};
use strategy_runtime_core::stage8b_o3_readmission::*;

pub(in crate::stage8b_p1_semantic) struct O3RetainedContext<'a> {
    pub source: &'a Stage8bO3PaperSource,
    parent: &'a std::path::Path,
}

impl ObservedM10Context {
    pub(in crate::stage8b_p1_semantic) fn o3(&self) -> Option<O3RetainedContext<'_>> {
        let Self::Recovery(c) = self else {
            return None;
        };
        Some(O3RetainedContext {
            source: c.fresh_window.as_ref()?.o3()?,
            parent: &c.retained_parent,
        })
    }
}

impl O3RetainedContext<'_> {
    fn path(
        &self,
        seal: &str,
        b: &Stage5gP1SemanticBindingInput,
    ) -> Result<PathBuf, Stage8bP1CanonicalM10Error> {
        let bytes = serde_json::to_vec(&(
            self.source.scope(),
            seal,
            &b.operational_identity_sha256,
            &b.m10_redis_id,
            &b.m10_semantic_id_sha256,
            &b.m10_payload_sha256,
        ))
        .map_err(|_| Stage8bP1CanonicalM10Error::Decode)?;
        let key = sha256_hex(&bytes);
        Ok(retained::path(self.parent, &key)?
            .with_file_name(format!("stage8b-o3-decision-{key}.json")))
    }

    pub fn fresh(
        &self,
        b: &Stage5gP1SemanticBindingInput,
        exact: Stage8bO3DecisionBinding,
    ) -> Result<Stage8bO3PaperPermit, Stage8bP1CanonicalM10Error> {
        let fail = Stage8bP1CanonicalM10Error::DigestMismatch;
        let path = self.path(&exact.source_seal_commitment_sha256, b)?;
        let permit = match fs::symlink_metadata(&path) {
            Ok(_) => {
                Stage8bO3PaperPermit::restore_pending(&retained::read(&path)?, self.source.scope())
                    .map_err(|_| fail)?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let permit = self.source.issue(exact.clone()).map_err(|_| fail)?;
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                    .open(&path)
                    .map_err(|_| fail)?;
                file.write_all(permit.retained_bytes())
                    .and_then(|_| file.sync_all())
                    .map_err(|_| fail)?;
                File::open(self.parent)
                    .and_then(|f| f.sync_all())
                    .map_err(|_| fail)?;
                if retained::read(&path)? != permit.retained_bytes() {
                    return Err(fail);
                }
                permit
            }
            Err(_) => return Err(fail),
        };
        if permit.binding() != &exact {
            return Err(fail);
        }
        Ok(permit)
    }

    pub fn replay(
        &self,
        pending: &crate::recovery::P1SemanticPrepublicationPending,
        b: &Stage5gP1SemanticBindingInput,
    ) -> Result<Stage8bO3PaperPermit, Stage8bP1CanonicalM10Error> {
        let bytes = retained::read(&self.path(pending.o3_source_seal(), b)?)?;
        pending
            .replay_o3_permit(b, &bytes, self.source.scope())
            .map_err(|_| Stage8bP1CanonicalM10Error::DigestMismatch)
    }
}
