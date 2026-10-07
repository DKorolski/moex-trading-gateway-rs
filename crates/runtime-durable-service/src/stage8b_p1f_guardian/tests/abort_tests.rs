use super::super::materialization_abort::{AbortPins, INTENT};
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub(super) enum Case {
    Reopen(u8),
    Reject(u8),
    PartialArchive,
    PartialCreation,
    ChangedParent,
    PrematureComplete,
    CorruptCompleted,
}

#[test]
fn o2_abort_linked_reopen_and_lost_response_frontiers() {
    // 5 is the existing fsynced event-temp checkpoint; all others are durable
    // abort/terminal frontiers. No SIGKILL claim is made.
    for point in [0, 2, 40, 41, 42, 43, 44, 45, 46, 5, 47, 52, 48, 51, 49] {
        o2_materialization_frontier_inner(None, true, 3, true, Some(Case::Reopen(point)));
        println!("PASS abort reopen frontier {point}: ACTIVE/1/11 -> EXPIRED/1/12");
    }
}

#[test]
fn o2_abort_initial_negative_cases_are_nonmutating() {
    for case in 0..28 {
        o2_materialization_frontier_inner(None, true, 3, true, Some(Case::Reject(case)));
        println!("PASS abort nonmutating negative {case}");
    }
}

#[test]
fn o2_abort_partial_archive_and_completed_archive_tamper_are_closed() {
    for case in [
        Case::PartialArchive,
        Case::PartialCreation,
        Case::ChangedParent,
        Case::PrematureComplete,
        Case::CorruptCompleted,
    ] {
        o2_materialization_frontier_inner(None, true, 3, true, Some(case));
    }
}

type Snapshot = BTreeMap<PathBuf, (u64, u64, u32, u32, u32, Vec<u8>)>;
struct NoRuntimeEffects;
impl NoRuntimeEffects {
    fn begin() -> Self {
        crate::stage8b_p1_semantic::p1e_i1_begin_direct_effect_audit();
        Self
    }
}
impl Drop for NoRuntimeEffects {
    fn drop(&mut self) {
        let counters = crate::stage8b_p1_semantic::p1e_i1_take_direct_effect_audit();
        assert_eq!(
            counters,
            Default::default(),
            "abort invoked runtime/provider/Redis effects"
        );
    }
}
fn snapshot(paths: &[&Path]) -> Snapshot {
    let mut result = BTreeMap::new();
    let mut todo: Vec<PathBuf> = paths.iter().map(|p| p.to_path_buf()).collect();
    while let Some(path) = todo.pop() {
        let m = fs::symlink_metadata(&path).unwrap();
        let bytes = if m.file_type().is_symlink() {
            fs::read_link(&path)
                .unwrap()
                .as_os_str()
                .as_bytes()
                .to_vec()
        } else if m.is_file() {
            fs::read(&path).unwrap()
        } else {
            vec![]
        };
        result.insert(
            path.clone(),
            (m.dev(), m.ino(), m.uid(), m.gid(), m.mode(), bytes),
        );
        if m.is_dir() {
            for e in fs::read_dir(path).unwrap() {
                todo.push(e.unwrap().path());
            }
        }
    }
    result
}

