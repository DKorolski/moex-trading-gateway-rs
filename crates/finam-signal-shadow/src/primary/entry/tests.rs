use super::*;
use crate::{
    primary::calendar::tests::{Dir, Fixture},
    timing::{advance_fixture_clock, now_ms, with_fixture_clock},
};
use serde_json::json;

fn fixture() -> Fixture {
    let mut f = Fixture::new();
    f.config.base.previous_segment_id = None;
    f
}
pub(crate) fn plan(f: &Fixture, startup: Startup) -> Plan {
    let mut binding = f.binding.clone();
    binding.config_sha256 = f.config.fingerprint().unwrap();
    binding.model_day_sha256 = digest(&serde_json::to_vec(&f.model).unwrap());
    binding.raw_calendar_sha256 = digest(&serde_json::to_vec(&f.raw).unwrap());
    Plan {
        schema_version: 1,
        config_sha256: f.config.fingerprint().unwrap(),
        account_id: "ACCOUNT_TEST_PRIMARY".into(),
        read_only_credentials: true,
        auth_policy: "provider_expiry_readonly_rtsx_v1".into(),
        calendar: binding,
        startup,
    }
}
pub(crate) fn prepared(
    f: &Fixture,
    plan: Plan,
    prefix: Option<&[u8]>,
) -> Result<PreparedEntry, EntryError> {
    let model = serde_json::to_vec(&f.model).unwrap();
    let raw = serde_json::to_vec(&f.raw).unwrap();
    prepare(
        f.config.clone(),
        plan,
        Inputs {
            reference: &f.materials[0],
            mapping: &f.materials[1],
            seed_calendar: &f.materials[2],
            seed: &f.materials[3],
            model_day: &model,
            raw_calendar: &raw,
            prefix,
        },
    )
}
pub(crate) fn fresh(f: &Fixture) -> PreparedEntry {
    prepared(f, plan(f, Startup::FreshSession), None).unwrap()
}
fn go(t: i64) {
    advance_fixture_clock((t - now_ms()).try_into().unwrap());
}

#[test]
fn fresh_preparation_is_read_only_and_entry_rechecks_window_stop_and_output_identity() {
    let f = fixture();
    with_fixture_clock(f.open(), || {
        let d = Dir::new();
        let path = d.0.join("out");
        let prepared = fresh(&f);
        let receipt = serde_json::to_string(&prepared.receipt().unwrap()).unwrap();
        assert!(!receipt.contains("ACCOUNT_TEST_PRIMARY"));
        assert!(!path.exists());
        let opened = prepared.open(&path, StopHandle::default()).unwrap();
        assert_eq!(opened.owner.processor().next_label_ms(), Some(f.open()));
        assert!(path.join("journal.jsonl").is_file());
        assert!(path.join("entry.json").is_file());
        assert!(!path.join("completion.json").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                std::fs::metadata(path.join("entry.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let bytes = std::fs::read(path.join("journal.jsonl")).unwrap();
        assert!(matches!(
            fresh(&f).open(&path, StopHandle::default()),
            Err(EntryError::Output)
        ));
        assert_eq!(bytes, std::fs::read(path.join("journal.jsonl")).unwrap());
        let stop = StopHandle::default();
        stop.stop();
        assert!(matches!(
            fresh(&f).open(&d.0.join("stopped"), stop),
            Err(EntryError::Stopped)
        ));
        assert!(!d.0.join("stopped").exists());
        let prepared = fresh(&f);
        go(f.binding.start_before_ms);
        assert!(matches!(
            prepared.open(&d.0.join("late"), StopHandle::default()),
            Err(EntryError::Calendar)
        ));
        assert!(!d.0.join("late").exists());
    });
}

#[test]
fn entry_rejects_unbound_account_modes_seed_calendar_and_missing_or_partial_prefix() {
    let f = fixture();
    with_fixture_clock(f.open(), || {
        for key in [
            "readonly", "account", "auth", "config", "raw", "seed", "reset", "prefix",
        ] {
            let mut f = f.clone();
            let mut p = plan(&f, Startup::FreshSession);
            let bytes = b"partial-prefix";
            let prefix = match key {
                "readonly" => {
                    p.read_only_credentials = false;
                    None
                }
                "account" => {
                    p.account_id = "unsafe account".into();
                    None
                }
                "auth" => {
                    p.auth_policy = "fallback".into();
                    None
                }
                "config" => {
                    p.config_sha256 = "0".repeat(64);
                    None
                }
                "raw" => {
                    p.calendar.raw_calendar_sha256 = "0".repeat(64);
                    None
                }
                "seed" => {
                    f.materials[3].push(b' ');
                    None
                }
                "reset" => {
                    f.config.base.previous_segment_id = Some("prior".into());
                    p = plan(&f, Startup::FreshSession);
                    None
                }
                "prefix" => Some(bytes.as_slice()),
                _ => unreachable!(),
            };
            assert!(prepared(&f, p, prefix).is_err(), "{key}");
        }
        let p = plan(&f, Startup::FreshSession);
        let raw = serde_json::to_vec(&p).unwrap();
        assert!(Plan::parse(&raw).is_ok());
        let text = String::from_utf8(raw).unwrap();
        let duplicate = text.replacen('{', "{\"schema_version\":1,", 1);
        assert!(Plan::parse(duplicate.as_bytes()).is_err());
        let mut value = serde_json::to_value(&p).unwrap();
        value["endpoint"] = json!("http://untrusted.invalid");
        assert!(Plan::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    });
}

#[test]
fn accepted_committed_prefix_restores_same_model_frontier_but_not_transport_readiness() {
    let mut f = fixture();
    with_fixture_clock(f.open(), || {
        let d = Dir::new();
        let old_path = d.0.join("old.jsonl");
        let mut owner = f.owner(&old_path, true);
        for i in 0..=20 {
            let t = f.open() + i * 60000;
            go(t + 100);
            let text=json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(t)]}}).to_string();
            let frame = super::super::WireFrame::normalize(
                &text,
                1,
                "fixture",
                chrono::DateTime::from_timestamp_millis(now_ms()).unwrap(),
            )
            .unwrap();
            owner.on_frame(frame).unwrap();
            if i > 0 && i % 10 == 0 {
                go(t + 1100);
                assert!(matches!(
                    owner.tick().unwrap(),
                    crate::runner::primary::Tick::Published { .. }
                ));
            }
        }
        drop(owner);
        let old = std::fs::read(&old_path).unwrap();
        let previous = f.config.clone();
        f.config.base.previous_segment_id = Some(f.config.base.segment_id.clone());
        f.config.base.segment_id = "successor-primary".into();
        f.binding.next_label_ms = f.open() + 1200000;
        f.binding.start_not_before_ms = f.binding.next_label_ms;
        f.binding.start_before_ms = f.binding.next_label_ms + 540000;
        let binding = RestartBinding {
            schema_version: 1,
            config_sha256: f.config.fingerprint().unwrap(),
            previous_config_sha256: previous.fingerprint().unwrap(),
            previous_journal_sha256: digest(&old),
            previous_segment_id: previous.base.segment_id,
            required_labels_sha256: restart::inventory_sha256(&f.model.required_m10_labels_ms),
            committed_labels: 2,
            max_reconstruction_ms: 10000,
        };
        let p = plan(&f, Startup::CommittedPrefix { binding });
        assert!(prepared(&f, p.clone(), None).is_err());
        let candidate = prepared(&f, p.clone(), Some(&old)).unwrap();
        assert_eq!(candidate.receipt().unwrap().committed_prefix_labels, 2);
        let mut opened = candidate
            .open(&d.0.join("resumed"), StopHandle::default())
            .unwrap();
        assert_eq!(
            opened.owner.processor().next_label_ms(),
            Some(f.binding.next_label_ms)
        );
        assert!(!opened.owner.processor().subscription_data_ready());
        assert_eq!(opened.owner.processor().counters().restart_committed, 2);
        opened.owner.bind_calendar(opened.calendar).unwrap();
        assert_eq!(old, std::fs::read(old_path).unwrap());
        let mut damaged = old.clone();
        damaged.extend(b"{\"sequence\":");
        let mut p = p;
        if let Startup::CommittedPrefix { binding } = &mut p.startup {
            binding.previous_journal_sha256 = digest(&damaged);
        }
        assert!(prepared(&f, p, Some(&damaged)).is_err());
    });
}

