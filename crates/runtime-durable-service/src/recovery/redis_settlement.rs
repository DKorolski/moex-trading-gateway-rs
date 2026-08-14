#![allow(
    dead_code,
    reason = "Stage 7B-d-b is composed into the supervised consumer only in closed d-c"
)]

use super::Stage7bDurableAckAuthorized;
use broker_core::command::CommandAckStatus;
use broker_core::{ClientOrderId, CommandAckReasonCode, StrategyRequestId};
use redis::aio::ConnectionManager;
use redis::streams::StreamRangeReply;
use runtime_command_bridge::{
    Stage7aCanonicalCommandIdentity, Stage7aDeterministicRejectionClass,
    Stage7aDeterministicRejectionEvidence, Stage7aDlqReason, Stage7aPermanentPoisonEvidence,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use strategy_runtime_core::Stage6RequestFinalDispositionV1;

const PAPER_PREFIX: &str = "finam_imoexf_paper:";
const MARKER_SCHEMA: u16 = 1;

const ATOMIC_SETTLEMENT_LUA: &str = r#"
local function type_name(key)
  local result = redis.call('TYPE', key)
  if type(result) == 'table' then return result['ok'] end
  return result
end

local source = KEYS[1]
local output = KEYS[2]
local entry_marker = KEYS[3]
local request_marker = KEYS[4]
local group = ARGV[1]
local entry_id = ARGV[2]
local kind = ARGV[3]
local canonical_fp = ARGV[4]
local canonical_payload = ARGV[5]
local duplicate_fp = ARGV[6]
local duplicate_payload = ARGV[7]
local request_identity = ARGV[8]
local terminal_request_ack_identity = ARGV[9]
local canonical_command_sha256 = ARGV[10]
local schema = tonumber(ARGV[11])

if schema ~= 1 then return redis.error_reply('STAGE7B_SCHEMA') end
if kind ~= 'ack' and kind ~= 'dlq' then return redis.error_reply('STAGE7B_KIND') end
if type_name(source) ~= 'stream' then return redis.error_reply('STAGE7B_SOURCE_TYPE') end
local output_type = type_name(output)
if output_type ~= 'none' and output_type ~= 'stream' then
  return redis.error_reply('STAGE7B_OUTPUT_TYPE')
end
local entry_type = type_name(entry_marker)
if entry_type ~= 'none' and entry_type ~= 'string' then
  return redis.error_reply('STAGE7B_ENTRY_MARKER_TYPE')
end
if kind == 'ack' then
  local request_type = type_name(request_marker)
  if request_type ~= 'none' and request_type ~= 'string' then
    return redis.error_reply('STAGE7B_REQUEST_MARKER_TYPE')
  end
end

local existing_entry = redis.call('GET', entry_marker)
if existing_entry then
  local ok, marker = pcall(cjson.decode, existing_entry)
  if not ok or marker['schema_version'] ~= schema or marker['settlement_kind'] ~= kind
     or marker['output_stream'] ~= output then
    return redis.error_reply('STAGE7B_ENTRY_MARKER_INVALID')
  end
  local fingerprint = marker['payload_fingerprint']
  if fingerprint ~= canonical_fp and fingerprint ~= duplicate_fp then
    return redis.error_reply('STAGE7B_CONFLICT_ENTRY_FINGERPRINT')
  end
  return {'committed', marker['output_id'], marker['classification']}
end

local classification = 'canonical'
local selected_fp = canonical_fp
local selected_payload = canonical_payload
local existing_request = false
if kind == 'ack' then
  existing_request = redis.call('GET', request_marker)
  if existing_request then
    local ok, marker = pcall(cjson.decode, existing_request)
    if not ok or marker['schema_version'] ~= schema
       or marker['request_identity'] ~= request_identity
       or marker['terminal_request_ack_identity'] ~= terminal_request_ack_identity
       or marker['canonical_command_sha256'] ~= canonical_command_sha256
       or marker['canonical_output_stream'] ~= output
       or marker['publication_known'] ~= true then
      return redis.error_reply('STAGE7B_CONFLICT_REQUEST_MARKER')
    end
    classification = 'duplicate'
    selected_fp = duplicate_fp
    selected_payload = duplicate_payload
  end
end

local pending = redis.call('XPENDING', source, group, entry_id, entry_id, 1)
if #pending ~= 1 or tostring(pending[1][1]) ~= entry_id then
  return redis.error_reply('STAGE7B_SOURCE_NOT_PENDING')
end

local output_id = redis.call('XADD', output, '*', 'payload', selected_payload)
redis.call('SET', entry_marker, cjson.encode({
  schema_version = schema,
  settlement_kind = kind,
  payload_fingerprint = selected_fp,
  output_stream = output,
  output_id = output_id,
  classification = classification
}))
if kind == 'ack' and not existing_request then
  redis.call('SET', request_marker, cjson.encode({
    schema_version = schema,
    request_identity = request_identity,
    terminal_request_ack_identity = terminal_request_ack_identity,
    canonical_command_sha256 = canonical_command_sha256,
    canonical_output_stream = output,
    canonical_output_id = output_id,
    publication_known = true
  }))
end
redis.call('XACK', source, group, entry_id)
return {'committed', output_id, classification}
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stage7bRedisSettlementContext {
    hash_tag: String,
    source_stream: String,
    ack_stream: String,
    dlq_stream: String,
    consumer_group: String,
    redis_entry_id: String,
}

impl Stage7bRedisSettlementContext {
    pub(crate) fn new(
        hash_tag: impl Into<String>,
        source_stream: impl Into<String>,
        ack_stream: impl Into<String>,
        dlq_stream: impl Into<String>,
        consumer_group: impl Into<String>,
        redis_entry_id: impl Into<String>,
    ) -> Result<Self, Stage7bRedisSettlementError> {
        let context = Self {
            hash_tag: hash_tag.into(),
            source_stream: source_stream.into(),
            ack_stream: ack_stream.into(),
            dlq_stream: dlq_stream.into(),
            consumer_group: consumer_group.into(),
            redis_entry_id: redis_entry_id.into(),
        };
        context.validate()?;
        Ok(context)
    }

    fn validate(&self) -> Result<(), Stage7bRedisSettlementError> {
        if !token(&self.hash_tag)
            || !token(&self.consumer_group)
            || !stream_id(&self.redis_entry_id)
        {
            return Err(Stage7bRedisSettlementError::InvalidContext);
        }
        let expected_tag = format!("{{{}}}", self.hash_tag);
        let streams = [&self.source_stream, &self.ack_stream, &self.dlq_stream];
        if streams.iter().any(|stream| {
            !stream.starts_with(PAPER_PREFIX)
                || stream.matches('{').count() != 1
                || stream.matches('}').count() != 1
                || !stream.contains(&expected_tag)
                || stream.contains(char::is_whitespace)
        }) || self.source_stream == self.ack_stream
            || self.source_stream == self.dlq_stream
            || self.ack_stream == self.dlq_stream
        {
            return Err(Stage7bRedisSettlementError::InvalidContext);
        }
        Ok(())
    }

    fn output_stream(&self, kind: Stage7bSettlementKind) -> &str {
        match kind {
            Stage7bSettlementKind::Ack => &self.ack_stream,
            Stage7bSettlementKind::Dlq => &self.dlq_stream,
        }
    }

    fn entry_marker_key(&self, kind: Stage7bSettlementKind) -> String {
        let stable = format!(
            "{}\0{}\0{}\0{}",
            self.source_stream,
            self.consumer_group,
            self.redis_entry_id,
            kind.as_str()
        );
        format!(
            "{PAPER_PREFIX}{{{}}}:stage7b:settlement:entry:{}",
            self.hash_tag,
            sha256_hex(stable.as_bytes())
        )
    }

    fn request_marker_key(&self, request_id: Option<StrategyRequestId>) -> String {
        let identity = request_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("poison:{}", self.redis_entry_id));
        format!(
            "{PAPER_PREFIX}{{{}}}:stage7b:settlement:request:{}",
            self.hash_tag,
            sha256_hex(identity.as_bytes())
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage7bSettlementKind {
    Ack,
    Dlq,
}

impl Stage7bSettlementKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ack => "ack",
            Self::Dlq => "dlq",
        }
    }
}

