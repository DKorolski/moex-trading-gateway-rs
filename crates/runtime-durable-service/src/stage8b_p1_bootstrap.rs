//! Stage 8B-P1-a bootstrap and operational-identity composition.
//!
//! This module deliberately owns no Redis connection, FINAM transport,
//! command publication or paper provider.  It only binds one source-produced
//! Stage 5G TimerReady seed to the accepted Stage 7B durable owner.

use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use broker_core::{BrokerAccountId, Exchange, InstrumentId, Market};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use strategy_runtime_core::{
    authorize_stage6d_first_boot, export_stage5g_clean_restart, restore_stage5g_clean_restart,
    stage6d_operational_identity_sha256, validate_stage5g_timer_checkpoint,
    HybridIntradayRuntimeStrategy, Stage5gCleanRestartExportInput, Stage5gCleanRestartSource,
    Stage5gLifecycleCommitmentKey, Stage5gTimerReadyPaperStrategy, Stage6dFirstBootConfig,
    Stage6dLiveCoreError, Stage6dOperationalIdentityConfig,
};

use crate::{
    Stage7bDurableRootAuthority, Stage7bDurableStorageError, Stage7bRecoveryError,
    Stage7bRecoveryReadyOwner, Stage7bRestartOutcome,
};

pub const STAGE8B_P1_BOOTSTRAP_CONFIG_SCHEMA_VERSION: u16 = 1;
pub const STAGE8B_P1_BROKER_ID: &str = "finam-paper";
pub const STAGE8B_P1_STRATEGY_ID: &str = "hybrid_imoexf";
pub const STAGE8B_P1_INTERNAL_SYMBOL: &str = "IMOEXF";
pub const STAGE8B_P1_VENUE_SYMBOL: &str = "IMOEXF@RTSX";
pub const STAGE8B_P1_EXCHANGE: &str = "moex";
pub const STAGE8B_P1_MARKET: &str = "futures";
pub const STAGE8B_P1_TICK_SIZE: &str = "0.5";
pub const STAGE8B_P1_REDIS_HASH_TAG: &str = "finam-imoexf-p1";
pub const STAGE8B_P1_M10_CONSUMER_GROUP: &str = "finam-imoexf-p1-m10-lifecycle-v1";
pub const STAGE8B_P1_STAGE7B_CONSUMER_GROUP: &str = "stage7b-paper-command-consumer-v1";
pub const STAGE8B_P1_COMMITMENT_CREDENTIAL_FILE: &str = "stage8b-p1-lifecycle.key";
pub const STAGE8B_P1_FIRST_BOOT_CONFIRMATION: &str = "CREATE_NEW_STAGE8B_P1_DURABLE_ROOT";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1BootstrapConfig {
    pub schema_version: u16,
    pub broker_id: String,
    pub strategy_id: String,
    pub account_id: String,
    pub internal_symbol: String,
    pub venue_symbol: String,
    pub exchange: String,
    pub market: String,
    pub tick_size: String,
    pub runtime_config_fingerprint_sha256: String,
    pub instrument_map_fingerprint_sha256: String,
    pub deployment_id: String,
    pub deployment_generation: u64,
    pub gateway_instance_id: String,
    pub market_data_generation: u64,
    pub command_consumer_generation: u64,
    pub stage8a4_writer_issuer_public_key_hex: String,
    pub durable_parent: PathBuf,
}

