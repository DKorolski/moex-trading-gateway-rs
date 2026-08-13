#![allow(
    dead_code,
    reason = "Stage 7B-d-b is composed into the supervised consumer only in closed d-c"
)]

use super::Stage7bDurableAckAuthorized;
use broker_core::StrategyRequestId;
use redis::aio::ConnectionManager;
use serde::Serialize;
use sha2::{Digest, Sha256};
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
local authority_fp = ARGV[9]
local schema = tonumber(ARGV[10])

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
       or marker['canonical_ack_fingerprint'] ~= authority_fp then
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
    canonical_ack_fingerprint = authority_fp,
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
    canonical_ack_fingerprint_sha256: &'a str,
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
    authority_fingerprint: String,
    canonical_payload: String,
    canonical_payload_fingerprint: String,
    duplicate_payload: String,
    duplicate_payload_fingerprint: String,
}

pub(crate) struct Stage7bPreStage6PoisonObservation {
    context: Stage7bRedisSettlementContext,
    payload_len: usize,
    redacted_payload_sha256: String,
    stage6_checkpoint_sha256: String,
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
        canonical_ack_fingerprint_sha256: &authority.canonical_ack_fingerprint_sha256,
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
        canonical_ack_fingerprint_sha256: &authority.canonical_ack_fingerprint_sha256,
        publication: "duplicate",
    })?;
    Ok(Stage7bRedisAckSettlementPlan {
        context,
        request_id: authority.strategy_request_id,
        authority_fingerprint: authority.canonical_ack_fingerprint_sha256,
        canonical_payload_fingerprint: sha256_hex(canonical_payload.as_bytes()),
        duplicate_payload_fingerprint: sha256_hex(duplicate_payload.as_bytes()),
        canonical_payload,
        duplicate_payload,
    })
}

pub(super) fn poison_observation(
    context: Stage7bRedisSettlementContext,
    raw_payload: &[u8],
    stage6_checkpoint_sha256: String,
) -> Stage7bPreStage6PoisonObservation {
    Stage7bPreStage6PoisonObservation {
        context,
        payload_len: raw_payload.len(),
        redacted_payload_sha256: sha256_hex(raw_payload),
        stage6_checkpoint_sha256,
    }
}

pub(super) fn authorize_poison(
    observation: Stage7bPreStage6PoisonObservation,
    raw_payload: &[u8],
    poison_reason: &str,
    current_stage6_checkpoint_sha256: &str,
) -> Result<Stage7bPoisonDlqAuthorized, Stage7bRedisSettlementError> {
    if !poison_reason_token(poison_reason)
        || observation.payload_len != raw_payload.len()
        || observation.redacted_payload_sha256 != sha256_hex(raw_payload)
        || observation.stage6_checkpoint_sha256 != current_stage6_checkpoint_sha256
    {
        return Err(Stage7bRedisSettlementError::PoisonAuthorityDrift);
    }
    Ok(Stage7bPoisonDlqAuthorized {
        context: observation.context,
        poison_reason: poison_reason.to_string(),
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
    healthy: bool,
}

impl Stage7bRedisSettlementBackend {
    pub(crate) async fn connect(redis_url: &str) -> Result<Self, Stage7bRedisSettlementError> {
        let client = redis::Client::open(redis_url)?;
        let connection = ConnectionManager::new(client).await?;
        Ok(Self {
            connection,
            healthy: true,
        })
    }

    pub(crate) fn healthy(&self) -> bool {
        self.healthy
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
                authority_fingerprint: &plan.authority_fingerprint,
            },
        )
        .await;
        self.finish(result, lose_response_after_commit)
    }

    pub(super) async fn settle_dlq(
        &mut self,
        plan: Stage7bRedisDlqSettlementPlan,
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
                authority_fingerprint: "",
            },
        )
        .await;
        self.finish(result, false)
    }

    fn finish(
        &mut self,
        result: Result<Stage7bRedisSettlementOutcome, Stage7bRedisSettlementError>,
        lose_response_after_commit: bool,
    ) -> Result<Stage7bRedisSettlementOutcome, Stage7bRedisSettlementError> {
        match result {
            Ok(_) if lose_response_after_commit => {
                self.healthy = false;
                Err(Stage7bRedisSettlementError::ResponseLostAfterCommit)
            }
            Ok(outcome) => {
                self.healthy = true;
                Ok(outcome)
            }
            Err(error) => {
                if !matches!(
                    error,
                    Stage7bRedisSettlementError::Conflict
                        | Stage7bRedisSettlementError::SourceNotPending
                ) {
                    self.healthy = false;
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
    authority_fingerprint: &'a str,
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
        .arg(invocation.authority_fingerprint)
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

fn poison_reason_token(value: &str) -> bool {
    token(value) && value.starts_with("permanent_")
}

fn stream_id(value: &str) -> bool {
    let Some((milliseconds, sequence)) = value.split_once('-') else {
        return false;
    };
    milliseconds.parse::<u64>().is_ok()
        && sequence.parse::<u64>().is_ok()
        && !value.contains(char::is_whitespace)
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
            canonical_ack_fingerprint_sha256: canonical_byte.to_string().repeat(64),
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
    async fn stage7b_d_b_later_exact_entry_is_duplicate_and_conflict_stays_pending() {
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
            .settle_ack(ack_plan(authority(100, 'a'), duplicate.clone()).unwrap())
            .await
            .unwrap();
        assert_eq!(outcome.classification, "duplicate");
        assert_eq!(stream_len(&mut inspector, &duplicate.ack_stream).await, 2);
        let conflict = pending_context(&mut inspector, "duplicate", "command-3").await;
        assert!(matches!(
            backend
                .settle_ack(ack_plan(authority(100, 'b'), conflict.clone()).unwrap())
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
        let rejected = poison_observation(context.clone(), b"raw-secret-value", checkpoint.clone());
        assert!(matches!(
            authorize_poison(
                rejected,
                b"changed-secret",
                "permanent_invalid_json",
                &checkpoint
            ),
            Err(Stage7bRedisSettlementError::PoisonAuthorityDrift)
        ));
        let observation =
            poison_observation(context.clone(), b"raw-secret-value", checkpoint.clone());
        let authority = authorize_poison(
            observation,
            b"raw-secret-value",
            "permanent_invalid_json",
            &checkpoint,
        )
        .unwrap();
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
        assert!(payload.contains(&sha256_hex(b"raw-secret-value")));
    }
}