#[derive(Serialize)]
struct Stage7bAckPayload<'a> {
    schema_version: u16,
    request_id: StrategyRequestId,
    client_order_id: &'a str,
    broker_order_id: Option<&'a str>,
    canonical_command_sha256: &'a str,
    final_disposition: Stage6RequestFinalDispositionV1,
    final_record_id: &'a str,
    final_sequence: u64,
    stage6_checkpoint_sha256: &'a str,
    seal_generation: u64,
    seal_commitment_sha256: &'a str,
    settlement_authority_fingerprint_sha256: &'a str,
    terminal_request_ack_identity_sha256: &'a str,
    publication: &'static str,
}

#[derive(Serialize)]
struct Stage7bPreStage6AckPayload<'a> {
    schema_version: u16,
    request_id: StrategyRequestId,
    client_order_id: &'a str,
    broker_order_id: Option<&'a str>,
    canonical_command_sha256: &'a str,
    status: CommandAckStatus,
    reason_code: CommandAckReasonCode,
    rejection_class: Stage7aDeterministicRejectionClass,
    stage6_checkpoint_sha256: &'a str,
    seal_generation: u64,
    seal_commitment_sha256: &'a str,
    settlement_authority_fingerprint_sha256: &'a str,
    terminal_request_ack_identity_sha256: &'a str,
    stage6_mutation: bool,
    publication: &'static str,
}

#[derive(Serialize)]
struct Stage7bDlqPayload<'a> {
    schema_version: u16,
    redis_entry_id: &'a str,
    poison_reason: &'a str,
    payload_len: usize,
    redacted_payload_sha256: &'a str,
    stage6_checkpoint_sha256: &'a str,
}

pub(super) struct Stage7bRedisAckSettlementPlan {
    context: Stage7bRedisSettlementContext,
    request_id: StrategyRequestId,
    terminal_request_ack_identity: String,
    canonical_command_sha256: String,
    canonical_payload: String,
    canonical_payload_fingerprint: String,
    duplicate_payload: String,
    duplicate_payload_fingerprint: String,
}

pub(crate) struct Stage7bPreStage6PoisonObservation {
    context: Stage7bRedisSettlementContext,
    poison_reason: Stage7aDlqReason,
    payload_len: usize,
    redacted_payload_sha256: String,
    stage6_checkpoint_sha256: String,
}

pub(crate) struct Stage7bPreStage6CommandObservation {
    request_id: StrategyRequestId,
    stage6_checkpoint_sha256: String,
    request_identity_was_established: bool,
}

impl Stage7bPreStage6CommandObservation {
    pub(crate) fn request_identity_was_established(&self) -> bool {
        self.request_identity_was_established
    }
}

/// Validated publication-only history read from the d-b request marker and
/// its canonical ACK stream entry.  Fields remain private so this proof cannot
/// be converted into Stage 6 admission or provider authority.
pub(crate) struct Stage7bCanonicalRequestPublicationEvidence {
    request_id: StrategyRequestId,
    canonical_command_sha256: String,
    terminal_request_ack_identity: String,
    canonical_output_id: String,
    canonical_payload: String,
    duplicate_payload: String,
}

impl Stage7bCanonicalRequestPublicationEvidence {
    pub(crate) fn matches(&self, identity: &Stage7aCanonicalCommandIdentity) -> bool {
        self.request_id == identity.strategy_request_id()
            && self.canonical_command_sha256 == identity.canonical_command_sha256()
    }
}

pub(crate) enum Stage7bCanonicalRequestPublicationLookup {
    Absent,
    Present(Stage7bCanonicalRequestPublicationEvidence),
}

pub(super) struct Stage7bCanonicalMarkerDuplicateAuthorized {
    evidence: Stage7bCanonicalRequestPublicationEvidence,
}

pub(super) struct Stage7bPreStage6AckAuthorized {
    operational_identity_sha256: String,
    strategy_request_id: StrategyRequestId,
    durable_client_order_id: ClientOrderId,
    canonical_command_sha256: String,
    status: CommandAckStatus,
    reason_code: CommandAckReasonCode,
    rejection_class: Stage7aDeterministicRejectionClass,
    stage6_checkpoint_sha256: String,
    seal_generation: u64,
    seal_commitment_sha256: String,
    settlement_authority_fingerprint_sha256: String,
    terminal_request_ack_identity_sha256: String,
}

pub(super) fn authorize_canonical_marker_duplicate(
    observation: Stage7bPreStage6CommandObservation,
    identity: &Stage7aCanonicalCommandIdentity,
    evidence: Stage7bCanonicalRequestPublicationEvidence,
    current_stage6_checkpoint_sha256: &str,
    current_request_identity_exists: bool,
) -> Result<Stage7bCanonicalMarkerDuplicateAuthorized, Stage7bRedisSettlementError> {
    if observation.request_id != identity.strategy_request_id()
        || observation.request_identity_was_established
        || current_request_identity_exists
        || observation.stage6_checkpoint_sha256 != current_stage6_checkpoint_sha256
        || !evidence.matches(identity)
    {
        return Err(Stage7bRedisSettlementError::RequestMarkerAuthorityDrift);
    }
    Ok(Stage7bCanonicalMarkerDuplicateAuthorized { evidence })
}

pub(super) fn canonical_marker_duplicate_ack_plan(
    authority: Stage7bCanonicalMarkerDuplicateAuthorized,
    context: Stage7bRedisSettlementContext,
) -> Result<Stage7bRedisAckSettlementPlan, Stage7bRedisSettlementError> {
    context.validate()?;
    let evidence = authority.evidence;
    Ok(Stage7bRedisAckSettlementPlan {
        context,
        request_id: evidence.request_id,
        terminal_request_ack_identity: evidence.terminal_request_ack_identity,
        canonical_command_sha256: evidence.canonical_command_sha256,
        canonical_payload_fingerprint: sha256_hex(evidence.canonical_payload.as_bytes()),
        duplicate_payload_fingerprint: sha256_hex(evidence.duplicate_payload.as_bytes()),
        canonical_payload: evidence.canonical_payload,
        duplicate_payload: evidence.duplicate_payload,
    })
}

pub(super) struct Stage7bPoisonDlqAuthorized {
    context: Stage7bRedisSettlementContext,
    poison_reason: String,
    payload_len: usize,
    redacted_payload_sha256: String,
    stage6_checkpoint_sha256: String,
}

pub(super) struct Stage7bRedisDlqSettlementPlan {
    context: Stage7bRedisSettlementContext,
    payload: String,
    payload_fingerprint: String,
}

