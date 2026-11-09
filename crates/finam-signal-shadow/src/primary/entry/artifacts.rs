//! Create-new output only. No glob discovery, old output rewrite or secret.
//! The manifest is written last; a partial export is not a completed package.
use super::*;
use crate::primary::verify::{self, EvidenceBinding};
use serde_json::json;
use std::{
    cell::Cell,
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

pub(crate) struct Artifacts {
    directory: PathBuf,
    state: Cell<State>,
    owns_manifest: Cell<bool>,
    last_failed_io: Cell<Option<(&'static str, &'static str)>>,
    #[cfg(test)]
    faults: tests::Faults,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    New,
    Admitted,
    Finalizing,
    Finished,
    Failed,
}
#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub export_complete: bool,
    pub semantic_eod_passed: bool,
    pub all_workers_drained: bool,
    pub full_session_qualified: bool,
    pub network_qualified: bool,
    pub cgroup_qualified: bool,
    pub parity_accepted: bool,
}
impl Artifacts {
    pub fn create(path: &Path) -> Result<Self, EntryError> {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path).map_err(|_| EntryError::Output)?;
        Ok(Self {
            directory: path.to_owned(),
            state: Cell::new(State::New),
            owns_manifest: Cell::new(false),
            last_failed_io: Cell::new(None),
            #[cfg(test)]
            faults: tests::Faults::default(),
        })
    }
    pub fn journal(&self) -> PathBuf {
        self.directory.join("journal.jsonl")
    }
    pub fn record_admission(&self, receipt: &AdmissionReceipt) -> Result<(), EntryError> {
        if self.state.get() != State::New {
            return Err(EntryError::Export);
        }
        match self.write("entry.json", receipt) {
            Ok(_) => {
                self.state.set(State::Admitted);
                Ok(())
            }
            Err(error) => {
                self.state.set(State::Failed);
                self.failure_cleanup();
                Err(error)
            }
        }
    }
    /// Test faults are private cfg(test) only. Production executes the real IO
    /// and exposes a fixed error class, never raw paths/OS/provider messages.
    fn io<T>(
        &self,
        action: &'static str,
        name: &'static str,
        work: impl FnOnce() -> std::io::Result<T>,
    ) -> Result<T, EntryError> {
        #[cfg(test)]
        if let Err(error) = self.faults.check(action, name) {
            self.last_failed_io.set(Some((action, name)));
            return Err(error);
        }
        let result = work().map_err(|_| EntryError::Export);
        if result.is_err() {
            self.last_failed_io.set(Some((action, name)));
        }
        result
    }
    fn sync_directory(&self, phase: &'static str) -> Result<(), EntryError> {
        self.io("directory_sync", phase, || {
            File::open(&self.directory)?.sync_all()
        })
    }
    fn failure_cleanup(&self) {
        let primary_io = self.last_failed_io.get();
        // Only a marker created by THIS attempt may be withdrawn. Retain all
        // evidence, partial files, foreign collisions and borrowed credentials.
        let owned = self.owns_manifest.get();
        let revoked = if owned {
            self.io("unlink", "manifest.json", || {
                std::fs::remove_file(self.directory.join("manifest.json"))
            })
            .is_ok()
        } else {
            false
        };
        // Independent actions, deliberately not joined by `?`, && or early return.
        let synced = self.sync_directory("revoke").is_ok();
        let _=self.write("export-failure.json",&json!({"schema_version":1,
            "export_complete":false,"semantic_eod_passed":false,"outcome":"FAILED_UNKNOWN_DURABILITY",
            "failed_io":primary_io.map(|(action,name)|json!({"action":action,"public_file_or_phase":name})),
            "manifest_owned_by_attempt":owned,"manifest_revocation":if !owned{"NOT_OWNED_NOT_REMOVED"}else if revoked{"REMOVED"}else{"UNKNOWN"},
            "revocation_directory_synced":synced,"remaining_cleanup_attempted":true,
            "manifest_alone_is_not_export_acceptance":true,"credentials_included":false}));
        let _ = self.sync_directory("failure-receipt");
    }
    fn write(&self, name: &'static str, value: &impl Serialize) -> Result<String, EntryError> {
        let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| EntryError::Export)?;
        bytes.push(b'\n');
        if bytes.len() > 1024 * 1024 {
            return Err(EntryError::Bounds);
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = self.io("open", name, || options.open(self.directory.join(name)))?;
        if name == "manifest.json" {
            self.owns_manifest.set(true);
        }
        self.io("write", name, || file.write_all(&bytes))?;
        self.io("file_sync", name, || file.sync_all())?;
        Ok(digest(&bytes))
    }
    fn read(&self, name: &'static str, bound: u64) -> Result<Vec<u8>, EntryError> {
        let path = self.directory.join(name);
        let meta = std::fs::symlink_metadata(&path).map_err(|_| EntryError::Export)?;
        if !meta.is_file() || meta.len() > bound {
            return Err(EntryError::Bounds);
        }
        let mut bytes = vec![];
        self.io("read", name, || {
            File::open(path)?.take(bound + 1).read_to_end(&mut bytes)
        })?;
        if bytes.len() as u64 > bound {
            return Err(EntryError::Bounds);
        }
        Ok(bytes)
    }
    pub fn finish(
        &self,
        config: &WsPrimaryConfig,
        calendar_sha256: &str,
        report: Result<&live::RunReport, &live::Cause>,
    ) -> Result<ExportResult, EntryError> {
        // No retry/rewrite of a completed or failed attempt. In particular a
        // second call cannot revoke an already accepted first-call manifest.
        if self.state.get() != State::Admitted {
            return Err(EntryError::Export);
        }
        self.state.set(State::Finalizing);
        let result = self.finish_inner(config, calendar_sha256, report);
        match result {
            Ok(outcome) => {
                self.state.set(State::Finished);
                Ok(outcome)
            }
            Err(error) => {
                self.state.set(State::Failed);
                self.failure_cleanup();
                Err(error)
            }
        }
    }
    fn finish_inner(
        &self,
        config: &WsPrimaryConfig,
        calendar_sha256: &str,
        report: Result<&live::RunReport, &live::Cause>,
    ) -> Result<ExportResult, EntryError> {
        let mut files = BTreeMap::new();
        files.insert("entry.json", digest(&self.read("entry.json", 16384)?));
        files.insert("run-report.json", self.write("run-report.json", &report)?);
        let drained = report.as_ref().is_ok_and(|r| {
            r.all_workers_drained && r.socket_drained && r.http_drained && r.writer_drained
        });
        if !drained {
            // A still-running blocking callback may continue writing. Never
            // pin/seal its changing journal or create a completed manifest.
            let outcome = ExportResult {
                export_complete: false,
                semantic_eod_passed: false,
                all_workers_drained: false,
                full_session_qualified: false,
                network_qualified: false,
                cgroup_qualified: false,
                parity_accepted: false,
            };
            self.write("export-incomplete.json",&json!({"result":outcome,"journal_sealed":false,"requires_process_completion_witness":true}))?;
            self.sync_directory("incomplete")?;
            return Ok(outcome);
        }
        let journal = self.read(
            "journal.jsonl",
            config.base.max_output_bytes.min(64 * 1024 * 1024),
        )?;
        files.insert("journal.jsonl", digest(&journal));
        let mut verified = None;
        let mut refusal = "no_post_join_completion".to_owned();
        if let Ok(report) = report {
            if let Some(completion) = &report.completion {
                files.insert(
                    "completion.json",
                    self.write("completion.json", completion)?,
                );
                let completion = self.read("completion.json", 16384)?;
                let binding = EvidenceBinding {
                    schema_version: 1,
                    config_sha256: config.fingerprint().map_err(|_| EntryError::Binding)?,
                    calendar_sha256: calendar_sha256.into(),
                    journal_sha256: digest(&journal),
                    completion_sha256: digest(&completion),
                };
                files.insert(
                    "evidence-binding.json",
                    self.write("evidence-binding.json", &binding)?,
                );
                let consistent = report.all_workers_drained
                    && report.socket_drained
                    && report.http_drained
                    && report.writer_drained
                    && report.session_finalized
                    && report.canonical_commits == 101
                    && report.unresolved_transport_fault.is_none()
                    && matches!(report.cause, live::Cause::EndOfSession)
                    && !report.full_session_qualified;
                if consistent {
                    match verify::verify(
                        config,
                        &binding,
                        &journal,
                        &completion,
                        crate::timing::now_ms(),
                    ) {
                        Ok(result) => verified = Some(result),
                        Err(error) => refusal = format!("{error:?}"),
                    }
                } else {
                    refusal = "report_does_not_attest_complete_eod".into();
                }
            }
        }
        let outcome = ExportResult {
            export_complete: true,
            semantic_eod_passed: verified.is_some(),
            all_workers_drained: report.as_ref().is_ok_and(|r| r.all_workers_drained),
            full_session_qualified: false,
            network_qualified: false,
            cgroup_qualified: false,
            parity_accepted: false,
        };
        files.insert("verification.json",self.write("verification.json",&json!({"result":outcome,"verified":verified,
            "refusal":if outcome.semantic_eod_passed{None}else{Some(refusal)},"verification_is_not_provider_authentication":true}))?);
        self.write("manifest.json",&json!({"schema_version":1,"config_sha256":config.fingerprint().map_err(|_|EntryError::Binding)?,
            "calendar_sha256":calendar_sha256,"files_sha256":files,"result":outcome,"credentials_included":false,
            "requires_successful_export_return":true,"manifest_alone_is_not_durability_or_worker_witness":true}))?;
        self.sync_directory("publish")?;
        Ok(outcome)
    }
}

#[cfg(test)]
pub(crate) mod tests;