/// Validated config authority. It is intentionally non-serializable and
/// non-cloneable so validation cannot be bypassed by deserializing this type.
pub struct Stage8bP1ValidatedBootstrapConfig {
    account_id: BrokerAccountId,
    instrument: InstrumentId,
    runtime_config_fingerprint_sha256: String,
    operational_identity: Stage6dOperationalIdentityConfig,
    operational_identity_sha256: String,
    durable_parent: PathBuf,
    expected_root_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8bP1RedisNamespace {
    pub hash_tag: String,
    pub canonical_m10_stream: String,
    pub canonical_command_stream: String,
    pub canonical_ack_stream: String,
    pub canonical_dlq_stream: String,
    pub settlement_key_prefix: String,
    pub canonical_order_stream: String,
    pub canonical_trade_stream: String,
    pub canonical_position_stream: String,
    pub runtime_state_stream: String,
    pub health_stream: String,
    pub readiness_stream: String,
    pub m10_consumer_group: String,
    pub stage7b_command_consumer_group: String,
    pub durable_consumer_activation_authorized: bool,
    pub m10_publication_authorized: bool,
}

/// Linear proof of the exact explicit administrative first-boot decision.
/// It deliberately implements none of Clone, Copy, Debug or serde traits.
pub struct Stage8bP1FirstBootAdminCommand {
    operational_identity_sha256: String,
    runtime_config_fingerprint_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8bP1BootstrapReceipt {
    pub schema_version: u16,
    pub boot_mode: &'static str,
    pub operational_identity_sha256: String,
    pub account_id_sha256: String,
    pub runtime_config_fingerprint_sha256: String,
    pub instrument_map_fingerprint_sha256: String,
    pub durable_root_name: String,
    pub recovery_seal_generation: u64,
    pub paper_only: bool,
    pub redis_consumer_attached: bool,
    pub finam_transport_attached: bool,
    pub broker_network_dispatch_attached: bool,
    pub runtime_live: bool,
    pub real_orders: bool,
}

pub struct Stage8bP1FirstBootOutcome {
    owner: Stage7bRecoveryReadyOwner,
    receipt: Stage8bP1BootstrapReceipt,
}

impl Stage8bP1FirstBootOutcome {
    pub fn owner(&self) -> &Stage7bRecoveryReadyOwner {
        &self.owner
    }

    pub fn receipt(&self) -> &Stage8bP1BootstrapReceipt {
        &self.receipt
    }