pub(super) fn ack_plan(
    authority: Stage7bDurableAckAuthorized,
    context: Stage7bRedisSettlementContext,
) -> Result<Stage7bRedisAckSettlementPlan, Stage7bRedisSettlementError> {
    context.validate()?;
    let canonical_payload = serde_json::to_string(&Stage7bAckPayload {
        schema_version: MARKER_SCHEMA,
        request_id: authority.strategy_request_id,
        client_order_id: authority.durable_client_order_id.as_str(),
        broker_order_id: authority
            .broker_order_id
            .as_ref()
            .map(|value| value.as_str()),
        canonical_command_sha256: &authority.canonical_command_sha256,
        final_disposition: authority.final_disposition,
        final_record_id: &authority.final_record_id,
        final_sequence: authority.final_sequence,
        stage6_checkpoint_sha256: &authority.stage6_checkpoint_sha256,
        seal_generation: authority.seal_generation,
        seal_commitment_sha256: &authority.seal_commitment_sha256,
        settlement_authority_fingerprint_sha256: &authority.settlement_authority_fingerprint_sha256,
        terminal_request_ack_identity_sha256: &authority.terminal_request_ack_identity_sha256,
        publication: "canonical",
    })?;
    let duplicate_payload = serde_json::to_string(&Stage7bAckPayload {
        schema_version: MARKER_SCHEMA,
        request_id: authority.strategy_request_id,
        client_order_id: authority.durable_client_order_id.as_str(),
        broker_order_id: authority
            .broker_order_id
            .as_ref()
            .map(|value| value.as_str()),
        canonical_command_sha256: &authority.canonical_command_sha256,
        final_disposition: authority.final_disposition,
        final_record_id: &authority.final_record_id,
        final_sequence: authority.final_sequence,
        stage6_checkpoint_sha256: &authority.stage6_checkpoint_sha256,
        seal_generation: authority.seal_generation,
        seal_commitment_sha256: &authority.seal_commitment_sha256,
        settlement_authority_fingerprint_sha256: &authority.settlement_authority_fingerprint_sha256,
        terminal_request_ack_identity_sha256: &authority.terminal_request_ack_identity_sha256,
        publication: "duplicate",
    })?;
    Ok(Stage7bRedisAckSettlementPlan {
        context,
        request_id: authority.strategy_request_id,
        terminal_request_ack_identity: authority.terminal_request_ack_identity_sha256,
        canonical_command_sha256: authority.canonical_command_sha256,
        canonical_payload_fingerprint: sha256_hex(canonical_payload.as_bytes()),
        duplicate_payload_fingerprint: sha256_hex(duplicate_payload.as_bytes()),
        canonical_payload,
        duplicate_payload,
    })
}

pub(super) fn pre_stage6_command_observation(
    request_id: StrategyRequestId,
    stage6_checkpoint_sha256: String,
    request_identity_was_established: bool,
) -> Stage7bPreStage6CommandObservation {
    Stage7bPreStage6CommandObservation {
        request_id,
        stage6_checkpoint_sha256,
        request_identity_was_established,
    }
}

pub(super) fn authorize_pre_stage6_rejection(
    observation: Stage7bPreStage6CommandObservation,
    evidence: Stage7aDeterministicRejectionEvidence,
    current_stage6_checkpoint_sha256: &str,
    current_request_identity_exists: bool,
    operational_identity_sha256: &str,
    seal_generation: u64,
    seal_commitment_sha256: &str,
) -> Result<Stage7bPreStage6AckAuthorized, Stage7bRedisSettlementError> {
    if observation.request_id != evidence.strategy_request_id()
        || observation.request_identity_was_established
        || current_request_identity_exists
        || observation.stage6_checkpoint_sha256 != current_stage6_checkpoint_sha256
    {
        return Err(Stage7bRedisSettlementError::PreStage6RejectionAuthorityDrift);
    }

    #[derive(Serialize)]
    struct SettlementAuthority<'a> {
        schema_version: u16,
        domain: &'static str,
        operational_identity_sha256: &'a str,
        strategy_request_id: StrategyRequestId,
        durable_client_order_id: &'a str,
        canonical_command_sha256: &'a str,
        status: CommandAckStatus,
        reason_code: CommandAckReasonCode,
        rejection_class: Stage7aDeterministicRejectionClass,
        stage6_checkpoint_sha256: &'a str,
        seal_generation: u64,
        seal_commitment_sha256: &'a str,
        stage6_mutation: bool,
    }

    #[derive(Serialize)]
    struct TerminalIdentity<'a> {
        schema_version: u16,
        domain: &'static str,
        operational_identity_sha256: &'a str,
        strategy_request_id: StrategyRequestId,
        durable_client_order_id: &'a str,
        canonical_command_sha256: &'a str,
        status: CommandAckStatus,
        reason_code: CommandAckReasonCode,
        rejection_class: Stage7aDeterministicRejectionClass,
        stage6_mutation: bool,
        terminal_ack_schema: u16,
    }

    let settlement_authority_fingerprint_sha256 =
        sha256_hex(&serde_json::to_vec(&SettlementAuthority {
            schema_version: 1,
            domain: "moex.stage7b.pre-stage6-rejection-authority.v1",
            operational_identity_sha256,
            strategy_request_id: evidence.strategy_request_id(),
            durable_client_order_id: evidence.durable_client_order_id().as_str(),
            canonical_command_sha256: evidence.canonical_command_sha256(),
            status: evidence.status(),
            reason_code: evidence.reason_code(),
            rejection_class: evidence.rejection_class(),
            stage6_checkpoint_sha256: current_stage6_checkpoint_sha256,
            seal_generation,
            seal_commitment_sha256,
            stage6_mutation: false,
        })?);
    let terminal_request_ack_identity_sha256 =
        sha256_hex(&serde_json::to_vec(&TerminalIdentity {
            schema_version: 1,
            domain: "moex.stage7b.pre-stage6-rejection-terminal-identity.v1",
            operational_identity_sha256,
            strategy_request_id: evidence.strategy_request_id(),
            durable_client_order_id: evidence.durable_client_order_id().as_str(),
            canonical_command_sha256: evidence.canonical_command_sha256(),
            status: evidence.status(),
            reason_code: evidence.reason_code(),
            rejection_class: evidence.rejection_class(),
            stage6_mutation: false,
            terminal_ack_schema: MARKER_SCHEMA,
        })?);

    Ok(Stage7bPreStage6AckAuthorized {
        operational_identity_sha256: operational_identity_sha256.to_string(),
        strategy_request_id: evidence.strategy_request_id(),
        durable_client_order_id: evidence.durable_client_order_id().clone(),
        canonical_command_sha256: evidence.canonical_command_sha256().to_string(),
        status: evidence.status(),
        reason_code: evidence.reason_code(),
        rejection_class: evidence.rejection_class(),
        stage6_checkpoint_sha256: current_stage6_checkpoint_sha256.to_string(),
        seal_generation,
        seal_commitment_sha256: seal_commitment_sha256.to_string(),
        settlement_authority_fingerprint_sha256,
        terminal_request_ack_identity_sha256,
    })
}

