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
    Stage8bP1d2AckCommittedOwner, Stage8bP1d2FeedbackAuditEvidenceV1,
    Stage8bP1d2PreAckPendingOwner, Stage8bP1d2TruthCommittedOwner, Stage8bP1d3AckCommittedOwner,
    Stage8bP1d3CancelCommitOutcome, Stage8bP1d3CancelContinuationOwner,
    Stage8bP1d3DispatchPendingOwner, Stage8bP1d3LaterCommitOutcome, Stage8bP1d3PreAckPendingOwner,
    Stage8bP1d3RecoveredCommitOutcome, Stage8bP1d3SemanticPendingOwner,
    Stage8bP1d3TruthCommittedOwner, Stage8bP1d4GeneratedMarketAckCommittedOwner,
    Stage8bP1d4GeneratedMarketDispatchPendingOwner, Stage8bP1d4GeneratedMarketOrderPendingOwner,
    Stage8bP1d4GeneratedMarketPreAckPendingOwner,
    Stage8bP1d4GeneratedMarketPreFinalizationPendingOwner,
    Stage8bP1d4GeneratedMarketPrepublicationOwner, Stage8bP1d4GeneratedMarketTruthCommittedOwner,
};
use crate::stage8b_p1_bootstrap::{stage8b_p1_redis_namespace, Stage8bP1RedisNamespace};
use broker_core::{BrokerCommand, Envelope, MessageType, StrategyRequestId, SCHEMA_VERSION};
use redis::aio::ConnectionManager;
use redis::streams::{
    StreamAutoClaimReply, StreamId, StreamPendingCountReply, StreamRangeReply, StreamReadReply,
};
use redis::FromRedisValue;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use strategy_runtime_core::{
    Stage5gLifecycleCommitmentKey, Stage6Stage8bP1SemanticCommitEvidenceV1,
    Stage8bP1d1CommandDecisionBinding, Stage8bP1d1ExecutionScheduleAuthority,
    Stage8bP1d1MarketOutcomeBundle, Stage8bP1d3DayExpiryAuthority, Stage8bP1d3InitialObservation,
    Stage8bP1d3LaterObservation, Stage8bP1d3ScheduleStepAuthority,
    Stage8bP1d4CommandPublicationBindingV1, Stage8bP1d4CommandPublicationReservationV1,
};
use uuid::Uuid;

const COMMAND_ENVELOPE_SOURCE: &str = "stage8b-p1-semantic";
const COMMAND_PUBLICATION_MARKER_SCHEMA_VERSION: u16 = 1;
const COMMAND_PUBLICATION_MARKER_DOMAIN: &str = "moex.stage8b.p1.command-publication-marker.v1";
const P1D4_COMMAND_PUBLICATION_MARKER_SCHEMA_VERSION: u16 = 1;
const P1D4_COMMAND_PUBLICATION_MARKER_DOMAIN: &str =
    "moex.stage8b.p1d4.command-publication-marker.v1";
const MIN_RETENTION_FLOOR: usize = super::STAGE8B_P1_LOCAL_M10_MIN_RETENTION;

const NAMESPACE_INITIALIZATION_LUA: &str = r#"
local function type_name(key)
  local result = redis.call('TYPE', key)
  if type(result) == 'table' then return result['ok'] end
  return result
end

local function exact_initial_group(stream, expected)
  local groups = redis.call('XINFO', 'GROUPS', stream)
  if #groups ~= 1 then return false end
  local name = nil
  local last_delivered_id = nil
  local pending = nil
  local consumers = nil
  for index = 1, #groups[1], 2 do
    local key = groups[1][index]
    if key == 'name' then name = groups[1][index + 1] end
    if key == 'last-delivered-id' then last_delivered_id = groups[1][index + 1] end
    if key == 'pending' then pending = tonumber(groups[1][index + 1]) end
    if key == 'consumers' then consumers = tonumber(groups[1][index + 1]) end
  end
  return name == expected and last_delivered_id == '0-0'
     and pending == 0 and consumers == 0
end

local m10_stream = KEYS[1]
local command_stream = KEYS[2]
local m10_group = ARGV[1]
local command_group = ARGV[2]
local m10_type = type_name(m10_stream)
local command_type = type_name(command_stream)

if m10_type == 'none' and command_type == 'none' then
  redis.call('XGROUP', 'CREATE', m10_stream, m10_group, '0-0', 'MKSTREAM')
  redis.call('XGROUP', 'CREATE', command_stream, command_group, '0-0', 'MKSTREAM')
  return 'initialized'
end

if m10_type ~= 'stream' or command_type ~= 'stream'
   or redis.call('XLEN', m10_stream) ~= 0
   or redis.call('XLEN', command_stream) ~= 0
   or not exact_initial_group(m10_stream, m10_group)
   or not exact_initial_group(command_stream, command_group) then
  return redis.error_reply('STAGE8B_P1_NAMESPACE_NOT_FRESH')
end
return 'idempotent_fresh'
"#;

const NAMESPACE_VERIFY_LUA: &str = r#"
local function type_name(key)
  local result = redis.call('TYPE', key)
  if type(result) == 'table' then return result['ok'] end
  return result
end

local function exact_group_frontier(stream, expected)
  local groups = redis.call('XINFO', 'GROUPS', stream)
  if #groups ~= 1 then return nil end
  local name = nil
  local last_delivered_id = nil
  for index = 1, #groups[1], 2 do
    local key = groups[1][index]
    if key == 'name' then name = groups[1][index + 1] end
    if key == 'last-delivered-id' then last_delivered_id = groups[1][index + 1] end
  end
  if name ~= expected or not last_delivered_id then return nil end
  return last_delivered_id
end

local m10_stream = KEYS[1]
local command_stream = KEYS[2]
if type_name(m10_stream) ~= 'stream' or type_name(command_stream) ~= 'stream' then
  return redis.error_reply('STAGE8B_P1_GROUP_MISSING')
end
local m10_frontier = exact_group_frontier(m10_stream, ARGV[1])
local command_frontier = exact_group_frontier(command_stream, ARGV[2])
if not m10_frontier or not command_frontier then
  return redis.error_reply('STAGE8B_P1_GROUP_MISSING')
end
return {m10_frontier, command_frontier}
"#;

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

const P1D4_COMMAND_PUBLICATION_LUA: &str = r#"
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

local function last_generated_id(stream)
  local info = redis.call('XINFO', 'STREAM', stream)
  for index = 1, #info, 2 do
    if info[index] == 'last-generated-id' then return tostring(info[index + 1]) end
  end
  return nil
end

local source = KEYS[1]
local command_stream = KEYS[2]
local marker_key = KEYS[3]
local source_group = ARGV[1]
local source_id = ARGV[2]
local source_payload = ARGV[3]
local command_group = ARGV[4]
local predecessor_id = ARGV[5]
local reserved_id = ARGV[6]
local envelope_payload = ARGV[7]
local marker_payload = ARGV[8]

if type_name(source) ~= 'stream' or type_name(command_stream) ~= 'stream' then
  return redis.error_reply('STAGE8B_P1D4_STREAM_TYPE')
end
if not has_group(command_stream, command_group) then
  return redis.error_reply('STAGE8B_P1D4_COMMAND_GROUP_MISSING')
end
local marker_type = type_name(marker_key)
if marker_type ~= 'none' and marker_type ~= 'string' then
  return redis.error_reply('STAGE8B_P1D4_MARKER_TYPE')
end

local exact_source = redis.call('XRANGE', source, source_id, source_id)
if #exact_source ~= 1 or tostring(exact_source[1][1]) ~= source_id then
  return redis.error_reply('STAGE8B_P1D4_SOURCE_MISSING')
end
local source_fields = exact_source[1][2]
if #source_fields ~= 2 or source_fields[1] ~= 'payload' or source_fields[2] ~= source_payload then
  return redis.error_reply('STAGE8B_P1D4_SOURCE_CONFLICT')
end
local pending = redis.call('XPENDING', source, source_group, source_id, source_id, 1)
if #pending ~= 1 or tostring(pending[1][1]) ~= source_id then
  return redis.error_reply('STAGE8B_P1D4_SOURCE_NOT_PENDING')
end

local existing_marker = redis.call('GET', marker_key)
local exact_reserved = redis.call('XRANGE', command_stream, reserved_id, reserved_id)
if existing_marker then
  if existing_marker ~= marker_payload then
    return redis.error_reply('STAGE8B_P1D4_PUBLICATION_CONFLICT')
  end
  if #exact_reserved ~= 1 or tostring(exact_reserved[1][1]) ~= reserved_id then
    return redis.error_reply('STAGE8B_P1D4_COMMAND_MISSING')
  end
  local command_fields = exact_reserved[1][2]
  if #command_fields ~= 2 or command_fields[1] ~= 'payload'
     or command_fields[2] ~= envelope_payload then
    return redis.error_reply('STAGE8B_P1D4_COMMAND_CONFLICT')
  end
  return {'existing', reserved_id}
end

if #exact_reserved ~= 0 then
  return redis.error_reply('STAGE8B_P1D4_MARKER_MISSING')
end
if last_generated_id(command_stream) ~= predecessor_id then
  return redis.error_reply('STAGE8B_P1D4_STREAM_ADVANCED')
end
local output_id = redis.call('XADD', command_stream, reserved_id, 'payload', envelope_payload)
if tostring(output_id) ~= reserved_id then
  return redis.error_reply('STAGE8B_P1D4_RESERVED_ID_MISMATCH')
end
redis.call('SET', marker_key, marker_payload)
return {'published', reserved_id}
"#;

// Read-only proof used after the precommitted publication has succeeded. The
// truth phase deliberately accepts an already-XACKed source so a lost XACK
// response remains recoverable; every earlier phase requires the exact PEL
// member to remain pending.
const P1D4_COMMAND_PUBLICATION_REVALIDATE_LUA: &str = r#"
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
local source_group = ARGV[1]
local source_id = ARGV[2]
local source_payload = ARGV[3]
local command_group = ARGV[4]
local reserved_id = ARGV[5]
local envelope_payload = ARGV[6]
local marker_payload = ARGV[7]
local pending_requirement = ARGV[8]

if type_name(source) ~= 'stream' or type_name(command_stream) ~= 'stream'
   or type_name(marker_key) ~= 'string' then
  return redis.error_reply('STAGE8B_P1D4_REVALIDATE_TYPE')
end
if not has_group(command_stream, command_group) then
  return redis.error_reply('STAGE8B_P1D4_COMMAND_GROUP_MISSING')
end

local exact_source = redis.call('XRANGE', source, source_id, source_id)
if #exact_source ~= 1 or tostring(exact_source[1][1]) ~= source_id then
  return redis.error_reply('STAGE8B_P1D4_SOURCE_MISSING')
end
local source_fields = exact_source[1][2]
if #source_fields ~= 2 or source_fields[1] ~= 'payload' or source_fields[2] ~= source_payload then
  return redis.error_reply('STAGE8B_P1D4_SOURCE_CONFLICT')
end

local pending = redis.call('XPENDING', source, source_group, source_id, source_id, 1)
if pending_requirement == 'required' then
  if #pending ~= 1 or tostring(pending[1][1]) ~= source_id then
    return redis.error_reply('STAGE8B_P1D4_SOURCE_NOT_PENDING')
  end
elseif pending_requirement ~= 'optional' then
  return redis.error_reply('STAGE8B_P1D4_PENDING_REQUIREMENT')
end

if redis.call('GET', marker_key) ~= marker_payload then
  return redis.error_reply('STAGE8B_P1D4_PUBLICATION_CONFLICT')
end
local exact_reserved = redis.call('XRANGE', command_stream, reserved_id, reserved_id)
if #exact_reserved ~= 1 or tostring(exact_reserved[1][1]) ~= reserved_id then
  return redis.error_reply('STAGE8B_P1D4_COMMAND_MISSING')
end
local command_fields = exact_reserved[1][2]
if #command_fields ~= 2 or command_fields[1] ~= 'payload'
   or command_fields[2] ~= envelope_payload then
  return redis.error_reply('STAGE8B_P1D4_COMMAND_CONFLICT')
end
return {'existing', reserved_id}
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publication_reservation_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publication_binding_sha256: Option<String>,
    pub disposition: Stage8bP1RedisCommandPublicationDisposition,
    pub m10_acknowledged: bool,
    pub paper_provider_invoked: bool,
    pub finam_transport_attached: bool,
    pub broker_network_dispatch_attached: bool,
    pub runtime_live: bool,
    pub real_orders: bool,
}

struct Stage8bP1d4PreparedCommandPublication {
    reservation: Stage8bP1d4CommandPublicationReservationV1,
    envelope_bytes: Vec<u8>,
}

#[derive(Serialize)]
struct Stage8bP1d4RedisPublicationMarkerV1<'a> {
    schema_version: u16,
    domain: &'static str,
    source_stream: &'a str,
    source_group: &'a str,
    source_m10_redis_id: &'a str,
    semantic_batch_id_sha256: &'a str,
    strategy_request_id: String,
    canonical_command_sha256: &'a str,
    canonical_envelope_sha256: &'a str,
    command_stream: &'a str,
    command_group: &'a str,
    command_stream_predecessor_id: &'a str,
    command_entry_id: &'a str,
    prepublication_package_generation: String,
    publication_reservation_sha256: &'a str,
    prepublication_seal_generation: String,
    prepublication_seal_commitment_sha256: &'a str,
    publication_binding_sha256: &'a str,
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
    #[error("Stage 8B-P1 Redis namespace is not an exact fresh initialization state")]
    NamespaceNotFresh,
    #[error("Stage 8B-P1 Ready source has more than one pending M10")]
    AmbiguousReadyPendingEntries,
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
    #[error("Stage 8B-P1-d1 decision binding conflicts with the published command")]
    P1d1DecisionBindingConflict,
}

struct Stage8bP1RedisBackend {
    connection: ConnectionManager,
    namespace: Stage8bP1RedisNamespace,
    config: Stage8bP1RedisConfig,
    claim_cursor: String,
    groups_verified: bool,
}

enum Stage8bP1ReadySourceAcquisition {
    Delivery(Stage8bP1PendingM10Delivery),
    PendingNotClaimable(String),
}

/// One-shot namespace initialization. Group creation is allowed only when
/// both streams are absent, or when both are still in the exact empty initial
/// state. Historical or partially initialized namespaces fail closed.
pub async fn initialize_stage8b_p1_redis_namespace(
    redis_url: &str,
    config: Stage8bP1RedisConfig,
) -> Result<Stage8bP1RedisSemanticCompositionTransport, Stage8bP1RedisSemanticError> {
    let mut backend = open_backend(redis_url, config).await?;
    backend.initialize_fresh_namespace().await?;
    backend.verify_groups().await?;
    Ok(Stage8bP1RedisSemanticCompositionTransport { backend })
}

/// Normal and restart attachment is verify-only. It never creates or repairs
/// a stream or consumer group.
pub async fn attach_stage8b_p1_redis(
    redis_url: &str,
    config: Stage8bP1RedisConfig,
) -> Result<Stage8bP1RedisSemanticCompositionTransport, Stage8bP1RedisSemanticError> {
    let mut backend = open_backend(redis_url, config).await?;
    backend.verify_groups().await?;
    Ok(Stage8bP1RedisSemanticCompositionTransport { backend })
}