    pub fn into_owner(self) -> Stage7bRecoveryReadyOwner {
        self.owner
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1BootstrapError {
    #[error("Stage 8B-P1 bootstrap config is invalid")]
    InvalidConfig,
    #[error("Stage 8B-P1 durable parent is unsafe or noncanonical")]
    UnsafeDurableParent,
    #[error("Stage 8B-P1 first boot was not explicitly authorized")]
    FirstBootNotAuthorized,
    #[error("Stage 8B-P1 source does not match the validated deployment identity")]
    SourceIdentityMismatch,
    #[error("Stage 8B-P1 source is not the initial zero-intent TimerReady authority")]
    InvalidInitialSource,
    #[error("Stage 8B-P1 fresh runtime config does not match the validated deployment")]
    RuntimeConfigMismatch,
    #[error("Stage 8B-P1 durable root already exists")]
    DurableRootAlreadyExists,
    #[error("Stage 8B-P1 durable root is missing")]
    DurableRootMissing,
    #[error("Stage 8B-P1 durable root creation failed: {0:?}")]
    DurableRootCreate(ErrorKind),
    #[error("Stage 8B-P1 commitment credential boundary is invalid")]
    InvalidCommitmentCredential,
    #[error("Stage 8B-P1 Stage 5G source export or validation failed")]
    Stage5g,
    #[error("Stage 8B-P1 Stage 6 first-boot authorization failed")]
    Stage6,
    #[error("Stage 8B-P1 Stage 7 durable-root validation failed")]
    Stage7Storage,
    #[error("Stage 8B-P1 Stage 7 recovery failed")]
    Stage7Recovery,
}

#[derive(Serialize)]
struct Stage8bP1InstrumentMapProjection<'a> {
    schema_version: u16,
    domain: &'static str,
    broker_id: &'a str,
    internal_symbol: &'a str,
    venue_symbol: &'a str,
    exchange: &'a str,
    market: &'a str,
    tick_size: &'a str,
}

pub fn stage8b_p1_imoexf_instrument_map_fingerprint_sha256() -> String {
    let projection = Stage8bP1InstrumentMapProjection {
        schema_version: 1,
        domain: "moex.stage8b.p1.instrument-map.v1",
        broker_id: STAGE8B_P1_BROKER_ID,
        internal_symbol: STAGE8B_P1_INTERNAL_SYMBOL,
        venue_symbol: STAGE8B_P1_VENUE_SYMBOL,
        exchange: STAGE8B_P1_EXCHANGE,
        market: STAGE8B_P1_MARKET,
        tick_size: STAGE8B_P1_TICK_SIZE,
    };
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage8b.p1.instrument-map.v1\0");
    hasher.update(
        serde_json::to_vec(&projection)
            .expect("typed Stage 8B-P1 instrument map remains serializable"),
    );
    format!("{:x}", hasher.finalize())
}

pub fn stage8b_p1_redis_namespace() -> Stage8bP1RedisNamespace {
    let prefix = format!("finam_imoexf_paper:{{{STAGE8B_P1_REDIS_HASH_TAG}}}");
    Stage8bP1RedisNamespace {
        hash_tag: STAGE8B_P1_REDIS_HASH_TAG.to_string(),
        canonical_m10_stream: format!("{prefix}:market-data:m10"),
        canonical_command_stream: format!("{prefix}:stage7b:commands"),
        canonical_ack_stream: format!("{prefix}:stage7b:acks"),
        canonical_dlq_stream: format!("{prefix}:stage7b:dlq"),
        settlement_key_prefix: format!("{prefix}:stage7b:settlement"),
        canonical_order_stream: format!("{prefix}:broker:orders"),
        canonical_trade_stream: format!("{prefix}:broker:trades"),
        canonical_position_stream: format!("{prefix}:broker:positions"),
        runtime_state_stream: format!("{prefix}:runtime:state"),
        health_stream: format!("{prefix}:health"),
        readiness_stream: format!("{prefix}:readiness"),
        m10_consumer_group: STAGE8B_P1_M10_CONSUMER_GROUP.to_string(),
        stage7b_command_consumer_group: STAGE8B_P1_STAGE7B_CONSUMER_GROUP.to_string(),
        durable_consumer_activation_authorized: false,
        m10_publication_authorized: false,
    }
}

pub fn validate_stage8b_p1_bootstrap_config(
    config: Stage8bP1BootstrapConfig,
) -> Result<Stage8bP1ValidatedBootstrapConfig, Stage8bP1BootstrapError> {
    if config.schema_version != STAGE8B_P1_BOOTSTRAP_CONFIG_SCHEMA_VERSION
        || config.broker_id != STAGE8B_P1_BROKER_ID
        || config.strategy_id != STAGE8B_P1_STRATEGY_ID
        || config.internal_symbol != STAGE8B_P1_INTERNAL_SYMBOL
        || config.venue_symbol != STAGE8B_P1_VENUE_SYMBOL
        || config.exchange != STAGE8B_P1_EXCHANGE
        || config.market != STAGE8B_P1_MARKET
        || config.tick_size != STAGE8B_P1_TICK_SIZE
        || !canonical_token(&config.account_id)
        || !is_sha256_hex(&config.runtime_config_fingerprint_sha256)
        || config.instrument_map_fingerprint_sha256
            != stage8b_p1_imoexf_instrument_map_fingerprint_sha256()
    {
        return Err(Stage8bP1BootstrapError::InvalidConfig);
    }
    let durable_parent = validate_durable_parent(&config.durable_parent)?;
    let operational_identity = Stage6dOperationalIdentityConfig {
        broker_id: config.broker_id,
        strategy_instance_id: config.strategy_id,
        deployment_id: config.deployment_id,
        deployment_generation: config.deployment_generation,
        gateway_instance_id: config.gateway_instance_id,
        instrument_map_fingerprint_sha256: config.instrument_map_fingerprint_sha256,
        market_data_generation: config.market_data_generation,
        command_consumer_generation: config.command_consumer_generation,
        stage8a4_writer_issuer_public_key_hex: config.stage8a4_writer_issuer_public_key_hex,
    };
    let operational_identity_sha256 = stage6d_operational_identity_sha256(&operational_identity)
        .map_err(|_| Stage8bP1BootstrapError::InvalidConfig)?
        .as_str()
        .to_string();
    let expected_root_name =
        Stage7bDurableRootAuthority::expected_directory_name(&operational_identity)
            .map_err(|_| Stage8bP1BootstrapError::InvalidConfig)?;
    Ok(Stage8bP1ValidatedBootstrapConfig {
        account_id: BrokerAccountId::new(config.account_id),
        instrument: InstrumentId {
            symbol: STAGE8B_P1_INTERNAL_SYMBOL.to_string(),
            venue_symbol: Some(STAGE8B_P1_VENUE_SYMBOL.to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        },
        runtime_config_fingerprint_sha256: config.runtime_config_fingerprint_sha256,
        operational_identity,
        operational_identity_sha256,
        durable_parent,
        expected_root_name,
    })
}

pub fn authorize_stage8b_p1_first_boot(
    config: &Stage8bP1ValidatedBootstrapConfig,
    exact_confirmation: &str,
) -> Result<Stage8bP1FirstBootAdminCommand, Stage8bP1BootstrapError> {
    if exact_confirmation != STAGE8B_P1_FIRST_BOOT_CONFIRMATION {
        return Err(Stage8bP1BootstrapError::FirstBootNotAuthorized);
    }
    Ok(Stage8bP1FirstBootAdminCommand {
        operational_identity_sha256: config.operational_identity_sha256.clone(),
        runtime_config_fingerprint_sha256: config.runtime_config_fingerprint_sha256.clone(),
    })
}

pub fn load_stage8b_p1_commitment_key_from_systemd_credential(
) -> Result<Stage5gLifecycleCommitmentKey, Stage8bP1BootstrapError> {
    let directory = std::env::var_os("CREDENTIALS_DIRECTORY")
        .ok_or(Stage8bP1BootstrapError::InvalidCommitmentCredential)?;
    load_stage8b_p1_commitment_key_from_directory(Path::new(&directory))
}

fn load_stage8b_p1_commitment_key_from_directory(
    directory: &Path,
) -> Result<Stage5gLifecycleCommitmentKey, Stage8bP1BootstrapError> {
    if !directory.is_absolute()
        || fs::canonicalize(directory).ok().as_deref() != Some(directory)
        || fs::symlink_metadata(directory)
            .map(|metadata| metadata.file_type().is_symlink() || !metadata.is_dir())
            .unwrap_or(true)
    {
        return Err(Stage8bP1BootstrapError::InvalidCommitmentCredential);
    }
    let path = directory.join(STAGE8B_P1_COMMITMENT_CREDENTIAL_FILE);
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| Stage8bP1BootstrapError::InvalidCommitmentCredential)?;
    let metadata = file
        .metadata()
        .map_err(|_| Stage8bP1BootstrapError::InvalidCommitmentCredential)?;
    let effective_uid = unsafe { libc::geteuid() };
    if !metadata.file_type().is_file()
        || metadata.nlink() != 1
        || metadata.len() != 32
        || metadata.permissions().mode() & 0o077 != 0
        || (metadata.uid() != 0 && metadata.uid() != effective_uid)
    {
        return Err(Stage8bP1BootstrapError::InvalidCommitmentCredential);
    }
    let mut secret = [0_u8; 32];
    if file.read_exact(&mut secret).is_err() {
        secret.fill(0);
        return Err(Stage8bP1BootstrapError::InvalidCommitmentCredential);
    }
    let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&secret);
    secret.fill(0);
    key.map_err(|_| Stage8bP1BootstrapError::InvalidCommitmentCredential)
}

pub fn first_boot_stage8b_p1(
    config: Stage8bP1ValidatedBootstrapConfig,
    admin: Stage8bP1FirstBootAdminCommand,
    source: Stage5gTimerReadyPaperStrategy,
    export_input: Stage5gCleanRestartExportInput,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: HybridIntradayRuntimeStrategy,
) -> Result<Stage8bP1FirstBootOutcome, Stage8bP1BootstrapError> {
    if admin.operational_identity_sha256 != config.operational_identity_sha256
        || admin.runtime_config_fingerprint_sha256 != config.runtime_config_fingerprint_sha256
    {
        return Err(Stage8bP1BootstrapError::FirstBootNotAuthorized);
    }
    validate_initial_source(&config, &source, &fresh_runtime)?;

    let stage5g_seed = export_stage5g_clean_restart(
        Stage5gCleanRestartSource::P1BootstrapReady(source),
        export_input,
        commitment_key,
    )
    .map_err(|_| Stage8bP1BootstrapError::Stage5g)?;
    // Complete decode, HMAC, binding and reconstruction before creating a
    // directory. Invalid source bytes cannot leave an empty first-boot root.
    drop(
        restore_stage5g_clean_restart(&stage5g_seed, commitment_key, fresh_runtime.clone())
            .map_err(|_| Stage8bP1BootstrapError::Stage5g)?,
    );

    let root_path = config.durable_parent.join(&config.expected_root_name);
    if fs::symlink_metadata(&root_path).is_ok() {
        return Err(Stage8bP1BootstrapError::DurableRootAlreadyExists);
    }
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    builder
        .create(&root_path)
        .map_err(|error| Stage8bP1BootstrapError::DurableRootCreate(error.kind()))?;
    File::open(&config.durable_parent)
        .and_then(|parent| parent.sync_all())
        .map_err(|error| Stage8bP1BootstrapError::DurableRootCreate(error.kind()))?;

    let root = Stage7bDurableRootAuthority::validate(&root_path, &config.operational_identity)
        .map_err(|_| Stage8bP1BootstrapError::Stage7Storage)?;
    let authorization = authorize_stage6d_first_boot(Stage6dFirstBootConfig {
        deployment_id: config.operational_identity.deployment_id.clone(),
        expected_runtime_config_fingerprint_sha256: config
            .runtime_config_fingerprint_sha256
            .clone(),
        allow_create_missing_journal: true,
    })
    .map_err(|_| Stage8bP1BootstrapError::Stage6)?;
    let owner = Stage7bRecoveryReadyOwner::first_boot(
        root,
        config.operational_identity,
        authorization,
        &stage5g_seed,
        commitment_key,
        fresh_runtime,
    )
    .map_err(|_| Stage8bP1BootstrapError::Stage7Recovery)?;
    if !owner.stage8b_p1_source_binding_matches(
        STAGE8B_P1_STRATEGY_ID,
        &config.account_id,
        &config.instrument,
        &config.runtime_config_fingerprint_sha256,
    ) {
        return Err(Stage8bP1BootstrapError::SourceIdentityMismatch);
    }
    let seal_generation = owner
        .committed_seal()
        .map_err(|_| Stage8bP1BootstrapError::Stage7Recovery)?
        .seal_generation();
    let receipt = Stage8bP1BootstrapReceipt {
        schema_version: 1,
        boot_mode: "first_boot",
        operational_identity_sha256: config.operational_identity_sha256,
        account_id_sha256: sha256_hex(config.account_id.as_str().as_bytes()),
        runtime_config_fingerprint_sha256: config.runtime_config_fingerprint_sha256,
        instrument_map_fingerprint_sha256: stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
        durable_root_name: config.expected_root_name,
        recovery_seal_generation: seal_generation,
        paper_only: true,
        redis_consumer_attached: false,
        finam_transport_attached: false,
        broker_network_dispatch_attached: false,
        runtime_live: false,
        real_orders: false,
    };
    Ok(Stage8bP1FirstBootOutcome { owner, receipt })
}

pub fn restart_stage8b_p1(
    config: Stage8bP1ValidatedBootstrapConfig,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    fresh_runtime: HybridIntradayRuntimeStrategy,
) -> Result<Stage7bRestartOutcome, Stage8bP1BootstrapError> {
    if fresh_runtime.stage5c_config_fingerprint() != config.runtime_config_fingerprint_sha256 {
        return Err(Stage8bP1BootstrapError::RuntimeConfigMismatch);
    }
    let root_path = config.durable_parent.join(&config.expected_root_name);
    if !root_path.exists() {
        return Err(Stage8bP1BootstrapError::DurableRootMissing);
    }
    let root = Stage7bDurableRootAuthority::validate(&root_path, &config.operational_identity)
        .map_err(|_| Stage8bP1BootstrapError::Stage7Storage)?;
    let outcome = Stage7bRecoveryReadyOwner::restart(
        root,
        config.operational_identity,
        commitment_key,
        fresh_runtime,
    )
    .map_err(|_| Stage8bP1BootstrapError::Stage7Recovery)?;
    // A blocked outcome deliberately carries no reusable runtime authority,
    // so it cannot prove the positive source binding. Preserve that explicit
    // fail-closed diagnostic instead of obscuring it as a source mismatch.
    if !matches!(outcome, Stage7bRestartOutcome::Blocked(_))
        && !outcome.stage8b_p1_source_binding_matches(
            STAGE8B_P1_STRATEGY_ID,
            &config.account_id,
            &config.instrument,
            &config.runtime_config_fingerprint_sha256,
        )
    {
        return Err(Stage8bP1BootstrapError::SourceIdentityMismatch);
    }
    Ok(outcome)
}

fn validate_initial_source(
    config: &Stage8bP1ValidatedBootstrapConfig,
    source: &Stage5gTimerReadyPaperStrategy,
    fresh_runtime: &HybridIntradayRuntimeStrategy,
) -> Result<(), Stage8bP1BootstrapError> {
    if source.strategy_id() != STAGE8B_P1_STRATEGY_ID
        || source.account_id() != &config.account_id
        || source.instrument() != &config.instrument
        || source.runtime_config_fingerprint_sha256() != config.runtime_config_fingerprint_sha256
    {
        return Err(Stage8bP1BootstrapError::SourceIdentityMismatch);
    }
    if fresh_runtime.stage5c_config_fingerprint() != config.runtime_config_fingerprint_sha256 {
        return Err(Stage8bP1BootstrapError::RuntimeConfigMismatch);
    }
    let checkpoint = source.checkpoint();
    if validate_stage5g_timer_checkpoint(&checkpoint).is_err()
        || source.summary().request_count != 0
        || source.summary().terminal_request_count != 0
        || source.summary().order_transition_count != 0
        || source.summary().correlated_trade_count != 0
        || source.summary().position_confirmation_count != 0
        || source.summary().duplicate_evidence_count != 0
        || source.summary().last_total_sequence.is_some()
        || source.summary().stage5c_callback_count != 1
        || !is_sha256_hex(&source.summary().lifecycle_fingerprint_sha256)
        || !source.summary().mock_feedback_only
        || source.summary().redis_attached
        || source.summary().finam_transport_attached
        || source.summary().broker_execution_attached
        || !checkpoint.payload.evidence_replay_ledger.is_empty()
        || checkpoint.payload.package_discriminator.is_some()
        || checkpoint.payload.current_evidence_identity.is_some()
        || checkpoint.payload.last_broker_truth_received_at.is_some()
        || checkpoint.payload.last_broker_truth_received_ms.is_some()
        || checkpoint.payload.duplicate_evidence_count != 0
        || checkpoint.payload.last_total_sequence.is_some()
        || checkpoint.payload.last_continuation_checkpoint_ts_utc_ms
            != Some(source.checkpoint_ts_utc_ms())
    {
        return Err(Stage8bP1BootstrapError::InvalidInitialSource);
    }
    Ok(())
}

fn validate_durable_parent(path: &Path) -> Result<PathBuf, Stage8bP1BootstrapError> {
    if !path.is_absolute() {
        return Err(Stage8bP1BootstrapError::UnsafeDurableParent);
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|_| Stage8bP1BootstrapError::UnsafeDurableParent)?;
    let canonical =
        fs::canonicalize(path).map_err(|_| Stage8bP1BootstrapError::UnsafeDurableParent)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || canonical != path
        || metadata.permissions().mode() & 0o022 != 0
    {
        return Err(Stage8bP1BootstrapError::UnsafeDurableParent);
    }
    Ok(canonical)
}

fn canonical_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl From<Stage6dLiveCoreError> for Stage8bP1BootstrapError {
    fn from(_: Stage6dLiveCoreError) -> Self {
        Self::Stage6
    }
}

impl From<Stage7bDurableStorageError> for Stage8bP1BootstrapError {
    fn from(_: Stage7bDurableStorageError) -> Self {
        Self::Stage7Storage
    }
}

impl From<Stage7bRecoveryError> for Stage8bP1BootstrapError {
    fn from(_: Stage7bRecoveryError) -> Self {
        Self::Stage7Recovery
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn temp_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stage8b-p1-{label}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder.create(&path).unwrap();
        fs::canonicalize(path).unwrap()
    }

