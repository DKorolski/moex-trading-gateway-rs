//! Fixed-path process composition for the Stage 8B-P1-e paper supervisor.
//!
//! This boundary deliberately owns no FINAM dependency and no broker send
//! capability.  Bootstrap and recovery are filesystem-only.  `run` performs
//! authenticated local classification before the verify-only Redis attach.

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::Path,
};

use chrono::{DateTime, Utc};

use crate::{
    authorize_stage8b_p1_first_boot, authorize_stage8b_p1e_pre_seal_recovery_v5,
    build_stage8b_p1_first_boot_source_v1, first_boot_stage8b_p1e_transaction_v5,
    load_stage8b_p1_commitment_key_from_systemd_credential,
    recover_stage8b_p1e_first_boot_adoption_v5,
    recover_stage8b_p1e_first_boot_pre_seal_from_supervisor_v5,
    validate_stage8b_p1e_supervisor_config_v1, Stage8bP1eAdoptionRecoveryActionV5,
    Stage8bP1ePreSealRecoveryActionV5, Stage8bP1eSupervisorConfigV1,
    Stage8bP1eValidatedSupervisorConfigV1, STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION,
    STAGE8B_P1E_SUPERVISOR_CONFIG_PATH,
};

use crate::stage8b_p1e_first_boot_transaction::next_stage8b_p1e_bootstrap_attempt_generation_v5;

const STAGE8B_P1E_BOOT_ID_PATH: &str = "/proc/sys/kernel/random/boot_id";
const STAGE8B_P1E_CONFIG_MAX_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage8bP1eProcessCommandV1 {
    ValidateConfig,
    Bootstrap,
    BootstrapRecover {
        transaction_id_sha256: String,
        action: Stage8bP1eProcessRecoveryActionV1,
    },
    Run,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eProcessRecoveryActionV1 {
    PreSeal(Stage8bP1ePreSealRecoveryActionV5),
    Adoption(Stage8bP1eAdoptionRecoveryActionV5),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage8bP1eProcessSuccessV1 {
    ConfigValid,
    BootstrapAdopted,
    RecoveryApplied,
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1eProcessErrorV1 {
    #[error("invalid Stage 8B-P1-e command line")]
    Usage,
    #[error("supervisor config boundary is invalid")]
    ConfigBoundary,
    #[error("supervisor config is invalid")]
    Config,
    #[error("host boot identity is invalid")]
    BootIdentity,
    #[error("lifecycle credential is invalid")]
    Credential,
    #[error("first-boot source is invalid")]
    FirstBootSource,
    #[error("first-boot transaction failed")]
    FirstBootTransaction,
    #[error("first-boot recovery failed")]
    FirstBootRecovery,
    #[error("ordinary run requires one authenticated adopted root")]
    RunNotAdopted,
    #[error("owner-loop composition is not present in this administrative checkpoint")]
    OwnerLoopUnavailable,
    #[error("durable restart failed")]
    DurableRestart,
    #[error("durable restart is blocked before Redis")]
    RestartBlocked,
    #[error("verify-only Redis attachment failed")]
    RedisAttach,
}

impl Stage8bP1eProcessErrorV1 {
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Usage | Self::ConfigBoundary | Self::Config | Self::BootIdentity => 64,
            Self::Credential
            | Self::FirstBootSource
            | Self::FirstBootTransaction
            | Self::FirstBootRecovery
            | Self::RunNotAdopted
            | Self::OwnerLoopUnavailable
            | Self::DurableRestart
            | Self::RestartBlocked => 66,
            Self::RedisAttach => 67,
        }
    }
}

/// Parses only the deployment-identity V2 argv grammar.  The config path is
/// fixed; arbitrary files and environment-selected paths are rejected.
pub fn parse_stage8b_p1e_process_command_v1<I, S>(
    args: I,
) -> Result<Stage8bP1eProcessCommandV1, Stage8bP1eProcessErrorV1>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let strings: Option<Vec<&str>> = args.iter().map(|value| value.to_str()).collect();
    let strings = strings.ok_or(Stage8bP1eProcessErrorV1::Usage)?;
    match strings.as_slice() {
        [mode, config]
            if *config == STAGE8B_P1E_SUPERVISOR_CONFIG_PATH && *mode == "validate-config" =>
        {
            Ok(Stage8bP1eProcessCommandV1::ValidateConfig)
        }
        [mode, config, confirmation]
            if *config == STAGE8B_P1E_SUPERVISOR_CONFIG_PATH
                && *mode == "bootstrap"
                && *confirmation == crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION =>
        {
            Ok(Stage8bP1eProcessCommandV1::Bootstrap)
        }
        [mode, config] if *config == STAGE8B_P1E_SUPERVISOR_CONFIG_PATH && *mode == "run" => {
            Ok(Stage8bP1eProcessCommandV1::Run)
        }
        [mode, config, selector, confirmation]
            if *config == STAGE8B_P1E_SUPERVISOR_CONFIG_PATH
                && *mode == "bootstrap-recover"
                && *confirmation == STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION =>
        {
            let (transaction_id_sha256, action) = parse_recovery_selector(selector)?;
            Ok(Stage8bP1eProcessCommandV1::BootstrapRecover {
                transaction_id_sha256,
                action,
            })
        }
        _ => Err(Stage8bP1eProcessErrorV1::Usage),
    }
}