pub(super) fn exercise(
    case: Case,
    setup: &mut Setup,
    config: &Path,
    installation: &[u8],
    source: &[u8],
    manifest: &str,
    at: DateTime<Utc>,
) {
    let authority = setup.root.join(AUTHORITY_DIRECTORY);
    let dir = authority.join(MANIFESTS_DIRECTORY).join(manifest);
    let temp = config.join("bootstrap/.stage8b-p1-first-boot-source-v1.json.p1f-next");
    let pending = fs::read(dir.join(PENDING_MATERIALIZATION_FILE)).unwrap();
    let p: PendingMaterializationV1 = parse_canonical(&pending).unwrap();
    let phase: Stage8bP1fPhaseManifestV1 = setup
        .store
        .read_authority_file(&dir.join("phase-manifest.json"), 0o440)
        .unwrap();
    let staged = canonical_json(&serde_json::json!({"exact_source_json":std::str::from_utf8(source).unwrap(),"manifest_sha256":manifest})).unwrap();
    let mut pins = AbortPins {
        phase: manifest.into(),
        predecessor: p.predecessor_event_sha256.clone(),
        pending: sha256_hex(&pending),
        source: sha256_hex(source),
        source_size: source.len() as u64,
        staged: sha256_hex(&staged),
        installation: sha256_hex(installation),
        deadline: phase.deadline_utc,
        public_key: setup.public_key_hex(),
        key_id: "p1f-offline-1".into(),
        generation: 1,
        sequence: 12,
    };
    assert_eq!(p.authority_sequence, 12);
    let before_head: HistoryHeadV1 = setup
        .store
        .read_authority_file(&authority.join(HISTORY_HEAD_FILE), 0o440)
        .unwrap();
    assert_eq!(before_head.latest_sequence, 11);
    let _execution = setup.store.lock_o2_effects().unwrap();
    let effects = NoRuntimeEffects::begin();
    let mut call_at = at;
    if let Case::Reject(n) = case {
        match n {
            0 => pins.phase = "0".repeat(64),
            1 => pins.predecessor = "0".repeat(64),
            2 => pins.pending = "0".repeat(64),
            3 => pins.installation = "0".repeat(64),
            4 => pins.source = "0".repeat(64),
            5 => pins.public_key = "0".repeat(64),
            6 => pins.sequence = 13,
            7 => pins.generation = 2,
            8 => call_at = setup.now,
            9 => fs::set_permissions(&temp, fs::Permissions::from_mode(0o440)).unwrap(),
            10 => {
                fs::set_permissions(&temp, fs::Permissions::from_mode(0o600)).unwrap();
                fs::write(&temp, b"wrong source").unwrap();
                fs::set_permissions(&temp, fs::Permissions::from_mode(0o400)).unwrap();
            }
            11 => fs::hard_link(&temp, config.join("alias")).unwrap(),
            12 => {
                fs::remove_file(&temp).unwrap();
                std::os::unix::fs::symlink(config.join("missing"), &temp).unwrap();
            }
            13 => setup
                .store
                .write_create_new(
                    &dir.join("aborted-materialization-source.json"),
                    b"foreign",
                    0o400,
                )
                .unwrap(),
            14 => setup
                .store
                .write_create_new(&dir.join(MATERIALIZED_SET_RECEIPT_FILE), b"{}", 0o440)
                .unwrap(),
            15 => setup
                .store
                .write_create_new(&config.join("supervisor.json"), b"{}", 0o440)
                .unwrap(),
            16 => setup
                .store
                .write_create_new(&event_path(&authority, 12), b"{}", 0o440)
                .unwrap(),
            17 => setup
                .store
                .write_create_new(&dir.join(EXECUTION_OWNER_FILE), b"{}", 0o440)
                .unwrap(),
            18 => setup
                .store
                .write_create_new(&dir.join(INTENT), b"{}", 0o440)
                .unwrap(),
            19 => fs::set_permissions(
                dir.join(PENDING_MATERIALIZATION_FILE),
                fs::Permissions::from_mode(0o400),
            )
            .unwrap(),
            20 => setup
                .store
                .write_create_new(
                    &config.join("bootstrap/stage8b-p1-first-boot-source-v1.json"),
                    b"{}",
                    0o440,
                )
                .unwrap(),
            21 => {
                let mut h = before_head.clone();
                h.latest_event_sha256 = "0".repeat(64);
                setup
                    .store
                    .replace_exact(
                        &authority.join(HISTORY_HEAD_FILE),
                        &canonical_json(&h).unwrap(),
                        0o440,
                    )
                    .unwrap();
            }
            22 => {
                fs::set_permissions(
                    dir.join(PENDING_MATERIALIZATION_FILE),
                    fs::Permissions::from_mode(0o600),
                )
                .unwrap();
                fs::write(dir.join(PENDING_MATERIALIZATION_FILE), b"{}").unwrap();
                fs::set_permissions(
                    dir.join(PENDING_MATERIALIZATION_FILE),
                    fs::Permissions::from_mode(0o440),
                )
                .unwrap();
            }
            23 => setup
                .store
                .write_create_new(&config.join(".supervisor.json.p1f-next"), b"{}", 0o440)
                .unwrap(),
            24 => std::os::unix::fs::symlink(
                config.join("missing"),
                authority.join(".pending-terminal.json.p1f-create"),
            )
            .unwrap(),
            25 => setup
                .store
                .write_create_new(&dir.join("foreign-abort.json"), b"{}", 0o440)
                .unwrap(),
            26 => fs::set_permissions(config.join("bootstrap"), fs::Permissions::from_mode(0o755))
                .unwrap(),
            27 => setup
                .store
                .write_create_new(&authority.join(".history-head.json.next"), b"{}", 0o440)
                .unwrap(),
            _ => unreachable!(),
        }
        let before = snapshot(&[&setup.root, config]);
        assert!(
            setup
                .store
                .abort_expired_materialization(&pins, config, installation, &staged, call_at)
                .is_err(),
            "{case:?}"
        );
        assert_eq!(snapshot(&[&setup.root, config]), before, "{case:?}");
        return;
    }
    let point = match case {
        Case::Reopen(p) => p,
        Case::PartialArchive => 40,
        Case::PartialCreation => 50,
        Case::ChangedParent => 40,
        Case::PrematureComplete => 44,
        _ => 0,
    };
    P1F_TEST_FAULT_POINT.store(point, AtomicOrdering::SeqCst);
    let first = setup
        .store
        .abort_expired_materialization(&pins, config, installation, &staged, at);
    if point != 0 {
        assert_eq!(
            first.unwrap_err(),
            Stage8bP1fAuthorityErrorV1::Io(ErrorKind::Interrupted),
            "{point}"
        );
        if point != 49 {
            assert!(setup.store.inspect().is_err(), "inspect frontier {point}");
            assert!(setup.store.admit_active_phase(manifest, at).is_err());
            assert!(
                setup.store.resume_pending_terminal(manifest).is_err()
                    || !authority.join(PENDING_TERMINAL_FILE).exists()
            );
        }
    } else {
        assert_eq!(first.unwrap().authority_sequence, 12);
    }
    setup.store =
        Stage8bP1fAuthorityStoreV1::open_at(&setup.root, unsafe { libc::geteuid() }, unsafe {
            libc::getegid()
        })
        .unwrap();
    if matches!(case, Case::PrematureComplete) {
        let intent = fs::read(dir.join(INTENT)).unwrap();
        setup
            .store
            .write_create_new(
                &dir.join("materialization-abort-complete.json"),
                &canonical_json(&sha256_hex(&intent)).unwrap(),
                0o440,
            )
            .unwrap();
        let before = snapshot(&[&setup.root, config]);
        assert!(setup
            .store
            .abort_expired_materialization(&pins, config, installation, &staged, at)
            .is_err());
        assert_eq!(snapshot(&[&setup.root, config]), before);
        return;
    }
    if matches!(case, Case::ChangedParent) {
        fs::rename(config.join("bootstrap"), config.join("original-bootstrap")).unwrap();
        fs::create_dir(config.join("bootstrap")).unwrap();
        fs::set_permissions(config.join("bootstrap"), fs::Permissions::from_mode(0o750)).unwrap();
        let before = snapshot(&[&setup.root, config]);
        assert!(setup
            .store
            .abort_expired_materialization(&pins, config, installation, &staged, at)
            .is_err());
        assert_eq!(snapshot(&[&setup.root, config]), before);
        return;
    }
    if matches!(case, Case::PartialArchive | Case::PartialCreation) {
        if matches!(case, Case::PartialArchive) {
            setup
                .store
                .write_create_new(
                    &dir.join("aborted-materialization-source.json"),
                    b"partial",
                    0o400,
                )
                .unwrap();
        }
        let before = snapshot(&[&setup.root, config]);
        assert!(setup
            .store
            .abort_expired_materialization(&pins, config, installation, &staged, at)
            .is_err());
        assert_eq!(snapshot(&[&setup.root, config]), before);
        assert_eq!(fs::read(&temp).unwrap(), source);
        return;
    }
    // The later clock must not allocate a new decision timestamp.
    let receipt = setup
        .store
        .abort_expired_materialization(
            &pins,
            config,
            installation,
            &staged,
            at + Duration::hours(1),
        )
        .unwrap();
    assert_eq!(receipt.authority_generation, 1);
    assert_eq!(receipt.authority_sequence, 12);
    assert_eq!(receipt.recorded_at_utc, canonical_timestamp(at));
    assert_eq!(receipt.terminal_state, Stage8bP1fPhaseStateV1::Expired);
    assert_eq!(
        fs::read(dir.join("aborted-materialization-pending.json")).unwrap(),
        pending
    );
    assert_eq!(
        fs::read(dir.join("aborted-materialization-source.json")).unwrap(),
        source
    );
    assert_eq!(
        fs::metadata(dir.join("aborted-materialization-source.json"))
            .unwrap()
            .mode()
            & 0o777,
        0o400
    );
    assert!(!temp.exists());
    assert!(!dir.join(PENDING_MATERIALIZATION_FILE).exists());
    assert!(!dir.join(MATERIALIZED_SET_RECEIPT_FILE).exists());
    assert!(!dir.join(EXECUTION_OWNER_FILE).exists());
    assert!(!config.join("supervisor.json").exists());
    assert_eq!(
        setup.store.terminal_receipt(manifest).unwrap(),
        Some(receipt.clone())
    );
    let before = snapshot(&[&setup.root, config]);
    assert_eq!(
        setup
            .store
            .abort_expired_materialization(
                &pins,
                config,
                installation,
                &staged,
                at + Duration::hours(2)
            )
            .unwrap(),
        receipt
    );
    assert_eq!(
        snapshot(&[&setup.root, config]),
        before,
        "lost response changes retained state"
    );
    if matches!(case, Case::CorruptCompleted) {
        fs::set_permissions(
            dir.join("aborted-materialization-source.json"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        fs::write(
            dir.join("aborted-materialization-source.json"),
            b"bad archive",
        )
        .unwrap();
        fs::set_permissions(
            dir.join("aborted-materialization-source.json"),
            fs::Permissions::from_mode(0o400),
        )
        .unwrap();
        assert!(setup.store.inspect().is_err());
        return;
    }
    let head = setup.store.inspect().unwrap();
    assert_eq!(head.latest_sequence, 12);
    assert!(head.active_manifest_sha256.is_none() && head.deadline_utc.is_none());
    assert_eq!(
        fs::read_dir(authority.join(EVENTS_DIRECTORY))
            .unwrap()
            .count(),
        13
    );
    let terminal_event: AuthorityEventV1 = setup
        .store
        .read_authority_file(&event_path(&authority, 12), 0o440)
        .unwrap();
    assert_eq!(terminal_event.event_kind, "PHASE_TERMINAL");
    drop(effects); // Future fixture claim is NOT an abort effect.
    setup.now = at + Duration::hours(3);
    let next = setup.phase(&head, Stage8bP1fPhaseV1::O2MaterializeBootstrap);
    // Only the future signed claim, never a bootstrap/admit/run permit.
    setup
        .store
        .claim_phase(&next, &setup.public_key_hex(), "p1f-offline-1", setup.now)
        .unwrap();
    assert_eq!(setup.store.inspect().unwrap().latest_sequence, 13);
    assert!(!temp.exists());
}