    fn config(parent: PathBuf) -> Stage8bP1BootstrapConfig {
        Stage8bP1BootstrapConfig {
            schema_version: STAGE8B_P1_BOOTSTRAP_CONFIG_SCHEMA_VERSION,
            broker_id: STAGE8B_P1_BROKER_ID.to_string(),
            strategy_id: STAGE8B_P1_STRATEGY_ID.to_string(),
            account_id: "ACC_TEST_0001".to_string(),
            internal_symbol: STAGE8B_P1_INTERNAL_SYMBOL.to_string(),
            venue_symbol: STAGE8B_P1_VENUE_SYMBOL.to_string(),
            exchange: STAGE8B_P1_EXCHANGE.to_string(),
            market: STAGE8B_P1_MARKET.to_string(),
            tick_size: STAGE8B_P1_TICK_SIZE.to_string(),
            runtime_config_fingerprint_sha256: "11".repeat(32),
            instrument_map_fingerprint_sha256: stage8b_p1_imoexf_instrument_map_fingerprint_sha256(
            ),
            deployment_id: "finam-imoexf-paper-p1".to_string(),
            deployment_generation: 1,
            gateway_instance_id: "finam-imoexf-paper-gateway-1".to_string(),
            market_data_generation: 1,
            command_consumer_generation: 1,
            stage8a4_writer_issuer_public_key_hex: "22".repeat(32),
            durable_parent: parent,
        }
    }