#[test]
fn output_creation_refuses_existing_files_and_leaf_symlinks() {
    let f = fixture();
    with_fixture_clock(f.open(), || {
        let d = Dir::new();
        let path = d.0.join("file");
        std::fs::write(&path, b"existing").unwrap();
        assert!(matches!(
            fresh(&f).open(&path, StopHandle::default()),
            Err(EntryError::Output)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), b"existing");
        #[cfg(unix)]
        {
            let link = d.0.join("link");
            std::os::unix::fs::symlink(&d.0, &link).unwrap();
            assert!(matches!(
                fresh(&f).open(&link, StopHandle::default()),
                Err(EntryError::Output)
            ));
        }
    });
}

#[test]
fn unresolved_worker_export_never_pins_a_mutable_journal_or_writes_a_completion_manifest() {
    let f = fixture();
    with_fixture_clock(f.open(), || {
        let d = Dir::new();
        let path = d.0.join("incomplete");
        let receipt = fresh(&f).receipt().unwrap();
        let artifacts = artifacts::Artifacts::create(&path).unwrap();
        artifacts.record_admission(&receipt).unwrap();
        let report = Ok(live::RunReport {
            completion: None,
            recovery: vec![],
            unresolved_transport_fault: None,
            cause: live::Cause::DrainTimeout,
            canonical_commits: 0,
            http: live::HttpTotals::default(),
            socket_drained: true,
            http_drained: true,
            writer_drained: false,
            all_workers_drained: false,
            session_finalized: false,
            socket: None,
            full_session_qualified: false,
        });
        let result = artifacts
            .finish(&f.config, &receipt.calendar_sha256, report.as_ref())
            .unwrap();
        assert!(!result.export_complete && !result.semantic_eod_passed);
        assert!(path.join("export-incomplete.json").is_file());
        for name in ["completion.json", "evidence-binding.json", "manifest.json"] {
            assert!(!path.join(name).exists());
        }
    });
}

#[test]
fn public_run_refuses_wrong_auth_or_client_account_before_creating_outputs() {
    let f = fixture();
    with_fixture_clock(f.open(), || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let d = Dir::new();
                for mismatch_auth in [true, false] {
                    let account = "ACCOUNT_TEST_PRIMARY";
                    let other = "OTHER_ACCOUNT";
                    let auth = AuthLifecycle::new(
                        broker_finam::SecretToken::new("dummy-not-a-real-credential"),
                        if mismatch_auth { other } else { account }.into(),
                    )
                    .unwrap();
                    let client = Arc::new(
                        Stage8bP1fO2GetOnlyClientV1::new(if mismatch_auth {
                            account
                        } else {
                            other
                        })
                        .unwrap(),
                    );
                    let output = d.0.join(format!("mismatch-{mismatch_auth}"));
                    assert!(matches!(
                        run(fresh(&f), auth, client, &output, StopHandle::default()).await,
                        Err(EntryError::Binding)
                    ));
                    assert!(!output.exists());
                }
            });
    });
}
