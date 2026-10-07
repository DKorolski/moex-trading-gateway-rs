//! Terminal-only consumption of the reviewed October-7 pre-publication incident.
//! No materialization, admission, service start, broker or Redis capability.
use super::*;

pub(super) const INTENT: &str = "materialization-abort-intent.json";
const COMPLETE: &str = "materialization-abort-complete.json";
const SOURCE_ARCHIVE: &str = "aborted-materialization-source.json";
const PENDING_ARCHIVE: &str = "aborted-materialization-pending.json";
const SOURCE_TEMP: &str = ".stage8b-p1-first-boot-source-v1.json.p1f-next";
const REASON: &str = "o2-materialization-custody-expired.";

type Result<T> = std::result::Result<T, Stage8bP1fAuthorityErrorV1>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AbortPins {
    pub phase: String,
    pub predecessor: String,
    pub pending: String,
    pub source: String,
    pub source_size: u64,
    pub staged: String,
    pub installation: String,
    pub deadline: String,
    pub public_key: String,
    pub key_id: String,
    pub generation: u64,
    pub sequence: u64,
}

pub(crate) fn incident_pins() -> AbortPins {
    AbortPins {
        phase: "992be63a6406153eb5bab43d746927df598d59e24df2d113ae12c605752b80b5".into(),
        predecessor: "7880c9da6d1ab573bf20fd6b221c0717e35056bbdb12ae318dd298cc82ab89e9".into(),
        pending: "42322f612f1604a053874baeff3b0c99ba53878e8036678c144c9c1bd8eff9b6".into(),
        source: "f4c135d7f3fec1a2164c4dfbf0aa9f6f960830acb79373fb9f7450f2763f9952".into(),
        source_size: 1_876_641,
        staged: "7135486f3abe51d9233e53e780b8f2db67f29faa8496f935ad023972b376dcbb".into(),
        installation: "395f9e3e987ce6f5b2c52b6083d1ebc79c71287df72096c6d313ee3e56da32f5".into(),
        deadline: "2026-10-07T04:25:00Z".into(),
        public_key: "8ed71461f37c51d6239db25aeac1e709f250bd16d6d7bb6ba117b716cebf4802".into(),
        key_id: "stage8b-p1f-o2-authority-generation-1".into(),
        generation: 1,
        sequence: 12,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    dev: u64,
    ino: u64,
    uid: u32,
    gid: u32,
    mode: u32,
}
impl Identity {
    fn of(m: &fs::Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            uid: m.uid(),
            gid: m.gid(),
            mode: m.mode(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AbortBinding {
    schema_version: u16,
    domain: String,
    pins: AbortPins,
    pending_bytes: String,
    claim_sha256: String,
    source_original: Identity,
    pending_original: Identity,
    config_parent: Identity,
    source_parent: Identity,
    archive_parent: Identity,
    source_archive: String,
    pending_archive: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AbortIntent {
    binding: AbortBinding,
    pub(super) terminal: PendingTerminalV1,
}

impl AbortIntent {
    fn reason(&self) -> Result<String> {
        Ok(format!(
            "{REASON}{}",
            sha256_hex(&canonical_json(&self.binding)?)
        ))
    }
    pub(super) fn receipt(&self) -> Stage8bP1fTerminalReceiptV1 {
        let p = &self.terminal;
        Stage8bP1fTerminalReceiptV1 {
            schema_version: 1,
            domain: "stage8b-p1f-terminal-receipt-v1".into(),
            manifest_sha256: p.manifest_sha256.clone(),
            authority_generation: p.authority_generation,
            authority_sequence: p.authority_sequence,
            predecessor_event_sha256: p.predecessor_event_sha256.clone(),
            terminal_state: p.terminal_state,
            reason_code: p.reason_code.clone(),
            recorded_at_utc: p.recorded_at_utc.clone(),
        }
    }
}

// Path entries are never followed when testing absence (including dangling links).
fn present(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
fn absent(path: &Path) -> Result<()> {
    if present(path)? {
        Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired)
    } else {
        Ok(())
    }
}
fn prepared(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        ".{}.p1f-create",
        path.file_name().unwrap().to_string_lossy()
    ))
}

// Retain each parent inode, use descriptor-relative archive writes/unlinks, and
// revalidate its full path before each effect. A swapped/symlink parent is not
// an alternate root. This is private to this single abort, not a restore API.
struct Parent {
    path: PathBuf,
    file: File,
    identity: Identity,
}
impl Parent {
    fn open(store: &Stage8bP1fAuthorityStoreV1, path: &Path) -> Result<Self> {
        if fs::canonicalize(path)? != path {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidPath);
        }
        store.validate_directory(path)?;
        let file = open_directory(path)?;
        let identity = Identity::of(&file.metadata()?);
        let result = Self {
            path: path.into(),
            file,
            identity,
        };
        result.check()?;
        Ok(result)
    }
    fn check(&self) -> Result<()> {
        if fs::canonicalize(&self.path)? != self.path
            || Identity::of(&fs::symlink_metadata(&self.path)?) != self.identity
            || Identity::of(&self.file.metadata()?) != self.identity
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        Ok(())
    }
    fn open_file(&self, name: &str, flags: i32, mode: u32) -> Result<File> {
        self.check()?;
        let name = CString::new(name).map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidPath)?;
        let fd = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
                mode as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn read(
        &self,
        store: &Stage8bP1fAuthorityStoreV1,
        name: &str,
        mode: u32,
        size: u64,
    ) -> Result<(Vec<u8>, Identity)> {
        let mut file = self.open_file(name, libc::O_RDONLY, 0)?;
        let metadata = file.metadata()?;
        validate_file_metadata(&metadata, store.expected_uid, store.service_gid, mode)?;
        if metadata.len() != size || size > crate::STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidDocument);
        }
        let identity = Identity::of(&metadata);
        let mut bytes = Vec::new();
        (&mut file).take(size + 1).read_to_end(&mut bytes)?;
        self.check()?;
        if bytes.len() as u64 != size
            || Identity::of(&fs::symlink_metadata(self.path.join(name))?) != identity
            || Identity::of(&file.metadata()?) != identity
            || file.metadata()?.nlink() != 1
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        Ok((bytes, identity))
    }
    fn archive(
        &self,
        store: &Stage8bP1fAuthorityStoreV1,
        name: &str,
        bytes: &[u8],
        mode: u32,
    ) -> Result<()> {
        if !present(&self.path.join(name))? {
            let mut file =
                self.open_file(name, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL, mode)?;
            if unsafe { libc::fchmod(file.as_raw_fd(), mode as libc::mode_t) } != 0
                || unsafe { libc::fchown(file.as_raw_fd(), store.expected_uid, store.service_gid) }
                    != 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            if name == SOURCE_ARCHIVE {
                file.sync_all()?;
                self.file.sync_all()?;
                inject_test_fault(50)?;
            }
            file.write_all(bytes)?;
            file.sync_all()?;
            self.file.sync_all()?;
        }
        let (retained, _) = self.read(store, name, mode, bytes.len() as u64)?;
        if retained != bytes {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        self.open_file(name, libc::O_RDONLY, 0)?.sync_all()?;
        self.file.sync_all()?;
        Ok(())
    }
    fn consume(
        &self,
        store: &Stage8bP1fAuthorityStoreV1,
        name: &str,
        bytes: &[u8],
        identity: &Identity,
        mode: u32,
    ) -> Result<()> {
        if !present(&self.path.join(name))? {
            return Ok(());
        }
        let (retained, actual) = self.read(store, name, mode, bytes.len() as u64)?;
        if retained != bytes || &actual != identity {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        // Keep the original inode open through unlink; the parent is root-only
        // writable and both guardian/execution locks are held.
        let original = self.open_file(name, libc::O_RDONLY, 0)?;
        if Identity::of(&original.metadata()?) != *identity
            || Identity::of(&fs::symlink_metadata(self.path.join(name))?) != *identity
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        self.check()?;
        let name = CString::new(name).map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidPath)?;
        if unsafe { libc::unlinkat(self.file.as_raw_fd(), name.as_ptr(), 0) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if original.metadata()?.nlink() != 0 {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        self.file.sync_all()?;
        Ok(())
    }
}

impl Stage8bP1fAuthorityStoreV1 {
    /// Caller holds the execution lock and supplies fresh, checked stopped/
    /// preservation evidence. No public arbitrary-path or generic repair API.
    pub(crate) fn abort_expired_materialization(
        &self,
        pins: &AbortPins,
        config_root: &Path,
        installation: &[u8],
        staged: &[u8],
        now: DateTime<Utc>,
    ) -> Result<Stage8bP1fTerminalReceiptV1> {
        let _lease = self.acquire_lease()?;
        self.reject_quarantine()?;
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let directory = authority.join(MANIFESTS_DIRECTORY).join(&pins.phase);
        let archive = Parent::open(self, &directory)?;
        let config = Parent::open(self, config_root)?;
        let source_parent = Parent::open(self, &config_root.join("bootstrap"))?;
        let phase_bytes =
            self.read_authority_bytes(&directory.join("phase-manifest.json"), 0o440)?;
        let phase = verify_phase(&phase_bytes, pins)?;
        if sha256_hex(installation) != pins.installation
            || sha256_hex(staged) != pins.staged
            || now < parse_timestamp(&pins.deadline)?
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        let installed: Value = serde_json::from_slice(installation)
            .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
        let genesis: Stage8bP1fGenesisManifestV1 =
            self.read_authority_file(&authority.join(GENESIS_MANIFEST_FILE), 0o440)?;
        if installed["installation_id"].as_str() != Some(phase.installation_id.as_str())
            || installed["target_id"].as_str() != Some(STAGE8B_P1F_TARGET_HOST_ID)
            || genesis.installation_id != phase.installation_id
            || genesis.control_root != self.root.to_string_lossy()
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        // Historical bytes only. Do NOT call fresh-source validation, Hybrid
        // replay or materializer (the old observation must remain old).
        let staged_value: Value = serde_json::from_slice(staged)
            .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?;
        let source = staged_value["exact_source_json"]
            .as_str()
            .ok_or(Stage8bP1fAuthorityErrorV1::InvalidDocument)?
            .as_bytes();
        if staged_value["manifest_sha256"].as_str() != Some(&pins.phase)
            || source.len() as u64 != pins.source_size
            || sha256_hex(source) != pins.source
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        for path in [
            config_root.join("supervisor.json"),
            config_root.join(".supervisor.json.p1f-next"),
            config_root.join("bootstrap/stage8b-p1-first-boot-source-v1.json"),
            directory.join(MATERIALIZED_SET_RECEIPT_FILE),
            directory.join(EXECUTION_OWNER_FILE),
        ] {
            absent(&path)?;
            absent(&prepared(&path))?;
        }
        absent(&authority.join(PENDING_CLAIM_FILE))?;
        absent(&authority.join(PENDING_STOPPING_FILE))?;
        absent(&prepared(&authority.join(PENDING_CLAIM_FILE)))?;
        absent(&prepared(&authority.join(PENDING_STOPPING_FILE)))?;
        for parent in [config_root.to_path_buf(), config_root.join("bootstrap")] {
            for entry in fs::read_dir(&parent)? {
                let name = entry?
                    .file_name()
                    .into_string()
                    .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidPath)?;
                if (name.contains("supervisor.json")
                    || name.contains("stage8b-p1-first-boot-source-v1.json"))
                    && !(parent == config_root.join("bootstrap") && name == SOURCE_TEMP)
                {
                    return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
                }
            }
        }
        let intent_path = directory.join(INTENT);
        let retained_path = if present(&intent_path)? {
            intent_path.clone()
        } else {
            prepared(&intent_path)
        };
        let initial = !present(&retained_path)?;
        let allowed = [
            "phase-manifest.json".to_string(),
            "claim-receipt.json".to_string(),
            PENDING_MATERIALIZATION_FILE.to_string(),
            INTENT.to_string(),
            COMPLETE.to_string(),
            SOURCE_ARCHIVE.to_string(),
            PENDING_ARCHIVE.to_string(),
            format!(".{INTENT}.p1f-create"),
            format!(".{COMPLETE}.p1f-create"),
            format!("terminal-receipt-{:020}.json", pins.sequence),
            format!(".terminal-receipt-{:020}.json.p1f-create", pins.sequence),
        ];
        for e in fs::read_dir(&directory)? {
            let name = e?
                .file_name()
                .into_string()
                .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidPath)?;
            if !allowed.contains(&name) {
                return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
            }
        }
        let intent = if initial {
            absent(&directory.join(COMPLETE))?;
            absent(&prepared(&directory.join(COMPLETE)))?;
            absent(&directory.join(SOURCE_ARCHIVE))?;
            absent(&directory.join(PENDING_ARCHIVE))?;
            absent(&authority.join(PENDING_TERMINAL_FILE))?;
            absent(&prepared(&authority.join(PENDING_TERMINAL_FILE)))?;
            absent(&authority.join(format!(".{HISTORY_HEAD_FILE}.next")))?;
            absent(&prepared(
                &authority.join(format!(".{HISTORY_HEAD_FILE}.next")),
            ))?;
            absent(&event_path(&authority, pins.sequence))?;
            absent(&prepared(&event_path(&authority, pins.sequence)))?;
            let terminal_path =
                directory.join(format!("terminal-receipt-{:020}.json", pins.sequence));
            absent(&terminal_path)?;
            absent(&prepared(&terminal_path))?;
            let pending_bytes =
                self.read_authority_bytes(&directory.join(PENDING_MATERIALIZATION_FILE), 0o440)?;
            if sha256_hex(&pending_bytes) != pins.pending {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            let (actual_source, source_original) =
                source_parent.read(self, SOURCE_TEMP, 0o400, pins.source_size)?;
            if actual_source != source {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            let head = self.validate_history_with_pending(true, None, Some(pins.sequence))?;
            if head.latest_sequence + 1 != pins.sequence
                || head.state != Stage8bP1fPhaseStateV1::Active
                || head.latest_event_sha256 != pins.predecessor
                || head.authority_generation != pins.generation
                || head.active_manifest_sha256.as_deref() != Some(&pins.phase)
                || head.deadline_utc.as_deref() != Some(&pins.deadline)
            {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            let binding = AbortBinding {
                schema_version: 1,
                domain: "stage8b-p1f-materialization-abort-binding-v1".into(),
                pins: pins.clone(),
                pending_bytes: String::from_utf8(pending_bytes)
                    .map_err(|_| Stage8bP1fAuthorityErrorV1::InvalidDocument)?,
                claim_sha256: sha256_hex(
                    &self.read_authority_bytes(&directory.join("claim-receipt.json"), 0o440)?,
                ),
                source_original,
                pending_original: Identity::of(&fs::symlink_metadata(
                    directory.join(PENDING_MATERIALIZATION_FILE),
                )?),
                config_parent: config.identity.clone(),
                source_parent: source_parent.identity.clone(),
                archive_parent: archive.identity.clone(),
                source_archive: SOURCE_ARCHIVE.into(),
                pending_archive: PENDING_ARCHIVE.into(),
            };
            let mut intent = AbortIntent {
                binding,
                terminal: PendingTerminalV1 {
                    schema_version: 1,
                    domain: "stage8b-p1f-pending-terminal-v1".into(),
                    manifest_sha256: pins.phase.clone(),
                    authority_generation: pins.generation,
                    authority_sequence: pins.sequence,
                    predecessor_event_sha256: pins.predecessor.clone(),
                    terminal_state: Stage8bP1fPhaseStateV1::Expired,
                    reason_code: String::new(),
                    recorded_at_utc: canonical_timestamp(now),
                },
            };
            intent.terminal.reason_code = intent.reason()?;
            intent
        } else {
            self.read_authority_file::<AbortIntent>(&retained_path, 0o440)?
        };
        self.validate_abort_intent(&intent)?;
        if intent.binding.pins != *pins
            || intent.binding.archive_parent != archive.identity
            || intent.binding.source_parent != source_parent.identity
            || intent.binding.config_parent != config.identity
        {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        self.validate_abort_terminal_frontier(&intent, initial)?;
        // Validate ALL existing archive/original frontiers before ANY replay
        // mutation. In particular never remove an original beside partial bytes.
        for (parent, name, bytes, mode, identity) in [
            (
                &source_parent,
                SOURCE_TEMP,
                source,
                0o400,
                &intent.binding.source_original,
            ),
            (
                &archive,
                PENDING_MATERIALIZATION_FILE,
                intent.binding.pending_bytes.as_bytes(),
                0o440,
                &intent.binding.pending_original,
            ),
        ] {
            if present(&parent.path.join(name))? {
                let (actual, id) = parent.read(self, name, mode, bytes.len() as u64)?;
                if actual != bytes || id != *identity {
                    return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
                }
            }
        }
        for (name, bytes, mode, original) in [
            (
                SOURCE_ARCHIVE,
                source,
                0o400,
                source_parent.path.join(SOURCE_TEMP),
            ),
            (
                PENDING_ARCHIVE,
                intent.binding.pending_bytes.as_bytes(),
                0o440,
                directory.join(PENDING_MATERIALIZATION_FILE),
            ),
        ] {
            if present(&directory.join(name))? {
                if archive.read(self, name, mode, bytes.len() as u64)?.0 != bytes {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
            } else if !present(&original)? {
                return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
            }
        }
        if !initial {
            self.validate_history_inner(true, None, Some(pins.sequence), Some(&intent))?;
        }
        let head: HistoryHeadV1 =
            self.read_authority_file(&authority.join(HISTORY_HEAD_FILE), 0o440)?;
        if !((head.latest_sequence + 1 == pins.sequence
            && head.latest_event_sha256 == pins.predecessor
            && head.state == Stage8bP1fPhaseStateV1::Active
            && head.active_manifest_sha256.as_deref() == Some(&pins.phase))
            || (head.latest_sequence == pins.sequence
                && head.state == Stage8bP1fPhaseStateV1::Expired
                && head.active_manifest_sha256.is_none()))
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        config.check()?;
        source_parent.check()?;
        archive.check()?;
        self.write_or_require_exact(&intent_path, &canonical_json(&intent)?, 0o440)?;
        inject_test_fault(40)?;
        archive.archive(
            self,
            PENDING_ARCHIVE,
            intent.binding.pending_bytes.as_bytes(),
            0o440,
        )?;
        inject_test_fault(41)?;
        archive.archive(self, SOURCE_ARCHIVE, source, 0o400)?;
        inject_test_fault(42)?;
        config.check()?;
        source_parent.check()?;
        archive.check()?;
        source_parent.consume(
            self,
            SOURCE_TEMP,
            source,
            &intent.binding.source_original,
            0o400,
        )?;
        inject_test_fault(43)?;
        archive.consume(
            self,
            PENDING_MATERIALIZATION_FILE,
            intent.binding.pending_bytes.as_bytes(),
            &intent.binding.pending_original,
            0o440,
        )?;
        inject_test_fault(44)?;
        let complete_path = directory.join(COMPLETE);
        if present(&complete_path)? {
            self.validate_history(true)?;
            if self.terminal_receipt(&pins.phase)?.as_ref() != Some(&intent.receipt()) {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            return Ok(intent.receipt());
        }
        self.validate_abort_archives(&intent)?;
        self.write_or_require_exact(
            &authority.join(PENDING_TERMINAL_FILE),
            &canonical_json(&intent.terminal)?,
            0o440,
        )?;
        inject_test_fault(45)?;
        let receipt = self.continue_terminal_transaction_inner(&intent.terminal, Some(&intent))?;
        self.validate_history_inner(true, None, Some(pins.sequence), Some(&intent))?;
        self.write_or_require_exact(
            &complete_path,
            &canonical_json(&sha256_hex(&canonical_json(&intent)?))?,
            0o440,
        )?;
        inject_test_fault(49)?;
        self.validate_history(true)?;
        Ok(receipt)
    }

    fn abort_directory(&self, intent: &AbortIntent) -> PathBuf {
        self.root
            .join(AUTHORITY_DIRECTORY)
            .join(MANIFESTS_DIRECTORY)
            .join(&intent.binding.pins.phase)
    }

    fn validate_abort_terminal_frontier(&self, i: &AbortIntent, initial: bool) -> Result<()> {
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        let receipt = i.receipt();
        let receipt_bytes = canonical_json(&receipt)?;
        let event = AuthorityEventV1 {
            schema_version: 1,
            domain: "stage8b-p1f-authority-event-v1".into(),
            authority_generation: receipt.authority_generation,
            authority_sequence: receipt.authority_sequence,
            predecessor_event_sha256: receipt.predecessor_event_sha256.clone(),
            event_kind: "PHASE_TERMINAL".into(),
            state: receipt.terminal_state,
            manifest_sha256: receipt.manifest_sha256.clone(),
            receipt_sha256: sha256_hex(&receipt_bytes),
            recorded_at_utc: receipt.recorded_at_utc.clone(),
        };
        let head = HistoryHeadV1 {
            schema_version: 1,
            domain: "stage8b-p1f-history-head-v1".into(),
            authority_generation: receipt.authority_generation,
            latest_sequence: receipt.authority_sequence,
            latest_event_sha256: event_digest(&canonical_json(&event)?),
            state: receipt.terminal_state,
            active_manifest_sha256: None,
            deadline_utc: None,
            force_kill_at_utc: None,
        };
        for (path, bytes) in [
            (
                authority.join(PENDING_TERMINAL_FILE),
                canonical_json(&i.terminal)?,
            ),
            (
                self.abort_directory(i).join(format!(
                    "terminal-receipt-{:020}.json",
                    receipt.authority_sequence
                )),
                receipt_bytes,
            ),
            (
                event_path(&authority, receipt.authority_sequence),
                canonical_json(&event)?,
            ),
            (
                authority.join(format!(".{HISTORY_HEAD_FILE}.next")),
                canonical_json(&head)?,
            ),
            (
                self.abort_directory(i).join(COMPLETE),
                canonical_json(&sha256_hex(&canonical_json(i)?))?,
            ),
        ] {
            let tmp = prepared(&path);
            if present(&path)? && present(&tmp)? {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            if path == self.abort_directory(i).join(COMPLETE) && (present(&path)? || present(&tmp)?)
            {
                let actual: HistoryHeadV1 =
                    self.read_authority_file(&authority.join(HISTORY_HEAD_FILE), 0o440)?;
                if actual != head {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
            }
            for candidate in [path, tmp] {
                if present(&candidate)?
                    && (initial || self.read_authority_bytes(&candidate, 0o440)? != bytes)
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
            }
        }
        Ok(())
    }

    fn validate_abort_intent(&self, i: &AbortIntent) -> Result<()> {
        let b = &i.binding;
        let p = &b.pins;
        if b.schema_version != 1
            || b.domain != "stage8b-p1f-materialization-abort-binding-v1"
            || b.source_archive != SOURCE_ARCHIVE
            || b.pending_archive != PENDING_ARCHIVE
            || !valid_sha256(&p.phase)
            || !valid_sha256(&p.source)
            || !valid_sha256(&p.pending)
            || !valid_sha256(&p.predecessor)
            || p.sequence == 0
            || p.source_size == 0
            || p.source_size > crate::STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES
            || i.terminal.schema_version != 1
            || i.terminal.domain != "stage8b-p1f-pending-terminal-v1"
            || i.terminal.manifest_sha256 != p.phase
            || i.terminal.authority_generation != p.generation
            || i.terminal.authority_sequence != p.sequence
            || i.terminal.predecessor_event_sha256 != p.predecessor
            || i.terminal.terminal_state != Stage8bP1fPhaseStateV1::Expired
            || i.terminal.reason_code != i.reason()?
            || parse_timestamp(&i.terminal.recorded_at_utc)? < parse_timestamp(&p.deadline)?
            || sha256_hex(b.pending_bytes.as_bytes()) != p.pending
            || b.source_original.mode & 0o7777 != 0o400
            || b.pending_original.mode & 0o7777 != 0o440
            || b.source_original.uid != self.expected_uid
            || b.source_original.gid != self.service_gid
            || b.pending_original.uid != self.expected_uid
            || b.pending_original.gid != self.service_gid
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        let directory = self.abort_directory(i);
        let phase = verify_phase(
            &self.read_authority_bytes(&directory.join("phase-manifest.json"), 0o440)?,
            p,
        )?;
        let claim = self.read_claim_receipt(&p.phase)?;
        let pending: PendingMaterializationV1 = parse_canonical(b.pending_bytes.as_bytes())?;
        if b.claim_sha256 != sha256_hex(&canonical_json(&claim)?)
            || pending.schema_version != 1
            || pending.domain != "stage8b-p1f-pending-materialization-v1"
            || pending.manifest_sha256 != p.phase
            || pending.authority_generation != p.generation
            || pending.authority_sequence != p.sequence
            || pending.predecessor_event_sha256 != p.predecessor
            || pending.source_sha256 != p.source
            || pending.installation_sha256 != p.installation
            || pending.claim_receipt_sha256 != b.claim_sha256
            || pending.materialization_policy_sha256 != phase.materialization_policy_sha256
            || pending.config_template_sha256 != phase.config_template_sha256
            || !valid_sha256(&pending.final_config_sha256)
            || claim.authority_sequence + 1 != p.sequence
            || claim.deadline_utc != p.deadline
            || parse_timestamp(&pending.ready_at_utc)? >= parse_timestamp(&p.deadline)?
            || parse_timestamp(&pending.broker_truth_checked_at_utc)?
                > parse_timestamp(&pending.ready_at_utc)?
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        Ok(())
    }

    fn validate_abort_archives(&self, i: &AbortIntent) -> Result<()> {
        self.validate_abort_intent(i)?;
        let archive = Parent::open(self, &self.abort_directory(i))?;
        if archive.identity != i.binding.archive_parent {
            return Err(Stage8bP1fAuthorityErrorV1::InvalidCustody);
        }
        let (source, _) = archive.read(self, SOURCE_ARCHIVE, 0o400, i.binding.pins.source_size)?;
        let (pending, _) = archive.read(
            self,
            PENDING_ARCHIVE,
            0o440,
            i.binding.pending_bytes.len() as u64,
        )?;
        if sha256_hex(&source) != i.binding.pins.source
            || pending != i.binding.pending_bytes.as_bytes()
        {
            return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
        }
        Ok(())
    }

    pub(super) fn validate_abort_markers(&self, expected: Option<&AbortIntent>) -> Result<()> {
        let root = self
            .root
            .join(AUTHORITY_DIRECTORY)
            .join(MANIFESTS_DIRECTORY);
        if !present(&root)? {
            return Ok(());
        }
        let mut unresolved = 0;
        for entry in fs::read_dir(root)? {
            let dir = entry?.path();
            self.validate_directory(&dir)?;
            let path = dir.join(INTENT);
            let final_exists = present(&path)?;
            let prepared_exists = present(&prepared(&path))?;
            if final_exists && prepared_exists {
                return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
            }
            if !final_exists && !prepared_exists {
                for name in [COMPLETE, SOURCE_ARCHIVE, PENDING_ARCHIVE] {
                    absent(&dir.join(name))?;
                }
                continue;
            }
            let intent: AbortIntent = self
                .read_authority_file(&if final_exists { path } else { prepared(&path) }, 0o440)?;
            if prepared_exists && expected != Some(&intent) {
                return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
            }
            self.validate_abort_intent(&intent)?;
            if self.abort_directory(&intent) != dir {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
            if present(&dir.join(COMPLETE))? {
                let complete: String = self.read_authority_file(&dir.join(COMPLETE), 0o440)?;
                if complete != sha256_hex(&canonical_json(&intent)?) {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
                self.validate_abort_archives(&intent)?;
                absent(&dir.join(PENDING_MATERIALIZATION_FILE))?;
                let head: HistoryHeadV1 = self.read_authority_file(
                    &self.root.join(AUTHORITY_DIRECTORY).join(HISTORY_HEAD_FILE),
                    0o440,
                )?;
                let terminal: Stage8bP1fTerminalReceiptV1 = self.read_authority_file(
                    &dir.join(format!(
                        "terminal-receipt-{:020}.json",
                        intent.terminal.authority_sequence
                    )),
                    0o440,
                )?;
                if head.latest_sequence < intent.terminal.authority_sequence
                    || terminal != intent.receipt()
                {
                    return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                }
            } else {
                unresolved += 1;
                if expected != Some(&intent) {
                    return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
                }
            }
        }
        if unresolved > 1 {
            return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired);
        }
        Ok(())
    }

    pub(super) fn substitute_abort_pending(
        &self,
        i: &AbortIntent,
        pending: &mut Vec<(String, PathBuf)>,
    ) -> Result<()> {
        let authority = self.root.join(AUTHORITY_DIRECTORY);
        for (name, path) in pending.iter() {
            match name.as_str() {
                PENDING_TERMINAL_FILE if path == &authority.join(PENDING_TERMINAL_FILE) => {
                    if self.read_authority_file::<PendingTerminalV1>(path, 0o440)? != i.terminal {
                        return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                    }
                }
                PENDING_MATERIALIZATION_FILE
                    if path == &self.abort_directory(i).join(PENDING_MATERIALIZATION_FILE) =>
                {
                    if self.read_authority_bytes(path, 0o440)? != i.binding.pending_bytes.as_bytes()
                    {
                        return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
                    }
                }
                _ => return Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired),
            }
        }
        pending.clear();
        let path = self.abort_directory(i).join(INTENT);
        let path = if present(&path)? {
            path
        } else {
            prepared(&path)
        };
        pending.push((INTENT.into(), path));
        Ok(())
    }

    pub(super) fn validate_abort_terminal(
        &self,
        receipt: &Stage8bP1fTerminalReceiptV1,
    ) -> Result<()> {
        let dir = self
            .root
            .join(AUTHORITY_DIRECTORY)
            .join(MANIFESTS_DIRECTORY)
            .join(&receipt.manifest_sha256);
        if receipt.reason_code.starts_with(REASON) || present(&dir.join(INTENT))? {
            let intent: AbortIntent = self.read_authority_file(&dir.join(INTENT), 0o440)?;
            self.validate_abort_archives(&intent)?;
            if intent.receipt() != *receipt {
                return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
            }
        }
        Ok(())
    }
}

fn verify_phase(bytes: &[u8], p: &AbortPins) -> Result<Stage8bP1fPhaseManifestV1> {
    let phase: Stage8bP1fPhaseManifestV1 = read_signed_document(
        bytes,
        SIGNED_PHASE_DOMAIN,
        &p.public_key,
        &p.key_id,
        |v: &Stage8bP1fPhaseManifestV1| &v.issuer_key_id,
        |v: &Stage8bP1fPhaseManifestV1| &v.signature_ed25519_hex,
        |v: &Stage8bP1fPhaseManifestV1| {
            let mut v = v.clone();
            v.signature_ed25519_hex.clear();
            v
        },
    )?;
    if sha256_hex(bytes) != p.phase
        || phase.schema_version != 1
        || phase.domain != "stage8b-p1f-phase-manifest-v1"
        || phase.phase != Stage8bP1fPhaseV1::O2MaterializeBootstrap
        || phase.authority_generation != p.generation
        || phase.authority_sequence + 1 != p.sequence
        || phase.deadline_utc != p.deadline
        || phase.installation_sha256 != p.installation
        || phase.target_host_id != STAGE8B_P1F_TARGET_HOST_ID
        || phase.target_host_ssh_ed25519_sha256 != STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256
    {
        return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict);
    }
    Ok(phase)
}