    #[test]
    fn exact_identity_and_namespace_validate_without_activation() {
        let parent = temp_directory("identity");
        let validated = validate_stage8b_p1_bootstrap_config(config(parent.clone())).unwrap();
        assert_eq!(
            validated.expected_root_name,
            format!("stage7b-{}", validated.operational_identity_sha256)
        );
        assert_eq!(
            stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            "ba4e7b1190dc2686559b6d7c0df0185e96a10dfac6b13f6a582a349b14198558"
        );
        assert!(
            authorize_stage8b_p1_first_boot(&validated, STAGE8B_P1_FIRST_BOOT_CONFIRMATION).is_ok()
        );
        let namespace = stage8b_p1_redis_namespace();
        let tag = format!("{{{STAGE8B_P1_REDIS_HASH_TAG}}}");
        for stream in [
            &namespace.canonical_m10_stream,
            &namespace.canonical_command_stream,
            &namespace.canonical_ack_stream,
            &namespace.canonical_dlq_stream,
            &namespace.settlement_key_prefix,
            &namespace.canonical_order_stream,
            &namespace.canonical_trade_stream,
            &namespace.canonical_position_stream,
            &namespace.runtime_state_stream,
            &namespace.health_stream,
            &namespace.readiness_stream,
        ] {
            assert!(stream.contains(&tag));
        }
        assert!(!namespace.durable_consumer_activation_authorized);
        assert!(!namespace.m10_publication_authorized);
        assert_ne!(
            namespace.canonical_m10_stream,
            "finam_imoexf_paper:ws:market_data"
        );
        fs::remove_dir(parent).unwrap();
    }

