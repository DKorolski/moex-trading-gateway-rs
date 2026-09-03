//! Stage 8B-P1-c real-Redis semantic source and command-publication boundary.
//!
//! This module deliberately stops at a canonical command stream entry. It
//! owns no paper provider, FINAM transport, broker dispatch or runtime-live
//! operation. Operational DB0 activation is a separate deployment gate.

use super::{
    binding_from_delivery, parse_stage8b_p1_canonical_m10, Stage8bP1CanonicalM10Error,
    Stage8bP1PendingM10Delivery, Stage8bP1SemanticCompositionError,
};
use crate::recovery::{
    P1SemanticPrepublicationPending, P1SemanticZeroIntentAckPending, Stage7bRecoveryError,
    Stage7bRecoveryReadyOwner, Stage8bP1SemanticCommitOutcome,
    Stage8bP1SemanticPrepublicationOwner, Stage8bP1ZeroIntentCommitReceipt,
};
use crate::stage8b_p1_bootstrap::{stage8b_p1_redis_namespace, Stage8bP1RedisNamespace};
use broker_core::{BrokerCommand, Envelope, MessageType, StrategyRequestId, SCHEMA_VERSION};
use redis::aio::ConnectionManager;
use redis::streams::{
    StreamAutoClaimReply, StreamId, StreamPendingCountReply, StreamRangeReply, StreamReadReply,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use strategy_runtime_core::{
    Stage5gLifecycleCommitmentKey, Stage6Stage8bP1SemanticCommitEvidenceV1,
};
use uuid::Uuid;

const COMMAND_ENVELOPE_SOURCE: &str = "stage8b-p1-semantic";
const COMMAND_PUBLICATION_MARKER_SCHEMA_VERSION: u16 = 1;
const COMMAND_PUBLICATION_MARKER_DOMAIN: &str = "moex.stage8b.p1.command-publication-marker.v1";
const MIN_RETENTION_FLOOR: usize = super::STAGE8B_P1_LOCAL_M10_MIN_RETENTION;

const M10_PUBLICATION_LUA: &str = r#"
local function has_group(stream, expected)
  local groups = redis.call('XINFO', 'GROUPS', stream)
  for _, group in ipairs(groups) do
    for index = 1, #group, 2 do
      if group[index] == 'name' and group[index + 1] == expected then
        return true
      end
    end
  end
  return false
end

local stream = KEYS[1]
local group = ARGV[1]
local entry_id = ARGV[2]
local payload = ARGV[3]
if not has_group(stream, group) then
  return redis.error_reply('STAGE8B_P1_M10_GROUP_MISSING')
end
return redis.call('XADD', stream, entry_id, 'payload', payload)
"#;

const COMMAND_PUBLICATION_LUA: &str = r#"
local function type_name(key)
  local result = redis.call('TYPE', key)
  if type(result) == 'table' then return result['ok'] end
  return result
end

local function has_group(stream, expected)
  local groups = redis.call('XINFO', 'GROUPS', stream)
  for _, candidate in ipairs(groups) do
    for index = 1, #candidate, 2 do
      if candidate[index] == 'name' and candidate[index + 1] == expected then
        return true
      end
    end
  end
  return false
end

local source = KEYS[1]
local command_stream = KEYS[2]
local marker_key = KEYS[3]
local group = ARGV[1]
local source_id = ARGV[2]
local source_payload = ARGV[3]
local semantic_batch_id = ARGV[4]
local request_id = ARGV[5]
local command_sha256 = ARGV[6]
local envelope_sha256 = ARGV[7]
local envelope_payload = ARGV[8]
local seal_generation = tonumber(ARGV[9])
local seal_commitment = ARGV[10]
local schema = tonumber(ARGV[11])
local domain = ARGV[12]
local command_group = ARGV[13]

if schema ~= 1 or domain ~= 'moex.stage8b.p1.command-publication-marker.v1' then
  return redis.error_reply('STAGE8B_P1_PUBLICATION_SCHEMA')
end
if type_name(source) ~= 'stream' then
  return redis.error_reply('STAGE8B_P1_SOURCE_TYPE')
end
local command_type = type_name(command_stream)
if command_type ~= 'stream' then
  return redis.error_reply('STAGE8B_P1_COMMAND_STREAM_TYPE')
end
if not has_group(command_stream, command_group) then
  return redis.error_reply('STAGE8B_P1_COMMAND_GROUP_MISSING')
end
local marker_type = type_name(marker_key)
if marker_type ~= 'none' and marker_type ~= 'string' then
  return redis.error_reply('STAGE8B_P1_MARKER_TYPE')
end

local exact_source = redis.call('XRANGE', source, source_id, source_id)
if #exact_source ~= 1 or tostring(exact_source[1][1]) ~= source_id then
  return redis.error_reply('STAGE8B_P1_SOURCE_MISSING')
end
local source_fields = exact_source[1][2]
if #source_fields ~= 2 or source_fields[1] ~= 'payload' or source_fields[2] ~= source_payload then
  return redis.error_reply('STAGE8B_P1_SOURCE_CONFLICT')
end

local pending = redis.call('XPENDING', source, group, source_id, source_id, 1)
if #pending ~= 1 or tostring(pending[1][1]) ~= source_id then
  return redis.error_reply('STAGE8B_P1_SOURCE_NOT_PENDING')
end

local existing = redis.call('GET', marker_key)
if existing then
  local ok, marker = pcall(cjson.decode, existing)
  if not ok or marker['schema_version'] ~= schema or marker['domain'] ~= domain
     or marker['source_stream'] ~= source or marker['source_group'] ~= group
     or marker['source_id'] ~= source_id
     or marker['semantic_batch_id_sha256'] ~= semantic_batch_id
     or marker['strategy_request_id'] ~= request_id
     or marker['canonical_command_sha256'] ~= command_sha256
     or marker['canonical_envelope_sha256'] ~= envelope_sha256
     or marker['command_stream'] ~= command_stream
     or marker['command_group'] ~= command_group
     or marker['seal_generation'] ~= seal_generation
     or marker['seal_commitment_sha256'] ~= seal_commitment then
    return redis.error_reply('STAGE8B_P1_PUBLICATION_CONFLICT')
  end
  local output_id = marker['command_entry_id']
  local exact_command = redis.call('XRANGE', command_stream, output_id, output_id)
  if #exact_command ~= 1 or tostring(exact_command[1][1]) ~= output_id then
    return redis.error_reply('STAGE8B_P1_COMMAND_MISSING')
  end
  local command_fields = exact_command[1][2]
  if #command_fields ~= 2 or command_fields[1] ~= 'payload'
     or command_fields[2] ~= envelope_payload then
    return redis.error_reply('STAGE8B_P1_COMMAND_CONFLICT')
  end
  return {'existing', output_id}
end

local output_id = redis.call('XADD', command_stream, '*', 'payload', envelope_payload)
redis.call('SET', marker_key, cjson.encode({
  schema_version = schema,
  domain = domain,
  source_stream = source,
  source_group = group,
  source_id = source_id,
  semantic_batch_id_sha256 = semantic_batch_id,
  strategy_request_id = request_id,
  canonical_command_sha256 = command_sha256,
  canonical_envelope_sha256 = envelope_sha256,
  command_stream = command_stream,
  command_group = command_group,
  command_entry_id = output_id,
  seal_generation = seal_generation,
  seal_commitment_sha256 = seal_commitment
}))
return {'published', output_id}
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1RedisConfig {
    pub consumer_name: String,
    pub read_count: usize,
    pub claim_count: usize,
    pub claim_idle_ms: u64,
    pub max_claim_pages: usize,
    pub retention_floor: usize,
}

impl Stage8bP1RedisConfig {
    pub fn paper_default_auto() -> Self {
        Self {
            consumer_name: format!("stage8b-p1-boot-{}", Uuid::new_v4().simple()),
            read_count: 1,
            claim_count: 32,
            claim_idle_ms: 30_000,
            max_claim_pages: 128,
            retention_floor: MIN_RETENTION_FLOOR,
        }
    }

    pub fn validate(&self) -> Result<(), Stage8bP1RedisSemanticError> {
        if !token(&self.consumer_name)
            || self.read_count != 1
            || self.claim_count == 0
            || self.claim_idle_ms == 0
            || self.max_claim_pages == 0
            || self.retention_floor < MIN_RETENTION_FLOOR
        {
            return Err(Stage8bP1RedisSemanticError::InvalidConfig);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1RedisM10PublishDisposition {
    Published,
    IdempotentExisting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8bP1RedisCommandPublicationDisposition {
    Published,
    IdempotentExisting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1RedisZeroIntentAckDisposition {
    AcknowledgedPending,
    AlreadyAcknowledged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8bP1RedisCommandPublicationReceipt {
    pub schema_version: u16,
    pub source_m10_redis_id: String,
    pub semantic_batch_id_sha256: String,
    pub strategy_request_id: StrategyRequestId,
    pub canonical_command_sha256: String,
    pub canonical_envelope_sha256: String,
    pub command_entry_id: String,
    pub covering_seal_generation: u64,
    pub covering_seal_commitment_sha256: String,
    pub disposition: Stage8bP1RedisCommandPublicationDisposition,
    pub m10_acknowledged: bool,
    pub paper_provider_invoked: bool,
    pub finam_transport_attached: bool,
    pub broker_network_dispatch_attached: bool,
    pub runtime_live: bool,
    pub real_orders: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1RedisSemanticError {
    #[error("Stage 8B-P1 Redis config is invalid")]
    InvalidConfig,
    #[error("Stage 8B-P1 Redis operation failed: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("Stage 8B-P1 canonical M10 failed validation: {0}")]
    CanonicalM10(#[from] Stage8bP1CanonicalM10Error),
    #[error("Stage 8B-P1 durable transition failed: {0}")]
    Durable(#[from] Stage7bRecoveryError),
    #[error("Stage 8B-P1 local semantic composition failed: {0}")]
    Semantic(#[from] Stage8bP1SemanticCompositionError),
    #[error("Stage 8B-P1 Redis group is absent or inconsistent")]
    GroupMissing,
    #[error("Stage 8B-P1 exact M10 source is absent or not pending")]
    ExactPendingEntryMissing,
    #[error("Stage 8B-P1 exact M10 source conflicts with durable evidence")]
    ExactSourceConflict,
    #[error("Stage 8B-P1 command publication conflicts with durable evidence")]
    CommandPublicationConflict,
    #[error("Stage 8B-P1 Redis reply is malformed or ambiguous")]
    InvalidRedisReply,
    #[error("Stage 8B-P1 retained M10 floor was violated")]
    RetentionViolation,
}

struct Stage8bP1RedisBackend {
    connection: ConnectionManager,
    namespace: Stage8bP1RedisNamespace,
    config: Stage8bP1RedisConfig,
    claim_cursor: String,
    groups_verified: bool,
}

pub async fn connect_stage8b_p1_redis(
    redis_url: &str,
    config: Stage8bP1RedisConfig,
) -> Result<Stage8bP1RedisSemanticCompositionTransport, Stage8bP1RedisSemanticError> {
    config.validate()?;
    let client = redis::Client::open(redis_url)?;
    let connection = ConnectionManager::new(client).await?;
    let mut backend = Stage8bP1RedisBackend {
        connection,
        namespace: stage8b_p1_redis_namespace(),
        config,
        claim_cursor: "0-0".to_string(),
        groups_verified: false,
    };
    backend.ensure_groups().await?;
    Ok(Stage8bP1RedisSemanticCompositionTransport { backend })
}

/// Linear transport handle. It exposes no raw Redis connection, arbitrary
/// namespace, XACK, command publication or provider method.
pub struct Stage8bP1RedisSemanticCompositionTransport {
    backend: Stage8bP1RedisBackend,
}

impl Stage8bP1RedisSemanticCompositionTransport {
    pub fn consumer_name(&self) -> &str {
        &self.backend.config.consumer_name
    }

    pub fn claim_cursor(&self) -> &str {
        &self.backend.claim_cursor
    }

    pub async fn publish_canonical_m10(
        &mut self,
        canonical_bytes: &[u8],
        expected_operational_identity_sha256: &str,
    ) -> Result<Stage8bP1RedisM10PublishDisposition, Stage8bP1RedisSemanticError> {
        self.backend
            .publish_canonical_m10(canonical_bytes, expected_operational_identity_sha256)
            .await
    }

    pub async fn retained_m10_count(&mut self) -> Result<usize, Stage8bP1RedisSemanticError> {
        self.backend.retained_m10_count().await
    }
}

pub struct Stage8bP1RedisSemanticCompositionOwner {
    stage7: Stage7bRecoveryReadyOwner,
    transport: Stage8bP1RedisSemanticCompositionTransport,
}

impl Stage8bP1RedisSemanticCompositionOwner {
    pub fn new(
        stage7: Stage7bRecoveryReadyOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
    ) -> Self {
        Self { stage7, transport }
    }

    pub fn transport_mut(&mut self) -> &mut Stage8bP1RedisSemanticCompositionTransport {
        &mut self.transport
    }

    pub async fn process_next(
        mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisSemanticOutcome, Stage8bP1RedisSemanticError> {
        let delivery = self.transport.backend.read_next_pending().await?;
        let operational_identity_sha256 = self
            .stage7
            .stage8b_p1_operational_identity_sha256()
            .to_string();
        let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
        let accepted_bar = delivery
            .parse_exact(&operational_identity_sha256)?
            .into_stage5c_semantic_bar()?;
        match self
            .stage7
            .commit_stage8b_p1_semantic(accepted_bar, binding, commitment_key)?
        {
            Stage8bP1SemanticCommitOutcome::ZeroIntent { owner, receipt } => {
                let disposition = self.transport.backend.acknowledge_exact(&delivery).await?;
                Ok(Stage8bP1RedisSemanticOutcome::Ready {
                    owner: Box::new(Self {
                        stage7: *owner,
                        transport: self.transport,
                    }),
                    receipt: Box::new(receipt),
                    ack_disposition: disposition,
                })
            }
            Stage8bP1SemanticCommitOutcome::OneIntentPrepublication(durable) => {
                Ok(Stage8bP1RedisSemanticOutcome::Prepublication(Box::new(
                    Stage8bP1RedisPrepublicationPending {
                        durable: *durable,
                        transport: self.transport,
                        pending_m10: delivery,
                    },
                )))
            }
            Stage8bP1SemanticCommitOutcome::MultiIntentBlocked(durable) => {
                Ok(Stage8bP1RedisSemanticOutcome::MultiIntentBlocked {
                    semantic_batch_id_sha256: durable.semantic_batch_id_sha256().to_string(),
                    intent_count: durable.intent_count(),
                })
            }
        }
    }
}

pub enum Stage8bP1RedisSemanticOutcome {
    Ready {
        owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
        receipt: Box<Stage8bP1ZeroIntentCommitReceipt>,
        ack_disposition: Stage8bP1RedisZeroIntentAckDisposition,
    },
    Prepublication(Box<Stage8bP1RedisPrepublicationPending>),
    MultiIntentBlocked {
        semantic_batch_id_sha256: String,
        intent_count: usize,
    },
}

pub struct Stage8bP1RedisPrepublicationPending {
    durable: Stage8bP1SemanticPrepublicationOwner,
    transport: Stage8bP1RedisSemanticCompositionTransport,
    pending_m10: Stage8bP1PendingM10Delivery,
}

impl Stage8bP1RedisPrepublicationPending {
    pub fn evidence(&self) -> &Stage6Stage8bP1SemanticCommitEvidenceV1 {
        self.durable.evidence()
    }

    pub fn pending_m10_redis_id(&self) -> &str {
        self.pending_m10.redis_id()
    }

    pub fn paper_provider_invocation_allowed(&self) -> bool {
        false
    }

    pub fn m10_xack_allowed(&self) -> bool {
        false
    }

    pub async fn publish_exact_command(
        mut self,
    ) -> Result<Stage8bP1RedisCommandPublished, Stage8bP1RedisSemanticError> {
        let receipt = self
            .transport
            .backend
            .publish_exact_command(&self.durable, &self.pending_m10)
            .await?;
        let (stage7, evidence, command) = self.durable.into_p1c_parts();
        Ok(Stage8bP1RedisCommandPublished {
            stage7,
            evidence,
            command,
            transport: self.transport,
            pending_m10: self.pending_m10,
            receipt,
        })
    }
}

/// P1-c terminal output. The command is in Redis, while the source M10 stays
/// pending. P1-d will be the only allowed consumer of the retained owner.
pub struct Stage8bP1RedisCommandPublished {
    #[allow(
        dead_code,
        reason = "linear continuation is intentionally sealed until P1-d"
    )]
    stage7: Stage7bRecoveryReadyOwner,
    evidence: Stage6Stage8bP1SemanticCommitEvidenceV1,
    command: BrokerCommand,
    #[allow(
        dead_code,
        reason = "linear continuation is intentionally sealed until P1-d"
    )]
    transport: Stage8bP1RedisSemanticCompositionTransport,
    pending_m10: Stage8bP1PendingM10Delivery,
    receipt: Stage8bP1RedisCommandPublicationReceipt,
}

impl Stage8bP1RedisCommandPublished {
    pub fn evidence(&self) -> &Stage6Stage8bP1SemanticCommitEvidenceV1 {
        &self.evidence
    }

    pub fn receipt(&self) -> &Stage8bP1RedisCommandPublicationReceipt {
        &self.receipt
    }

    pub fn pending_m10_redis_id(&self) -> &str {
        self.pending_m10.redis_id()
    }

    pub fn command_matches_durable_evidence(&self) -> bool {
        self.evidence.canonical_command_sha256.as_deref()
            == serde_json::to_vec(&self.command)
                .ok()
                .map(|bytes| sha256_hex(&bytes))
                .as_deref()
    }

    pub fn paper_provider_invocation_allowed(&self) -> bool {
        false
    }

    pub fn m10_xack_allowed(&self) -> bool {
        false
    }

    pub fn finam_transport_attached(&self) -> bool {
        false
    }

    pub fn runtime_live_enabled(&self) -> bool {
        false
    }

    pub fn real_orders_enabled(&self) -> bool {
        false
    }
}

pub struct Stage8bP1RedisZeroIntentAckResolved {
    owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
    disposition: Stage8bP1RedisZeroIntentAckDisposition,
    evidence: Stage6Stage8bP1SemanticCommitEvidenceV1,
    stage5c_callback_count: usize,
}

impl Stage8bP1RedisZeroIntentAckResolved {
    pub fn disposition(&self) -> Stage8bP1RedisZeroIntentAckDisposition {
        self.disposition
    }

    pub fn evidence(&self) -> &Stage6Stage8bP1SemanticCommitEvidenceV1 {
        &self.evidence
    }

    pub fn stage5c_callback_count(&self) -> usize {
        self.stage5c_callback_count
    }

    pub fn into_ready_owner(self) -> Box<Stage8bP1RedisSemanticCompositionOwner> {
        self.owner
    }
}

pub async fn resolve_stage8b_p1_zero_intent_ack_with_redis(
    pending: P1SemanticZeroIntentAckPending,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1RedisZeroIntentAckResolved, Stage8bP1RedisSemanticError> {
    let evidence = pending.evidence().clone();
    validate_zero_intent_evidence(&evidence)?;
    let delivery = transport
        .backend
        .exact_delivery_for_evidence(&evidence, pending.operational_identity_sha256())
        .await?;
    let disposition = transport.backend.acknowledge_exact(&delivery).await?;
    let stage5c_callback_count = pending.stage5c_callback_count();
    let stage7 = pending.into_ready_after_exact_source_resolution();
    Ok(Stage8bP1RedisZeroIntentAckResolved {
        owner: Box::new(Stage8bP1RedisSemanticCompositionOwner { stage7, transport }),
        disposition,
        evidence,
        stage5c_callback_count,
    })
}

pub async fn resume_stage8b_p1_journal_ahead_with_redis(
    pending: P1SemanticPrepublicationPending,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisPrepublicationPending, Stage8bP1RedisSemanticError> {
    let delivery = transport.backend.reclaim_single_pending().await?;
    let operational_identity_sha256 = pending.operational_identity_sha256().to_string();
    let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
    let accepted_bar = delivery
        .parse_exact(&operational_identity_sha256)?
        .into_stage5c_semantic_bar()?;
    let durable =
        pending.complete_with_exact_semantic_input(accepted_bar, binding, commitment_key)?;
    Ok(Stage8bP1RedisPrepublicationPending {
        durable,
        transport,
        pending_m10: delivery,
    })
}

pub async fn resume_stage8b_p1_prepublication_with_redis(
    durable: Stage8bP1SemanticPrepublicationOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1RedisPrepublicationPending, Stage8bP1RedisSemanticError> {
    let evidence = durable.evidence().clone();
    let delivery = transport.backend.reclaim_exact_evidence(&evidence).await?;
    Ok(Stage8bP1RedisPrepublicationPending {
        durable,
        transport,
        pending_m10: delivery,
    })
}

impl Stage8bP1RedisBackend {
    async fn ensure_groups(&mut self) -> Result<(), Stage8bP1RedisSemanticError> {
        ensure_group(
            &mut self.connection,
            &self.namespace.canonical_m10_stream,
            &self.namespace.m10_consumer_group,
        )
        .await?;
        ensure_group(
            &mut self.connection,
            &self.namespace.canonical_command_stream,
            &self.namespace.stage7b_command_consumer_group,
        )
        .await?;
        self.verify_group(
            &self.namespace.canonical_m10_stream.clone(),
            &self.namespace.m10_consumer_group.clone(),
        )
        .await?;
        self.verify_group(
            &self.namespace.canonical_command_stream.clone(),
            &self.namespace.stage7b_command_consumer_group.clone(),
        )
        .await?;
        self.groups_verified = true;
        Ok(())
    }

    async fn verify_group(
        &mut self,
        stream: &str,
        group: &str,
    ) -> Result<(), Stage8bP1RedisSemanticError> {
        let pending: redis::RedisResult<redis::streams::StreamPendingReply> =
            redis::cmd("XPENDING")
                .arg(stream)
                .arg(group)
                .query_async(&mut self.connection)
                .await;
        pending
            .map(|_| ())
            .map_err(|_| Stage8bP1RedisSemanticError::GroupMissing)
    }

    async fn publish_canonical_m10(
        &mut self,
        canonical_bytes: &[u8],
        expected_operational_identity_sha256: &str,
    ) -> Result<Stage8bP1RedisM10PublishDisposition, Stage8bP1RedisSemanticError> {
        if !self.groups_verified {
            return Err(Stage8bP1RedisSemanticError::GroupMissing);
        }
        let m10 =
            parse_stage8b_p1_canonical_m10(canonical_bytes, expected_operational_identity_sha256)?;
        let payload = std::str::from_utf8(m10.canonical_bytes())
            .map_err(|_| Stage8bP1RedisSemanticError::InvalidRedisReply)?;
        let result: redis::RedisResult<String> = redis::cmd("EVAL")
            .arg(M10_PUBLICATION_LUA)
            .arg(1)
            .arg(&self.namespace.canonical_m10_stream)
            .arg(&self.namespace.m10_consumer_group)
            .arg(m10.redis_id())
            .arg(payload)
            .query_async(&mut self.connection)
            .await;
        match result {
            Ok(returned) if returned == m10.redis_id() => {
                Ok(Stage8bP1RedisM10PublishDisposition::Published)
            }
            Ok(_) => Err(Stage8bP1RedisSemanticError::InvalidRedisReply),
            Err(error) if error.to_string().contains("STAGE8B_P1_M10_GROUP_MISSING") => {
                Err(Stage8bP1RedisSemanticError::GroupMissing)
            }
            Err(_) => {
                let existing = self.exact_stream_entry(m10.redis_id()).await?;
                if existing.as_deref() == Some(payload) {
                    Ok(Stage8bP1RedisM10PublishDisposition::IdempotentExisting)
                } else {
                    Err(Stage8bP1RedisSemanticError::ExactSourceConflict)
                }
            }
        }
    }

    async fn read_next_pending(
        &mut self,
    ) -> Result<Stage8bP1PendingM10Delivery, Stage8bP1RedisSemanticError> {
        if !self.groups_verified {
            return Err(Stage8bP1RedisSemanticError::GroupMissing);
        }
        let reply: StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(&self.namespace.m10_consumer_group)
            .arg(&self.config.consumer_name)
            .arg("COUNT")
            .arg(self.config.read_count)
            .arg("STREAMS")
            .arg(&self.namespace.canonical_m10_stream)
            .arg(">")
            .query_async(&mut self.connection)
            .await?;
        let mut entries = reply.keys.into_iter().flat_map(|key| key.ids);
        let entry = entries
            .next()
            .ok_or(Stage8bP1RedisSemanticError::ExactPendingEntryMissing)?;
        if entries.next().is_some() {
            return Err(Stage8bP1RedisSemanticError::InvalidRedisReply);
        }
        delivery_from_entry(entry)
    }

    async fn reclaim_single_pending(
        &mut self,
    ) -> Result<Stage8bP1PendingM10Delivery, Stage8bP1RedisSemanticError> {
        let pending = self.pending_entries("-", "+", 2).await?;
        if pending.ids.len() != 1 {
            return Err(Stage8bP1RedisSemanticError::ExactPendingEntryMissing);
        }
        let expected = pending.ids[0].id.clone();
        self.reclaim_exact_id(&expected).await
    }

    async fn reclaim_exact_evidence(
        &mut self,
        evidence: &Stage6Stage8bP1SemanticCommitEvidenceV1,
    ) -> Result<Stage8bP1PendingM10Delivery, Stage8bP1RedisSemanticError> {
        if evidence.intent_count != 1
            || evidence.strategy_request_id.is_none()
            || evidence.canonical_command_sha256.is_none()
        {
            return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
        }
        let pending = self.pending_entries("-", "+", 2).await?;
        if pending.ids.len() != 1 || pending.ids[0].id != evidence.m10_redis_id {
            return Err(Stage8bP1RedisSemanticError::ExactPendingEntryMissing);
        }
        let delivery = self.reclaim_exact_id(&evidence.m10_redis_id).await?;
        if delivery.semantic_id_sha256() != evidence.m10_semantic_id_sha256
            || delivery.payload_sha256() != evidence.m10_payload_sha256
        {
            return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
        }
        Ok(delivery)
    }

    async fn reclaim_exact_id(
        &mut self,
        expected_id: &str,
    ) -> Result<Stage8bP1PendingM10Delivery, Stage8bP1RedisSemanticError> {
        for _ in 0..self.config.max_claim_pages {
            let start = self.claim_cursor.clone();
            let reply: StreamAutoClaimReply = redis::cmd("XAUTOCLAIM")
                .arg(&self.namespace.canonical_m10_stream)
                .arg(&self.namespace.m10_consumer_group)
                .arg(&self.config.consumer_name)
                .arg(self.config.claim_idle_ms)
                .arg(&start)
                .arg("COUNT")
                .arg(self.config.claim_count)
                .query_async(&mut self.connection)
                .await?;
            self.claim_cursor = reply.next_stream_id;
            for entry in reply.claimed {
                if entry.id == expected_id {
                    return delivery_from_entry(entry);
                }
            }
            if self.claim_cursor == "0-0" || self.claim_cursor == start {
                self.claim_cursor = "0-0".to_string();
                break;
            }
        }
        Err(Stage8bP1RedisSemanticError::ExactPendingEntryMissing)
    }

    async fn exact_delivery_for_evidence(
        &mut self,
        evidence: &Stage6Stage8bP1SemanticCommitEvidenceV1,
        expected_operational_identity_sha256: &str,
    ) -> Result<Stage8bP1PendingM10Delivery, Stage8bP1RedisSemanticError> {
        let payload = self
            .exact_stream_entry(&evidence.m10_redis_id)
            .await?
            .ok_or(Stage8bP1RedisSemanticError::ExactPendingEntryMissing)?;
        let validated = parse_stage8b_p1_canonical_m10(
            payload.as_bytes(),
            expected_operational_identity_sha256,
        )?;
        if validated.semantic_id_sha256() != evidence.m10_semantic_id_sha256
            || validated.payload_sha256() != evidence.m10_payload_sha256
        {
            return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
        }
        Ok(Stage8bP1PendingM10Delivery {
            redis_id: evidence.m10_redis_id.clone(),
            semantic_id_sha256: evidence.m10_semantic_id_sha256.clone(),
            payload_sha256: evidence.m10_payload_sha256.clone(),
            canonical_bytes: payload.into_bytes(),
        })
    }

    async fn acknowledge_exact(
        &mut self,
        delivery: &Stage8bP1PendingM10Delivery,
    ) -> Result<Stage8bP1RedisZeroIntentAckDisposition, Stage8bP1RedisSemanticError> {
        let existing = self
            .exact_stream_entry(delivery.redis_id())
            .await?
            .ok_or(Stage8bP1RedisSemanticError::ExactPendingEntryMissing)?;
        if existing.as_bytes() != delivery.canonical_bytes.as_slice() {
            return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
        }
        let pending = self
            .pending_entries(delivery.redis_id(), delivery.redis_id(), 2)
            .await?;
        match pending.ids.len() {
            0 => Ok(Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged),
            1 if pending.ids[0].id == delivery.redis_id() => {
                let acknowledged: usize = redis::cmd("XACK")
                    .arg(&self.namespace.canonical_m10_stream)
                    .arg(&self.namespace.m10_consumer_group)
                    .arg(delivery.redis_id())
                    .query_async(&mut self.connection)
                    .await?;
                if acknowledged == 1 {
                    Ok(Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending)
                } else {
                    let after = self
                        .pending_entries(delivery.redis_id(), delivery.redis_id(), 2)
                        .await?;
                    if after.ids.is_empty() {
                        Ok(Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged)
                    } else {
                        Err(Stage8bP1RedisSemanticError::InvalidRedisReply)
                    }
                }
            }
            _ => Err(Stage8bP1RedisSemanticError::ExactSourceConflict),
        }
    }

    async fn publish_exact_command(
        &mut self,
        durable: &Stage8bP1SemanticPrepublicationOwner,
        delivery: &Stage8bP1PendingM10Delivery,
    ) -> Result<Stage8bP1RedisCommandPublicationReceipt, Stage8bP1RedisSemanticError> {
        let evidence = durable.evidence();
        let request_id = evidence
            .strategy_request_id
            .ok_or(Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let command_bytes = serde_json::to_vec(durable.command())
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let command_sha256 = sha256_hex(&command_bytes);
        if evidence.intent_count != 1
            || evidence.canonical_command_sha256.as_deref() != Some(&command_sha256)
            || delivery.redis_id() != evidence.m10_redis_id
            || delivery.semantic_id_sha256() != evidence.m10_semantic_id_sha256
            || delivery.payload_sha256() != evidence.m10_payload_sha256
        {
            return Err(Stage8bP1RedisSemanticError::CommandPublicationConflict);
        }
        let envelope = Envelope {
            schema_version: SCHEMA_VERSION,
            ts_utc: command_created_at(durable.command()),
            source: COMMAND_ENVELOPE_SOURCE.to_string(),
            msg_type: MessageType::Command,
            payload: durable.command().clone(),
        };
        let envelope_bytes = serde_json::to_vec(&envelope)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        runtime_command_bridge::decode_stage7a_pre_admission(&envelope_bytes)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let envelope_payload = std::str::from_utf8(&envelope_bytes)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let envelope_sha256 = sha256_hex(&envelope_bytes);
        let source_payload = std::str::from_utf8(&delivery.canonical_bytes)
            .map_err(|_| Stage8bP1RedisSemanticError::ExactSourceConflict)?;
        let marker_key = publication_marker_key(&self.namespace, request_id);
        let result: Vec<String> = redis::cmd("EVAL")
            .arg(COMMAND_PUBLICATION_LUA)
            .arg(3)
            .arg(&self.namespace.canonical_m10_stream)
            .arg(&self.namespace.canonical_command_stream)
            .arg(marker_key)
            .arg(&self.namespace.m10_consumer_group)
            .arg(delivery.redis_id())
            .arg(source_payload)
            .arg(&evidence.semantic_batch_id_sha256)
            .arg(request_id.to_string())
            .arg(&command_sha256)
            .arg(&envelope_sha256)
            .arg(envelope_payload)
            .arg(durable.recovery_seal_generation())
            .arg(durable.recovery_seal_commitment_sha256())
            .arg(COMMAND_PUBLICATION_MARKER_SCHEMA_VERSION)
            .arg(COMMAND_PUBLICATION_MARKER_DOMAIN)
            .arg(&self.namespace.stage7b_command_consumer_group)
            .query_async(&mut self.connection)
            .await?;
        let [classification, command_entry_id] = result.as_slice() else {
            return Err(Stage8bP1RedisSemanticError::InvalidRedisReply);
        };
        let disposition = match classification.as_str() {
            "published" => Stage8bP1RedisCommandPublicationDisposition::Published,
            "existing" => Stage8bP1RedisCommandPublicationDisposition::IdempotentExisting,
            _ => return Err(Stage8bP1RedisSemanticError::InvalidRedisReply),
        };
        Ok(Stage8bP1RedisCommandPublicationReceipt {
            schema_version: 1,
            source_m10_redis_id: delivery.redis_id().to_string(),
            semantic_batch_id_sha256: evidence.semantic_batch_id_sha256.clone(),
            strategy_request_id: request_id,
            canonical_command_sha256: command_sha256,
            canonical_envelope_sha256: envelope_sha256,
            command_entry_id: command_entry_id.clone(),
            covering_seal_generation: durable.recovery_seal_generation(),
            covering_seal_commitment_sha256: durable.recovery_seal_commitment_sha256().to_string(),
            disposition,
            m10_acknowledged: false,
            paper_provider_invoked: false,
            finam_transport_attached: false,
            broker_network_dispatch_attached: false,
            runtime_live: false,
            real_orders: false,
        })
    }

    async fn pending_entries(
        &mut self,
        start: &str,
        end: &str,
        count: usize,
    ) -> Result<StreamPendingCountReply, Stage8bP1RedisSemanticError> {
        Ok(redis::cmd("XPENDING")
            .arg(&self.namespace.canonical_m10_stream)
            .arg(&self.namespace.m10_consumer_group)
            .arg(start)
            .arg(end)
            .arg(count)
            .query_async(&mut self.connection)
            .await?)
    }

    async fn exact_stream_entry(
        &mut self,
        redis_id: &str,
    ) -> Result<Option<String>, Stage8bP1RedisSemanticError> {
        let reply: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&self.namespace.canonical_m10_stream)
            .arg(redis_id)
            .arg(redis_id)
            .query_async(&mut self.connection)
            .await?;
        match reply.ids.as_slice() {
            [] => Ok(None),
            [entry] if entry.id == redis_id && entry.map.len() == 1 => entry
                .get::<String>("payload")
                .map(Some)
                .ok_or(Stage8bP1RedisSemanticError::InvalidRedisReply),
            _ => Err(Stage8bP1RedisSemanticError::InvalidRedisReply),
        }
    }

    async fn retained_m10_count(&mut self) -> Result<usize, Stage8bP1RedisSemanticError> {
        let count: usize = redis::cmd("XLEN")
            .arg(&self.namespace.canonical_m10_stream)
            .query_async(&mut self.connection)
            .await?;
        // P1-c never invokes XTRIM/XDEL. The floor is an admission constraint
        // for future bounded retention, not permission to trim active input.
        if self.config.retention_floor < MIN_RETENTION_FLOOR {
            return Err(Stage8bP1RedisSemanticError::RetentionViolation);
        }
        Ok(count)
    }
}

async fn ensure_group(
    connection: &mut ConnectionManager,
    stream: &str,
    group: &str,
) -> Result<(), Stage8bP1RedisSemanticError> {
    let result: redis::RedisResult<()> = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(stream)
        .arg(group)
        .arg("0-0")
        .arg("MKSTREAM")
        .query_async(connection)
        .await;
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.to_string().contains("BUSYGROUP") => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn delivery_from_entry(
    entry: StreamId,
) -> Result<Stage8bP1PendingM10Delivery, Stage8bP1RedisSemanticError> {
    if entry.map.len() != 1 {
        return Err(Stage8bP1RedisSemanticError::InvalidRedisReply);
    }
    let payload = entry
        .get::<String>("payload")
        .ok_or(Stage8bP1RedisSemanticError::InvalidRedisReply)?;
    let validated = parse_stage8b_p1_canonical_m10_without_identity(payload.as_bytes())?;
    if validated.redis_id() != entry.id {
        return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
    }
    Ok(Stage8bP1PendingM10Delivery {
        redis_id: entry.id,
        semantic_id_sha256: validated.semantic_id_sha256().to_string(),
        payload_sha256: validated.payload_sha256().to_string(),
        canonical_bytes: payload.into_bytes(),
    })
}

fn parse_stage8b_p1_canonical_m10_without_identity(
    bytes: &[u8],
) -> Result<super::Stage8bP1ValidatedCanonicalM10, Stage8bP1RedisSemanticError> {
    #[derive(Deserialize)]
    struct IdentityProbe {
        payload: IdentityPayload,
    }
    #[derive(Deserialize)]
    struct IdentityPayload {
        operational_identity_sha256: String,
    }
    let probe: IdentityProbe = serde_json::from_slice(bytes)
        .map_err(|_| Stage8bP1RedisSemanticError::InvalidRedisReply)?;
    Ok(parse_stage8b_p1_canonical_m10(
        bytes,
        &probe.payload.operational_identity_sha256,
    )?)
}

fn validate_zero_intent_evidence(
    evidence: &Stage6Stage8bP1SemanticCommitEvidenceV1,
) -> Result<(), Stage8bP1RedisSemanticError> {
    if evidence.intent_count != 0
        || evidence.strategy_request_id.is_some()
        || evidence.canonical_command_sha256.is_some()
        || evidence.request_accepted_record_id.is_some()
        || evidence.request_accepted_source_evidence_sha256.is_some()
    {
        return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
    }
    Ok(())
}

fn publication_marker_key(
    namespace: &Stage8bP1RedisNamespace,
    request_id: StrategyRequestId,
) -> String {
    format!(
        "finam_imoexf_paper:{{{}}}:stage8b:p1:command-publication:{}",
        namespace.hash_tag,
        sha256_hex(request_id.to_string().as_bytes())
    )
}

fn command_created_at(command: &BrokerCommand) -> chrono::DateTime<chrono::Utc> {
    match command {
        BrokerCommand::PlaceOrder(command) => command.created_ts,
        BrokerCommand::CancelOrder(command) => command.created_ts,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage8b_p1_bootstrap::{
        authorize_stage8b_p1_first_boot, first_boot_stage8b_p1, restart_stage8b_p1,
        stage8b_p1_imoexf_instrument_map_fingerprint_sha256, validate_stage8b_p1_bootstrap_config,
        Stage8bP1BootstrapConfig, STAGE8B_P1_BOOTSTRAP_CONFIG_SCHEMA_VERSION, STAGE8B_P1_BROKER_ID,
        STAGE8B_P1_EXCHANGE, STAGE8B_P1_FIRST_BOOT_CONFIRMATION, STAGE8B_P1_INTERNAL_SYMBOL,
        STAGE8B_P1_MARKET, STAGE8B_P1_TICK_SIZE, STAGE8B_P1_VENUE_SYMBOL,
    };
    use crate::Stage7bRestartOutcome;
    use redis::streams::StreamPendingReply;
    use std::{
        fs,
        net::TcpListener,
        os::unix::fs::DirBuilderExt,
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        time::Duration,
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
                .expect("redis-server is required for the P1-c real-Redis proof");
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
                tokio::time::sleep(Duration::from_millis(10)).await;
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

    fn temp_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stage8b-p1c-{label}-{}-{}",
            std::process::id(),
            Uuid::new_v4().simple()
        ));
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder.create(&path).unwrap();
        fs::canonicalize(path).unwrap()
    }

    fn bootstrap_config(
        parent: PathBuf,
        runtime_config_fingerprint_sha256: String,
    ) -> Stage8bP1BootstrapConfig {
        Stage8bP1BootstrapConfig {
            schema_version: STAGE8B_P1_BOOTSTRAP_CONFIG_SCHEMA_VERSION,
            broker_id: STAGE8B_P1_BROKER_ID.to_string(),
            strategy_id: crate::STAGE8B_P1_STRATEGY_ID.to_string(),
            account_id: "ACC_TEST_0001".to_string(),
            internal_symbol: STAGE8B_P1_INTERNAL_SYMBOL.to_string(),
            venue_symbol: STAGE8B_P1_VENUE_SYMBOL.to_string(),
            exchange: STAGE8B_P1_EXCHANGE.to_string(),
            market: STAGE8B_P1_MARKET.to_string(),
            tick_size: STAGE8B_P1_TICK_SIZE.to_string(),
            runtime_config_fingerprint_sha256,
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

    fn first_boot(
        parent: &Path,
    ) -> (
        Stage7bRecoveryReadyOwner,
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
    ) {
        let (source, export_input, key, fresh) =
            strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let config = validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent.to_path_buf(),
            fresh.stage5c_config_fingerprint(),
        ))
        .unwrap();
        let admin =
            authorize_stage8b_p1_first_boot(&config, STAGE8B_P1_FIRST_BOOT_CONFIRMATION).unwrap();
        let outcome =
            first_boot_stage8b_p1(config, admin, source, export_input, &key, fresh.clone())
                .unwrap();
        let operational_identity_sha256 = outcome.receipt().operational_identity_sha256.clone();
        (
            outcome.into_owner(),
            key,
            fresh,
            operational_identity_sha256,
        )
    }

    fn source_m1(open_ts: i64) -> Vec<super::super::Stage8bP1CanonicalM10SourceM1> {
        (0..10)
            .map(|index| {
                let open = open_ts + index * 60_000;
                let close = open + 60_000;
                super::super::Stage8bP1CanonicalM10SourceM1 {
                    redis_id: format!("{close}-0"),
                    semantic_id_sha256: format!("{:064x}", index + 1),
                    payload_sha256: format!("{:064x}", index + 101),
                    open_ts_utc_ms: open,
                    close_ts_utc_ms: close,
                }
            })
            .collect()
    }

    fn canonical_m10(
        operational_identity_sha256: String,
        close_ts_utc_ms: i64,
        close_price: i64,
    ) -> Vec<u8> {
        let open_ts_utc_ms = close_ts_utc_ms - 600_000;
        super::super::build_stage8b_p1_canonical_m10(
            super::super::Stage8bP1CanonicalM10BuildInput {
                operational_identity_sha256,
                open_ts_utc_ms,
                close_ts_utc_ms,
                open: close_price.to_string(),
                high: (close_price + 1).to_string(),
                low: (close_price - 1).to_string(),
                close: close_price.to_string(),
                volume: "10000".to_string(),
                source_m1: source_m1(open_ts_utc_ms),
            },
        )
        .unwrap()
    }

    fn reclaim_config() -> Stage8bP1RedisConfig {
        let mut config = Stage8bP1RedisConfig::paper_default_auto();
        config.claim_idle_ms = 1;
        config.claim_count = 1;
        config.max_claim_pages = 16;
        config
    }

    async fn one_intent_pending(
        redis: &RedisServer,
        parent: &Path,
    ) -> (
        Stage8bP1RedisPrepublicationPending,
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
    ) {
        let (owner, key, fresh, identity) = first_boot(parent);
        let mut transport =
            connect_stage8b_p1_redis(&redis.url, Stage8bP1RedisConfig::paper_default_auto())
                .await
                .unwrap();
        let bytes = canonical_m10(identity.clone(), 1_785_759_000_000, 2_650);
        transport
            .publish_canonical_m10(&bytes, &identity)
            .await
            .unwrap();
        let outcome = Stage8bP1RedisSemanticCompositionOwner::new(owner, transport)
            .process_next(&key)
            .await
            .unwrap();
        let Stage8bP1RedisSemanticOutcome::Prepublication(pending) = outcome else {
            panic!("breakout M10 must produce one prepublication command");
        };
        (*pending, key, fresh, identity)
    }

    #[tokio::test]
    async fn p1c_real_redis_creates_groups_before_exact_m10_and_rejects_collision() {
        let redis = RedisServer::start().await;
        let mut transport =
            connect_stage8b_p1_redis(&redis.url, Stage8bP1RedisConfig::paper_default_auto())
                .await
                .unwrap();
        let namespace = stage8b_p1_redis_namespace();
        let identity = "11".repeat(32);
        let close_ts = 1_785_759_000_000_i64;
        let bytes = canonical_m10(identity.clone(), close_ts, 2_600);

        let mut connection = redis.connection().await;
        for (stream, group) in [
            (
                &namespace.canonical_m10_stream,
                &namespace.m10_consumer_group,
            ),
            (
                &namespace.canonical_command_stream,
                &namespace.stage7b_command_consumer_group,
            ),
        ] {
            let pending: StreamPendingReply = redis::cmd("XPENDING")
                .arg(stream)
                .arg(group)
                .query_async(&mut connection)
                .await
                .unwrap();
            assert_eq!(pending.count(), 0);
        }

        assert_eq!(
            transport
                .publish_canonical_m10(&bytes, &identity)
                .await
                .unwrap(),
            Stage8bP1RedisM10PublishDisposition::Published
        );
        assert_eq!(
            transport
                .publish_canonical_m10(&bytes, &identity)
                .await
                .unwrap(),
            Stage8bP1RedisM10PublishDisposition::IdempotentExisting
        );
        let changed = canonical_m10(identity.clone(), close_ts, 2_601);
        assert!(matches!(
            transport.publish_canonical_m10(&changed, &identity).await,
            Err(Stage8bP1RedisSemanticError::ExactSourceConflict)
        ));
        assert_eq!(transport.retained_m10_count().await.unwrap(), 1);

        let _: usize = redis::cmd("XGROUP")
            .arg("DESTROY")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        let later = canonical_m10(identity.clone(), close_ts + 600_000, 2_602);
        assert!(transport
            .publish_canonical_m10(&later, &identity)
            .await
            .is_err());
        assert!(matches!(
            transport.publish_canonical_m10(&bytes, &identity).await,
            Err(Stage8bP1RedisSemanticError::GroupMissing)
        ));
        assert_eq!(transport.retained_m10_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn p1c_zero_intent_xacks_last_and_restart_is_ack_only() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("zero-intent");
        let (owner, key, fresh, identity) = first_boot(&parent);
        let mut transport =
            connect_stage8b_p1_redis(&redis.url, Stage8bP1RedisConfig::paper_default_auto())
                .await
                .unwrap();
        let bytes = canonical_m10(identity.clone(), 1_785_759_000_000, 2_600);
        transport
            .publish_canonical_m10(&bytes, &identity)
            .await
            .unwrap();
        let outcome = Stage8bP1RedisSemanticCompositionOwner::new(owner, transport)
            .process_next(&key)
            .await
            .unwrap();
        let Stage8bP1RedisSemanticOutcome::Ready {
            owner,
            receipt,
            ack_disposition,
        } = outcome
        else {
            panic!("zero-intent M10 must settle only after S1");
        };
        assert_eq!(receipt.evidence.intent_count, 0);
        assert_eq!(
            ack_disposition,
            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
        );
        drop(owner);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(pending) = restart else {
            panic!("durable zero-intent S1 must resolve exact source ACK after restart");
        };
        let callback_count = pending.stage5c_callback_count();
        let transport =
            connect_stage8b_p1_redis(&redis.url, Stage8bP1RedisConfig::paper_default_auto())
                .await
                .unwrap();
        let resolved = resolve_stage8b_p1_zero_intent_ack_with_redis(*pending, transport)
            .await
            .unwrap();
        assert_eq!(
            resolved.disposition(),
            Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged
        );
        assert_eq!(resolved.stage5c_callback_count(), callback_count);
        drop(resolved);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1c_command_response_loss_republishes_exactly_once_and_retains_m10() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("command-response-loss");
        let (owner, key, fresh, identity) = first_boot(&parent);
        let mut transport =
            connect_stage8b_p1_redis(&redis.url, Stage8bP1RedisConfig::paper_default_auto())
                .await
                .unwrap();
        let bytes = canonical_m10(identity.clone(), 1_785_759_000_000, 2_650);
        transport
            .publish_canonical_m10(&bytes, &identity)
            .await
            .unwrap();
        let outcome = Stage8bP1RedisSemanticCompositionOwner::new(owner, transport)
            .process_next(&key)
            .await
            .unwrap();
        let Stage8bP1RedisSemanticOutcome::Prepublication(pending) = outcome else {
            panic!("breakout M10 must produce one prepublication command");
        };
        let published = pending.publish_exact_command().await.unwrap();
        assert_eq!(
            published.receipt().disposition,
            Stage8bP1RedisCommandPublicationDisposition::Published
        );
        assert!(published.command_matches_durable_evidence());
        assert!(!published.receipt().m10_acknowledged);
        assert!(!published.paper_provider_invocation_allowed());
        assert!(!published.m10_xack_allowed());
        drop(published);

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let command_count: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        let pending_count: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(command_count, 1);
        assert_eq!(pending_count.count(), 1);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticPrepublicationReady(durable) = restart else {
            panic!("S1 restart must retain exact prepublication command");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = connect_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let pending = resume_stage8b_p1_prepublication_with_redis(*durable, transport)
            .await
            .unwrap();
        let replayed = pending.publish_exact_command().await.unwrap();
        assert_eq!(
            replayed.receipt().disposition,
            Stage8bP1RedisCommandPublicationDisposition::IdempotentExisting
        );
        assert_eq!(
            replayed.receipt().command_entry_id,
            published_entry_id(&mut connection, &namespace).await
        );
        drop(replayed);
        let command_count: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        let pending_count: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(command_count, 1);
        assert_eq!(pending_count.count(), 1);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1c_command_publication_rejects_source_xacked_before_command() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("source-xacked-early");
        let (pending, _key, _fresh, _identity) = one_intent_pending(&redis, &parent).await;
        let source_id = pending.pending_m10_redis_id().to_string();
        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let acknowledged: usize = redis::cmd("XACK")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .arg(&source_id)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(acknowledged, 1);
        assert!(matches!(
            pending.publish_exact_command().await,
            Err(Stage8bP1RedisSemanticError::Redis(_))
        ));
        let command_count: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(command_count, 0);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1c_command_publication_rejects_missing_stage7_group_atomically() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("command-group-missing");
        let (pending, _key, _fresh, _identity) = one_intent_pending(&redis, &parent).await;
        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let removed: usize = redis::cmd("XGROUP")
            .arg("DESTROY")
            .arg(&namespace.canonical_command_stream)
            .arg(&namespace.stage7b_command_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(removed, 1);
        assert!(matches!(
            pending.publish_exact_command().await,
            Err(Stage8bP1RedisSemanticError::Redis(_))
        ));
        let command_count: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        let source_pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(command_count, 0);
        assert_eq!(source_pending.count(), 1);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1c_tampered_publication_marker_cannot_duplicate_command() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("tampered-marker");
        let (pending, key, fresh, _identity) = one_intent_pending(&redis, &parent).await;
        let published = pending.publish_exact_command().await.unwrap();
        let request_id = published.receipt().strategy_request_id;
        drop(published);

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let _: () = redis::cmd("SET")
            .arg(publication_marker_key(&namespace, request_id))
            .arg("{\"schema_version\":1}")
            .query_async(&mut connection)
            .await
            .unwrap();

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticPrepublicationReady(durable) = restart else {
            panic!("S1 restart must retain exact prepublication command");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = connect_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let pending = resume_stage8b_p1_prepublication_with_redis(*durable, transport)
            .await
            .unwrap();
        assert!(matches!(
            pending.publish_exact_command().await,
            Err(Stage8bP1RedisSemanticError::Redis(_))
        ));
        let command_count: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        let source_pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(command_count, 1);
        assert_eq!(source_pending.count(), 1);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1c_s1_restart_rejects_ambiguous_multi_entry_pel() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("ambiguous-pel");
        let (mut pending, key, fresh, identity) = one_intent_pending(&redis, &parent).await;
        let extra = canonical_m10(identity.clone(), 1_785_759_600_000, 2_651);
        pending
            .transport
            .publish_canonical_m10(&extra, &identity)
            .await
            .unwrap();
        let second_delivery = pending.transport.backend.read_next_pending().await.unwrap();
        assert_ne!(second_delivery.redis_id(), pending.pending_m10_redis_id());
        drop(pending);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticPrepublicationReady(durable) = restart else {
            panic!("S1 restart must retain prepublication authority");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = connect_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        assert!(matches!(
            resume_stage8b_p1_prepublication_with_redis(*durable, transport).await,
            Err(Stage8bP1RedisSemanticError::ExactPendingEntryMissing)
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    async fn published_entry_id(
        connection: &mut ConnectionManager,
        namespace: &Stage8bP1RedisNamespace,
    ) -> String {
        let reply: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&namespace.canonical_command_stream)
            .arg("-")
            .arg("+")
            .query_async(connection)
            .await
            .unwrap();
        reply.ids[0].id.clone()
    }

    #[tokio::test]
    async fn p1c_journal_ahead_reclaims_real_pel_before_reconstructing_s1() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("journal-ahead");
        let (owner, key, fresh, identity) = first_boot(&parent);
        let mut transport =
            connect_stage8b_p1_redis(&redis.url, Stage8bP1RedisConfig::paper_default_auto())
                .await
                .unwrap();
        let bytes = canonical_m10(identity.clone(), 1_785_759_000_000, 2_650);
        transport
            .publish_canonical_m10(&bytes, &identity)
            .await
            .unwrap();
        let delivery = transport.backend.read_next_pending().await.unwrap();
        let binding = binding_from_delivery(&delivery, identity.clone());
        let accepted_bar = delivery
            .parse_exact(&identity)
            .unwrap()
            .into_stage5c_semantic_bar()
            .unwrap();
        let before_crash = crate::recovery::stage8b_p1_test_stop_after_request_accepted(
            owner,
            accepted_bar,
            binding,
            &key,
        )
        .unwrap();
        drop(transport);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticPrepublicationPending(pending) = restart else {
            panic!("uncovered RequestAccepted must remain typed pending");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = connect_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let pending = resume_stage8b_p1_journal_ahead_with_redis(*pending, transport, &key)
            .await
            .unwrap();
        assert_eq!(
            pending.evidence().strategy_request_id,
            before_crash.strategy_request_id
        );
        let published = pending.publish_exact_command().await.unwrap();
        assert_eq!(
            published.receipt().disposition,
            Stage8bP1RedisCommandPublicationDisposition::Published
        );
        assert!(!published.m10_xack_allowed());
        drop(published);
        fs::remove_dir_all(parent).unwrap();
    }
}
