//! Fixed Stage 8B-P1F Id Redis roles, resource limits and redacted audit.
//!
//! This is a closed composition for the accepted P1F design.  It is not a
//! general ACL/policy engine: the eight roles, ten source operations and eight
//! Lua identities are exhaustive enums and no raw Redis connection escapes.

use std::{
    collections::VecDeque,
    ffi::CString,
    sync::{Arc, Mutex},
    time::Duration,
};

use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    attach_stage8b_p1_redis, initialize_stage8b_p1_redis_namespace, stage8b_p1_redis_namespace,
    Stage8bP1RedisConfig, Stage8bP1RedisM10PublishDisposition,
    Stage8bP1RedisSemanticCompositionTransport, Stage8bP1RedisSemanticError,
    Stage8bP1eCoordinatorDecisionV1, Stage8bP1eCoordinatorV1, Stage8bP1eRedisControlError,
    Stage8bP1eShutdownCauseV1, Stage8bP1eShutdownIntentV1, Stage8bP1eShutdownLatchV1,
    Stage8bP1eSupervisorEventV1, STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS,
    STAGE8B_P1E_REDIS_URL_IPV4, STAGE8B_P1E_REDIS_URL_IPV6,
};

pub const STAGE8B_P1F_REDIS_DATABASE: u8 = 15;
pub const STAGE8B_P1F_REDIS_ROLE_COUNT: usize = 8;
pub const STAGE8B_P1F_REDIS_SOURCE_OPERATION_COUNT: usize = 10;
pub const STAGE8B_P1F_REDIS_SCRIPT_COUNT: usize = 8;
pub const STAGE8B_P1F_COMMAND_AUDIT_CAPACITY: usize = 4_096;
pub const STAGE8B_P1F_RESOURCE_POLL_INTERVAL_SECONDS: u64 = 5;
pub const STAGE8B_P1F_TOTAL_PEL_FAIL_STOP_THRESHOLD: u64 = 64;
pub const STAGE8B_P1F_DB15_EVIDENCE_BUDGET_BYTES: u64 = 536_870_912;
pub const STAGE8B_P1F_MINIMUM_ROOT_FREE_BYTES: u64 = 10_737_418_240;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage8bP1fRedisRoleV1 {
    Provisioner,
    SyntheticM10Feeder,
    FinamBarsFeeder,
    SchedulePublisher,
    Supervisor,
    ReadOnlyAuditor,
    PhaseGuardian,
    BrokerTruthObserver,
}

