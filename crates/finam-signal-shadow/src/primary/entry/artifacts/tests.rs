//! Private, per-attempt IO faults: no production switches/endpoint overrides.
use super::*;
use crate::{
    primary::calendar::tests::{Dir, Fixture},
    timing::with_fixture_clock,
};
use std::{cell::RefCell, collections::BTreeSet};

#[derive(Default)]
pub(super) struct Faults {
    failures: RefCell<BTreeSet<(String, String)>>,
    calls: RefCell<Vec<(String, String)>>,
}
impl Faults {
    pub fn check(&self, action: &str, name: &str) -> Result<(), EntryError> {
        let key = (action.into(), name.into());
        self.calls.borrow_mut().push(key.clone());
        if self.failures.borrow().contains(&key) {
            Err(EntryError::Export)
        } else {
            Ok(())
        }
    }
    fn arm(&self, faults: &[(&str, &str)]) {
        *self.failures.borrow_mut() = faults
            .iter()
            .map(|(a, n)| ((*a).into(), (*n).into()))
            .collect();
        self.calls.borrow_mut().clear();
    }
    fn called(&self, a: &str, n: &str) -> bool {
        self.calls.borrow().contains(&(a.into(), n.into()))
    }
}
fn report(writer_drained: bool) -> live::RunReport {
    live::RunReport {
        completion: None,
        recovery: vec![],
        unresolved_transport_fault: None,
        cause: live::Cause::RequestedStop,
        canonical_commits: 0,
        http: live::HttpTotals::default(),
        socket_drained: true,
        http_drained: true,
        writer_drained,
        all_workers_drained: writer_drained,
        session_finalized: writer_drained,
        socket: None,
        full_session_qualified: false,
    }
}
fn receipt(f: &Fixture) -> AdmissionReceipt {
    super::super::tests::fresh(f).receipt().unwrap()
}
fn failure(a: &Artifacts) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(a.directory.join("export-failure.json")).unwrap())
        .unwrap()
}
fn assert_cleanup(a: &Artifacts) {
    assert!(a.faults.called("directory_sync", "revoke"));
    assert!(a.faults.called("open", "export-failure.json"));
    assert!(a.faults.called("directory_sync", "failure-receipt"));
}
fn public_no_canary(path: &Path) {
    for e in std::fs::read_dir(path).unwrap() {
        let p = e.unwrap().path();
        if std::fs::symlink_metadata(&p).unwrap().is_file() {
            let bytes = std::fs::read(p).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            assert!(!text.contains("SECRET_PATH_CANARY"));
            assert!(!text.contains("ACCOUNT_TEST_PRIMARY"));
        }
    }
}

#[test]
fn admission_receipt_failures_are_terminal_before_any_output_runtime_or_retry() {
    let mut f = Fixture::new();
    f.config.base.previous_segment_id = None;
    with_fixture_clock(f.open(), || {
        for action in ["open", "write", "file_sync"] {
            let d = Dir::new();
            let a = Artifacts::create(&d.0.join("out")).unwrap();
            a.faults.arm(&[(action, "entry.json")]);
            assert!(a.record_admission(&receipt(&f)).is_err());
            assert_cleanup(&a);
            assert_eq!(failure(&a)["export_complete"], false);
            assert!(!a.journal().exists());
            assert!(!a.directory.join("manifest.json").exists());
            let calls = a.faults.calls.borrow().clone();
            assert!(a.record_admission(&receipt(&f)).is_err());
            assert!(a.finish(&f.config, "unused", Ok(&report(true))).is_err());
            assert_eq!(calls, *a.faults.calls.borrow());
            public_no_canary(&a.directory);
        }
    });
}

#[test]
fn unknown_workers_do_not_read_or_seal_the_journal_even_when_incomplete_export_fails() {
    let mut f = Fixture::new();
    f.config.base.previous_segment_id = None;
    with_fixture_clock(f.open(), || {
        for fault in [
            None,
            Some(("write", "export-incomplete.json")),
            Some(("directory_sync", "incomplete")),
        ] {
            let d = Dir::new();
            let a = Artifacts::create(&d.0.join("out")).unwrap();
            let r = receipt(&f);
            a.record_admission(&r).unwrap();
            a.faults.arm(
                &fault
                    .into_iter()
                    .chain([("read", "journal.jsonl")])
                    .collect::<Vec<_>>(),
            );
            // Deliberately mutable/incomplete and not valid JSONL.
            std::fs::write(a.journal(), b"partial writer tail").unwrap();
            let before = std::fs::read(a.journal()).unwrap();
            let done = report(false);
            let result = a.finish(&f.config, &r.calendar_sha256, Ok(&done));
            if fault.is_none() {
                let result = result.unwrap();
                assert!(!result.export_complete && !result.semantic_eod_passed);
            } else {
                assert!(result.is_err());
                assert_cleanup(&a);
            }
            assert!(!a.faults.called("read", "journal.jsonl"));
            assert_eq!(before, std::fs::read(a.journal()).unwrap());
            for name in ["manifest.json", "completion.json", "evidence-binding.json"] {
                assert!(!a.directory.join(name).exists());
            }
        }
    });
}

