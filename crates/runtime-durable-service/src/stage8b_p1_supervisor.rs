//! Stage 8B-P1-e deployable paper-supervisor contracts.
//!
//! This module is intentionally broker-network-free. It owns the exact
//! production Hybrid profile, strict non-secret process configuration and the
//! redacted coordinator/telemetry vocabulary used by the later process loop.

use std::{collections::BTreeSet, fmt, future::Future, time::Duration as StdDuration};

use chrono::{DateTime, Duration, NaiveTime, SecondsFormat, Utc};
use serde::{
    de::{MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use strategy_runtime_core::{
    hybrid_intraday::{
        BreakoutEodMode, HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
        MinRangeMode,
    },
    BrokerNeutralMarketOrderStyle, HybridIntradayProfile, HybridIntradayRuntimeConfig,
    HybridIntradayRuntimeStrategy, MeanReversionVariant, MrGatePolicy, RiskGateMode,
};

use crate::{
    attach_stage8b_p1_redis, stage8b_p1_imoexf_instrument_map_fingerprint_sha256,
    stage8b_p1_redis_namespace, validate_stage8b_p1_bootstrap_config, Stage7bRestartOutcome,
    Stage8bP1BootstrapConfig, Stage8bP1BootstrapError, Stage8bP1RedisConfig,
    Stage8bP1RedisNamespace, Stage8bP1RedisSemanticCompositionTransport,
    Stage8bP1ValidatedBootstrapConfig, Stage8bP1eShutdownCauseV1, Stage8bP1eShutdownIntentV1,
    Stage8bP1eShutdownLatchV1,
};

use redis::{
    aio::ConnectionManager,
    streams::{StreamInfoConsumersReply, StreamInfoGroupsReply, StreamPendingReply},
};

pub const STAGE8B_P1E_SUPERVISOR_CONFIG_SCHEMA_VERSION: u16 = 1;
pub const STAGE8B_P1E_RUNTIME_PROFILE_ID: &str = "imoexf-hybrid-high180-paper-v1";
pub const STAGE8B_P1E_RUNTIME_PROFILE_SHA256: &str =
    "dd5a211e708db0d40175d19ed1eeb51db26497d344a553b41d7afbfdddde0ef6";
pub const STAGE8B_P1E_REDIS_RUNTIME_POLICY_ID: &str = "imoexf-hybrid-paper-db15-runtime-v2";
pub const STAGE8B_P1E_REDIS_RUNTIME_POLICY_SHA256: &str =
    "c39decbea8af3f305da1e930f1220a54c46e060d972fbb9bbc0c599e8c40ab1f";
pub const STAGE8B_P1E_TELEMETRY_CONTRACT_SHA256: &str =
    "d2161a02e982a0e5e95b13596d6632a4368b8d74787c3376d5ffa0d11ef120f5";
pub const STAGE8B_P1E_REDIS_URL_IPV4: &str = "redis://127.0.0.1:6379/15";
pub const STAGE8B_P1E_REDIS_URL_IPV6: &str = "redis://[::1]:6379/15";
pub const STAGE8B_P1E_HEALTH_INTERVAL_MIN_MS: u64 = 1_000;
pub const STAGE8B_P1E_HEALTH_INTERVAL_MAX_MS: u64 = 60_000;
pub const STAGE8B_P1E_SHUTDOWN_GRACE_MIN_MS: u64 = 5_000;
pub const STAGE8B_P1E_SHUTDOWN_GRACE_MAX_MS: u64 = 90_000;
pub const STAGE8B_P1E_SYSTEMD_STOP_TIMEOUT_MS: u64 = 100_000;
pub const STAGE8B_P1E_TELEMETRY_RETENTION: usize = 4_096;
pub const STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS: u64 = 2_000;
pub const STAGE8B_P1E_STALE_CONSUMER_IDLE_MS: usize = 86_400_000;
pub const STAGE8B_P1E_STALE_CONSUMER_INVENTORY_MAX: usize = 64;
pub const STAGE8B_P1E_STALE_CONSUMER_EXAMINE_MAX: usize = 16;
pub const STAGE8B_P1E_DEPLOYMENT_MANIFEST_KEY: &str =
    "finam_imoexf_paper:{finam-imoexf-p1}:deployment-manifest";
pub const STAGE8B_P1E_SUPERVISOR_CONFIG_PATH: &str = "/etc/moex-finam-p1-paper/supervisor.json";
pub const STAGE8B_P1E_FIRST_BOOT_SOURCE_PATH: &str =
    "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json";
pub const STAGE8B_P1E_NAMESPACE_DIGEST_SHA256: &str =
    "18efd270fb03fa68f92b8288968f6cf16e8f529330c4335cb86cdaaba0b826ed";

const RUNTIME_PROFILE_BYTES: &[u8] =
    include_bytes!("../../../docs/stage-8/stage8b-p1e-runtime-profile-v1.json");
#[cfg(test)]
const REDIS_RUNTIME_POLICY_BYTES: &[u8] =
    include_bytes!("../../../docs/stage-8/stage8b-p1e-redis-runtime-policy-v2.json");

/// The discovery snapshot only bounds and orders work. Eligibility is checked
/// again in this Redis-atomic script immediately before DELCONSUMER, so a
/// delivery assigned after discovery can never be deleted with its consumer.
const ATOMIC_STALE_CONSUMER_DELETE_SCRIPT_V1: &str =
    include_str!("../../../docs/stage-8/stage8b-p1e-atomic-stale-consumer-delete-v1.lua");

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1eSupervisorConfigError {
    #[error("Stage 8B-P1-e supervisor JSON is invalid")]
    InvalidJson,
    #[error("Stage 8B-P1-e supervisor JSON contains duplicate object keys")]
    DuplicateJsonKey,
    #[error("Stage 8B-P1-e supervisor config is invalid")]
    InvalidConfig,
    #[error("Stage 8B-P1-e embedded runtime profile is invalid")]
    InvalidRuntimeProfile,
    #[error("Stage 8B-P1-e bootstrap config is invalid")]
    InvalidBootstrap,
}

impl From<Stage8bP1BootstrapError> for Stage8bP1eSupervisorConfigError {
    fn from(_: Stage8bP1BootstrapError) -> Self {
        Self::InvalidBootstrap
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1eSupervisorConfigV1 {
    pub schema_version: u16,
    pub runtime_profile_id: String,
    pub runtime_profile_sha256: String,
    pub first_boot_source_bundle_sha256: String,
    pub redis_url: String,
    pub redis_deployment_manifest_sha256: String,
    pub redis_runtime_policy_id: String,
    pub redis_runtime_policy_sha256: String,
    pub telemetry_contract_sha256: String,
    pub health_interval_ms: u64,
    pub shutdown_grace_ms: u64,
    pub bootstrap: Stage8bP1BootstrapConfig,
}

/// Validated, non-cloneable supervisor input. Secret material and a Redis
/// connection are deliberately absent.
pub struct Stage8bP1eValidatedSupervisorConfigV1 {
    bootstrap: Stage8bP1ValidatedBootstrapConfig,
    runtime: HybridIntradayRuntimeStrategy,
    runtime_config_fingerprint_sha256: String,
    redis_url: String,
    redis_deployment_manifest_sha256: String,
    first_boot_source_bundle_sha256: String,
    redis_config: Stage8bP1RedisConfig,
    namespace: Stage8bP1RedisNamespace,
    health_interval_ms: u64,
    shutdown_grace_ms: u64,
}

/// Non-secret, immutable S05 attachment plan retained after the linear
/// bootstrap authority and runtime have moved into authenticated restart.
/// It has no namespace initialization or repair capability.
pub struct Stage8bP1eRedisAttachPlanV1 {
    redis_url: String,
    redis_deployment_manifest_sha256: String,
    redis_config: Stage8bP1RedisConfig,
    namespace: Stage8bP1RedisNamespace,
    operational_identity_sha256: String,
    runtime_config_fingerprint_sha256: String,
    deployment_generation: u64,
    consumer_generation: u64,
}

/// Immutable non-secret timings retained by the process coordinator.
pub struct Stage8bP1eRunSettingsV1 {
    pub health_interval_ms: u64,
    pub shutdown_grace_ms: u64,
}

impl Stage8bP1eValidatedSupervisorConfigV1 {
    pub fn bootstrap(&self) -> &Stage8bP1ValidatedBootstrapConfig {
        &self.bootstrap
    }

    pub fn runtime(&self) -> &HybridIntradayRuntimeStrategy {
        &self.runtime
    }

    pub fn runtime_config_fingerprint_sha256(&self) -> &str {
        &self.runtime_config_fingerprint_sha256
    }

    pub fn redis_url(&self) -> &str {
        &self.redis_url
    }

    pub fn redis_deployment_manifest_sha256(&self) -> &str {
        &self.redis_deployment_manifest_sha256
    }

    pub fn first_boot_source_bundle_sha256(&self) -> &str {
        &self.first_boot_source_bundle_sha256
    }

    pub fn redis_config(&self) -> &Stage8bP1RedisConfig {
        &self.redis_config
    }

    pub fn namespace(&self) -> &Stage8bP1RedisNamespace {
        &self.namespace
    }

    pub const fn health_interval_ms(&self) -> u64 {
        self.health_interval_ms
    }

    pub const fn shutdown_grace_ms(&self) -> u64 {
        self.shutdown_grace_ms
    }

    pub fn into_run_parts(
        self,
    ) -> (
        Stage8bP1ValidatedBootstrapConfig,
        HybridIntradayRuntimeStrategy,
        Stage8bP1eRedisAttachPlanV1,
        Stage8bP1eRunSettingsV1,
    ) {
        let attach_plan = Stage8bP1eRedisAttachPlanV1 {
            redis_url: self.redis_url,
            redis_deployment_manifest_sha256: self.redis_deployment_manifest_sha256,
            redis_config: self.redis_config,
            namespace: self.namespace,
            operational_identity_sha256: self.bootstrap.operational_identity_sha256().to_string(),
            runtime_config_fingerprint_sha256: self.runtime_config_fingerprint_sha256,
            deployment_generation: self.bootstrap.deployment_generation(),
            consumer_generation: self.bootstrap.command_consumer_generation(),
        };
        let settings = Stage8bP1eRunSettingsV1 {
            health_interval_ms: self.health_interval_ms,
            shutdown_grace_ms: self.shutdown_grace_ms,
        };
        (self.bootstrap, self.runtime, attach_plan, settings)
    }
}

impl Stage8bP1eRedisAttachPlanV1 {
    pub fn consumer_name(&self) -> &str {
        &self.redis_config.consumer_name
    }

    pub fn namespace(&self) -> &Stage8bP1RedisNamespace {
        &self.namespace
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1eRedisControlError {
    #[error("Stage 8B-P1-e Redis operation failed")]
    Redis,
    #[error("Stage 8B-P1-e Redis operation timed out")]
    OperationTimeout,
    #[error("Stage 8B-P1-e deployment manifest is invalid")]
    ManifestMismatch,
    #[error("Stage 8B-P1-e Redis key type is invalid")]
    KeyTypeMismatch,
    #[error("Stage 8B-P1-e Redis group inventory is invalid")]
    GroupMismatch,
    #[error("Stage 8B-P1-e stale-consumer inventory is invalid")]
    ConsumerInventoryInvalid,
    #[error("Stage 8B-P1-e telemetry write failed")]
    TelemetryWriteFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage8bP1eRedisDeploymentManifestV1 {
    schema_version: u16,
    domain: String,
    manifest_key: String,
    redis_url_allowlist: Vec<String>,
    redis_db_index: u8,
    operational_identity_sha256: String,
    runtime_config_fingerprint_sha256: String,
    instrument_map_fingerprint_sha256: String,
    deployment_generation: u64,
    consumer_generation: u64,
    namespace_digest_sha256: String,
    keys: Vec<Stage8bP1eRedisManifestKeyV1>,
    settlement_key_prefix: String,
    provisioning_owner: String,
    run_mode: String,
    run_may_create_or_repair: bool,
    telemetry_write_command: String,
    manifest_write_allowed_in_p1e: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage8bP1eRedisManifestKeyV1 {
    name: String,
    #[serde(rename = "type")]
    key_type: String,
    groups: Vec<String>,
}

/// Verified loopback DB15 control plane.  It exposes only bounded diagnostic
/// operations and the two fixed telemetry writes; arbitrary Redis commands,
/// namespace creation and manifest mutation remain impossible through this
/// type.
pub struct Stage8bP1eRedisControlV1 {
    connection: ConnectionManager,
    namespace: Stage8bP1RedisNamespace,
}

/// Linear result of S05.  The lifecycle transport and diagnostic control
/// connection are kept distinct so telemetry can never obtain an XACK or
/// command-publication capability.
pub struct Stage8bP1eVerifiedRedisSessionV1 {
    transport: Stage8bP1RedisSemanticCompositionTransport,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eVerifiedRedisSessionV1 {
    pub fn into_parts(
        self,
    ) -> (
        Stage8bP1RedisSemanticCompositionTransport,
        Stage8bP1eRedisControlV1,
    ) {
        (self.transport, self.control)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1eConsumerHygieneReportV1 {
    pub inventory_count: usize,
    pub examined_count: usize,
    pub deleted_count: usize,
}

impl Stage8bP1eRedisControlV1 {
    pub async fn pel_count(&mut self) -> Result<usize, Stage8bP1eRedisControlError> {
        let reply: StreamPendingReply = redis_operation(
            redis::cmd("XPENDING")
                .arg(&self.namespace.canonical_m10_stream)
                .arg(&self.namespace.m10_consumer_group)
                .query_async(&mut self.connection),
        )
        .await?;
        Ok(reply.count())
    }

    pub async fn clean_stale_zero_pending_consumers(
        &mut self,
        current_consumer: &str,
    ) -> Result<Stage8bP1eConsumerHygieneReportV1, Stage8bP1eRedisControlError> {
        self.clean_stale_zero_pending_consumers_inner(
            current_consumer,
            STAGE8B_P1E_STALE_CONSUMER_IDLE_MS,
            || async { Ok(()) },
        )
        .await
    }

    async fn clean_stale_zero_pending_consumers_inner<AfterDiscovery, AfterDiscoveryFuture>(
        &mut self,
        current_consumer: &str,
        minimum_idle_ms: usize,
        after_discovery: AfterDiscovery,
    ) -> Result<Stage8bP1eConsumerHygieneReportV1, Stage8bP1eRedisControlError>
    where
        AfterDiscovery: FnOnce() -> AfterDiscoveryFuture,
        AfterDiscoveryFuture: Future<Output = Result<(), Stage8bP1eRedisControlError>>,
    {
        let mut consumers: StreamInfoConsumersReply = redis_operation(
            redis::cmd("XINFO")
                .arg("CONSUMERS")
                .arg(&self.namespace.canonical_m10_stream)
                .arg(&self.namespace.m10_consumer_group)
                .query_async(&mut self.connection),
        )
        .await?;
        let inventory_count = consumers.consumers.len();
        if inventory_count > STAGE8B_P1E_STALE_CONSUMER_INVENTORY_MAX {
            return Err(Stage8bP1eRedisControlError::ConsumerInventoryInvalid);
        }
        consumers.consumers.sort_by(|left, right| {
            left.pending
                .cmp(&right.pending)
                .then_with(|| right.idle.cmp(&left.idle))
                .then_with(|| left.name.as_bytes().cmp(right.name.as_bytes()))
        });
        let candidates: Vec<_> = consumers
            .consumers
            .into_iter()
            .filter(|consumer| consumer.name != current_consumer)
            .take(STAGE8B_P1E_STALE_CONSUMER_EXAMINE_MAX)
            .collect();
        let examined_count = candidates.len();
        after_discovery().await?;
        let mut deleted_count = 0;
        for consumer in candidates {
            if consumer.pending != 0 || consumer.idle < minimum_idle_ms {
                continue;
            }
            let deleted: i64 = redis_operation(
                redis::cmd("EVAL")
                    .arg(ATOMIC_STALE_CONSUMER_DELETE_SCRIPT_V1)
                    .arg(1)
                    .arg(&self.namespace.canonical_m10_stream)
                    .arg(&self.namespace.m10_consumer_group)
                    .arg(&consumer.name)
                    .arg(minimum_idle_ms)
                    .query_async(&mut self.connection),
            )
            .await?;
            if deleted != 0 && deleted != 1 {
                return Err(Stage8bP1eRedisControlError::ConsumerInventoryInvalid);
            }
            deleted_count += usize::try_from(deleted)
                .map_err(|_| Stage8bP1eRedisControlError::ConsumerInventoryInvalid)?;
        }
        Ok(Stage8bP1eConsumerHygieneReportV1 {
            inventory_count,
            examined_count,
            deleted_count,
        })
    }

    pub async fn publish_health(
        &mut self,
        envelope: &Stage8bP1eTelemetryEnvelopeV1<Stage8bP1eHealthPayloadV1>,
    ) -> Result<String, Stage8bP1eRedisControlError> {
        let payload = envelope
            .canonical_bytes()
            .map_err(|_| Stage8bP1eRedisControlError::TelemetryWriteFailed)?;
        self.publish_telemetry(&self.namespace.health_stream.clone(), payload)
            .await
    }

    pub async fn publish_readiness(
        &mut self,
        envelope: &Stage8bP1eTelemetryEnvelopeV1<Stage8bP1eReadinessPayloadV1>,
    ) -> Result<String, Stage8bP1eRedisControlError> {
        let payload = envelope
            .canonical_bytes()
            .map_err(|_| Stage8bP1eRedisControlError::TelemetryWriteFailed)?;
        self.publish_telemetry(&self.namespace.readiness_stream.clone(), payload)
            .await
    }

    async fn publish_telemetry(
        &mut self,
        stream: &str,
        payload: Vec<u8>,
    ) -> Result<String, Stage8bP1eRedisControlError> {
        let payload = std::str::from_utf8(&payload)
            .map_err(|_| Stage8bP1eRedisControlError::TelemetryWriteFailed)?;
        redis_operation(
            redis::cmd("XADD")
                .arg(stream)
                .arg("NOMKSTREAM")
                .arg("MAXLEN")
                .arg("=")
                .arg(STAGE8B_P1E_TELEMETRY_RETENTION)
                .arg("*")
                .arg("payload")
                .arg(payload)
                .query_async(&mut self.connection),
        )
        .await
        .map_err(|_| Stage8bP1eRedisControlError::TelemetryWriteFailed)
    }
}

/// Performs S05 against a pre-provisioned DB15 namespace.  No command in this
/// function can create a key, group or stream.
pub async fn attach_stage8b_p1e_verified_redis(
    plan: &Stage8bP1eRedisAttachPlanV1,
) -> Result<Stage8bP1eVerifiedRedisSessionV1, Stage8bP1eRedisControlError> {
    let client = redis::Client::open(plan.redis_url.as_str())
        .map_err(|_| Stage8bP1eRedisControlError::Redis)?;
    let mut connection = redis_operation(ConnectionManager::new(client)).await?;

    let manifest_type: String = redis_operation(
        redis::cmd("TYPE")
            .arg(STAGE8B_P1E_DEPLOYMENT_MANIFEST_KEY)
            .query_async(&mut connection),
    )
    .await?;
    if manifest_type != "string" {
        return Err(Stage8bP1eRedisControlError::KeyTypeMismatch);
    }
    let manifest_bytes: Vec<u8> = redis_operation(
        redis::cmd("GET")
            .arg(STAGE8B_P1E_DEPLOYMENT_MANIFEST_KEY)
            .query_async(&mut connection),
    )
    .await?;
    let canonical = canonical_json_bytes(&manifest_bytes)
        .map_err(|_| Stage8bP1eRedisControlError::ManifestMismatch)?;
    if canonical != manifest_bytes
        || sha256_hex(&canonical) != plan.redis_deployment_manifest_sha256
    {
        return Err(Stage8bP1eRedisControlError::ManifestMismatch);
    }
    let manifest: Stage8bP1eRedisDeploymentManifestV1 = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| Stage8bP1eRedisControlError::ManifestMismatch)?;
    validate_redis_deployment_manifest(plan, &manifest)?;

    for key in &manifest.keys {
        let actual_type: String = redis_operation(
            redis::cmd("TYPE")
                .arg(&key.name)
                .query_async(&mut connection),
        )
        .await?;
        if actual_type != key.key_type {
            return Err(Stage8bP1eRedisControlError::KeyTypeMismatch);
        }
        let groups: StreamInfoGroupsReply = redis_operation(
            redis::cmd("XINFO")
                .arg("GROUPS")
                .arg(&key.name)
                .query_async(&mut connection),
        )
        .await?;
        let mut actual_groups: Vec<_> = groups.groups.into_iter().map(|group| group.name).collect();
        actual_groups.sort();
        let mut expected_groups = key.groups.clone();
        expected_groups.sort();
        if actual_groups != expected_groups {
            return Err(Stage8bP1eRedisControlError::GroupMismatch);
        }
    }

    let transport = tokio::time::timeout(
        StdDuration::from_millis(STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
        attach_stage8b_p1_redis(&plan.redis_url, plan.redis_config.clone()),
    )
    .await
    .map_err(|_| Stage8bP1eRedisControlError::OperationTimeout)?
    .map_err(|_| Stage8bP1eRedisControlError::GroupMismatch)?;
    Ok(Stage8bP1eVerifiedRedisSessionV1 {
        transport,
        control: Stage8bP1eRedisControlV1 {
            connection,
            namespace: plan.namespace.clone(),
        },
    })
}

fn validate_redis_deployment_manifest(
    plan: &Stage8bP1eRedisAttachPlanV1,
    manifest: &Stage8bP1eRedisDeploymentManifestV1,
) -> Result<(), Stage8bP1eRedisControlError> {
    let namespace = &plan.namespace;
    let expected_keys = vec![
        manifest_key(
            &namespace.canonical_m10_stream,
            &[&namespace.m10_consumer_group],
        ),
        manifest_key(
            &namespace.canonical_command_stream,
            &[&namespace.stage7b_command_consumer_group],
        ),
        manifest_key(&namespace.canonical_ack_stream, &[]),
        manifest_key(&namespace.canonical_dlq_stream, &[]),
        manifest_key(&namespace.canonical_order_stream, &[]),
        manifest_key(&namespace.canonical_trade_stream, &[]),
        manifest_key(&namespace.canonical_position_stream, &[]),
        manifest_key(&namespace.runtime_state_stream, &[]),
        manifest_key(&namespace.health_stream, &[]),
        manifest_key(&namespace.readiness_stream, &[]),
    ];
    let expected_urls = vec![
        STAGE8B_P1E_REDIS_URL_IPV4.to_string(),
        STAGE8B_P1E_REDIS_URL_IPV6.to_string(),
    ];
    if manifest.schema_version != 1
        || manifest.domain != "moex.stage8b.p1e.redis-deployment-manifest.v1"
        || manifest.manifest_key != STAGE8B_P1E_DEPLOYMENT_MANIFEST_KEY
        || manifest.redis_url_allowlist != expected_urls
        || manifest.redis_db_index != 15
        || manifest.operational_identity_sha256 != plan.operational_identity_sha256
        || manifest.runtime_config_fingerprint_sha256
            != plan.runtime_config_fingerprint_sha256
        || manifest.instrument_map_fingerprint_sha256
            != stage8b_p1_imoexf_instrument_map_fingerprint_sha256()
        || manifest.deployment_generation != plan.deployment_generation
        || manifest.consumer_generation != plan.consumer_generation
        || manifest.namespace_digest_sha256 != STAGE8B_P1E_NAMESPACE_DIGEST_SHA256
        || manifest.keys != expected_keys
        || manifest.settlement_key_prefix != namespace.settlement_key_prefix
        || manifest.provisioning_owner != "stage8b-p1f-administrative-boundary"
        || manifest.run_mode != "verify-only"
        || manifest.run_may_create_or_repair
        || manifest.telemetry_write_command
            != "XADD <health-or-readiness-stream> NOMKSTREAM MAXLEN = 4096 * payload <canonical-json>"
        || manifest.manifest_write_allowed_in_p1e
    {
        return Err(Stage8bP1eRedisControlError::ManifestMismatch);
    }
    Ok(())
}

fn manifest_key(name: &str, groups: &[&str]) -> Stage8bP1eRedisManifestKeyV1 {
    Stage8bP1eRedisManifestKeyV1 {
        name: name.to_string(),
        key_type: "stream".to_string(),
        groups: groups.iter().map(|group| (*group).to_string()).collect(),
    }
}

async fn redis_operation<T, F>(operation: F) -> Result<T, Stage8bP1eRedisControlError>
where
    F: Future<Output = redis::RedisResult<T>>,
{
    tokio::time::timeout(
        StdDuration::from_millis(STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
        operation,
    )
    .await
    .map_err(|_| Stage8bP1eRedisControlError::OperationTimeout)?
    .map_err(|_| Stage8bP1eRedisControlError::Redis)
}

pub fn parse_stage8b_p1e_supervisor_config_v1(
    bytes: &[u8],
) -> Result<Stage8bP1eSupervisorConfigV1, Stage8bP1eSupervisorConfigError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = NoDuplicateJson::deserialize(&mut deserializer)?.0;
    deserializer
        .end()
        .map_err(|_| Stage8bP1eSupervisorConfigError::InvalidJson)?;
    serde_json::from_value(value).map_err(|_| Stage8bP1eSupervisorConfigError::InvalidJson)
}

pub fn validate_stage8b_p1e_supervisor_config_v1(
    config: Stage8bP1eSupervisorConfigV1,
    boot_id: [u8; 16],
) -> Result<Stage8bP1eValidatedSupervisorConfigV1, Stage8bP1eSupervisorConfigError> {
    if config.schema_version != STAGE8B_P1E_SUPERVISOR_CONFIG_SCHEMA_VERSION
        || config.runtime_profile_id != STAGE8B_P1E_RUNTIME_PROFILE_ID
        || config.runtime_profile_sha256 != STAGE8B_P1E_RUNTIME_PROFILE_SHA256
        || config.redis_runtime_policy_id != STAGE8B_P1E_REDIS_RUNTIME_POLICY_ID
        || config.redis_runtime_policy_sha256 != STAGE8B_P1E_REDIS_RUNTIME_POLICY_SHA256
        || config.telemetry_contract_sha256 != STAGE8B_P1E_TELEMETRY_CONTRACT_SHA256
        || !is_sha256_hex(&config.first_boot_source_bundle_sha256)
        || !is_sha256_hex(&config.redis_deployment_manifest_sha256)
        || !matches!(
            config.redis_url.as_str(),
            STAGE8B_P1E_REDIS_URL_IPV4 | STAGE8B_P1E_REDIS_URL_IPV6
        )
        || !(STAGE8B_P1E_HEALTH_INTERVAL_MIN_MS..=STAGE8B_P1E_HEALTH_INTERVAL_MAX_MS)
            .contains(&config.health_interval_ms)
        || !(STAGE8B_P1E_SHUTDOWN_GRACE_MIN_MS..=STAGE8B_P1E_SHUTDOWN_GRACE_MAX_MS)
            .contains(&config.shutdown_grace_ms)
        || config.shutdown_grace_ms >= STAGE8B_P1E_SYSTEMD_STOP_TIMEOUT_MS
    {
        return Err(Stage8bP1eSupervisorConfigError::InvalidConfig);
    }

    let (runtime, runtime_config_fingerprint_sha256) =
        Stage8bP1RuntimeProfileV1::build_hybrid_runtime()?;
    if config.bootstrap.runtime_config_fingerprint_sha256 != runtime_config_fingerprint_sha256 {
        return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile);
    }
    let bootstrap = validate_stage8b_p1_bootstrap_config(config.bootstrap)?;
    let consumer_name = format!(
        "p1e-{}-{}",
        bootstrap.deployment_generation(),
        lower_hex(&boot_id)
    );
    let redis_config = Stage8bP1RedisConfig {
        consumer_name,
        read_count: 1,
        claim_count: 2,
        claim_idle_ms: 30_000,
        max_claim_pages: 1,
        retention_floor: STAGE8B_P1E_TELEMETRY_RETENTION,
    };
    redis_config
        .validate()
        .map_err(|_| Stage8bP1eSupervisorConfigError::InvalidConfig)?;

    Ok(Stage8bP1eValidatedSupervisorConfigV1 {
        bootstrap,
        runtime,
        runtime_config_fingerprint_sha256,
        redis_url: config.redis_url,
        redis_deployment_manifest_sha256: config.redis_deployment_manifest_sha256,
        first_boot_source_bundle_sha256: config.first_boot_source_bundle_sha256,
        redis_config,
        namespace: stage8b_p1_redis_namespace(),
        health_interval_ms: config.health_interval_ms,
        shutdown_grace_ms: config.shutdown_grace_ms,
    })
}

pub struct Stage8bP1RuntimeProfileV1;

impl Stage8bP1RuntimeProfileV1 {
    pub fn build_hybrid_runtime(
    ) -> Result<(HybridIntradayRuntimeStrategy, String), Stage8bP1eSupervisorConfigError> {
        let canonical = canonical_json_bytes(RUNTIME_PROFILE_BYTES)?;
        if sha256_hex(&canonical) != STAGE8B_P1E_RUNTIME_PROFILE_SHA256 {
            return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile);
        }
        let profile: RuntimeProfileDocument = serde_json::from_slice(RUNTIME_PROFILE_BYTES)
            .map_err(|_| Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile)?;
        profile.validate_envelope()?;
        let semantic = profile.semantic_config;
        semantic.validate_high180_defaults()?;
        let config = HybridIntradayRuntimeConfig {
            symbol: exact_string(&semantic.symbol, "IMOEXF")?,
            profile: match semantic.profile.as_str() {
                "imoexf_primary_riskgate_high180_lb120" => {
                    HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120
                }
                _ => return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile),
            },
            mr_variant: match semantic.mr_variant.as_str() {
                "high180" => MeanReversionVariant::High180,
                _ => return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile),
            },
            mr_gate_policy: match semantic.mr_gate_policy.as_str() {
                "shadow_pnl_lb120_positive" => MrGatePolicy::ShadowPnlLb120Positive,
                _ => return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile),
            },
            risk_gate_mode: match semantic.risk_gate_mode.as_str() {
                "normal_append" => RiskGateMode::NormalAppend,
                _ => return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile),
            },
            risk_gate_seed_file: require_none(semantic.risk_gate_seed_file)?,
            risk_gate_ledger_key: require_none(semantic.risk_gate_ledger_key)?,
            model_session_start_time: Some(parse_time(&semantic.model_session_start_time)?),
            model_session_end_time: Some(parse_time(&semantic.model_session_end_time)?),
            qty: parse_decimal(&semantic.qty)?,
            live_order_style: match semantic.live_order_style.as_str() {
                "market" => BrokerNeutralMarketOrderStyle::Market,
                _ => return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile),
            },
            tick_size: parse_decimal(&semantic.tick_size)?,
            marketable_limit_offset_ticks: semantic.marketable_limit_offset_ticks,
            timezone_offset_hours: semantic.timezone_offset_hours,
            session_close_hour: semantic.session_close_hour,
            session_close_minute: semantic.session_close_minute,
            weekends_off: semantic.weekends_off,
            stop_end_buffer_sec: semantic.stop_end_buffer_sec,
            repair_deadline_sec: semantic.repair_deadline_sec,
            sl_escalate_timeout_sec: semantic.sl_escalate_timeout_sec,
            max_repair_retries: semantic.max_repair_retries,
            repair_backoff_base_sec: semantic.repair_backoff_base_sec,
            repair_backoff_max_sec: semantic.repair_backoff_max_sec,
            pending_timeout_sec: semantic.pending_timeout_sec,
            partial_entry_fill_timeout_ms: semantic.partial_entry_fill_timeout_ms,
            mr_config: MeanReversionConfig {
                min_range_long: parse_decimal(&semantic.mean_reversion.min_range_long)?,
                max_range_long: parse_decimal(&semantic.mean_reversion.max_range_long)?,
                k_long: parse_decimal(&semantic.mean_reversion.k_long)?,
                take_k_long: parse_decimal(&semantic.mean_reversion.take_k_long)?,
                stop_k_long: parse_decimal(&semantic.mean_reversion.stop_k_long)?,
                min_range_short: parse_decimal(&semantic.mean_reversion.min_range_short)?,
                max_range_short: parse_decimal(&semantic.mean_reversion.max_range_short)?,
                k_short: parse_decimal(&semantic.mean_reversion.k_short)?,
                take_k_short: parse_decimal(&semantic.mean_reversion.take_k_short)?,
                stop_k_short: parse_decimal(&semantic.mean_reversion.stop_k_short)?,
                tick_size: parse_decimal(&semantic.mean_reversion.tick_size)?,
                session_end_time: parse_time(&semantic.mean_reversion.session_end_time)?,
                exit_offset: Duration::seconds(semantic.mean_reversion.exit_offset_seconds),
            },
            breakout_config: IntradayBreakoutConfig {
                k: parse_decimal(&semantic.breakout.k)?,
                stop1_range: parse_decimal(&semantic.breakout.stop1_range)?,
                stop2_range: parse_decimal(&semantic.breakout.stop2_range)?,
                big_move_threshold: parse_decimal(&semantic.breakout.big_move_threshold)?,
                min_range: parse_decimal(&semantic.breakout.min_range)?,
                min_range_mode: match semantic.breakout.min_range_mode.as_str() {
                    "absolute" => MinRangeMode::Absolute,
                    _ => return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile),
                },
                exclude_weekends: semantic.breakout.exclude_weekends,
                wait_hours: parse_decimal(&semantic.breakout.wait_hours)?,
            },
            orchestrator_config: HybridOrchestratorConfig {
                breakout_eod_mode: match semantic.orchestrator.breakout_eod_mode.as_str() {
                    "same_day" => BreakoutEodMode::SameDay,
                    _ => return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile),
                },
                breakout_overnight_exit_time: parse_time(
                    &semantic.orchestrator.breakout_overnight_exit_time,
                )?,
            },
        };
        let runtime = HybridIntradayRuntimeStrategy::new(config);
        let fingerprint = runtime.stage5c_config_fingerprint();
        Ok((runtime, fingerprint))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1eHealthStatusV1 {
    Starting,
    Healthy,
    Degraded,
    Draining,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1eReadinessPhaseV1 {
    Starting,
    PaperReady,
    Degraded,
    Draining,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1eReadinessReasonV1 {
    Startup,
    DurableNotReady,
    RedisManifestMismatch,
    ClaimScanIncomplete,
    SourcePollStale,
    PelPending,
    LifecycleUnresolved,
    BlockedMaterial,
    ShutdownRequested,
    OwnerLost,
    TelemetryFailed,
    SignalTaskFailed,
    RedisError,
    GraceExpired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1eFailureClassV1 {
    None,
    ConfigInvalid,
    CredentialInvalid,
    DurableAuthenticationFailed,
    RecoveryBlocked,
    RedisUrlInvalid,
    RedisManifestMismatch,
    RedisKeyTypeMismatch,
    RedisGroupMismatch,
    RedisClaimFailed,
    RedisReadFailed,
    RedisTelemetryFailed,
    OwnerPanicked,
    OwnerReturnedWithoutOwner,
    OwnerReturnedUnexpectedly,
    SignalTaskFailed,
    GraceDeadlineExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1eShutdownPhaseV1 {
    Clear,
    Requested,
    Draining,
    Stopped,
}

/// Exhaustive S04 classification of the authenticated durable restart.
///
/// This enum deliberately mirrors every `Stage7bRestartOutcome` variant.  A
/// newly added durable outcome therefore makes the classifier below fail to
/// compile until the P1-e restart contract is extended explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Stage8bP1eRestartKindV1 {
    Ready,
    Stage8a4I3Pending,
    P1SemanticPrepublicationPending,
    P1SemanticPrepublicationReady,
    P1SemanticZeroIntentAckPending,
    P1d2PreAckPending,
    P1d2AckCommitted,
    P1d2TruthCommitted,
    P1d4GeneratedMarketPrepublicationPending,
    P1d4GeneratedMarketDispatchPending,
    P1d4GeneratedMarketOrderPending,
    P1d4GeneratedMarketPreFinalizationPending,
    P1d4GeneratedMarketPreAckPending,
    P1d4GeneratedMarketAckCommitted,
    P1d4GeneratedMarketTruthCommitted,
    P1d3DispatchPending,
    P1d3PreAckPending,
    P1d3AckCommitted,
    P1d3TruthCommitted,
    P1d3CancelContinuationPending,
    P1d3SemanticPending,
    Blocked,
}

impl Stage8bP1eRestartKindV1 {
    pub const ALL: [Self; 22] = [
        Self::Ready,
        Self::Stage8a4I3Pending,
        Self::P1SemanticPrepublicationPending,
        Self::P1SemanticPrepublicationReady,
        Self::P1SemanticZeroIntentAckPending,
        Self::P1d2PreAckPending,
        Self::P1d2AckCommitted,
        Self::P1d2TruthCommitted,
        Self::P1d4GeneratedMarketPrepublicationPending,
        Self::P1d4GeneratedMarketDispatchPending,
        Self::P1d4GeneratedMarketOrderPending,
        Self::P1d4GeneratedMarketPreFinalizationPending,
        Self::P1d4GeneratedMarketPreAckPending,
        Self::P1d4GeneratedMarketAckCommitted,
        Self::P1d4GeneratedMarketTruthCommitted,
        Self::P1d3DispatchPending,
        Self::P1d3PreAckPending,
        Self::P1d3AckCommitted,
        Self::P1d3TruthCommitted,
        Self::P1d3CancelContinuationPending,
        Self::P1d3SemanticPending,
        Self::Blocked,
    ];

    /// S05 Redis attachment is forbidden for the two fail-closed outcomes.
    pub const fn permits_redis_attach(self) -> bool {
        !matches!(self, Self::Stage8a4I3Pending | Self::Blocked)
    }
}

pub const fn stage8b_p1e_classify_restart_v1(
    outcome: &Stage7bRestartOutcome,
) -> Stage8bP1eRestartKindV1 {
    match outcome {
        Stage7bRestartOutcome::Ready(_) => Stage8bP1eRestartKindV1::Ready,
        Stage7bRestartOutcome::Stage8a4I3Pending(_) => Stage8bP1eRestartKindV1::Stage8a4I3Pending,
        Stage7bRestartOutcome::P1SemanticPrepublicationPending(_) => {
            Stage8bP1eRestartKindV1::P1SemanticPrepublicationPending
        }
        Stage7bRestartOutcome::P1SemanticPrepublicationReady(_) => {
            Stage8bP1eRestartKindV1::P1SemanticPrepublicationReady
        }
        Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(_) => {
            Stage8bP1eRestartKindV1::P1SemanticZeroIntentAckPending
        }
        Stage7bRestartOutcome::P1d2PreAckPending(_) => Stage8bP1eRestartKindV1::P1d2PreAckPending,
        Stage7bRestartOutcome::P1d2AckCommitted(_) => Stage8bP1eRestartKindV1::P1d2AckCommitted,
        Stage7bRestartOutcome::P1d2TruthCommitted(_) => Stage8bP1eRestartKindV1::P1d2TruthCommitted,
        Stage7bRestartOutcome::P1d4GeneratedMarketPrepublicationPending(_) => {
            Stage8bP1eRestartKindV1::P1d4GeneratedMarketPrepublicationPending
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketDispatchPending(_) => {
            Stage8bP1eRestartKindV1::P1d4GeneratedMarketDispatchPending
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketOrderPending(_) => {
            Stage8bP1eRestartKindV1::P1d4GeneratedMarketOrderPending
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketPreFinalizationPending(_) => {
            Stage8bP1eRestartKindV1::P1d4GeneratedMarketPreFinalizationPending
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketPreAckPending(_) => {
            Stage8bP1eRestartKindV1::P1d4GeneratedMarketPreAckPending
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketAckCommitted(_) => {
            Stage8bP1eRestartKindV1::P1d4GeneratedMarketAckCommitted
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketTruthCommitted(_) => {
            Stage8bP1eRestartKindV1::P1d4GeneratedMarketTruthCommitted
        }
        Stage7bRestartOutcome::P1d3DispatchPending(_) => {
            Stage8bP1eRestartKindV1::P1d3DispatchPending
        }
        Stage7bRestartOutcome::P1d3PreAckPending(_) => Stage8bP1eRestartKindV1::P1d3PreAckPending,
        Stage7bRestartOutcome::P1d3AckCommitted(_) => Stage8bP1eRestartKindV1::P1d3AckCommitted,
        Stage7bRestartOutcome::P1d3TruthCommitted(_) => Stage8bP1eRestartKindV1::P1d3TruthCommitted,
        Stage7bRestartOutcome::P1d3CancelContinuationPending(_) => {
            Stage8bP1eRestartKindV1::P1d3CancelContinuationPending
        }
        Stage7bRestartOutcome::P1d3SemanticPending(_) => {
            Stage8bP1eRestartKindV1::P1d3SemanticPending
        }
        Stage7bRestartOutcome::Blocked(_) => Stage8bP1eRestartKindV1::Blocked,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1eReadinessInputsV1 {
    pub owner_live: bool,
    pub durable_ready: bool,
    pub redis_manifest_verified: bool,
    pub claim_scan_complete: bool,
    pub source_poll_fresh: bool,
    pub settlement_healthy: bool,
    pub pel_count: u64,
    pub unresolved_lifecycle_count: u64,
    pub blocked_request_count: u64,
    pub shutdown_phase: Stage8bP1eShutdownPhaseV1,
    pub telemetry_healthy: bool,
    pub signal_task_healthy: bool,
    pub grace_expired: bool,
}

pub fn stage8b_p1e_readiness_v1(
    input: &Stage8bP1eReadinessInputsV1,
) -> (Stage8bP1eReadinessPhaseV1, Vec<Stage8bP1eReadinessReasonV1>) {
    let mut reasons = BTreeSet::new();
    if !input.durable_ready {
        reasons.insert(Stage8bP1eReadinessReasonV1::DurableNotReady);
    }
    if !input.redis_manifest_verified {
        reasons.insert(Stage8bP1eReadinessReasonV1::RedisManifestMismatch);
    }
    if !input.claim_scan_complete {
        reasons.insert(Stage8bP1eReadinessReasonV1::ClaimScanIncomplete);
    }
    if !input.source_poll_fresh {
        reasons.insert(Stage8bP1eReadinessReasonV1::SourcePollStale);
    }
    if input.pel_count != 0 {
        reasons.insert(Stage8bP1eReadinessReasonV1::PelPending);
    }
    if input.unresolved_lifecycle_count != 0 {
        reasons.insert(Stage8bP1eReadinessReasonV1::LifecycleUnresolved);
    }
    if input.blocked_request_count != 0 {
        reasons.insert(Stage8bP1eReadinessReasonV1::BlockedMaterial);
    }
    if !input.owner_live {
        reasons.insert(Stage8bP1eReadinessReasonV1::OwnerLost);
    }
    if !input.settlement_healthy {
        reasons.insert(Stage8bP1eReadinessReasonV1::RedisError);
    }
    if !input.telemetry_healthy {
        reasons.insert(Stage8bP1eReadinessReasonV1::TelemetryFailed);
    }
    if !input.signal_task_healthy {
        reasons.insert(Stage8bP1eReadinessReasonV1::SignalTaskFailed);
    }
    if input.grace_expired {
        reasons.insert(Stage8bP1eReadinessReasonV1::GraceExpired);
    }
    let phase = match input.shutdown_phase {
        Stage8bP1eShutdownPhaseV1::Draining | Stage8bP1eShutdownPhaseV1::Requested => {
            reasons.insert(Stage8bP1eReadinessReasonV1::ShutdownRequested);
            Stage8bP1eReadinessPhaseV1::Draining
        }
        Stage8bP1eShutdownPhaseV1::Stopped => Stage8bP1eReadinessPhaseV1::Stopped,
        Stage8bP1eShutdownPhaseV1::Clear if reasons.is_empty() => {
            Stage8bP1eReadinessPhaseV1::PaperReady
        }
        Stage8bP1eShutdownPhaseV1::Clear => Stage8bP1eReadinessPhaseV1::Degraded,
    };
    (phase, reasons.into_iter().collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8bP1eTelemetryEnvelopeV1<T> {
    pub schema_version: u16,
    pub domain: &'static str,
    pub observation_ts_utc: String,
    pub boot_id: String,
    pub boot_mode: &'static str,
    pub payload: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8bP1eHealthPayloadV1 {
    pub status: Stage8bP1eHealthStatusV1,
    pub operational_identity_sha256: String,
    pub deployment_generation: u64,
    pub runtime_config_fingerprint_sha256: String,
    pub durable_seal_generation: u64,
    pub durable_commitment_sha256: String,
    pub consumer_name: String,
    pub source_poll_fresh: bool,
    pub claim_scan_complete: bool,
    pub pel_count: u64,
    pub blocked_request_count: u64,
    pub blocked_request_hashes: Vec<String>,
    pub last_semantic_bar_ts_utc: Option<String>,
    pub last_canonical_ack_ts_utc: Option<String>,
    pub shutdown_phase: Stage8bP1eShutdownPhaseV1,
    pub last_failure_class: Stage8bP1eFailureClassV1,
    pub paper_only: bool,
    pub finam_transport_attached: bool,
    pub broker_network_dispatch_attached: bool,
    pub runtime_live: bool,
    pub real_orders: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8bP1eReadinessPayloadV1 {
    pub phase: Stage8bP1eReadinessPhaseV1,
    pub reasons: Vec<Stage8bP1eReadinessReasonV1>,
    pub operational_identity_sha256: String,
    pub deployment_generation: u64,
    pub runtime_config_fingerprint_sha256: String,
    pub durable_seal_generation: u64,
    pub consumer_name: String,
    pub source_poll_fresh: bool,
    pub claim_scan_complete: bool,
    pub pel_count: u64,
    pub unresolved_lifecycle_count: u64,
    pub blocked_request_count: u64,
    pub shutdown_phase: Stage8bP1eShutdownPhaseV1,
    pub paper_only: bool,
    pub finam_transport_attached: bool,
    pub broker_network_dispatch_attached: bool,
    pub runtime_live: bool,
    pub real_orders: bool,
}

impl<T: Serialize> Stage8bP1eTelemetryEnvelopeV1<T> {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Stage8bP1eSupervisorConfigError> {
        serde_json::to_vec(self).map_err(|_| Stage8bP1eSupervisorConfigError::InvalidConfig)
    }
}

pub fn stage8b_p1e_telemetry_envelope_v1<T>(
    domain: &'static str,
    observed_at: DateTime<Utc>,
    boot_id: [u8; 16],
    payload: T,
) -> Stage8bP1eTelemetryEnvelopeV1<T> {
    Stage8bP1eTelemetryEnvelopeV1 {
        schema_version: 1,
        domain,
        observation_ts_utc: observed_at.to_rfc3339_opts(SecondsFormat::Micros, true),
        boot_id: lower_hex(&boot_id),
        boot_mode: "restart_only",
        payload,
    }
}

pub fn stage8b_p1e_redact_account_id(account_id: &str) -> String {
    domain_hash("moex.stage8b.p1e.redact.account.v1", account_id)
}

pub fn stage8b_p1e_redact_request_id(request_id: &str) -> String {
    domain_hash("moex.stage8b.p1e.redact.request.v1", request_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eSupervisorEventV1 {
    ExternalSignal,
    OwnerPanicked,
    OwnerReturnedWithoutOwner,
    OwnerReturnedUnexpectedly,
    TelemetryFailed,
    SignalTaskFailed,
    RedisLifecycleFailed,
    GraceExpired,
    AuthenticatedBoundaryReached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stage8bP1eCoordinatorDecisionV1 {
    pub action: Stage8bP1eCoordinatorActionV1,
    pub exit_code: Option<u8>,
    pub terminal_failure: Option<Stage8bP1eTerminalFailureV1>,
    pub readiness_phase: Option<Stage8bP1eReadinessPhaseV1>,
    pub request_shutdown: bool,
    pub owner_may_bounded_drain: bool,
    pub retained_shutdown_cause: Option<Stage8bP1eShutdownCauseV1>,
    pub first_request_sequence: Option<u64>,
    pub grace_deadline_utc_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eCoordinatorActionV1 {
    ContinueOwnerLoop,
    DrainToAuthenticatedBoundary,
    CompleteUsingRetainedIntent,
}

/// A fatal completion outcome is deliberately separate from the initiating
/// first-wins shutdown intent. This keeps the original diagnostic cause while
/// preventing a later owner loss from inheriting an earlier successful exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eTerminalFailureV1 {
    OwnerLost,
    RedisLifecycle,
}

/// Stateful process coordinator. The first shutdown request owns the cause,
/// deadline and request sequence until process completion. A normal durable
/// checkpoint is therefore not itself a process-completion event.
pub struct Stage8bP1eCoordinatorV1 {
    shutdown_latch: Stage8bP1eShutdownLatchV1,
}

impl Default for Stage8bP1eCoordinatorV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage8bP1eCoordinatorV1 {
    pub const fn new() -> Self {
        Self {
            shutdown_latch: Stage8bP1eShutdownLatchV1::new(),
        }
    }

    pub fn shutdown_intent(&self) -> Option<&Stage8bP1eShutdownIntentV1> {
        self.shutdown_latch.intent()
    }

    pub fn coordinate(
        &mut self,
        event: Stage8bP1eSupervisorEventV1,
        owner_available: bool,
        now_utc_ms: i64,
        grace_deadline_utc_ms: i64,
        request_sequence: u64,
    ) -> Stage8bP1eCoordinatorDecisionV1 {
        match event {
            Stage8bP1eSupervisorEventV1::AuthenticatedBoundaryReached => {
                return self.authenticated_boundary_decision(now_utc_ms);
            }
            Stage8bP1eSupervisorEventV1::GraceExpired => {
                return self.grace_expired_decision();
            }
            Stage8bP1eSupervisorEventV1::RedisLifecycleFailed => {
                return Stage8bP1eCoordinatorDecisionV1 {
                    action: Stage8bP1eCoordinatorActionV1::CompleteUsingRetainedIntent,
                    exit_code: Some(67),
                    terminal_failure: Some(Stage8bP1eTerminalFailureV1::RedisLifecycle),
                    readiness_phase: Some(Stage8bP1eReadinessPhaseV1::Degraded),
                    request_shutdown: true,
                    owner_may_bounded_drain: false,
                    retained_shutdown_cause: self
                        .shutdown_latch
                        .intent()
                        .map(|intent| intent.cause()),
                    first_request_sequence: self
                        .shutdown_latch
                        .intent()
                        .map(|intent| intent.first_request_sequence()),
                    grace_deadline_utc_ms: self
                        .shutdown_latch
                        .intent()
                        .map(|intent| intent.grace_deadline_utc_ms()),
                };
            }
            Stage8bP1eSupervisorEventV1::OwnerPanicked
            | Stage8bP1eSupervisorEventV1::OwnerReturnedWithoutOwner => {
                self.shutdown_latch.request(Stage8bP1eShutdownIntentV1::new(
                    Stage8bP1eShutdownCauseV1::OwnerFailure,
                    grace_deadline_utc_ms,
                    request_sequence,
                ));
                return self.retained_decision(
                    Stage8bP1eCoordinatorActionV1::CompleteUsingRetainedIntent,
                    Some(70),
                    Some(Stage8bP1eTerminalFailureV1::OwnerLost),
                    Some(Stage8bP1eReadinessPhaseV1::Degraded),
                    false,
                );
            }
            _ => {}
        }

        let cause = match event {
            Stage8bP1eSupervisorEventV1::ExternalSignal => {
                Stage8bP1eShutdownCauseV1::ExternalSignal
            }
            Stage8bP1eSupervisorEventV1::OwnerReturnedUnexpectedly => {
                Stage8bP1eShutdownCauseV1::OwnerFailure
            }
            Stage8bP1eSupervisorEventV1::TelemetryFailed => {
                Stage8bP1eShutdownCauseV1::TelemetryFailure
            }
            Stage8bP1eSupervisorEventV1::SignalTaskFailed => {
                Stage8bP1eShutdownCauseV1::SignalTaskFailure
            }
            Stage8bP1eSupervisorEventV1::RedisLifecycleFailed
            | Stage8bP1eSupervisorEventV1::OwnerPanicked
            | Stage8bP1eSupervisorEventV1::OwnerReturnedWithoutOwner
            | Stage8bP1eSupervisorEventV1::GraceExpired
            | Stage8bP1eSupervisorEventV1::AuthenticatedBoundaryReached => unreachable!(
                "terminal and boundary events are handled before shutdown intent issuance"
            ),
        };
        self.shutdown_latch.request(Stage8bP1eShutdownIntentV1::new(
            cause,
            grace_deadline_utc_ms,
            request_sequence,
        ));

        let owner_may_bounded_drain = owner_available;
        if owner_may_bounded_drain {
            self.retained_decision(
                Stage8bP1eCoordinatorActionV1::DrainToAuthenticatedBoundary,
                None,
                None,
                Some(Stage8bP1eReadinessPhaseV1::Draining),
                true,
            )
        } else {
            let exit_code = self
                .shutdown_latch
                .intent()
                .map(|intent| intent.bounded_exit_class(now_utc_ms));
            self.retained_decision(
                Stage8bP1eCoordinatorActionV1::CompleteUsingRetainedIntent,
                exit_code,
                None,
                Some(Stage8bP1eReadinessPhaseV1::Degraded),
                false,
            )
        }
    }

    fn authenticated_boundary_decision(&self, now_utc_ms: i64) -> Stage8bP1eCoordinatorDecisionV1 {
        match self.shutdown_latch.intent() {
            Some(intent) => self.retained_decision(
                Stage8bP1eCoordinatorActionV1::CompleteUsingRetainedIntent,
                Some(intent.bounded_exit_class(now_utc_ms)),
                None,
                Some(Stage8bP1eReadinessPhaseV1::Stopped),
                false,
            ),
            None => Self::continue_owner_loop_decision(),
        }
    }

    fn grace_expired_decision(&self) -> Stage8bP1eCoordinatorDecisionV1 {
        if self.shutdown_latch.intent().is_some() {
            self.retained_decision(
                Stage8bP1eCoordinatorActionV1::CompleteUsingRetainedIntent,
                Some(72),
                None,
                Some(Stage8bP1eReadinessPhaseV1::Degraded),
                false,
            )
        } else {
            Self::continue_owner_loop_decision()
        }
    }

    const fn continue_owner_loop_decision() -> Stage8bP1eCoordinatorDecisionV1 {
        Stage8bP1eCoordinatorDecisionV1 {
            action: Stage8bP1eCoordinatorActionV1::ContinueOwnerLoop,
            exit_code: None,
            terminal_failure: None,
            readiness_phase: None,
            request_shutdown: false,
            owner_may_bounded_drain: false,
            retained_shutdown_cause: None,
            first_request_sequence: None,
            grace_deadline_utc_ms: None,
        }
    }

    fn retained_decision(
        &self,
        action: Stage8bP1eCoordinatorActionV1,
        exit_code: Option<u8>,
        terminal_failure: Option<Stage8bP1eTerminalFailureV1>,
        readiness_phase: Option<Stage8bP1eReadinessPhaseV1>,
        owner_may_bounded_drain: bool,
    ) -> Stage8bP1eCoordinatorDecisionV1 {
        let intent = self.shutdown_latch.intent();
        Stage8bP1eCoordinatorDecisionV1 {
            action,
            exit_code,
            terminal_failure,
            readiness_phase,
            request_shutdown: intent.is_some(),
            owner_may_bounded_drain,
            retained_shutdown_cause: intent.map(|value| value.cause()),
            first_request_sequence: intent.map(|value| value.first_request_sequence()),
            grace_deadline_utc_ms: intent.map(|value| value.grace_deadline_utc_ms()),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeProfileDocument {
    schema_version: u16,
    domain: String,
    profile_id: String,
    constructor: String,
    semantic_config: RuntimeSemanticConfig,
    paper_safety: RuntimePaperSafety,
}

impl RuntimeProfileDocument {
    fn validate_envelope(&self) -> Result<(), Stage8bP1eSupervisorConfigError> {
        if self.schema_version != 1
            || self.domain != "moex.stage8b.p1e.runtime-profile.v1"
            || self.profile_id != STAGE8B_P1E_RUNTIME_PROFILE_ID
            || self.constructor != "Stage8bP1RuntimeProfileV1::build_hybrid_runtime"
            || self.paper_safety.trade_mode != "paper"
            || self.paper_safety.allow_live_orders
            || self.paper_safety.finam_transport_attached
            || self.paper_safety.broker_dispatch_attached
            || self.paper_safety.runtime_live
            || self.paper_safety.real_orders
        {
            return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeSemanticConfig {
    symbol: String,
    profile: String,
    mr_variant: String,
    mr_gate_policy: String,
    risk_gate_mode: String,
    risk_gate_seed_file: Option<String>,
    risk_gate_ledger_key: Option<String>,
    model_session_start_time: String,
    model_session_end_time: String,
    qty: String,
    live_order_style: String,
    tick_size: String,
    marketable_limit_offset_ticks: i64,
    timezone_offset_hours: i32,
    session_close_hour: u32,
    session_close_minute: u32,
    weekends_off: bool,
    stop_end_buffer_sec: u64,
    repair_deadline_sec: u64,
    sl_escalate_timeout_sec: u64,
    max_repair_retries: u32,
    repair_backoff_base_sec: u64,
    repair_backoff_max_sec: u64,
    pending_timeout_sec: u64,
    partial_entry_fill_timeout_ms: u64,
    mean_reversion: RuntimeMeanReversion,
    breakout: RuntimeBreakout,
    orchestrator: RuntimeOrchestrator,
    high180: RuntimeHigh180,
}

impl RuntimeSemanticConfig {
    fn validate_high180_defaults(&self) -> Result<(), Stage8bP1eSupervisorConfigError> {
        let expected = (
            0.005,
            0.050,
            0.085,
            0.090,
            7.0,
            180_u64,
            NaiveTime::from_hms_opt(11, 59, 59)
                .ok_or(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile)?,
        );
        let actual = (
            parse_decimal(&self.high180.min_rel_range)?,
            parse_decimal(&self.high180.max_rel_range)?,
            parse_decimal(&self.high180.k_long)?,
            parse_decimal(&self.high180.k_short)?,
            parse_decimal(&self.high180.stop_loss_mult)?,
            self.high180.max_hold_minutes,
            parse_time(&self.high180.entry_end_time)?,
        );
        if actual != expected {
            return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeMeanReversion {
    min_range_long: String,
    max_range_long: String,
    k_long: String,
    take_k_long: String,
    stop_k_long: String,
    min_range_short: String,
    max_range_short: String,
    k_short: String,
    take_k_short: String,
    stop_k_short: String,
    tick_size: String,
    session_end_time: String,
    exit_offset_seconds: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeBreakout {
    k: String,
    stop1_range: String,
    stop2_range: String,
    big_move_threshold: String,
    min_range: String,
    min_range_mode: String,
    exclude_weekends: bool,
    wait_hours: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeOrchestrator {
    breakout_eod_mode: String,
    breakout_overnight_exit_time: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeHigh180 {
    min_rel_range: String,
    max_rel_range: String,
    k_long: String,
    k_short: String,
    stop_loss_mult: String,
    max_hold_minutes: u64,
    entry_end_time: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimePaperSafety {
    trade_mode: String,
    allow_live_orders: bool,
    finam_transport_attached: bool,
    broker_dispatch_attached: bool,
    runtime_live: bool,
    real_orders: bool,
}

struct NoDuplicateJson(Value);

impl<'de> Deserialize<'de> for NoDuplicateJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct NoDuplicateVisitor;

        impl<'de> Visitor<'de> for NoDuplicateVisitor {
            type Value = NoDuplicateJson;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("JSON without duplicate object keys")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::Bool(value)))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::Number(value.into())))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::Number(value.into())))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                serde_json::Number::from_f64(value)
                    .map(Value::Number)
                    .map(NoDuplicateJson)
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(NoDuplicateJson(Value::String(value.to_string())))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::String(value)))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::Null))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::Null))
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                NoDuplicateJson::deserialize(deserializer)
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<NoDuplicateJson>()? {
                    values.push(value.0);
                }
                Ok(NoDuplicateJson(Value::Array(values)))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                    let value = map.next_value::<NoDuplicateJson>()?;
                    values.insert(key, value.0);
                }
                Ok(NoDuplicateJson(Value::Object(values)))
            }
        }

        deserializer.deserialize_any(NoDuplicateVisitor)
    }
}

impl From<serde_json::Error> for Stage8bP1eSupervisorConfigError {
    fn from(error: serde_json::Error) -> Self {
        if error.to_string().contains("duplicate JSON key") {
            Self::DuplicateJsonKey
        } else {
            Self::InvalidJson
        }
    }
}

fn canonical_json_bytes(bytes: &[u8]) -> Result<Vec<u8>, Stage8bP1eSupervisorConfigError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = NoDuplicateJson::deserialize(&mut deserializer)?.0;
    deserializer
        .end()
        .map_err(|_| Stage8bP1eSupervisorConfigError::InvalidJson)?;
    serde_json::to_vec(&value).map_err(|_| Stage8bP1eSupervisorConfigError::InvalidJson)
}

fn parse_decimal(value: &str) -> Result<f64, Stage8bP1eSupervisorConfigError> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile)?;
    if !parsed.is_finite() {
        return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile);
    }
    Ok(parsed)
}

fn parse_time(value: &str) -> Result<NaiveTime, Stage8bP1eSupervisorConfigError> {
    NaiveTime::parse_from_str(value, "%H:%M:%S")
        .map_err(|_| Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile)
}

fn exact_string(value: &str, expected: &str) -> Result<String, Stage8bP1eSupervisorConfigError> {
    if value != expected {
        return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile);
    }
    Ok(value.to_string())
}

fn require_none(value: Option<String>) -> Result<Option<String>, Stage8bP1eSupervisorConfigError> {
    if value.is_some() {
        return Err(Stage8bP1eSupervisorConfigError::InvalidRuntimeProfile);
    }
    Ok(None)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn domain_hash(domain: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn lower_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        net::TcpListener,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        process::{Child, Command, Stdio},
    };

    use super::*;
    use crate::{
        stage8b_p1_imoexf_instrument_map_fingerprint_sha256, STAGE8B_P1_BROKER_ID,
        STAGE8B_P1_EXCHANGE, STAGE8B_P1_INTERNAL_SYMBOL, STAGE8B_P1_MARKET, STAGE8B_P1_STRATEGY_ID,
        STAGE8B_P1_TICK_SIZE, STAGE8B_P1_VENUE_SYMBOL,
    };

    struct RedisServer {
        child: Child,
        url: String,
    }

    impl RedisServer {
        async fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            let mut child = Command::new("redis-server")
                .args([
                    "--bind",
                    "127.0.0.1",
                    "--port",
                    &port.to_string(),
                    "--save",
                    "",
                    "--appendonly",
                    "no",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("redis-server is required for the P1-e cleanup proof");
            let url = format!("redis://127.0.0.1:{port}/");
            for _ in 0..100 {
                if let Ok(client) = redis::Client::open(url.as_str()) {
                    if let Ok(mut connection) = ConnectionManager::new(client).await {
                        let pong: redis::RedisResult<String> =
                            redis::cmd("PING").query_async(&mut connection).await;
                        if pong.as_deref() == Ok("PONG") && child.try_wait().unwrap().is_none() {
                            return Self { child, url };
                        }
                    }
                }
                tokio::time::sleep(StdDuration::from_millis(10)).await;
            }
            let _ = child.kill();
            let _ = child.wait();
            panic!("temporary Redis did not start");
        }

        async fn connection(&self) -> ConnectionManager {
            ConnectionManager::new(redis::Client::open(self.url.as_str()).unwrap())
                .await
                .unwrap()
        }
    }

    impl Drop for RedisServer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    async fn create_m10_group(
        connection: &mut ConnectionManager,
        namespace: &Stage8bP1RedisNamespace,
    ) {
        let _: String = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .arg("0-0")
            .arg("MKSTREAM")
            .query_async(connection)
            .await
            .unwrap();
    }

    async fn create_consumer(
        connection: &mut ConnectionManager,
        namespace: &Stage8bP1RedisNamespace,
        consumer: &str,
    ) {
        let created: i64 = redis::cmd("XGROUP")
            .arg("CREATECONSUMER")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .arg(consumer)
            .query_async(connection)
            .await
            .unwrap();
        assert_eq!(created, 1);
    }

    async fn consumer_inventory(
        connection: &mut ConnectionManager,
        namespace: &Stage8bP1RedisNamespace,
    ) -> StreamInfoConsumersReply {
        redis::cmd("XINFO")
            .arg("CONSUMERS")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(connection)
            .await
            .unwrap()
    }

    fn supervisor_config(parent: PathBuf) -> Stage8bP1eSupervisorConfigV1 {
        let (_, fingerprint) = Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
        Stage8bP1eSupervisorConfigV1 {
            schema_version: 1,
            runtime_profile_id: STAGE8B_P1E_RUNTIME_PROFILE_ID.to_string(),
            runtime_profile_sha256: STAGE8B_P1E_RUNTIME_PROFILE_SHA256.to_string(),
            first_boot_source_bundle_sha256: "11".repeat(32),
            redis_url: STAGE8B_P1E_REDIS_URL_IPV4.to_string(),
            redis_deployment_manifest_sha256: "22".repeat(32),
            redis_runtime_policy_id: STAGE8B_P1E_REDIS_RUNTIME_POLICY_ID.to_string(),
            redis_runtime_policy_sha256: STAGE8B_P1E_REDIS_RUNTIME_POLICY_SHA256.to_string(),
            telemetry_contract_sha256: STAGE8B_P1E_TELEMETRY_CONTRACT_SHA256.to_string(),
            health_interval_ms: 5_000,
            shutdown_grace_ms: 30_000,
            bootstrap: Stage8bP1BootstrapConfig {
                schema_version: 1,
                broker_id: STAGE8B_P1_BROKER_ID.to_string(),
                strategy_id: STAGE8B_P1_STRATEGY_ID.to_string(),
                account_id: "paper-account".to_string(),
                internal_symbol: STAGE8B_P1_INTERNAL_SYMBOL.to_string(),
                venue_symbol: STAGE8B_P1_VENUE_SYMBOL.to_string(),
                exchange: STAGE8B_P1_EXCHANGE.to_string(),
                market: STAGE8B_P1_MARKET.to_string(),
                tick_size: STAGE8B_P1_TICK_SIZE.to_string(),
                runtime_config_fingerprint_sha256: fingerprint,
                instrument_map_fingerprint_sha256:
                    stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                deployment_id: "finam-imoexf-paper-p1".to_string(),
                deployment_generation: 7,
                gateway_instance_id: "finam-imoexf-paper-gateway-1".to_string(),
                market_data_generation: 1,
                command_consumer_generation: 1,
                stage8a4_writer_issuer_public_key_hex: "33".repeat(32),
                durable_parent: parent,
            },
        }
    }

    fn durable_parent() -> PathBuf {
        let path = std::env::temp_dir().join(format!("p1e-supervisor-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path.canonicalize().unwrap()
    }

    #[test]
    fn exact_runtime_profile_builds_real_fingerprint() {
        let (runtime, fingerprint) = Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
        assert_eq!(runtime.stage5c_config_fingerprint(), fingerprint);
        assert!(is_sha256_hex(&fingerprint));
        assert_eq!(
            sha256_hex(&canonical_json_bytes(RUNTIME_PROFILE_BYTES).unwrap()),
            STAGE8B_P1E_RUNTIME_PROFILE_SHA256
        );
        assert_eq!(
            sha256_hex(&canonical_json_bytes(REDIS_RUNTIME_POLICY_BYTES).unwrap()),
            STAGE8B_P1E_REDIS_RUNTIME_POLICY_SHA256
        );
        assert_eq!(
            sha256_hex(ATOMIC_STALE_CONSUMER_DELETE_SCRIPT_V1.as_bytes()),
            "cfabc24e0563c490e9950e250fdb146f992403394d48953a1b5eb2a2ffd09b6b"
        );
    }

    #[test]
    fn config_is_exact_db15_and_derives_fresh_consumer() {
        let parent = durable_parent();
        let validated = validate_stage8b_p1e_supervisor_config_v1(
            supervisor_config(parent.clone()),
            [0xab; 16],
        )
        .unwrap();
        assert_eq!(validated.redis_url(), STAGE8B_P1E_REDIS_URL_IPV4);
        assert_eq!(
            validated.redis_config().consumer_name,
            format!("p1e-7-{}", "ab".repeat(16))
        );
        assert_eq!(validated.redis_config().claim_count, 2);
        assert_eq!(validated.redis_config().max_claim_pages, 1);
        fs::remove_dir(parent).unwrap();
    }

    fn manifest_for(plan: &Stage8bP1eRedisAttachPlanV1) -> Stage8bP1eRedisDeploymentManifestV1 {
        let namespace = plan.namespace();
        Stage8bP1eRedisDeploymentManifestV1 {
            schema_version: 1,
            domain: "moex.stage8b.p1e.redis-deployment-manifest.v1".to_string(),
            manifest_key: STAGE8B_P1E_DEPLOYMENT_MANIFEST_KEY.to_string(),
            redis_url_allowlist: vec![
                STAGE8B_P1E_REDIS_URL_IPV4.to_string(),
                STAGE8B_P1E_REDIS_URL_IPV6.to_string(),
            ],
            redis_db_index: 15,
            operational_identity_sha256: plan.operational_identity_sha256.clone(),
            runtime_config_fingerprint_sha256: plan
                .runtime_config_fingerprint_sha256
                .clone(),
            instrument_map_fingerprint_sha256:
                stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            deployment_generation: plan.deployment_generation,
            consumer_generation: plan.consumer_generation,
            namespace_digest_sha256: STAGE8B_P1E_NAMESPACE_DIGEST_SHA256.to_string(),
            keys: vec![
                manifest_key(
                    &namespace.canonical_m10_stream,
                    &[&namespace.m10_consumer_group],
                ),
                manifest_key(
                    &namespace.canonical_command_stream,
                    &[&namespace.stage7b_command_consumer_group],
                ),
                manifest_key(&namespace.canonical_ack_stream, &[]),
                manifest_key(&namespace.canonical_dlq_stream, &[]),
                manifest_key(&namespace.canonical_order_stream, &[]),
                manifest_key(&namespace.canonical_trade_stream, &[]),
                manifest_key(&namespace.canonical_position_stream, &[]),
                manifest_key(&namespace.runtime_state_stream, &[]),
                manifest_key(&namespace.health_stream, &[]),
                manifest_key(&namespace.readiness_stream, &[]),
            ],
            settlement_key_prefix: namespace.settlement_key_prefix.clone(),
            provisioning_owner: "stage8b-p1f-administrative-boundary".to_string(),
            run_mode: "verify-only".to_string(),
            run_may_create_or_repair: false,
            telemetry_write_command:
                "XADD <health-or-readiness-stream> NOMKSTREAM MAXLEN = 4096 * payload <canonical-json>"
                    .to_string(),
            manifest_write_allowed_in_p1e: false,
        }
    }

    #[test]
    fn run_parts_preserve_exact_non_secret_s05_bindings() {
        let parent = durable_parent();
        let validated = validate_stage8b_p1e_supervisor_config_v1(
            supervisor_config(parent.clone()),
            [0xcd; 16],
        )
        .unwrap();
        let (_, runtime, plan, settings) = validated.into_run_parts();
        assert_eq!(
            runtime.stage5c_config_fingerprint(),
            plan.runtime_config_fingerprint_sha256
        );
        assert_eq!(plan.consumer_name(), format!("p1e-7-{}", "cd".repeat(16)));
        assert_eq!(settings.health_interval_ms, 5_000);
        assert_eq!(settings.shutdown_grace_ms, 30_000);
        fs::remove_dir(parent).unwrap();
    }

    #[test]
    fn redis_manifest_validation_is_exact_and_verify_only() {
        let parent = durable_parent();
        let validated = validate_stage8b_p1e_supervisor_config_v1(
            supervisor_config(parent.clone()),
            [0xef; 16],
        )
        .unwrap();
        let (_, _, plan, _) = validated.into_run_parts();
        let manifest = manifest_for(&plan);
        assert_eq!(validate_redis_deployment_manifest(&plan, &manifest), Ok(()));

        let mut create_enabled = manifest.clone();
        create_enabled.run_may_create_or_repair = true;
        assert_eq!(
            validate_redis_deployment_manifest(&plan, &create_enabled),
            Err(Stage8bP1eRedisControlError::ManifestMismatch)
        );

        let mut extra_group = manifest.clone();
        extra_group.keys[0].groups.push("unexpected".to_string());
        assert_eq!(
            validate_redis_deployment_manifest(&plan, &extra_group),
            Err(Stage8bP1eRedisControlError::ManifestMismatch)
        );
        fs::remove_dir(parent).unwrap();
    }

    #[test]
    fn restart_classifier_inventory_matches_the_accepted_matrix() {
        let expected: BTreeSet<String> =
            include_str!("../../../docs/stage-8/stage8b-p1e-restart-continuation-matrix-v3.csv")
                .lines()
                .skip(1)
                .filter_map(|line| line.split(',').next())
                .map(str::to_string)
                .collect();
        let actual: BTreeSet<String> = Stage8bP1eRestartKindV1::ALL
            .iter()
            .map(|kind| {
                serde_json::to_value(kind)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();

        assert_eq!(actual, expected);
        assert_eq!(Stage8bP1eRestartKindV1::ALL.len(), 22);
        assert!(!Stage8bP1eRestartKindV1::Stage8a4I3Pending.permits_redis_attach());
        assert!(!Stage8bP1eRestartKindV1::Blocked.permits_redis_attach());
        assert!(Stage8bP1eRestartKindV1::Ready.permits_redis_attach());
    }

    #[test]
    fn config_rejects_redis_alias_and_db0_before_connection() {
        for url in [
            "redis://localhost:6379/15",
            "redis://127.0.0.1:6379/0",
            "REDIS://127.0.0.1:6379/15",
            "redis://127.0.0.1/15",
            "redis://127.0.0.1:6379/015",
            "redis://127.0.0.1:6379/15?x=1",
        ] {
            let mut config = supervisor_config(std::env::temp_dir());
            config.redis_url = url.to_string();
            assert!(matches!(
                validate_stage8b_p1e_supervisor_config_v1(config, [1; 16]),
                Err(Stage8bP1eSupervisorConfigError::InvalidConfig)
            ));
        }
    }

    #[test]
    fn parser_rejects_duplicate_and_unknown_keys() {
        assert!(matches!(
            parse_stage8b_p1e_supervisor_config_v1(br#"{"schema_version":1,"schema_version":1}"#),
            Err(Stage8bP1eSupervisorConfigError::DuplicateJsonKey)
        ));
        assert!(matches!(
            parse_stage8b_p1e_supervisor_config_v1(br#"{"unexpected":1}"#),
            Err(Stage8bP1eSupervisorConfigError::InvalidJson)
        ));
    }

    #[test]
    fn paper_ready_requires_every_positive_predicate() {
        let ready = Stage8bP1eReadinessInputsV1 {
            owner_live: true,
            durable_ready: true,
            redis_manifest_verified: true,
            claim_scan_complete: true,
            source_poll_fresh: true,
            settlement_healthy: true,
            pel_count: 0,
            unresolved_lifecycle_count: 0,
            blocked_request_count: 0,
            shutdown_phase: Stage8bP1eShutdownPhaseV1::Clear,
            telemetry_healthy: true,
            signal_task_healthy: true,
            grace_expired: false,
        };
        assert_eq!(
            stage8b_p1e_readiness_v1(&ready),
            (Stage8bP1eReadinessPhaseV1::PaperReady, Vec::new())
        );

        let mut stale = ready;
        stale.source_poll_fresh = false;
        assert_eq!(
            stage8b_p1e_readiness_v1(&stale),
            (
                Stage8bP1eReadinessPhaseV1::Degraded,
                vec![Stage8bP1eReadinessReasonV1::SourcePollStale]
            )
        );
    }

    #[test]
    fn telemetry_is_fixed_order_redacted_and_never_live_ready() {
        let envelope = stage8b_p1e_telemetry_envelope_v1(
            "moex.stage8b.p1e.readiness.v1",
            DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            [0x12; 16],
            Stage8bP1eReadinessPayloadV1 {
                phase: Stage8bP1eReadinessPhaseV1::PaperReady,
                reasons: Vec::new(),
                operational_identity_sha256: "44".repeat(32),
                deployment_generation: 1,
                runtime_config_fingerprint_sha256: "55".repeat(32),
                durable_seal_generation: 4,
                consumer_name: "p1e-1-12121212121212121212121212121212".to_string(),
                source_poll_fresh: true,
                claim_scan_complete: true,
                pel_count: 0,
                unresolved_lifecycle_count: 0,
                blocked_request_count: 0,
                shutdown_phase: Stage8bP1eShutdownPhaseV1::Clear,
                paper_only: true,
                finam_transport_attached: false,
                broker_network_dispatch_attached: false,
                runtime_live: false,
                real_orders: false,
            },
        );
        let text = String::from_utf8(envelope.canonical_bytes().unwrap()).unwrap();
        assert!(text.starts_with("{\"schema_version\":1,\"domain\":"));
        assert!(text.contains("\"phase\":\"paper_ready\""));
        assert!(!text.contains("LiveReady"));
        assert!(!text.contains("redis://"));
        assert_eq!(
            stage8b_p1e_redact_account_id("7502MIW"),
            domain_hash("moex.stage8b.p1e.redact.account.v1", "7502MIW")
        );
    }

    #[tokio::test]
    async fn cleanup_atomically_retains_consumer_that_gains_pending_after_discovery() {
        let redis = RedisServer::start().await;
        let namespace = stage8b_p1_redis_namespace();
        let mut setup = redis.connection().await;
        create_m10_group(&mut setup, &namespace).await;
        create_consumer(&mut setup, &namespace, "stale-race").await;
        let source_id: String = redis::cmd("XADD")
            .arg(&namespace.canonical_m10_stream)
            .arg("*")
            .arg("payload")
            .arg("race-source")
            .query_async(&mut setup)
            .await
            .unwrap();

        let race_url = redis.url.clone();
        let race_stream = namespace.canonical_m10_stream.clone();
        let race_group = namespace.m10_consumer_group.clone();
        let mut control = Stage8bP1eRedisControlV1 {
            connection: redis.connection().await,
            namespace: namespace.clone(),
        };
        let report = control
            .clean_stale_zero_pending_consumers_inner("current", 0, move || async move {
                let mut connection = ConnectionManager::new(
                    redis::Client::open(race_url.as_str())
                        .map_err(|_| Stage8bP1eRedisControlError::Redis)?,
                )
                .await
                .map_err(|_| Stage8bP1eRedisControlError::Redis)?;
                let reply: redis::streams::StreamReadReply = redis::cmd("XREADGROUP")
                    .arg("GROUP")
                    .arg(&race_group)
                    .arg("stale-race")
                    .arg("COUNT")
                    .arg(1)
                    .arg("STREAMS")
                    .arg(&race_stream)
                    .arg(">")
                    .query_async(&mut connection)
                    .await
                    .map_err(|_| Stage8bP1eRedisControlError::Redis)?;
                if reply.keys.len() != 1 {
                    return Err(Stage8bP1eRedisControlError::Redis);
                }
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(report.deleted_count, 0);

        let mut verify = redis.connection().await;
        let inventory = consumer_inventory(&mut verify, &namespace).await;
        let retained = inventory
            .consumers
            .iter()
            .find(|consumer| consumer.name == "stale-race")
            .unwrap();
        assert_eq!(retained.pending, 1);
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut verify)
            .await
            .unwrap();
        assert_eq!(pending.count(), 1);
        let StreamPendingReply::Data(pending) = pending else {
            panic!("race source must remain in the exact PEL");
        };
        assert_eq!(pending.start_id, source_id);
        assert_eq!(pending.end_id, source_id);
        assert_eq!(pending.consumers.len(), 1);
        assert_eq!(pending.consumers[0].name, "stale-race");
        assert_eq!(pending.consumers[0].pending, 1);
    }

    #[tokio::test]
    async fn cleanup_deletes_only_rechecked_zero_pending_idle_consumers() {
        let redis = RedisServer::start().await;
        let namespace = stage8b_p1_redis_namespace();
        let mut setup = redis.connection().await;
        create_m10_group(&mut setup, &namespace).await;
        create_consumer(&mut setup, &namespace, "current").await;
        create_consumer(&mut setup, &namespace, "eligible").await;
        create_consumer(&mut setup, &namespace, "too-young").await;

        let mut control = Stage8bP1eRedisControlV1 {
            connection: redis.connection().await,
            namespace: namespace.clone(),
        };
        let retained = control
            .clean_stale_zero_pending_consumers_inner("current", usize::MAX, || async { Ok(()) })
            .await
            .unwrap();
        assert_eq!(retained.deleted_count, 0);

        let deleted = control
            .clean_stale_zero_pending_consumers_inner("current", 0, || async { Ok(()) })
            .await
            .unwrap();
        assert_eq!(deleted.deleted_count, 2);
        let inventory = consumer_inventory(&mut control.connection, &namespace).await;
        assert_eq!(inventory.consumers.len(), 1);
        assert_eq!(inventory.consumers[0].name, "current");
    }

    #[tokio::test]
    async fn cleanup_rejects_unbounded_inventory_without_deletion() {
        let redis = RedisServer::start().await;
        let namespace = stage8b_p1_redis_namespace();
        let mut setup = redis.connection().await;
        create_m10_group(&mut setup, &namespace).await;
        for index in 0..=STAGE8B_P1E_STALE_CONSUMER_INVENTORY_MAX {
            create_consumer(&mut setup, &namespace, &format!("inventory-{index:03}")).await;
        }
        let mut control = Stage8bP1eRedisControlV1 {
            connection: redis.connection().await,
            namespace: namespace.clone(),
        };
        assert_eq!(
            control
                .clean_stale_zero_pending_consumers_inner("current", 0, || async { Ok(()) })
                .await,
            Err(Stage8bP1eRedisControlError::ConsumerInventoryInvalid)
        );
        let inventory = consumer_inventory(&mut control.connection, &namespace).await;
        assert_eq!(
            inventory.consumers.len(),
            STAGE8B_P1E_STALE_CONSUMER_INVENTORY_MAX + 1
        );
    }

    #[tokio::test]
    async fn cleanup_uses_a_deterministic_bounded_prefix_of_sixteen() {
        let redis = RedisServer::start().await;
        let namespace = stage8b_p1_redis_namespace();
        let mut setup = redis.connection().await;
        create_m10_group(&mut setup, &namespace).await;
        for index in 0..18 {
            create_consumer(&mut setup, &namespace, &format!("bounded-{index:02}")).await;
        }
        let mut control = Stage8bP1eRedisControlV1 {
            connection: redis.connection().await,
            namespace: namespace.clone(),
        };
        let first = control
            .clean_stale_zero_pending_consumers_inner("current", 0, || async { Ok(()) })
            .await
            .unwrap();
        assert_eq!(first.inventory_count, 18);
        assert_eq!(first.examined_count, STAGE8B_P1E_STALE_CONSUMER_EXAMINE_MAX);
        assert_eq!(first.deleted_count, STAGE8B_P1E_STALE_CONSUMER_EXAMINE_MAX);

        let second = control
            .clean_stale_zero_pending_consumers_inner("current", 0, || async { Ok(()) })
            .await
            .unwrap();
        assert_eq!(second.inventory_count, 2);
        assert_eq!(second.examined_count, 2);
        assert_eq!(second.deleted_count, 2);
    }

    #[test]
    fn coordinator_preserves_error_cause_through_authenticated_boundary() {
        for (event, cause, exit_code) in [
            (
                Stage8bP1eSupervisorEventV1::OwnerReturnedUnexpectedly,
                Stage8bP1eShutdownCauseV1::OwnerFailure,
                70,
            ),
            (
                Stage8bP1eSupervisorEventV1::TelemetryFailed,
                Stage8bP1eShutdownCauseV1::TelemetryFailure,
                71,
            ),
            (
                Stage8bP1eSupervisorEventV1::SignalTaskFailed,
                Stage8bP1eShutdownCauseV1::SignalTaskFailure,
                73,
            ),
        ] {
            let mut coordinator = Stage8bP1eCoordinatorV1::new();
            let draining = coordinator.coordinate(event, true, 1_000, 2_000, 41);
            assert_eq!(
                draining.action,
                Stage8bP1eCoordinatorActionV1::DrainToAuthenticatedBoundary
            );
            assert_eq!(draining.exit_code, None);
            assert_eq!(draining.retained_shutdown_cause, Some(cause));

            let completed = coordinator.coordinate(
                Stage8bP1eSupervisorEventV1::AuthenticatedBoundaryReached,
                true,
                1_500,
                9_999,
                99,
            );
            assert_eq!(
                completed.action,
                Stage8bP1eCoordinatorActionV1::CompleteUsingRetainedIntent
            );
            assert_eq!(completed.exit_code, Some(exit_code));
            assert_eq!(completed.retained_shutdown_cause, Some(cause));
            assert_eq!(completed.first_request_sequence, Some(41));
            assert_eq!(completed.grace_deadline_utc_ms, Some(2_000));
        }
    }

    #[test]
    fn coordinator_preserves_external_signal_and_first_request() {
        let mut coordinator = Stage8bP1eCoordinatorV1::new();
        let signal = coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::ExternalSignal,
            true,
            1_000,
            2_000,
            17,
        );
        assert_eq!(signal.exit_code, None);
        assert_eq!(
            signal.retained_shutdown_cause,
            Some(Stage8bP1eShutdownCauseV1::ExternalSignal)
        );

        let repeated = coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::TelemetryFailed,
            true,
            1_100,
            8_000,
            18,
        );
        assert_eq!(
            repeated.retained_shutdown_cause,
            Some(Stage8bP1eShutdownCauseV1::ExternalSignal)
        );
        assert_eq!(repeated.first_request_sequence, Some(17));
        assert_eq!(repeated.grace_deadline_utc_ms, Some(2_000));

        let completed = coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::AuthenticatedBoundaryReached,
            true,
            1_500,
            9_000,
            19,
        );
        assert_eq!(completed.exit_code, Some(0));
        assert_eq!(completed.terminal_failure, None);
        assert_eq!(completed.first_request_sequence, Some(17));
    }

    #[test]
    fn coordinator_external_signal_then_owner_panic_is_fatal_without_losing_diagnostics() {
        let mut coordinator = Stage8bP1eCoordinatorV1::new();
        coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::ExternalSignal,
            true,
            1_000,
            2_000,
            17,
        );
        let fatal = coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::OwnerPanicked,
            false,
            1_100,
            8_000,
            18,
        );
        assert_eq!(
            fatal.action,
            Stage8bP1eCoordinatorActionV1::CompleteUsingRetainedIntent
        );
        assert_eq!(fatal.exit_code, Some(70));
        assert_eq!(
            fatal.terminal_failure,
            Some(Stage8bP1eTerminalFailureV1::OwnerLost)
        );
        assert!(!fatal.owner_may_bounded_drain);
        assert_eq!(
            fatal.retained_shutdown_cause,
            Some(Stage8bP1eShutdownCauseV1::ExternalSignal)
        );
        assert_eq!(fatal.first_request_sequence, Some(17));
        assert_eq!(fatal.grace_deadline_utc_ms, Some(2_000));
    }

    #[test]
    fn coordinator_external_signal_then_ownerless_return_is_fatal() {
        let mut coordinator = Stage8bP1eCoordinatorV1::new();
        coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::ExternalSignal,
            true,
            1_000,
            2_000,
            21,
        );
        let fatal = coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::OwnerReturnedWithoutOwner,
            false,
            1_100,
            8_000,
            22,
        );
        assert_eq!(fatal.exit_code, Some(70));
        assert_eq!(
            fatal.terminal_failure,
            Some(Stage8bP1eTerminalFailureV1::OwnerLost)
        );
        assert_eq!(
            fatal.retained_shutdown_cause,
            Some(Stage8bP1eShutdownCauseV1::ExternalSignal)
        );
        assert_eq!(fatal.first_request_sequence, Some(21));
        assert_eq!(fatal.grace_deadline_utc_ms, Some(2_000));
    }

    #[test]
    fn coordinator_telemetry_failure_then_owner_panic_retains_initiating_diagnostics() {
        let mut coordinator = Stage8bP1eCoordinatorV1::new();
        coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::TelemetryFailed,
            true,
            1_000,
            2_000,
            31,
        );
        let fatal = coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::OwnerPanicked,
            false,
            1_100,
            8_000,
            32,
        );
        assert_eq!(fatal.exit_code, Some(70));
        assert_eq!(
            fatal.terminal_failure,
            Some(Stage8bP1eTerminalFailureV1::OwnerLost)
        );
        assert_eq!(
            fatal.retained_shutdown_cause,
            Some(Stage8bP1eShutdownCauseV1::TelemetryFailure)
        );
        assert_eq!(fatal.first_request_sequence, Some(31));
        assert_eq!(fatal.grace_deadline_utc_ms, Some(2_000));
    }

    #[test]
    fn coordinator_owner_loss_without_prior_shutdown_records_owner_diagnostics() {
        for event in [
            Stage8bP1eSupervisorEventV1::OwnerPanicked,
            Stage8bP1eSupervisorEventV1::OwnerReturnedWithoutOwner,
        ] {
            let mut coordinator = Stage8bP1eCoordinatorV1::new();
            let fatal = coordinator.coordinate(event, false, 1_000, 2_000, 41);
            assert_eq!(fatal.exit_code, Some(70));
            assert_eq!(
                fatal.terminal_failure,
                Some(Stage8bP1eTerminalFailureV1::OwnerLost)
            );
            assert_eq!(
                fatal.retained_shutdown_cause,
                Some(Stage8bP1eShutdownCauseV1::OwnerFailure)
            );
            assert_eq!(fatal.first_request_sequence, Some(41));
            assert_eq!(fatal.grace_deadline_utc_ms, Some(2_000));
        }
    }

    #[test]
    fn coordinator_observed_owner_loss_precedes_unprocessed_elapsed_grace() {
        let mut coordinator = Stage8bP1eCoordinatorV1::new();
        coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::ExternalSignal,
            true,
            1_000,
            2_000,
            51,
        );
        let fatal = coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::OwnerPanicked,
            false,
            2_100,
            9_000,
            52,
        );
        assert_eq!(fatal.exit_code, Some(70));
        assert_eq!(
            fatal.terminal_failure,
            Some(Stage8bP1eTerminalFailureV1::OwnerLost)
        );
        assert_eq!(fatal.first_request_sequence, Some(51));
        assert_eq!(fatal.grace_deadline_utc_ms, Some(2_000));
    }

    #[test]
    fn coordinator_grace_expiry_keeps_initiating_diagnostics() {
        let mut coordinator = Stage8bP1eCoordinatorV1::new();
        coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::TelemetryFailed,
            true,
            1_000,
            2_000,
            23,
        );
        let expired = coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::GraceExpired,
            true,
            2_000,
            7_000,
            24,
        );
        assert_eq!(expired.exit_code, Some(72));
        assert_eq!(
            expired.retained_shutdown_cause,
            Some(Stage8bP1eShutdownCauseV1::TelemetryFailure)
        );
        assert_eq!(expired.first_request_sequence, Some(23));
        assert_eq!(expired.grace_deadline_utc_ms, Some(2_000));
    }

    #[test]
    fn authenticated_boundary_without_shutdown_keeps_owner_running() {
        let mut coordinator = Stage8bP1eCoordinatorV1::new();
        let decision = coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::AuthenticatedBoundaryReached,
            true,
            1_000,
            2_000,
            1,
        );
        assert_eq!(
            decision.action,
            Stage8bP1eCoordinatorActionV1::ContinueOwnerLoop
        );
        assert_eq!(decision.exit_code, None);
        assert!(!decision.request_shutdown);

        let stray_grace = coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::GraceExpired,
            true,
            2_000,
            2_000,
            2,
        );
        assert_eq!(
            stray_grace.action,
            Stage8bP1eCoordinatorActionV1::ContinueOwnerLoop
        );
        assert_eq!(stray_grace.exit_code, None);
    }
}