pub async fn execute_stage8b_p1e_process_command_v1(
    command: Stage8bP1eProcessCommandV1,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    execute_with_boundaries(
        command,
        Path::new(STAGE8B_P1E_SUPERVISOR_CONFIG_PATH),
        Path::new(STAGE8B_P1E_BOOT_ID_PATH),
        Utc::now(),
        0,
    )
    .await
}

async fn execute_with_boundaries(
    command: Stage8bP1eProcessCommandV1,
    config_path: &Path,
    boot_id_path: &Path,
    trusted_now: DateTime<Utc>,
    expected_config_uid: u32,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    let supervisor = load_validated_supervisor(config_path, boot_id_path, expected_config_uid)?;
    match command {
        Stage8bP1eProcessCommandV1::ValidateConfig => Ok(Stage8bP1eProcessSuccessV1::ConfigValid),
        Stage8bP1eProcessCommandV1::Bootstrap => execute_bootstrap(supervisor, trusted_now),
        Stage8bP1eProcessCommandV1::BootstrapRecover {
            transaction_id_sha256,
            action,
        } => execute_bootstrap_recovery(supervisor, trusted_now, &transaction_id_sha256, action),
        // `run` is part of the frozen argv grammar, but this intermediate
        // administrative checkpoint must fail closed until the exhaustive
        // restart-route owner loop is composed. In particular it must not
        // attach Redis and then exit successfully after dropping the owner.
        Stage8bP1eProcessCommandV1::Run => {
            drop(supervisor);
            Err(Stage8bP1eProcessErrorV1::OwnerLoopUnavailable)
        }
    }
}

fn load_validated_supervisor(
    config_path: &Path,
    boot_id_path: &Path,
    expected_config_uid: u32,
) -> Result<Stage8bP1eValidatedSupervisorConfigV1, Stage8bP1eProcessErrorV1> {
    let bytes = read_protected_config(config_path, expected_config_uid)?;
    let config: Stage8bP1eSupervisorConfigV1 =
        crate::parse_stage8b_p1e_supervisor_config_v1(&bytes)
            .map_err(|_| Stage8bP1eProcessErrorV1::Config)?;
    let boot_id = read_boot_id(boot_id_path)?;
    validate_stage8b_p1e_supervisor_config_v1(config, boot_id)
        .map_err(|_| Stage8bP1eProcessErrorV1::Config)
}

fn execute_bootstrap(
    supervisor: Stage8bP1eValidatedSupervisorConfigV1,
    trusted_now: DateTime<Utc>,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    let commitment_key = load_stage8b_p1_commitment_key_from_systemd_credential()
        .map_err(|_| Stage8bP1eProcessErrorV1::Credential)?;
    let admin = authorize_stage8b_p1_first_boot(
        supervisor.bootstrap(),
        crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
    )
    .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootTransaction)?;
    let generation =
        next_stage8b_p1e_bootstrap_attempt_generation_v5(supervisor.bootstrap(), &commitment_key)
            .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootTransaction)?;
    let prepared = build_stage8b_p1_first_boot_source_v1(supervisor, trusted_now)
        .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootSource)?;
    drop(
        first_boot_stage8b_p1e_transaction_v5(prepared, admin, generation, &commitment_key)
            .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootTransaction)?,
    );
    Ok(Stage8bP1eProcessSuccessV1::BootstrapAdopted)
}