    #[test]
    fn raw_config_decode_is_strict() {
        let parent = temp_directory("strict-config");
        let raw = format!(
            r#"{{
                "schema_version": 1,
                "broker_id": "finam-paper",
                "strategy_id": "hybrid_imoexf",
                "account_id": "ACC_TEST_0001",
                "internal_symbol": "IMOEXF",
                "venue_symbol": "IMOEXF@RTSX",
                "exchange": "moex",
                "market": "futures",
                "tick_size": "0.5",
                "runtime_config_fingerprint_sha256": "{}",
                "instrument_map_fingerprint_sha256": "{}",
                "deployment_id": "finam-imoexf-paper-p1",
                "deployment_generation": 1,
                "gateway_instance_id": "finam-imoexf-paper-gateway-1",
                "market_data_generation": 1,
                "command_consumer_generation": 1,
                "stage8a4_writer_issuer_public_key_hex": "{}",
                "durable_parent": {},
                "unexpected": true
            }}"#,
            "11".repeat(32),
            stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            "22".repeat(32),
            serde_json::to_string(&parent).unwrap()
        );
        assert!(serde_json::from_str::<Stage8bP1BootstrapConfig>(&raw).is_err());
        fs::remove_dir(parent).unwrap();
    }

    #[test]
    fn wrong_identity_and_implicit_first_boot_fail_closed() {
        let parent = temp_directory("wrong-identity");
        let mut wrong = config(parent.clone());
        wrong.venue_symbol = "RTS-9.26@RTSX".to_string();
        assert!(matches!(
            validate_stage8b_p1_bootstrap_config(wrong),
            Err(Stage8bP1BootstrapError::InvalidConfig)
        ));
        let validated = validate_stage8b_p1_bootstrap_config(config(parent.clone())).unwrap();
        assert!(matches!(
            authorize_stage8b_p1_first_boot(&validated, "yes"),
            Err(Stage8bP1BootstrapError::FirstBootNotAuthorized)
        ));
        fs::remove_dir(parent).unwrap();
    }

