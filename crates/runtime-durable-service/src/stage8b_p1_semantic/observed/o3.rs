//! Exact pre-append attempt beside the already protected observed receipts.
//! No discovery, overwrite, refresh of an expired attempt, or new journal.
use super::*;
use std::{
    fs::{self, OpenOptions},
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
                file.write_all(permit.retained_bytes()).map_err(|_| fail)?;
                #[cfg(test)]
                sync_test::step(sync_test::Point::AfterWrite)?;
                permit
            }
            Err(_) => return Err(fail),
        };
        if permit.binding() != &exact {
            return Err(fail);
        }
        // An earlier process may have stopped after write_all but before either
        // sync. Existing bytes alone do not establish pre-RequestAccepted durability.
        // Validate and sync the SAME regular no-follow descriptor for both paths.
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(&path)
            .map_err(|_| fail)?;
        if retained::read_file(&mut file)? != permit.retained_bytes() {
            return Err(fail);
        }
        #[cfg(test)]
        sync_test::step(sync_test::Point::BeforeFileSync)?;
        file.sync_all().map_err(|_| fail)?;
        #[cfg(test)]
        sync_test::step(sync_test::Point::FileSynced)?;
        let parent = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_DIRECTORY)
            .open(self.parent)
            .map_err(|_| fail)?;
        #[cfg(test)]
        sync_test::step(sync_test::Point::BeforeParentSync)?;
        parent.sync_all().map_err(|_| fail)?;
        #[cfg(test)]
        sync_test::step(sync_test::Point::ParentSynced)?;
        #[cfg(test)]
        sync_test::step(sync_test::Point::BeforeReread)?;
        if retained::read(&path)? != permit.retained_bytes() {
            return Err(fail);
        }
        #[cfg(test)]
        sync_test::step(sync_test::Point::ExactReread)?;
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

// Thread-local instrumentation is absent from production/fixture builds. The
// default current-thread Tokio witnesses can stop at a completed write, or fail
// a sync/reread boundary, without process-wide fault races or real power cuts.
#[cfg(test)]
pub(in crate::stage8b_p1_semantic) mod sync_test {
    use super::Stage8bP1CanonicalM10Error;
    use std::cell::RefCell;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Point {
        AfterWrite,
        BeforeFileSync,
        FileSynced,
        BeforeParentSync,
        ParentSynced,
        BeforeReread,
        ExactReread,
        CommitEntered,
        CommitReturned,
    }

    #[derive(Default)]
    struct State {
        active: bool,
        fault: Option<Point>,
        trace: Vec<Point>,
    }
    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State::default());
    }
    pub struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            STATE.with(|s| *s.borrow_mut() = State::default());
        }
    }
    pub fn arm(fault: Option<Point>) -> Guard {
        STATE.with(|s| {
            assert!(!s.borrow().active, "nested proof sync audit");
            *s.borrow_mut() = State {
                active: true,
                fault,
                trace: Vec::new(),
            };
        });
        Guard
    }
    pub fn record(point: Point) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.active {
                s.trace.push(point);
            }
        });
    }
    pub fn step(point: Point) -> Result<(), Stage8bP1CanonicalM10Error> {
        record(point);
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.active && s.fault == Some(point) {
                s.fault = None;
                Err(Stage8bP1CanonicalM10Error::DigestMismatch)
            } else {
                Ok(())
            }
        })
    }
    pub fn trace() -> Vec<Point> {
        STATE.with(|s| s.borrow().trace.clone())
    }
}