fn execute_bootstrap_recovery(
    supervisor: Stage8bP1eValidatedSupervisorConfigV1,
    trusted_now: DateTime<Utc>,
    transaction_id_sha256: &str,
    action: Stage8bP1eProcessRecoveryActionV1,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    let commitment_key = load_stage8b_p1_commitment_key_from_systemd_credential()
        .map_err(|_| Stage8bP1eProcessErrorV1::Credential)?;
    match action {
        Stage8bP1eProcessRecoveryActionV1::PreSeal(action) => {
            let selector = authorize_stage8b_p1e_pre_seal_recovery_v5(
                supervisor.bootstrap(),
                transaction_id_sha256,
                action,
                STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION,
            )
            .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootRecovery)?;
            drop(
                recover_stage8b_p1e_first_boot_pre_seal_from_supervisor_v5(
                    supervisor,
                    trusted_now,
                    selector,
                    &commitment_key,
                )
                .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootRecovery)?,
            );
        }
        Stage8bP1eProcessRecoveryActionV1::Adoption(action) => {
            let (config, runtime) = supervisor.into_first_boot_parts();
            drop(
                recover_stage8b_p1e_first_boot_adoption_v5(
                    config,
                    &commitment_key,
                    runtime,
                    transaction_id_sha256,
                    action,
                )
                .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootRecovery)?,
            );
        }
    }
    Ok(Stage8bP1eProcessSuccessV1::RecoveryApplied)
}

fn parse_recovery_selector(
    selector: &str,
) -> Result<(String, Stage8bP1eProcessRecoveryActionV1), Stage8bP1eProcessErrorV1> {
    let Some((transaction_id, action)) = selector.split_once('.') else {
        return Err(Stage8bP1eProcessErrorV1::Usage);
    };
    if transaction_id.len() != 64
        || !transaction_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || action.contains('.')
    {
        return Err(Stage8bP1eProcessErrorV1::Usage);
    }
    let action = match action {
        "remove-marker-temp" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::RemoveMarkerTemp,
        ),
        "resume-prepared" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::ResumePrepared,
        ),
        "quarantine-root" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::QuarantineRoot,
        ),
        "finalize-quarantine" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::FinalizeQuarantine,
        ),
        "complete-prepared-to-root-published" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::CompletePreparedToRootPublished,
        ),
        "complete-root-published-to-journal-durable" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::CompleteRootPublishedToJournalDurable,
        ),
        "complete-journal-durable-to-seal-committed" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::CompleteJournalDurableToSealCommitted,
        ),
        "remove-receipt-temp-and-adopt" => Stage8bP1eProcessRecoveryActionV1::Adoption(
            Stage8bP1eAdoptionRecoveryActionV5::RemoveReceiptTempAndAdopt,
        ),
        "adopt-committed-root" => Stage8bP1eProcessRecoveryActionV1::Adoption(
            Stage8bP1eAdoptionRecoveryActionV5::AdoptCommittedRoot,
        ),
        "start-seal-committed-to-adopted" => Stage8bP1eProcessRecoveryActionV1::Adoption(
            Stage8bP1eAdoptionRecoveryActionV5::StartSealCommittedToAdopted,
        ),
        "complete-seal-committed-to-adopted" => Stage8bP1eProcessRecoveryActionV1::Adoption(
            Stage8bP1eAdoptionRecoveryActionV5::CompleteSealCommittedToAdopted,
        ),
        _ => return Err(Stage8bP1eProcessErrorV1::Usage),
    };
    Ok((transaction_id.to_string(), action))
}

fn read_protected_config(
    path: &Path,
    expected_uid: u32,
) -> Result<Vec<u8>, Stage8bP1eProcessErrorV1> {
    if !path.is_absolute() || path.as_os_str().as_bytes().contains(&0) {
        return Err(Stage8bP1eProcessErrorV1::ConfigBoundary);
    }
    let before =
        fs::symlink_metadata(path).map_err(|_| Stage8bP1eProcessErrorV1::ConfigBoundary)?;
    validate_config_metadata(&before, expected_uid)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| Stage8bP1eProcessErrorV1::ConfigBoundary)?;
    let opened = file
        .metadata()
        .map_err(|_| Stage8bP1eProcessErrorV1::ConfigBoundary)?;
    validate_config_metadata(&opened, expected_uid)?;
    if !same_file_metadata(&before, &opened) {
        return Err(Stage8bP1eProcessErrorV1::ConfigBoundary);
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(opened.len()).map_err(|_| Stage8bP1eProcessErrorV1::ConfigBoundary)?,
    );
    file.by_ref()
        .take(STAGE8B_P1E_CONFIG_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Stage8bP1eProcessErrorV1::ConfigBoundary)?;
    let after = file
        .metadata()
        .map_err(|_| Stage8bP1eProcessErrorV1::ConfigBoundary)?;
    if bytes.len() as u64 != opened.len()
        || bytes.len() as u64 > STAGE8B_P1E_CONFIG_MAX_BYTES
        || !same_file_metadata(&opened, &after)
    {
        return Err(Stage8bP1eProcessErrorV1::ConfigBoundary);
    }
    Ok(bytes)
}