async fn open_backend(
    redis_url: &str,
    config: Stage8bP1RedisConfig,
) -> Result<Stage8bP1RedisBackend, Stage8bP1RedisSemanticError> {
    config.validate()?;
    let client = redis::Client::open(redis_url)?;
    let connection = ConnectionManager::new(client).await?;
    Ok(Stage8bP1RedisBackend {
        connection,
        namespace: stage8b_p1_redis_namespace(),
        config,
        claim_cursor: "0-0".to_string(),
        groups_verified: false,
    })
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
        let delivery = match self.transport.backend.acquire_ready_delivery().await? {
            Stage8bP1ReadySourceAcquisition::Delivery(delivery) => delivery,
            Stage8bP1ReadySourceAcquisition::PendingNotClaimable(redis_id) => {
                return Ok(Stage8bP1RedisSemanticOutcome::PendingNotClaimable {
                    owner: Box::new(self),
                    pending_m10_redis_id: redis_id,
                });
            }
        };
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

    /// Evaluates the exact next canonical M10 against the authenticated
    /// P1-d3 working order. S_eval/S_terminal is committed and reread before
    /// the same delivery is allowed to reach the Hybrid callback.
    pub async fn process_next_working_limit(
        mut self,
        schedule_authority: Stage8bP1d3ScheduleStepAuthority,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisSemanticOutcome, Stage8bP1RedisSemanticError> {
        let delivery = match self.transport.backend.acquire_ready_delivery().await? {
            Stage8bP1ReadySourceAcquisition::Delivery(delivery) => delivery,
            Stage8bP1ReadySourceAcquisition::PendingNotClaimable(redis_id) => {
                return Ok(Stage8bP1RedisSemanticOutcome::PendingNotClaimable {
                    owner: Box::new(self),
                    pending_m10_redis_id: redis_id,
                });
            }
        };
        crate::recovery::stage8b_p1d4_test_crash_frontier("F11");
        let operational_identity_sha256 = self
            .stage7
            .stage8b_p1_operational_identity_sha256()
            .to_string();
        let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
        let evidence = delivery
            .parse_exact(&operational_identity_sha256)?
            .into_p1d3_limit_evidence()?;
        let accepted_bar = delivery
            .parse_exact(&operational_identity_sha256)?
            .into_stage5c_semantic_bar()?;
        let pending_semantic_source = self.stage7.recovered_stage8b_p1d3_pending_semantic_source();
        if pending_semantic_source.as_ref().is_some_and(|expected| {
            expected.redis_id() == delivery.redis_id()
                && expected.semantic_id_sha256() == delivery.semantic_id_sha256()
                && expected.payload_sha256() == delivery.payload_sha256()
        }) {
            let exact_source = pending_semantic_source
                .as_ref()
                .expect("checked P1-d3 pending semantic source");
            let pending = self
                .stage7
                .into_stage8b_p1d3_pending_semantic_for_exact_source(exact_source)?;
            return complete_stage8b_p1d3_semantic(
                pending,
                self.transport,
                delivery,
                accepted_bar,
                binding,
                commitment_key,
            )
            .await;
        }
        let pending = match self.stage7.commit_stage8b_p1d3_later_limit(
            Stage8bP1d3LaterObservation::Candidate {
                evidence: Box::new(evidence),
                schedule: schedule_authority,
            },
            commitment_key,
        )? {
            Stage8bP1d3LaterCommitOutcome::SemanticPending(pending) => *pending,
            Stage8bP1d3LaterCommitOutcome::Ready(_) => {
                return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
            }
        };
        complete_stage8b_p1d3_semantic(
            pending,
            self.transport,
            delivery,
            accepted_bar,
            binding,
            commitment_key,
        )
        .await
    }

    /// Consumes a one-use Stage 5E day-boundary authority after the exact last
    /// eligible M10 has already completed. No Redis source is acquired and no
    /// Hybrid callback is run at the boundary itself.
    pub fn expire_working_limit(
        self,
        authority: Stage8bP1d3DayExpiryAuthority,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Self, Stage8bP1RedisSemanticError> {
        match self.stage7.commit_stage8b_p1d3_later_limit(
            Stage8bP1d3LaterObservation::DayExpiry { authority },
            commitment_key,
        )? {
            Stage8bP1d3LaterCommitOutcome::Ready(stage7) => Ok(Self {
                stage7: *stage7,
                transport: self.transport,
            }),
            Stage8bP1d3LaterCommitOutcome::SemanticPending(_) => {
                Err(Stage8bP1RedisSemanticError::ExactSourceConflict)
            }
        }
    }
}

async fn complete_stage8b_p1d3_semantic(
    pending: Stage8bP1d3SemanticPendingOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    delivery: Stage8bP1PendingM10Delivery,
    accepted_bar: strategy_runtime_core::Stage5cAcceptedSemanticBar,
    binding: strategy_runtime_core::Stage5gP1SemanticBindingInput,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisSemanticOutcome, Stage8bP1RedisSemanticError> {
    let outcome = pending.commit_exact_semantic(accepted_bar, binding, commitment_key)?;
    match outcome {
        Stage8bP1SemanticCommitOutcome::ZeroIntent { owner, receipt } => {
            crate::recovery::stage8b_p1d4_test_crash_frontier("F15");
            let disposition = transport.backend.acknowledge_exact(&delivery).await?;
            crate::recovery::stage8b_p1d4_test_crash_frontier("F16");
            Ok(Stage8bP1RedisSemanticOutcome::Ready {
                owner: Box::new(Stage8bP1RedisSemanticCompositionOwner {
                    stage7: *owner,
                    transport,
                }),
                receipt: Box::new(receipt),
                ack_disposition: disposition,
            })
        }
        Stage8bP1SemanticCommitOutcome::OneIntentPrepublication(durable) => {
            if !durable.stage8b_p1d4_generated_market_candidate() {
                crate::recovery::stage8b_p1d4_test_crash_frontier("F15");
            }
            Ok(Stage8bP1RedisSemanticOutcome::Prepublication(Box::new(
                Stage8bP1RedisPrepublicationPending {
                    durable: *durable,
                    transport,
                    pending_m10: delivery,
                },
            )))
        }
        Stage8bP1SemanticCommitOutcome::MultiIntentBlocked(durable) => {
            crate::recovery::stage8b_p1d4_test_crash_frontier("F15");
            Ok(Stage8bP1RedisSemanticOutcome::MultiIntentBlocked {
                semantic_batch_id_sha256: durable.semantic_batch_id_sha256().to_string(),
                intent_count: durable.intent_count(),
            })
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
    PendingNotClaimable {
        owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
        pending_m10_redis_id: String,
    },
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
        if self.durable.stage8b_p1d4_generated_market_candidate() {
            return Err(Stage8bP1RedisSemanticError::CommandPublicationConflict);
        }
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
            p1d4_reservation: None,
            p1d4_binding: None,
        })
    }

    /// Generated-Market publication first commits and rereads a reservation-
    /// bearing Stage 5G package and its covering seal, then uses exactly the
    /// reserved Redis ID. The accepted generic `XADD *` path is unreachable.
    pub async fn publish_exact_generated_market_command(
        mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisCommandPublished, Stage8bP1RedisSemanticError> {
        if !self.durable.stage8b_p1d4_generated_market_candidate() {
            return Err(Stage8bP1RedisSemanticError::CommandPublicationConflict);
        }
        let prepared = self
            .transport
            .backend
            .prepare_p1d4_command_publication(&self.durable, &self.pending_m10)
            .await?;
        let durable = self
            .durable
            .reserve_stage8b_p1d4_generated_market_publication(
                prepared.reservation.clone(),
                commitment_key,
            )?;
        crate::recovery::stage8b_p1d4_test_crash_frontier("GM00");
        let receipt = self
            .transport
            .backend
            .publish_reserved_p1d4_command(&durable, &self.pending_m10, &prepared)
            .await?;
        crate::recovery::stage8b_p1d4_test_crash_frontier("GM01");
        crate::recovery::stage8b_p1d4_test_crash_frontier("F15");
        let (stage7, evidence, command, reservation, binding) =
            durable.into_p1d4_publication_parts();
        Ok(Stage8bP1RedisCommandPublished {
            stage7,
            evidence,
            command,
            transport: self.transport,
            pending_m10: self.pending_m10,
            receipt,
            p1d4_reservation: Some(reservation),
            p1d4_binding: Some(binding),
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
    p1d4_reservation: Option<Stage8bP1d4CommandPublicationReservationV1>,
    p1d4_binding: Option<Stage8bP1d4CommandPublicationBindingV1>,
}

/// Exact source M10 remains pending while the replacement S_ack is already
/// durable and reread. ACK replay is structurally unavailable from this type.
pub struct Stage8bP1RedisFeedbackAckCommitted {
    durable: Stage8bP1d2AckCommittedOwner,
    transport: Stage8bP1RedisSemanticCompositionTransport,
    pending_m10: Stage8bP1PendingM10Delivery,
}

/// Exact source M10 remains pending while replacement S_truth is durable and
/// reread. Its only external mutation is source resolution via XACK.
pub struct Stage8bP1RedisFeedbackTruthCommitted {
    durable: Stage8bP1d2TruthCommittedOwner,
    transport: Stage8bP1RedisSemanticCompositionTransport,
    pending_m10: Stage8bP1PendingM10Delivery,
}

/// Generated-Market combined S_ack. Publication identity is retained for an
/// exact Redis revalidation before the truth-only continuation.
pub struct Stage8bP1RedisGeneratedMarketAckCommitted {
    durable: Stage8bP1d4GeneratedMarketAckCommittedOwner,
    evidence: Stage6Stage8bP1SemanticCommitEvidenceV1,
    command: BrokerCommand,
    reservation: Stage8bP1d4CommandPublicationReservationV1,
    transport: Stage8bP1RedisSemanticCompositionTransport,
    pending_m10: Stage8bP1PendingM10Delivery,
}

/// Generated-Market combined S_truth. Exact publication and source PEL are
/// revalidated before its sole external source-XACK mutation.
pub struct Stage8bP1RedisGeneratedMarketTruthCommitted {
    durable: Stage8bP1d4GeneratedMarketTruthCommittedOwner,
    evidence: Stage6Stage8bP1SemanticCommitEvidenceV1,
    command: BrokerCommand,
    reservation: Stage8bP1d4CommandPublicationReservationV1,
    transport: Stage8bP1RedisSemanticCompositionTransport,
    pending_m10: Stage8bP1PendingM10Delivery,
}

/// Exact command source remains pending while P1-d3 S_ack is durable and
/// reread. This type has no source-XACK or schedule observation method.
pub struct Stage8bP1RedisLimitAckCommitted {
    durable: Stage8bP1d3AckCommittedOwner,
    transport: Stage8bP1RedisSemanticCompositionTransport,
    pending_m10: Stage8bP1PendingM10Delivery,
}

/// Exact command source remains pending while P1-d3 S_working/S_terminal is
/// durable and reread. Only exact source acknowledgement remains.
pub struct Stage8bP1RedisLimitTruthCommitted {
    durable: Stage8bP1d3TruthCommittedOwner,
    transport: Stage8bP1RedisSemanticCompositionTransport,
    pending_m10: Stage8bP1PendingM10Delivery,
}

/// Target truth won the cancel race and is already replacement-sealed. This
/// owner keeps the original CANCEL source pending while exposing only the
/// no-input recovered-cancel continuation.
pub struct Stage8bP1RedisCancelContinuationPending {
    durable: Stage8bP1d3CancelContinuationOwner,
    transport: Stage8bP1RedisSemanticCompositionTransport,
    pending_m10: Stage8bP1PendingM10Delivery,
}

pub enum Stage8bP1RedisCancelCommitOutcome {
    AckCommitted(Stage8bP1RedisLimitAckCommitted),
    TruthCommitted(Stage8bP1RedisLimitTruthCommitted),
    CancelContinuationPending(Stage8bP1RedisCancelContinuationPending),
}

pub enum Stage8bP1RedisPreAckRecoveryOutcome {
    AckCommitted(Stage8bP1RedisLimitAckCommitted),
    TruthCommitted(Stage8bP1RedisLimitTruthCommitted),
    Semantic(Stage8bP1RedisSemanticOutcome),
}

pub struct Stage8bP1RedisLimitResolved {
    owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
    disposition: Stage8bP1RedisZeroIntentAckDisposition,
}

pub struct Stage8bP1RedisFeedbackResolved {
    owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
    disposition: Stage8bP1RedisZeroIntentAckDisposition,
    audit_evidence: Stage8bP1d2FeedbackAuditEvidenceV1,
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

    /// Issues an opaque decision binding from the authenticated runtime and
    /// its exact RequestAccepted record. The predecessor is retained in that
    /// runtime projection; no Redis/caller DTO may replace its identity or
    /// close time.
    pub fn p1d1_command_decision_binding(
        &self,
    ) -> Result<Stage8bP1d1CommandDecisionBinding, Stage8bP1RedisSemanticError> {
        let command_request_id = match &self.command {
            BrokerCommand::PlaceOrder(place) => place.request_id,
            BrokerCommand::CancelOrder(cancel) => cancel.request_id,
        };
        if self.evidence.strategy_request_id != Some(command_request_id)
            || !self.command_matches_durable_evidence()
            || self.pending_m10.redis_id() != self.evidence.m10_redis_id
            || self.receipt.source_m10_redis_id != self.evidence.m10_redis_id
        {
            return Err(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict);
        }
        self.stage7
            .stage8b_p1d1_command_decision_binding()
            .map_err(Stage8bP1RedisSemanticError::Durable)
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

    #[cfg(test)]
    fn stage8b_p1d4_test_append_dispatch_only(
        self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<(), Stage8bP1RedisSemanticError> {
        self.stage7
            .stage8b_p1d4_test_append_current_semantic_dispatch(commitment_key)?;
        drop(self.transport);
        Ok(())
    }

    /// Executes the sole deterministic P1-d1 Market provider path from the
    /// first retained canonical successor M10, then durably commits S_ack.
    /// The originating decision M10 remains pending throughout.
    pub async fn execute_next_canonical_market(
        mut self,
        schedule_authority: Stage8bP1d1ExecutionScheduleAuthority,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisFeedbackAckCommitted, Stage8bP1RedisSemanticError> {
        if self.p1d4_reservation.is_some() || self.p1d4_binding.is_some() {
            return Err(Stage8bP1RedisSemanticError::CommandPublicationConflict);
        }
        let operational_identity_sha256 = self
            .stage7
            .stage8b_p1_operational_identity_sha256()
            .to_string();
        let successor = self
            .transport
            .backend
            .exact_first_successor_m10(self.pending_m10.redis_id(), &operational_identity_sha256)
            .await?;
        let eligibility = self.stage7.stage8b_p1d1_execution_eligibility(
            schedule_authority,
            successor.into_p1d1_execution_evidence()?,
        )?;
        crate::recovery::stage8b_p1d4_test_crash_frontier("GM02");
        let provider = self
            .stage7
            .admit_p1d1_eligible_market_dispatch(eligibility)?;
        crate::recovery::stage8b_p1d4_test_crash_frontier("GM03");
        crate::recovery::stage8b_p1_test_crash_barrier(
            "p1d4-market-after-dispatch-before-provider-outcome",
        );
        let outcome = provider.execute();
        crate::recovery::stage8b_p1d4_test_crash_frontier("GM04");
        self.commit_market_feedback_ack(outcome, commitment_key)
    }

    /// Generated-Market continuation for a valid P1-d4 composite. The exact
    /// reservation, marker and command entry are revalidated before schedule
    /// selection; the accepted standalone P1-d2 path is unreachable here.
    pub async fn execute_next_canonical_generated_market(
        mut self,
        schedule_authority: Stage8bP1d1ExecutionScheduleAuthority,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisGeneratedMarketAckCommitted, Stage8bP1RedisSemanticError> {
        let reservation = self
            .p1d4_reservation
            .take()
            .ok_or(Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let binding = self
            .p1d4_binding
            .take()
            .ok_or(Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        self.transport
            .backend
            .revalidate_p1d4_publication(
                &self.evidence,
                &self.command,
                &self.pending_m10,
                &reservation,
                &binding,
                true,
            )
            .await?;
        let operational_identity_sha256 = self
            .stage7
            .stage8b_p1_operational_identity_sha256()
            .to_string();
        let successor = self
            .transport
            .backend
            .exact_first_successor_m10(self.pending_m10.redis_id(), &operational_identity_sha256)
            .await?;
        let eligibility = self.stage7.stage8b_p1d1_execution_eligibility(
            schedule_authority,
            successor.into_p1d1_execution_evidence()?,
        )?;
        crate::recovery::stage8b_p1d4_test_crash_frontier("GM02");
        let provider = self
            .stage7
            .admit_p1d1_eligible_market_dispatch(eligibility)?;
        crate::recovery::stage8b_p1d4_test_crash_frontier("GM03");
        crate::recovery::stage8b_p1_test_crash_barrier(
            "p1d4-market-after-dispatch-before-provider-outcome",
        );
        let outcome = provider.execute();
        crate::recovery::stage8b_p1d4_test_crash_frontier("GM04");
        if outcome.strategy_request_id()
            != self
                .evidence
                .strategy_request_id
                .ok_or(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict)?
            || outcome.canonical_command_sha256()
                != self
                    .evidence
                    .canonical_command_sha256
                    .as_deref()
                    .ok_or(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict)?
            || self.pending_m10.redis_id() != self.evidence.m10_redis_id
        {
            return Err(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict);
        }
        let durable = self.stage7.commit_stage8b_p1d4_generated_market_ack(
            outcome,
            binding,
            commitment_key,
        )?;
        Ok(Stage8bP1RedisGeneratedMarketAckCommitted {
            durable,
            evidence: self.evidence,
            command: self.command,
            reservation,
            transport: self.transport,
            pending_m10: self.pending_m10,
        })
    }

    /// Consumes the exact first retained canonical successor together with a
    /// one-use Stage 5E schedule authority. The initial LIMIT result is fully
    /// journaled and its replacement S_ack is committed+reread before this
    /// method returns. The decision M10 remains pending.
    pub async fn execute_next_canonical_limit(
        mut self,
        schedule_authority: Stage8bP1d3ScheduleStepAuthority,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisLimitAckCommitted, Stage8bP1RedisSemanticError> {
        crate::recovery::stage8b_p1d4_test_crash_frontier("F00");
        let operational_identity_sha256 = self
            .stage7
            .stage8b_p1_operational_identity_sha256()
            .to_string();
        let successor = self
            .transport
            .backend
            .exact_first_successor_m10(self.pending_m10.redis_id(), &operational_identity_sha256)
            .await?;
        crate::recovery::stage8b_p1d4_test_crash_frontier("F01");
        let observation = Stage8bP1d3InitialObservation::Candidate {
            evidence: Box::new(successor.into_p1d3_limit_evidence()?),
            schedule: schedule_authority,
        };
        self.commit_initial_limit_ack(observation, commitment_key)
    }

    /// Evaluates the target before settling an exact canonical CANCEL. The
    /// successor M10 is read without consumer acquisition; the original
    /// command source remains the sole pending delivery until cancel truth is
    /// replacement-sealed and acknowledged.
    pub async fn execute_next_canonical_cancel(
        mut self,
        schedule_authority: Stage8bP1d3ScheduleStepAuthority,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisCancelCommitOutcome, Stage8bP1RedisSemanticError> {
        if !matches!(self.command, BrokerCommand::CancelOrder(_))
            || self.pending_m10.redis_id() != self.evidence.m10_redis_id
            || !self.command_matches_durable_evidence()
        {
            return Err(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict);
        }
        crate::recovery::stage8b_p1d4_test_crash_frontier("F00");
        let operational_identity_sha256 = self
            .stage7
            .stage8b_p1_operational_identity_sha256()
            .to_string();
        let successor = self
            .transport
            .backend
            .exact_first_successor_m10(self.pending_m10.redis_id(), &operational_identity_sha256)
            .await?;
        crate::recovery::stage8b_p1d4_test_crash_frontier("F01");
        match self.stage7.commit_stage8b_p1d3_cancel(
            successor.into_p1d3_limit_evidence()?,
            schedule_authority,
            commitment_key,
        )? {
            Stage8bP1d3CancelCommitOutcome::AckCommitted(durable) => Ok(
                Stage8bP1RedisCancelCommitOutcome::AckCommitted(Stage8bP1RedisLimitAckCommitted {
                    durable: *durable,
                    transport: self.transport,
                    pending_m10: self.pending_m10,
                }),
            ),
            Stage8bP1d3CancelCommitOutcome::TruthCommitted(durable) => {
                Ok(Stage8bP1RedisCancelCommitOutcome::TruthCommitted(
                    Stage8bP1RedisLimitTruthCommitted {
                        durable: *durable,
                        transport: self.transport,
                        pending_m10: self.pending_m10,
                    },
                ))
            }
            Stage8bP1d3CancelCommitOutcome::CancelContinuationPending(durable) => Ok(
                Stage8bP1RedisCancelCommitOutcome::CancelContinuationPending(
                    Stage8bP1RedisCancelContinuationPending {
                        durable: *durable,
                        transport: self.transport,
                        pending_m10: self.pending_m10,
                    },
                ),
            ),
        }
    }

    /// Boundary-only initial Day expiry. The opaque authority is issued by
    /// Stage 5E; neither this Redis owner nor core consults wall clock or
    /// reconstructs a calendar boundary.
    pub fn execute_initial_limit_expiry(
        self,
        expiry_authority: Stage8bP1d3DayExpiryAuthority,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisLimitAckCommitted, Stage8bP1RedisSemanticError> {
        crate::recovery::stage8b_p1d4_test_crash_frontier("F00");
        crate::recovery::stage8b_p1d4_test_crash_frontier("F01");
        self.commit_initial_limit_ack(
            Stage8bP1d3InitialObservation::DayExpiry {
                authority: expiry_authority,
            },
            commitment_key,
        )
    }

    fn commit_initial_limit_ack(
        self,
        observation: Stage8bP1d3InitialObservation,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisLimitAckCommitted, Stage8bP1RedisSemanticError> {
        if self.pending_m10.redis_id() != self.evidence.m10_redis_id
            || !self.command_matches_durable_evidence()
        {
            return Err(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict);
        }
        let durable = self
            .stage7
            .commit_stage8b_p1d3_initial_limit_ack(observation, commitment_key)?;
        Ok(Stage8bP1RedisLimitAckCommitted {
            durable,
            transport: self.transport,
            pending_m10: self.pending_m10,
        })
    }

    /// Starts P1-d2 from the opaque deterministic P1-d1 outcome. Stage 6/7
    /// finalization and the replacement S_ack commit both complete before the
    /// returned capability exists; the source M10 is not acknowledged here.
    pub fn commit_market_feedback_ack(
        self,
        outcome: Stage8bP1d1MarketOutcomeBundle,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisFeedbackAckCommitted, Stage8bP1RedisSemanticError> {
        if self.p1d4_reservation.is_some() || self.p1d4_binding.is_some() {
            return Err(Stage8bP1RedisSemanticError::CommandPublicationConflict);
        }
        if outcome.strategy_request_id()
            != self
                .evidence
                .strategy_request_id
                .ok_or(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict)?
            || outcome.canonical_command_sha256()
                != self
                    .evidence
                    .canonical_command_sha256
                    .as_deref()
                    .ok_or(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict)?
            || self.pending_m10.redis_id() != self.evidence.m10_redis_id
        {
            return Err(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict);
        }
        let durable = self
            .stage7
            .commit_stage8b_p1d2_ack(outcome, commitment_key)?;
        Ok(Stage8bP1RedisFeedbackAckCommitted {
            durable,
            transport: self.transport,
            pending_m10: self.pending_m10,
        })
    }
}

impl Stage8bP1RedisLimitAckCommitted {
    pub fn pending_m10_redis_id(&self) -> &str {
        self.pending_m10.redis_id()
    }

    pub fn m10_xack_allowed(&self) -> bool {
        false
    }

    pub fn recovery_seal_generation(&self) -> u64 {
        self.durable.recovery_seal_generation()
    }

    pub fn recovery_seal_commitment_sha256(&self) -> &str {
        self.durable.recovery_seal_commitment_sha256()
    }

    pub fn commit_truth(
        self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisLimitTruthCommitted, Stage8bP1RedisSemanticError> {
        let durable = self.durable.commit_truth(commitment_key)?;
        Ok(Stage8bP1RedisLimitTruthCommitted {
            durable,
            transport: self.transport,
            pending_m10: self.pending_m10,
        })
    }
}

impl Stage8bP1RedisCancelContinuationPending {
    pub fn pending_m10_redis_id(&self) -> &str {
        self.pending_m10.redis_id()
    }

    pub fn market_or_schedule_input_allowed(&self) -> bool {
        false
    }

    pub fn m10_xack_allowed(&self) -> bool {
        false
    }

    #[cfg(test)]
    fn stage8b_p1d3_test_restart_snapshot(&self) -> (u64, u64, usize) {
        self.durable.stage8b_p1d3_test_restart_snapshot()
    }

    pub fn commit_recovered_cancel(
        self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisLimitTruthCommitted, Stage8bP1RedisSemanticError> {
        let durable = self.durable.commit_recovered_cancel(commitment_key)?;
        Ok(Stage8bP1RedisLimitTruthCommitted {
            durable,
            transport: self.transport,
            pending_m10: self.pending_m10,
        })
    }
}

impl Stage8bP1RedisLimitTruthCommitted {
    pub fn pending_m10_redis_id(&self) -> &str {
        self.pending_m10.redis_id()
    }

    pub fn m10_xack_allowed(&self) -> bool {
        true
    }

    pub fn recovery_seal_generation(&self) -> u64 {
        self.durable.recovery_seal_generation()
    }

    pub fn recovery_seal_commitment_sha256(&self) -> &str {
        self.durable.recovery_seal_commitment_sha256()
    }

    #[cfg(test)]
    fn stage8b_p1d3_test_restart_snapshot(&self) -> (u64, u64, usize) {
        self.durable.stage8b_p1d3_test_restart_snapshot()
    }

    pub async fn acknowledge_source(
        mut self,
    ) -> Result<Stage8bP1RedisLimitResolved, Stage8bP1RedisSemanticError> {
        let disposition = self
            .transport
            .backend
            .acknowledge_exact(&self.pending_m10)
            .await?;
        crate::recovery::stage8b_p1d4_test_crash_frontier("F16");
        let stage7 = self.durable.into_ready_after_source_resolution();
        Ok(Stage8bP1RedisLimitResolved {
            owner: Box::new(Stage8bP1RedisSemanticCompositionOwner {
                stage7,
                transport: self.transport,
            }),
            disposition,
        })
    }
}

impl Stage8bP1RedisLimitResolved {
    pub fn disposition(&self) -> Stage8bP1RedisZeroIntentAckDisposition {
        self.disposition
    }

    pub fn into_ready_owner(self) -> Box<Stage8bP1RedisSemanticCompositionOwner> {
        self.owner
    }
}

impl Stage8bP1RedisFeedbackAckCommitted {
    pub fn pending_m10_redis_id(&self) -> &str {
        self.pending_m10.redis_id()
    }

    pub fn m10_xack_allowed(&self) -> bool {
        false
    }

    pub fn recovery_seal_generation(&self) -> u64 {
        self.durable.recovery_seal_generation()
    }

    pub fn recovery_seal_commitment_sha256(&self) -> &str {
        self.durable.recovery_seal_commitment_sha256()
    }

    pub fn commit_truth(
        self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisFeedbackTruthCommitted, Stage8bP1RedisSemanticError> {
        let durable = self.durable.commit_truth(commitment_key)?;
        Ok(Stage8bP1RedisFeedbackTruthCommitted {
            durable,
            transport: self.transport,
            pending_m10: self.pending_m10,
        })
    }
}

impl Stage8bP1RedisFeedbackTruthCommitted {
    pub fn pending_m10_redis_id(&self) -> &str {
        self.pending_m10.redis_id()
    }

    pub fn m10_xack_allowed(&self) -> bool {
        true
    }

    pub fn recovery_seal_generation(&self) -> u64 {
        self.durable.recovery_seal_generation()
    }

    pub fn recovery_seal_commitment_sha256(&self) -> &str {
        self.durable.recovery_seal_commitment_sha256()
    }

    pub fn audit_evidence(
        &self,
    ) -> Result<Stage8bP1d2FeedbackAuditEvidenceV1, Stage8bP1RedisSemanticError> {
        Ok(self.durable.feedback_audit_evidence()?)
    }

    pub async fn acknowledge_source(
        mut self,
    ) -> Result<Stage8bP1RedisFeedbackResolved, Stage8bP1RedisSemanticError> {
        // Construct audit evidence from the authenticated/reread S_truth
        // before the sole terminal external mutation. Failure therefore
        // leaves the source pending.
        let audit_evidence = self.durable.feedback_audit_evidence()?;
        let disposition = self
            .transport
            .backend
            .acknowledge_exact(&self.pending_m10)
            .await?;
        let stage7 = self.durable.into_ready_after_source_resolution();
        Ok(Stage8bP1RedisFeedbackResolved {
            owner: Box::new(Stage8bP1RedisSemanticCompositionOwner {
                stage7,
                transport: self.transport,
            }),
            disposition,
            audit_evidence,
        })
    }
}

impl Stage8bP1RedisGeneratedMarketAckCommitted {
    pub fn pending_m10_redis_id(&self) -> &str {
        self.pending_m10.redis_id()
    }

    pub fn m10_xack_allowed(&self) -> bool {
        false
    }

    pub fn recovery_seal_generation(&self) -> u64 {
        self.durable.recovery_seal_generation()
    }

    pub fn recovery_seal_commitment_sha256(&self) -> &str {
        self.durable.recovery_seal_commitment_sha256()
    }

    pub async fn commit_truth(
        mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisGeneratedMarketTruthCommitted, Stage8bP1RedisSemanticError> {
        let binding = self.durable.publication_binding().clone();
        self.transport
            .backend
            .revalidate_p1d4_publication(
                &self.evidence,
                &self.command,
                &self.pending_m10,
                &self.reservation,
                &binding,
                true,
            )
            .await?;
        let durable = self.durable.commit_truth(commitment_key)?;
        Ok(Stage8bP1RedisGeneratedMarketTruthCommitted {
            durable,
            evidence: self.evidence,
            command: self.command,
            reservation: self.reservation,
            transport: self.transport,
            pending_m10: self.pending_m10,
        })
    }
}

impl Stage8bP1RedisGeneratedMarketTruthCommitted {
    pub fn pending_m10_redis_id(&self) -> &str {
        self.pending_m10.redis_id()
    }

    pub fn m10_xack_allowed(&self) -> bool {
        true
    }

    pub fn recovery_seal_generation(&self) -> u64 {
        self.durable.recovery_seal_generation()
    }

    pub fn recovery_seal_commitment_sha256(&self) -> &str {
        self.durable.recovery_seal_commitment_sha256()
    }

    pub fn audit_evidence(
        &self,
    ) -> Result<Stage8bP1d2FeedbackAuditEvidenceV1, Stage8bP1RedisSemanticError> {
        Ok(self.durable.feedback_audit_evidence()?)
    }

    pub async fn acknowledge_source(
        mut self,
    ) -> Result<Stage8bP1RedisFeedbackResolved, Stage8bP1RedisSemanticError> {
        let binding = self.durable.publication_binding().clone();
        self.transport
            .backend
            .revalidate_p1d4_publication(
                &self.evidence,
                &self.command,
                &self.pending_m10,
                &self.reservation,
                &binding,
                false,
            )
            .await?;
        let audit_evidence = self.durable.feedback_audit_evidence()?;
        let disposition = self
            .transport
            .backend
            .acknowledge_exact(&self.pending_m10)
            .await?;
        crate::recovery::stage8b_p1d4_test_crash_frontier("F16");
        let stage7 = self.durable.into_ready_after_source_resolution();
        Ok(Stage8bP1RedisFeedbackResolved {
            owner: Box::new(Stage8bP1RedisSemanticCompositionOwner {
                stage7,
                transport: self.transport,
            }),
            disposition,
            audit_evidence,
        })
    }
}

impl Stage8bP1RedisFeedbackResolved {
    pub fn disposition(&self) -> Stage8bP1RedisZeroIntentAckDisposition {
        self.disposition
    }

    pub fn into_ready_owner(self) -> Box<Stage8bP1RedisSemanticCompositionOwner> {
        self.owner
    }

    pub fn audit_evidence(&self) -> &Stage8bP1d2FeedbackAuditEvidenceV1 {
        &self.audit_evidence
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

/// Restores the reservation-bearing zero-suffix composite and performs only
/// the exact precommitted publication. This is the GM00-GM02 continuation;
/// it cannot fall back to generic `XADD *` publication.
pub async fn resume_stage8b_p1d4_prepublication_with_redis(
    durable: Stage8bP1d4GeneratedMarketPrepublicationOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1RedisCommandPublished, Stage8bP1RedisSemanticError> {
    let evidence = durable.evidence().clone();
    let pending_m10 = transport.backend.reclaim_exact_evidence(&evidence).await?;
    let prepared = prepare_recovered_p1d4_publication(&durable)?;
    let receipt = transport
        .backend
        .publish_reserved_p1d4_command(&durable, &pending_m10, &prepared)
        .await?;
    let (stage7, evidence, command, reservation, binding) = durable.into_p1d4_publication_parts();
    Ok(Stage8bP1RedisCommandPublished {
        stage7,
        evidence,
        command,
        transport,
        pending_m10,
        receipt,
        p1d4_reservation: Some(reservation),
        p1d4_binding: Some(binding),
    })
}

enum Stage8bP1d4JournalAheadPending {
    Dispatch(Stage8bP1d4GeneratedMarketDispatchPendingOwner),
    Order(Stage8bP1d4GeneratedMarketOrderPendingOwner),
    PreFinalization(Stage8bP1d4GeneratedMarketPreFinalizationPendingOwner),
    PreAck(Stage8bP1d4GeneratedMarketPreAckPendingOwner),
}

impl Stage8bP1d4JournalAheadPending {
    fn source_m10_evidence(
        &self,
    ) -> Result<Stage6Stage8bP1SemanticCommitEvidenceV1, Stage7bRecoveryError> {
        match self {
            Self::Dispatch(owner) => owner.source_m10_evidence(),
            Self::Order(owner) => owner.source_m10_evidence(),
            Self::PreFinalization(owner) => owner.source_m10_evidence(),
            Self::PreAck(owner) => owner.source_m10_evidence(),
        }
    }

    fn operational_identity_sha256(&self) -> &str {
        match self {
            Self::Dispatch(owner) => owner.operational_identity_sha256(),
            Self::Order(owner) => owner.operational_identity_sha256(),
            Self::PreFinalization(owner) => owner.operational_identity_sha256(),
            Self::PreAck(owner) => owner.operational_identity_sha256(),
        }
    }

    fn command_material(
        &self,
    ) -> Result<(Stage6Stage8bP1SemanticCommitEvidenceV1, BrokerCommand), Stage7bRecoveryError>
    {
        match self {
            Self::Dispatch(owner) => owner.command_material(),
            Self::Order(owner) => owner.command_material(),
            Self::PreFinalization(owner) => owner.command_material(),
            Self::PreAck(owner) => owner.command_material(),
        }
    }

    fn reservation(&self) -> &Stage8bP1d4CommandPublicationReservationV1 {
        match self {
            Self::Dispatch(owner) => owner.reservation(),
            Self::Order(owner) => owner.reservation(),
            Self::PreFinalization(owner) => owner.reservation(),
            Self::PreAck(owner) => owner.reservation(),
        }
    }

    fn publication_binding(&self) -> &Stage8bP1d4CommandPublicationBindingV1 {
        match self {
            Self::Dispatch(owner) => owner.publication_binding(),
            Self::Order(owner) => owner.publication_binding(),
            Self::PreFinalization(owner) => owner.publication_binding(),
            Self::PreAck(owner) => owner.publication_binding(),
        }
    }

    fn commit_reconstructed_ack(
        self,
        canonical_m10: strategy_runtime_core::Stage8bP1d1CanonicalM10Evidence,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1d4GeneratedMarketAckCommittedOwner, Stage7bRecoveryError> {
        match self {
            Self::Dispatch(owner) => owner.commit_reconstructed_ack(canonical_m10, commitment_key),
            Self::Order(owner) => owner.commit_reconstructed_ack(canonical_m10, commitment_key),
            Self::PreFinalization(owner) => {
                owner.commit_reconstructed_ack(canonical_m10, commitment_key)
            }
            Self::PreAck(owner) => owner.commit_reconstructed_ack(canonical_m10, commitment_key),
        }
    }
}

async fn resume_stage8b_p1d4_journal_ahead_with_redis(
    durable: Stage8bP1d4JournalAheadPending,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisGeneratedMarketAckCommitted, Stage8bP1RedisSemanticError> {
    let source = durable.source_m10_evidence()?;
    let pending_m10 = transport.backend.reclaim_exact_evidence(&source).await?;
    let (evidence, command) = durable.command_material()?;
    let reservation = durable.reservation().clone();
    let binding = durable.publication_binding().clone();
    transport
        .backend
        .revalidate_p1d4_publication(
            &evidence,
            &command,
            &pending_m10,
            &reservation,
            &binding,
            true,
        )
        .await?;
    let successor = transport
        .backend
        .exact_first_successor_m10(
            pending_m10.redis_id(),
            durable.operational_identity_sha256(),
        )
        .await?;
    let durable = durable
        .commit_reconstructed_ack(successor.into_p1d1_execution_evidence()?, commitment_key)?;
    Ok(Stage8bP1RedisGeneratedMarketAckCommitted {
        durable,
        evidence,
        command,
        reservation,
        transport,
        pending_m10,
    })
}

macro_rules! define_stage8b_p1d4_journal_ahead_resume {
    ($function:ident, $owner:ty, $variant:ident) => {
        pub async fn $function(
            durable: $owner,
            transport: Stage8bP1RedisSemanticCompositionTransport,
            commitment_key: &Stage5gLifecycleCommitmentKey,
        ) -> Result<Stage8bP1RedisGeneratedMarketAckCommitted, Stage8bP1RedisSemanticError> {
            resume_stage8b_p1d4_journal_ahead_with_redis(
                Stage8bP1d4JournalAheadPending::$variant(durable),
                transport,
                commitment_key,
            )
            .await
        }
    };
}

define_stage8b_p1d4_journal_ahead_resume!(
    resume_stage8b_p1d4_dispatch_pending_with_redis,
    Stage8bP1d4GeneratedMarketDispatchPendingOwner,
    Dispatch
);
define_stage8b_p1d4_journal_ahead_resume!(
    resume_stage8b_p1d4_order_pending_with_redis,
    Stage8bP1d4GeneratedMarketOrderPendingOwner,
    Order
);
define_stage8b_p1d4_journal_ahead_resume!(
    resume_stage8b_p1d4_pre_finalization_with_redis,
    Stage8bP1d4GeneratedMarketPreFinalizationPendingOwner,
    PreFinalization
);
define_stage8b_p1d4_journal_ahead_resume!(
    resume_stage8b_p1d4_pre_ack_with_redis,
    Stage8bP1d4GeneratedMarketPreAckPendingOwner,
    PreAck
);

/// Reattaches only the exact retained source and immutable publication of a
/// combined S_ack. The sole continuation remains truth application.
pub async fn resume_stage8b_p1d4_ack_with_redis(
    durable: Stage8bP1d4GeneratedMarketAckCommittedOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1RedisGeneratedMarketAckCommitted, Stage8bP1RedisSemanticError> {
    let (evidence, command, reservation) = durable.redis_resume_material()?;
    let pending_m10 = transport.backend.reclaim_exact_evidence(&evidence).await?;
    let binding = durable.publication_binding().clone();
    transport
        .backend
        .revalidate_p1d4_publication(
            &evidence,
            &command,
            &pending_m10,
            &reservation,
            &binding,
            true,
        )
        .await?;
    Ok(Stage8bP1RedisGeneratedMarketAckCommitted {
        durable,
        evidence,
        command,
        reservation,
        transport,
        pending_m10,
    })
}

/// Reattaches S_truth without requiring the source to remain in the PEL. The
/// relaxed PEL condition is limited to this authenticated terminal phase and
/// permits exact `AlreadyAcknowledged` recovery after a lost XACK response.
pub async fn resume_stage8b_p1d4_truth_with_redis(
    durable: Stage8bP1d4GeneratedMarketTruthCommittedOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1RedisGeneratedMarketTruthCommitted, Stage8bP1RedisSemanticError> {
    let (evidence, command, reservation) = durable.redis_resume_material()?;
    let pending_m10 = transport
        .backend
        .exact_delivery_for_evidence(&evidence, durable.operational_identity_sha256())
        .await?;
    let binding = durable.publication_binding().clone();
    transport
        .backend
        .revalidate_p1d4_publication(
            &evidence,
            &command,
            &pending_m10,
            &reservation,
            &binding,
            false,
        )
        .await?;
    Ok(Stage8bP1RedisGeneratedMarketTruthCommitted {
        durable,
        evidence,
        command,
        reservation,
        transport,
        pending_m10,
    })
}

/// Reattaches only the exact pending M10 retained by authenticated S_ack.
/// The returned owner has no ACK API and can continue with truth only.
pub async fn resume_stage8b_p1d2_ack_with_redis(
    durable: Stage8bP1d2AckCommittedOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1RedisFeedbackAckCommitted, Stage8bP1RedisSemanticError> {
    let binding = durable.source_m10_binding()?;
    let delivery = transport
        .backend
        .reclaim_exact_binding(
            binding.redis_id(),
            binding.semantic_id_sha256(),
            binding.payload_sha256(),
            durable.operational_identity_sha256(),
        )
        .await?;
    Ok(Stage8bP1RedisFeedbackAckCommitted {
        durable,
        transport,
        pending_m10: delivery,
    })
}

/// Resumes either pre-S_ack crash frontier from durable Stage 6/7 facts and
/// the exact retained canonical successor M10. It never invokes the provider;
/// the reconstructed ACK is the only allowed continuation.
pub async fn resume_stage8b_p1d2_pre_ack_with_redis(
    durable: Stage8bP1d2PreAckPendingOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisFeedbackAckCommitted, Stage8bP1RedisSemanticError> {
    let predecessor = durable.predecessor_m10_binding()?;
    let pending_m10 = transport
        .backend
        .reclaim_exact_evidence(&predecessor)
        .await?;
    let successor = transport
        .backend
        .exact_first_successor_m10(
            pending_m10.redis_id(),
            durable.operational_identity_sha256(),
        )
        .await?;
    let durable = durable
        .commit_reconstructed_ack(successor.into_p1d1_execution_evidence()?, commitment_key)?;
    Ok(Stage8bP1RedisFeedbackAckCommitted {
        durable,
        transport,
        pending_m10,
    })
}

/// Reattaches only the exact pending M10 retained by authenticated S_truth.
/// Source XACK is the sole continuation exposed by the returned owner.
pub async fn resume_stage8b_p1d2_truth_with_redis(
    durable: Stage8bP1d2TruthCommittedOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1RedisFeedbackTruthCommitted, Stage8bP1RedisSemanticError> {
    let binding = durable.source_m10_binding()?;
    let delivery = transport
        .backend
        .exact_delivery_for_binding(
            binding.redis_id(),
            binding.semantic_id_sha256(),
            binding.payload_sha256(),
            durable.operational_identity_sha256(),
        )
        .await?;
    Ok(Stage8bP1RedisFeedbackTruthCommitted {
        durable,
        transport,
        pending_m10: delivery,
    })
}

/// Reattaches the exact pending decision M10 and reconstructs replacement
/// S_ack solely from the authenticated V3 outcome already durable in Stage 6.
/// No schedule/provider/candidate lookup is repeated on this path.
pub async fn resume_stage8b_p1d3_pre_ack_with_redis(
    durable: Stage8bP1d3PreAckPendingOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisPreAckRecoveryOutcome, Stage8bP1RedisSemanticError> {
    let source_is_command = durable.source_is_command_m10();
    let operational_identity_sha256 = durable.operational_identity_sha256().to_string();
    let pending_m10 = if source_is_command {
        let evidence = durable.source_m10_evidence()?;
        transport.backend.reclaim_exact_evidence(&evidence).await?
    } else {
        let source = durable
            .candidate_semantic_source_binding()
            .ok_or(Stage8bP1RedisSemanticError::ExactSourceConflict)?;
        transport
            .backend
            .reclaim_exact_binding(
                source.redis_id(),
                source.semantic_id_sha256(),
                source.payload_sha256(),
                &operational_identity_sha256,
            )
            .await?
    };
    let semantic_input = if source_is_command {
        None
    } else {
        let binding = binding_from_delivery(&pending_m10, operational_identity_sha256.clone());
        let accepted_bar = pending_m10
            .parse_exact(&operational_identity_sha256)?
            .into_stage5c_semantic_bar()?;
        Some((accepted_bar, binding))
    };
    match durable.commit_reconstructed_transition(commitment_key)? {
        Stage8bP1d3RecoveredCommitOutcome::AckCommitted(durable) => Ok(
            Stage8bP1RedisPreAckRecoveryOutcome::AckCommitted(Stage8bP1RedisLimitAckCommitted {
                durable: *durable,
                transport,
                pending_m10,
            }),
        ),
        Stage8bP1d3RecoveredCommitOutcome::TruthCommitted(durable) => {
            Ok(Stage8bP1RedisPreAckRecoveryOutcome::TruthCommitted(
                Stage8bP1RedisLimitTruthCommitted {
                    durable: *durable,
                    transport,
                    pending_m10,
                },
            ))
        }
        Stage8bP1d3RecoveredCommitOutcome::CancelContinuationPending(durable) => {
            let durable = durable.commit_recovered_cancel(commitment_key)?;
            Ok(Stage8bP1RedisPreAckRecoveryOutcome::TruthCommitted(
                Stage8bP1RedisLimitTruthCommitted {
                    durable,
                    transport,
                    pending_m10,
                },
            ))
        }
        Stage8bP1d3RecoveredCommitOutcome::SemanticCallbackPending(durable) => {
            let (accepted_bar, binding) =
                semantic_input.ok_or(Stage8bP1RedisSemanticError::ExactSourceConflict)?;
            let outcome = complete_stage8b_p1d3_semantic(
                *durable,
                transport,
                pending_m10,
                accepted_bar,
                binding,
                commitment_key,
            )
            .await?;
            Ok(Stage8bP1RedisPreAckRecoveryOutcome::Semantic(outcome))
        }
    }
}

/// Reclaims the exact command M10 and its first successor for a dispatch-only
/// LIMIT frontier. The recovered durable owner reuses the existing dispatch;
/// this composition owns no dispatch helper of its own.
pub async fn resume_stage8b_p1d3_dispatch_limit_with_redis(
    durable: Stage8bP1d3DispatchPendingOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    schedule: Stage8bP1d3ScheduleStepAuthority,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisLimitAckCommitted, Stage8bP1RedisSemanticError> {
    if !durable.is_limit_place() {
        return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
    }
    let source = durable.source_m10_evidence()?;
    let pending_m10 = transport.backend.reclaim_exact_evidence(&source).await?;
    let successor = transport
        .backend
        .exact_first_successor_m10(
            pending_m10.redis_id(),
            durable.operational_identity_sha256(),
        )
        .await?;
    let observation = Stage8bP1d3InitialObservation::Candidate {
        evidence: Box::new(successor.into_p1d3_limit_evidence()?),
        schedule,
    };
    let durable = durable.commit_limit(observation, commitment_key)?;
    Ok(Stage8bP1RedisLimitAckCommitted {
        durable,
        transport,
        pending_m10,
    })
}

/// Reissues only the exact Stage 5E day-boundary authority for a dispatch-only
/// initial LIMIT expiry. The original command source remains pending.
pub async fn resume_stage8b_p1d3_dispatch_expiry_with_redis(
    durable: Stage8bP1d3DispatchPendingOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    authority: Stage8bP1d3DayExpiryAuthority,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisLimitAckCommitted, Stage8bP1RedisSemanticError> {
    if !durable.is_limit_place() {
        return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
    }
    let source = durable.source_m10_evidence()?;
    let pending_m10 = transport.backend.reclaim_exact_evidence(&source).await?;
    let durable = durable.commit_limit(
        Stage8bP1d3InitialObservation::DayExpiry { authority },
        commitment_key,
    )?;
    Ok(Stage8bP1RedisLimitAckCommitted {
        durable,
        transport,
        pending_m10,
    })
}

/// Reclaims the exact command M10 and first successor for a dispatch-only
/// CANCEL frontier. Target-first fills retain the accepted two-V3
/// continuation through `CancelContinuationPending`.
pub async fn resume_stage8b_p1d3_dispatch_cancel_with_redis(
    durable: Stage8bP1d3DispatchPendingOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    schedule: Stage8bP1d3ScheduleStepAuthority,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisCancelCommitOutcome, Stage8bP1RedisSemanticError> {
    if !durable.is_cancel() {
        return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
    }
    let source = durable.source_m10_evidence()?;
    let pending_m10 = transport.backend.reclaim_exact_evidence(&source).await?;
    let successor = transport
        .backend
        .exact_first_successor_m10(
            pending_m10.redis_id(),
            durable.operational_identity_sha256(),
        )
        .await?;
    match durable.commit_cancel(
        successor.into_p1d3_limit_evidence()?,
        schedule,
        commitment_key,
    )? {
        Stage8bP1d3CancelCommitOutcome::AckCommitted(durable) => Ok(
            Stage8bP1RedisCancelCommitOutcome::AckCommitted(Stage8bP1RedisLimitAckCommitted {
                durable: *durable,
                transport,
                pending_m10,
            }),
        ),
        Stage8bP1d3CancelCommitOutcome::TruthCommitted(durable) => Ok(
            Stage8bP1RedisCancelCommitOutcome::TruthCommitted(Stage8bP1RedisLimitTruthCommitted {
                durable: *durable,
                transport,
                pending_m10,
            }),
        ),
        Stage8bP1d3CancelCommitOutcome::CancelContinuationPending(durable) => Ok(
            Stage8bP1RedisCancelCommitOutcome::CancelContinuationPending(
                Stage8bP1RedisCancelContinuationPending {
                    durable: *durable,
                    transport,
                    pending_m10,
                },
            ),
        ),
    }
}

/// Reattaches only the exact pending decision M10 retained by P1-d3 S_ack.
/// The returned capability can commit truth but cannot replay ACK or XACK.
pub async fn resume_stage8b_p1d3_ack_with_redis(
    durable: Stage8bP1d3AckCommittedOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1RedisLimitAckCommitted, Stage8bP1RedisSemanticError> {
    let evidence = durable.source_m10_evidence()?;
    let pending_m10 = transport.backend.reclaim_exact_evidence(&evidence).await?;
    Ok(Stage8bP1RedisLimitAckCommitted {
        durable,
        transport,
        pending_m10,
    })
}

/// Reattaches only the exact pending decision M10 retained by a P1-d3 truth
/// replacement. Source XACK is the sole exposed external mutation.
pub async fn resume_stage8b_p1d3_truth_with_redis(
    durable: Stage8bP1d3TruthCommittedOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1RedisLimitTruthCommitted, Stage8bP1RedisSemanticError> {
    let evidence = durable.source_m10_evidence()?;
    let pending_m10 = transport
        .backend
        .exact_delivery_for_binding(
            &evidence.m10_redis_id,
            &evidence.m10_semantic_id_sha256,
            &evidence.m10_payload_sha256,
            durable.operational_identity_sha256(),
        )
        .await?;
    Ok(Stage8bP1RedisLimitTruthCommitted {
        durable,
        transport,
        pending_m10,
    })
}

/// Reattaches the exact pending CANCEL source while target truth is already
/// covered by `S_terminal`, then performs only the authenticated no-input
/// recovered-cancel continuation. No successor lookup is repeated.
pub async fn resume_stage8b_p1d3_cancel_continuation_with_redis(
    durable: Stage8bP1d3CancelContinuationOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisLimitTruthCommitted, Stage8bP1RedisSemanticError> {
    let evidence = durable.source_m10_evidence()?;
    let pending_m10 = transport.backend.reclaim_exact_evidence(&evidence).await?;
    let durable = durable.commit_recovered_cancel(commitment_key)?;
    Ok(Stage8bP1RedisLimitTruthCommitted {
        durable,
        transport,
        pending_m10,
    })
}

/// Resumes only the exact M10 named by a reread P1-d3 S_eval/S_terminal.
/// Order evaluation and schedule consumption are never repeated; after the
/// exact source is validated, the sole continuation is its same-bar callback.
pub async fn resume_stage8b_p1d3_semantic_with_redis(
    durable: Stage8bP1d3SemanticPendingOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisSemanticOutcome, Stage8bP1RedisSemanticError> {
    let expected = durable.source_binding()?;
    let operational_identity_sha256 = durable.operational_identity_sha256().to_string();
    let delivery = transport
        .backend
        .reclaim_exact_binding(
            expected.redis_id(),
            expected.semantic_id_sha256(),
            expected.payload_sha256(),
            &operational_identity_sha256,
        )
        .await?;
    let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
    let accepted_bar = delivery
        .parse_exact(&operational_identity_sha256)?
        .into_stage5c_semantic_bar()?;
    complete_stage8b_p1d3_semantic(
        durable,
        transport,
        delivery,
        accepted_bar,
        binding,
        commitment_key,
    )
    .await
}

impl Stage8bP1RedisBackend {
    async fn initialize_fresh_namespace(&mut self) -> Result<(), Stage8bP1RedisSemanticError> {
        let result: redis::RedisResult<String> = redis::cmd("EVAL")
            .arg(NAMESPACE_INITIALIZATION_LUA)
            .arg(2)
            .arg(&self.namespace.canonical_m10_stream)
            .arg(&self.namespace.canonical_command_stream)
            .arg(&self.namespace.m10_consumer_group)
            .arg(&self.namespace.stage7b_command_consumer_group)
            .query_async(&mut self.connection)
            .await;
        match result {
            Ok(classification)
                if classification == "initialized" || classification == "idempotent_fresh" =>
            {
                Ok(())
            }
            Ok(_) => Err(Stage8bP1RedisSemanticError::InvalidRedisReply),
            Err(error) if error.to_string().contains("STAGE8B_P1_NAMESPACE_NOT_FRESH") => {
                Err(Stage8bP1RedisSemanticError::NamespaceNotFresh)
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn verify_groups(&mut self) -> Result<(String, String), Stage8bP1RedisSemanticError> {
        let result: redis::RedisResult<Vec<String>> = redis::cmd("EVAL")
            .arg(NAMESPACE_VERIFY_LUA)
            .arg(2)
            .arg(&self.namespace.canonical_m10_stream)
            .arg(&self.namespace.canonical_command_stream)
            .arg(&self.namespace.m10_consumer_group)
            .arg(&self.namespace.stage7b_command_consumer_group)
            .query_async(&mut self.connection)
            .await;
        let frontiers = match result {
            Ok(frontiers) => frontiers,
            Err(error) if error.to_string().contains("STAGE8B_P1_GROUP_MISSING") => {
                return Err(Stage8bP1RedisSemanticError::GroupMissing);
            }
            Err(error) => return Err(error.into()),
        };
        let [m10_frontier, command_frontier] = frontiers.as_slice() else {
            return Err(Stage8bP1RedisSemanticError::InvalidRedisReply);
        };
        parse_redis_id(m10_frontier)?;
        parse_redis_id(command_frontier)?;
        self.groups_verified = true;
        Ok((m10_frontier.clone(), command_frontier.clone()))
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

    async fn acquire_ready_delivery(
        &mut self,
    ) -> Result<Stage8bP1ReadySourceAcquisition, Stage8bP1RedisSemanticError> {
        let pending = self.pending_entries("-", "+", 2).await?;
        match pending.ids.as_slice() {
            [] => self
                .read_next_fresh()
                .await
                .map(Stage8bP1ReadySourceAcquisition::Delivery),
            [entry] => {
                let redis_id = entry.id.clone();
                match self.try_reclaim_exact_id(&redis_id).await? {
                    Some(delivery) => Ok(Stage8bP1ReadySourceAcquisition::Delivery(delivery)),
                    None => Ok(Stage8bP1ReadySourceAcquisition::PendingNotClaimable(
                        redis_id,
                    )),
                }
            }
            _ => Err(Stage8bP1RedisSemanticError::AmbiguousReadyPendingEntries),
        }
    }

    async fn read_next_fresh(
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

    async fn reclaim_exact_binding(
        &mut self,
        redis_id: &str,
        semantic_id_sha256: &str,
        payload_sha256: &str,
        expected_operational_identity_sha256: &str,
    ) -> Result<Stage8bP1PendingM10Delivery, Stage8bP1RedisSemanticError> {
        let pending = self.pending_entries("-", "+", 2).await?;
        if pending.ids.len() != 1 || pending.ids[0].id != redis_id {
            return Err(Stage8bP1RedisSemanticError::ExactPendingEntryMissing);
        }
        let delivery = self.reclaim_exact_id(redis_id).await?;
        let parsed = delivery.parse_exact(expected_operational_identity_sha256)?;
        if delivery.semantic_id_sha256() != semantic_id_sha256
            || delivery.payload_sha256() != payload_sha256
            || parsed.redis_id() != redis_id
        {
            return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
        }
        Ok(delivery)
    }

    async fn exact_delivery_for_binding(
        &mut self,
        redis_id: &str,
        semantic_id_sha256: &str,
        payload_sha256: &str,
        expected_operational_identity_sha256: &str,
    ) -> Result<Stage8bP1PendingM10Delivery, Stage8bP1RedisSemanticError> {
        let payload = self
            .exact_stream_entry(redis_id)
            .await?
            .ok_or(Stage8bP1RedisSemanticError::ExactPendingEntryMissing)?;
        let validated = parse_stage8b_p1_canonical_m10(
            payload.as_bytes(),
            expected_operational_identity_sha256,
        )?;
        if validated.redis_id() != redis_id
            || validated.semantic_id_sha256() != semantic_id_sha256
            || validated.payload_sha256() != payload_sha256
        {
            return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
        }
        Ok(Stage8bP1PendingM10Delivery {
            redis_id: redis_id.to_string(),
            semantic_id_sha256: semantic_id_sha256.to_string(),
            payload_sha256: payload_sha256.to_string(),
            canonical_bytes: payload.into_bytes(),
        })
    }

    async fn reclaim_exact_id(
        &mut self,
        expected_id: &str,
    ) -> Result<Stage8bP1PendingM10Delivery, Stage8bP1RedisSemanticError> {
        self.try_reclaim_exact_id(expected_id)
            .await?
            .ok_or(Stage8bP1RedisSemanticError::ExactPendingEntryMissing)
    }

    async fn try_reclaim_exact_id(
        &mut self,
        expected_id: &str,
    ) -> Result<Option<Stage8bP1PendingM10Delivery>, Stage8bP1RedisSemanticError> {
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
                    return delivery_from_entry(entry).map(Some);
                }
            }
            if self.claim_cursor == "0-0" || self.claim_cursor == start {
                self.claim_cursor = "0-0".to_string();
                break;
            }
        }
        Ok(None)
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
        let (m10_group_frontier, _) = self.verify_groups().await?;
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
            0 if redis_id_at_least(&m10_group_frontier, delivery.redis_id())? => {
                Ok(Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged)
            }
            0 => Err(Stage8bP1RedisSemanticError::ExactSourceConflict),
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
                    let (after_m10_group_frontier, _) = self.verify_groups().await?;
                    if after.ids.is_empty()
                        && redis_id_at_least(&after_m10_group_frontier, delivery.redis_id())?
                    {
                        Ok(Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged)
                    } else {
                        Err(Stage8bP1RedisSemanticError::ExactSourceConflict)
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
            publication_reservation_sha256: None,
            publication_binding_sha256: None,
            disposition,
            m10_acknowledged: false,
            paper_provider_invoked: false,
            finam_transport_attached: false,
            broker_network_dispatch_attached: false,
            runtime_live: false,
            real_orders: false,
        })
    }

    async fn prepare_p1d4_command_publication(
        &mut self,
        durable: &Stage8bP1SemanticPrepublicationOwner,
        delivery: &Stage8bP1PendingM10Delivery,
    ) -> Result<Stage8bP1d4PreparedCommandPublication, Stage8bP1RedisSemanticError> {
        let evidence = durable.evidence();
        let request_id = evidence
            .strategy_request_id
            .ok_or(Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let command_bytes = serde_json::to_vec(durable.command())
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let command_sha256 = sha256_hex(&command_bytes);
        if !durable.stage8b_p1d4_generated_market_candidate()
            || evidence.intent_count != 1
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
        let envelope_sha256 = sha256_hex(&envelope_bytes);
        let predecessor = self.command_stream_last_generated_id().await?;
        let reserved = strategy_runtime_core::stage8b_p1d4_immediate_redis_successor(&predecessor)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let prepublication_package_generation = durable
            .stage8b_p1d4_next_write_generation()
            .ok_or(Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let reservation = Stage8bP1d4CommandPublicationReservationV1::new(
            self.namespace.canonical_m10_stream.clone(),
            self.namespace.m10_consumer_group.clone(),
            delivery.redis_id().to_string(),
            evidence.semantic_batch_id_sha256.clone(),
            request_id,
            command_sha256,
            envelope_sha256,
            self.namespace.canonical_command_stream.clone(),
            self.namespace.stage7b_command_consumer_group.clone(),
            predecessor,
            reserved,
            prepublication_package_generation,
        )
        .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        Ok(Stage8bP1d4PreparedCommandPublication {
            reservation,
            envelope_bytes,
        })
    }

    async fn publish_reserved_p1d4_command(
        &mut self,
        durable: &Stage8bP1d4GeneratedMarketPrepublicationOwner,
        delivery: &Stage8bP1PendingM10Delivery,
        prepared: &Stage8bP1d4PreparedCommandPublication,
    ) -> Result<Stage8bP1RedisCommandPublicationReceipt, Stage8bP1RedisSemanticError> {
        let evidence = durable.evidence();
        let reservation = durable.reservation();
        let binding = durable.binding();
        let request_id = evidence
            .strategy_request_id
            .ok_or(Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let command_sha256 = sha256_hex(
            &serde_json::to_vec(durable.command())
                .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?,
        );
        let envelope_sha256 = sha256_hex(&prepared.envelope_bytes);
        if reservation != &prepared.reservation
            || reservation.strategy_request_id() != request_id
            || reservation.canonical_command_sha256() != command_sha256
            || reservation.canonical_envelope_sha256() != envelope_sha256
            || reservation.source_m10_redis_id() != delivery.redis_id()
            || reservation.source_stream() != self.namespace.canonical_m10_stream
            || reservation.source_group() != self.namespace.m10_consumer_group
            || reservation.command_stream() != self.namespace.canonical_command_stream
            || reservation.command_group() != self.namespace.stage7b_command_consumer_group
            || binding.validate_against(reservation).is_err()
        {
            return Err(Stage8bP1RedisSemanticError::CommandPublicationConflict);
        }
        let marker = Stage8bP1d4RedisPublicationMarkerV1 {
            schema_version: P1D4_COMMAND_PUBLICATION_MARKER_SCHEMA_VERSION,
            domain: P1D4_COMMAND_PUBLICATION_MARKER_DOMAIN,
            source_stream: reservation.source_stream(),
            source_group: reservation.source_group(),
            source_m10_redis_id: reservation.source_m10_redis_id(),
            semantic_batch_id_sha256: reservation.semantic_batch_id_sha256(),
            strategy_request_id: reservation.strategy_request_id().to_string(),
            canonical_command_sha256: reservation.canonical_command_sha256(),
            canonical_envelope_sha256: reservation.canonical_envelope_sha256(),
            command_stream: reservation.command_stream(),
            command_group: reservation.command_group(),
            command_stream_predecessor_id: reservation.command_stream_predecessor_id(),
            command_entry_id: reservation.reserved_command_entry_id(),
            prepublication_package_generation: reservation
                .prepublication_package_generation()
                .to_string(),
            publication_reservation_sha256: reservation.publication_reservation_sha256(),
            prepublication_seal_generation: binding.prepublication_seal_generation().to_string(),
            prepublication_seal_commitment_sha256: binding.prepublication_seal_commitment_sha256(),
            publication_binding_sha256: binding.publication_binding_sha256(),
        };
        let marker_payload = serde_json::to_string(&marker)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let envelope_payload = std::str::from_utf8(&prepared.envelope_bytes)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let source_payload = std::str::from_utf8(&delivery.canonical_bytes)
            .map_err(|_| Stage8bP1RedisSemanticError::ExactSourceConflict)?;
        let marker_key = publication_marker_key(&self.namespace, request_id);
        let result: Vec<String> = redis::cmd("EVAL")
            .arg(P1D4_COMMAND_PUBLICATION_LUA)
            .arg(3)
            .arg(&self.namespace.canonical_m10_stream)
            .arg(&self.namespace.canonical_command_stream)
            .arg(marker_key)
            .arg(&self.namespace.m10_consumer_group)
            .arg(delivery.redis_id())
            .arg(source_payload)
            .arg(&self.namespace.stage7b_command_consumer_group)
            .arg(reservation.command_stream_predecessor_id())
            .arg(reservation.reserved_command_entry_id())
            .arg(envelope_payload)
            .arg(&marker_payload)
            .query_async(&mut self.connection)
            .await?;
        let [classification, command_entry_id] = result.as_slice() else {
            return Err(Stage8bP1RedisSemanticError::InvalidRedisReply);
        };
        if command_entry_id != reservation.reserved_command_entry_id() {
            return Err(Stage8bP1RedisSemanticError::CommandPublicationConflict);
        }
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
            covering_seal_generation: binding.prepublication_seal_generation(),
            covering_seal_commitment_sha256: binding
                .prepublication_seal_commitment_sha256()
                .to_string(),
            publication_reservation_sha256: Some(
                reservation.publication_reservation_sha256().to_string(),
            ),
            publication_binding_sha256: Some(binding.publication_binding_sha256().to_string()),
            disposition,
            m10_acknowledged: false,
            paper_provider_invoked: false,
            finam_transport_attached: false,
            broker_network_dispatch_attached: false,
            runtime_live: false,
            real_orders: false,
        })
    }

    async fn revalidate_p1d4_publication(
        &mut self,
        evidence: &Stage6Stage8bP1SemanticCommitEvidenceV1,
        command: &BrokerCommand,
        delivery: &Stage8bP1PendingM10Delivery,
        reservation: &Stage8bP1d4CommandPublicationReservationV1,
        binding: &Stage8bP1d4CommandPublicationBindingV1,
        source_must_be_pending: bool,
    ) -> Result<(), Stage8bP1RedisSemanticError> {
        let request_id = evidence
            .strategy_request_id
            .ok_or(Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let command_bytes = serde_json::to_vec(command)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let command_sha256 = sha256_hex(&command_bytes);
        let envelope = Envelope {
            schema_version: SCHEMA_VERSION,
            ts_utc: command_created_at(command),
            source: COMMAND_ENVELOPE_SOURCE.to_string(),
            msg_type: MessageType::Command,
            payload: command.clone(),
        };
        let envelope_bytes = serde_json::to_vec(&envelope)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        runtime_command_bridge::decode_stage7a_pre_admission(&envelope_bytes)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        if reservation.validate().is_err()
            || binding.validate_against(reservation).is_err()
            || reservation.strategy_request_id() != request_id
            || reservation.canonical_command_sha256() != command_sha256
            || reservation.canonical_envelope_sha256() != sha256_hex(&envelope_bytes)
            || reservation.source_m10_redis_id() != delivery.redis_id()
            || reservation.source_stream() != self.namespace.canonical_m10_stream
            || reservation.source_group() != self.namespace.m10_consumer_group
            || reservation.command_stream() != self.namespace.canonical_command_stream
            || reservation.command_group() != self.namespace.stage7b_command_consumer_group
            || evidence.semantic_batch_id_sha256 != reservation.semantic_batch_id_sha256()
            || evidence.m10_semantic_id_sha256 != delivery.semantic_id_sha256()
            || evidence.m10_payload_sha256 != delivery.payload_sha256()
        {
            return Err(Stage8bP1RedisSemanticError::CommandPublicationConflict);
        }
        let marker = Stage8bP1d4RedisPublicationMarkerV1 {
            schema_version: P1D4_COMMAND_PUBLICATION_MARKER_SCHEMA_VERSION,
            domain: P1D4_COMMAND_PUBLICATION_MARKER_DOMAIN,
            source_stream: reservation.source_stream(),
            source_group: reservation.source_group(),
            source_m10_redis_id: reservation.source_m10_redis_id(),
            semantic_batch_id_sha256: reservation.semantic_batch_id_sha256(),
            strategy_request_id: request_id.to_string(),
            canonical_command_sha256: reservation.canonical_command_sha256(),
            canonical_envelope_sha256: reservation.canonical_envelope_sha256(),
            command_stream: reservation.command_stream(),
            command_group: reservation.command_group(),
            command_stream_predecessor_id: reservation.command_stream_predecessor_id(),
            command_entry_id: reservation.reserved_command_entry_id(),
            prepublication_package_generation: reservation
                .prepublication_package_generation()
                .to_string(),
            publication_reservation_sha256: reservation.publication_reservation_sha256(),
            prepublication_seal_generation: binding.prepublication_seal_generation().to_string(),
            prepublication_seal_commitment_sha256: binding.prepublication_seal_commitment_sha256(),
            publication_binding_sha256: binding.publication_binding_sha256(),
        };
        let marker_payload = serde_json::to_string(&marker)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let envelope_payload = std::str::from_utf8(&envelope_bytes)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let source_payload = std::str::from_utf8(&delivery.canonical_bytes)
            .map_err(|_| Stage8bP1RedisSemanticError::ExactSourceConflict)?;
        let marker_key = publication_marker_key(&self.namespace, request_id);
        let result: Vec<String> = redis::cmd("EVAL")
            .arg(P1D4_COMMAND_PUBLICATION_REVALIDATE_LUA)
            .arg(3)
            .arg(&self.namespace.canonical_m10_stream)
            .arg(&self.namespace.canonical_command_stream)
            .arg(marker_key)
            .arg(&self.namespace.m10_consumer_group)
            .arg(delivery.redis_id())
            .arg(source_payload)
            .arg(&self.namespace.stage7b_command_consumer_group)
            .arg(reservation.reserved_command_entry_id())
            .arg(envelope_payload)
            .arg(marker_payload)
            .arg(if source_must_be_pending {
                "required"
            } else {
                "optional"
            })
            .query_async(&mut self.connection)
            .await?;
        match result.as_slice() {
            [classification, command_entry_id]
                if classification == "existing"
                    && command_entry_id == reservation.reserved_command_entry_id() =>
            {
                Ok(())
            }
            _ => Err(Stage8bP1RedisSemanticError::CommandPublicationConflict),
        }
    }

    async fn command_stream_last_generated_id(
        &mut self,
    ) -> Result<String, Stage8bP1RedisSemanticError> {
        let values: Vec<redis::Value> = redis::cmd("XINFO")
            .arg("STREAM")
            .arg(&self.namespace.canonical_command_stream)
            .query_async(&mut self.connection)
            .await?;
        if values.len() % 2 != 0 {
            return Err(Stage8bP1RedisSemanticError::InvalidRedisReply);
        }
        for pair in values.chunks_exact(2) {
            let key = String::from_redis_value(&pair[0])
                .map_err(|_| Stage8bP1RedisSemanticError::InvalidRedisReply)?;
            if key == "last-generated-id" {
                let value = String::from_redis_value(&pair[1])
                    .map_err(|_| Stage8bP1RedisSemanticError::InvalidRedisReply)?;
                strategy_runtime_core::stage8b_p1d4_immediate_redis_successor(&value)
                    .map_err(|_| Stage8bP1RedisSemanticError::InvalidRedisReply)?;
                return Ok(value);
            }
        }
        Err(Stage8bP1RedisSemanticError::InvalidRedisReply)
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

    async fn exact_first_successor_m10(
        &mut self,
        predecessor_redis_id: &str,
        expected_operational_identity_sha256: &str,
    ) -> Result<super::Stage8bP1ValidatedCanonicalM10, Stage8bP1RedisSemanticError> {
        let (predecessor_ms, predecessor_sequence) = parse_redis_id(predecessor_redis_id)?;
        if predecessor_sequence != 0 {
            return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
        }
        let expected_close_ms = predecessor_ms
            .checked_add(600_000)
            .ok_or(Stage8bP1RedisSemanticError::ExactSourceConflict)?;
        let expected_id = format!("{expected_close_ms}-0");
        let reply: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&self.namespace.canonical_m10_stream)
            .arg(format!("({predecessor_redis_id}"))
            .arg("+")
            .arg("COUNT")
            .arg(2)
            .query_async(&mut self.connection)
            .await?;
        let Some(first) = reply.ids.first() else {
            return Err(Stage8bP1RedisSemanticError::ExactPendingEntryMissing);
        };
        if first.id != expected_id || first.map.len() != 1 {
            return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
        }
        let payload = first
            .get::<String>("payload")
            .ok_or(Stage8bP1RedisSemanticError::InvalidRedisReply)?;
        let validated = parse_stage8b_p1_canonical_m10(
            payload.as_bytes(),
            expected_operational_identity_sha256,
        )?;
        if validated.redis_id() != expected_id {
            return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
        }
        Ok(validated)
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

fn prepare_recovered_p1d4_publication(
    durable: &Stage8bP1d4GeneratedMarketPrepublicationOwner,
) -> Result<Stage8bP1d4PreparedCommandPublication, Stage8bP1RedisSemanticError> {
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
    if sha256_hex(&envelope_bytes) != durable.reservation().canonical_envelope_sha256() {
        return Err(Stage8bP1RedisSemanticError::CommandPublicationConflict);
    }
    Ok(Stage8bP1d4PreparedCommandPublication {
        reservation: durable.reservation().clone(),
        envelope_bytes,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn parse_redis_id(redis_id: &str) -> Result<(u64, u64), Stage8bP1RedisSemanticError> {
    let (milliseconds, sequence) = redis_id
        .split_once('-')
        .ok_or(Stage8bP1RedisSemanticError::InvalidRedisReply)?;
    let milliseconds = milliseconds
        .parse()
        .map_err(|_| Stage8bP1RedisSemanticError::InvalidRedisReply)?;
    let sequence = sequence
        .parse()
        .map_err(|_| Stage8bP1RedisSemanticError::InvalidRedisReply)?;
    Ok((milliseconds, sequence))
}

fn redis_id_at_least(candidate: &str, expected: &str) -> Result<bool, Stage8bP1RedisSemanticError> {
    Ok(parse_redis_id(candidate)? >= parse_redis_id(expected)?)
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
    use broker_core::{
        BrokerAccountId, ClientOrderId, HybridRuntimeAttribution, OrderSide, OrderType, PlaceOrder,
        TimeInForce,
    };
    use chrono::{TimeZone, Utc};
    use redis::streams::StreamPendingReply;
    use rust_decimal::Decimal;
    use std::{
        fs,
        net::TcpListener,
        os::unix::{fs::DirBuilderExt, process::ExitStatusExt},
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    use strategy_runtime_core::{
        stage8b_p1d3_test_expiry_authority, stage8b_p1d3_test_materialize_host_cancel_command,
        stage8b_p1d3_test_step_authority, Stage5gP1SemanticBindingInput,
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

    fn p1d2_test_schedule_authority() -> strategy_runtime_core::Stage8bP1d1ExecutionScheduleAuthority
    {
        let predecessor_close_ts_utc_ms = 1_785_759_000_000_i64;
        strategy_runtime_core::stage8b_p1d1_test_schedule_authority(
            super::super::p1_instrument(),
            predecessor_close_ts_utc_ms,
            predecessor_close_ts_utc_ms + 600_000,
        )
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
        one_intent_pending_at(&redis.url, parent).await
    }

    async fn one_intent_pending_at(
        redis_url: &str,
        parent: &Path,
    ) -> (
        Stage8bP1RedisPrepublicationPending,
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
    ) {
        one_intent_pending_at_with_entry_side(redis_url, parent, false).await
    }

    async fn one_intent_pending_at_with_entry_side(
        redis_url: &str,
        parent: &Path,
        short_entry: bool,
    ) -> (
        Stage8bP1RedisPrepublicationPending,
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
    ) {
        let (owner, key, fresh, identity) = first_boot(parent);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            redis_url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let bytes = canonical_m10(
            identity.clone(),
            1_785_759_000_000,
            if short_entry { 2_550 } else { 2_650 },
        );
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

    const P1D3_PLACE_DECISION_CLOSE_MS: i64 = 1_785_760_200_000;
    const P1D3_CANCEL_DECISION_CLOSE_MS: i64 = 1_785_760_800_000;
    const P1D3_CANCEL_CANDIDATE_CLOSE_MS: i64 = 1_785_761_400_000;

    fn p1d3_test_binding(
        identity: &str,
        close_ts_utc_ms: i64,
        price: i64,
    ) -> Stage5gP1SemanticBindingInput {
        let bytes = canonical_m10(identity.to_string(), close_ts_utc_ms, price);
        let parsed = parse_stage8b_p1_canonical_m10(&bytes, identity).unwrap();
        Stage5gP1SemanticBindingInput {
            operational_identity_sha256: identity.to_string(),
            m10_redis_id: parsed.redis_id().to_string(),
            m10_semantic_id_sha256: parsed.semantic_id_sha256().to_string(),
            m10_payload_sha256: parsed.payload_sha256().to_string(),
        }
    }

    fn p1d3_cancel_schedule(
        predecessor_close_ts_utc_ms: i64,
        candidate_close_ts_utc_ms: i64,
    ) -> Stage8bP1d3ScheduleStepAuthority {
        let trading_day = Utc
            .timestamp_millis_opt(candidate_close_ts_utc_ms)
            .single()
            .unwrap()
            .date_naive()
            .to_string();
        stage8b_p1d3_test_step_authority(
            "44".repeat(32),
            trading_day,
            format!("{candidate_close_ts_utc_ms}-0"),
            format!("{predecessor_close_ts_utc_ms}-0"),
            format!("{candidate_close_ts_utc_ms}-0"),
        )
    }

    fn p1d3_place_command(
        attribution: HybridRuntimeAttribution,
        request_id: StrategyRequestId,
    ) -> (BrokerCommand, HybridRuntimeAttribution, ClientOrderId) {
        let comment = attribution.internal_comment().to_string();
        let client_order_id = ClientOrderId::from_strategy_request(request_id);
        (
            BrokerCommand::PlaceOrder(PlaceOrder {
                request_id,
                created_ts: Utc
                    .timestamp_millis_opt(P1D3_PLACE_DECISION_CLOSE_MS)
                    .single()
                    .unwrap(),
                ttl_ms: None,
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                client_order_id: client_order_id.clone(),
                instrument: super::super::p1_instrument(),
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                qty: Decimal::ONE,
                limit_price: Some(Decimal::new(2_210, 0)),
                time_in_force: TimeInForce::Day,
                comment: Some(comment),
            }),
            attribution,
            client_order_id,
        )
    }

    fn p1d3_cancel_command(
        strategy: &strategy_runtime_core::HybridIntradayRuntimeStrategy,
        target: broker_core::BrokerOrderId,
        source_attribution: &HybridRuntimeAttribution,
        request_id: StrategyRequestId,
        supplied_target_client_order_id: Option<ClientOrderId>,
    ) -> (BrokerCommand, HybridRuntimeAttribution) {
        let (prefix, _) = source_attribution
            .internal_comment()
            .rsplit_once("|r=")
            .unwrap();
        let comment = format!("{prefix}|r=CANCEL");
        let cancel_attribution =
            HybridRuntimeAttribution::parse_source_comment(comment.clone()).unwrap();
        let command = stage8b_p1d3_test_materialize_host_cancel_command(
            strategy,
            request_id,
            P1D3_CANCEL_DECISION_CLOSE_MS / 1_000,
            BrokerAccountId::new("ACC_TEST_0001"),
            super::super::p1_instrument(),
            target,
            cancel_attribution.clone(),
        )
        .unwrap();
        let BrokerCommand::CancelOrder(mut cancel) = command else {
            unreachable!("Stage 5C host cancel materializer returned a non-CANCEL command")
        };
        assert!(
            cancel.client_order_id.is_none(),
            "actual Stage 5C host CANCEL contract must keep TCID optional"
        );
        cancel.client_order_id = supplied_target_client_order_id;
        (BrokerCommand::CancelOrder(cancel), cancel_attribution)
    }

    async fn prepare_p1d3_initial_limit_source(
        redis_url: &str,
        parent: &Path,
        candidate_price: i64,
    ) -> (
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        Stage8bP1RedisCommandPublished,
    ) {
        prepare_p1d3_initial_limit_source_with_options(
            redis_url,
            parent,
            candidate_price,
            false,
            false,
            |_| {},
        )
        .await
        .expect("canonical P1-d3 LIMIT fixture must be constructible")
    }

    async fn prepare_p1d3_initial_filled_source(
        redis_url: &str,
        parent: &Path,
    ) -> (
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        Stage8bP1RedisCommandPublished,
    ) {
        prepare_p1d3_initial_limit_source_with_options(
            redis_url,
            parent,
            2_230,
            true,
            false,
            |_| {},
        )
        .await
        .expect("canonical filled P1-d3 LIMIT fixture must be constructible")
    }

    async fn prepare_p1d4_later_working_owner(
        redis_url: &str,
        parent: &Path,
        filled_exit: bool,
        initial_short: bool,
        working_limit_price: Option<i64>,
    ) -> (
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        Stage8bP1RedisSemanticCompositionOwner,
    ) {
        let (key, fresh, identity, published) = prepare_p1d3_initial_limit_source_with_options(
            redis_url,
            parent,
            2_220,
            filled_exit,
            initial_short,
            |place| {
                if let Some(price) = working_limit_price {
                    place.limit_price = Some(Decimal::new(price, 0));
                }
            },
        )
        .await
        .expect("P1-d4 later Working fixture must be constructible");
        let resolved = published
            .execute_next_canonical_limit(
                p1d3_cancel_schedule(P1D3_PLACE_DECISION_CLOSE_MS, P1D3_CANCEL_DECISION_CLOSE_MS),
                &key,
            )
            .await
            .unwrap()
            .commit_truth(&key)
            .unwrap()
            .acknowledge_source()
            .await
            .unwrap();
        let Stage8bP1RedisSemanticCompositionOwner { stage7, transport } =
            *resolved.into_ready_owner();
        drop(transport);

        let mut connection = ConnectionManager::new(redis::Client::open(redis_url).unwrap())
            .await
            .unwrap();
        let _: String = redis::cmd("FLUSHALL")
            .query_async(&mut connection)
            .await
            .unwrap();
        drop(connection);
        let transport = initialize_stage8b_p1_redis_namespace(
            redis_url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let mut owner = Stage8bP1RedisSemanticCompositionOwner::new(stage7, transport);
        owner
            .transport
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), P1D3_CANCEL_CANDIDATE_CLOSE_MS, 2_220),
                &identity,
            )
            .await
            .unwrap();

        // Cover the just-resolved initial command with one ordinary untouched
        // later-bar replacement before constructing an F11+ crash fixture.
        // Without this production transition the durable package correctly
        // remains at the prior command's S_truth (XACK itself is external),
        // which is not the registry's Ready precondition for later-bar cells.
        let normalized = owner
            .process_next_working_limit(
                p1d3_cancel_schedule(
                    P1D3_CANCEL_DECISION_CLOSE_MS,
                    P1D3_CANCEL_CANDIDATE_CLOSE_MS,
                ),
                &key,
            )
            .await
            .unwrap();
        let Stage8bP1RedisSemanticOutcome::Ready {
            owner: normalized, ..
        } = normalized
        else {
            panic!("P1-d4 later fixture normalization must remain zero-intent Ready")
        };
        owner = *normalized;
        (key, fresh, identity, owner)
    }

    async fn prepare_p1d4_later_filled_zero_owner(
        redis_url: &str,
        parent: &Path,
    ) -> (
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        Stage8bP1RedisSemanticCompositionOwner,
    ) {
        let (key, fresh, identity, mut owner) =
            prepare_p1d4_later_working_owner(redis_url, parent, true, false, None).await;
        let mut predecessor_close_ms = P1D3_CANCEL_CANDIDATE_CLOSE_MS;
        for candidate_close_ms in [
            P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000,
            P1D3_CANCEL_CANDIDATE_CLOSE_MS + 1_200_000,
        ] {
            owner
                .transport
                .publish_canonical_m10(
                    &canonical_m10(identity.clone(), candidate_close_ms, 2_220),
                    &identity,
                )
                .await
                .unwrap();
            let outcome = owner
                .process_next_working_limit(
                    p1d3_cancel_schedule(predecessor_close_ms, candidate_close_ms),
                    &key,
                )
                .await
                .unwrap();
            let Stage8bP1RedisSemanticOutcome::Ready {
                owner: next_owner, ..
            } = outcome
            else {
                panic!("P1-d4 S06 setup bar must be untouched with zero callback intents")
            };
            owner = *next_owner;
            predecessor_close_ms = candidate_close_ms;
        }
        (key, fresh, identity, owner)
    }

    async fn prepare_p1d4_generated_market_prepublication(
        redis_url: &str,
        parent: &Path,
    ) -> (
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        Stage8bP1RedisPrepublicationPending,
        i64,
    ) {
        let (key, fresh, identity, mut owner) =
            prepare_p1d4_later_working_owner(redis_url, parent, false, true, Some(2_000)).await;
        let decision_close_ms = P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000;
        let fill_close_ms = decision_close_ms + 600_000;
        owner
            .transport
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), decision_close_ms, 2_650),
                &identity,
            )
            .await
            .unwrap();
        owner
            .transport
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), fill_close_ms, 2_175),
                &identity,
            )
            .await
            .unwrap();
        let outcome = owner
            .process_next_working_limit(
                p1d3_cancel_schedule(P1D3_CANCEL_CANDIDATE_CLOSE_MS, decision_close_ms),
                &key,
            )
            .await
            .unwrap();
        let Stage8bP1RedisSemanticOutcome::Prepublication(pending) = outcome else {
            panic!("P1-d4 S05 fixture must generate one Market exit")
        };
        (key, fresh, identity, *pending, decision_close_ms)
    }

    #[tokio::test]
    async fn p1d4_generated_market_write_and_seal_generations_are_independent() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1d4-independent-write-and-seal-generations");
        let (key, _, _, mut pending, decision_close_ms) =
            prepare_p1d4_generated_market_prepublication(&redis.url, &parent).await;

        let write_generation_w0 = pending
            .durable
            .stage8b_p1d4_next_write_generation()
            .expect("generated-Market W0");
        pending.durable = pending
            .durable
            .stage8b_p1d4_test_advance_recovery_seal_to(99, &key)
            .expect("advance only recovery-seal generation to G0 predecessor");
        assert_eq!(pending.durable.recovery_seal_generation(), 99);
        assert_eq!(
            pending.durable.stage8b_p1d4_next_write_generation(),
            Some(write_generation_w0),
            "advancing G must not advance W"
        );

        let published = pending
            .publish_exact_generated_market_command(&key)
            .await
            .unwrap();
        let reservation = published.p1d4_reservation.as_ref().unwrap();
        let binding = published.p1d4_binding.as_ref().unwrap();
        assert_eq!(
            reservation.prepublication_package_generation(),
            write_generation_w0
        );
        assert_eq!(
            binding.prepublication_package_generation(),
            write_generation_w0
        );
        assert_eq!(binding.prepublication_seal_generation(), 100);
        assert_ne!(
            binding.prepublication_seal_generation(),
            write_generation_w0
        );
        assert_ne!(
            binding.prepublication_seal_generation(),
            write_generation_w0 + 1,
            "the accepted golden-fixture offset is not a production invariant"
        );

        let ack = published
            .execute_next_canonical_generated_market(
                strategy_runtime_core::stage8b_p1d1_test_schedule_authority(
                    super::super::p1_instrument(),
                    decision_close_ms,
                    decision_close_ms + 600_000,
                ),
                &key,
            )
            .await
            .unwrap();
        assert_eq!(
            ack.durable.stage8b_p1d4_test_package_write_generation(),
            write_generation_w0 + 1
        );
        assert_eq!(ack.recovery_seal_generation(), 101);

        let truth = ack.commit_truth(&key).await.unwrap();
        assert_eq!(
            truth.durable.stage8b_p1d4_test_package_write_generation(),
            write_generation_w0 + 2
        );
        assert_eq!(truth.recovery_seal_generation(), 102);
        let resolved = truth.acknowledge_source().await.unwrap();
        drop(resolved);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1d4_publication_marker_preserves_high_u64_generation_exactly() {
        let redis = RedisServer::start().await;
        let mut connection = redis.connection().await;
        let source_stream = "p1d4:high-u64:source";
        let source_group = "p1d4-high-u64-source-group";
        let source_id = "10-1";
        let source_payload = "canonical-source-payload";
        let command_stream = "p1d4:high-u64:commands";
        let command_group = "p1d4-high-u64-command-group";
        let command_predecessor_id = "0-0";
        let command_entry_id = "0-1";
        let command_payload = "canonical-command-envelope";
        let marker_key = "p1d4:high-u64:publication-marker";
        let request_id =
            StrategyRequestId::from(Uuid::from_u128(0xd404_0000_0000_4000_8000_0000_0000_0001));
        let generation = 9_007_199_254_740_993_u64;

        let inserted_source: String = redis::cmd("XADD")
            .arg(source_stream)
            .arg(source_id)
            .arg("payload")
            .arg(source_payload)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(inserted_source, source_id);
        let _: () = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(source_stream)
            .arg(source_group)
            .arg("0-0")
            .query_async(&mut connection)
            .await
            .unwrap();
        let claimed: redis::streams::StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(source_group)
            .arg("high-u64-consumer")
            .arg("COUNT")
            .arg(1)
            .arg("STREAMS")
            .arg(source_stream)
            .arg(">")
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(claimed.keys.len(), 1);
        let _: () = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(command_stream)
            .arg(command_group)
            .arg("0-0")
            .arg("MKSTREAM")
            .query_async(&mut connection)
            .await
            .unwrap();

        let reservation = Stage8bP1d4CommandPublicationReservationV1::new(
            source_stream.to_string(),
            source_group.to_string(),
            source_id.to_string(),
            "11".repeat(32),
            request_id,
            "22".repeat(32),
            "33".repeat(32),
            command_stream.to_string(),
            command_group.to_string(),
            command_predecessor_id.to_string(),
            command_entry_id.to_string(),
            41,
        )
        .unwrap();
        let binding = Stage8bP1d4CommandPublicationBindingV1::from_reservation(
            &reservation,
            generation,
            "44".repeat(32),
        )
        .unwrap();
        let marker_payload = serde_json::to_string(&Stage8bP1d4RedisPublicationMarkerV1 {
            schema_version: P1D4_COMMAND_PUBLICATION_MARKER_SCHEMA_VERSION,
            domain: P1D4_COMMAND_PUBLICATION_MARKER_DOMAIN,
            source_stream: reservation.source_stream(),
            source_group: reservation.source_group(),
            source_m10_redis_id: reservation.source_m10_redis_id(),
            semantic_batch_id_sha256: reservation.semantic_batch_id_sha256(),
            strategy_request_id: request_id.to_string(),
            canonical_command_sha256: reservation.canonical_command_sha256(),
            canonical_envelope_sha256: reservation.canonical_envelope_sha256(),
            command_stream: reservation.command_stream(),
            command_group: reservation.command_group(),
            command_stream_predecessor_id: reservation.command_stream_predecessor_id(),
            command_entry_id: reservation.reserved_command_entry_id(),
            prepublication_package_generation: reservation
                .prepublication_package_generation()
                .to_string(),
            publication_reservation_sha256: reservation.publication_reservation_sha256(),
            prepublication_seal_generation: binding.prepublication_seal_generation().to_string(),
            prepublication_seal_commitment_sha256: binding.prepublication_seal_commitment_sha256(),
            publication_binding_sha256: binding.publication_binding_sha256(),
        })
        .unwrap();
        let marker_json: serde_json::Value = serde_json::from_str(&marker_payload).unwrap();
        assert_eq!(
            marker_json["prepublication_seal_generation"].as_str(),
            Some("9007199254740993")
        );
        assert_eq!(
            marker_json["prepublication_package_generation"].as_str(),
            Some("41")
        );

        macro_rules! publish {
            ($marker_payload:expr) => {
                redis::cmd("EVAL")
                    .arg(P1D4_COMMAND_PUBLICATION_LUA)
                    .arg(3)
                    .arg(source_stream)
                    .arg(command_stream)
                    .arg(marker_key)
                    .arg(source_group)
                    .arg(source_id)
                    .arg(source_payload)
                    .arg(command_group)
                    .arg(command_predecessor_id)
                    .arg(command_entry_id)
                    .arg(command_payload)
                    .arg($marker_payload)
                    .query_async::<Vec<String>>(&mut connection)
                    .await
            };
        }
        let first = publish!(&marker_payload).unwrap();
        assert_eq!(first, vec!["published", command_entry_id]);
        let recovered = publish!(&marker_payload).unwrap();
        assert_eq!(recovered, vec!["existing", command_entry_id]);

        let neighboring_binding = Stage8bP1d4CommandPublicationBindingV1::from_reservation(
            &reservation,
            generation + 1,
            "44".repeat(32),
        )
        .unwrap();
        let neighboring_marker = marker_payload
            .replace("9007199254740993", "9007199254740994")
            .replace(
                binding.publication_binding_sha256(),
                neighboring_binding.publication_binding_sha256(),
            );
        let conflict = publish!(&neighboring_marker);
        assert!(conflict.is_err());

        let stored_marker: String = redis::cmd("GET")
            .arg(marker_key)
            .query_async(&mut connection)
            .await
            .unwrap();
        let command_count: usize = redis::cmd("XLEN")
            .arg(command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(source_stream)
            .arg(source_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(stored_marker, marker_payload);
        assert_eq!(command_count, 1);
        assert_eq!(pending.count(), 1);
    }

    async fn prepare_p1d3_initial_limit_source_with_mutation(
        redis_url: &str,
        parent: &Path,
        candidate_price: i64,
        mutate: impl FnOnce(&mut PlaceOrder),
    ) -> Option<(
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        Stage8bP1RedisCommandPublished,
    )> {
        prepare_p1d3_initial_limit_source_with_options(
            redis_url,
            parent,
            candidate_price,
            false,
            false,
            mutate,
        )
        .await
    }

    async fn prepare_p1d3_initial_limit_source_with_options(
        redis_url: &str,
        parent: &Path,
        candidate_price: i64,
        filled_exit: bool,
        initial_short: bool,
        mutate: impl FnOnce(&mut PlaceOrder),
    ) -> Option<(
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        Stage8bP1RedisCommandPublished,
    )> {
        let (mut pending, key, fresh, identity) =
            one_intent_pending_at_with_entry_side(redis_url, parent, initial_short).await;
        pending
            .transport
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), 1_785_759_600_000, 2_175),
                &identity,
            )
            .await
            .unwrap();
        let resolved = pending
            .publish_exact_command()
            .await
            .unwrap()
            .execute_next_canonical_market(p1d2_test_schedule_authority(), &key)
            .await
            .unwrap()
            .commit_truth(&key)
            .unwrap()
            .acknowledge_source()
            .await
            .unwrap();
        let Stage8bP1RedisSemanticCompositionOwner { stage7, transport } =
            *resolved.into_ready_owner();
        drop(transport);
        let stage7 = if initial_short {
            stage7
                .stage8b_p1d4_test_migrate_position_synced_p1d2(&key)
                .unwrap()
        } else {
            stage7
                .migrate_stage8b_p1d3_from_resolved_p1d2(&key)
                .unwrap()
        };

        let mut connection = ConnectionManager::new(redis::Client::open(redis_url).unwrap())
            .await
            .unwrap();
        let _: String = redis::cmd("FLUSHALL")
            .query_async(&mut connection)
            .await
            .unwrap();
        drop(connection);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            redis_url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        for (close, price) in [
            (P1D3_PLACE_DECISION_CLOSE_MS, 2_210),
            (P1D3_CANCEL_DECISION_CLOSE_MS, candidate_price),
        ] {
            transport
                .publish_canonical_m10(&canonical_m10(identity.clone(), close, price), &identity)
                .await
                .unwrap();
        }
        let pending_m10 = transport.backend.read_next_fresh().await.unwrap();
        let binding = binding_from_delivery(&pending_m10, identity.clone());
        let attribution = stage7.stage8b_p1d3_test_working_book_attribution().unwrap();
        let attribution = if filled_exit || initial_short {
            let (prefix, _) = attribution.internal_comment().rsplit_once("|r=").unwrap();
            HybridRuntimeAttribution::parse_source_comment(format!("{prefix}|r=EXIT")).unwrap()
        } else {
            attribution
        };
        let request_id =
            StrategyRequestId::from(Uuid::from_u128(0xd401_0000_0000_4000_8000_0000_0000_0001));
        let (mut command, attribution, _) = p1d3_place_command(attribution, request_id);
        let BrokerCommand::PlaceOrder(place) = &mut command else {
            unreachable!("P1-d4 LIMIT fixture returned a non-PLACE command")
        };
        if filled_exit {
            place.side = OrderSide::Sell;
            place.limit_price = Some(Decimal::new(2_230, 0));
        }
        mutate(place);
        let durable = stage7
            .stage8b_p1d3_test_inject_one_intent(binding, command, attribution, &key)
            .ok()?;
        let published = Stage8bP1RedisPrepublicationPending {
            durable,
            transport,
            pending_m10,
        }
        .publish_exact_command()
        .await
        .ok()?;
        Some((key, fresh, identity, published))
    }

    async fn prepare_p1d3_terminal_cancel_source(
        redis_url: &str,
        parent: &Path,
    ) -> (
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        Stage8bP1RedisCommandPublished,
    ) {
        prepare_p1d3_terminal_cancel_source_with_ids(
            redis_url,
            parent,
            StrategyRequestId::from(Uuid::from_u128(0xd301_0000_0000_4000_8000_0000_0000_0001)),
            StrategyRequestId::from(Uuid::from_u128(0xd302_0000_0000_4000_8000_0000_0000_0001)),
            false,
        )
        .await
    }

    #[derive(Clone, Copy)]
    enum P1d4CancelTargetState {
        Working,
        Filled,
    }

    async fn prepare_p1d4_cancel_source(
        redis_url: &str,
        parent: &Path,
        target_state: P1d4CancelTargetState,
    ) -> (
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        Stage8bP1RedisCommandPublished,
    ) {
        let (mut pending, key, fresh, identity) = one_intent_pending_at(redis_url, parent).await;
        pending
            .transport
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), 1_785_759_600_000, 2_175),
                &identity,
            )
            .await
            .unwrap();
        let resolved = pending
            .publish_exact_command()
            .await
            .unwrap()
            .execute_next_canonical_market(p1d2_test_schedule_authority(), &key)
            .await
            .unwrap()
            .commit_truth(&key)
            .unwrap()
            .acknowledge_source()
            .await
            .unwrap();
        let Stage8bP1RedisSemanticCompositionOwner { stage7, transport } =
            *resolved.into_ready_owner();
        drop(transport);
        let stage7 = stage7
            .migrate_stage8b_p1d3_from_resolved_p1d2(&key)
            .unwrap();

        let inherited_attribution = stage7.stage8b_p1d3_test_working_book_attribution().unwrap();
        let (prefix, _) = inherited_attribution
            .internal_comment()
            .rsplit_once("|r=")
            .unwrap();
        let place_attribution =
            HybridRuntimeAttribution::parse_source_comment(format!("{prefix}|r=EXIT")).unwrap();
        let place_request_id =
            StrategyRequestId::from(Uuid::from_u128(0xd431_0000_0000_4000_8000_0000_0000_0001));
        let (mut place, place_attribution, _place_client_order_id) =
            p1d3_place_command(place_attribution, place_request_id);
        let BrokerCommand::PlaceOrder(place_order) = &mut place else {
            unreachable!("P1-d4 test place helper returned a non-PLACE command")
        };
        place_order.side = OrderSide::Sell;
        place_order.limit_price = Some(Decimal::new(2_230, 0));
        let durable = stage7
            .stage8b_p1d3_test_inject_one_intent(
                p1d3_test_binding(&identity, P1D3_PLACE_DECISION_CLOSE_MS, 2_210),
                place,
                place_attribution.clone(),
                &key,
            )
            .unwrap();
        let (stage7, _, _) = durable.into_p1c_parts();
        let stage7 = match target_state {
            P1d4CancelTargetState::Working | P1d4CancelTargetState::Filled => {
                let candidate_price = match target_state {
                    P1d4CancelTargetState::Working => 2_220,
                    P1d4CancelTargetState::Filled => 2_230,
                };
                let candidate = parse_stage8b_p1_canonical_m10(
                    &canonical_m10(
                        identity.clone(),
                        P1D3_CANCEL_DECISION_CLOSE_MS,
                        candidate_price,
                    ),
                    &identity,
                )
                .unwrap()
                .into_p1d3_limit_evidence()
                .unwrap();
                stage7
                    .commit_stage8b_p1d3_initial_limit_ack(
                        Stage8bP1d3InitialObservation::Candidate {
                            evidence: Box::new(candidate),
                            schedule: p1d3_cancel_schedule(
                                P1D3_PLACE_DECISION_CLOSE_MS,
                                P1D3_CANCEL_DECISION_CLOSE_MS,
                            ),
                        },
                        &key,
                    )
                    .unwrap()
                    .commit_truth(&key)
                    .unwrap()
                    .into_ready_after_source_resolution()
            }
        };
        let target = stage7
            .stage8b_p1d3_test_latest_outcome_broker_order_id()
            .unwrap();

        let mut connection = ConnectionManager::new(redis::Client::open(redis_url).unwrap())
            .await
            .unwrap();
        let _: String = redis::cmd("FLUSHALL")
            .query_async(&mut connection)
            .await
            .unwrap();
        drop(connection);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            redis_url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        for (close, price) in [
            (P1D3_CANCEL_DECISION_CLOSE_MS, 2_220),
            (P1D3_CANCEL_CANDIDATE_CLOSE_MS, 2_225),
        ] {
            transport
                .publish_canonical_m10(&canonical_m10(identity.clone(), close, price), &identity)
                .await
                .unwrap();
        }
        let pending_m10 = transport.backend.read_next_fresh().await.unwrap();
        let binding = binding_from_delivery(&pending_m10, identity.clone());
        let cancel_request_id =
            StrategyRequestId::from(Uuid::from_u128(0xd432_0000_0000_4000_8000_0000_0000_0001));
        let (cancel, cancel_attribution) =
            p1d3_cancel_command(&fresh, target, &place_attribution, cancel_request_id, None);
        let durable = stage7
            .stage8b_p1d3_test_inject_one_intent(binding, cancel, cancel_attribution, &key)
            .unwrap();
        let published = Stage8bP1RedisPrepublicationPending {
            durable,
            transport,
            pending_m10,
        }
        .publish_exact_command()
        .await
        .unwrap();
        (key, fresh, identity, published)
    }

    async fn prepare_p1d3_terminal_cancel_source_with_ids(
        redis_url: &str,
        parent: &Path,
        place_request_id: StrategyRequestId,
        cancel_request_id: StrategyRequestId,
        supply_target_client_order_id: bool,
    ) -> (
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        Stage8bP1RedisCommandPublished,
    ) {
        let (mut pending, key, fresh, identity) = one_intent_pending_at(redis_url, parent).await;
        pending
            .transport
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), 1_785_759_600_000, 2_175),
                &identity,
            )
            .await
            .unwrap();
        let resolved = pending
            .publish_exact_command()
            .await
            .unwrap()
            .execute_next_canonical_market(p1d2_test_schedule_authority(), &key)
            .await
            .unwrap()
            .commit_truth(&key)
            .unwrap()
            .acknowledge_source()
            .await
            .unwrap();
        let Stage8bP1RedisSemanticCompositionOwner { stage7, transport } =
            *resolved.into_ready_owner();
        drop(transport);
        let stage7 = stage7
            .migrate_stage8b_p1d3_from_resolved_p1d2(&key)
            .unwrap();

        let place_attribution = stage7.stage8b_p1d3_test_working_book_attribution().unwrap();
        let (place, place_attribution, place_client_order_id) =
            p1d3_place_command(place_attribution, place_request_id);
        let durable = stage7
            .stage8b_p1d3_test_inject_one_intent(
                p1d3_test_binding(&identity, P1D3_PLACE_DECISION_CLOSE_MS, 2_210),
                place,
                place_attribution.clone(),
                &key,
            )
            .unwrap();
        let (stage7, _, _) = durable.into_p1c_parts();
        let trading_day = Utc
            .timestamp_millis_opt(P1D3_PLACE_DECISION_CLOSE_MS)
            .single()
            .unwrap()
            .date_naive()
            .to_string();
        let expiry = stage8b_p1d3_test_expiry_authority(
            "44".repeat(32),
            trading_day,
            format!("{P1D3_PLACE_DECISION_CLOSE_MS}-0"),
            P1D3_CANCEL_DECISION_CLOSE_MS,
        );
        let stage7 = stage7
            .commit_stage8b_p1d3_initial_limit_ack(
                Stage8bP1d3InitialObservation::DayExpiry { authority: expiry },
                &key,
            )
            .unwrap()
            .commit_truth(&key)
            .unwrap()
            .into_ready_after_source_resolution();
        let target = stage7
            .stage8b_p1d3_test_latest_outcome_broker_order_id()
            .unwrap();

        let mut connection = ConnectionManager::new(redis::Client::open(redis_url).unwrap())
            .await
            .unwrap();
        let _: String = redis::cmd("FLUSHALL")
            .query_async(&mut connection)
            .await
            .unwrap();
        drop(connection);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            redis_url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        for (close, price) in [
            (P1D3_CANCEL_DECISION_CLOSE_MS, 2_220),
            (P1D3_CANCEL_CANDIDATE_CLOSE_MS, 2_225),
        ] {
            transport
                .publish_canonical_m10(&canonical_m10(identity.clone(), close, price), &identity)
                .await
                .unwrap();
        }
        let pending_m10 = transport.backend.read_next_fresh().await.unwrap();
        assert_eq!(
            pending_m10.redis_id(),
            format!("{P1D3_CANCEL_DECISION_CLOSE_MS}-0")
        );
        let binding = binding_from_delivery(&pending_m10, identity.clone());
        let (cancel, cancel_attribution) = p1d3_cancel_command(
            &fresh,
            target,
            &place_attribution,
            cancel_request_id,
            supply_target_client_order_id.then_some(place_client_order_id),
        );
        let durable = stage7
            .stage8b_p1d3_test_inject_one_intent(binding, cancel, cancel_attribution, &key)
            .unwrap();
        let published = Stage8bP1RedisPrepublicationPending {
            durable,
            transport,
            pending_m10,
        }
        .publish_exact_command()
        .await
        .unwrap();
        assert_eq!(
            published.pending_m10_redis_id(),
            format!("{P1D3_CANCEL_DECISION_CLOSE_MS}-0")
        );
        (key, fresh, identity, published)
    }

    async fn prepare_p1d3_working_target_first_cancel_source(
        redis_url: &str,
        parent: &Path,
    ) -> (
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        Stage8bP1RedisCommandPublished,
    ) {
        let (mut pending, key, fresh, identity) = one_intent_pending_at(redis_url, parent).await;
        pending
            .transport
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), 1_785_759_600_000, 2_175),
                &identity,
            )
            .await
            .unwrap();
        let resolved = pending
            .publish_exact_command()
            .await
            .unwrap()
            .execute_next_canonical_market(p1d2_test_schedule_authority(), &key)
            .await
            .unwrap()
            .commit_truth(&key)
            .unwrap()
            .acknowledge_source()
            .await
            .unwrap();
        let Stage8bP1RedisSemanticCompositionOwner { stage7, transport } =
            *resolved.into_ready_owner();
        drop(transport);
        let stage7 = stage7
            .migrate_stage8b_p1d3_from_resolved_p1d2(&key)
            .unwrap();

        let inherited_attribution = stage7.stage8b_p1d3_test_working_book_attribution().unwrap();
        let (prefix, _) = inherited_attribution
            .internal_comment()
            .rsplit_once("|r=")
            .unwrap();
        let place_attribution =
            HybridRuntimeAttribution::parse_source_comment(format!("{prefix}|r=EXIT")).unwrap();
        let place_request_id =
            StrategyRequestId::from(Uuid::from_u128(0xd301_0000_0000_4000_8000_0000_0000_0001));
        let (mut place, place_attribution, _) =
            p1d3_place_command(place_attribution, place_request_id);
        let BrokerCommand::PlaceOrder(place_order) = &mut place else {
            unreachable!("P1-d3 test place helper returned a non-PLACE command")
        };
        place_order.side = OrderSide::Sell;
        place_order.limit_price = Some(Decimal::new(2_230, 0));
        let durable = stage7
            .stage8b_p1d3_test_inject_one_intent(
                p1d3_test_binding(&identity, P1D3_PLACE_DECISION_CLOSE_MS, 2_210),
                place,
                place_attribution.clone(),
                &key,
            )
            .unwrap();
        let (stage7, _, _) = durable.into_p1c_parts();
        let initial_candidate = parse_stage8b_p1_canonical_m10(
            &canonical_m10(identity.clone(), P1D3_CANCEL_DECISION_CLOSE_MS, 2_220),
            &identity,
        )
        .unwrap()
        .into_p1d3_limit_evidence()
        .unwrap();
        let stage7 = stage7
            .commit_stage8b_p1d3_initial_limit_ack(
                Stage8bP1d3InitialObservation::Candidate {
                    evidence: Box::new(initial_candidate),
                    schedule: p1d3_cancel_schedule(
                        P1D3_PLACE_DECISION_CLOSE_MS,
                        P1D3_CANCEL_DECISION_CLOSE_MS,
                    ),
                },
                &key,
            )
            .unwrap()
            .commit_truth(&key)
            .unwrap()
            .into_ready_after_source_resolution();
        let target = stage7
            .stage8b_p1d3_test_latest_outcome_broker_order_id()
            .unwrap();

        let mut connection = ConnectionManager::new(redis::Client::open(redis_url).unwrap())
            .await
            .unwrap();
        let _: String = redis::cmd("FLUSHALL")
            .query_async(&mut connection)
            .await
            .unwrap();
        drop(connection);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            redis_url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        for (close, price) in [
            (P1D3_CANCEL_DECISION_CLOSE_MS, 2_220),
            // SELL LIMIT 2230 is touched by high=2231, so target truth wins.
            (P1D3_CANCEL_CANDIDATE_CLOSE_MS, 2_230),
        ] {
            transport
                .publish_canonical_m10(&canonical_m10(identity.clone(), close, price), &identity)
                .await
                .unwrap();
        }
        let pending_m10 = transport.backend.read_next_fresh().await.unwrap();
        let binding = binding_from_delivery(&pending_m10, identity.clone());
        let cancel_request_id =
            StrategyRequestId::from(Uuid::from_u128(0xd302_0000_0000_4000_8000_0000_0000_0001));
        let (cancel, cancel_attribution) =
            p1d3_cancel_command(&fresh, target, &place_attribution, cancel_request_id, None);
        let durable = stage7
            .stage8b_p1d3_test_inject_one_intent(binding, cancel, cancel_attribution, &key)
            .unwrap();
        let published = Stage8bP1RedisPrepublicationPending {
            durable,
            transport,
            pending_m10,
        }
        .publish_exact_command()
        .await
        .unwrap();
        (key, fresh, identity, published)
    }

    fn wait_for_p1d2_crash_barrier(child: &mut Child, marker: &Path) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !marker.exists() && Instant::now() < deadline {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("P1-d2 crash child exited before barrier: {status}");
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(marker.exists(), "P1-d2 crash child missed barrier");
    }

    fn wait_for_p1d3_crash_barrier(child: &mut Child, marker: &Path) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !marker.exists() && Instant::now() < deadline {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("P1-d3 crash child exited before barrier: {status}");
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(marker.exists(), "P1-d3 crash child missed barrier");
    }

    #[derive(Debug)]
    struct P1d4RegistryCell<'a> {
        cell_id: &'a str,
        scenario_id: &'a str,
        frontier_id: &'a str,
        kill_hook_name: &'a str,
        expected_restart_disposition: &'a str,
    }

    fn p1d4_registry_cells() -> Vec<P1d4RegistryCell<'static>> {
        let matrix = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv"
        ));
        matrix
            .lines()
            .skip(1)
            .map(|line| {
                let fields = line.split(',').collect::<Vec<_>>();
                assert_eq!(fields.len(), 19, "P1-d4 registry row must remain exact");
                P1d4RegistryCell {
                    cell_id: fields[0],
                    scenario_id: fields[1],
                    frontier_id: fields[4],
                    kill_hook_name: fields[6],
                    expected_restart_disposition: fields[7],
                }
            })
            .collect()
    }

    fn p1d4_registry_cell(scenario_id: &str, frontier_id: &str) -> P1d4RegistryCell<'static> {
        let matches = p1d4_registry_cells()
            .into_iter()
            .filter(|cell| cell.scenario_id == scenario_id && cell.frontier_id == frontier_id)
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "P1-d4 scenario/frontier lookup must be unique: {scenario_id}/{frontier_id}"
        );
        matches.into_iter().next().unwrap()
    }

    fn p1d4_generated_market_registry_cells() -> Vec<P1d4RegistryCell<'static>> {
        let matrix = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv"
        ));
        matrix
            .lines()
            .skip(1)
            .map(|line| {
                let fields = line.split(',').collect::<Vec<_>>();
                assert_eq!(
                    fields.len(),
                    28,
                    "P1-d4 generated-Market registry row must remain exact"
                );
                P1d4RegistryCell {
                    cell_id: fields[0],
                    scenario_id: fields[1],
                    frontier_id: fields[2],
                    kill_hook_name: match fields[2] {
                        "GM00" => "p1d4-generated-market-gm00",
                        "GM01" => "p1d4-generated-market-gm01",
                        "GM02" => "p1d4-generated-market-gm02",
                        "GM03" => "p1d4-generated-market-gm03",
                        "GM04" => "p1d4-generated-market-gm04",
                        "GM05" => "p1d4-generated-market-gm05",
                        "GM06" => "p1d4-generated-market-gm06",
                        "GM07" => "p1d4-generated-market-gm07",
                        "GM08" => "p1d4-generated-market-gm08",
                        "GM09" => "p1d4-generated-market-gm09",
                        "GM10" => "p1d4-generated-market-gm10",
                        "GM11" => "p1d4-generated-market-gm11",
                        "GM12" => "p1d4-generated-market-gm12",
                        _ => panic!("unknown generated-Market frontier"),
                    },
                    expected_restart_disposition: fields[5],
                }
            })
            .collect()
    }

    fn p1d4_scenario_name(scenario_id: &str) -> &'static str {
        match scenario_id {
            "S01" => "initial-working",
            "S02" => "initial-filled",
            "S03" => "initial-expired",
            "S04" => "later-untouched-zero",
            "S05" => "later-untouched-one",
            "S06" => "later-filled",
            "S07" => "later-expired",
            "S08" => "cancel-canceled",
            "S09" => "cancel-target-first",
            "S10" => "cancel-filled",
            "S11" => "cancel-expired",
            _ => panic!("unknown P1-d4 registry scenario: {scenario_id}"),
        }
    }

    fn p1d4_restart_disposition(outcome: &Stage7bRestartOutcome) -> &'static str {
        match outcome {
            Stage7bRestartOutcome::Ready(_) => "Ready",
            Stage7bRestartOutcome::P1SemanticPrepublicationReady(_) => {
                "P1SemanticPrepublicationReady"
            }
            Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(_) => {
                "P1SemanticZeroIntentAckPending"
            }
            Stage7bRestartOutcome::P1d3DispatchPending(_) => "P1d3DispatchPending",
            Stage7bRestartOutcome::P1d3PreAckPending(_) => "P1d3PreAckPending",
            Stage7bRestartOutcome::P1d3AckCommitted(_) => "P1d3AckCommitted",
            Stage7bRestartOutcome::P1d3TruthCommitted(_) => "P1d3TruthCommitted",
            Stage7bRestartOutcome::P1d3CancelContinuationPending(_) => {
                "P1d3CancelContinuationPending"
            }
            Stage7bRestartOutcome::P1d3SemanticPending(_) => "P1d3SemanticPending",
            Stage7bRestartOutcome::P1SemanticPrepublicationPending(_) => {
                "P1SemanticPrepublicationPending"
            }
            Stage7bRestartOutcome::P1d2PreAckPending(_) => "P1d2PreAckPending",
            Stage7bRestartOutcome::P1d2AckCommitted(_) => "P1d2AckCommitted",
            Stage7bRestartOutcome::P1d2TruthCommitted(_) => "P1d2TruthCommitted",
            Stage7bRestartOutcome::P1d4GeneratedMarketPrepublicationPending(_) => {
                "P1d4GeneratedMarketPrepublicationPending"
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketDispatchPending(_) => {
                "P1d4GeneratedMarketDispatchPending"
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketOrderPending(_) => {
                "P1d4GeneratedMarketOrderPending"
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketPreFinalizationPending(_) => {
                "P1d4GeneratedMarketPreFinalizationPending"
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketPreAckPending(_) => {
                "P1d4GeneratedMarketPreAckPending"
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketAckCommitted(_) => {
                "P1d4GeneratedMarketAckCommitted"
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketTruthCommitted(_) => {
                "P1d4GeneratedMarketTruthCommitted"
            }
            Stage7bRestartOutcome::Stage8a4I3Pending(_) => "Stage8a4I3Pending",
            Stage7bRestartOutcome::Blocked(_) => "Blocked",
        }
    }

    async fn p1d4_effective_restart_disposition(
        outcome: Stage7bRestartOutcome,
        redis: &RedisServer,
        expected: &str,
    ) -> &'static str {
        if expected != "Ready" {
            return p1d4_restart_disposition(&outcome);
        }
        match outcome {
            Stage7bRestartOutcome::Ready(_) => "Ready",
            Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(pending) => {
                tokio::time::sleep(Duration::from_millis(5)).await;
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                let resolved = resolve_stage8b_p1_zero_intent_ack_with_redis(*pending, transport)
                    .await
                    .unwrap();
                assert_eq!(
                    resolved.disposition(),
                    Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged
                );
                drop(resolved.into_ready_owner());
                "Ready"
            }
            Stage7bRestartOutcome::P1d3TruthCommitted(truth) => {
                tokio::time::sleep(Duration::from_millis(5)).await;
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                let resolved = resume_stage8b_p1d3_truth_with_redis(*truth, transport)
                    .await
                    .unwrap()
                    .acknowledge_source()
                    .await
                    .unwrap();
                assert_eq!(
                    resolved.disposition(),
                    Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged
                );
                drop(resolved.into_ready_owner());
                "Ready"
            }
            other => p1d4_restart_disposition(&other),
        }
    }

    fn assert_p1d4_marker_and_sigkill(
        child: &mut Child,
        marker: &Path,
        parent: &Path,
        cell: &P1d4RegistryCell<'_>,
    ) {
        wait_for_p1d3_crash_barrier(child, marker);
        assert!(child.try_wait().unwrap().is_none());
        let marker_bytes = fs::read(marker).unwrap();
        let marker_json: serde_json::Value = serde_json::from_slice(&marker_bytes).unwrap();
        let marker_object = marker_json.as_object().unwrap();
        assert_eq!(marker_object.len(), 8);
        assert_eq!(marker_json["schema_version"], 1);
        assert_eq!(marker_json["domain"], "moex.stage8b.p1d4.crash-marker.v1");
        assert_eq!(marker_json["child_pid"], u64::from(child.id()));
        assert_eq!(marker_json["cell_id"], cell.cell_id);
        assert_eq!(marker_json["scenario_id"], cell.scenario_id);
        assert_eq!(marker_json["frontier_id"], cell.frontier_id);
        assert_eq!(marker_json["kill_hook_name"], cell.kill_hook_name);
        let pre_kill_audit_sha256 = marker_json["pre_kill_audit_sha256"].as_str().unwrap();
        assert_eq!(
            pre_kill_audit_sha256,
            crate::recovery::stage8b_p1d4_pre_kill_audit_sha256(
                parent,
                marker,
                cell.cell_id,
                cell.scenario_id,
                cell.frontier_id,
                cell.kill_hook_name,
            )
        );
        let expected_canonical = format!(
            "{{\"cell_id\":\"{}\",\"child_pid\":{},\"domain\":\"moex.stage8b.p1d4.crash-marker.v1\",\"frontier_id\":\"{}\",\"kill_hook_name\":\"{}\",\"pre_kill_audit_sha256\":\"{}\",\"scenario_id\":\"{}\",\"schema_version\":1}}",
            cell.cell_id,
            child.id(),
            cell.frontier_id,
            cell.kill_hook_name,
            pre_kill_audit_sha256,
            cell.scenario_id,
        );
        assert_eq!(marker_bytes, expected_canonical.as_bytes());

        let normalized = format!(
            "{{\"cell_id\":\"{}\",\"child_pid\":0,\"domain\":\"moex.stage8b.p1d4.crash-marker.v1\",\"frontier_id\":\"{}\",\"kill_hook_name\":\"{}\",\"pre_kill_audit_sha256\":\"{}\",\"scenario_id\":\"{}\",\"schema_version\":1}}",
            cell.cell_id,
            cell.frontier_id,
            cell.kill_hook_name,
            pre_kill_audit_sha256,
            cell.scenario_id,
        );
        let mut normalized_hasher = Sha256::new();
        normalized_hasher.update(b"moex.stage8b.p1d4.crash-marker.normalized.v1\0");
        normalized_hasher.update((normalized.len() as u64).to_be_bytes());
        normalized_hasher.update(normalized.as_bytes());
        assert_eq!(format!("{:x}", normalized_hasher.finalize()).len(), 64);
        child.kill().unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.code(), None);
        assert_eq!(status.signal(), Some(libc::SIGKILL));
    }

    #[tokio::test]
    #[ignore]
    async fn p1d4_dispatch_frontier_child() {
        let parent = PathBuf::from(std::env::var_os("STAGE8B_P1_TEST_PARENT").unwrap());
        let redis_url = std::env::var("STAGE8B_P1_TEST_REDIS_URL").unwrap();
        let scenario = std::env::var("STAGE8B_P1D4_SCENARIO").unwrap();
        eprintln!(
            "P1-d4 child scenario={scenario} frontier={}",
            std::env::var("STAGE8B_P1D4_FRONTIER_ID").unwrap_or_default()
        );
        match scenario.as_str() {
            "initial-working" => {
                std::env::set_var("STAGE8B_P1D4_ARMED", "1");
                let (key, _, _, published) =
                    prepare_p1d3_initial_limit_source(&redis_url, &parent, 2_220).await;
                let _ = published
                    .execute_next_canonical_limit(
                        p1d3_cancel_schedule(
                            P1D3_PLACE_DECISION_CLOSE_MS,
                            P1D3_CANCEL_DECISION_CLOSE_MS,
                        ),
                        &key,
                    )
                    .await
                    .unwrap()
                    .commit_truth(&key)
                    .unwrap()
                    .acknowledge_source()
                    .await
                    .unwrap();
            }
            "initial-filled" => {
                std::env::set_var("STAGE8B_P1D4_ARMED", "1");
                let (key, _, _, published) =
                    prepare_p1d3_initial_filled_source(&redis_url, &parent).await;
                let _ = published
                    .execute_next_canonical_limit(
                        p1d3_cancel_schedule(
                            P1D3_PLACE_DECISION_CLOSE_MS,
                            P1D3_CANCEL_DECISION_CLOSE_MS,
                        ),
                        &key,
                    )
                    .await
                    .unwrap()
                    .commit_truth(&key)
                    .unwrap()
                    .acknowledge_source()
                    .await
                    .unwrap();
            }
            "initial-expired" => {
                std::env::set_var("STAGE8B_P1D4_ARMED", "1");
                let (key, _, _, published) =
                    prepare_p1d3_initial_limit_source(&redis_url, &parent, 2_220).await;
                let trading_day = Utc
                    .timestamp_millis_opt(P1D3_PLACE_DECISION_CLOSE_MS)
                    .single()
                    .unwrap()
                    .date_naive()
                    .to_string();
                let authority = stage8b_p1d3_test_expiry_authority(
                    "44".repeat(32),
                    trading_day,
                    format!("{P1D3_PLACE_DECISION_CLOSE_MS}-0"),
                    P1D3_CANCEL_DECISION_CLOSE_MS,
                );
                let _ = published
                    .execute_initial_limit_expiry(authority, &key)
                    .unwrap()
                    .commit_truth(&key)
                    .unwrap()
                    .acknowledge_source()
                    .await
                    .unwrap();
            }
            "later-untouched-zero" | "later-untouched-one" | "later-filled" => {
                let (key, _, identity, mut owner) = match scenario.as_str() {
                    "later-filled" => {
                        prepare_p1d4_later_filled_zero_owner(&redis_url, &parent).await
                    }
                    "later-untouched-one" => {
                        prepare_p1d4_later_working_owner(
                            &redis_url,
                            &parent,
                            false,
                            true,
                            Some(2_000),
                        )
                        .await
                    }
                    _ => {
                        prepare_p1d4_later_working_owner(&redis_url, &parent, false, false, None)
                            .await
                    }
                };
                let (predecessor_close_ms, candidate_close_ms) = match scenario.as_str() {
                    "later-filled" => (
                        P1D3_CANCEL_CANDIDATE_CLOSE_MS + 1_200_000,
                        P1D3_CANCEL_CANDIDATE_CLOSE_MS + 1_800_000,
                    ),
                    _ => (
                        P1D3_CANCEL_CANDIDATE_CLOSE_MS,
                        P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000,
                    ),
                };
                let price = match scenario.as_str() {
                    "later-untouched-zero" => 2_220,
                    "later-untouched-one" => 2_650,
                    "later-filled" => 2_230,
                    _ => unreachable!(),
                };
                owner
                    .transport
                    .publish_canonical_m10(
                        &canonical_m10(identity.clone(), candidate_close_ms, price),
                        &identity,
                    )
                    .await
                    .unwrap();
                if scenario == "later-untouched-one" {
                    owner
                        .transport
                        .publish_canonical_m10(
                            &canonical_m10(identity.clone(), candidate_close_ms + 600_000, 2_175),
                            &identity,
                        )
                        .await
                        .unwrap();
                }
                std::env::set_var("STAGE8B_P1D4_ARMED", "1");
                let outcome = owner
                    .process_next_working_limit(
                        p1d3_cancel_schedule(predecessor_close_ms, candidate_close_ms),
                        &key,
                    )
                    .await
                    .unwrap();
                match (scenario.as_str(), outcome) {
                    (
                        "later-untouched-one",
                        Stage8bP1RedisSemanticOutcome::Prepublication(pending),
                    ) => {
                        let _ = pending
                            .publish_exact_generated_market_command(&key)
                            .await
                            .unwrap()
                            .execute_next_canonical_generated_market(
                                strategy_runtime_core::stage8b_p1d1_test_schedule_authority(
                                    super::super::p1_instrument(),
                                    candidate_close_ms,
                                    candidate_close_ms + 600_000,
                                ),
                                &key,
                            )
                            .await
                            .unwrap()
                            .commit_truth(&key)
                            .await
                            .unwrap()
                            .acknowledge_source()
                            .await
                            .unwrap();
                    }
                    (
                        "later-untouched-zero" | "later-filled",
                        Stage8bP1RedisSemanticOutcome::Ready { .. },
                    ) => {}
                    _ => panic!("P1-d4 later scenario produced the wrong callback outcome"),
                }
            }
            "later-expired" => {
                let (key, _, _, owner) =
                    prepare_p1d4_later_working_owner(&redis_url, &parent, false, false, None).await;
                let trading_day = Utc
                    .timestamp_millis_opt(P1D3_CANCEL_CANDIDATE_CLOSE_MS)
                    .single()
                    .unwrap()
                    .date_naive()
                    .to_string();
                let authority = stage8b_p1d3_test_expiry_authority(
                    "44".repeat(32),
                    trading_day,
                    format!("{P1D3_CANCEL_CANDIDATE_CLOSE_MS}-0"),
                    P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000,
                );
                std::env::set_var("STAGE8B_P1D4_ARMED", "1");
                let _ = owner.expire_working_limit(authority, &key).unwrap();
            }
            "cancel-target-first" => {
                let (key, _, _, published) =
                    prepare_p1d3_working_target_first_cancel_source(&redis_url, &parent).await;
                std::env::set_var("STAGE8B_P1D4_ARMED", "1");
                let outcome = published
                    .execute_next_canonical_cancel(
                        p1d3_cancel_schedule(
                            P1D3_CANCEL_DECISION_CLOSE_MS,
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS,
                        ),
                        &key,
                    )
                    .await
                    .unwrap();
                let Stage8bP1RedisCancelCommitOutcome::CancelContinuationPending(pending) = outcome
                else {
                    panic!("target-first child must enter cancel continuation")
                };
                let _ = pending
                    .commit_recovered_cancel(&key)
                    .unwrap()
                    .acknowledge_source()
                    .await
                    .unwrap();
            }
            "cancel-canceled" | "cancel-filled" | "cancel-expired" => {
                let (key, _, _, published) = if scenario == "cancel-expired" {
                    prepare_p1d3_terminal_cancel_source(&redis_url, &parent).await
                } else {
                    let target_state = match scenario.as_str() {
                        "cancel-canceled" => P1d4CancelTargetState::Working,
                        "cancel-filled" => P1d4CancelTargetState::Filled,
                        _ => unreachable!(),
                    };
                    prepare_p1d4_cancel_source(&redis_url, &parent, target_state).await
                };
                std::env::set_var("STAGE8B_P1D4_ARMED", "1");
                let schedule_source_close_ms = if scenario == "cancel-expired" {
                    // The accepted S11 fixture inherits its already-terminal
                    // target from the original LIMIT decision boundary.
                    P1D3_PLACE_DECISION_CLOSE_MS
                } else {
                    P1D3_CANCEL_DECISION_CLOSE_MS
                };
                let outcome = published
                    .execute_next_canonical_cancel(
                        p1d3_cancel_schedule(
                            schedule_source_close_ms,
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS,
                        ),
                        &key,
                    )
                    .await
                    .unwrap();
                let truth = match outcome {
                    Stage8bP1RedisCancelCommitOutcome::AckCommitted(ack) => {
                        ack.commit_truth(&key).unwrap()
                    }
                    Stage8bP1RedisCancelCommitOutcome::TruthCommitted(truth) => truth,
                    Stage8bP1RedisCancelCommitOutcome::CancelContinuationPending(_) => {
                        panic!("non-target-first cancel child entered target continuation")
                    }
                };
                let _ = truth.acknowledge_source().await.unwrap();
            }
            "market-predecessor" => {
                let (mut pending, key, _, identity) =
                    one_intent_pending_at(&redis_url, &parent).await;
                pending
                    .transport
                    .publish_canonical_m10(
                        &canonical_m10(identity.clone(), 1_785_759_600_000, 2_175),
                        &identity,
                    )
                    .await
                    .unwrap();
                let _ = pending
                    .publish_exact_command()
                    .await
                    .unwrap()
                    .execute_next_canonical_market(p1d2_test_schedule_authority(), &key)
                    .await
                    .unwrap();
            }
            "generated-market" => {
                let (key, _, _, pending, decision_close_ms) =
                    prepare_p1d4_generated_market_prepublication(&redis_url, &parent).await;
                std::env::set_var("STAGE8B_P1D4_ARMED", "1");
                let truth = pending
                    .publish_exact_generated_market_command(&key)
                    .await
                    .unwrap()
                    .execute_next_canonical_generated_market(
                        strategy_runtime_core::stage8b_p1d1_test_schedule_authority(
                            super::super::p1_instrument(),
                            decision_close_ms,
                            decision_close_ms + 600_000,
                        ),
                        &key,
                    )
                    .await
                    .unwrap()
                    .commit_truth(&key)
                    .await
                    .unwrap();
                let _ = truth.acknowledge_source().await.unwrap();
            }
            _ => panic!("unknown P1-d4 crash scenario: {scenario}"),
        }
        panic!("configured P1-d4 crash barrier was not reached");
    }

    async fn spawn_p1d4_exact_frontier(
        redis: &RedisServer,
        parent: &Path,
        scenario: &str,
        scenario_id: &str,
        frontier_id: &str,
    ) {
        let cell = p1d4_registry_cell(scenario_id, frontier_id);
        assert!(!cell.expected_restart_disposition.is_empty());
        let marker = parent.join(format!("{}-{}.marker", cell.cell_id, cell.kill_hook_name));
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("stage8b_p1_semantic::redis::tests::p1d4_dispatch_frontier_child")
            .arg("--nocapture")
            .env("STAGE8B_P1_TEST_PARENT", parent)
            .env("STAGE8B_P1_TEST_CRASH_PHASE", cell.kill_hook_name)
            .env("STAGE8B_P1_TEST_CRASH_MARKER", &marker)
            .env("STAGE8B_P1_TEST_REDIS_URL", &redis.url)
            .env("STAGE8B_P1D4_SCENARIO", scenario)
            .env("STAGE8B_P1D4_CELL_ID", cell.cell_id)
            .env("STAGE8B_P1D4_SCENARIO_ID", cell.scenario_id)
            .env("STAGE8B_P1D4_FRONTIER_ID", cell.frontier_id)
            .env("RUST_MIN_STACK", "16777216")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        assert_p1d4_marker_and_sigkill(&mut child, &marker, parent, &cell);
    }

    async fn spawn_p1d4_generated_market_frontier(
        redis: &RedisServer,
        parent: &Path,
        cell: &P1d4RegistryCell<'_>,
    ) {
        let marker = parent.join(format!("{}-{}.marker", cell.cell_id, cell.kill_hook_name));
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("stage8b_p1_semantic::redis::tests::p1d4_dispatch_frontier_child")
            .arg("--nocapture")
            .env("STAGE8B_P1_TEST_PARENT", parent)
            .env("STAGE8B_P1_TEST_CRASH_PHASE", cell.kill_hook_name)
            .env("STAGE8B_P1_TEST_CRASH_MARKER", &marker)
            .env("STAGE8B_P1_TEST_REDIS_URL", &redis.url)
            .env("STAGE8B_P1D4_SCENARIO", "generated-market")
            .env("STAGE8B_P1D4_CELL_ID", cell.cell_id)
            .env("STAGE8B_P1D4_SCENARIO_ID", cell.scenario_id)
            .env("STAGE8B_P1D4_FRONTIER_ID", cell.frontier_id)
            .env("RUST_MIN_STACK", "16777216")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        assert_p1d4_marker_and_sigkill(&mut child, &marker, parent, cell);
    }

    async fn spawn_p1d4_legacy_frontier(
        redis: &RedisServer,
        parent: &Path,
        scenario: &str,
        phase: &str,
    ) {
        let marker = parent.join(format!("{scenario}-{phase}.marker"));
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("stage8b_p1_semantic::redis::tests::p1d4_dispatch_frontier_child")
            .arg("--nocapture")
            .env("STAGE8B_P1_TEST_PARENT", parent)
            .env("STAGE8B_P1_TEST_CRASH_PHASE", phase)
            .env("STAGE8B_P1_TEST_CRASH_MARKER", &marker)
            .env("STAGE8B_P1_TEST_REDIS_URL", &redis.url)
            .env("STAGE8B_P1D4_SCENARIO", scenario)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_p1d3_crash_barrier(&mut child, &marker);
        let marker_json: serde_json::Value =
            serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
        assert_eq!(marker_json["phase"], phase);
        assert_eq!(marker_json["pid"], u64::from(child.id()));
        child.kill().unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.signal(), Some(libc::SIGKILL));
    }

    #[derive(Clone, Copy)]
    enum P1d3CancelExpectedRestart {
        PreRecoveredSeal,
        Truth,
    }

    #[tokio::test]
    #[ignore]
    async fn p1d3_cancel_recovery_crash_frontier_child() {
        let parent = PathBuf::from(std::env::var_os("STAGE8B_P1_TEST_PARENT").unwrap());
        let redis_url = std::env::var("STAGE8B_P1_TEST_REDIS_URL").unwrap();
        let (key, _, _, published) = prepare_p1d3_terminal_cancel_source(&redis_url, &parent).await;
        let trading_day = Utc
            .timestamp_millis_opt(P1D3_CANCEL_CANDIDATE_CLOSE_MS)
            .single()
            .unwrap()
            .date_naive()
            .to_string();
        let schedule = stage8b_p1d3_test_step_authority(
            "44".repeat(32),
            trading_day,
            format!("{P1D3_CANCEL_CANDIDATE_CLOSE_MS}-0"),
            format!("{P1D3_PLACE_DECISION_CLOSE_MS}-0"),
            format!("{P1D3_CANCEL_CANDIDATE_CLOSE_MS}-0"),
        );
        let _ = published
            .execute_next_canonical_cancel(schedule, &key)
            .await
            .unwrap();
        panic!("configured P1-d3 cancel crash barrier was not reached");
    }

    async fn run_p1d3_cancel_recovery_crash_case(phase: &str, expected: P1d3CancelExpectedRestart) {
        let redis = RedisServer::start().await;
        let parent = temp_directory(phase);
        let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();

        let marker = parent.join(format!("{phase}.marker"));
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("stage8b_p1_semantic::redis::tests::p1d3_cancel_recovery_crash_frontier_child")
            .arg("--nocapture")
            .env("STAGE8B_P1_TEST_PARENT", &parent)
            .env("STAGE8B_P1_TEST_CRASH_PHASE", phase)
            .env("STAGE8B_P1_TEST_CRASH_MARKER", &marker)
            .env("STAGE8B_P1_TEST_REDIS_URL", &redis.url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_p1d3_crash_barrier(&mut child, &marker);
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending_before_resume: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_before_resume.count(), 1, "{phase}");
        let command_count: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(command_count, 1, "{phase}");
        drop(connection);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh.clone(),
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let truth = match (expected, restart) {
            (
                P1d3CancelExpectedRestart::PreRecoveredSeal,
                Stage7bRestartOutcome::P1d3PreAckPending(pending),
            ) => {
                assert!(pending.request_finalized(), "{phase}");
                assert!(!pending.paper_provider_invocation_allowed(), "{phase}");
                assert!(pending.ack_reconstruction_allowed(), "{phase}");
                assert!(!pending.broker_truth_allowed(), "{phase}");
                assert!(!pending.m10_xack_allowed(), "{phase}");
                let Stage8bP1RedisPreAckRecoveryOutcome::TruthCommitted(truth) =
                    resume_stage8b_p1d3_pre_ack_with_redis(*pending, transport, &key)
                        .await
                        .unwrap()
                else {
                    panic!("{phase} must reconstruct only recovered CANCEL truth");
                };
                truth
            }
            (
                P1d3CancelExpectedRestart::Truth,
                Stage7bRestartOutcome::P1d3TruthCommitted(truth),
            ) => resume_stage8b_p1d3_truth_with_redis(*truth, transport)
                .await
                .unwrap(),
            _ => panic!("{phase} recovered the wrong typed P1-d3 authority"),
        };
        assert!(truth.m10_xack_allowed(), "{phase}");
        assert_eq!(
            truth.pending_m10_redis_id(),
            format!("{P1D3_CANCEL_DECISION_CLOSE_MS}-0"),
            "{phase}"
        );
        let resolved = truth.acknowledge_source().await.unwrap();
        assert_eq!(
            resolved.disposition(),
            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending,
            "{phase}"
        );
        drop(resolved);

        let mut connection = redis.connection().await;
        let pending_after_xack: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_after_xack.count(), 0, "{phase}");
        let final_restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        assert!(
            matches!(final_restart, Stage7bRestartOutcome::P1d3TruthCommitted(_)),
            "{phase}"
        );
        fs::remove_dir_all(parent).unwrap();
    }

    #[derive(Clone, Copy)]
    enum P1d2ExpectedRestart {
        PreAck { request_finalized: bool },
        Ack,
        Truth,
    }

    #[tokio::test]
    #[ignore]
    async fn p1d2_crash_frontier_child() {
        let parent = PathBuf::from(std::env::var_os("STAGE8B_P1_TEST_PARENT").unwrap());
        let redis_url = std::env::var("STAGE8B_P1_TEST_REDIS_URL").unwrap();
        let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let config = validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent,
            fresh.stage5c_config_fingerprint(),
        ))
        .unwrap();
        let restart = restart_stage8b_p1(config, &key, fresh).unwrap();
        let Stage7bRestartOutcome::Ready(owner) = restart else {
            panic!("P1-d2 crash child must begin from the exact S0 Ready state");
        };
        let transport =
            attach_stage8b_p1_redis(&redis_url, Stage8bP1RedisConfig::paper_default_auto())
                .await
                .unwrap();
        let outcome = Stage8bP1RedisSemanticCompositionOwner::new(*owner, transport)
            .process_next(&key)
            .await
            .unwrap();
        let Stage8bP1RedisSemanticOutcome::Prepublication(pending) = outcome else {
            panic!("P1-d2 crash child requires one retained Market command");
        };
        let published = pending.publish_exact_command().await.unwrap();
        let ack = published
            .execute_next_canonical_market(p1d2_test_schedule_authority(), &key)
            .await
            .unwrap();
        let truth = ack.commit_truth(&key).unwrap();
        let _ = truth.acknowledge_source().await.unwrap();
        panic!("configured P1-d2 crash barrier was not reached");
    }

    async fn run_p1d2_crash_case(phase: &str, expected: P1d2ExpectedRestart) {
        let redis = RedisServer::start().await;
        let parent = temp_directory(phase);
        let (owner, key, fresh, identity) = first_boot(&parent);
        drop(owner);

        let predecessor_close = 1_785_759_000_000_i64;
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        for (close, price) in [
            (predecessor_close, 2_650),
            (predecessor_close + 600_000, 2_175),
        ] {
            transport
                .publish_canonical_m10(&canonical_m10(identity.clone(), close, price), &identity)
                .await
                .unwrap();
        }
        drop(transport);

        let marker = parent.join(format!("{phase}.marker"));
        let sequence_pair_marker = parent.join(format!("{phase}.sequence-pair"));
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("stage8b_p1_semantic::redis::tests::p1d2_crash_frontier_child")
            .arg("--nocapture")
            .env("STAGE8B_P1_TEST_PARENT", &parent)
            .env("STAGE8B_P1_TEST_REDIS_URL", &redis.url)
            .env("STAGE8B_P1_TEST_CRASH_PHASE", phase)
            .env("STAGE8B_P1_TEST_CRASH_MARKER", &marker)
            .env(
                "STAGE8B_P1_TEST_SEQUENCE_PAIR_MARKER",
                &sequence_pair_marker,
            )
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_p1d2_crash_barrier(&mut child, &marker);
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        let pre_kill_sequence_pair = if phase == "p1d2-after-sequence-pair-before-ack" {
            let marker = fs::read_to_string(&sequence_pair_marker)
                .expect("pre-kill sequence-pair marker must exist");
            let mut lines = marker.lines();
            let seq_ack = lines
                .next()
                .and_then(|line| line.strip_prefix("seq_ack="))
                .and_then(|value| value.parse::<u64>().ok())
                .expect("pre-kill seq_ack marker must be canonical");
            let seq_truth = lines
                .next()
                .and_then(|line| line.strip_prefix("seq_truth="))
                .and_then(|value| value.parse::<u64>().ok())
                .expect("pre-kill seq_truth marker must be canonical");
            assert!(
                lines.next().is_none(),
                "sequence-pair marker has extra rows"
            );
            Some((seq_ack, seq_truth))
        } else {
            assert!(
                !sequence_pair_marker.exists(),
                "non-pair crash frontier wrote sequence authority"
            );
            None
        };

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh.clone(),
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let truth = match (expected, restart) {
            (
                P1d2ExpectedRestart::PreAck { request_finalized },
                Stage7bRestartOutcome::P1d2PreAckPending(pending),
            ) => {
                assert_eq!(pending.request_finalized(), request_finalized, "{phase}");
                assert!(!pending.paper_provider_invocation_allowed(), "{phase}");
                assert!(pending.ack_reconstruction_allowed(), "{phase}");
                assert!(!pending.broker_truth_allowed(), "{phase}");
                assert!(!pending.m10_xack_allowed(), "{phase}");
                let ack = resume_stage8b_p1d2_pre_ack_with_redis(*pending, transport, &key)
                    .await
                    .unwrap();
                assert!(!ack.m10_xack_allowed(), "{phase}");
                ack.commit_truth(&key).unwrap()
            }
            (P1d2ExpectedRestart::Ack, Stage7bRestartOutcome::P1d2AckCommitted(ack)) => {
                let ack = resume_stage8b_p1d2_ack_with_redis(*ack, transport)
                    .await
                    .unwrap();
                assert!(!ack.m10_xack_allowed(), "{phase}");
                ack.commit_truth(&key).unwrap()
            }
            (P1d2ExpectedRestart::Truth, Stage7bRestartOutcome::P1d2TruthCommitted(truth)) => {
                resume_stage8b_p1d2_truth_with_redis(*truth, transport)
                    .await
                    .unwrap()
            }
            _ => panic!("{phase} recovered the wrong typed P1-d2 authority"),
        };
        assert!(truth.m10_xack_allowed(), "{phase}");
        let resolved = truth.acknowledge_source().await.unwrap();
        assert_eq!(
            resolved.disposition(),
            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending,
            "{phase}"
        );
        let audit = resolved.audit_evidence();
        assert_eq!(audit.schema_version, 1, "{phase}");
        assert_eq!(
            audit.core.seq_ack.checked_add(1),
            Some(audit.core.seq_truth),
            "{phase}"
        );
        assert_eq!(audit.audit_sha256.len(), 64, "{phase}");
        if let Some(pre_kill_sequence_pair) = pre_kill_sequence_pair {
            assert_eq!(
                (audit.core.seq_ack, audit.core.seq_truth),
                pre_kill_sequence_pair,
                "restart must recover the exact pre-kill sequence pair"
            );
        }
        drop(resolved);

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        let command_count: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0, "{phase}");
        assert_eq!(command_count, 1, "{phase}");

        let final_restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        assert!(
            matches!(final_restart, Stage7bRestartOutcome::P1d2TruthCommitted(_)),
            "{phase}"
        );
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1c_real_redis_creates_groups_before_exact_m10_and_rejects_collision() {
        let redis = RedisServer::start().await;
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let namespace = stage8b_p1_redis_namespace();
        let identity = "11".repeat(32);
        let close_ts = 1_785_759_000_000_i64;
        let bytes = canonical_m10(identity.clone(), close_ts, 2_600);

        let repeated_fresh = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        drop(repeated_fresh);

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
    async fn p1c_initializer_rejects_historical_stream_with_missing_group() {
        let redis = RedisServer::start().await;
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let namespace = stage8b_p1_redis_namespace();
        let identity = "11".repeat(32);
        let bytes = canonical_m10(identity.clone(), 1_785_759_000_000, 2_600);
        transport
            .publish_canonical_m10(&bytes, &identity)
            .await
            .unwrap();
        drop(transport);

        let mut connection = redis.connection().await;
        let removed: usize = redis::cmd("XGROUP")
            .arg("DESTROY")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(removed, 1);
        assert!(matches!(
            initialize_stage8b_p1_redis_namespace(
                &redis.url,
                Stage8bP1RedisConfig::paper_default_auto(),
            )
            .await,
            Err(Stage8bP1RedisSemanticError::NamespaceNotFresh)
        ));
        let pending: redis::RedisResult<StreamPendingReply> = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await;
        assert!(pending.is_err());
    }

    #[tokio::test]
    async fn p1c_zero_intent_xacks_last_and_restart_is_ack_only() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("zero-intent");
        let (owner, key, fresh, identity) = first_boot(&parent);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
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
            attach_stage8b_p1_redis(&redis.url, Stage8bP1RedisConfig::paper_default_auto())
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
    async fn p1c_zero_intent_restart_rejects_deleted_m10_group_without_recreation() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("zero-intent-group-loss");
        let (owner, key, fresh, identity) = first_boot(&parent);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
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
        let Stage8bP1RedisSemanticOutcome::Ready { owner, .. } = outcome else {
            panic!("fixture must commit zero-intent S1");
        };
        drop(owner);

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let removed: usize = redis::cmd("XGROUP")
            .arg("DESTROY")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(removed, 1);
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
        assert!(matches!(
            restart,
            Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(_)
        ));
        assert!(matches!(
            attach_stage8b_p1_redis(&redis.url, Stage8bP1RedisConfig::paper_default_auto()).await,
            Err(Stage8bP1RedisSemanticError::GroupMissing)
        ));
        let pending: redis::RedisResult<StreamPendingReply> = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await;
        assert!(pending.is_err());
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1c_zero_intent_rejects_externally_recreated_group_frontier() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("zero-intent-recreated-group");
        let (owner, key, fresh, identity) = first_boot(&parent);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
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
        let Stage8bP1RedisSemanticOutcome::Ready { owner, .. } = outcome else {
            panic!("fixture must commit zero-intent S1");
        };
        drop(owner);

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let removed: usize = redis::cmd("XGROUP")
            .arg("DESTROY")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(removed, 1);
        let _: () = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .arg("0-0")
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
        let Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(pending) = restart else {
            panic!("durable zero-intent S1 must retain ACK-only authority");
        };
        let transport =
            attach_stage8b_p1_redis(&redis.url, Stage8bP1RedisConfig::paper_default_auto())
                .await
                .unwrap();
        assert!(matches!(
            resolve_stage8b_p1_zero_intent_ack_with_redis(*pending, transport).await,
            Err(Stage8bP1RedisSemanticError::ExactSourceConflict)
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1c_restart_attach_rejects_deleted_command_group_without_recreation() {
        let redis = RedisServer::start().await;
        let transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        drop(transport);
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
            attach_stage8b_p1_redis(&redis.url, Stage8bP1RedisConfig::paper_default_auto()).await,
            Err(Stage8bP1RedisSemanticError::GroupMissing)
        ));
        let pending: redis::RedisResult<StreamPendingReply> = redis::cmd("XPENDING")
            .arg(&namespace.canonical_command_stream)
            .arg(&namespace.stage7b_command_consumer_group)
            .query_async(&mut connection)
            .await;
        assert!(pending.is_err());
    }

    #[tokio::test]
    async fn p1c_command_response_loss_republishes_exactly_once_and_retains_m10() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("command-response-loss");
        let (owner, key, fresh, identity) = first_boot(&parent);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
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
        let _decision_binding = published.p1d1_command_decision_binding().unwrap();
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
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
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
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
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
        let second_delivery = pending.transport.backend.read_next_fresh().await.unwrap();
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
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        assert!(matches!(
            resume_stage8b_p1_prepublication_with_redis(*durable, transport).await,
            Err(Stage8bP1RedisSemanticError::ExactPendingEntryMissing)
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1c_ready_restart_reclaims_stale_a_before_fresh_b() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("ready-reclaim-before-fresh");
        let (owner, key, fresh, identity) = first_boot(&parent);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let a = canonical_m10(identity.clone(), 1_785_759_000_000, 2_600);
        let b = canonical_m10(identity.clone(), 1_785_759_600_000, 2_601);
        transport
            .publish_canonical_m10(&a, &identity)
            .await
            .unwrap();
        transport
            .publish_canonical_m10(&b, &identity)
            .await
            .unwrap();
        let delivered_a = transport.backend.read_next_fresh().await.unwrap();
        assert_eq!(delivered_a.redis_id(), "1785759000000-0");
        drop(transport);
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
        let Stage7bRestartOutcome::Ready(owner) = restart else {
            panic!("pre-semantic crash must restart as ordinary Ready");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let first = Stage8bP1RedisSemanticCompositionOwner::new(*owner, transport)
            .process_next(&key)
            .await
            .unwrap();
        let Stage8bP1RedisSemanticOutcome::Ready {
            owner,
            receipt,
            ack_disposition,
        } = first
        else {
            panic!("A fixture must be the zero-intent transition");
        };
        assert_eq!(receipt.evidence.m10_redis_id, "1785759000000-0");
        assert_eq!(
            ack_disposition,
            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
        );
        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0, "B must still be undelivered after A");

        let second = owner.process_next(&key).await.unwrap();
        match second {
            Stage8bP1RedisSemanticOutcome::Ready { receipt, .. } => {
                assert_eq!(receipt.evidence.m10_redis_id, "1785759600000-0");
            }
            Stage8bP1RedisSemanticOutcome::Prepublication(pending) => {
                assert_eq!(pending.pending_m10_redis_id(), "1785759600000-0");
            }
            Stage8bP1RedisSemanticOutcome::PendingNotClaimable { .. }
            | Stage8bP1RedisSemanticOutcome::MultiIntentBlocked { .. } => {
                panic!("B must be the next semantic source")
            }
        }
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1c_ready_with_unclaimable_stale_pel_never_reads_fresh() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("ready-unclaimable");
        let (owner, key, fresh, identity) = first_boot(&parent);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let a = canonical_m10(identity.clone(), 1_785_759_000_000, 2_600);
        let b = canonical_m10(identity.clone(), 1_785_759_600_000, 2_601);
        transport
            .publish_canonical_m10(&a, &identity)
            .await
            .unwrap();
        transport
            .publish_canonical_m10(&b, &identity)
            .await
            .unwrap();
        let delivered_a = transport.backend.read_next_fresh().await.unwrap();
        drop(transport);
        drop(owner);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::Ready(owner) = restart else {
            panic!("pre-semantic crash must restart as ordinary Ready");
        };
        let mut not_claimable = Stage8bP1RedisConfig::paper_default_auto();
        not_claimable.claim_idle_ms = 60_000;
        let transport = attach_stage8b_p1_redis(&redis.url, not_claimable)
            .await
            .unwrap();
        let outcome = Stage8bP1RedisSemanticCompositionOwner::new(*owner, transport)
            .process_next(&key)
            .await
            .unwrap();
        let Stage8bP1RedisSemanticOutcome::PendingNotClaimable {
            owner,
            pending_m10_redis_id,
        } = outcome
        else {
            panic!("young stale PEL must return retryable ownership");
        };
        assert_eq!(pending_m10_redis_id, delivered_a.redis_id());
        drop(owner);

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .arg("-")
            .arg("+")
            .arg(2)
            .query_async::<StreamPendingCountReply>(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.ids.len(), 1);
        assert_eq!(pending.ids[0].id, delivered_a.redis_id());
        assert!(matches!(
            restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh,
            )
            .unwrap(),
            Stage7bRestartOutcome::Ready(_)
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1c_ready_restart_rejects_ambiguous_pel_without_callback() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("ready-ambiguous-pel");
        let (owner, key, fresh, identity) = first_boot(&parent);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let a = canonical_m10(identity.clone(), 1_785_759_000_000, 2_600);
        let b = canonical_m10(identity.clone(), 1_785_759_600_000, 2_601);
        transport
            .publish_canonical_m10(&a, &identity)
            .await
            .unwrap();
        transport
            .publish_canonical_m10(&b, &identity)
            .await
            .unwrap();
        transport.backend.read_next_fresh().await.unwrap();
        transport.backend.read_next_fresh().await.unwrap();
        drop(transport);
        drop(owner);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::Ready(owner) = restart else {
            panic!("pre-semantic crash must restart as ordinary Ready");
        };
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        assert!(matches!(
            Stage8bP1RedisSemanticCompositionOwner::new(*owner, transport)
                .process_next(&key)
                .await,
            Err(Stage8bP1RedisSemanticError::AmbiguousReadyPendingEntries)
        ));
        assert!(matches!(
            restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh,
            )
            .unwrap(),
            Stage7bRestartOutcome::Ready(_)
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
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let bytes = canonical_m10(identity.clone(), 1_785_759_000_000, 2_650);
        transport
            .publish_canonical_m10(&bytes, &identity)
            .await
            .unwrap();
        let delivery = transport.backend.read_next_fresh().await.unwrap();
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
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
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

    #[tokio::test]
    async fn p1d2_market_feedback_commits_ack_then_truth_then_xacks_source() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1d2-market-feedback");
        let (mut pending, key, fresh, identity) = one_intent_pending(&redis, &parent).await;
        let predecessor_close = 1_785_759_000_000_i64;
        let successor = canonical_m10(identity.clone(), predecessor_close + 600_000, 2_175);
        pending
            .transport
            .publish_canonical_m10(&successor, &identity)
            .await
            .unwrap();
        let published = pending.publish_exact_command().await.unwrap();
        let pre_ack_generation = published.receipt().covering_seal_generation;
        let ack = published
            .execute_next_canonical_market(p1d2_test_schedule_authority(), &key)
            .await
            .unwrap();
        assert_eq!(ack.recovery_seal_generation(), pre_ack_generation + 1);
        assert_eq!(ack.recovery_seal_commitment_sha256().len(), 64);
        assert_eq!(ack.pending_m10_redis_id(), "1785759000000-0");
        assert!(!ack.m10_xack_allowed());
        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending_before_truth: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_before_truth.count(), 1);

        drop(ack);
        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::P1d2AckCommitted(ack) = restart else {
            panic!("durable S_ack must restart as truth-only authority");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let ack = resume_stage8b_p1d2_ack_with_redis(*ack, transport)
            .await
            .unwrap();
        let ack_generation = ack.recovery_seal_generation();
        let truth = ack.commit_truth(&key).unwrap();
        assert_eq!(truth.recovery_seal_generation(), ack_generation + 1);
        assert_eq!(truth.recovery_seal_commitment_sha256().len(), 64);
        assert_eq!(truth.pending_m10_redis_id(), "1785759000000-0");
        assert!(truth.m10_xack_allowed());
        let pending_before_xack: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_before_xack.count(), 1);

        let audit_before_restart = truth.audit_evidence().unwrap();

        drop(truth);
        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::P1d2TruthCommitted(truth) = restart else {
            panic!("durable S_truth must restart as source-resolution-only authority");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let truth = resume_stage8b_p1d2_truth_with_redis(*truth, transport)
            .await
            .unwrap();
        assert_eq!(truth.audit_evidence().unwrap(), audit_before_restart);
        let resolved = truth.acknowledge_source().await.unwrap();
        assert_eq!(
            resolved.disposition(),
            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
        );
        let audit = resolved.audit_evidence();
        assert_eq!(audit.schema_version, 1);
        assert_eq!(
            audit.core.seq_ack.checked_add(1),
            Some(audit.core.seq_truth)
        );
        assert!(audit.post_feedback_seal_generation > 0);
        assert_eq!(audit.post_feedback_seal_commitment_sha256.len(), 64);
        assert_eq!(audit.audit_sha256.len(), 64);
        assert_eq!(audit, &audit_before_restart);
        let redacted = serde_json::to_string(audit).unwrap();
        assert!(!redacted.contains("hmac"));
        assert!(!redacted.contains("secret"));
        drop(resolved);
        let pending_after_xack: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_after_xack.count(), 0);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::P1d2TruthCommitted(truth) = restart else {
            panic!("S_truth remains the local restart authority after source XACK");
        };
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let truth = resume_stage8b_p1d2_truth_with_redis(*truth, transport)
            .await
            .unwrap();
        let resolved = truth.acknowledge_source().await.unwrap();
        assert_eq!(
            resolved.disposition(),
            Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged
        );
        drop(resolved);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1d3_read_only_successor_observation_retains_original_source_until_xack() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1d3-read-only-successor");
        let (_, _, _, identity) = first_boot(&parent);
        let predecessor_close = 1_785_759_000_000_i64;
        let successor_close = predecessor_close + 600_000;
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        for (close, price) in [(predecessor_close, 2_210), (successor_close, 2_220)] {
            transport
                .publish_canonical_m10(&canonical_m10(identity.clone(), close, price), &identity)
                .await
                .unwrap();
        }

        let source = transport.backend.read_next_fresh().await.unwrap();
        assert_eq!(source.redis_id(), format!("{predecessor_close}-0"));
        let pending_before = transport
            .backend
            .pending_entries("-", "+", 2)
            .await
            .unwrap();
        assert_eq!(pending_before.ids.len(), 1);
        assert_eq!(pending_before.ids[0].id, source.redis_id());

        let successor = transport
            .backend
            .exact_first_successor_m10(source.redis_id(), &identity)
            .await
            .unwrap();
        assert_eq!(successor.redis_id(), format!("{successor_close}-0"));

        // Candidate-bar observation is XRANGE-only: it must neither consume the
        // successor nor acknowledge the decision bar. P1-d3 can XACK the source
        // only after its replacement package has been persisted and reread.
        let pending_after_observation = transport
            .backend
            .pending_entries("-", "+", 2)
            .await
            .unwrap();
        assert_eq!(pending_after_observation.ids.len(), 1);
        assert_eq!(pending_after_observation.ids[0].id, source.redis_id());

        assert_eq!(
            transport.backend.acknowledge_exact(&source).await.unwrap(),
            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
        );
        assert!(transport
            .backend
            .pending_entries("-", "+", 2)
            .await
            .unwrap()
            .ids
            .is_empty());

        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1d3_actual_host_optional_tcid_completes_recovered_cancel_and_xacks_last() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1d3-host-optional-tcid");
        let (key, _, _, published) = prepare_p1d3_terminal_cancel_source(&redis.url, &parent).await;
        let BrokerCommand::CancelOrder(cancel) = &published.command else {
            panic!("fixture must publish the actual Stage 5C host CANCEL")
        };
        assert!(cancel.client_order_id.is_none());

        let outcome = published
            .execute_next_canonical_cancel(
                p1d3_cancel_schedule(P1D3_PLACE_DECISION_CLOSE_MS, P1D3_CANCEL_CANDIDATE_CLOSE_MS),
                &key,
            )
            .await
            .unwrap();
        let truth = match outcome {
            Stage8bP1RedisCancelCommitOutcome::AckCommitted(ack) => ack.commit_truth(&key).unwrap(),
            Stage8bP1RedisCancelCommitOutcome::TruthCommitted(truth) => truth,
            Stage8bP1RedisCancelCommitOutcome::CancelContinuationPending(pending) => {
                pending.commit_recovered_cancel(&key).unwrap()
            }
        };
        assert!(truth.m10_xack_allowed());

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending_before_xack: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_before_xack.count(), 1);
        let resolved = truth.acknowledge_source().await.unwrap();
        assert_eq!(
            resolved.disposition(),
            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
        );
        drop(resolved);
        let pending_after_xack: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_after_xack.count(), 0);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1d3_colliding_cancel_dcid_fails_before_dispatch_for_optional_or_exact_tcid() {
        let place_request_id = StrategyRequestId::from(
            Uuid::parse_str("00000000-0000-0000-0000-00000000d301").unwrap(),
        );
        let cancel_request_id = StrategyRequestId::from(
            Uuid::parse_str("00000000-0000-0000-0000-00000000d302").unwrap(),
        );
        assert_ne!(place_request_id, cancel_request_id);
        assert_eq!(
            ClientOrderId::from_strategy_request(place_request_id),
            ClientOrderId::from_strategy_request(cancel_request_id)
        );

        for supply_target_client_order_id in [false, true] {
            let redis = RedisServer::start().await;
            let parent = temp_directory(if supply_target_client_order_id {
                "p1d3-collision-exact-tcid"
            } else {
                "p1d3-collision-optional-tcid"
            });
            let (key, fresh, _, published) = prepare_p1d3_terminal_cancel_source_with_ids(
                &redis.url,
                &parent,
                place_request_id,
                cancel_request_id,
                supply_target_client_order_id,
            )
            .await;
            let BrokerCommand::CancelOrder(cancel) = &published.command else {
                panic!("collision fixture must publish CANCEL")
            };
            assert_eq!(
                cancel.client_order_id.is_some(),
                supply_target_client_order_id
            );
            let accepted_frames = published
                .stage7
                .recovered()
                .unwrap()
                .journal_frontier()
                .frame_count();
            let accepted_seal_generation =
                published.stage7.committed_seal().unwrap().seal_generation();

            assert!(matches!(
                published
                    .execute_next_canonical_cancel(
                        p1d3_cancel_schedule(
                            P1D3_PLACE_DECISION_CLOSE_MS,
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS,
                        ),
                        &key,
                    )
                    .await,
                Err(Stage8bP1RedisSemanticError::Durable(
                    Stage7bRecoveryError::Runtime(
                        strategy_runtime_core::Stage6dLiveCoreError::DurableOrderingViolation
                    )
                ))
            ));

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
            let ready = match restart {
                Stage7bRestartOutcome::Ready(ready) => *ready,
                Stage7bRestartOutcome::P1SemanticPrepublicationReady(prepublication) => {
                    let (ready, evidence, command) = prepublication.into_p1c_parts();
                    assert_eq!(evidence.strategy_request_id, Some(cancel_request_id));
                    assert!(matches!(command, BrokerCommand::CancelOrder(_)));
                    ready
                }
                Stage7bRestartOutcome::P1SemanticPrepublicationPending(_) => {
                    panic!("collision restarted as uncovered RequestAccepted")
                }
                Stage7bRestartOutcome::P1d3SemanticPending(_) => {
                    panic!("collision restarted as P1-d3 semantic pending")
                }
                Stage7bRestartOutcome::Blocked(_) => panic!("collision restart was blocked"),
                _ => panic!("collision restarted with downstream lifecycle authority"),
            };
            assert_eq!(
                ready.committed_seal().unwrap().seal_generation(),
                accepted_seal_generation
            );
            let recovered = ready.recovered().unwrap();
            assert_eq!(recovered.journal_frontier().frame_count(), accepted_frames);
            let cancel_request = recovered.replay().request(cancel_request_id).unwrap();
            assert_eq!(cancel_request.dispatch_attempt_count(), 0);
            assert_eq!(
                cancel_request.dispatch_safety_state(),
                strategy_runtime_core::Stage6DispatchSafetyStateV1::ReadyForFirstDispatch
            );
            assert!(cancel_request.cancel_outcome().is_none());
            assert!(cancel_request.final_disposition().is_none());
            assert!(!cancel_request.conflict_observed());

            let namespace = stage8b_p1_redis_namespace();
            let mut connection = redis.connection().await;
            let source_pending: StreamPendingReply = redis::cmd("XPENDING")
                .arg(&namespace.canonical_m10_stream)
                .arg(&namespace.m10_consumer_group)
                .query_async(&mut connection)
                .await
                .unwrap();
            let command_count: usize = redis::cmd("XLEN")
                .arg(&namespace.canonical_command_stream)
                .query_async(&mut connection)
                .await
                .unwrap();
            assert_eq!(source_pending.count(), 1);
            assert_eq!(command_count, 1);
            drop(ready);
            fs::remove_dir_all(parent).unwrap();
        }
    }

    #[tokio::test]
    async fn p1d3_optional_tcid_target_first_restart_continues_without_duplicate_effects() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1d3-optional-tcid-target-first-restart");
        let (key, fresh, _, published) =
            prepare_p1d3_working_target_first_cancel_source(&redis.url, &parent).await;
        let BrokerCommand::CancelOrder(cancel) = &published.command else {
            panic!("target-first fixture must publish CANCEL")
        };
        assert!(cancel.client_order_id.is_none());

        let outcome = published
            .execute_next_canonical_cancel(
                p1d3_cancel_schedule(
                    P1D3_CANCEL_DECISION_CLOSE_MS,
                    P1D3_CANCEL_CANDIDATE_CLOSE_MS,
                ),
                &key,
            )
            .await
            .unwrap();
        let Stage8bP1RedisCancelCommitOutcome::CancelContinuationPending(pending) = outcome else {
            panic!("target fill must win before recovered CANCEL settlement")
        };
        assert!(!pending.market_or_schedule_input_allowed());
        assert!(!pending.m10_xack_allowed());
        let before_restart = pending.stage8b_p1d3_test_restart_snapshot();
        drop(pending);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::P1d3CancelContinuationPending(pending) = restart else {
            panic!("target-first restart must expose only recovered-cancel continuation")
        };
        assert_eq!(
            pending.stage8b_p1d3_test_restart_snapshot(),
            before_restart,
            "restart must not repeat provider, target truth, ACK or callback"
        );
        assert!(!pending.market_or_schedule_input_allowed());
        assert!(!pending.source_xack_allowed());

        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let truth = resume_stage8b_p1d3_cancel_continuation_with_redis(*pending, transport, &key)
            .await
            .unwrap();
        let after_recovered_cancel = truth.stage8b_p1d3_test_restart_snapshot();
        assert_eq!(after_recovered_cancel.0, before_restart.0 + 1);
        assert_eq!(after_recovered_cancel.2, before_restart.2);
        assert!(truth.m10_xack_allowed());

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending_before_xack: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        let command_count: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_before_xack.count(), 1);
        assert_eq!(command_count, 1);

        let resolved = truth.acknowledge_source().await.unwrap();
        assert_eq!(
            resolved.disposition(),
            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
        );
        drop(resolved);
        let pending_after_xack: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_after_xack.count(), 0);

        let final_restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        assert!(matches!(
            final_restart,
            Stage7bRestartOutcome::P1d3TruthCommitted(_)
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1d3_subprocess_sigkill_brackets_s_cancel_recovered() {
        for (phase, expected) in [
            (
                "p1d3-after-recovered-cancel-before-s-cancel-recovered",
                P1d3CancelExpectedRestart::PreRecoveredSeal,
            ),
            (
                "p1d3-after-s-cancel-recovered-before-source-xack",
                P1d3CancelExpectedRestart::Truth,
            ),
        ] {
            run_p1d3_cancel_recovery_crash_case(phase, expected).await;
        }
    }

    #[tokio::test]
    async fn p1d4_initial_dispatch_only_frontiers_reuse_one_dispatch_and_xack_last() {
        for (scenario, scenario_id, frontier_id, expiry) in [
            ("initial-working", "S01", "F20", false),
            ("initial-working", "S01", "F02", false),
            ("initial-filled", "S02", "F20", false),
            ("initial-filled", "S02", "F02", false),
            ("initial-expired", "S03", "F20", true),
            ("initial-expired", "S03", "F02", true),
        ] {
            let redis = RedisServer::start().await;
            let parent = temp_directory(&format!("p1d4-{scenario}-{frontier_id}"));
            let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
            spawn_p1d4_exact_frontier(&redis, &parent, scenario, scenario_id, frontier_id).await;

            let restart = restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh.clone(),
            )
            .unwrap();
            let Stage7bRestartOutcome::P1d3DispatchPending(pending) = restart else {
                panic!("{scenario}/{frontier_id} must recover as P1d3DispatchPending")
            };
            assert!(pending.is_limit_place());
            assert!(!pending.is_cancel());
            assert!(!pending.second_dispatch_allowed());
            assert!(!pending.source_xack_allowed());
            let request_id = pending.strategy_request_id();
            let dispatch_id = pending.dispatch_record_id().to_string();
            let dispatch_sequence = pending.dispatch_sequence();

            tokio::time::sleep(Duration::from_millis(5)).await;
            let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                .await
                .unwrap();
            let ack = if expiry {
                let trading_day = Utc
                    .timestamp_millis_opt(P1D3_PLACE_DECISION_CLOSE_MS)
                    .single()
                    .unwrap()
                    .date_naive()
                    .to_string();
                resume_stage8b_p1d3_dispatch_expiry_with_redis(
                    *pending,
                    transport,
                    stage8b_p1d3_test_expiry_authority(
                        "44".repeat(32),
                        trading_day,
                        format!("{P1D3_PLACE_DECISION_CLOSE_MS}-0"),
                        P1D3_CANCEL_DECISION_CLOSE_MS,
                    ),
                    &key,
                )
                .await
                .unwrap()
            } else {
                resume_stage8b_p1d3_dispatch_limit_with_redis(
                    *pending,
                    transport,
                    p1d3_cancel_schedule(
                        P1D3_PLACE_DECISION_CLOSE_MS,
                        P1D3_CANCEL_DECISION_CLOSE_MS,
                    ),
                    &key,
                )
                .await
                .unwrap()
            };
            assert!(!ack.m10_xack_allowed());
            let truth = ack.commit_truth(&key).unwrap();
            assert!(truth.m10_xack_allowed());

            let namespace = stage8b_p1_redis_namespace();
            let mut connection = redis.connection().await;
            let command_count: usize = redis::cmd("XLEN")
                .arg(&namespace.canonical_command_stream)
                .query_async(&mut connection)
                .await
                .unwrap();
            let pending_before_xack: StreamPendingReply = redis::cmd("XPENDING")
                .arg(&namespace.canonical_m10_stream)
                .arg(&namespace.m10_consumer_group)
                .query_async(&mut connection)
                .await
                .unwrap();
            assert_eq!(command_count, 1, "{scenario}/{frontier_id}");
            assert_eq!(pending_before_xack.count(), 1, "{scenario}/{frontier_id}");
            let resolved = truth.acknowledge_source().await.unwrap();
            assert_eq!(
                resolved.disposition(),
                Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
            );
            drop(resolved);

            let final_restart = restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh,
            )
            .unwrap();
            let Stage7bRestartOutcome::P1d3TruthCommitted(truth) = final_restart else {
                panic!("{scenario}/{frontier_id} must finish at P1d3TruthCommitted")
            };
            assert!(truth.source_xack_allowed());
            let (dispatch_count, last_record_id, last_sequence) = truth
                .stage8b_p1d4_test_request_audit(request_id)
                .expect("recovered LIMIT request must remain in replay");
            assert_eq!(dispatch_count, 1);
            assert_ne!(last_record_id, dispatch_id);
            assert!(last_sequence > dispatch_sequence);
            fs::remove_dir_all(parent).unwrap();
        }
    }

    #[tokio::test]
    async fn p1d4_later_scenario_hooks_are_reached_by_production_composition() {
        for (scenario, scenario_id, frontier_id) in [
            ("later-untouched-zero", "S04", "F12"),
            ("later-untouched-zero", "S04", "F15"),
            ("later-untouched-zero", "S04", "F16"),
            ("later-untouched-one", "S05", "F15"),
            ("later-untouched-one", "S05", "F16"),
            ("later-filled", "S06", "F02"),
            ("later-filled", "S06", "F15"),
            ("later-filled", "S06", "F16"),
            ("later-expired", "S07", "F17"),
            ("later-expired", "S07", "F19"),
        ] {
            let redis = RedisServer::start().await;
            let parent = temp_directory(&format!("p1d4-{scenario}-{frontier_id}"));
            spawn_p1d4_exact_frontier(&redis, &parent, scenario, scenario_id, frontier_id).await;
            fs::remove_dir_all(parent).unwrap();
        }
    }

    #[tokio::test]
    #[ignore]
    async fn p1d4_probe_later_fill_callback_outcomes() {
        for (filled_exit, working_limit_price, prices) in [
            (
                false,
                None,
                &[2_100, 2_175, 2_220, 2_300, 2_650, 2_700, 3_000][..],
            ),
            (true, None, &[2_230, 2_240, 2_300, 2_650][..]),
            (
                true,
                Some(3_000),
                &[2_100, 2_175, 2_220, 2_300, 2_650, 3_000][..],
            ),
        ] {
            for price in prices {
                let redis = RedisServer::start().await;
                let parent = temp_directory(&format!("p1d4-probe-{filled_exit}-{price}"));
                let (key, _, identity, mut owner) = prepare_p1d4_later_working_owner(
                    &redis.url,
                    &parent,
                    filled_exit,
                    false,
                    working_limit_price,
                )
                .await;
                owner
                    .transport
                    .publish_canonical_m10(
                        &canonical_m10(
                            identity.clone(),
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000,
                            *price,
                        ),
                        &identity,
                    )
                    .await
                    .unwrap();
                let result = owner
                    .process_next_working_limit(
                        p1d3_cancel_schedule(
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS,
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000,
                        ),
                        &key,
                    )
                    .await;
                let label = match result {
                    Ok(Stage8bP1RedisSemanticOutcome::Ready { .. }) => "ready",
                    Ok(Stage8bP1RedisSemanticOutcome::Prepublication(_)) => "prepublication",
                    Ok(Stage8bP1RedisSemanticOutcome::PendingNotClaimable { .. }) => "pending",
                    Ok(Stage8bP1RedisSemanticOutcome::MultiIntentBlocked { .. }) => "multi",
                    Err(_) => "error",
                };
                eprintln!(
                    "P1-d4 fill probe exit={filled_exit} limit={working_limit_price:?} price={price} outcome={label}"
                );
                fs::remove_dir_all(parent).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn p1d4_exact_registry_cells_sigkill_with_duplicate_and_conflict_variants() {
        let all_cells = p1d4_registry_cells();
        assert_eq!(all_cells.len(), 92);
        let filter = std::env::var("STAGE8B_P1D4_DEBUG_CELL").ok();
        let cells = all_cells
            .into_iter()
            .filter(|cell| filter.as_deref().is_none_or(|value| cell.cell_id >= value))
            .collect::<Vec<_>>();
        for cell in cells {
            let redis = RedisServer::start().await;
            let parent = temp_directory(&format!("p1d4-exact-{}", cell.cell_id));
            let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
            spawn_p1d4_exact_frontier(
                &redis,
                &parent,
                p1d4_scenario_name(cell.scenario_id),
                cell.scenario_id,
                cell.frontier_id,
            )
            .await;

            let wrong_key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x6b; 32])
                .expect("one-field conflict commitment key");
            let conflict = restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &wrong_key,
                fresh.clone(),
            );
            assert!(
                matches!(&conflict, Err(_) | Ok(Stage7bRestartOutcome::Blocked(_))),
                "{} must reject the one-field authority conflict",
                cell.cell_id
            );
            drop(conflict);
            let duplicate = restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh.clone(),
            )
            .unwrap();
            let duplicate_disposition = p1d4_restart_disposition(&duplicate);
            drop(duplicate);
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
            assert_eq!(
                p1d4_restart_disposition(&restart),
                duplicate_disposition,
                "{} byte-identical duplicate restart drifted",
                cell.cell_id
            );
            let actual = p1d4_effective_restart_disposition(
                restart,
                &redis,
                cell.expected_restart_disposition,
            )
            .await;
            assert_eq!(
                actual, cell.expected_restart_disposition,
                "{} {}/{}",
                cell.cell_id, cell.scenario_id, cell.frontier_id,
            );
            fs::remove_dir_all(parent).unwrap();
        }
    }

    #[tokio::test]
    async fn p1d4_generated_market_registry_extends_base_to_105_with_variants() {
        let generated_cells = p1d4_generated_market_registry_cells();
        assert_eq!(p1d4_registry_cells().len(), 92);
        assert_eq!(generated_cells.len(), 13);
        assert_eq!(p1d4_registry_cells().len() + generated_cells.len(), 105);

        for cell in generated_cells {
            let redis = RedisServer::start().await;
            let parent = temp_directory(&format!("p1d4-generated-{}", cell.cell_id));
            let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
            spawn_p1d4_generated_market_frontier(&redis, &parent, &cell).await;

            let wrong_key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x6b; 32])
                .expect("one-field conflict commitment key");
            let conflict = restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &wrong_key,
                fresh.clone(),
            );
            assert!(
                matches!(&conflict, Err(_) | Ok(Stage7bRestartOutcome::Blocked(_))),
                "{} must reject the one-field authority conflict",
                cell.cell_id
            );
            drop(conflict);
            let duplicate = restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh.clone(),
            )
            .unwrap();
            assert_eq!(
                p1d4_restart_disposition(&duplicate),
                cell.expected_restart_disposition,
                "{} byte-identical duplicate restart drifted",
                cell.cell_id
            );
            drop(duplicate);
            let restart = restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh.clone(),
            )
            .unwrap();
            assert_eq!(
                p1d4_restart_disposition(&restart),
                cell.expected_restart_disposition,
                "{} must route through its authenticated composite owner",
                cell.cell_id
            );

            tokio::time::sleep(Duration::from_millis(5)).await;
            let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                .await
                .unwrap();
            let ack = match restart {
                Stage7bRestartOutcome::P1d4GeneratedMarketPrepublicationPending(owner) => {
                    let decision_close_ms = P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000;
                    resume_stage8b_p1d4_prepublication_with_redis(*owner, transport)
                        .await
                        .unwrap()
                        .execute_next_canonical_generated_market(
                            strategy_runtime_core::stage8b_p1d1_test_schedule_authority(
                                super::super::p1_instrument(),
                                decision_close_ms,
                                decision_close_ms + 600_000,
                            ),
                            &key,
                        )
                        .await
                        .unwrap()
                }
                Stage7bRestartOutcome::P1d4GeneratedMarketDispatchPending(owner) => {
                    resume_stage8b_p1d4_dispatch_pending_with_redis(*owner, transport, &key)
                        .await
                        .unwrap()
                }
                Stage7bRestartOutcome::P1d4GeneratedMarketOrderPending(owner) => {
                    resume_stage8b_p1d4_order_pending_with_redis(*owner, transport, &key)
                        .await
                        .unwrap()
                }
                Stage7bRestartOutcome::P1d4GeneratedMarketPreFinalizationPending(owner) => {
                    resume_stage8b_p1d4_pre_finalization_with_redis(*owner, transport, &key)
                        .await
                        .unwrap()
                }
                Stage7bRestartOutcome::P1d4GeneratedMarketPreAckPending(owner) => {
                    resume_stage8b_p1d4_pre_ack_with_redis(*owner, transport, &key)
                        .await
                        .unwrap()
                }
                Stage7bRestartOutcome::P1d4GeneratedMarketAckCommitted(owner) => {
                    resume_stage8b_p1d4_ack_with_redis(*owner, transport)
                        .await
                        .unwrap()
                }
                Stage7bRestartOutcome::P1d4GeneratedMarketTruthCommitted(owner) => {
                    let truth = resume_stage8b_p1d4_truth_with_redis(*owner, transport)
                        .await
                        .unwrap();
                    let namespace = stage8b_p1_redis_namespace();
                    let mut connection = redis.connection().await;
                    let command_count: usize = redis::cmd("XLEN")
                        .arg(&namespace.canonical_command_stream)
                        .query_async(&mut connection)
                        .await
                        .unwrap();
                    assert_eq!(command_count, 1, "{}", cell.cell_id);
                    let resolved = truth.acknowledge_source().await.unwrap();
                    assert_eq!(
                        resolved.disposition(),
                        Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
                    );
                    drop(resolved);
                    fs::remove_dir_all(parent).unwrap();
                    continue;
                }
                other => panic!(
                    "{} routed to unexpected {}",
                    cell.cell_id,
                    p1d4_restart_disposition(&other)
                ),
            };
            let truth = ack.commit_truth(&key).await.unwrap();
            assert!(truth.m10_xack_allowed());

            let namespace = stage8b_p1_redis_namespace();
            let mut connection = redis.connection().await;
            let command_count: usize = redis::cmd("XLEN")
                .arg(&namespace.canonical_command_stream)
                .query_async(&mut connection)
                .await
                .unwrap();
            let pending_before: StreamPendingReply = redis::cmd("XPENDING")
                .arg(&namespace.canonical_m10_stream)
                .arg(&namespace.m10_consumer_group)
                .query_async(&mut connection)
                .await
                .unwrap();
            assert_eq!(command_count, 1, "{}", cell.cell_id);
            assert_eq!(pending_before.count(), 1, "{}", cell.cell_id);
            let resolved = truth.acknowledge_source().await.unwrap();
            assert_eq!(
                resolved.disposition(),
                Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
            );
            drop(resolved);
            let pending_after: StreamPendingReply = redis::cmd("XPENDING")
                .arg(&namespace.canonical_m10_stream)
                .arg(&namespace.m10_consumer_group)
                .query_async(&mut connection)
                .await
                .unwrap();
            assert_eq!(pending_after.count(), 0, "{}", cell.cell_id);
            fs::remove_dir_all(parent).unwrap();
        }
    }

    #[tokio::test]
    async fn p1d4_simple_cancel_dispatch_only_frontiers_reuse_one_dispatch_and_xack_last() {
        for (scenario, scenario_id) in [
            ("cancel-canceled", "S08"),
            ("cancel-filled", "S10"),
            ("cancel-expired", "S11"),
        ] {
            for frontier_id in ["F20", "F02"] {
                let redis = RedisServer::start().await;
                let parent = temp_directory(&format!("p1d4-{scenario}-{frontier_id}"));
                let (_, _, key, fresh) =
                    strategy_runtime_core::stage8b_p1_test_first_boot_material();
                spawn_p1d4_exact_frontier(&redis, &parent, scenario, scenario_id, frontier_id)
                    .await;

                let restart = restart_stage8b_p1(
                    validate_stage8b_p1_bootstrap_config(bootstrap_config(
                        parent.clone(),
                        fresh.stage5c_config_fingerprint(),
                    ))
                    .unwrap(),
                    &key,
                    fresh.clone(),
                )
                .unwrap();
                let Stage7bRestartOutcome::P1d3DispatchPending(pending) = restart else {
                    panic!("{scenario}/{frontier_id} must recover as P1d3DispatchPending")
                };
                assert!(pending.is_cancel());
                assert!(!pending.second_dispatch_allowed());
                let request_id = pending.strategy_request_id();
                let dispatch_id = pending.dispatch_record_id().to_string();
                let dispatch_sequence = pending.dispatch_sequence();

                tokio::time::sleep(Duration::from_millis(5)).await;
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                let schedule_source_close_ms = if scenario_id == "S11" {
                    P1D3_PLACE_DECISION_CLOSE_MS
                } else {
                    P1D3_CANCEL_DECISION_CLOSE_MS
                };
                let outcome = resume_stage8b_p1d3_dispatch_cancel_with_redis(
                    *pending,
                    transport,
                    p1d3_cancel_schedule(schedule_source_close_ms, P1D3_CANCEL_CANDIDATE_CLOSE_MS),
                    &key,
                )
                .await
                .unwrap();
                let truth = match outcome {
                    Stage8bP1RedisCancelCommitOutcome::AckCommitted(ack) => {
                        assert_eq!(scenario_id, "S08");
                        ack.commit_truth(&key).unwrap()
                    }
                    Stage8bP1RedisCancelCommitOutcome::TruthCommitted(truth) => {
                        assert!(matches!(scenario_id, "S10" | "S11"));
                        truth
                    }
                    Stage8bP1RedisCancelCommitOutcome::CancelContinuationPending(_) => {
                        panic!("{scenario}/{frontier_id} unexpectedly required target-first flow")
                    }
                };
                assert!(truth.m10_xack_allowed());

                let namespace = stage8b_p1_redis_namespace();
                let mut connection = redis.connection().await;
                let command_count: usize = redis::cmd("XLEN")
                    .arg(&namespace.canonical_command_stream)
                    .query_async(&mut connection)
                    .await
                    .unwrap();
                assert_eq!(command_count, 1, "{scenario}/{frontier_id}");
                let resolved = truth.acknowledge_source().await.unwrap();
                assert_eq!(
                    resolved.disposition(),
                    Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
                );
                drop(resolved);

                let final_restart = restart_stage8b_p1(
                    validate_stage8b_p1_bootstrap_config(bootstrap_config(
                        parent.clone(),
                        fresh.stage5c_config_fingerprint(),
                    ))
                    .unwrap(),
                    &key,
                    fresh,
                )
                .unwrap();
                let Stage7bRestartOutcome::P1d3TruthCommitted(truth) = final_restart else {
                    panic!("{scenario}/{frontier_id} must finish at P1d3TruthCommitted")
                };
                let (dispatch_count, last_record_id, last_sequence) = truth
                    .stage8b_p1d4_test_request_audit(request_id)
                    .expect("recovered CANCEL request must remain in replay");
                assert_eq!(dispatch_count, 1);
                assert_ne!(last_record_id, dispatch_id);
                assert!(last_sequence > dispatch_sequence);
                fs::remove_dir_all(parent).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn p1d4_s09_dispatch_only_frontiers_complete_two_stage_without_redispatch() {
        for frontier_id in ["F20", "F02"] {
            let redis = RedisServer::start().await;
            let parent = temp_directory(&format!("p1d4-s09-{frontier_id}"));
            let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
            spawn_p1d4_exact_frontier(&redis, &parent, "cancel-target-first", "S09", frontier_id)
                .await;

            let restart = restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh.clone(),
            )
            .unwrap();
            let Stage7bRestartOutcome::P1d3DispatchPending(pending) = restart else {
                panic!("S09/{frontier_id} must recover as P1d3DispatchPending")
            };
            assert!(pending.is_cancel());
            assert!(!pending.second_dispatch_allowed());
            let request_id = pending.strategy_request_id();
            let dispatch_id = pending.dispatch_record_id().to_string();
            let dispatch_sequence = pending.dispatch_sequence();

            tokio::time::sleep(Duration::from_millis(5)).await;
            let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                .await
                .unwrap();
            let outcome = resume_stage8b_p1d3_dispatch_cancel_with_redis(
                *pending,
                transport,
                p1d3_cancel_schedule(
                    P1D3_CANCEL_DECISION_CLOSE_MS,
                    P1D3_CANCEL_CANDIDATE_CLOSE_MS,
                ),
                &key,
            )
            .await
            .unwrap();
            let Stage8bP1RedisCancelCommitOutcome::CancelContinuationPending(continuation) =
                outcome
            else {
                panic!("S09/{frontier_id} must persist target truth before recovered CANCEL")
            };
            let target_snapshot = continuation.stage8b_p1d3_test_restart_snapshot();
            let truth = continuation.commit_recovered_cancel(&key).unwrap();
            let final_snapshot = truth.stage8b_p1d3_test_restart_snapshot();
            assert_eq!(final_snapshot.0, target_snapshot.0 + 1);
            assert!(final_snapshot.1 > target_snapshot.1);
            assert_eq!(final_snapshot.2, target_snapshot.2);

            let namespace = stage8b_p1_redis_namespace();
            let mut connection = redis.connection().await;
            let command_count: usize = redis::cmd("XLEN")
                .arg(&namespace.canonical_command_stream)
                .query_async(&mut connection)
                .await
                .unwrap();
            assert_eq!(command_count, 1, "S09/{frontier_id}");
            let resolved = truth.acknowledge_source().await.unwrap();
            assert_eq!(
                resolved.disposition(),
                Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
            );
            drop(resolved);

            let final_restart = restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh,
            )
            .unwrap();
            let Stage7bRestartOutcome::P1d3TruthCommitted(truth) = final_restart else {
                panic!("S09/{frontier_id} must finish at P1d3TruthCommitted")
            };
            let (dispatch_count, last_record_id, last_sequence) = truth
                .stage8b_p1d4_test_request_audit(request_id)
                .expect("recovered CANCEL request must remain in replay");
            assert_eq!(dispatch_count, 1);
            assert_ne!(last_record_id, dispatch_id);
            assert!(last_sequence > dispatch_sequence);
            fs::remove_dir_all(parent).unwrap();
        }
    }

    #[tokio::test]
    async fn p1d4_market_dispatch_frontier_is_not_intercepted_by_p1d3() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1d4-market-non-interception");
        let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
        spawn_p1d4_legacy_frontier(
            &redis,
            &parent,
            "market-predecessor",
            "p1d4-market-after-dispatch-before-provider-outcome",
        )
        .await;
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
        assert!(matches!(restart, Stage7bRestartOutcome::Blocked(_)));

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let command_count: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(command_count, 1);
        assert_eq!(pending.count(), 1);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1d4_malformed_limit_dispatch_suffix_never_mints_recovery_owner() {
        for case in [
            "non-day",
            "ttl",
            "fractional-qty",
            "zero-qty",
            "missing-price",
            "nonpositive-price",
        ] {
            let redis = RedisServer::start().await;
            let parent = temp_directory(&format!("p1d4-excluded-{case}"));
            let prepared = prepare_p1d3_initial_limit_source_with_mutation(
                &redis.url,
                &parent,
                2_220,
                |place| match case {
                    "non-day" => place.time_in_force = TimeInForce::GoodTillCancel,
                    "ttl" => place.ttl_ms = Some(60_000),
                    "fractional-qty" => place.qty = Decimal::new(15, 1),
                    "zero-qty" => place.qty = Decimal::ZERO,
                    "missing-price" => place.limit_price = None,
                    "nonpositive-price" => place.limit_price = Some(Decimal::ZERO),
                    _ => unreachable!("complete malformed LIMIT fixture inventory"),
                },
            )
            .await;

            if let Some((key, fresh, _, published)) = prepared {
                published
                    .stage8b_p1d4_test_append_dispatch_only(&key)
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
                assert!(
                    matches!(restart, Stage7bRestartOutcome::Blocked(_)),
                    "{case} must fail closed before P1d3DispatchPending"
                );
            }
            fs::remove_dir_all(parent).unwrap();
        }
    }

    #[tokio::test]
    async fn p1d2_missing_successor_fails_before_feedback_and_retains_source() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1d2-missing-successor");
        let (pending, key, fresh, _) = one_intent_pending(&redis, &parent).await;
        let published = pending.publish_exact_command().await.unwrap();
        assert!(matches!(
            published
                .execute_next_canonical_market(p1d2_test_schedule_authority(), &key)
                .await,
            Err(Stage8bP1RedisSemanticError::ExactPendingEntryMissing)
        ));

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 1);
        assert!(matches!(
            restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh,
            )
            .unwrap(),
            Stage7bRestartOutcome::P1SemanticPrepublicationReady(_)
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1d2_noncontiguous_successor_fails_before_feedback_and_retains_source() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1d2-noncontiguous-successor");
        let (mut pending, key, fresh, identity) = one_intent_pending(&redis, &parent).await;
        pending
            .transport
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), 1_785_760_200_000, 2_175),
                &identity,
            )
            .await
            .unwrap();
        let published = pending.publish_exact_command().await.unwrap();
        assert!(matches!(
            published
                .execute_next_canonical_market(p1d2_test_schedule_authority(), &key)
                .await,
            Err(Stage8bP1RedisSemanticError::ExactSourceConflict)
        ));

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 1);
        assert!(matches!(
            restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh,
            )
            .unwrap(),
            Stage7bRestartOutcome::P1SemanticPrepublicationReady(_)
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1d2_subprocess_kill_matrix_recovers_all_six_durable_frontiers() {
        for (phase, expected) in [
            (
                "p1d2-after-stage6-before-request-finalized",
                P1d2ExpectedRestart::PreAck {
                    request_finalized: false,
                },
            ),
            (
                "p1d2-after-request-finalized-before-ack",
                P1d2ExpectedRestart::PreAck {
                    request_finalized: true,
                },
            ),
            (
                "p1d2-after-ack-before-s-ack",
                P1d2ExpectedRestart::PreAck {
                    request_finalized: true,
                },
            ),
            ("p1d2-after-s-ack-before-truth", P1d2ExpectedRestart::Ack),
            ("p1d2-after-truth-before-s-truth", P1d2ExpectedRestart::Ack),
            ("p1d2-after-s-truth-before-xack", P1d2ExpectedRestart::Truth),
        ] {
            run_p1d2_crash_case(phase, expected).await;
        }
    }

    #[tokio::test]
    async fn p1d2_sequence_pair_allocation_crash_reconstructs_exact_ack_path() {
        run_p1d2_crash_case(
            "p1d2-after-sequence-pair-before-ack",
            P1d2ExpectedRestart::PreAck {
                request_finalized: true,
            },
        )
        .await;
    }
}