pub(super) fn pre_stage6_rejection_ack_plan(
    authority: Stage7bPreStage6AckAuthorized,
    context: Stage7bRedisSettlementContext,
) -> Result<Stage7bRedisAckSettlementPlan, Stage7bRedisSettlementError> {
    context.validate()?;
    let payload = |publication| {
        serde_json::to_string(&Stage7bPreStage6AckPayload {
            schema_version: MARKER_SCHEMA,
            request_id: authority.strategy_request_id,
            client_order_id: authority.durable_client_order_id.as_str(),
            broker_order_id: None,
            canonical_command_sha256: &authority.canonical_command_sha256,
            status: authority.status,
            reason_code: authority.reason_code,
            rejection_class: authority.rejection_class,
            stage6_checkpoint_sha256: &authority.stage6_checkpoint_sha256,
            seal_generation: authority.seal_generation,
            seal_commitment_sha256: &authority.seal_commitment_sha256,
            settlement_authority_fingerprint_sha256: &authority
                .settlement_authority_fingerprint_sha256,
            terminal_request_ack_identity_sha256: &authority.terminal_request_ack_identity_sha256,
            stage6_mutation: false,
            publication,
        })
    };
    let canonical_payload = payload("canonical")?;
    let duplicate_payload = payload("duplicate")?;
    Ok(Stage7bRedisAckSettlementPlan {
        context,
        request_id: authority.strategy_request_id,
        terminal_request_ack_identity: authority.terminal_request_ack_identity_sha256,
        canonical_command_sha256: authority.canonical_command_sha256,
        canonical_payload_fingerprint: sha256_hex(canonical_payload.as_bytes()),
        duplicate_payload_fingerprint: sha256_hex(duplicate_payload.as_bytes()),
        canonical_payload,
        duplicate_payload,
    })
}

pub(super) fn poison_observation(
    context: Stage7bRedisSettlementContext,
    evidence: Stage7aPermanentPoisonEvidence,
    stage6_checkpoint_sha256: String,
) -> Result<Stage7bPreStage6PoisonObservation, Stage7bRedisSettlementError> {
    if context.redis_entry_id != evidence.redis_entry_id() {
        return Err(Stage7bRedisSettlementError::PoisonAuthorityDrift);
    }
    Ok(Stage7bPreStage6PoisonObservation {
        context,
        poison_reason: evidence.reason(),
        payload_len: evidence.payload_len(),
        redacted_payload_sha256: evidence.redacted_payload_sha256().to_string(),
        stage6_checkpoint_sha256,
    })
}

pub(super) fn authorize_poison(
    observation: Stage7bPreStage6PoisonObservation,
    current_stage6_checkpoint_sha256: &str,
) -> Result<Stage7bPoisonDlqAuthorized, Stage7bRedisSettlementError> {
    if observation.stage6_checkpoint_sha256 != current_stage6_checkpoint_sha256 {
        return Err(Stage7bRedisSettlementError::PoisonAuthorityDrift);
    }
    Ok(Stage7bPoisonDlqAuthorized {
        context: observation.context,
        poison_reason: observation.poison_reason.as_str().to_string(),
        payload_len: observation.payload_len,
        redacted_payload_sha256: observation.redacted_payload_sha256,
        stage6_checkpoint_sha256: observation.stage6_checkpoint_sha256,
    })
}