pub const STAGE8B_P1F_REDIS_ROLES: [Stage8bP1fRedisRoleV1; STAGE8B_P1F_REDIS_ROLE_COUNT] = [
    Stage8bP1fRedisRoleV1::Provisioner,
    Stage8bP1fRedisRoleV1::SyntheticM10Feeder,
    Stage8bP1fRedisRoleV1::FinamBarsFeeder,
    Stage8bP1fRedisRoleV1::SchedulePublisher,
    Stage8bP1fRedisRoleV1::Supervisor,
    Stage8bP1fRedisRoleV1::ReadOnlyAuditor,
    Stage8bP1fRedisRoleV1::PhaseGuardian,
    Stage8bP1fRedisRoleV1::BrokerTruthObserver,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1fRedisDatabaseScopeV1 {
    Db15,
    Db0And15ReadOnly,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stage8bP1fRedisRoleContractV1 {
    pub role: Stage8bP1fRedisRoleV1,
    pub database_scope: Stage8bP1fRedisDatabaseScopeV1,
    pub allowed_commands: &'static [&'static str],
}

pub fn stage8b_p1f_redis_role_contract_v1(
    role: Stage8bP1fRedisRoleV1,
) -> Stage8bP1fRedisRoleContractV1 {
    let (database_scope, allowed_commands): (_, &'static [&'static str]) = match role {
        Stage8bP1fRedisRoleV1::Provisioner => (
            Stage8bP1fRedisDatabaseScopeV1::Db15,
            &[
                "PING",
                "TYPE exact-key",
                "GET deployment-manifest",
                "SET deployment-manifest NX exact-canonical-bytes",
                "EVAL namespace-initialization-v1 exact-keys-argv",
                "EVAL namespace-verify-v1 exact-keys-argv",
                "XADD exact-nonconsumed-provisioning-marker once",
                "XINFO exact-stream-or-group",
            ],
        ),
        Stage8bP1fRedisRoleV1::SyntheticM10Feeder | Stage8bP1fRedisRoleV1::FinamBarsFeeder => (
            Stage8bP1fRedisDatabaseScopeV1::Db15,
            &[
                "PING",
                "EVAL namespace-verify-v1 exact-keys-argv",
                "EVAL m10-publication-v1 exact-key-argv",
                "XRANGE canonical-m10 exact-id exact-id",
                "XLEN canonical-m10",
            ],
        ),
        Stage8bP1fRedisRoleV1::SchedulePublisher => (
            Stage8bP1fRedisDatabaseScopeV1::Db15,
            &[
                "PING",
                "TYPE market-schedule",
                "XADD market-schedule NOMKSTREAM MAXLEN=4096 payload",
            ],
        ),
        Stage8bP1fRedisRoleV1::Supervisor => (
            Stage8bP1fRedisDatabaseScopeV1::Db15,
            &[
                "PING",
                "TYPE exact-key",
                "GET exact-marker-or-manifest",
                "SET exact-settlement-marker",
                "XINFO exact-stream-group-or-consumer bounded",
                "XPENDING exact-M10-group bounded",
                "XAUTOCLAIM exact-M10-group exact-id bounded",
                "XREADGROUP exact-M10-group bounded",
                "XRANGE exact-source-lifecycle-or-command id-bounded",
                "XREVRANGE market-schedule + - COUNT 64",
                "XLEN canonical-m10",
                "XADD exact-command-lifecycle-telemetry NOMKSTREAM bounded",
                "EVAL namespace-verify-v1 exact-keys-argv",
                "EVAL one-of-four-command-publication-scripts exact-keys-argv",
                "EVAL atomic-stale-consumer-delete-v1 exact-key-argv",
                "XACK exact-source-group last",
            ],
        ),
        Stage8bP1fRedisRoleV1::ReadOnlyAuditor => (
            Stage8bP1fRedisDatabaseScopeV1::Db0And15ReadOnly,
            &[
                "PING",
                "INFO persistence-or-memory",
                "TYPE",
                "EXISTS",
                "SCAN bounded",
                "GET",
                "XLEN",
                "XRANGE bounded",
                "XREVRANGE bounded",
                "XINFO",
                "XPENDING",
                "PTTL",
                "DUMP hash-only",
            ],
        ),
        Stage8bP1fRedisRoleV1::PhaseGuardian | Stage8bP1fRedisRoleV1::BrokerTruthObserver => {
            (Stage8bP1fRedisDatabaseScopeV1::None, &[])
        }
    };
    Stage8bP1fRedisRoleContractV1 {
        role,
        database_scope,
        allowed_commands,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage8bP1fRedisSourceOperationV1 {
    FreshNamespace,
    VerifyOnlyAttach,
    M10Publish,
    RetentionAdmission,
    ScheduleRead,
    StaleConsumerDiscovery,
    StaleConsumerCleanup,
    SourceAcquireAndReclaim,
    CommandPublication,
    SourceXackLast,
}

pub const STAGE8B_P1F_REDIS_SOURCE_OPERATIONS: [Stage8bP1fRedisSourceOperationV1;
    STAGE8B_P1F_REDIS_SOURCE_OPERATION_COUNT] = [
    Stage8bP1fRedisSourceOperationV1::FreshNamespace,
    Stage8bP1fRedisSourceOperationV1::VerifyOnlyAttach,
    Stage8bP1fRedisSourceOperationV1::M10Publish,
    Stage8bP1fRedisSourceOperationV1::RetentionAdmission,
    Stage8bP1fRedisSourceOperationV1::ScheduleRead,
    Stage8bP1fRedisSourceOperationV1::StaleConsumerDiscovery,
    Stage8bP1fRedisSourceOperationV1::StaleConsumerCleanup,
    Stage8bP1fRedisSourceOperationV1::SourceAcquireAndReclaim,
    Stage8bP1fRedisSourceOperationV1::CommandPublication,
    Stage8bP1fRedisSourceOperationV1::SourceXackLast,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stage8bP1fRedisSourceOperationContractV1 {
    pub operation: Stage8bP1fRedisSourceOperationV1,
    pub command_and_bounds: &'static str,
    pub key_and_argv_contract: &'static str,
}

pub fn stage8b_p1f_redis_source_operation_contract_v1(
    operation: Stage8bP1fRedisSourceOperationV1,
) -> Stage8bP1fRedisSourceOperationContractV1 {
    let (command_and_bounds, key_and_argv_contract) = match operation {
        Stage8bP1fRedisSourceOperationV1::FreshNamespace => (
            "ordered EVAL namespace-initialization-v1 then namespace-verify-v1",
            "same exact M10/command keys and exact two group argv",
        ),
        Stage8bP1fRedisSourceOperationV1::VerifyOnlyAttach => (
            "EVAL namespace-verify-v1",
            "exact M10/command keys and exact two group argv",
        ),
        Stage8bP1fRedisSourceOperationV1::M10Publish => (
            "EVAL m10-publication-v1 then XRANGE exact-id",
            "exact M10 key; exact group redis-id canonical-payload argv",
        ),
        Stage8bP1fRedisSourceOperationV1::RetentionAdmission => ("XLEN", "exact canonical M10 key"),
        Stage8bP1fRedisSourceOperationV1::ScheduleRead => (
            "XREVRANGE + - COUNT 64",
            "exact market-schedule key and fixed bound 64",
        ),
        Stage8bP1fRedisSourceOperationV1::StaleConsumerDiscovery => (
            "XINFO CONSUMERS inventory<=64 examine<=16",
            "exact M10 key and group",
        ),
        Stage8bP1fRedisSourceOperationV1::StaleConsumerCleanup => (
            "EVAL atomic-stale-consumer-delete-v1",
            "exact M10 key; exact group consumer minimum-idle=86400000 argv",
        ),
        Stage8bP1fRedisSourceOperationV1::SourceAcquireAndReclaim => (
            "XPENDING then exact XAUTOCLAIM before bounded XREADGROUP",
            "exact M10 key/group/id and accepted count bounds",
        ),
        Stage8bP1fRedisSourceOperationV1::CommandPublication => (
            "EVAL one of four pinned command publication/revalidation scripts",
            "exact source command marker keys and accepted script-specific argv",
        ),
        Stage8bP1fRedisSourceOperationV1::SourceXackLast => (
            "XACK only after durable truth reread",
            "exact source key group and id",
        ),
    };
    Stage8bP1fRedisSourceOperationContractV1 {
        operation,
        command_and_bounds,
        key_and_argv_contract,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage8bP1fRedisScriptV1 {
    NamespaceInitializationV1,
    NamespaceVerifyV1,
    M10PublicationV1,
    CommandPublicationV1,
    CommandPublicationRevalidateV1,
    P1d4CommandPublicationV1,
    P1d4CommandPublicationRevalidateV1,
    AtomicStaleConsumerDeleteV1,
}

impl Stage8bP1fRedisScriptV1 {
    pub const fn sha256(self) -> &'static str {
        match self {
            Self::NamespaceInitializationV1 => {
                "c7db9e1660bb6519ab7502b63cc695ce6d3a5f453be248b4aaad4ddbc614dd3e"
            }
            Self::NamespaceVerifyV1 => {
                "de29c3ca22ec8eb16925a48ada22bf1dd4fd5b6bd7fa58a0438000a0923e7d49"
            }
            Self::M10PublicationV1 => {
                "ecf60a82c8efdb92c2b4a13a86cd607dee46a216bdbf3c7207ea3208989baf54"
            }
            Self::CommandPublicationV1 => {
                "0c5b4d4cfbbe8615ed863e00bfdd3a3325e028f9fba78ee1bf2e5bb819346ce1"
            }
            Self::CommandPublicationRevalidateV1 => {
                "f0c37828d3057d77243506c2eee21e9c12f8e67a8489c42071580ba378f71993"
            }
            Self::P1d4CommandPublicationV1 => {
                "ec80534b837877db2dc992ffda1803f47ad8569c966d399edc3265fbc21b51c6"
            }
            Self::P1d4CommandPublicationRevalidateV1 => {
                "f15f0f0ef1ad1397cceaf1158e421f49bbdd8d0ad956a97ae3fe46c947108358"
            }
            Self::AtomicStaleConsumerDeleteV1 => {
                "cfabc24e0563c490e9950e250fdb146f992403394d48953a1b5eb2a2ffd09b6b"
            }
        }
    }
}

pub const STAGE8B_P1F_REDIS_SCRIPTS: [Stage8bP1fRedisScriptV1; STAGE8B_P1F_REDIS_SCRIPT_COUNT] = [
    Stage8bP1fRedisScriptV1::NamespaceInitializationV1,
    Stage8bP1fRedisScriptV1::NamespaceVerifyV1,
    Stage8bP1fRedisScriptV1::M10PublicationV1,
    Stage8bP1fRedisScriptV1::CommandPublicationV1,
    Stage8bP1fRedisScriptV1::CommandPublicationRevalidateV1,
    Stage8bP1fRedisScriptV1::P1d4CommandPublicationV1,
    Stage8bP1fRedisScriptV1::P1d4CommandPublicationRevalidateV1,
    Stage8bP1fRedisScriptV1::AtomicStaleConsumerDeleteV1,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1fRedisAuditResultV1 {
    Succeeded,
    IdempotentExisting,
    Rejected,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage8bP1fRedisAuxiliaryOperationV1 {
    FeederVerifyAttach,
    SchedulePublication,
    ResourcePoll,
    P0ReadOnlyAudit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Stage8bP1fRedisAuditedOperationV1 {
    Source(Stage8bP1fRedisSourceOperationV1),
    Auxiliary(Stage8bP1fRedisAuxiliaryOperationV1),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fRedisCommandAuditRecordV1 {
    pub schema_version: u16,
    pub sequence: u64,
    pub role: Stage8bP1fRedisRoleV1,
    pub operation: Stage8bP1fRedisAuditedOperationV1,
    pub database: u8,
    pub script_sha256: Option<String>,
    pub command_fingerprint_sha256: String,
    pub result: Stage8bP1fRedisAuditResultV1,
}

#[derive(Debug, Default)]
pub struct Stage8bP1fRedisCommandAuditV1 {
    next_sequence: u64,
    records: VecDeque<Stage8bP1fRedisCommandAuditRecordV1>,
}

impl Stage8bP1fRedisCommandAuditV1 {
    pub fn records(&self) -> &VecDeque<Stage8bP1fRedisCommandAuditRecordV1> {
        &self.records
    }

    pub fn record(
        &mut self,
        role: Stage8bP1fRedisRoleV1,
        operation: Stage8bP1fRedisSourceOperationV1,
        script: Option<Stage8bP1fRedisScriptV1>,
        exact_command_material: &[u8],
        result: Stage8bP1fRedisAuditResultV1,
    ) -> Result<(), Stage8bP1fRedisRoleErrorV1> {
        if let Err(error) = authorize_stage8b_p1f_redis_operation(role, operation) {
            self.push_record(
                role,
                Stage8bP1fRedisAuditedOperationV1::Source(operation),
                script,
                exact_command_material,
                Stage8bP1fRedisAuditResultV1::Rejected,
            )?;
            return Err(error);
        }
        if let Some(script) = script {
            if let Err(error) = authorize_script_for_operation(operation, script) {
                self.push_record(
                    role,
                    Stage8bP1fRedisAuditedOperationV1::Source(operation),
                    Some(script),
                    exact_command_material,
                    Stage8bP1fRedisAuditResultV1::Rejected,
                )?;
                return Err(error);
            }
        }
        self.push_record(
            role,
            Stage8bP1fRedisAuditedOperationV1::Source(operation),
            script,
            exact_command_material,
            result,
        )
    }

    fn push_record(
        &mut self,
        role: Stage8bP1fRedisRoleV1,
        operation: Stage8bP1fRedisAuditedOperationV1,
        script: Option<Stage8bP1fRedisScriptV1>,
        exact_command_material: &[u8],
        result: Stage8bP1fRedisAuditResultV1,
    ) -> Result<(), Stage8bP1fRedisRoleErrorV1> {
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(Stage8bP1fRedisRoleErrorV1::AuditOverflow)?;
        if self.records.len() == STAGE8B_P1F_COMMAND_AUDIT_CAPACITY {
            self.records.pop_front();
        }
        self.records.push_back(Stage8bP1fRedisCommandAuditRecordV1 {
            schema_version: 1,
            sequence: self.next_sequence,
            role,
            operation,
            database: STAGE8B_P1F_REDIS_DATABASE,
            script_sha256: script.map(|value| value.sha256().to_string()),
            command_fingerprint_sha256: sha256_hex(exact_command_material),
            result,
        });
        Ok(())
    }

    pub fn record_auxiliary(
        &mut self,
        role: Stage8bP1fRedisRoleV1,
        operation: Stage8bP1fRedisAuxiliaryOperationV1,
        script: Option<Stage8bP1fRedisScriptV1>,
        exact_command_material: &[u8],
        result: Stage8bP1fRedisAuditResultV1,
    ) -> Result<(), Stage8bP1fRedisRoleErrorV1> {
        let allowed = matches!(
            (role, operation),
            (
                Stage8bP1fRedisRoleV1::SyntheticM10Feeder | Stage8bP1fRedisRoleV1::FinamBarsFeeder,
                Stage8bP1fRedisAuxiliaryOperationV1::FeederVerifyAttach
            ) | (
                Stage8bP1fRedisRoleV1::SchedulePublisher,
                Stage8bP1fRedisAuxiliaryOperationV1::SchedulePublication
            ) | (
                Stage8bP1fRedisRoleV1::ReadOnlyAuditor,
                Stage8bP1fRedisAuxiliaryOperationV1::ResourcePoll
            ) | (
                Stage8bP1fRedisRoleV1::ReadOnlyAuditor,
                Stage8bP1fRedisAuxiliaryOperationV1::P0ReadOnlyAudit
            )
        );
        if !allowed {
            self.push_record(
                role,
                Stage8bP1fRedisAuditedOperationV1::Auxiliary(operation),
                script,
                exact_command_material,
                Stage8bP1fRedisAuditResultV1::Rejected,
            )?;
            return Err(Stage8bP1fRedisRoleErrorV1::ForbiddenOperation);
        }
        let script_allowed = matches!(
            (operation, script),
            (
                Stage8bP1fRedisAuxiliaryOperationV1::FeederVerifyAttach,
                Some(Stage8bP1fRedisScriptV1::NamespaceVerifyV1)
            ) | (
                Stage8bP1fRedisAuxiliaryOperationV1::SchedulePublication
                    | Stage8bP1fRedisAuxiliaryOperationV1::ResourcePoll
                    | Stage8bP1fRedisAuxiliaryOperationV1::P0ReadOnlyAudit,
                None
            )
        );
        if !script_allowed {
            self.push_record(
                role,
                Stage8bP1fRedisAuditedOperationV1::Auxiliary(operation),
                script,
                exact_command_material,
                Stage8bP1fRedisAuditResultV1::Rejected,
            )?;
            return Err(Stage8bP1fRedisRoleErrorV1::ForbiddenScript);
        }
        self.push_record(
            role,
            Stage8bP1fRedisAuditedOperationV1::Auxiliary(operation),
            script,
            exact_command_material,
            result,
        )
    }
}

/// Cloneable redacted audit sink shared by the already-existing Redis
/// capabilities.  It contains no Redis connection or execution authority.
#[derive(Debug, Clone, Default)]
pub struct Stage8bP1fRedisCommandAuditHandleV1 {
    inner: Arc<Mutex<Stage8bP1fRedisCommandAuditV1>>,
}

impl Stage8bP1fRedisCommandAuditHandleV1 {
    pub fn record(
        &self,
        role: Stage8bP1fRedisRoleV1,
        operation: Stage8bP1fRedisSourceOperationV1,
        script: Option<Stage8bP1fRedisScriptV1>,
        exact_command_material: &[u8],
        result: Stage8bP1fRedisAuditResultV1,
    ) -> Result<(), Stage8bP1fRedisRoleErrorV1> {
        self.inner
            .lock()
            .map_err(|_| Stage8bP1fRedisRoleErrorV1::AuditPoisoned)?
            .record(role, operation, script, exact_command_material, result)
    }

    pub fn record_auxiliary(
        &self,
        role: Stage8bP1fRedisRoleV1,
        operation: Stage8bP1fRedisAuxiliaryOperationV1,
        script: Option<Stage8bP1fRedisScriptV1>,
        exact_command_material: &[u8],
        result: Stage8bP1fRedisAuditResultV1,
    ) -> Result<(), Stage8bP1fRedisRoleErrorV1> {
        self.inner
            .lock()
            .map_err(|_| Stage8bP1fRedisRoleErrorV1::AuditPoisoned)?
            .record_auxiliary(role, operation, script, exact_command_material, result)
    }

    pub fn snapshot(
        &self,
    ) -> Result<Vec<Stage8bP1fRedisCommandAuditRecordV1>, Stage8bP1fRedisRoleErrorV1> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| Stage8bP1fRedisRoleErrorV1::AuditPoisoned)?
            .records()
            .iter()
            .cloned()
            .collect())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1fRedisRoleErrorV1 {
    #[error("the fixed Redis role cannot perform the requested operation")]
    ForbiddenOperation,
    #[error("the requested Lua identity is not pinned for this operation")]
    ForbiddenScript,
    #[error("the Redis endpoint is not the fixed loopback DB15 endpoint")]
    WrongRedisEndpoint,
    #[error("the bounded command audit sequence overflowed")]
    AuditOverflow,
    #[error("the bounded command audit lock was poisoned")]
    AuditPoisoned,
    #[error("the exact M10 publication receipt conflicts with retained bytes")]
    PublicationConflict,
    #[error("the bounded resource probe failed")]
    ResourceProbe,
    #[error(transparent)]
    Redis(#[from] Stage8bP1RedisSemanticError),
    #[error(transparent)]
    RedisCommand(#[from] redis::RedisError),
    #[error(transparent)]
    RedisControl(#[from] Stage8bP1eRedisControlError),
}

pub fn authorize_stage8b_p1f_redis_operation(
    role: Stage8bP1fRedisRoleV1,
    operation: Stage8bP1fRedisSourceOperationV1,
) -> Result<(), Stage8bP1fRedisRoleErrorV1> {
    let allowed = match operation {
        Stage8bP1fRedisSourceOperationV1::FreshNamespace => {
            role == Stage8bP1fRedisRoleV1::Provisioner
        }
        Stage8bP1fRedisSourceOperationV1::M10Publish => matches!(
            role,
            Stage8bP1fRedisRoleV1::SyntheticM10Feeder | Stage8bP1fRedisRoleV1::FinamBarsFeeder
        ),
        Stage8bP1fRedisSourceOperationV1::VerifyOnlyAttach
        | Stage8bP1fRedisSourceOperationV1::RetentionAdmission
        | Stage8bP1fRedisSourceOperationV1::ScheduleRead
        | Stage8bP1fRedisSourceOperationV1::StaleConsumerDiscovery
        | Stage8bP1fRedisSourceOperationV1::StaleConsumerCleanup
        | Stage8bP1fRedisSourceOperationV1::SourceAcquireAndReclaim
        | Stage8bP1fRedisSourceOperationV1::CommandPublication
        | Stage8bP1fRedisSourceOperationV1::SourceXackLast => {
            role == Stage8bP1fRedisRoleV1::Supervisor
        }
    };
    allowed
        .then_some(())
        .ok_or(Stage8bP1fRedisRoleErrorV1::ForbiddenOperation)
}

fn authorize_script_for_operation(
    operation: Stage8bP1fRedisSourceOperationV1,
    script: Stage8bP1fRedisScriptV1,
) -> Result<(), Stage8bP1fRedisRoleErrorV1> {
    let allowed = match operation {
        Stage8bP1fRedisSourceOperationV1::FreshNamespace => matches!(
            script,
            Stage8bP1fRedisScriptV1::NamespaceInitializationV1
                | Stage8bP1fRedisScriptV1::NamespaceVerifyV1
        ),
        Stage8bP1fRedisSourceOperationV1::VerifyOnlyAttach => {
            script == Stage8bP1fRedisScriptV1::NamespaceVerifyV1
        }
        Stage8bP1fRedisSourceOperationV1::M10Publish => {
            script == Stage8bP1fRedisScriptV1::M10PublicationV1
        }
        Stage8bP1fRedisSourceOperationV1::StaleConsumerCleanup => {
            script == Stage8bP1fRedisScriptV1::AtomicStaleConsumerDeleteV1
        }
        Stage8bP1fRedisSourceOperationV1::CommandPublication => matches!(
            script,
            Stage8bP1fRedisScriptV1::CommandPublicationV1
                | Stage8bP1fRedisScriptV1::CommandPublicationRevalidateV1
                | Stage8bP1fRedisScriptV1::P1d4CommandPublicationV1
                | Stage8bP1fRedisScriptV1::P1d4CommandPublicationRevalidateV1
        ),
        Stage8bP1fRedisSourceOperationV1::RetentionAdmission
        | Stage8bP1fRedisSourceOperationV1::ScheduleRead
        | Stage8bP1fRedisSourceOperationV1::StaleConsumerDiscovery
        | Stage8bP1fRedisSourceOperationV1::SourceAcquireAndReclaim
        | Stage8bP1fRedisSourceOperationV1::SourceXackLast => false,
    };
    allowed
        .then_some(())
        .ok_or(Stage8bP1fRedisRoleErrorV1::ForbiddenScript)
}

fn validate_production_db15_endpoint(redis_url: &str) -> Result<(), Stage8bP1fRedisRoleErrorV1> {
    matches!(
        redis_url,
        STAGE8B_P1E_REDIS_URL_IPV4 | STAGE8B_P1E_REDIS_URL_IPV6
    )
    .then_some(())
    .ok_or(Stage8bP1fRedisRoleErrorV1::WrongRedisEndpoint)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fM10RedisPublicationReceiptV1 {
    pub schema_version: u16,
    pub role: Stage8bP1fRedisRoleV1,
    pub redis_id: String,
    pub canonical_bytes_sha256: String,
    pub disposition: Stage8bP1RedisM10PublishDisposition,
    pub exact_reread: bool,
}

/// Feeder-only adapter. It cannot acquire/XACK source work, publish commands,
/// inspect arbitrary keys or obtain the raw Redis connection.
pub struct Stage8bP1fM10FeederRedisV1 {
    role: Stage8bP1fRedisRoleV1,
    transport: Stage8bP1RedisSemanticCompositionTransport,
    audit: Stage8bP1fRedisCommandAuditV1,
}

impl Stage8bP1fM10FeederRedisV1 {
    pub async fn connect_synthetic(
        redis_url: &str,
        config: Stage8bP1RedisConfig,
    ) -> Result<Self, Stage8bP1fRedisRoleErrorV1> {
        validate_production_db15_endpoint(redis_url)?;
        Self::connect_at(redis_url, config, Stage8bP1fRedisRoleV1::SyntheticM10Feeder).await
    }

    pub async fn connect_finam_bars(
        redis_url: &str,
        config: Stage8bP1RedisConfig,
    ) -> Result<Self, Stage8bP1fRedisRoleErrorV1> {
        validate_production_db15_endpoint(redis_url)?;
        Self::connect_at(redis_url, config, Stage8bP1fRedisRoleV1::FinamBarsFeeder).await
    }

    /// Local-only constructor used by retained artifact fixtures. Production
    /// callers remain restricted to the two fixed loopback DB15 endpoints.
    #[cfg(any(test, feature = "stage8b-p1-test-fixtures"))]
    #[doc(hidden)]
    pub async fn connect_synthetic_local_evidence(
        redis_url: &str,
        config: Stage8bP1RedisConfig,
    ) -> Result<Self, Stage8bP1fRedisRoleErrorV1> {
        Self::connect_at(redis_url, config, Stage8bP1fRedisRoleV1::SyntheticM10Feeder).await
    }

    async fn connect_at(
        redis_url: &str,
        config: Stage8bP1RedisConfig,
        role: Stage8bP1fRedisRoleV1,
    ) -> Result<Self, Stage8bP1fRedisRoleErrorV1> {
        authorize_stage8b_p1f_redis_operation(role, Stage8bP1fRedisSourceOperationV1::M10Publish)?;
        let transport = attach_stage8b_p1_redis(redis_url, config).await?;
        let mut audit = Stage8bP1fRedisCommandAuditV1::default();
        // Feeder attachment invokes the pinned read-only namespace verifier,
        // but it is setup evidence rather than an M10 publication event.
        audit.record_auxiliary(
            role,
            Stage8bP1fRedisAuxiliaryOperationV1::FeederVerifyAttach,
            Some(Stage8bP1fRedisScriptV1::NamespaceVerifyV1),
            b"stage8b-p1f-feeder-namespace-verify-v1",
            Stage8bP1fRedisAuditResultV1::Succeeded,
        )?;
        Ok(Self {
            role,
            transport,
            audit,
        })
    }

    pub fn role(&self) -> Stage8bP1fRedisRoleV1 {
        self.role
    }

    pub fn audit_records(&self) -> &VecDeque<Stage8bP1fRedisCommandAuditRecordV1> {
        self.audit.records()
    }

    pub async fn publish_and_reread_exact_m10(
        &mut self,
        redis_id: &str,
        canonical_bytes: &[u8],
        expected_operational_identity_sha256: &str,
    ) -> Result<Stage8bP1fM10RedisPublicationReceiptV1, Stage8bP1fRedisRoleErrorV1> {
        authorize_stage8b_p1f_redis_operation(
            self.role,
            Stage8bP1fRedisSourceOperationV1::M10Publish,
        )?;
        let command_material = publication_fingerprint_material(
            redis_id,
            canonical_bytes,
            expected_operational_identity_sha256,
        );
        let parsed = match crate::parse_stage8b_p1_canonical_m10(
            canonical_bytes,
            expected_operational_identity_sha256,
        ) {
            Ok(value) => value,
            Err(_) => {
                self.audit.record(
                    self.role,
                    Stage8bP1fRedisSourceOperationV1::M10Publish,
                    Some(Stage8bP1fRedisScriptV1::M10PublicationV1),
                    &command_material,
                    Stage8bP1fRedisAuditResultV1::Rejected,
                )?;
                return Err(Stage8bP1fRedisRoleErrorV1::PublicationConflict);
            }
        };
        if parsed.redis_id() != redis_id {
            self.audit.record(
                self.role,
                Stage8bP1fRedisSourceOperationV1::M10Publish,
                Some(Stage8bP1fRedisScriptV1::M10PublicationV1),
                &command_material,
                Stage8bP1fRedisAuditResultV1::Rejected,
            )?;
            return Err(Stage8bP1fRedisRoleErrorV1::PublicationConflict);
        }
        let disposition = match self
            .transport
            .publish_canonical_m10(canonical_bytes, expected_operational_identity_sha256)
            .await
        {
            Ok(value) => value,
            Err(error) => {
                self.audit.record(
                    self.role,
                    Stage8bP1fRedisSourceOperationV1::M10Publish,
                    Some(Stage8bP1fRedisScriptV1::M10PublicationV1),
                    &command_material,
                    Stage8bP1fRedisAuditResultV1::Failed,
                )?;
                return Err(error.into());
            }
        };
        if let Err(error) = self
            .transport
            .verify_exact_canonical_m10(
                redis_id,
                canonical_bytes,
                expected_operational_identity_sha256,
            )
            .await
        {
            self.audit.record(
                self.role,
                Stage8bP1fRedisSourceOperationV1::M10Publish,
                Some(Stage8bP1fRedisScriptV1::M10PublicationV1),
                &command_material,
                Stage8bP1fRedisAuditResultV1::Failed,
            )?;
            return Err(error.into());
        }
        self.audit.record(
            self.role,
            Stage8bP1fRedisSourceOperationV1::M10Publish,
            Some(Stage8bP1fRedisScriptV1::M10PublicationV1),
            &command_material,
            match disposition {
                Stage8bP1RedisM10PublishDisposition::Published => {
                    Stage8bP1fRedisAuditResultV1::Succeeded
                }
                Stage8bP1RedisM10PublishDisposition::IdempotentExisting => {
                    Stage8bP1fRedisAuditResultV1::IdempotentExisting
                }
            },
        )?;
        Ok(Stage8bP1fM10RedisPublicationReceiptV1 {
            schema_version: 1,
            role: self.role,
            redis_id: redis_id.to_string(),
            canonical_bytes_sha256: sha256_hex(canonical_bytes),
            disposition,
            exact_reread: true,
        })
    }
}

pub async fn provision_stage8b_p1f_fresh_namespace_v1(
    redis_url: &str,
    config: Stage8bP1RedisConfig,
) -> Result<VecDeque<Stage8bP1fRedisCommandAuditRecordV1>, Stage8bP1fRedisRoleErrorV1> {
    validate_production_db15_endpoint(redis_url)?;
    let role = Stage8bP1fRedisRoleV1::Provisioner;
    authorize_stage8b_p1f_redis_operation(role, Stage8bP1fRedisSourceOperationV1::FreshNamespace)?;
    initialize_stage8b_p1_redis_namespace(redis_url, config).await?;
    let mut audit = Stage8bP1fRedisCommandAuditV1::default();
    for script in [
        Stage8bP1fRedisScriptV1::NamespaceInitializationV1,
        Stage8bP1fRedisScriptV1::NamespaceVerifyV1,
    ] {
        audit.record(
            role,
            Stage8bP1fRedisSourceOperationV1::FreshNamespace,
            Some(script),
            script.sha256().as_bytes(),
            Stage8bP1fRedisAuditResultV1::Succeeded,
        )?;
    }
    Ok(audit.records)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fResourceSampleV1 {
    pub m10_pel_count: u64,
    pub command_pel_count: u64,
    pub total_pel_count: u64,
    pub db15_evidence_bytes: u64,
    pub root_free_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1fResourceStopReasonV1 {
    PelLimitExceeded,
    EvidenceBudgetExceeded,
    RootFreeBelowMinimum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1fResourceDispositionV1 {
    Continue,
    StopP1Phase(Stage8bP1fResourceStopReasonV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1fResourceMonitorOutcomeV1 {
    ShutdownObserved {
        completed_ticks: u64,
    },
    StopP1Phase {
        completed_ticks: u64,
        reason: Stage8bP1fResourceStopReasonV1,
    },
    ProbeFailed {
        completed_ticks: u64,
    },
}

pub fn evaluate_stage8b_p1f_resource_sample_v1(
    sample: Stage8bP1fResourceSampleV1,
) -> Stage8bP1fResourceDispositionV1 {
    if sample.command_pel_count != 0
        || sample.total_pel_count > STAGE8B_P1F_TOTAL_PEL_FAIL_STOP_THRESHOLD
    {
        Stage8bP1fResourceDispositionV1::StopP1Phase(
            Stage8bP1fResourceStopReasonV1::PelLimitExceeded,
        )
    } else if sample.db15_evidence_bytes > STAGE8B_P1F_DB15_EVIDENCE_BUDGET_BYTES {
        Stage8bP1fResourceDispositionV1::StopP1Phase(
            Stage8bP1fResourceStopReasonV1::EvidenceBudgetExceeded,
        )
    } else if sample.root_free_bytes < STAGE8B_P1F_MINIMUM_ROOT_FREE_BYTES {
        Stage8bP1fResourceDispositionV1::StopP1Phase(
            Stage8bP1fResourceStopReasonV1::RootFreeBelowMinimum,
        )
    } else {
        Stage8bP1fResourceDispositionV1::Continue
    }
}

/// Read-only resource adapter. The Redis connection remains private and can
/// issue only bounded `XPENDING` against the two fixed P1 groups plus
/// `INFO memory`. `used_memory` is a conservative upper bound for the isolated
/// DB15 evidence budget. A non-empty command PEL is fail-closed.
pub struct Stage8bP1fResourceProbeV1 {
    connection: ConnectionManager,
    audit: Stage8bP1fRedisCommandAuditHandleV1,
}

impl Stage8bP1fResourceProbeV1 {
    pub async fn connect(redis_url: &str) -> Result<Self, Stage8bP1fRedisRoleErrorV1> {
        validate_production_db15_endpoint(redis_url)?;
        Self::connect_with_audit(redis_url, Stage8bP1fRedisCommandAuditHandleV1::default()).await
    }

    pub(crate) async fn connect_with_audit(
        redis_url: &str,
        audit: Stage8bP1fRedisCommandAuditHandleV1,
    ) -> Result<Self, Stage8bP1fRedisRoleErrorV1> {
        let client = redis::Client::open(redis_url)?;
        let connection = tokio::time::timeout(
            Duration::from_millis(STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
            ConnectionManager::new(client),
        )
        .await
        .map_err(|_| Stage8bP1fRedisRoleErrorV1::ResourceProbe)??;
        Ok(Self { connection, audit })
    }

    pub fn audit_records(
        &self,
    ) -> Result<Vec<Stage8bP1fRedisCommandAuditRecordV1>, Stage8bP1fRedisRoleErrorV1> {
        self.audit.snapshot()
    }

    pub async fn poll(&mut self) -> Result<Stage8bP1fResourceSampleV1, Stage8bP1fRedisRoleErrorV1> {
        let result = self.poll_inner().await;
        let (material, audit_result) = match &result {
            Ok(sample) => (
                format!(
                    "m10_pel={};command_pel={};total_pel={};used_memory={};root_free={}",
                    sample.m10_pel_count,
                    sample.command_pel_count,
                    sample.total_pel_count,
                    sample.db15_evidence_bytes,
                    sample.root_free_bytes
                ),
                Stage8bP1fRedisAuditResultV1::Succeeded,
            ),
            Err(_) => (
                "stage8b-p1f-resource-probe-failed-v1".to_string(),
                Stage8bP1fRedisAuditResultV1::Failed,
            ),
        };
        self.audit.record_auxiliary(
            Stage8bP1fRedisRoleV1::ReadOnlyAuditor,
            Stage8bP1fRedisAuxiliaryOperationV1::ResourcePoll,
            None,
            material.as_bytes(),
            audit_result,
        )?;
        result
    }

    async fn poll_inner(
        &mut self,
    ) -> Result<Stage8bP1fResourceSampleV1, Stage8bP1fRedisRoleErrorV1> {
        let namespace = stage8b_p1_redis_namespace();
        let m10_pending: redis::streams::StreamPendingReply = tokio::time::timeout(
            Duration::from_millis(STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
            redis::cmd("XPENDING")
                .arg(&namespace.canonical_m10_stream)
                .arg(&namespace.m10_consumer_group)
                .query_async(&mut self.connection),
        )
        .await
        .map_err(|_| Stage8bP1fRedisRoleErrorV1::ResourceProbe)??;
        let command_pending: redis::streams::StreamPendingReply = tokio::time::timeout(
            Duration::from_millis(STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
            redis::cmd("XPENDING")
                .arg(&namespace.canonical_command_stream)
                .arg(&namespace.stage7b_command_consumer_group)
                .query_async(&mut self.connection),
        )
        .await
        .map_err(|_| Stage8bP1fRedisRoleErrorV1::ResourceProbe)??;
        let m10_pel_count = u64::try_from(m10_pending.count())
            .map_err(|_| Stage8bP1fRedisRoleErrorV1::ResourceProbe)?;
        let command_pel_count = u64::try_from(command_pending.count())
            .map_err(|_| Stage8bP1fRedisRoleErrorV1::ResourceProbe)?;
        let total_pel_count = m10_pel_count
            .checked_add(command_pel_count)
            .ok_or(Stage8bP1fRedisRoleErrorV1::ResourceProbe)?;
        let info: String = tokio::time::timeout(
            Duration::from_millis(STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
            redis::cmd("INFO")
                .arg("memory")
                .query_async(&mut self.connection),
        )
        .await
        .map_err(|_| Stage8bP1fRedisRoleErrorV1::ResourceProbe)??;
        Ok(Stage8bP1fResourceSampleV1 {
            m10_pel_count,
            command_pel_count,
            total_pel_count,
            db15_evidence_bytes: parse_used_memory(&info)?,
            root_free_bytes: root_free_bytes()?,
        })
    }
}

/// The sole periodic Id resource task. It owns no lifecycle authority: an
/// exceeded limit or probe failure requests the existing first-wins I1 latch,
/// after which the existing owner and outer coordinator perform bounded
/// shutdown to an authenticated boundary.
pub(crate) async fn run_stage8b_p1f_resource_monitor_v1(
    mut probe: Stage8bP1fResourceProbeV1,
    latch: Arc<Stage8bP1eShutdownLatchV1>,
    shutdown_grace_ms: u64,
) -> Stage8bP1fResourceMonitorOutcomeV1 {
    run_stage8b_p1f_resource_monitor_with_period_v1(
        &mut probe,
        latch,
        shutdown_grace_ms,
        Duration::from_secs(STAGE8B_P1F_RESOURCE_POLL_INTERVAL_SECONDS),
    )
    .await
}

async fn run_stage8b_p1f_resource_monitor_with_period_v1(
    probe: &mut Stage8bP1fResourceProbeV1,
    latch: Arc<Stage8bP1eShutdownLatchV1>,
    shutdown_grace_ms: u64,
    period: Duration,
) -> Stage8bP1fResourceMonitorOutcomeV1 {
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut completed_ticks = 0_u64;
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = wait_for_stage8b_p1f_shutdown_v1(latch.as_ref()) => {
                return Stage8bP1fResourceMonitorOutcomeV1::ShutdownObserved { completed_ticks };
            }
        }
        if latch.intent().is_some() {
            return Stage8bP1fResourceMonitorOutcomeV1::ShutdownObserved { completed_ticks };
        }
        completed_ticks = completed_ticks.saturating_add(1);
        let sample = match probe.poll().await {
            Ok(sample) => sample,
            Err(_) => {
                request_stage8b_p1f_resource_shutdown_v1(latch.as_ref(), shutdown_grace_ms);
                return Stage8bP1fResourceMonitorOutcomeV1::ProbeFailed { completed_ticks };
            }
        };
        if let Stage8bP1fResourceDispositionV1::StopP1Phase(reason) =
            evaluate_stage8b_p1f_resource_sample_v1(sample)
        {
            request_stage8b_p1f_resource_shutdown_v1(latch.as_ref(), shutdown_grace_ms);
            return Stage8bP1fResourceMonitorOutcomeV1::StopP1Phase {
                completed_ticks,
                reason,
            };
        }
    }
}

fn request_stage8b_p1f_resource_shutdown_v1(
    latch: &Stage8bP1eShutdownLatchV1,
    shutdown_grace_ms: u64,
) {
    let now_utc_ms = chrono::Utc::now().timestamp_millis();
    let _ = latch.request(Stage8bP1eShutdownIntentV1::new(
        Stage8bP1eShutdownCauseV1::RedisLifecycleFailure,
        now_utc_ms.saturating_add(shutdown_grace_ms as i64),
        1,
    ));
}

async fn wait_for_stage8b_p1f_shutdown_v1(latch: &Stage8bP1eShutdownLatchV1) {
    while latch.intent().is_none() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Routes a resource failure into the already accepted supervisor terminal
/// path.  It never trims Redis, changes configuration or acts on P0.
pub fn coordinate_stage8b_p1f_resource_sample_v1(
    coordinator: &mut Stage8bP1eCoordinatorV1,
    sample: Stage8bP1fResourceSampleV1,
    owner_available: bool,
    now_utc_ms: i64,
    grace_deadline_utc_ms: i64,
    request_sequence: u64,
) -> Option<Stage8bP1eCoordinatorDecisionV1> {
    match evaluate_stage8b_p1f_resource_sample_v1(sample) {
        Stage8bP1fResourceDispositionV1::Continue => None,
        Stage8bP1fResourceDispositionV1::StopP1Phase(_) => Some(coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::RedisLifecycleFailed,
            owner_available,
            now_utc_ms,
            grace_deadline_utc_ms,
            request_sequence,
        )),
    }
}

fn publication_fingerprint_material(
    redis_id: &str,
    canonical_bytes: &[u8],
    operational_identity_sha256: &str,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"moex.stage8b.p1f.redis-m10-command.v1\0");
    bytes.extend_from_slice(redis_id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(operational_identity_sha256.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(sha256_hex(canonical_bytes).as_bytes());
    bytes
}

fn parse_used_memory(info: &str) -> Result<u64, Stage8bP1fRedisRoleErrorV1> {
    let values = info
        .lines()
        .filter_map(|line| line.strip_prefix("used_memory:"))
        .collect::<Vec<_>>();
    let [value] = values.as_slice() else {
        return Err(Stage8bP1fRedisRoleErrorV1::ResourceProbe);
    };
    value
        .trim_end_matches('\r')
        .parse()
        .map_err(|_| Stage8bP1fRedisRoleErrorV1::ResourceProbe)
}

fn root_free_bytes() -> Result<u64, Stage8bP1fRedisRoleErrorV1> {
    let root = CString::new("/").map_err(|_| Stage8bP1fRedisRoleErrorV1::ResourceProbe)?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `root` is a valid NUL-terminated path and `stats` points to
    // writable storage initialized by statvfs on success.
    if unsafe { libc::statvfs(root.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(Stage8bP1fRedisRoleErrorV1::ResourceProbe);
    }
    // SAFETY: the successful call above initialized the complete structure.
    let stats = unsafe { stats.assume_init() };
    #[cfg(target_os = "macos")]
    let available_blocks = u64::from(stats.f_bavail);
    #[cfg(not(target_os = "macos"))]
    let available_blocks = stats.f_bavail;
    available_blocks
        .checked_mul(stats.f_frsize)
        .ok_or(Stage8bP1fRedisRoleErrorV1::ResourceProbe)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use std::{
        net::TcpListener,
        process::{Child, Command, Stdio},
        time::Duration,
    };

    use super::*;

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
                .expect("redis-server is required for the P1F-Id composition proof");
            let url = format!("redis://127.0.0.1:{port}/15");
            for _ in 0..100 {
                if let Ok(client) = redis::Client::open(url.as_str()) {
                    if let Ok(mut connection) = redis::aio::ConnectionManager::new(client).await {
                        let pong: redis::RedisResult<String> =
                            redis::cmd("PING").query_async(&mut connection).await;
                        if pong.as_deref() == Ok("PONG") && child.try_wait().unwrap().is_none() {
                            return Self { child, url };
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let _ = child.kill();
            let _ = child.wait();
            panic!("temporary Redis did not start");
        }
    }

    impl Drop for RedisServer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn canonical_m10(identity: &str) -> Vec<u8> {
        let close_ts_utc_ms = 1_785_759_600_000;
        let open_ts_utc_ms = close_ts_utc_ms - 600_000;
        let source_m1 = (0..10)
            .map(|index| {
                let open = open_ts_utc_ms + index * 60_000;
                crate::Stage8bP1CanonicalM10SourceM1 {
                    redis_id: format!("{}-0", open + 60_000),
                    semantic_id_sha256: format!("{:064x}", index + 1),
                    payload_sha256: format!("{:064x}", index + 101),
                    open_ts_utc_ms: open,
                    close_ts_utc_ms: open + 60_000,
                }
            })
            .collect();
        crate::build_stage8b_p1_canonical_m10(crate::Stage8bP1CanonicalM10BuildInput {
            operational_identity_sha256: identity.to_string(),
            open_ts_utc_ms,
            close_ts_utc_ms,
            open: "2200".to_string(),
            high: "2202".to_string(),
            low: "2199".to_string(),
            close: "2201".to_string(),
            volume: "10000".to_string(),
            source_m1,
        })
        .unwrap()
    }

    #[test]
    fn fixed_matrix_has_exact_cardinality_and_rejects_cross_role_authority() {
        assert_eq!(STAGE8B_P1F_REDIS_ROLES.len(), 8);
        assert_eq!(STAGE8B_P1F_REDIS_SOURCE_OPERATIONS.len(), 10);
        assert_eq!(STAGE8B_P1F_REDIS_SCRIPTS.len(), 8);
        assert!(authorize_stage8b_p1f_redis_operation(
            Stage8bP1fRedisRoleV1::Provisioner,
            Stage8bP1fRedisSourceOperationV1::FreshNamespace
        )
        .is_ok());
        assert!(authorize_stage8b_p1f_redis_operation(
            Stage8bP1fRedisRoleV1::Supervisor,
            Stage8bP1fRedisSourceOperationV1::SourceXackLast
        )
        .is_ok());
        for role in [
            Stage8bP1fRedisRoleV1::SchedulePublisher,
            Stage8bP1fRedisRoleV1::ReadOnlyAuditor,
            Stage8bP1fRedisRoleV1::PhaseGuardian,
            Stage8bP1fRedisRoleV1::BrokerTruthObserver,
        ] {
            assert!(authorize_stage8b_p1f_redis_operation(
                role,
                Stage8bP1fRedisSourceOperationV1::M10Publish
            )
            .is_err());
        }
        assert_eq!(
            stage8b_p1f_redis_role_contract_v1(Stage8bP1fRedisRoleV1::PhaseGuardian).database_scope,
            Stage8bP1fRedisDatabaseScopeV1::None
        );
        assert_eq!(
            stage8b_p1f_redis_source_operation_contract_v1(
                Stage8bP1fRedisSourceOperationV1::ScheduleRead
            )
            .key_and_argv_contract,
            "exact market-schedule key and fixed bound 64"
        );
    }

    #[test]
    fn script_identity_is_operation_specific() {
        assert!(authorize_script_for_operation(
            Stage8bP1fRedisSourceOperationV1::M10Publish,
            Stage8bP1fRedisScriptV1::M10PublicationV1
        )
        .is_ok());
        assert!(authorize_script_for_operation(
            Stage8bP1fRedisSourceOperationV1::M10Publish,
            Stage8bP1fRedisScriptV1::CommandPublicationV1
        )
        .is_err());
        assert!(authorize_script_for_operation(
            Stage8bP1fRedisSourceOperationV1::CommandPublication,
            Stage8bP1fRedisScriptV1::P1d4CommandPublicationRevalidateV1
        )
        .is_ok());
    }

    #[test]
    fn command_audit_is_bounded_and_contains_only_hashes() {
        let mut audit = Stage8bP1fRedisCommandAuditV1::default();
        for index in 0..=STAGE8B_P1F_COMMAND_AUDIT_CAPACITY {
            audit
                .record(
                    Stage8bP1fRedisRoleV1::FinamBarsFeeder,
                    Stage8bP1fRedisSourceOperationV1::M10Publish,
                    Some(Stage8bP1fRedisScriptV1::M10PublicationV1),
                    format!("secret-material-{index}").as_bytes(),
                    Stage8bP1fRedisAuditResultV1::Succeeded,
                )
                .unwrap();
        }
        assert_eq!(audit.records().len(), STAGE8B_P1F_COMMAND_AUDIT_CAPACITY);
        assert_eq!(audit.records().front().unwrap().sequence, 2);
        let encoded = serde_json::to_string(audit.records()).unwrap();
        assert!(!encoded.contains("secret-material"));
    }

    #[test]
    fn resource_boundaries_stop_only_after_crossing_exact_limits() {
        assert_eq!(
            evaluate_stage8b_p1f_resource_sample_v1(Stage8bP1fResourceSampleV1 {
                m10_pel_count: 64,
                command_pel_count: 0,
                total_pel_count: 64,
                db15_evidence_bytes: 536_870_912,
                root_free_bytes: 10_737_418_240,
            }),
            Stage8bP1fResourceDispositionV1::Continue
        );
        assert_eq!(
            evaluate_stage8b_p1f_resource_sample_v1(Stage8bP1fResourceSampleV1 {
                m10_pel_count: 65,
                command_pel_count: 0,
                total_pel_count: 65,
                db15_evidence_bytes: 0,
                root_free_bytes: u64::MAX,
            }),
            Stage8bP1fResourceDispositionV1::StopP1Phase(
                Stage8bP1fResourceStopReasonV1::PelLimitExceeded
            )
        );
        assert_eq!(
            evaluate_stage8b_p1f_resource_sample_v1(Stage8bP1fResourceSampleV1 {
                m10_pel_count: 63,
                command_pel_count: 1,
                total_pel_count: 64,
                db15_evidence_bytes: 0,
                root_free_bytes: u64::MAX,
            }),
            Stage8bP1fResourceDispositionV1::StopP1Phase(
                Stage8bP1fResourceStopReasonV1::PelLimitExceeded
            )
        );
        assert_eq!(
            evaluate_stage8b_p1f_resource_sample_v1(Stage8bP1fResourceSampleV1 {
                m10_pel_count: 0,
                command_pel_count: 0,
                total_pel_count: 0,
                db15_evidence_bytes: STAGE8B_P1F_DB15_EVIDENCE_BUDGET_BYTES,
                root_free_bytes: STAGE8B_P1F_MINIMUM_ROOT_FREE_BYTES,
            }),
            Stage8bP1fResourceDispositionV1::Continue
        );
        assert_eq!(
            evaluate_stage8b_p1f_resource_sample_v1(Stage8bP1fResourceSampleV1 {
                m10_pel_count: 0,
                command_pel_count: 0,
                total_pel_count: 0,
                db15_evidence_bytes: STAGE8B_P1F_DB15_EVIDENCE_BUDGET_BYTES + 1,
                root_free_bytes: u64::MAX,
            }),
            Stage8bP1fResourceDispositionV1::StopP1Phase(
                Stage8bP1fResourceStopReasonV1::EvidenceBudgetExceeded
            )
        );
        assert_eq!(
            evaluate_stage8b_p1f_resource_sample_v1(Stage8bP1fResourceSampleV1 {
                m10_pel_count: 0,
                command_pel_count: 0,
                total_pel_count: 0,
                db15_evidence_bytes: 0,
                root_free_bytes: STAGE8B_P1F_MINIMUM_ROOT_FREE_BYTES - 1,
            }),
            Stage8bP1fResourceDispositionV1::StopP1Phase(
                Stage8bP1fResourceStopReasonV1::RootFreeBelowMinimum
            )
        );
        assert_eq!(STAGE8B_P1F_RESOURCE_POLL_INTERVAL_SECONDS, 5);
    }

    #[test]
    fn rejected_cross_role_attempt_is_retained_without_command_material() {
        let audit = Stage8bP1fRedisCommandAuditHandleV1::default();
        assert!(matches!(
            audit.record(
                Stage8bP1fRedisRoleV1::ReadOnlyAuditor,
                Stage8bP1fRedisSourceOperationV1::CommandPublication,
                Some(Stage8bP1fRedisScriptV1::CommandPublicationV1),
                b"forbidden-command-material",
                Stage8bP1fRedisAuditResultV1::Succeeded,
            ),
            Err(Stage8bP1fRedisRoleErrorV1::ForbiddenOperation)
        ));
        let records = audit.snapshot().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].result, Stage8bP1fRedisAuditResultV1::Rejected);
        assert_eq!(records[0].role, Stage8bP1fRedisRoleV1::ReadOnlyAuditor);
        assert_eq!(
            records[0].operation,
            Stage8bP1fRedisAuditedOperationV1::Source(
                Stage8bP1fRedisSourceOperationV1::CommandPublication
            )
        );
        let encoded = serde_json::to_string(&records).unwrap();
        assert!(!encoded.contains("forbidden-command-material"));
    }

    #[test]
    fn resource_failure_uses_existing_terminal_coordinator() {
        let mut coordinator = Stage8bP1eCoordinatorV1::new();
        let decision = coordinate_stage8b_p1f_resource_sample_v1(
            &mut coordinator,
            Stage8bP1fResourceSampleV1 {
                m10_pel_count: 0,
                command_pel_count: 0,
                total_pel_count: 0,
                db15_evidence_bytes: STAGE8B_P1F_DB15_EVIDENCE_BUDGET_BYTES + 1,
                root_free_bytes: u64::MAX,
            },
            true,
            1,
            2,
            1,
        )
        .unwrap();
        assert_eq!(decision.exit_code, Some(67));
        assert!(decision.request_shutdown);
        assert!(!decision.owner_may_bounded_drain);
    }

    #[test]
    fn production_adapter_rejects_wrong_database_or_host() {
        assert!(validate_production_db15_endpoint("redis://127.0.0.1:6379/0").is_err());
        assert!(validate_production_db15_endpoint(STAGE8B_P1E_REDIS_URL_IPV4).is_ok());
        assert!(validate_production_db15_endpoint(STAGE8B_P1E_REDIS_URL_IPV6).is_ok());
    }

    #[tokio::test]
    async fn real_redis_feeder_replays_exact_id_and_rereads_exact_bytes() {
        let redis = RedisServer::start().await;
        let identity = "4".repeat(64);
        let bytes = canonical_m10(&identity);
        let redis_id = crate::parse_stage8b_p1_canonical_m10(&bytes, &identity)
            .unwrap()
            .redis_id()
            .to_string();
        let mut first = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        assert_eq!(
            first
                .publish_canonical_m10(&bytes, &identity)
                .await
                .unwrap(),
            Stage8bP1RedisM10PublishDisposition::Published
        );
        drop(first);

        let mut feeder = Stage8bP1fM10FeederRedisV1::connect_at(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
            Stage8bP1fRedisRoleV1::SyntheticM10Feeder,
        )
        .await
        .unwrap();
        assert_eq!(feeder.audit_records().len(), 1);
        assert_eq!(
            feeder.audit_records().front().unwrap().operation,
            Stage8bP1fRedisAuditedOperationV1::Auxiliary(
                Stage8bP1fRedisAuxiliaryOperationV1::FeederVerifyAttach
            )
        );
        assert_eq!(
            feeder
                .audit_records()
                .front()
                .unwrap()
                .script_sha256
                .as_deref(),
            Some(Stage8bP1fRedisScriptV1::NamespaceVerifyV1.sha256())
        );
        let receipt = feeder
            .publish_and_reread_exact_m10(&redis_id, &bytes, &identity)
            .await
            .unwrap();
        assert_eq!(
            receipt.disposition,
            Stage8bP1RedisM10PublishDisposition::IdempotentExisting
        );
        assert!(receipt.exact_reread);
        assert_eq!(receipt.canonical_bytes_sha256, sha256_hex(&bytes));
        assert!(feeder
            .audit_records()
            .iter()
            .all(|entry| entry.role == Stage8bP1fRedisRoleV1::SyntheticM10Feeder));
    }

    #[tokio::test]
    async fn feeder_rejects_wrong_deterministic_id_before_redis_effect() {
        let redis = RedisServer::start().await;
        let identity = "4".repeat(64);
        let bytes = canonical_m10(&identity);
        initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let mut feeder = Stage8bP1fM10FeederRedisV1::connect_at(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
            Stage8bP1fRedisRoleV1::FinamBarsFeeder,
        )
        .await
        .unwrap();
        assert!(matches!(
            feeder
                .publish_and_reread_exact_m10("1-0", &bytes, &identity)
                .await,
            Err(Stage8bP1fRedisRoleErrorV1::PublicationConflict)
        ));
        assert_eq!(feeder.transport.retained_m10_count().await.unwrap(), 0);
        assert_eq!(
            feeder.audit_records().back().unwrap().result,
            Stage8bP1fRedisAuditResultV1::Rejected
        );
    }

    #[tokio::test]
    async fn resource_probe_uses_real_bounded_redis_reads_and_hash_only_audit() {
        let redis = RedisServer::start().await;
        initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let mut probe = Stage8bP1fResourceProbeV1::connect_with_audit(
            &redis.url,
            Stage8bP1fRedisCommandAuditHandleV1::default(),
        )
        .await
        .unwrap();
        let sample = probe.poll().await.unwrap();
        assert_eq!(sample.m10_pel_count, 0);
        assert_eq!(sample.command_pel_count, 0);
        assert_eq!(sample.total_pel_count, 0);
        assert!(sample.db15_evidence_bytes > 0);
        assert!(sample.root_free_bytes > 0);
        let audit = probe.audit_records().unwrap();
        assert_eq!(audit.len(), 1);
        let encoded = serde_json::to_string(&audit).unwrap();
        assert!(!encoded.contains("used_memory="));
        assert!(encoded.contains("command_fingerprint_sha256"));
    }

    #[tokio::test]
    async fn resource_monitor_performs_sequential_ticks_until_existing_latch_stops_it() {
        let redis = RedisServer::start().await;
        initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let audit = Stage8bP1fRedisCommandAuditHandleV1::default();
        let mut probe = Stage8bP1fResourceProbeV1::connect_with_audit(&redis.url, audit.clone())
            .await
            .unwrap();
        let latch = Arc::new(Stage8bP1eShutdownLatchV1::new());
        let monitor_latch = Arc::clone(&latch);
        let monitor = tokio::spawn(async move {
            run_stage8b_p1f_resource_monitor_with_period_v1(
                &mut probe,
                monitor_latch,
                100,
                Duration::from_millis(10),
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(45)).await;
        assert!(latch.intent().is_none());
        assert!(latch.request(Stage8bP1eShutdownIntentV1::new(
            Stage8bP1eShutdownCauseV1::ExternalSignal,
            chrono::Utc::now().timestamp_millis() + 100,
            1,
        )));
        let outcome = monitor.await.unwrap();
        assert!(matches!(
            outcome,
            Stage8bP1fResourceMonitorOutcomeV1::ShutdownObserved {
                completed_ticks: 2..
            }
        ));
        let records = audit.snapshot().unwrap();
        assert!(records.len() >= 2);
        assert!(records.iter().all(|record| {
            record.operation
                == Stage8bP1fRedisAuditedOperationV1::Auxiliary(
                    Stage8bP1fRedisAuxiliaryOperationV1::ResourcePoll,
                )
                && record.result == Stage8bP1fRedisAuditResultV1::Succeeded
        }));
    }

    #[tokio::test]
    async fn resource_monitor_probe_failure_requests_bounded_existing_shutdown() {
        let redis = RedisServer::start().await;
        initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let audit = Stage8bP1fRedisCommandAuditHandleV1::default();
        let mut probe = Stage8bP1fResourceProbeV1::connect_with_audit(&redis.url, audit.clone())
            .await
            .unwrap();
        drop(redis);
        let latch = Arc::new(Stage8bP1eShutdownLatchV1::new());
        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            run_stage8b_p1f_resource_monitor_with_period_v1(
                &mut probe,
                Arc::clone(&latch),
                100,
                Duration::from_millis(1),
            ),
        )
        .await
        .unwrap();
        assert!(matches!(
            outcome,
            Stage8bP1fResourceMonitorOutcomeV1::ProbeFailed { completed_ticks: 1 }
        ));
        let intent = latch.intent().expect("probe failure must stop the owner");
        assert_eq!(
            intent.cause(),
            Stage8bP1eShutdownCauseV1::RedisLifecycleFailure
        );
        assert_eq!(intent.cause().exit_class(), 67);
        assert_eq!(
            audit.snapshot().unwrap().last().unwrap().result,
            Stage8bP1fRedisAuditResultV1::Failed
        );
    }
}