    #[test]
    fn commitment_credential_is_exact_private_regular_file() {
        let directory = temp_directory("credential");
        let key_path = directory.join(STAGE8B_P1_COMMITMENT_CREDENTIAL_FILE);
        fs::write(&key_path, [0x8b_u8; 32]).unwrap();
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600)).unwrap();
        let key = load_stage8b_p1_commitment_key_from_directory(&directory).unwrap();
        drop(key);

        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            load_stage8b_p1_commitment_key_from_directory(&directory),
            Err(Stage8bP1BootstrapError::InvalidCommitmentCredential)
        ));
        fs::remove_file(&key_path).unwrap();
        let outside = directory.join("outside");
        fs::write(&outside, [0x8b_u8; 32]).unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&outside, &key_path).unwrap();
        assert!(matches!(
            load_stage8b_p1_commitment_key_from_directory(&directory),
            Err(Stage8bP1BootstrapError::InvalidCommitmentCredential)
        ));
        fs::remove_file(key_path).unwrap();
        fs::remove_file(outside).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn restart_never_creates_a_missing_root() {
        let parent = temp_directory("restart-missing");
        let (_seed, key, fresh) =
            strategy_runtime_core::stage6d_test_authenticated_restart_fixture();
        let mut raw = config(parent.clone());
        raw.runtime_config_fingerprint_sha256 = fresh.stage5c_config_fingerprint();
        let validated = validate_stage8b_p1_bootstrap_config(raw).unwrap();
        assert!(matches!(
            restart_stage8b_p1(validated, &key, fresh),
            Err(Stage8bP1BootstrapError::DurableRootMissing)
        ));
        assert_eq!(fs::read_dir(&parent).unwrap().count(), 0);
        fs::remove_dir(parent).unwrap();
    }

    #[test]
    fn source_produced_first_boot_and_restart_preserve_one_durable_owner() {
        let parent = temp_directory("first-boot");
        let (source, export_input, key, fresh) =
            strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let runtime_fingerprint = fresh.stage5c_config_fingerprint();
        let mut raw = config(parent.clone());
        raw.runtime_config_fingerprint_sha256 = runtime_fingerprint.clone();
        let validated = validate_stage8b_p1_bootstrap_config(raw).unwrap();
        let root_name = validated.expected_root_name.clone();
        let admin = authorize_stage8b_p1_first_boot(&validated, STAGE8B_P1_FIRST_BOOT_CONFIRMATION)
            .unwrap();
        let outcome =
            first_boot_stage8b_p1(validated, admin, source, export_input, &key, fresh.clone())
                .expect("source-produced first boot succeeds");
        assert!(outcome.owner().recovery_ready());
        assert_eq!(outcome.receipt().boot_mode, "first_boot");
        assert_eq!(
            outcome.receipt().runtime_config_fingerprint_sha256,
            runtime_fingerprint
        );
        assert!(!outcome.receipt().redis_consumer_attached);
        assert!(!outcome.receipt().finam_transport_attached);
        assert!(!outcome.receipt().broker_network_dispatch_attached);
        assert!(!outcome.receipt().runtime_live);
        assert!(!outcome.receipt().real_orders);
        drop(outcome);

        let mut wrong_account = config(parent.clone());
        wrong_account.account_id = "OTHER_ACCOUNT".to_string();
        wrong_account.runtime_config_fingerprint_sha256 = fresh.stage5c_config_fingerprint();
        let wrong_restart = validate_stage8b_p1_bootstrap_config(wrong_account).unwrap();
        assert!(matches!(
            restart_stage8b_p1(wrong_restart, &key, fresh.clone()),
            Err(Stage8bP1BootstrapError::SourceIdentityMismatch)
        ));

        let mut restart_raw = config(parent.clone());
        restart_raw.runtime_config_fingerprint_sha256 = fresh.stage5c_config_fingerprint();
        let restart_config = validate_stage8b_p1_bootstrap_config(restart_raw).unwrap();
        let restarted = restart_stage8b_p1(restart_config, &key, fresh)
            .expect("normal service restart opens only the existing exact root");
        assert!(restarted.recovery_ready());
        drop(restarted);

        assert!(parent.join(root_name).is_dir());
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn source_identity_mismatch_precedes_durable_root_creation() {
        let parent = temp_directory("source-mismatch");
        let (source, export_input, key, fresh) =
            strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let mut raw = config(parent.clone());
        raw.account_id = "OTHER_ACCOUNT".to_string();
        raw.runtime_config_fingerprint_sha256 = fresh.stage5c_config_fingerprint();
        let validated = validate_stage8b_p1_bootstrap_config(raw).unwrap();
        let admin = authorize_stage8b_p1_first_boot(&validated, STAGE8B_P1_FIRST_BOOT_CONFIRMATION)
            .unwrap();
        assert!(matches!(
            first_boot_stage8b_p1(validated, admin, source, export_input, &key, fresh),
            Err(Stage8bP1BootstrapError::SourceIdentityMismatch)
        ));
        assert_eq!(fs::read_dir(&parent).unwrap().count(), 0);
        fs::remove_dir(parent).unwrap();
    }
}