fn validate_config_metadata(
    metadata: &fs::Metadata,
    expected_uid: u32,
) -> Result<(), Stage8bP1eProcessErrorV1> {
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.nlink() != 1
        || metadata.uid() != expected_uid
        || metadata.permissions().mode() & 0o022 != 0
        || metadata.len() > STAGE8B_P1E_CONFIG_MAX_BYTES
    {
        return Err(Stage8bP1eProcessErrorV1::ConfigBoundary);
    }
    Ok(())
}

fn same_file_metadata(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.len() == right.len()
        && left.uid() == right.uid()
        && left.gid() == right.gid()
        && left.nlink() == right.nlink()
        && left.permissions().mode() & 0o777 == right.permissions().mode() & 0o777
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

fn read_boot_id(path: &Path) -> Result<[u8; 16], Stage8bP1eProcessErrorV1> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(64).read_to_end(&mut bytes))
        .map_err(|_| Stage8bP1eProcessErrorV1::BootIdentity)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Stage8bP1eProcessErrorV1::BootIdentity)?
        .trim_end_matches('\n');
    if text.len() != 36 || text.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(Stage8bP1eProcessErrorV1::BootIdentity);
    }
    let uuid = uuid::Uuid::parse_str(text).map_err(|_| Stage8bP1eProcessErrorV1::BootIdentity)?;
    if uuid.hyphenated().to_string() != text {
        return Err(Stage8bP1eProcessErrorV1::BootIdentity);
    }
    Ok(*uuid.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixed_config() -> &'static str {
        STAGE8B_P1E_SUPERVISOR_CONFIG_PATH
    }

    #[test]
    fn cli_accepts_only_fixed_deployment_grammar() {
        assert_eq!(
            parse_stage8b_p1e_process_command_v1(["validate-config", fixed_config()]).unwrap(),
            Stage8bP1eProcessCommandV1::ValidateConfig
        );
        assert_eq!(
            parse_stage8b_p1e_process_command_v1([
                "bootstrap",
                fixed_config(),
                crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
            ])
            .unwrap(),
            Stage8bP1eProcessCommandV1::Bootstrap
        );
        assert!(parse_stage8b_p1e_process_command_v1(["run", "/tmp/supervisor.json"]).is_err());
        assert!(
            parse_stage8b_p1e_process_command_v1(["bootstrap", fixed_config(), "YES",]).is_err()
        );
    }

    #[test]
    fn recovery_selector_is_exact_and_complete() {
        let transaction = "a".repeat(64);
        let actions = [
            "remove-marker-temp",
            "resume-prepared",
            "quarantine-root",
            "finalize-quarantine",
            "complete-prepared-to-root-published",
            "complete-root-published-to-journal-durable",
            "complete-journal-durable-to-seal-committed",
            "remove-receipt-temp-and-adopt",
            "adopt-committed-root",
            "start-seal-committed-to-adopted",
            "complete-seal-committed-to-adopted",
        ];
        for action in actions {
            let selector = format!("{transaction}.{action}");
            assert!(parse_stage8b_p1e_process_command_v1([
                "bootstrap-recover",
                fixed_config(),
                selector.as_str(),
                STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION,
            ])
            .is_ok());
        }
        for selector in [
            format!("{}.resume-prepared", "A".repeat(64)),
            format!("{transaction}.unknown"),
            format!("{transaction}.resume-prepared.extra"),
        ] {
            assert!(parse_stage8b_p1e_process_command_v1([
                "bootstrap-recover",
                fixed_config(),
                selector.as_str(),
                STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION,
            ])
            .is_err());
        }
    }

    #[test]
    fn boot_id_parser_rejects_noncanonical_text() {
        let directory =
            std::env::temp_dir().join(format!("stage8b-p1e-boot-id-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("boot_id");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"01234567-89ab-cdef-0123-456789abcdef\n")
            .unwrap();
        assert_eq!(
            read_boot_id(&path).unwrap(),
            *uuid::Uuid::parse_str("01234567-89ab-cdef-0123-456789abcdef")
                .unwrap()
                .as_bytes()
        );
        fs::write(&path, b"01234567-89AB-cdef-0123-456789abcdef\n").unwrap();
        assert!(read_boot_id(&path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn protected_config_rejects_symlink_and_writable_file() {
        let directory = std::env::temp_dir().join(format!(
            "stage8b-p1e-config-boundary-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("supervisor.json");
        fs::write(&path, b"{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let uid = fs::metadata(&path).unwrap().uid();
        assert_eq!(read_protected_config(&path, uid).unwrap(), b"{}");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o620)).unwrap();
        assert!(read_protected_config(&path, uid).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let link = directory.join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_protected_config(&link, uid).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