/// Called by the actual managed compressed-day EOD test, not a forged PASS DTO.
pub(crate) fn export_fault_matrix(
    config: &WsPrimaryConfig,
    entry: &AdmissionReceipt,
    report: &live::RunReport,
    journal: &[u8],
) {
    assert!(report.completion.as_ref().unwrap().semantic_eod_passed);
    let d = Dir::new();
    // This output-path canary is NOT a real secret; raw IO messages must not
    // leak it or the account through public diagnostic receipts.
    let root = d.0.join("SECRET_PATH_CANARY");
    std::fs::create_dir(&root).unwrap();
    let borrowed = root.join("borrowed-credential");
    std::fs::write(&borrowed, b"dummy-borrowed-secret").unwrap();
    let setup = |name: &str| {
        let a = Artifacts::create(&root.join(name)).unwrap();
        a.record_admission(entry).unwrap();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(a.journal())
            .unwrap();
        file.write_all(journal).unwrap();
        file.sync_all().unwrap();
        a
    };
    let positive = setup("positive");
    let done = positive
        .finish(config, &entry.calendar_sha256, Ok(report))
        .unwrap();
    assert!(done.export_complete && done.semantic_eod_passed && done.all_workers_drained);
    let manifest = std::fs::read(positive.directory.join("manifest.json")).unwrap();
    let calls = positive.faults.calls.borrow().clone();
    assert!(positive
        .finish(config, &entry.calendar_sha256, Ok(report))
        .is_err());
    assert_eq!(calls, *positive.faults.calls.borrow());
    assert_eq!(
        manifest,
        std::fs::read(positive.directory.join("manifest.json")).unwrap()
    );
    assert!(!positive.directory.join("export-failure.json").exists());

    let mut cases: Vec<(String, Vec<(&str, &str)>)> = vec![];
    for name in [
        "run-report.json",
        "completion.json",
        "evidence-binding.json",
        "verification.json",
        "manifest.json",
    ] {
        for action in ["open", "write", "file_sync"] {
            cases.push((format!("{action}-{name}"), vec![(action, name)]));
        }
    }
    for name in ["entry.json", "journal.jsonl", "completion.json"] {
        cases.push((format!("read-{name}"), vec![("read", name)]));
    }
    cases.push((
        "directory-sync-publish".into(),
        vec![("directory_sync", "publish")],
    ));
    for (id, action, name) in [
        ("unlink-failure", "unlink", "manifest.json"),
        ("revoke-sync-failure", "directory_sync", "revoke"),
        ("failure-receipt-open", "open", "export-failure.json"),
        ("failure-receipt-sync", "file_sync", "export-failure.json"),
        (
            "failure-receipt-directory-sync",
            "directory_sync",
            "failure-receipt",
        ),
    ] {
        cases.push((
            id.into(),
            vec![("file_sync", "manifest.json"), (action, name)],
        ));
    }
    cases.push((
        "unlink-and-receipt-failure".into(),
        vec![
            ("file_sync", "manifest.json"),
            ("unlink", "manifest.json"),
            ("open", "export-failure.json"),
        ],
    ));
    assert_eq!(cases.len(), 25);
    let mut evidence = vec![];
    for (index, (id, faults)) in cases.iter().enumerate() {
        let a = setup(&format!("case-{index}"));
        a.faults.arm(faults);
        assert!(
            a.finish(config, &entry.calendar_sha256, Ok(report))
                .is_err(),
            "{id}"
        );
        assert_cleanup(&a);
        let owned = a.owns_manifest.get();
        assert_eq!(a.faults.called("unlink", "manifest.json"), owned);
        let marker_remains = a.directory.join("manifest.json").exists();
        assert_eq!(
            marker_remains,
            id == "unlink-failure" || id == "unlink-and-receipt-failure",
            "{id}"
        );
        let failure_bytes = std::fs::read(a.directory.join("export-failure.json")).ok();
        let failure = failure_bytes
            .as_deref()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok());
        if let Some(value) = &failure {
            assert_eq!(value["export_complete"], false);
            assert_eq!(value["semantic_eod_passed"], false);
            assert_eq!(value["failed_io"]["action"], faults[0].0);
            assert_eq!(value["failed_io"]["public_file_or_phase"], faults[0].1);
            if marker_remains {
                assert_eq!(value["manifest_revocation"], "UNKNOWN");
            }
        }
        assert_eq!(journal, std::fs::read(a.journal()).unwrap());
        assert_eq!(std::fs::read(&borrowed).unwrap(), b"dummy-borrowed-secret");
        public_no_canary(&a.directory);
        let calls = a.faults.calls.borrow().clone();
        assert!(a
            .finish(config, &entry.calendar_sha256, Ok(report))
            .is_err());
        assert_eq!(calls, *a.faults.calls.borrow(), "no retry cleanup: {id}");
        evidence.push(json!({"id":id,"faults":faults,"result":"ERR_NOT_PASS","calls":calls,
            "manifest_owned":owned,"manifest_remains":marker_remains,"failure_receipt":failure,
            "journal_unchanged":true,"borrowed_credential_unchanged":true,"public_canary_absent":true}));
    }
    for symlink in [false, true] {
        #[cfg(not(unix))]
        if symlink {
            continue;
        }
        let a = setup(if symlink {
            "foreign-symlink"
        } else {
            "foreign-file"
        });
        let target = a.directory.join("manifest.json");
        if symlink {
            #[cfg(unix)]
            std::os::unix::fs::symlink(&borrowed, &target).unwrap();
        } else {
            std::fs::write(&target, b"FOREIGN_KEEP").unwrap();
        }
        a.faults.arm(&[]);
        assert!(a
            .finish(config, &entry.calendar_sha256, Ok(report))
            .is_err());
        assert_cleanup(&a);
        assert!(!a.faults.called("unlink", "manifest.json"));
        assert_eq!(failure(&a)["manifest_revocation"], "NOT_OWNED_NOT_REMOVED");
        if symlink {
            assert!(std::fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink());
        } else {
            assert_eq!(std::fs::read(target).unwrap(), b"FOREIGN_KEEP");
        }
        assert_eq!(std::fs::read(&borrowed).unwrap(), b"dummy-borrowed-secret");
        evidence.push(json!({"id":if symlink{"foreign-symlink"}else{"foreign-file"},
            "result":"ERR_NOT_PASS","calls":a.faults.calls.borrow().clone(),"foreign_unchanged":true,
            "borrowed_credential_unchanged":true}));
    }
    emit(config, &positive.directory, &evidence);
}
fn emit(config: &WsPrimaryConfig, positive: &Path, cases: &[serde_json::Value]) {
    if let Some(base) = std::env::var_os("WS_PRIMARY_T39_EVIDENCE_DIR") {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = Path::new(&base).join(format!(
            "run-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
        std::fs::create_dir(&dir).unwrap();
        let out = dir.join("positive");
        std::fs::create_dir(&out).unwrap();
        for name in [
            "entry.json",
            "run-report.json",
            "journal.jsonl",
            "completion.json",
            "evidence-binding.json",
            "verification.json",
            "manifest.json",
        ] {
            std::fs::copy(positive.join(name), out.join(name)).unwrap();
        }
        for (name, value) in [
            ("config.json", serde_json::to_value(config).unwrap()),
            (
                "faults.json",
                json!({
            "scenario":"T39","scope":"LOCAL_IO_FAULTS_AND_SYNTHETIC_COMPRESSED_DAY_NOT_FINAM",
            "primary_fault_cases":25,"foreign_collision_cases":cases.len()-25,"cases":cases,
            "positive_semantic_eod_verified":true,"no_fault_returned_pass":true,"network_qualified":false,
            "verification_now_ms":crate::timing::now_ms(),
            "cgroup_qualified":false,"operational_qualified":false}),
            ),
        ] {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir.join(name))
                .unwrap();
            file.write_all(&serde_json::to_vec_pretty(&value).unwrap())
                .unwrap();
            file.sync_all().unwrap();
        }
        println!("T39_EVIDENCE_DIR={}", dir.display());
    }
}