pub(super) fn dlq_plan(
    authority: Stage7bPoisonDlqAuthorized,
) -> Result<Stage7bRedisDlqSettlementPlan, Stage7bRedisSettlementError> {
    let payload = serde_json::to_string(&Stage7bDlqPayload {
        schema_version: MARKER_SCHEMA,
        redis_entry_id: &authority.context.redis_entry_id,
        poison_reason: &authority.poison_reason,
        payload_len: authority.payload_len,
        redacted_payload_sha256: &authority.redacted_payload_sha256,
        stage6_checkpoint_sha256: &authority.stage6_checkpoint_sha256,
    })?;
    Ok(Stage7bRedisDlqSettlementPlan {
        context: authority.context,
        payload_fingerprint: sha256_hex(payload.as_bytes()),
        payload,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stage7bRedisSettlementOutcome {
    pub(super) output_id: String,
    pub(super) classification: String,
}

pub(crate) struct Stage7bRedisSettlementBackend {
    connection: ConnectionManager,
    transport_healthy: bool,
    unresolved_settlement_keys: HashSet<String>,
}

impl Stage7bRedisSettlementBackend {
    pub(crate) async fn connect(redis_url: &str) -> Result<Self, Stage7bRedisSettlementError> {
        let client = redis::Client::open(redis_url)?;
        let connection = ConnectionManager::new(client).await?;
        Ok(Self {
            connection,
            transport_healthy: true,
            unresolved_settlement_keys: HashSet::new(),
        })
    }

    pub(crate) fn healthy(&self) -> bool {
        self.transport_healthy && self.unresolved_settlement_keys.is_empty()
    }

    /// Reads publication history only.  The returned opaque proof can veto a
    /// new Stage 6 admission or reproduce an existing ACK; it cannot authorize
    /// provider execution.
    pub(crate) async fn lookup_canonical_request_publication(
        &mut self,
        context: &Stage7bRedisSettlementContext,
        request_id: StrategyRequestId,
    ) -> Result<Stage7bCanonicalRequestPublicationLookup, Stage7bRedisSettlementError> {
        context.validate()?;
        let request_marker = context.request_marker_key(Some(request_id));
        let result = self
            .lookup_canonical_request_publication_inner(context, request_id, &request_marker)
            .await;
        match result {
            Ok(lookup) => {
                self.transport_healthy = true;
                self.unresolved_settlement_keys.remove(&request_marker);
                Ok(lookup)
            }
            Err(error) => {
                self.transport_healthy = false;
                self.unresolved_settlement_keys.insert(request_marker);
                Err(error)
            }
        }
    }

    async fn lookup_canonical_request_publication_inner(
        &mut self,
        context: &Stage7bRedisSettlementContext,
        request_id: StrategyRequestId,
        request_marker: &str,
    ) -> Result<Stage7bCanonicalRequestPublicationLookup, Stage7bRedisSettlementError> {
        #[derive(Deserialize)]
        struct RequestMarker {
            schema_version: u16,
            request_identity: String,
            terminal_request_ack_identity: String,
            canonical_command_sha256: String,
            canonical_output_stream: String,
            canonical_output_id: String,
            publication_known: bool,
        }

        #[derive(Deserialize)]
        struct CanonicalAckProbe {
            schema_version: u16,
            request_id: StrategyRequestId,
            canonical_command_sha256: String,
            terminal_request_ack_identity_sha256: String,
            publication: String,
        }

        let encoded: Option<String> = redis::cmd("GET")
            .arg(request_marker)
            .query_async(&mut self.connection)
            .await?;
        let Some(encoded) = encoded else {
            return Ok(Stage7bCanonicalRequestPublicationLookup::Absent);
        };
        let marker: RequestMarker = serde_json::from_str(&encoded)
            .map_err(|_| Stage7bRedisSettlementError::InvalidRequestMarker)?;
        if marker.schema_version != MARKER_SCHEMA
            || marker.request_identity != request_id.to_string()
            || !marker.publication_known
            || marker.canonical_output_stream != context.ack_stream
            || !sha256(&marker.canonical_command_sha256)
            || !sha256(&marker.terminal_request_ack_identity)
            || !stream_id(&marker.canonical_output_id)
        {
            return Err(Stage7bRedisSettlementError::InvalidRequestMarker);
        }
        let output: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&context.ack_stream)
            .arg(&marker.canonical_output_id)
            .arg(&marker.canonical_output_id)
            .arg("COUNT")
            .arg(1)
            .query_async(&mut self.connection)
            .await?;
        if output.ids.len() != 1 || output.ids[0].id != marker.canonical_output_id {
            return Err(Stage7bRedisSettlementError::InvalidCanonicalOutput);
        }
        let canonical_payload = output.ids[0]
            .get::<String>("payload")
            .ok_or(Stage7bRedisSettlementError::InvalidCanonicalOutput)?;
        let probe: CanonicalAckProbe = serde_json::from_str(&canonical_payload)
            .map_err(|_| Stage7bRedisSettlementError::InvalidCanonicalOutput)?;
        if probe.schema_version != MARKER_SCHEMA
            || probe.request_id != request_id
            || probe.canonical_command_sha256 != marker.canonical_command_sha256
            || probe.terminal_request_ack_identity_sha256 != marker.terminal_request_ack_identity
            || probe.publication != "canonical"
        {
            return Err(Stage7bRedisSettlementError::InvalidCanonicalOutput);
        }
        let mut duplicate: serde_json::Value = serde_json::from_str(&canonical_payload)
            .map_err(|_| Stage7bRedisSettlementError::InvalidCanonicalOutput)?;
        let Some(publication) = duplicate.get_mut("publication") else {
            return Err(Stage7bRedisSettlementError::InvalidCanonicalOutput);
        };
        *publication = serde_json::Value::String("duplicate".to_string());
        let duplicate_payload = serde_json::to_string(&duplicate)?;
        Ok(Stage7bCanonicalRequestPublicationLookup::Present(
            Stage7bCanonicalRequestPublicationEvidence {
                request_id,
                canonical_command_sha256: marker.canonical_command_sha256,
                terminal_request_ack_identity: marker.terminal_request_ack_identity,
                canonical_output_id: marker.canonical_output_id,
                canonical_payload,
                duplicate_payload,
            },
        ))
    }

    pub(super) async fn settle_ack(
        &mut self,
        plan: Stage7bRedisAckSettlementPlan,
    ) -> Result<Stage7bRedisSettlementOutcome, Stage7bRedisSettlementError> {
        self.settle_ack_inner(plan, false).await
    }

    #[cfg(test)]
    pub(super) async fn settle_ack_with_lost_response(
        &mut self,
        plan: Stage7bRedisAckSettlementPlan,
    ) -> Result<Stage7bRedisSettlementOutcome, Stage7bRedisSettlementError> {
        self.settle_ack_inner(plan, true).await
    }

    async fn settle_ack_inner(
        &mut self,
        plan: Stage7bRedisAckSettlementPlan,
        lose_response_after_commit: bool,
    ) -> Result<Stage7bRedisSettlementOutcome, Stage7bRedisSettlementError> {
        let marker = plan.context.entry_marker_key(Stage7bSettlementKind::Ack);
        let request_marker = plan.context.request_marker_key(Some(plan.request_id));
        let request_identity = plan.request_id.to_string();
        let result = invoke(
            &mut self.connection,
            SettlementInvocation {
                context: &plan.context,
                kind: Stage7bSettlementKind::Ack,
                entry_marker: &marker,
                request_marker: &request_marker,
                canonical_fingerprint: &plan.canonical_payload_fingerprint,
                canonical_payload: &plan.canonical_payload,
                duplicate_fingerprint: &plan.duplicate_payload_fingerprint,
                duplicate_payload: &plan.duplicate_payload,
                request_identity: &request_identity,
                terminal_request_ack_identity: &plan.terminal_request_ack_identity,
                canonical_command_sha256: &plan.canonical_command_sha256,
            },
        )
        .await;
        self.finish(&marker, result, lose_response_after_commit)
    }

    pub(super) async fn settle_dlq(
        &mut self,
        plan: Stage7bRedisDlqSettlementPlan,
    ) -> Result<Stage7bRedisSettlementOutcome, Stage7bRedisSettlementError> {
        self.settle_dlq_inner(plan, false).await
    }

    #[cfg(test)]
    pub(super) async fn settle_dlq_with_lost_response(
        &mut self,
        plan: Stage7bRedisDlqSettlementPlan,
    ) -> Result<Stage7bRedisSettlementOutcome, Stage7bRedisSettlementError> {
        self.settle_dlq_inner(plan, true).await
    }

    async fn settle_dlq_inner(
        &mut self,
        plan: Stage7bRedisDlqSettlementPlan,
        lose_response_after_commit: bool,
    ) -> Result<Stage7bRedisSettlementOutcome, Stage7bRedisSettlementError> {
        let marker = plan.context.entry_marker_key(Stage7bSettlementKind::Dlq);
        let request_marker = plan.context.request_marker_key(None);
        let result = invoke(
            &mut self.connection,
            SettlementInvocation {
                context: &plan.context,
                kind: Stage7bSettlementKind::Dlq,
                entry_marker: &marker,
                request_marker: &request_marker,
                canonical_fingerprint: &plan.payload_fingerprint,
                canonical_payload: &plan.payload,
                duplicate_fingerprint: &plan.payload_fingerprint,
                duplicate_payload: &plan.payload,
                request_identity: "",
                terminal_request_ack_identity: "",
                canonical_command_sha256: "",
            },
        )
        .await;
        self.finish(&marker, result, lose_response_after_commit)
    }

    fn finish(
        &mut self,
        settlement_key: &str,
        result: Result<Stage7bRedisSettlementOutcome, Stage7bRedisSettlementError>,
        lose_response_after_commit: bool,
    ) -> Result<Stage7bRedisSettlementOutcome, Stage7bRedisSettlementError> {
        match result {
            Ok(_) if lose_response_after_commit => {
                self.transport_healthy = false;
                self.unresolved_settlement_keys
                    .insert(settlement_key.to_string());
                Err(Stage7bRedisSettlementError::ResponseLostAfterCommit)
            }
            Ok(outcome) => {
                self.transport_healthy = true;
                self.unresolved_settlement_keys.remove(settlement_key);
                Ok(outcome)
            }
            Err(error) => {
                if !matches!(
                    error,
                    Stage7bRedisSettlementError::Conflict
                        | Stage7bRedisSettlementError::SourceNotPending
                ) {
                    self.transport_healthy = false;
                    self.unresolved_settlement_keys
                        .insert(settlement_key.to_string());
                }
                Err(error)
            }
        }
    }
}

struct SettlementInvocation<'a> {
    context: &'a Stage7bRedisSettlementContext,
    kind: Stage7bSettlementKind,
    entry_marker: &'a str,
    request_marker: &'a str,
    canonical_fingerprint: &'a str,
    canonical_payload: &'a str,
    duplicate_fingerprint: &'a str,
    duplicate_payload: &'a str,
    request_identity: &'a str,
    terminal_request_ack_identity: &'a str,
    canonical_command_sha256: &'a str,
}

async fn invoke(
    connection: &mut ConnectionManager,
    invocation: SettlementInvocation<'_>,
) -> Result<Stage7bRedisSettlementOutcome, Stage7bRedisSettlementError> {
    let context = invocation.context;
    let result: redis::RedisResult<(String, String, String)> = redis::cmd("EVAL")
        .arg(ATOMIC_SETTLEMENT_LUA)
        .arg(4)
        .arg(&context.source_stream)
        .arg(context.output_stream(invocation.kind))
        .arg(invocation.entry_marker)
        .arg(invocation.request_marker)
        .arg(&context.consumer_group)
        .arg(&context.redis_entry_id)
        .arg(invocation.kind.as_str())
        .arg(invocation.canonical_fingerprint)
        .arg(invocation.canonical_payload)
        .arg(invocation.duplicate_fingerprint)
        .arg(invocation.duplicate_payload)
        .arg(invocation.request_identity)
        .arg(invocation.terminal_request_ack_identity)
        .arg(invocation.canonical_command_sha256)
        .arg(MARKER_SCHEMA)
        .query_async(connection)
        .await;
    match result {
        Ok((status, output_id, classification)) if status == "committed" => {
            Ok(Stage7bRedisSettlementOutcome {
                output_id,
                classification,
            })
        }
        Ok(_) => Err(Stage7bRedisSettlementError::InvalidRedisReply),
        Err(error) => {
            let message = error.to_string();
            if message.contains("STAGE7B_CONFLICT") {
                Err(Stage7bRedisSettlementError::Conflict)
            } else if message.contains("STAGE7B_SOURCE_NOT_PENDING") {
                Err(Stage7bRedisSettlementError::SourceNotPending)
            } else {
                Err(Stage7bRedisSettlementError::Redis(error))
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Stage7bRedisSettlementError {
    #[error("invalid Stage 7B Redis settlement context")]
    InvalidContext,
    #[error("Stage 7B poison authority drifted after pre-Stage6 observation")]
    PoisonAuthorityDrift,
    #[error("Stage 7B deterministic rejection authority drifted after pre-Stage6 observation")]
    PreStage6RejectionAuthorityDrift,
    #[error("Stage 7B request-marker publication authority drifted after observation")]
    RequestMarkerAuthorityDrift,
    #[error("Stage 7B request marker is absent-schema, malformed or inconsistent")]
    InvalidRequestMarker,
    #[error("Stage 7B canonical ACK output is missing or inconsistent with its marker")]
    InvalidCanonicalOutput,
    #[error("Stage 7B settlement identity conflict")]
    Conflict,
    #[error("Stage 7B source entry is not pending in the expected group")]
    SourceNotPending,
    #[error("Stage 7B Redis settlement reply is invalid")]
    InvalidRedisReply,
    #[error("Stage 7B Redis settlement committed but its response was lost")]
    ResponseLostAfterCommit,
    #[error("Stage 7B Redis settlement failed")]
    Redis(#[from] redis::RedisError),
    #[error("Stage 7B settlement payload encoding failed")]
    Json(#[from] serde_json::Error),
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn stream_id(value: &str) -> bool {
    let Some((milliseconds, sequence)) = value.split_once('-') else {
        return false;
    };
    milliseconds.parse::<u64>().is_ok()
        && sequence.parse::<u64>().is_ok()
        && !value.contains(char::is_whitespace)
}

fn sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{BrokerOrderId, ClientOrderId};
    use redis::streams::{StreamPendingCountReply, StreamRangeReply, StreamReadReply};
    use runtime_command_bridge::classify_stage7a_permanent_pre_admission_poison;
    use std::net::TcpListener;
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;
    use uuid::Uuid;

    struct RedisServer {
        child: Child,
        url: String,
    }

    static REDIS_START_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    impl RedisServer {
        async fn start() -> Self {
            let _guard = REDIS_START_LOCK.lock().await;
            for _ in 0..10 {
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
                    .expect("redis-server is required for Stage 7B-d-b tests");
                let url = format!("redis://127.0.0.1:{port}/");
                for _ in 0..100 {
                    if let Ok(client) = redis::Client::open(url.as_str()) {
                        if let Ok(mut connection) = ConnectionManager::new(client).await {
                            let pong: redis::RedisResult<String> =
                                redis::cmd("PING").query_async(&mut connection).await;
                            if pong.as_deref() == Ok("PONG") && child.try_wait().unwrap().is_none()
                            {
                                return Self { child, url };
                            }
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                let _ = child.kill();
                let _ = child.wait();
            }
            panic!("temporary Stage 7B Redis did not start")
        }
    }

    impl Drop for RedisServer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn authority(request: u128, canonical_byte: char) -> Stage7bDurableAckAuthorized {
        authority_with_fingerprints(request, canonical_byte, canonical_byte)
    }

    fn authority_with_fingerprints(
        request: u128,
        settlement_authority_byte: char,
        terminal_identity_byte: char,
    ) -> Stage7bDurableAckAuthorized {
        let request_id = StrategyRequestId::from(Uuid::from_u128(request));
        Stage7bDurableAckAuthorized {
            operational_identity_sha256: "1".repeat(64),
            strategy_request_id: request_id,
            durable_client_order_id: ClientOrderId::from_strategy_request(request_id),
            broker_order_id: Some(BrokerOrderId::new(format!("ORDER-{request}"))),
            canonical_command_sha256: "2".repeat(64),
            final_disposition: Stage6RequestFinalDispositionV1::Completed,
            final_record_id: format!("final-{request}"),
            final_sequence: 4,
            stage6_checkpoint_sha256: "3".repeat(64),
            seal_generation: 2,
            seal_commitment_sha256: "4".repeat(64),
            settlement_authority_fingerprint_sha256: settlement_authority_byte
                .to_string()
                .repeat(64),
            terminal_request_ack_identity_sha256: terminal_identity_byte.to_string().repeat(64),
        }
    }

    fn names(tag: &str) -> (String, String, String, String) {
        (
            format!("{PAPER_PREFIX}{{{tag}}}:commands"),
            format!("{PAPER_PREFIX}{{{tag}}}:acks"),
            format!("{PAPER_PREFIX}{{{tag}}}:dlq"),
            format!("stage7b-{tag}"),
        )
    }

    async fn connection(redis: &RedisServer) -> ConnectionManager {
        ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
            .await
            .unwrap()
    }

    async fn pending_context(
        connection: &mut ConnectionManager,
        tag: &str,
        payload: &str,
    ) -> Stage7bRedisSettlementContext {
        let (source, ack, dlq, group) = names(tag);
        let create: redis::RedisResult<()> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&source)
            .arg(&group)
            .arg("0-0")
            .arg("MKSTREAM")
            .query_async(connection)
            .await;
        if let Err(error) = create {
            assert!(error.to_string().contains("BUSYGROUP"));
        }
        let _: String = redis::cmd("XADD")
            .arg(&source)
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async(connection)
            .await
            .unwrap();
        let reply: StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(&group)
            .arg("consumer-a")
            .arg("COUNT")
            .arg(1)
            .arg("STREAMS")
            .arg(&source)
            .arg(">")
            .query_async(connection)
            .await
            .unwrap();
        let entry_id = reply.keys[0].ids[0].id.clone();
        Stage7bRedisSettlementContext::new(tag, source, ack, dlq, group, entry_id).unwrap()
    }

    async fn pending_len(
        connection: &mut ConnectionManager,
        context: &Stage7bRedisSettlementContext,
    ) -> usize {
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&context.source_stream)
            .arg(&context.consumer_group)
            .arg("-")
            .arg("+")
            .arg(100)
            .query_async(connection)
            .await
            .unwrap();
        pending.ids.len()
    }

    async fn stream_len(connection: &mut ConnectionManager, stream: &str) -> i64 {
        redis::cmd("XLEN")
            .arg(stream)
            .query_async(connection)
            .await
            .unwrap()
    }

    fn poison_plan(
        context: Stage7bRedisSettlementContext,
        raw_payload: &[u8],
        checkpoint: &str,
    ) -> Stage7bRedisDlqSettlementPlan {
        let evidence = classify_stage7a_permanent_pre_admission_poison(
            &context.redis_entry_id,
            Some(raw_payload),
        )
        .unwrap();
        let observation = poison_observation(context, evidence, checkpoint.to_string()).unwrap();
        dlq_plan(authorize_poison(observation, checkpoint).unwrap()).unwrap()
    }

    #[test]
    fn stage7b_d_b_b058_stable_transport_identity_never_uses_payload_fingerprint() {
        let context = Stage7bRedisSettlementContext::new(
            "stable-key",
            "finam_imoexf_paper:{stable-key}:commands",
            "finam_imoexf_paper:{stable-key}:acks",
            "finam_imoexf_paper:{stable-key}:dlq",
            "stage7b-stable-key",
            "100-1",
        )
        .unwrap();
        let first = ack_plan(authority(1, 'a'), context.clone()).unwrap();
        let second = ack_plan(authority(1, 'b'), context.clone()).unwrap();
        assert_ne!(
            first.canonical_payload_fingerprint,
            second.canonical_payload_fingerprint
        );
        assert_eq!(
            first.context.entry_marker_key(Stage7bSettlementKind::Ack),
            second.context.entry_marker_key(Stage7bSettlementKind::Ack)
        );
        for key in [
            &context.source_stream,
            &context.ack_stream,
            &context.dlq_stream,
        ] {
            assert!(key.contains("{stable-key}"));
        }
    }

    #[tokio::test]
    async fn stage7b_d_b_b057_atomic_ack_xadd_marker_and_xack() {
        let redis = RedisServer::start().await;
        let mut inspector = connection(&redis).await;
        let context = pending_context(&mut inspector, "b057", "command").await;
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        let outcome = backend
            .settle_ack(ack_plan(authority(57, 'a'), context.clone()).unwrap())
            .await
            .unwrap();
        assert_eq!(outcome.classification, "canonical");
        assert!(outcome.output_id.contains('-'));
        assert_eq!(pending_len(&mut inspector, &context).await, 0);
        assert_eq!(stream_len(&mut inspector, &context.ack_stream).await, 1);
        let marker: String = redis::cmd("GET")
            .arg(context.entry_marker_key(Stage7bSettlementKind::Ack))
            .query_async(&mut inspector)
            .await
            .unwrap();
        let marker: serde_json::Value = serde_json::from_str(&marker).unwrap();
        assert_eq!(marker["output_id"], outcome.output_id);
        assert!(backend.healthy());
    }

    #[tokio::test]
    async fn stage7b_d_b_b059_response_loss_exact_retry_is_idempotent() {
        let redis = RedisServer::start().await;
        let mut inspector = connection(&redis).await;
        let context = pending_context(&mut inspector, "b059", "command").await;
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        assert!(matches!(
            backend
                .settle_ack_with_lost_response(
                    ack_plan(authority(59, 'a'), context.clone()).unwrap()
                )
                .await,
            Err(Stage7bRedisSettlementError::ResponseLostAfterCommit)
        ));
        assert!(!backend.healthy());
        assert_eq!(pending_len(&mut inspector, &context).await, 0);
        assert_eq!(stream_len(&mut inspector, &context.ack_stream).await, 1);
        let recovered = backend
            .settle_ack(ack_plan(authority(59, 'a'), context.clone()).unwrap())
            .await
            .unwrap();
        assert_eq!(recovered.classification, "canonical");
        assert_eq!(stream_len(&mut inspector, &context.ack_stream).await, 1);
        assert!(backend.healthy());
    }

    #[tokio::test]
    async fn stage7b_d_b_seal_advanced_duplicate_and_true_identity_conflict() {
        let redis = RedisServer::start().await;
        let mut inspector = connection(&redis).await;
        let first = pending_context(&mut inspector, "duplicate", "command-1").await;
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        backend
            .settle_ack(ack_plan(authority(100, 'a'), first.clone()).unwrap())
            .await
            .unwrap();
        let duplicate = pending_context(&mut inspector, "duplicate", "command-2").await;
        let outcome = backend
            .settle_ack(
                ack_plan(
                    authority_with_fingerprints(100, 'b', 'a'),
                    duplicate.clone(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(outcome.classification, "duplicate");
        assert_eq!(stream_len(&mut inspector, &duplicate.ack_stream).await, 2);
        let conflict = pending_context(&mut inspector, "duplicate", "command-3").await;
        assert!(matches!(
            backend
                .settle_ack(
                    ack_plan(authority_with_fingerprints(100, 'c', 'c'), conflict.clone(),)
                        .unwrap()
                )
                .await,
            Err(Stage7bRedisSettlementError::Conflict)
        ));
        assert_eq!(pending_len(&mut inspector, &conflict).await, 1);
        assert_eq!(stream_len(&mut inspector, &conflict.ack_stream).await, 2);
        assert!(
            backend.healthy(),
            "semantic conflict is not a backend outage"
        );
    }

    #[tokio::test]
    async fn stage7b_d_b_new_settlement_requires_expected_pel_before_mutation() {
        let redis = RedisServer::start().await;
        let mut inspector = connection(&redis).await;
        let (source, ack, dlq, group) = names("pel");
        let _: () = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&source)
            .arg(&group)
            .arg("0-0")
            .arg("MKSTREAM")
            .query_async(&mut inspector)
            .await
            .unwrap();
        let entry_id: String = redis::cmd("XADD")
            .arg(&source)
            .arg("*")
            .arg("payload")
            .arg("not-pending")
            .query_async(&mut inspector)
            .await
            .unwrap();
        let context =
            Stage7bRedisSettlementContext::new("pel", source, ack, dlq, group, entry_id).unwrap();
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        assert!(matches!(
            backend
                .settle_ack(ack_plan(authority(200, 'a'), context.clone()).unwrap())
                .await,
            Err(Stage7bRedisSettlementError::SourceNotPending)
        ));
        assert_eq!(stream_len(&mut inspector, &context.ack_stream).await, 0);
        let marker: Option<String> = redis::cmd("GET")
            .arg(context.entry_marker_key(Stage7bSettlementKind::Ack))
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert!(marker.is_none());
    }

    #[tokio::test]
    async fn stage7b_d_b_b060_precommit_failure_keeps_pel_and_degrades_backend() {
        let redis = RedisServer::start().await;
        let mut inspector = connection(&redis).await;
        let context = pending_context(&mut inspector, "b060", "command").await;
        let _: () = redis::cmd("SET")
            .arg(&context.ack_stream)
            .arg("wrong-type")
            .query_async(&mut inspector)
            .await
            .unwrap();
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        assert!(matches!(
            backend
                .settle_ack(ack_plan(authority(60, 'a'), context.clone()).unwrap())
                .await,
            Err(Stage7bRedisSettlementError::Redis(_))
        ));
        assert_eq!(pending_len(&mut inspector, &context).await, 1);
        assert!(!backend.healthy());

        let _: i64 = redis::cmd("DEL")
            .arg(&context.ack_stream)
            .query_async(&mut inspector)
            .await
            .unwrap();
        let recovered = backend
            .settle_ack(ack_plan(authority(60, 'a'), context.clone()).unwrap())
            .await
            .unwrap();
        assert_eq!(recovered.classification, "canonical");
        assert_eq!(pending_len(&mut inspector, &context).await, 0);
        assert_eq!(stream_len(&mut inspector, &context.ack_stream).await, 1);
        assert!(
            backend.healthy(),
            "B-063 settlement health recovers only after an exact successful retry"
        );
    }

    #[tokio::test]
    async fn stage7b_d_b_b061_poison_dlq_is_redacted_atomic_and_checkpoint_bound() {
        let redis = RedisServer::start().await;
        let mut inspector = connection(&redis).await;
        let context = pending_context(&mut inspector, "b061", "raw-secret-value").await;
        let checkpoint = "c".repeat(64);
        let rejected_evidence =
            classify_stage7a_permanent_pre_admission_poison("999-0", Some(b"raw-secret-value"))
                .unwrap();
        let rejected = poison_observation(context.clone(), rejected_evidence, checkpoint.clone());
        assert!(matches!(
            rejected,
            Err(Stage7bRedisSettlementError::PoisonAuthorityDrift)
        ));
        let evidence = classify_stage7a_permanent_pre_admission_poison(
            &context.redis_entry_id,
            Some(b"raw-secret-value"),
        )
        .unwrap();
        let expected_sha = evidence.redacted_payload_sha256().to_string();
        let observation =
            poison_observation(context.clone(), evidence, checkpoint.clone()).unwrap();
        let authority = authorize_poison(observation, &checkpoint).unwrap();
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        backend
            .settle_dlq(dlq_plan(authority).unwrap())
            .await
            .unwrap();
        assert_eq!(pending_len(&mut inspector, &context).await, 0);
        let records: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&context.dlq_stream)
            .arg("-")
            .arg("+")
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(records.ids.len(), 1);
        let payload: String = records.ids[0].get("payload").unwrap();
        assert!(!payload.contains("raw-secret-value"));
        assert!(payload.contains(&expected_sha));
    }

    #[tokio::test]
    async fn stage7b_d_b_unrelated_success_does_not_heal_failed_entry() {
        let redis = RedisServer::start().await;
        let mut inspector = connection(&redis).await;
        let failed = pending_context(&mut inspector, "health-precommit", "command-a").await;
        let _: () = redis::cmd("SET")
            .arg(&failed.ack_stream)
            .arg("wrong-type")
            .query_async(&mut inspector)
            .await
            .unwrap();
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        assert!(matches!(
            backend
                .settle_ack(ack_plan(authority(610, 'a'), failed.clone()).unwrap())
                .await,
            Err(Stage7bRedisSettlementError::Redis(_))
        ));
        let _: i64 = redis::cmd("DEL")
            .arg(&failed.ack_stream)
            .query_async(&mut inspector)
            .await
            .unwrap();
        let unrelated = pending_context(&mut inspector, "health-precommit", "command-b").await;
        backend
            .settle_ack(ack_plan(authority(611, 'b'), unrelated).unwrap())
            .await
            .unwrap();
        assert!(!backend.healthy());
        assert_eq!(pending_len(&mut inspector, &failed).await, 1);
        backend
            .settle_ack(ack_plan(authority(610, 'a'), failed.clone()).unwrap())
            .await
            .unwrap();
        assert!(backend.healthy());
        assert_eq!(pending_len(&mut inspector, &failed).await, 0);
    }

    #[tokio::test]
    async fn stage7b_d_b_response_loss_is_entry_scoped_until_exact_marker_retry() {
        let redis = RedisServer::start().await;
        let mut inspector = connection(&redis).await;
        let uncertain = pending_context(&mut inspector, "health-lost", "command-a").await;
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        assert!(matches!(
            backend
                .settle_ack_with_lost_response(
                    ack_plan(authority(620, 'a'), uncertain.clone()).unwrap()
                )
                .await,
            Err(Stage7bRedisSettlementError::ResponseLostAfterCommit)
        ));
        let unrelated = pending_context(&mut inspector, "health-lost", "command-b").await;
        backend
            .settle_ack(ack_plan(authority(621, 'b'), unrelated).unwrap())
            .await
            .unwrap();
        assert!(!backend.healthy());
        backend
            .settle_ack(ack_plan(authority(620, 'a'), uncertain).unwrap())
            .await
            .unwrap();
        assert!(backend.healthy());
    }

    #[tokio::test]
    async fn stage7b_d_b_dlq_response_loss_is_entry_scoped_until_exact_retry() {
        let redis = RedisServer::start().await;
        let mut inspector = connection(&redis).await;
        let checkpoint = "d".repeat(64);
        let uncertain = pending_context(&mut inspector, "health-dlq", "not-json-a").await;
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        assert!(matches!(
            backend
                .settle_dlq_with_lost_response(poison_plan(
                    uncertain.clone(),
                    b"not-json-a",
                    &checkpoint
                ))
                .await,
            Err(Stage7bRedisSettlementError::ResponseLostAfterCommit)
        ));
        let unrelated = pending_context(&mut inspector, "health-dlq", "not-json-b").await;
        backend
            .settle_dlq(poison_plan(unrelated, b"not-json-b", &checkpoint))
            .await
            .unwrap();
        assert!(!backend.healthy());
        backend
            .settle_dlq(poison_plan(uncertain, b"not-json-a", &checkpoint))
            .await
            .unwrap();
        assert!(backend.healthy());
    }

    #[tokio::test]
    async fn stage7b_e_x15_dlq_outage_keeps_pel_and_degrades_backend() {
        let redis = RedisServer::start().await;
        let mut inspector = connection(&redis).await;
        let checkpoint = "e".repeat(64);
        let context = pending_context(&mut inspector, "x15-dlq-outage", "not-json").await;
        let _: () = redis::cmd("SET")
            .arg(&context.dlq_stream)
            .arg("wrong-type")
            .query_async(&mut inspector)
            .await
            .unwrap();
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        assert!(matches!(
            backend
                .settle_dlq(poison_plan(context.clone(), b"not-json", &checkpoint))
                .await,
            Err(Stage7bRedisSettlementError::Redis(_))
        ));
        assert!(!backend.healthy());
        assert_eq!(pending_len(&mut inspector, &context).await, 1);
        let unchanged: String = redis::cmd("GET")
            .arg(&context.dlq_stream)
            .query_async(&mut inspector)
            .await
            .unwrap();
        assert_eq!(unchanged, "wrong-type");
    }

    #[tokio::test]
    async fn stage7b_d_c_r2_legacy_or_incomplete_request_marker_fails_closed() {
        let redis = RedisServer::start().await;
        let mut inspector = connection(&redis).await;
        let context = pending_context(&mut inspector, "marker-schema", "command-a").await;
        let request_id = StrategyRequestId::from(Uuid::from_u128(701));
        let marker_key = context.request_marker_key(Some(request_id));
        let _: () = redis::cmd("SET")
            .arg(&marker_key)
            .arg(
                serde_json::json!({
                    "schema_version": MARKER_SCHEMA,
                    "request_identity": request_id.to_string(),
                    "terminal_request_ack_identity": "a".repeat(64),
                    "canonical_output_id": "1-0",
                    "publication_known": true
                })
                .to_string(),
            )
            .query_async(&mut inspector)
            .await
            .unwrap();
        let mut backend = Stage7bRedisSettlementBackend::connect(&redis.url)
            .await
            .unwrap();
        assert!(matches!(
            backend
                .lookup_canonical_request_publication(&context, request_id)
                .await,
            Err(Stage7bRedisSettlementError::InvalidRequestMarker)
        ));
        assert!(!backend.healthy());
        assert_eq!(pending_len(&mut inspector, &context).await, 1);
    }
}
