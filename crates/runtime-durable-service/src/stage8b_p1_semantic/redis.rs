//! Stage 8B-P1-c real-Redis semantic source and command-publication boundary.
//!
//! This module deliberately stops at a canonical command stream entry. It
//! owns no paper provider, FINAM transport, broker dispatch or runtime-live
//! operation. Operational DB0 activation is a separate deployment gate.

use super::{
    binding_from_delivery, parse_stage8b_p1_canonical_m10, Stage8bP1CanonicalM10Error,
    Stage8bP1M10SemanticIdentityV1, Stage8bP1PendingM10Delivery, Stage8bP1SemanticCompositionError,
    Stage8bP1ValidatedCanonicalM10,
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
use chrono::{DateTime, Utc};
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

#[cfg(test)]
std::thread_local! {
    static P1E_I0_POST_PERMIT_PARSE_AUDIT: std::cell::Cell<Option<u64>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(test)]
fn p1e_i0_begin_post_permit_parse_audit() {
    P1E_I0_POST_PERMIT_PARSE_AUDIT.with(|audit| audit.set(Some(0)));
}

#[cfg(test)]
fn p1e_i0_take_post_permit_parse_audit() -> u64 {
    P1E_I0_POST_PERMIT_PARSE_AUDIT.with(|audit| {
        let observed = audit
            .get()
            .expect("P1-e post-permit parse audit must be active");
        audit.set(None);
        observed
    })
}

fn parse_exact_after_p1e_permit(
    delivery: &Stage8bP1PendingM10Delivery,
    expected_operational_identity_sha256: &str,
) -> Result<Stage8bP1ValidatedCanonicalM10, Stage8bP1CanonicalM10Error> {
    #[cfg(test)]
    P1E_I0_POST_PERMIT_PARSE_AUDIT.with(|audit| {
        if let Some(observed) = audit.get() {
            audit.set(Some(observed + 1));
        }
    });
    delivery.parse_exact(expected_operational_identity_sha256)
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
struct P1eI0ObservedEffectCountersV1 {
    replacement_seal_commit_total: u64,
    callback_total: u64,
    publication_total: u64,
    publication_revalidation_total: u64,
    xack_total: u64,
    timer_reclassification_total: u64,
    timer_execution_total: u64,
}

#[cfg(test)]
std::thread_local! {
    static P1E_I0_EFFECT_AUDIT: std::cell::Cell<Option<P1eI0ObservedEffectCountersV1>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(test)]
fn p1e_i0_begin_effect_audit() {
    P1E_I0_EFFECT_AUDIT.with(|audit| {
        assert!(
            audit.get().is_none(),
            "P1-e I0 effect audit is already active"
        );
        audit.set(Some(P1eI0ObservedEffectCountersV1::default()));
    });
}

#[cfg(test)]
fn p1e_i0_observe_effect(update: impl FnOnce(&mut P1eI0ObservedEffectCountersV1)) {
    P1E_I0_EFFECT_AUDIT.with(|audit| {
        if let Some(mut observed) = audit.get() {
            update(&mut observed);
            audit.set(Some(observed));
        }
    });
}

#[cfg(test)]
fn p1e_i0_take_effect_audit() -> P1eI0ObservedEffectCountersV1 {
    P1E_I0_EFFECT_AUDIT.with(|audit| {
        let observed = audit.get().expect("P1-e I0 effect audit must be active");
        audit.set(None);
        observed
    })
}

#[cfg(test)]
fn p1e_i0_observe_replacement_seal_commit() {
    p1e_i0_observe_effect(|observed| observed.replacement_seal_commit_total += 1);
}

#[cfg(test)]
fn p1e_i0_observe_callback() {
    p1e_i0_observe_effect(|observed| observed.callback_total += 1);
}

#[cfg(test)]
fn p1e_i0_observe_publication() {
    p1e_i0_observe_effect(|observed| observed.publication_total += 1);
}

#[cfg(test)]
fn p1e_i0_observe_publication_revalidation() {
    p1e_i0_observe_effect(|observed| observed.publication_revalidation_total += 1);
}

#[cfg(test)]
fn p1e_i0_observe_xack() {
    p1e_i0_observe_effect(|observed| observed.xack_total += 1);
}

#[cfg(test)]
fn p1e_i0_observe_timer_reclassification() {
    p1e_i0_observe_effect(|observed| observed.timer_reclassification_total += 1);
}

#[cfg(test)]
fn p1e_i0_observe_timer_execution() {
    p1e_i0_observe_effect(|observed| observed.timer_execution_total += 1);
}

#[cfg(test)]
struct P1eI0AcquisitionBarrierV1 {
    entered: tokio::sync::oneshot::Sender<()>,
    release: tokio::sync::oneshot::Receiver<()>,
    entered_total: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(test)]
std::thread_local! {
    static P1E_I0_ACQUISITION_BARRIER: std::cell::RefCell<Option<P1eI0AcquisitionBarrierV1>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn p1e_i0_arm_acquisition_barrier() -> (
    tokio::sync::oneshot::Receiver<()>,
    tokio::sync::oneshot::Sender<()>,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    let (entered_sender, entered_receiver) = tokio::sync::oneshot::channel();
    let (release_sender, release_receiver) = tokio::sync::oneshot::channel();
    let entered_total = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    P1E_I0_ACQUISITION_BARRIER.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(slot.is_none(), "P1-e acquisition barrier is already armed");
        *slot = Some(P1eI0AcquisitionBarrierV1 {
            entered: entered_sender,
            release: release_receiver,
            entered_total: entered_total.clone(),
        });
    });
    (entered_receiver, release_sender, entered_total)
}

#[cfg(test)]
async fn p1e_i0_pause_acquisition_if_armed() {
    let barrier = P1E_I0_ACQUISITION_BARRIER.with(|slot| slot.borrow_mut().take());
    if let Some(barrier) = barrier {
        barrier
            .entered_total
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        barrier
            .entered
            .send(())
            .expect("P1-e acquisition-entry witness must be observed");
        barrier
            .release
            .await
            .expect("P1-e acquisition barrier must be released");
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum P1d4ObservedEffectEvent {
    P1d3Provider,
    P1d3Schedule,
    GeneratedPublication,
    GeneratedSchedule,
    GeneratedDispatch,
    GeneratedProvider,
    GeneratedOrder,
    GeneratedTrade,
    GeneratedRequestFinalized,
    GeneratedSAck(u64),
    GeneratedSTruth(u64),
    GeneratedXack,
}

#[cfg(test)]
impl P1d4ObservedEffectEvent {
    fn label(&self) -> &'static str {
        match self {
            Self::P1d3Provider => "p1d3_provider",
            Self::P1d3Schedule => "p1d3_schedule",
            Self::GeneratedPublication => "generated_publication",
            Self::GeneratedSchedule => "generated_schedule",
            Self::GeneratedDispatch => "generated_dispatch",
            Self::GeneratedProvider => "generated_provider",
            Self::GeneratedOrder => "generated_order",
            Self::GeneratedTrade => "generated_trade",
            Self::GeneratedRequestFinalized => "generated_request_finalized",
            Self::GeneratedSAck(_) => "generated_s_ack",
            Self::GeneratedSTruth(_) => "generated_s_truth",
            Self::GeneratedXack => "generated_xack",
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct P1d4ObservedEffectAudit {
    capture_p1d3: bool,
    capture_generated_market: bool,
    events: Vec<P1d4ObservedEffectEvent>,
}

#[cfg(test)]
fn p1d4_effect_audit() -> &'static std::sync::Mutex<Option<P1d4ObservedEffectAudit>> {
    static AUDIT: std::sync::OnceLock<std::sync::Mutex<Option<P1d4ObservedEffectAudit>>> =
        std::sync::OnceLock::new();
    AUDIT.get_or_init(|| std::sync::Mutex::new(None))
}

#[cfg(test)]
fn p1d4_begin_observed_effect_audit(capture_p1d3: bool) {
    *p1d4_effect_audit().lock().expect("P1-d4 effect audit lock") = Some(P1d4ObservedEffectAudit {
        capture_p1d3,
        capture_generated_market: !capture_p1d3,
        ..P1d4ObservedEffectAudit::default()
    });
}

#[cfg(test)]
fn p1d4_take_observed_effect_audit() -> P1d4ObservedEffectAudit {
    p1d4_effect_audit()
        .lock()
        .expect("P1-d4 effect audit lock")
        .take()
        .expect("P1-d4 effect audit must be active")
}

#[cfg(test)]
fn p1d4_observe_p1d3_provider() {
    if let Some(audit) = p1d4_effect_audit()
        .lock()
        .expect("P1-d4 effect audit lock")
        .as_mut()
    {
        if audit.capture_p1d3 {
            audit.events.push(P1d4ObservedEffectEvent::P1d3Provider);
        }
    }
}

#[cfg(test)]
fn p1d4_observe_p1d3_schedule() {
    if let Some(audit) = p1d4_effect_audit()
        .lock()
        .expect("P1-d4 effect audit lock")
        .as_mut()
    {
        if audit.capture_p1d3 {
            audit.events.push(P1d4ObservedEffectEvent::P1d3Schedule);
        }
    }
}

#[cfg(test)]
fn p1d4_observe_generated_market_provider() {
    if let Some(audit) = p1d4_effect_audit()
        .lock()
        .expect("P1-d4 effect audit lock")
        .as_mut()
    {
        if audit.capture_generated_market {
            audit
                .events
                .push(P1d4ObservedEffectEvent::GeneratedProvider);
        }
    }
}

#[cfg(test)]
fn p1d4_observe_generated_market_schedule() {
    if let Some(audit) = p1d4_effect_audit()
        .lock()
        .expect("P1-d4 effect audit lock")
        .as_mut()
    {
        if audit.capture_generated_market {
            audit
                .events
                .push(P1d4ObservedEffectEvent::GeneratedSchedule);
        }
    }
}

#[cfg(test)]
fn p1d4_observe_generated_market_s_ack(generation: u64) {
    if let Some(audit) = p1d4_effect_audit()
        .lock()
        .expect("P1-d4 effect audit lock")
        .as_mut()
    {
        if audit.capture_generated_market {
            audit
                .events
                .push(P1d4ObservedEffectEvent::GeneratedSAck(generation));
        }
    }
}

#[cfg(test)]
fn p1d4_observe_generated_market_s_truth(generation: u64) {
    if let Some(audit) = p1d4_effect_audit()
        .lock()
        .expect("P1-d4 effect audit lock")
        .as_mut()
    {
        if audit.capture_generated_market {
            audit
                .events
                .push(P1d4ObservedEffectEvent::GeneratedSTruth(generation));
        }
    }
}

#[cfg(test)]
fn p1d4_observe_generated_market_event(event: P1d4ObservedEffectEvent) {
    if let Some(audit) = p1d4_effect_audit()
        .lock()
        .expect("P1-d4 effect audit lock")
        .as_mut()
    {
        if audit.capture_generated_market {
            audit.events.push(event);
        }
    }
}

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

const COMMAND_PUBLICATION_REVALIDATE_LUA: &str = r#"
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
if type_name(source) ~= 'stream' or type_name(command_stream) ~= 'stream' then
  return redis.error_reply('STAGE8B_P1_STREAM_TYPE')
end
if not has_group(command_stream, command_group) then
  return redis.error_reply('STAGE8B_P1_COMMAND_GROUP_MISSING')
end
if type_name(marker_key) ~= 'string' then
  return redis.error_reply('STAGE8B_P1_MARKER_MISSING')
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

local ok, marker = pcall(cjson.decode, redis.call('GET', marker_key))
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
    #[error("Stage 8B-P1-e continuation permit does not match the requested route")]
    P1eContinuationPermitRouteMismatch,
    #[error("Stage 8B-P1-e trusted verification and durable binding clocks differ")]
    P1eScheduleClockMismatch,
    #[error("Stage 8B-P1-e signed schedule retry policy is invalid")]
    P1eScheduleRetryPolicyInvalid,
    #[error("Stage 8B-P1-e durable schedule high-water conflicts with the Ready owner")]
    P1eScheduleHighWaterConflict,
    #[error("Stage 8B-P1-e signed schedule composition failed: {0}")]
    P1eSchedule(#[from] crate::Stage8bP1eScheduleReadError),
}

struct Stage8bP1RedisBackend {
    connection: ConnectionManager,
    namespace: Stage8bP1RedisNamespace,
    config: Stage8bP1RedisConfig,
    claim_cursor: String,
    delivery_generation: u64,
    groups_verified: bool,
}

enum Stage8bP1ReadySourceAcquisition {
    Delivery(Box<Stage8bP1PendingM10Delivery>),
    PendingNotClaimable(String),
}

enum Stage8bP1eReadyPendingAcquisitionV1 {
    Delivery {
        delivery: Box<Stage8bP1PendingM10Delivery>,
        acquisition_kind: Stage8bP1eAcquisitionKindV1,
    },
    NoPending,
    PendingNotClaimable(String),
}

enum Stage8bP1eReadyFreshAcquisitionV1 {
    Delivery(Box<Stage8bP1PendingM10Delivery>),
    EmptyFreshPoll,
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
        delivery_generation: 0,
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

    pub(crate) fn operational_identity_sha256(&self) -> &str {
        self.stage7.stage8b_p1_operational_identity_sha256()
    }

    pub(crate) fn recover_stage8b_p1e_latest_schedule_high_water(
        &self,
        expected_runtime_config_fingerprint_sha256: &str,
        expected_instrument_map_fingerprint_sha256: &str,
    ) -> Result<
        Option<strategy_runtime_core::Stage8bP1eScheduleHighWaterV1>,
        Stage8bP1RedisSemanticError,
    > {
        self.stage7
            .recover_stage8b_p1e_latest_schedule_high_water(
                expected_runtime_config_fingerprint_sha256,
                expected_instrument_map_fingerprint_sha256,
            )
            .map_err(Into::into)
    }

    #[cfg(any(test, feature = "stage8a4-i3-test-fixtures"))]
    #[allow(dead_code, reason = "fixture trust is exercised only by restart tests")]
    pub(crate) fn stage8b_p1e_test_recover_latest_schedule_high_water_with_key(
        &self,
        expected_runtime_config_fingerprint_sha256: &str,
        expected_instrument_map_fingerprint_sha256: &str,
        public_key_hex: &str,
        key_valid_from: DateTime<Utc>,
        key_valid_until: DateTime<Utc>,
    ) -> Result<
        Option<strategy_runtime_core::Stage8bP1eScheduleHighWaterV1>,
        Stage8bP1RedisSemanticError,
    > {
        self.stage7
            .stage8b_p1e_test_recover_latest_schedule_high_water_with_key(
                expected_runtime_config_fingerprint_sha256,
                expected_instrument_map_fingerprint_sha256,
                public_key_hex,
                key_valid_from,
                key_valid_until,
            )
            .map_err(Into::into)
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
        self.process_claimed_ready(*delivery, commitment_key).await
    }

    async fn process_claimed_ready(
        mut self,
        delivery: Stage8bP1PendingM10Delivery,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisSemanticOutcome, Stage8bP1RedisSemanticError> {
        let operational_identity_sha256 = self
            .stage7
            .stage8b_p1_operational_identity_sha256()
            .to_string();
        let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
        let accepted_bar = parse_exact_after_p1e_permit(&delivery, &operational_identity_sha256)?
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
        #[cfg(test)]
        p1d4_observe_p1d3_schedule();
        let delivery = match self.transport.backend.acquire_ready_delivery().await? {
            Stage8bP1ReadySourceAcquisition::Delivery(delivery) => delivery,
            Stage8bP1ReadySourceAcquisition::PendingNotClaimable(redis_id) => {
                return Ok(Stage8bP1RedisSemanticOutcome::PendingNotClaimable {
                    owner: Box::new(self),
                    pending_m10_redis_id: redis_id,
                });
            }
        };
        self.process_claimed_working_limit(*delivery, schedule_authority, commitment_key)
            .await
    }

    async fn process_claimed_working_limit(
        self,
        delivery: Stage8bP1PendingM10Delivery,
        schedule_authority: Stage8bP1d3ScheduleStepAuthority,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisSemanticOutcome, Stage8bP1RedisSemanticError> {
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
        #[cfg(test)]
        p1d4_observe_p1d3_schedule();
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
    #[cfg(test)]
    p1e_i0_observe_callback();
    match outcome {
        Stage8bP1SemanticCommitOutcome::ZeroIntent { owner, receipt } => {
            #[cfg(test)]
            p1e_i0_observe_replacement_seal_commit();
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
            #[cfg(test)]
            p1e_i0_observe_replacement_seal_commit();
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
        #[cfg(test)]
        p1d4_observe_generated_market_event(P1d4ObservedEffectEvent::GeneratedPublication);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1ePublishedScheduleRouteV1 {
    PlainMarket,
    GeneratedMarket,
    InitialLimit,
    Unsupported,
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
    pub fn p1e_schedule_route(&self) -> Stage8bP1ePublishedScheduleRouteV1 {
        let publication_pair = (self.p1d4_reservation.is_some(), self.p1d4_binding.is_some());
        match (&self.command, publication_pair) {
            (BrokerCommand::PlaceOrder(place), (false, false))
                if place.order_type == broker_core::OrderType::Market =>
            {
                Stage8bP1ePublishedScheduleRouteV1::PlainMarket
            }
            (BrokerCommand::PlaceOrder(place), (true, true))
                if place.order_type == broker_core::OrderType::Market =>
            {
                Stage8bP1ePublishedScheduleRouteV1::GeneratedMarket
            }
            (BrokerCommand::PlaceOrder(place), (false, false))
                if place.order_type == broker_core::OrderType::Limit =>
            {
                Stage8bP1ePublishedScheduleRouteV1::InitialLimit
            }
            _ => Stage8bP1ePublishedScheduleRouteV1::Unsupported,
        }
    }

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
        #[cfg(test)]
        p1d4_observe_generated_market_schedule();
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
        #[cfg(test)]
        p1d4_observe_generated_market_event(P1d4ObservedEffectEvent::GeneratedDispatch);
        crate::recovery::stage8b_p1d4_test_crash_frontier("GM03");
        crate::recovery::stage8b_p1_test_crash_barrier(
            "p1d4-market-after-dispatch-before-provider-outcome",
        );
        #[cfg(test)]
        p1d4_observe_generated_market_provider();
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
        #[cfg(test)]
        for event in [
            P1d4ObservedEffectEvent::GeneratedOrder,
            P1d4ObservedEffectEvent::GeneratedTrade,
            P1d4ObservedEffectEvent::GeneratedRequestFinalized,
        ] {
            p1d4_observe_generated_market_event(event);
        }
        #[cfg(test)]
        p1d4_observe_generated_market_s_ack(durable.recovery_seal_generation());
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
        #[cfg(test)]
        p1d4_observe_p1d3_schedule();
        #[cfg(test)]
        p1d4_observe_p1d3_provider();
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
        #[cfg(test)]
        p1d4_observe_p1d3_schedule();
        #[cfg(test)]
        p1d4_observe_p1d3_provider();
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
        #[cfg(test)]
        p1d4_observe_p1d3_schedule();
        #[cfg(test)]
        p1d4_observe_p1d3_provider();
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
        #[cfg(test)]
        p1d4_observe_generated_market_s_truth(durable.recovery_seal_generation());
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
        #[cfg(test)]
        p1d4_observe_generated_market_event(P1d4ObservedEffectEvent::GeneratedXack);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eShutdownCauseV1 {
    ExternalSignal,
    OwnerFailure,
    TelemetryFailure,
    SignalTaskFailure,
}

impl Stage8bP1eShutdownCauseV1 {
    pub const fn exit_class(self) -> u8 {
        match self {
            Self::ExternalSignal => 0,
            Self::OwnerFailure => 70,
            Self::TelemetryFailure => 71,
            Self::SignalTaskFailure => 73,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1eShutdownIntentV1 {
    cause: Stage8bP1eShutdownCauseV1,
    final_exit_class: u8,
    grace_deadline_utc_ms: i64,
    first_request_sequence: u64,
}

impl Stage8bP1eShutdownIntentV1 {
    pub fn new(
        cause: Stage8bP1eShutdownCauseV1,
        grace_deadline_utc_ms: i64,
        first_request_sequence: u64,
    ) -> Self {
        Self {
            cause,
            final_exit_class: cause.exit_class(),
            grace_deadline_utc_ms,
            first_request_sequence,
        }
    }

    pub const fn cause(&self) -> Stage8bP1eShutdownCauseV1 {
        self.cause
    }

    pub const fn final_exit_class(&self) -> u8 {
        self.final_exit_class
    }

    pub const fn grace_deadline_utc_ms(&self) -> i64 {
        self.grace_deadline_utc_ms
    }

    pub const fn first_request_sequence(&self) -> u64 {
        self.first_request_sequence
    }

    pub const fn bounded_exit_class(&self, now_utc_ms: i64) -> u8 {
        if now_utc_ms >= self.grace_deadline_utc_ms {
            72
        } else {
            self.final_exit_class
        }
    }
}

#[derive(Default)]
pub struct Stage8bP1eShutdownLatchV1 {
    intent: std::sync::OnceLock<Stage8bP1eShutdownIntentV1>,
}

impl Stage8bP1eShutdownLatchV1 {
    pub const fn new() -> Self {
        Self {
            intent: std::sync::OnceLock::new(),
        }
    }

    /// First request wins across all process tasks. Later requests are
    /// diagnostic-only and cannot replace the initiating cause, sequence,
    /// deadline or exit class.
    pub fn request(&self, intent: Stage8bP1eShutdownIntentV1) -> bool {
        self.intent.set(intent).is_ok()
    }

    pub fn intent(&self) -> Option<&Stage8bP1eShutdownIntentV1> {
        self.intent.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage8bP1eAcquisitionKindV1 {
    ReclaimedPending,
    ReclaimedReady,
    FreshReady,
    ObservedTerminal,
}

enum Stage8bP1d4JournalAheadPending {
    Dispatch(Stage8bP1d4GeneratedMarketDispatchPendingOwner),
    Order(Stage8bP1d4GeneratedMarketOrderPendingOwner),
    PreFinalization(Stage8bP1d4GeneratedMarketPreFinalizationPendingOwner),
    PreAck(Stage8bP1d4GeneratedMarketPreAckPendingOwner),
}

enum Stage8bP1ePostAcquisitionRouteV1 {
    ReadySource {
        claimed: Stage8bP1eClaimedM10DeliveryV2,
    },
    ReadyWorkingLimit {
        claimed: Stage8bP1eClaimedM10DeliveryV2,
    },
    ZeroIntentAck {
        pending: P1SemanticZeroIntentAckPending,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    JournalAhead {
        pending: Box<P1SemanticPrepublicationPending>,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    Prepublication {
        durable: Stage8bP1SemanticPrepublicationOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d4Prepublication {
        durable: Stage8bP1d4GeneratedMarketPrepublicationOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d4JournalAhead {
        durable: Stage8bP1d4JournalAheadPending,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d4Ack {
        durable: Stage8bP1d4GeneratedMarketAckCommittedOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d4Truth {
        durable: Stage8bP1d4GeneratedMarketTruthCommittedOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d2Ack {
        durable: Stage8bP1d2AckCommittedOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d2PreAck {
        durable: Stage8bP1d2PreAckPendingOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d2Truth {
        durable: Stage8bP1d2TruthCommittedOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d3PreAck {
        durable: Stage8bP1d3PreAckPendingOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d3DispatchLimit {
        durable: Stage8bP1d3DispatchPendingOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d3DispatchExpiry {
        durable: Stage8bP1d3DispatchPendingOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d3DispatchCancel {
        durable: Stage8bP1d3DispatchPendingOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d3Ack {
        durable: Stage8bP1d3AckCommittedOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d3Truth {
        durable: Stage8bP1d3TruthCommittedOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d3CancelContinuation {
        durable: Stage8bP1d3CancelContinuationOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
    P1d3Semantic {
        durable: Stage8bP1d3SemanticPendingOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        pending_m10: Stage8bP1PendingM10Delivery,
    },
}

impl Stage8bP1ePostAcquisitionRouteV1 {
    fn route_id(&self) -> &'static str {
        match self {
            Self::ReadySource { .. } | Self::ReadyWorkingLimit { .. } => "S08",
            Self::JournalAhead { .. } => "LR01",
            Self::Prepublication { .. } => "LR02",
            Self::P1d2PreAck { .. } => "LR03",
            Self::P1d2Ack { .. } => "LR04",
            Self::P1d4Prepublication { .. } => "LR05",
            Self::P1d4JournalAhead {
                durable: Stage8bP1d4JournalAheadPending::Dispatch(_),
                ..
            } => "LR06",
            Self::P1d4JournalAhead {
                durable: Stage8bP1d4JournalAheadPending::Order(_),
                ..
            } => "LR07",
            Self::P1d4JournalAhead {
                durable: Stage8bP1d4JournalAheadPending::PreFinalization(_),
                ..
            } => "LR08",
            Self::P1d4JournalAhead {
                durable: Stage8bP1d4JournalAheadPending::PreAck(_),
                ..
            } => "LR09",
            Self::P1d4Ack { .. } => "LR10",
            Self::P1d3DispatchLimit { .. }
            | Self::P1d3DispatchExpiry { .. }
            | Self::P1d3DispatchCancel { .. } => "LR11",
            Self::P1d3PreAck { .. } => "LR12",
            Self::P1d3Ack { .. } => "LR13",
            Self::P1d3CancelContinuation { .. } => "LR14",
            Self::P1d3Semantic { .. } => "LR15",
            Self::ZeroIntentAck { .. } => "LT01",
            Self::P1d2Truth { .. } => "LT02",
            Self::P1d4Truth { .. } => "LT03",
            Self::P1d3Truth { .. } => "LT04/LT05",
        }
    }

    fn continuation_kind(&self) -> Stage8bP1eContinuationRouteKindV1 {
        match self {
            Self::ReadySource { .. } => Stage8bP1eContinuationRouteKindV1::ReadySemantic,
            Self::ReadyWorkingLimit { .. } => Stage8bP1eContinuationRouteKindV1::ReadyWorkingLimit,
            Self::ZeroIntentAck { .. } => Stage8bP1eContinuationRouteKindV1::ZeroIntentAck,
            Self::JournalAhead { .. } => Stage8bP1eContinuationRouteKindV1::JournalAhead,
            Self::Prepublication { .. } => Stage8bP1eContinuationRouteKindV1::Prepublication,
            Self::P1d4Prepublication { .. } => {
                Stage8bP1eContinuationRouteKindV1::P1d4Prepublication
            }
            Self::P1d4JournalAhead {
                durable: Stage8bP1d4JournalAheadPending::Dispatch(_),
                ..
            } => Stage8bP1eContinuationRouteKindV1::P1d4DispatchPending,
            Self::P1d4JournalAhead {
                durable: Stage8bP1d4JournalAheadPending::Order(_),
                ..
            } => Stage8bP1eContinuationRouteKindV1::P1d4OrderPending,
            Self::P1d4JournalAhead {
                durable: Stage8bP1d4JournalAheadPending::PreFinalization(_),
                ..
            } => Stage8bP1eContinuationRouteKindV1::P1d4PreFinalizationPending,
            Self::P1d4JournalAhead {
                durable: Stage8bP1d4JournalAheadPending::PreAck(_),
                ..
            } => Stage8bP1eContinuationRouteKindV1::P1d4PreAckPending,
            Self::P1d4Ack { .. } => Stage8bP1eContinuationRouteKindV1::P1d4Ack,
            Self::P1d4Truth { .. } => Stage8bP1eContinuationRouteKindV1::P1d4Truth,
            Self::P1d2Ack { .. } => Stage8bP1eContinuationRouteKindV1::P1d2Ack,
            Self::P1d2PreAck { .. } => Stage8bP1eContinuationRouteKindV1::P1d2PreAck,
            Self::P1d2Truth { .. } => Stage8bP1eContinuationRouteKindV1::P1d2Truth,
            Self::P1d3PreAck { .. } => Stage8bP1eContinuationRouteKindV1::P1d3PreAck,
            Self::P1d3DispatchLimit { .. } => Stage8bP1eContinuationRouteKindV1::P1d3DispatchLimit,
            Self::P1d3DispatchExpiry { .. } => {
                Stage8bP1eContinuationRouteKindV1::P1d3DispatchExpiry
            }
            Self::P1d3DispatchCancel { .. } => {
                Stage8bP1eContinuationRouteKindV1::P1d3DispatchCancel
            }
            Self::P1d3Ack { .. } => Stage8bP1eContinuationRouteKindV1::P1d3Ack,
            Self::P1d3Truth { .. } => Stage8bP1eContinuationRouteKindV1::P1d3Truth,
            Self::P1d3CancelContinuation { .. } => {
                Stage8bP1eContinuationRouteKindV1::P1d3CancelContinuation
            }
            Self::P1d3Semantic { .. } => Stage8bP1eContinuationRouteKindV1::P1d3Semantic,
        }
    }
}

/// Opaque linear owner created immediately after one exact Redis acquisition.
/// It deliberately exposes neither source bytes nor transport/durable parts.
///
/// ```compile_fail
/// fn requires_clone<T: Clone>() {}
/// requires_clone::<runtime_durable_service::Stage8bP1ePostAcquisitionOwnerV1>();
/// ```
///
/// ```compile_fail
/// fn requires_serialize<T: serde::Serialize>() {}
/// requires_serialize::<runtime_durable_service::Stage8bP1ePostAcquisitionOwnerV1>();
/// ```
///
/// ```compile_fail
/// fn requires_deserialize<T: serde::de::DeserializeOwned>() {}
/// requires_deserialize::<runtime_durable_service::Stage8bP1ePostAcquisitionOwnerV1>();
/// ```
///
/// ```compile_fail
/// use runtime_durable_service::Stage8bP1ePostAcquisitionOwnerV1;
/// fn split(owner: Stage8bP1ePostAcquisitionOwnerV1) {
///     let Stage8bP1ePostAcquisitionOwnerV1 { route, acquisition_kind } = owner;
/// }
/// ```
///
/// ```compile_fail
/// use runtime_durable_service::Stage8bP1ePostAcquisitionOwnerV1;
/// fn extract(owner: &Stage8bP1ePostAcquisitionOwnerV1) {
///     let _ = owner.route();
/// }
/// ```
pub struct Stage8bP1ePostAcquisitionOwnerV1 {
    route: Box<Stage8bP1ePostAcquisitionRouteV1>,
    acquisition_kind: Stage8bP1eAcquisitionKindV1,
}

/// Linear Ready delivery constructed only by the S06 exact reclaim or the
/// S08 bounded fresh read.  It owns the canonical bytes and the authenticated
/// Ready owner, exposes no constructor or payload accessor, and deliberately
/// implements neither Clone nor serde.
///
/// ```compile_fail
/// fn requires_clone<T: Clone>() {}
/// requires_clone::<runtime_durable_service::Stage8bP1eClaimedM10DeliveryV2>();
/// ```
///
/// ```compile_fail
/// fn requires_serialize<T: serde::Serialize>() {}
/// requires_serialize::<runtime_durable_service::Stage8bP1eClaimedM10DeliveryV2>();
/// ```
pub struct Stage8bP1eClaimedM10DeliveryV2 {
    owner: Stage8bP1RedisSemanticCompositionOwner,
    pending_m10: Stage8bP1PendingM10Delivery,
    source_stream: String,
    consumer_group: String,
    redis_entry_id: String,
    semantic_id_sha256: String,
    payload_sha256: String,
    semantic_m10_identity: Stage8bP1M10SemanticIdentityV1,
    operational_identity_sha256: String,
    acquisition_kind: Stage8bP1eAcquisitionKindV1,
    consumer_identity: String,
    delivery_generation: u64,
}

impl Stage8bP1eClaimedM10DeliveryV2 {
    fn new(
        owner: Stage8bP1RedisSemanticCompositionOwner,
        pending_m10: Stage8bP1PendingM10Delivery,
        acquisition_kind: Stage8bP1eAcquisitionKindV1,
        delivery_generation: u64,
    ) -> Result<Self, Stage8bP1RedisSemanticError> {
        if delivery_generation == 0 {
            return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
        }
        Ok(Self {
            source_stream: owner
                .transport
                .backend
                .namespace
                .canonical_m10_stream
                .clone(),
            consumer_group: owner.transport.backend.namespace.m10_consumer_group.clone(),
            redis_entry_id: pending_m10.redis_id().to_string(),
            semantic_id_sha256: pending_m10.semantic_id_sha256().to_string(),
            payload_sha256: pending_m10.payload_sha256().to_string(),
            semantic_m10_identity: pending_m10.semantic_m10_identity.clone(),
            operational_identity_sha256: owner
                .stage7
                .stage8b_p1_operational_identity_sha256()
                .to_string(),
            consumer_identity: owner.transport.backend.config.consumer_name.clone(),
            owner,
            pending_m10,
            acquisition_kind,
            delivery_generation,
        })
    }

    fn into_parts(
        self,
    ) -> Result<
        (
            Stage8bP1RedisSemanticCompositionOwner,
            Stage8bP1PendingM10Delivery,
        ),
        Stage8bP1RedisSemanticError,
    > {
        if self.delivery_generation == 0
            || !matches!(
                self.acquisition_kind,
                Stage8bP1eAcquisitionKindV1::ReclaimedReady
                    | Stage8bP1eAcquisitionKindV1::FreshReady
            )
            || self.source_stream != self.owner.transport.backend.namespace.canonical_m10_stream
            || self.consumer_group != self.owner.transport.backend.namespace.m10_consumer_group
            || self.consumer_identity != self.owner.transport.backend.config.consumer_name
            || self.operational_identity_sha256
                != self.owner.stage7.stage8b_p1_operational_identity_sha256()
            || self.redis_entry_id != self.pending_m10.redis_id()
            || self.semantic_id_sha256 != self.pending_m10.semantic_id_sha256()
            || self.payload_sha256 != self.pending_m10.payload_sha256()
            || self.semantic_m10_identity != self.pending_m10.semantic_m10_identity
        {
            return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
        }
        Ok((self.owner, self.pending_m10))
    }
}

/// Authenticated Ready continuation selected before the strategy sees the
/// acquired M10.  This is diagnostic routing only: it grants no source,
/// schedule or callback authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eReadySourceRouteV1 {
    Semantic,
    WorkingLimit,
}

impl Stage8bP1ePostAcquisitionOwnerV1 {
    pub fn ready_source_route(&self) -> Option<Stage8bP1eReadySourceRouteV1> {
        match self.route.as_ref() {
            Stage8bP1ePostAcquisitionRouteV1::ReadySource { .. } => {
                Some(Stage8bP1eReadySourceRouteV1::Semantic)
            }
            Stage8bP1ePostAcquisitionRouteV1::ReadyWorkingLimit { .. } => {
                Some(Stage8bP1eReadySourceRouteV1::WorkingLimit)
            }
            _ => None,
        }
    }
}

/// Opaque route-bound, single-use capability. Every continuation consumes it.
///
/// ```compile_fail
/// fn requires_copy<T: Copy>() {}
/// requires_copy::<runtime_durable_service::Stage8bP1eContinuationPermitV1>();
/// ```
///
/// ```compile_fail
/// fn requires_clone<T: Clone>() {}
/// requires_clone::<runtime_durable_service::Stage8bP1eContinuationPermitV1>();
/// ```
///
/// ```compile_fail
/// fn requires_serialize<T: serde::Serialize>() {}
/// requires_serialize::<runtime_durable_service::Stage8bP1eContinuationPermitV1>();
/// ```
///
/// ```compile_fail
/// fn requires_deserialize<T: serde::de::DeserializeOwned>() {}
/// requires_deserialize::<runtime_durable_service::Stage8bP1eContinuationPermitV1>();
/// ```
///
/// ```compile_fail
/// use runtime_durable_service::Stage8bP1eContinuationPermitV1;
/// fn split(permit: Stage8bP1eContinuationPermitV1) {
///     let Stage8bP1eContinuationPermitV1 { route } = permit;
/// }
/// ```
///
/// ```compile_fail
/// use runtime_durable_service::{
///     acquire_stage8b_p1d2_ack_with_redis, Stage8bP1RedisSemanticCompositionTransport,
///     Stage8bP1eContinuationPermitV1,
/// };
/// async fn acquire_twice(
///     permit: Stage8bP1eContinuationPermitV1,
///     transport: Stage8bP1RedisSemanticCompositionTransport,
/// ) {
///     let _ = acquire_stage8b_p1d2_ack_with_redis(permit, transport).await;
/// }
/// ```
pub struct Stage8bP1eContinuationPermitV1 {
    route: Box<Stage8bP1ePostAcquisitionRouteV1>,
}

/// Exhaustive diagnostic identity of the route selected after the mandatory
/// post-acquisition shutdown latch.  It grants no Redis, schedule, provider or
/// callback authority by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage8bP1eContinuationRouteKindV1 {
    ReadySemantic,
    ReadyWorkingLimit,
    ZeroIntentAck,
    JournalAhead,
    Prepublication,
    P1d4Prepublication,
    P1d4DispatchPending,
    P1d4OrderPending,
    P1d4PreFinalizationPending,
    P1d4PreAckPending,
    P1d4Ack,
    P1d4Truth,
    P1d2Ack,
    P1d2PreAck,
    P1d2Truth,
    P1d3PreAck,
    P1d3DispatchLimit,
    P1d3DispatchExpiry,
    P1d3DispatchCancel,
    P1d3Ack,
    P1d3Truth,
    P1d3CancelContinuation,
    P1d3Semantic,
}

impl Stage8bP1eContinuationRouteKindV1 {
    pub const ALL: [Self; 23] = [
        Self::ReadySemantic,
        Self::ReadyWorkingLimit,
        Self::ZeroIntentAck,
        Self::JournalAhead,
        Self::Prepublication,
        Self::P1d4Prepublication,
        Self::P1d4DispatchPending,
        Self::P1d4OrderPending,
        Self::P1d4PreFinalizationPending,
        Self::P1d4PreAckPending,
        Self::P1d4Ack,
        Self::P1d4Truth,
        Self::P1d2Ack,
        Self::P1d2PreAck,
        Self::P1d2Truth,
        Self::P1d3PreAck,
        Self::P1d3DispatchLimit,
        Self::P1d3DispatchExpiry,
        Self::P1d3DispatchCancel,
        Self::P1d3Ack,
        Self::P1d3Truth,
        Self::P1d3CancelContinuation,
        Self::P1d3Semantic,
    ];
}

/// Route-bound continuation selected only after the mandatory latch check.
/// Each variant owns the same opaque single-use permit, but its enum identity
/// forces the process dispatcher to name every accepted continuation.  The
/// underlying resume function still validates the route before any effect.
///
/// ```compile_fail
/// fn require_clone<T: Clone>() {}
/// require_clone::<runtime_durable_service::Stage8bP1eRoutedContinuationV1>();
/// ```
pub enum Stage8bP1eRoutedContinuationV1 {
    ReadySemantic(Stage8bP1eContinuationPermitV1),
    ReadyWorkingLimit(Stage8bP1eContinuationPermitV1),
    ZeroIntentAck(Stage8bP1eContinuationPermitV1),
    JournalAhead(Stage8bP1eContinuationPermitV1),
    Prepublication(Stage8bP1eContinuationPermitV1),
    P1d4Prepublication(Stage8bP1eContinuationPermitV1),
    P1d4DispatchPending(Stage8bP1eContinuationPermitV1),
    P1d4OrderPending(Stage8bP1eContinuationPermitV1),
    P1d4PreFinalizationPending(Stage8bP1eContinuationPermitV1),
    P1d4PreAckPending(Stage8bP1eContinuationPermitV1),
    P1d4Ack(Stage8bP1eContinuationPermitV1),
    P1d4Truth(Stage8bP1eContinuationPermitV1),
    P1d2Ack(Stage8bP1eContinuationPermitV1),
    P1d2PreAck(Stage8bP1eContinuationPermitV1),
    P1d2Truth(Stage8bP1eContinuationPermitV1),
    P1d3PreAck(Stage8bP1eContinuationPermitV1),
    P1d3DispatchLimit(Stage8bP1eContinuationPermitV1),
    P1d3DispatchExpiry(Stage8bP1eContinuationPermitV1),
    P1d3DispatchCancel(Stage8bP1eContinuationPermitV1),
    P1d3Ack(Stage8bP1eContinuationPermitV1),
    P1d3Truth(Stage8bP1eContinuationPermitV1),
    P1d3CancelContinuation(Stage8bP1eContinuationPermitV1),
    P1d3Semantic(Stage8bP1eContinuationPermitV1),
}

impl Stage8bP1eRoutedContinuationV1 {
    pub const fn kind(&self) -> Stage8bP1eContinuationRouteKindV1 {
        match self {
            Self::ReadySemantic(_) => Stage8bP1eContinuationRouteKindV1::ReadySemantic,
            Self::ReadyWorkingLimit(_) => Stage8bP1eContinuationRouteKindV1::ReadyWorkingLimit,
            Self::ZeroIntentAck(_) => Stage8bP1eContinuationRouteKindV1::ZeroIntentAck,
            Self::JournalAhead(_) => Stage8bP1eContinuationRouteKindV1::JournalAhead,
            Self::Prepublication(_) => Stage8bP1eContinuationRouteKindV1::Prepublication,
            Self::P1d4Prepublication(_) => Stage8bP1eContinuationRouteKindV1::P1d4Prepublication,
            Self::P1d4DispatchPending(_) => Stage8bP1eContinuationRouteKindV1::P1d4DispatchPending,
            Self::P1d4OrderPending(_) => Stage8bP1eContinuationRouteKindV1::P1d4OrderPending,
            Self::P1d4PreFinalizationPending(_) => {
                Stage8bP1eContinuationRouteKindV1::P1d4PreFinalizationPending
            }
            Self::P1d4PreAckPending(_) => Stage8bP1eContinuationRouteKindV1::P1d4PreAckPending,
            Self::P1d4Ack(_) => Stage8bP1eContinuationRouteKindV1::P1d4Ack,
            Self::P1d4Truth(_) => Stage8bP1eContinuationRouteKindV1::P1d4Truth,
            Self::P1d2Ack(_) => Stage8bP1eContinuationRouteKindV1::P1d2Ack,
            Self::P1d2PreAck(_) => Stage8bP1eContinuationRouteKindV1::P1d2PreAck,
            Self::P1d2Truth(_) => Stage8bP1eContinuationRouteKindV1::P1d2Truth,
            Self::P1d3PreAck(_) => Stage8bP1eContinuationRouteKindV1::P1d3PreAck,
            Self::P1d3DispatchLimit(_) => Stage8bP1eContinuationRouteKindV1::P1d3DispatchLimit,
            Self::P1d3DispatchExpiry(_) => Stage8bP1eContinuationRouteKindV1::P1d3DispatchExpiry,
            Self::P1d3DispatchCancel(_) => Stage8bP1eContinuationRouteKindV1::P1d3DispatchCancel,
            Self::P1d3Ack(_) => Stage8bP1eContinuationRouteKindV1::P1d3Ack,
            Self::P1d3Truth(_) => Stage8bP1eContinuationRouteKindV1::P1d3Truth,
            Self::P1d3CancelContinuation(_) => {
                Stage8bP1eContinuationRouteKindV1::P1d3CancelContinuation
            }
            Self::P1d3Semantic(_) => Stage8bP1eContinuationRouteKindV1::P1d3Semantic,
        }
    }
}

/// Diagnostic-only proof that an acquired source was deliberately retained.
/// It carries no source payload, Redis transport or durable authority.
pub struct Stage8bP1eRetainedSourceReceiptV1 {
    route_id: &'static str,
    acquisition_kind: Stage8bP1eAcquisitionKindV1,
    shutdown_intent: Stage8bP1eShutdownIntentV1,
}

impl Stage8bP1eRetainedSourceReceiptV1 {
    pub const fn route_id(&self) -> &'static str {
        self.route_id
    }

    pub fn shutdown_intent(&self) -> &Stage8bP1eShutdownIntentV1 {
        &self.shutdown_intent
    }

    pub const fn was_terminal_observation(&self) -> bool {
        matches!(
            self.acquisition_kind,
            Stage8bP1eAcquisitionKindV1::ObservedTerminal
        )
    }
}

pub enum Stage8bP1ePostAcquisitionDecisionV1 {
    RetainForRestart(Stage8bP1eRetainedSourceReceiptV1),
    Continue(Stage8bP1eContinuationPermitV1),
}

/// Mandatory latch result with an exhaustive continuation identity.  A set
/// latch destroys all effect authority and returns only a diagnostic receipt;
/// a clear latch returns exactly one route-bound linear permit.
pub enum Stage8bP1eRoutedPostAcquisitionDecisionV1 {
    RetainForRestart(Stage8bP1eRetainedSourceReceiptV1),
    Continue(Stage8bP1eRoutedContinuationV1),
}

/// Result of startup S06 pending-source inspection. This operation cannot read
/// a fresh M10 entry. A pending source below the claim threshold returns the
/// owner without parsing or invoking the strategy.
pub enum Stage8bP1eReadyPendingAcquisitionOutcomeV1 {
    Acquired(Stage8bP1ePostAcquisitionOwnerV1),
    NoPending(Box<Stage8bP1RedisSemanticCompositionOwner>),
    PendingNotClaimable {
        owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
        pending_m10_redis_id: String,
    },
}

/// Result of the steady-state S08 bounded fresh read. The startup pending scan
/// is deliberately unavailable through this function.
pub enum Stage8bP1eReadyFreshAcquisitionOutcomeV1 {
    Acquired(Stage8bP1ePostAcquisitionOwnerV1),
    EmptyFreshPoll(Box<Stage8bP1RedisSemanticCompositionOwner>),
}

/// Performs startup S06. It may reclaim the sole pending source, but cannot
/// consume a fresh source entry.
pub async fn acquire_stage8b_p1e_ready_pending_with_redis(
    mut owner: Stage8bP1RedisSemanticCompositionOwner,
) -> Result<Stage8bP1eReadyPendingAcquisitionOutcomeV1, Stage8bP1RedisSemanticError> {
    match owner
        .transport
        .backend
        .acquire_ready_pending_for_supervisor()
        .await?
    {
        Stage8bP1eReadyPendingAcquisitionV1::Delivery {
            delivery,
            acquisition_kind,
        } => Ok(Stage8bP1eReadyPendingAcquisitionOutcomeV1::Acquired(
            ready_post_acquisition_owner(owner, *delivery, acquisition_kind)?,
        )),
        Stage8bP1eReadyPendingAcquisitionV1::NoPending => Ok(
            Stage8bP1eReadyPendingAcquisitionOutcomeV1::NoPending(Box::new(owner)),
        ),
        Stage8bP1eReadyPendingAcquisitionV1::PendingNotClaimable(pending_m10_redis_id) => Ok(
            Stage8bP1eReadyPendingAcquisitionOutcomeV1::PendingNotClaimable {
                owner: Box::new(owner),
                pending_m10_redis_id,
            },
        ),
    }
}

/// Performs the bounded steady-state S08 read. The returned acquired owner
/// must pass through `decide_stage8b_p1e_post_acquisition_latch` before any
/// parse, callback or XACK is possible.
pub async fn poll_stage8b_p1e_ready_fresh_with_redis(
    mut owner: Stage8bP1RedisSemanticCompositionOwner,
) -> Result<Stage8bP1eReadyFreshAcquisitionOutcomeV1, Stage8bP1RedisSemanticError> {
    match owner
        .transport
        .backend
        .acquire_ready_fresh_for_supervisor()
        .await?
    {
        Stage8bP1eReadyFreshAcquisitionV1::Delivery(delivery) => Ok(
            Stage8bP1eReadyFreshAcquisitionOutcomeV1::Acquired(ready_post_acquisition_owner(
                owner,
                *delivery,
                Stage8bP1eAcquisitionKindV1::FreshReady,
            )?),
        ),
        Stage8bP1eReadyFreshAcquisitionV1::EmptyFreshPoll => Ok(
            Stage8bP1eReadyFreshAcquisitionOutcomeV1::EmptyFreshPoll(Box::new(owner)),
        ),
    }
}

/// Consumes the S06/S08 route-bound permit and processes the already acquired
/// bytes. No Redis acquisition is performed by this continuation.
pub async fn resume_stage8b_p1e_ready_source_with_redis(
    permit: Stage8bP1eContinuationPermitV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisSemanticOutcome, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::ReadySource { claimed } = consume_permit(permit) else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let (owner, pending_m10) = claimed.into_parts()?;
    owner
        .process_claimed_ready(pending_m10, commitment_key)
        .await
}

/// Consumes the already-acquired Ready/working-LIMIT source with the exact
/// source-produced schedule step.  This continuation performs no XPENDING,
/// XAUTOCLAIM or XREADGROUP operation.
pub async fn resume_stage8b_p1e_ready_working_limit_source_with_redis(
    permit: Stage8bP1eContinuationPermitV1,
    schedule_authority: Stage8bP1d3ScheduleStepAuthority,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisSemanticOutcome, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::ReadyWorkingLimit { claimed } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let (owner, pending_m10) = claimed.into_parts()?;
    owner
        .process_claimed_working_limit(pending_m10, schedule_authority, commitment_key)
        .await
}

/// Result of the complete C-already-read/D/E/F Working-LIMIT schedule
/// continuation. A stop result is diagnostic only: all lifecycle authority is
/// consumed so the next process must reconstruct from the authenticated
/// durable root while the exact source remains pending.
pub enum Stage8bP1eSignedWorkingScheduleOutcomeV1 {
    Stopped(crate::Stage8bP1eScheduleStopReceiptV1),
    Semantic(Stage8bP1RedisSemanticOutcome),
}

/// Result of the complete C-already-read/D/E/F Market schedule
/// continuation. The schedule binding is committed before either paper
/// provider path can observe the canonical successor M10. A stop destroys
/// all effect authority and leaves the originating source pending.
pub enum Stage8bP1eSignedMarketScheduleOutcomeV1 {
    Stopped(crate::Stage8bP1eScheduleStopReceiptV1),
    FeedbackAckCommitted(Box<Stage8bP1RedisFeedbackAckCommitted>),
}

/// Generated-Market counterpart of the signed Market continuation. The
/// reservation-bearing publication remains exact through the V4 schedule
/// binding and produces only the combined P1-d4 replacement S_ack.
pub enum Stage8bP1eSignedGeneratedMarketScheduleOutcomeV1 {
    Stopped(crate::Stage8bP1eScheduleStopReceiptV1),
    GeneratedMarketAckCommitted(Box<Stage8bP1RedisGeneratedMarketAckCommitted>),
}

/// Result of the complete C-already-read/D/E/F initial-LIMIT schedule
/// continuation. The replacement S_ack remains the only next lifecycle
/// authority; source XACK is not reachable from this result.
pub enum Stage8bP1eSignedInitialLimitScheduleOutcomeV1 {
    Stopped(crate::Stage8bP1eScheduleStopReceiptV1),
    LimitAckCommitted(Box<Stage8bP1RedisLimitAckCommitted>),
}

/// Restart continuation for an Initial LIMIT whose exact signed schedule and
/// command publication are already covered by V4. No schedule read or Hybrid
/// callback is reachable from this boundary.
pub enum Stage8bP1eRecoveredInitialLimitScheduleOutcomeV1 {
    Stopped(crate::Stage8bP1eScheduleStopReceiptV1),
    LimitAckCommitted {
        owner: Box<Stage8bP1RedisLimitAckCommitted>,
        high_water: strategy_runtime_core::Stage8bP1eScheduleHighWaterV1,
    },
}

/// Restart continuation for a generated Market whose exact P1-d4
/// publication and signed schedule are already covered by V4.
pub enum Stage8bP1eRecoveredGeneratedMarketScheduleOutcomeV1 {
    Stopped(crate::Stage8bP1eScheduleStopReceiptV1),
    GeneratedMarketAckCommitted {
        owner: Box<Stage8bP1RedisGeneratedMarketAckCommitted>,
        high_water: strategy_runtime_core::Stage8bP1eScheduleHighWaterV1,
    },
}

fn stage8b_p1e_m10_identity_from_validated(
    source: &Stage8bP1ValidatedCanonicalM10,
) -> strategy_runtime_core::Stage8bP1eM10IdentityV1 {
    strategy_runtime_core::Stage8bP1eM10IdentityV1 {
        close_ts_utc_ms: source.close_ts_utc_ms(),
        open_ts_utc_ms: source.open_ts_utc_ms(),
        payload_sha256: source.payload_sha256().to_string(),
        redis_id: source.redis_id().to_string(),
        semantic_id_sha256: source.semantic_id_sha256().to_string(),
    }
}

/// Binds one already verified signed snapshot to the exact published Market
/// decision and its first canonical successor, commits+rereads V4, performs
/// latch E and latch F, and only then enters the inherited paper provider.
/// No fresh M10 acquisition or source acknowledgement is performed here.
pub async fn resume_stage8b_p1e_command_published_with_signed_schedule(
    mut published: Stage8bP1RedisCommandPublished,
    snapshot: crate::Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eSignedMarketScheduleOutcomeV1, Stage8bP1RedisSemanticError> {
    if published.p1e_schedule_route() != Stage8bP1ePublishedScheduleRouteV1::PlainMarket {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    }
    if !published.command_matches_durable_evidence()
        || published.pending_m10.redis_id() != published.evidence.m10_redis_id
        || published.receipt.source_m10_redis_id != published.evidence.m10_redis_id
    {
        return Err(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict);
    }
    let operational_identity_sha256 = published
        .stage7
        .stage8b_p1_operational_identity_sha256()
        .to_string();
    let predecessor = published
        .pending_m10
        .parse_exact(&operational_identity_sha256)?;
    let candidate = published
        .transport
        .backend
        .exact_first_successor_m10(
            published.pending_m10.redis_id(),
            &operational_identity_sha256,
        )
        .await?;
    let predecessor = stage8b_p1e_m10_identity_from_validated(&predecessor);
    let candidate = stage8b_p1e_m10_identity_from_validated(&candidate);
    let strategy_request_id = published
        .evidence
        .strategy_request_id
        .ok_or(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict)?;
    let canonical_command_sha256 = published
        .evidence
        .canonical_command_sha256
        .clone()
        .ok_or(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict)?;
    let Stage8bP1RedisCommandPublished {
        stage7,
        evidence,
        command,
        transport,
        pending_m10,
        receipt,
        p1d4_reservation,
        p1d4_binding,
    } = published;
    let committed = match crate::bind_stage8b_p1e_market_schedule(
        stage7,
        snapshot,
        latch,
        &predecessor,
        &candidate,
        strategy_request_id.to_string(),
        canonical_command_sha256,
        bound_at_utc,
        commitment_key,
    )? {
        crate::Stage8bP1eScheduleBindingCommitV1::StoppedBeforeBinding { owner, receipt } => {
            drop(owner);
            drop(transport);
            drop(pending_m10);
            return Ok(Stage8bP1eSignedMarketScheduleOutcomeV1::Stopped(receipt));
        }
        crate::Stage8bP1eScheduleBindingCommitV1::Committed(owner) => *owner,
    };
    let permit = match crate::resume_stage8b_p1e_committed_schedule_binding(committed, latch) {
        crate::Stage8bP1eScheduleBindingDecisionV1::StoppedAfterBinding { owner, receipt } => {
            drop(owner);
            drop(transport);
            drop(pending_m10);
            return Ok(Stage8bP1eSignedMarketScheduleOutcomeV1::Stopped(receipt));
        }
        crate::Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
        crate::Stage8bP1eScheduleBindingDecisionV1::StoppedBeforeBinding { .. } => {
            unreachable!("committed schedule binding cannot stop before binding")
        }
    };
    let (stage7, authority) = match crate::continue_stage8b_p1e_market_schedule(permit, latch)? {
        crate::Stage8bP1eScheduleAuthorityDecisionV1::RetainForRestart { owner, receipt } => {
            drop(owner);
            drop(transport);
            drop(pending_m10);
            return Ok(Stage8bP1eSignedMarketScheduleOutcomeV1::Stopped(receipt));
        }
        crate::Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } => {
            (*owner, authority)
        }
    };
    let published = Stage8bP1RedisCommandPublished {
        stage7,
        evidence,
        command,
        transport,
        pending_m10,
        receipt,
        p1d4_reservation,
        p1d4_binding,
    };
    Ok(
        Stage8bP1eSignedMarketScheduleOutcomeV1::FeedbackAckCommitted(Box::new(
            published
                .execute_next_canonical_market(authority, commitment_key)
                .await?,
        )),
    )
}

/// Binds the exact reservation-bearing generated-Market publication to one
/// verified signed schedule, seals+rereads V4, then invokes only the accepted
/// P1-d4 provider path. No generic Market settlement or republish path is
/// reachable from this owner.
pub async fn resume_stage8b_p1e_generated_market_with_signed_schedule(
    mut published: Stage8bP1RedisCommandPublished,
    snapshot: crate::Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eSignedGeneratedMarketScheduleOutcomeV1, Stage8bP1RedisSemanticError> {
    if published.p1e_schedule_route() != Stage8bP1ePublishedScheduleRouteV1::GeneratedMarket {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    }
    if !published.command_matches_durable_evidence()
        || published.pending_m10.redis_id() != published.evidence.m10_redis_id
        || published.receipt.source_m10_redis_id != published.evidence.m10_redis_id
    {
        return Err(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict);
    }
    let (publication_seal_generation, publication_seal_commitment_sha256) = {
        let reservation = published
            .p1d4_reservation
            .as_ref()
            .ok_or(Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let binding = published
            .p1d4_binding
            .as_ref()
            .ok_or(Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        binding
            .validate_against(reservation)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        (
            binding.prepublication_seal_generation(),
            binding.prepublication_seal_commitment_sha256().to_string(),
        )
    };
    let operational_identity_sha256 = published
        .stage7
        .stage8b_p1_operational_identity_sha256()
        .to_string();
    let predecessor = published
        .pending_m10
        .parse_exact(&operational_identity_sha256)?;
    let candidate = published
        .transport
        .backend
        .exact_first_successor_m10(
            published.pending_m10.redis_id(),
            &operational_identity_sha256,
        )
        .await?;
    let predecessor = stage8b_p1e_m10_identity_from_validated(&predecessor);
    let candidate = stage8b_p1e_m10_identity_from_validated(&candidate);
    let strategy_request_id = published
        .evidence
        .strategy_request_id
        .ok_or(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict)?;
    let canonical_command_sha256 = published
        .evidence
        .canonical_command_sha256
        .clone()
        .ok_or(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict)?;
    let Stage8bP1RedisCommandPublished {
        stage7,
        evidence,
        command,
        transport,
        pending_m10,
        receipt,
        p1d4_reservation,
        p1d4_binding,
    } = published;
    let committed = match crate::bind_stage8b_p1e_generated_market_schedule(
        stage7,
        snapshot,
        latch,
        &predecessor,
        &candidate,
        strategy_request_id.to_string(),
        canonical_command_sha256,
        publication_seal_generation,
        publication_seal_commitment_sha256,
        bound_at_utc,
        commitment_key,
    )? {
        crate::Stage8bP1eScheduleBindingCommitV1::StoppedBeforeBinding { owner, receipt } => {
            drop(owner);
            drop(transport);
            drop(pending_m10);
            return Ok(Stage8bP1eSignedGeneratedMarketScheduleOutcomeV1::Stopped(
                receipt,
            ));
        }
        crate::Stage8bP1eScheduleBindingCommitV1::Committed(owner) => *owner,
    };
    let permit = match crate::resume_stage8b_p1e_committed_schedule_binding(committed, latch) {
        crate::Stage8bP1eScheduleBindingDecisionV1::StoppedAfterBinding { owner, receipt } => {
            drop(owner);
            drop(transport);
            drop(pending_m10);
            return Ok(Stage8bP1eSignedGeneratedMarketScheduleOutcomeV1::Stopped(
                receipt,
            ));
        }
        crate::Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
        crate::Stage8bP1eScheduleBindingDecisionV1::StoppedBeforeBinding { .. } => {
            unreachable!("committed schedule binding cannot stop before binding")
        }
    };
    let (stage7, authority) = match crate::continue_stage8b_p1e_market_schedule(permit, latch)? {
        crate::Stage8bP1eScheduleAuthorityDecisionV1::RetainForRestart { owner, receipt } => {
            drop(owner);
            drop(transport);
            drop(pending_m10);
            return Ok(Stage8bP1eSignedGeneratedMarketScheduleOutcomeV1::Stopped(
                receipt,
            ));
        }
        crate::Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } => {
            (*owner, authority)
        }
    };
    let published = Stage8bP1RedisCommandPublished {
        stage7,
        evidence,
        command,
        transport,
        pending_m10,
        receipt,
        p1d4_reservation,
        p1d4_binding,
    };
    Ok(
        Stage8bP1eSignedGeneratedMarketScheduleOutcomeV1::GeneratedMarketAckCommitted(Box::new(
            published
                .execute_next_canonical_generated_market(authority, commitment_key)
                .await?,
        )),
    )
}

/// Binds one verified signed snapshot to the exact published initial LIMIT
/// decision and first canonical successor. The V4 binding is sealed and
/// reread before the initial LIMIT paper transition can run.
pub async fn resume_stage8b_p1e_initial_limit_with_signed_schedule(
    mut published: Stage8bP1RedisCommandPublished,
    snapshot: crate::Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eSignedInitialLimitScheduleOutcomeV1, Stage8bP1RedisSemanticError> {
    if published.p1e_schedule_route() != Stage8bP1ePublishedScheduleRouteV1::InitialLimit {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    }
    if !published.command_matches_durable_evidence()
        || published.pending_m10.redis_id() != published.evidence.m10_redis_id
        || published.receipt.source_m10_redis_id != published.evidence.m10_redis_id
    {
        return Err(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict);
    }
    let operational_identity_sha256 = published
        .stage7
        .stage8b_p1_operational_identity_sha256()
        .to_string();
    let predecessor = published
        .pending_m10
        .parse_exact(&operational_identity_sha256)?;
    let candidate = published
        .transport
        .backend
        .exact_first_successor_m10(
            published.pending_m10.redis_id(),
            &operational_identity_sha256,
        )
        .await?;
    let predecessor = stage8b_p1e_m10_identity_from_validated(&predecessor);
    let candidate = stage8b_p1e_m10_identity_from_validated(&candidate);
    let strategy_request_id = published
        .evidence
        .strategy_request_id
        .ok_or(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict)?;
    let canonical_command_sha256 = published
        .evidence
        .canonical_command_sha256
        .clone()
        .ok_or(Stage8bP1RedisSemanticError::P1d1DecisionBindingConflict)?;
    let Stage8bP1RedisCommandPublished {
        stage7,
        evidence,
        command,
        transport,
        pending_m10,
        receipt,
        p1d4_reservation,
        p1d4_binding,
    } = published;
    let committed = match crate::bind_stage8b_p1e_initial_limit_schedule(
        stage7,
        snapshot,
        latch,
        &predecessor,
        &candidate,
        strategy_request_id.to_string(),
        canonical_command_sha256,
        receipt.covering_seal_generation,
        receipt.covering_seal_commitment_sha256.clone(),
        bound_at_utc,
        commitment_key,
    )? {
        crate::Stage8bP1eScheduleBindingCommitV1::StoppedBeforeBinding { owner, receipt } => {
            drop(owner);
            drop(transport);
            drop(pending_m10);
            return Ok(Stage8bP1eSignedInitialLimitScheduleOutcomeV1::Stopped(
                receipt,
            ));
        }
        crate::Stage8bP1eScheduleBindingCommitV1::Committed(owner) => *owner,
    };
    let permit = match crate::resume_stage8b_p1e_committed_schedule_binding(committed, latch) {
        crate::Stage8bP1eScheduleBindingDecisionV1::StoppedAfterBinding { owner, receipt } => {
            drop(owner);
            drop(transport);
            drop(pending_m10);
            return Ok(Stage8bP1eSignedInitialLimitScheduleOutcomeV1::Stopped(
                receipt,
            ));
        }
        crate::Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
        crate::Stage8bP1eScheduleBindingDecisionV1::StoppedBeforeBinding { .. } => {
            unreachable!("committed schedule binding cannot stop before binding")
        }
    };
    let (stage7, authority) = match crate::continue_stage8b_p1e_schedule_step(permit, latch)? {
        crate::Stage8bP1eScheduleAuthorityDecisionV1::RetainForRestart { owner, receipt } => {
            drop(owner);
            drop(transport);
            drop(pending_m10);
            return Ok(Stage8bP1eSignedInitialLimitScheduleOutcomeV1::Stopped(
                receipt,
            ));
        }
        crate::Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } => {
            (*owner, authority)
        }
    };
    let published = Stage8bP1RedisCommandPublished {
        stage7,
        evidence,
        command,
        transport,
        pending_m10,
        receipt,
        p1d4_reservation,
        p1d4_binding,
    };
    Ok(
        Stage8bP1eSignedInitialLimitScheduleOutcomeV1::LimitAckCommitted(Box::new(
            published
                .execute_next_canonical_limit(authority, commitment_key)
                .await?,
        )),
    )
}

/// Resumes only an authenticated Initial-LIMIT V4 tail. The exact source PEL,
/// immutable command publication marker and first successor M10 are all
/// cross-validated before the inherited P1-d3 S_ack commit. Source XACK stays
/// structurally unreachable until the returned owner commits S_truth.
pub async fn resume_stage8b_p1e_committed_initial_limit_with_redis(
    committed: Box<crate::Stage8bP1eScheduleBindingCommittedOwner>,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    latch: &Stage8bP1eShutdownLatchV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eRecoveredInitialLimitScheduleOutcomeV1, Stage8bP1RedisSemanticError> {
    let material = committed
        .initial_limit_restart_material()?
        .ok_or(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch)?;
    let permit = match crate::resume_stage8b_p1e_committed_schedule_binding(*committed, latch) {
        crate::Stage8bP1eScheduleBindingDecisionV1::StoppedAfterBinding { owner, receipt } => {
            drop(owner);
            drop(transport);
            return Ok(Stage8bP1eRecoveredInitialLimitScheduleOutcomeV1::Stopped(
                receipt,
            ));
        }
        crate::Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
        crate::Stage8bP1eScheduleBindingDecisionV1::StoppedBeforeBinding { .. } => {
            unreachable!("committed schedule binding cannot stop before binding")
        }
    };
    let (stage7, authority) = match crate::continue_stage8b_p1e_schedule_step(permit, latch)? {
        crate::Stage8bP1eScheduleAuthorityDecisionV1::RetainForRestart { owner, receipt } => {
            drop(owner);
            drop(transport);
            return Ok(Stage8bP1eRecoveredInitialLimitScheduleOutcomeV1::Stopped(
                receipt,
            ));
        }
        crate::Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } => {
            (*owner, authority)
        }
    };
    let pending_m10 = transport
        .backend
        .reclaim_exact_binding(
            &material.predecessor_m10.redis_id,
            &material.predecessor_m10.semantic_id_sha256,
            &material.predecessor_m10.payload_sha256,
        )
        .await?;
    transport
        .backend
        .revalidate_exact_command_publication(
            &material.evidence,
            &material.command,
            &pending_m10,
            material.publication_seal_generation,
            &material.publication_seal_commitment_sha256,
        )
        .await?;
    let successor = transport
        .backend
        .exact_first_successor_m10(
            pending_m10.redis_id(),
            &material.operational_identity_sha256,
        )
        .await?;
    if stage8b_p1e_m10_identity_from_validated(&successor) != material.candidate_m10 {
        return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
    }
    let observation = Stage8bP1d3InitialObservation::Candidate {
        evidence: Box::new(successor.into_p1d3_limit_evidence()?),
        schedule: authority,
    };
    let durable = stage7.commit_stage8b_p1d3_initial_limit_ack(observation, commitment_key)?;
    Ok(
        Stage8bP1eRecoveredInitialLimitScheduleOutcomeV1::LimitAckCommitted {
            owner: Box::new(Stage8bP1RedisLimitAckCommitted {
                durable,
                transport,
                pending_m10,
            }),
            high_water: material.high_water,
        },
    )
}

/// Resumes only an authenticated generated-Market V4 tail. The exact source
/// PEL, P1-d4 reservation/marker/command publication and first successor M10
/// are cross-validated before the inherited generated-Market provider can
/// commit its combined S_ack. No signed-schedule read or command republish is
/// reachable from this boundary.
pub async fn resume_stage8b_p1e_committed_generated_market_with_redis(
    committed: Box<crate::Stage8bP1eScheduleBindingCommittedOwner>,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    latch: &Stage8bP1eShutdownLatchV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eRecoveredGeneratedMarketScheduleOutcomeV1, Stage8bP1RedisSemanticError> {
    let material = committed
        .generated_market_restart_material()?
        .ok_or(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch)?;
    let permit = match crate::resume_stage8b_p1e_committed_schedule_binding(*committed, latch) {
        crate::Stage8bP1eScheduleBindingDecisionV1::StoppedAfterBinding { owner, receipt } => {
            drop(owner);
            drop(transport);
            return Ok(Stage8bP1eRecoveredGeneratedMarketScheduleOutcomeV1::Stopped(receipt));
        }
        crate::Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
        crate::Stage8bP1eScheduleBindingDecisionV1::StoppedBeforeBinding { .. } => {
            unreachable!("committed schedule binding cannot stop before binding")
        }
    };
    let (stage7, authority) = match crate::continue_stage8b_p1e_market_schedule(permit, latch)? {
        crate::Stage8bP1eScheduleAuthorityDecisionV1::RetainForRestart { owner, receipt } => {
            drop(owner);
            drop(transport);
            return Ok(Stage8bP1eRecoveredGeneratedMarketScheduleOutcomeV1::Stopped(receipt));
        }
        crate::Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } => {
            (*owner, authority)
        }
    };
    let pending_m10 = transport
        .backend
        .reclaim_exact_binding(
            &material.predecessor_m10.redis_id,
            &material.predecessor_m10.semantic_id_sha256,
            &material.predecessor_m10.payload_sha256,
        )
        .await?;
    transport
        .backend
        .revalidate_p1d4_publication(
            &material.evidence,
            &material.command,
            &pending_m10,
            &material.reservation,
            &material.publication_binding,
            true,
        )
        .await?;
    let successor = transport
        .backend
        .exact_first_successor_m10(
            pending_m10.redis_id(),
            &material.operational_identity_sha256,
        )
        .await?;
    if stage8b_p1e_m10_identity_from_validated(&successor) != material.candidate_m10 {
        return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
    }
    let receipt = Stage8bP1RedisCommandPublicationReceipt {
        schema_version: 1,
        source_m10_redis_id: material.evidence.m10_redis_id.clone(),
        semantic_batch_id_sha256: material.evidence.semantic_batch_id_sha256.clone(),
        strategy_request_id: material
            .evidence
            .strategy_request_id
            .ok_or(Stage8bP1RedisSemanticError::CommandPublicationConflict)?,
        canonical_command_sha256: material
            .evidence
            .canonical_command_sha256
            .clone()
            .ok_or(Stage8bP1RedisSemanticError::CommandPublicationConflict)?,
        canonical_envelope_sha256: material.reservation.canonical_envelope_sha256().to_string(),
        command_entry_id: material.publication_binding.command_entry_id().to_string(),
        covering_seal_generation: material
            .publication_binding
            .prepublication_seal_generation(),
        covering_seal_commitment_sha256: material
            .publication_binding
            .prepublication_seal_commitment_sha256()
            .to_string(),
        publication_reservation_sha256: Some(
            material
                .reservation
                .publication_reservation_sha256()
                .to_string(),
        ),
        publication_binding_sha256: Some(
            material
                .publication_binding
                .publication_binding_sha256()
                .to_string(),
        ),
        disposition: Stage8bP1RedisCommandPublicationDisposition::IdempotentExisting,
        m10_acknowledged: false,
        paper_provider_invoked: false,
        finam_transport_attached: false,
        broker_network_dispatch_attached: false,
        runtime_live: false,
        real_orders: false,
    };
    let published = Stage8bP1RedisCommandPublished {
        stage7,
        evidence: material.evidence,
        command: material.command,
        transport,
        pending_m10,
        receipt,
        p1d4_reservation: Some(material.reservation),
        p1d4_binding: Some(material.publication_binding),
    };
    Ok(
        Stage8bP1eRecoveredGeneratedMarketScheduleOutcomeV1::GeneratedMarketAckCommitted {
            owner: Box::new(
                published
                    .execute_next_canonical_generated_market(authority, commitment_key)
                    .await?,
            ),
            high_water: material.high_water,
        },
    )
}

/// Binds one already verified signed snapshot to the exact acquired Working
/// LIMIT M10, commits+rereads V4, performs latch E and latch F, then executes
/// only the inherited Working continuation. No Redis acquisition occurs here
/// and the source can reach XACK only through the returned typed lifecycle.
pub async fn resume_stage8b_p1e_ready_working_limit_with_signed_schedule(
    permit: Stage8bP1eContinuationPermitV1,
    snapshot: crate::Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eSignedWorkingScheduleOutcomeV1, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::ReadyWorkingLimit { claimed } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let (owner, pending_m10) = claimed.into_parts()?;
    let operational_identity_sha256 = owner
        .stage7
        .stage8b_p1_operational_identity_sha256()
        .to_string();
    let candidate = pending_m10.parse_exact(&operational_identity_sha256)?;
    let candidate = strategy_runtime_core::Stage8bP1eM10IdentityV1 {
        close_ts_utc_ms: candidate.close_ts_utc_ms(),
        open_ts_utc_ms: candidate.open_ts_utc_ms(),
        payload_sha256: candidate.payload_sha256().to_string(),
        redis_id: candidate.redis_id().to_string(),
        semantic_id_sha256: candidate.semantic_id_sha256().to_string(),
    };
    let (active_broker_order_id, working_book_transition_sha256, predecessor) = owner
        .stage7
        .stage8b_p1e_working_binding_parts()
        .ok_or(Stage8bP1RedisSemanticError::ExactSourceConflict)?;
    let Stage8bP1RedisSemanticCompositionOwner { stage7, transport } = owner;
    let committed = match crate::bind_stage8b_p1e_working_limit_schedule(
        stage7,
        snapshot,
        latch,
        &predecessor,
        &candidate,
        active_broker_order_id.as_str(),
        working_book_transition_sha256,
        bound_at_utc,
        commitment_key,
    )? {
        crate::Stage8bP1eScheduleBindingCommitV1::StoppedBeforeBinding { owner, receipt } => {
            drop(owner);
            drop(transport);
            drop(pending_m10);
            return Ok(Stage8bP1eSignedWorkingScheduleOutcomeV1::Stopped(receipt));
        }
        crate::Stage8bP1eScheduleBindingCommitV1::Committed(owner) => *owner,
    };
    let permit = match crate::resume_stage8b_p1e_committed_schedule_binding(committed, latch) {
        crate::Stage8bP1eScheduleBindingDecisionV1::StoppedAfterBinding { owner, receipt } => {
            drop(owner);
            drop(transport);
            drop(pending_m10);
            return Ok(Stage8bP1eSignedWorkingScheduleOutcomeV1::Stopped(receipt));
        }
        crate::Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
        crate::Stage8bP1eScheduleBindingDecisionV1::StoppedBeforeBinding { .. } => {
            unreachable!("committed schedule binding cannot stop before binding")
        }
    };
    let (stage7, authority) = match crate::continue_stage8b_p1e_schedule_step(permit, latch)? {
        crate::Stage8bP1eScheduleAuthorityDecisionV1::RetainForRestart { owner, receipt } => {
            drop(owner);
            drop(transport);
            drop(pending_m10);
            return Ok(Stage8bP1eSignedWorkingScheduleOutcomeV1::Stopped(receipt));
        }
        crate::Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } => {
            (*owner, authority)
        }
    };
    let owner = Stage8bP1RedisSemanticCompositionOwner { stage7, transport };
    let outcome = owner
        .process_claimed_working_limit(pending_m10, authority, commitment_key)
        .await?;
    Ok(Stage8bP1eSignedWorkingScheduleOutcomeV1::Semantic(outcome))
}

/// The only conversion from acquired ownership to either shutdown retention
/// or continuation authority. Both branches consume the acquired owner.
pub fn decide_stage8b_p1e_post_acquisition_latch(
    owner: Stage8bP1ePostAcquisitionOwnerV1,
    latch: &Stage8bP1eShutdownLatchV1,
) -> Stage8bP1ePostAcquisitionDecisionV1 {
    let Stage8bP1ePostAcquisitionOwnerV1 {
        route,
        acquisition_kind,
    } = owner;
    if let Some(intent) = latch.intent() {
        Stage8bP1ePostAcquisitionDecisionV1::RetainForRestart(Stage8bP1eRetainedSourceReceiptV1 {
            route_id: route.route_id(),
            acquisition_kind,
            shutdown_intent: intent.clone(),
        })
    } else {
        let _ = acquisition_kind;
        Stage8bP1ePostAcquisitionDecisionV1::Continue(Stage8bP1eContinuationPermitV1 { route })
    }
}

/// Performs the mandatory post-acquisition latch and exhaustively classifies
/// the resulting single-use continuation without inspecting or reconstructing
/// source bytes in the process layer.
pub fn route_stage8b_p1e_post_acquisition_v1(
    owner: Stage8bP1ePostAcquisitionOwnerV1,
    latch: &Stage8bP1eShutdownLatchV1,
) -> Stage8bP1eRoutedPostAcquisitionDecisionV1 {
    match decide_stage8b_p1e_post_acquisition_latch(owner, latch) {
        Stage8bP1ePostAcquisitionDecisionV1::RetainForRestart(receipt) => {
            Stage8bP1eRoutedPostAcquisitionDecisionV1::RetainForRestart(receipt)
        }
        Stage8bP1ePostAcquisitionDecisionV1::Continue(permit) => {
            let kind = permit.route.continuation_kind();
            let route = match kind {
                Stage8bP1eContinuationRouteKindV1::ReadySemantic => {
                    Stage8bP1eRoutedContinuationV1::ReadySemantic(permit)
                }
                Stage8bP1eContinuationRouteKindV1::ReadyWorkingLimit => {
                    Stage8bP1eRoutedContinuationV1::ReadyWorkingLimit(permit)
                }
                Stage8bP1eContinuationRouteKindV1::ZeroIntentAck => {
                    Stage8bP1eRoutedContinuationV1::ZeroIntentAck(permit)
                }
                Stage8bP1eContinuationRouteKindV1::JournalAhead => {
                    Stage8bP1eRoutedContinuationV1::JournalAhead(permit)
                }
                Stage8bP1eContinuationRouteKindV1::Prepublication => {
                    Stage8bP1eRoutedContinuationV1::Prepublication(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d4Prepublication => {
                    Stage8bP1eRoutedContinuationV1::P1d4Prepublication(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d4DispatchPending => {
                    Stage8bP1eRoutedContinuationV1::P1d4DispatchPending(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d4OrderPending => {
                    Stage8bP1eRoutedContinuationV1::P1d4OrderPending(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d4PreFinalizationPending => {
                    Stage8bP1eRoutedContinuationV1::P1d4PreFinalizationPending(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d4PreAckPending => {
                    Stage8bP1eRoutedContinuationV1::P1d4PreAckPending(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d4Ack => {
                    Stage8bP1eRoutedContinuationV1::P1d4Ack(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d4Truth => {
                    Stage8bP1eRoutedContinuationV1::P1d4Truth(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d2Ack => {
                    Stage8bP1eRoutedContinuationV1::P1d2Ack(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d2PreAck => {
                    Stage8bP1eRoutedContinuationV1::P1d2PreAck(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d2Truth => {
                    Stage8bP1eRoutedContinuationV1::P1d2Truth(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d3PreAck => {
                    Stage8bP1eRoutedContinuationV1::P1d3PreAck(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d3DispatchLimit => {
                    Stage8bP1eRoutedContinuationV1::P1d3DispatchLimit(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d3DispatchExpiry => {
                    Stage8bP1eRoutedContinuationV1::P1d3DispatchExpiry(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d3DispatchCancel => {
                    Stage8bP1eRoutedContinuationV1::P1d3DispatchCancel(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d3Ack => {
                    Stage8bP1eRoutedContinuationV1::P1d3Ack(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d3Truth => {
                    Stage8bP1eRoutedContinuationV1::P1d3Truth(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d3CancelContinuation => {
                    Stage8bP1eRoutedContinuationV1::P1d3CancelContinuation(permit)
                }
                Stage8bP1eContinuationRouteKindV1::P1d3Semantic => {
                    Stage8bP1eRoutedContinuationV1::P1d3Semantic(permit)
                }
            };
            Stage8bP1eRoutedPostAcquisitionDecisionV1::Continue(route)
        }
    }
}

fn post_acquisition_owner(
    route: Stage8bP1ePostAcquisitionRouteV1,
    acquisition_kind: Stage8bP1eAcquisitionKindV1,
) -> Stage8bP1ePostAcquisitionOwnerV1 {
    Stage8bP1ePostAcquisitionOwnerV1 {
        route: Box::new(route),
        acquisition_kind,
    }
}

fn ready_post_acquisition_owner(
    mut owner: Stage8bP1RedisSemanticCompositionOwner,
    pending_m10: Stage8bP1PendingM10Delivery,
    acquisition_kind: Stage8bP1eAcquisitionKindV1,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let working_limit = owner.stage7.stage8b_p1e_ready_source_is_working_limit()?;
    let delivery_generation = owner.transport.backend.next_delivery_generation()?;
    let claimed = Stage8bP1eClaimedM10DeliveryV2::new(
        owner,
        pending_m10,
        acquisition_kind,
        delivery_generation,
    )?;
    let route = if working_limit {
        Stage8bP1ePostAcquisitionRouteV1::ReadyWorkingLimit { claimed }
    } else {
        Stage8bP1ePostAcquisitionRouteV1::ReadySource { claimed }
    };
    Ok(post_acquisition_owner(route, acquisition_kind))
}

fn consume_permit(permit: Stage8bP1eContinuationPermitV1) -> Stage8bP1ePostAcquisitionRouteV1 {
    let Stage8bP1eContinuationPermitV1 { route } = permit;
    *route
}

pub async fn acquire_stage8b_p1_zero_intent_ack_with_redis(
    pending: P1SemanticZeroIntentAckPending,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let evidence = pending.evidence().clone();
    validate_zero_intent_evidence(&evidence)?;
    let pending_m10 = transport
        .backend
        .exact_delivery_for_evidence(&evidence, pending.operational_identity_sha256())
        .await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::ZeroIntentAck {
            pending,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ObservedTerminal,
    ))
}

pub async fn acquire_stage8b_p1_journal_ahead_with_redis(
    pending: P1SemanticPrepublicationPending,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let pending_m10 = transport.backend.reclaim_single_pending().await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::JournalAhead {
            pending: Box::new(pending),
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ReclaimedPending,
    ))
}

pub async fn acquire_stage8b_p1_prepublication_with_redis(
    durable: Stage8bP1SemanticPrepublicationOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    #[cfg(test)]
    p1e_i0_pause_acquisition_if_armed().await;
    let evidence = durable.evidence().clone();
    let pending_m10 = transport.backend.reclaim_exact_evidence(&evidence).await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::Prepublication {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ReclaimedPending,
    ))
}

pub async fn acquire_stage8b_p1d4_prepublication_with_redis(
    durable: Stage8bP1d4GeneratedMarketPrepublicationOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let evidence = durable.evidence().clone();
    let pending_m10 = transport.backend.reclaim_exact_evidence(&evidence).await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::P1d4Prepublication {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ReclaimedPending,
    ))
}

async fn acquire_stage8b_p1d4_journal_ahead_with_redis(
    durable: Stage8bP1d4JournalAheadPending,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let source = durable.source_m10_evidence()?;
    let pending_m10 = transport.backend.reclaim_exact_evidence(&source).await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::P1d4JournalAhead {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ReclaimedPending,
    ))
}

macro_rules! define_stage8b_p1d4_journal_ahead_acquire {
    ($function:ident, $owner:ty, $variant:ident) => {
        pub async fn $function(
            durable: $owner,
            transport: Stage8bP1RedisSemanticCompositionTransport,
        ) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
            acquire_stage8b_p1d4_journal_ahead_with_redis(
                Stage8bP1d4JournalAheadPending::$variant(durable),
                transport,
            )
            .await
        }
    };
}

define_stage8b_p1d4_journal_ahead_acquire!(
    acquire_stage8b_p1d4_dispatch_pending_with_redis,
    Stage8bP1d4GeneratedMarketDispatchPendingOwner,
    Dispatch
);
define_stage8b_p1d4_journal_ahead_acquire!(
    acquire_stage8b_p1d4_order_pending_with_redis,
    Stage8bP1d4GeneratedMarketOrderPendingOwner,
    Order
);
define_stage8b_p1d4_journal_ahead_acquire!(
    acquire_stage8b_p1d4_pre_finalization_with_redis,
    Stage8bP1d4GeneratedMarketPreFinalizationPendingOwner,
    PreFinalization
);
define_stage8b_p1d4_journal_ahead_acquire!(
    acquire_stage8b_p1d4_pre_ack_with_redis,
    Stage8bP1d4GeneratedMarketPreAckPendingOwner,
    PreAck
);

pub async fn acquire_stage8b_p1d4_ack_with_redis(
    durable: Stage8bP1d4GeneratedMarketAckCommittedOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let (evidence, _, _) = durable.redis_resume_material()?;
    let pending_m10 = transport.backend.reclaim_exact_evidence(&evidence).await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::P1d4Ack {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ReclaimedPending,
    ))
}

pub async fn acquire_stage8b_p1d4_truth_with_redis(
    durable: Stage8bP1d4GeneratedMarketTruthCommittedOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let (evidence, _, _) = durable.redis_resume_material()?;
    let pending_m10 = transport
        .backend
        .exact_delivery_for_evidence(&evidence, durable.operational_identity_sha256())
        .await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::P1d4Truth {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ObservedTerminal,
    ))
}

pub async fn acquire_stage8b_p1d2_ack_with_redis(
    durable: Stage8bP1d2AckCommittedOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let binding = durable.source_m10_binding()?;
    let pending_m10 = transport
        .backend
        .reclaim_exact_binding(
            binding.redis_id(),
            binding.semantic_id_sha256(),
            binding.payload_sha256(),
        )
        .await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::P1d2Ack {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ReclaimedPending,
    ))
}

pub async fn acquire_stage8b_p1d2_pre_ack_with_redis(
    durable: Stage8bP1d2PreAckPendingOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let predecessor = durable.predecessor_m10_binding()?;
    let pending_m10 = transport
        .backend
        .reclaim_exact_evidence(&predecessor)
        .await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::P1d2PreAck {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ReclaimedPending,
    ))
}

pub async fn acquire_stage8b_p1d2_truth_with_redis(
    durable: Stage8bP1d2TruthCommittedOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let binding = durable.source_m10_binding()?;
    let pending_m10 = transport
        .backend
        .exact_delivery_for_binding(
            binding.redis_id(),
            binding.semantic_id_sha256(),
            binding.payload_sha256(),
            durable.operational_identity_sha256(),
        )
        .await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::P1d2Truth {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ObservedTerminal,
    ))
}

pub async fn acquire_stage8b_p1d3_pre_ack_with_redis(
    durable: Stage8bP1d3PreAckPendingOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let pending_m10 = if durable.source_is_command_m10() {
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
            )
            .await?
    };
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::P1d3PreAck {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ReclaimedPending,
    ))
}

async fn acquire_stage8b_p1d3_dispatch_with_redis(
    durable: Stage8bP1d3DispatchPendingOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
    route: fn(
        Stage8bP1d3DispatchPendingOwner,
        Stage8bP1RedisSemanticCompositionTransport,
        Stage8bP1PendingM10Delivery,
    ) -> Stage8bP1ePostAcquisitionRouteV1,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let source = durable.source_m10_evidence()?;
    let pending_m10 = transport.backend.reclaim_exact_evidence(&source).await?;
    Ok(post_acquisition_owner(
        route(durable, transport, pending_m10),
        Stage8bP1eAcquisitionKindV1::ReclaimedPending,
    ))
}

pub async fn acquire_stage8b_p1d3_dispatch_limit_with_redis(
    durable: Stage8bP1d3DispatchPendingOwner,
    transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    if !durable.is_limit_place() {
        return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
    }
    acquire_stage8b_p1d3_dispatch_with_redis(
        durable,
        transport,
        |durable, transport, pending_m10| Stage8bP1ePostAcquisitionRouteV1::P1d3DispatchLimit {
            durable,
            transport,
            pending_m10,
        },
    )
    .await
}

pub async fn acquire_stage8b_p1d3_dispatch_expiry_with_redis(
    durable: Stage8bP1d3DispatchPendingOwner,
    transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    if !durable.is_limit_place() {
        return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
    }
    acquire_stage8b_p1d3_dispatch_with_redis(
        durable,
        transport,
        |durable, transport, pending_m10| Stage8bP1ePostAcquisitionRouteV1::P1d3DispatchExpiry {
            durable,
            transport,
            pending_m10,
        },
    )
    .await
}

pub async fn acquire_stage8b_p1d3_dispatch_cancel_with_redis(
    durable: Stage8bP1d3DispatchPendingOwner,
    transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    if !durable.is_cancel() {
        return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
    }
    acquire_stage8b_p1d3_dispatch_with_redis(
        durable,
        transport,
        |durable, transport, pending_m10| Stage8bP1ePostAcquisitionRouteV1::P1d3DispatchCancel {
            durable,
            transport,
            pending_m10,
        },
    )
    .await
}

pub async fn acquire_stage8b_p1d3_ack_with_redis(
    durable: Stage8bP1d3AckCommittedOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let evidence = durable.source_m10_evidence()?;
    let pending_m10 = transport.backend.reclaim_exact_evidence(&evidence).await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::P1d3Ack {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ReclaimedPending,
    ))
}

pub async fn acquire_stage8b_p1d3_truth_with_redis(
    durable: Stage8bP1d3TruthCommittedOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
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
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::P1d3Truth {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ObservedTerminal,
    ))
}

pub async fn acquire_stage8b_p1d3_cancel_continuation_with_redis(
    durable: Stage8bP1d3CancelContinuationOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let evidence = durable.source_m10_evidence()?;
    let pending_m10 = transport.backend.reclaim_exact_evidence(&evidence).await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::P1d3CancelContinuation {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ReclaimedPending,
    ))
}

pub async fn acquire_stage8b_p1d3_semantic_with_redis(
    durable: Stage8bP1d3SemanticPendingOwner,
    mut transport: Stage8bP1RedisSemanticCompositionTransport,
) -> Result<Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1RedisSemanticError> {
    let expected = durable.source_binding()?;
    let pending_m10 = transport
        .backend
        .reclaim_exact_binding(
            expected.redis_id(),
            expected.semantic_id_sha256(),
            expected.payload_sha256(),
        )
        .await?;
    Ok(post_acquisition_owner(
        Stage8bP1ePostAcquisitionRouteV1::P1d3Semantic {
            durable,
            transport,
            pending_m10,
        },
        Stage8bP1eAcquisitionKindV1::ReclaimedPending,
    ))
}

pub async fn resolve_stage8b_p1_zero_intent_ack_with_redis(
    permit: Stage8bP1eContinuationPermitV1,
) -> Result<Stage8bP1RedisZeroIntentAckResolved, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::ZeroIntentAck {
        pending,
        mut transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let evidence = pending.evidence().clone();
    let disposition = transport.backend.acknowledge_exact(&pending_m10).await?;
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
    permit: Stage8bP1eContinuationPermitV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisPrepublicationPending, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::JournalAhead {
        pending,
        transport,
        pending_m10: delivery,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let pending = *pending;
    let operational_identity_sha256 = pending.operational_identity_sha256().to_string();
    let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
    let accepted_bar = parse_exact_after_p1e_permit(&delivery, &operational_identity_sha256)?
        .into_stage5c_semantic_bar()?;
    let durable =
        pending.complete_with_exact_semantic_input(accepted_bar, binding, commitment_key)?;
    #[cfg(test)]
    p1e_i0_observe_replacement_seal_commit();
    Ok(Stage8bP1RedisPrepublicationPending {
        durable,
        transport,
        pending_m10: delivery,
    })
}

pub async fn resume_stage8b_p1_prepublication_with_redis(
    permit: Stage8bP1eContinuationPermitV1,
) -> Result<Stage8bP1RedisPrepublicationPending, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::Prepublication {
        durable,
        transport,
        pending_m10: delivery,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
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
    permit: Stage8bP1eContinuationPermitV1,
) -> Result<Stage8bP1RedisCommandPublished, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d4Prepublication {
        durable,
        mut transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let prepared = prepare_recovered_p1d4_publication(&durable)?;
    let receipt = transport
        .backend
        .publish_reserved_p1d4_command(&durable, &pending_m10, &prepared)
        .await?;
    #[cfg(test)]
    p1e_i0_observe_publication();
    #[cfg(test)]
    p1d4_observe_generated_market_event(P1d4ObservedEffectEvent::GeneratedPublication);
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
    permit: Stage8bP1eContinuationPermitV1,
    expected_kind: u8,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisGeneratedMarketAckCommitted, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d4JournalAhead {
        durable,
        mut transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let pending_kind = match &durable {
        Stage8bP1d4JournalAheadPending::Dispatch(_) => 0,
        Stage8bP1d4JournalAheadPending::Order(_) => 1,
        Stage8bP1d4JournalAheadPending::PreFinalization(_) => 2,
        Stage8bP1d4JournalAheadPending::PreAck(_) => 3,
    };
    if pending_kind != expected_kind {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    }
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
    #[cfg(test)]
    if pending_kind == 0 {
        p1d4_observe_generated_market_provider();
    }
    let durable = durable
        .commit_reconstructed_ack(successor.into_p1d1_execution_evidence()?, commitment_key)?;
    #[cfg(test)]
    p1e_i0_observe_replacement_seal_commit();
    #[cfg(test)]
    {
        let suffix = match pending_kind {
            0 => &[
                P1d4ObservedEffectEvent::GeneratedOrder,
                P1d4ObservedEffectEvent::GeneratedTrade,
                P1d4ObservedEffectEvent::GeneratedRequestFinalized,
            ][..],
            1 => &[
                P1d4ObservedEffectEvent::GeneratedTrade,
                P1d4ObservedEffectEvent::GeneratedRequestFinalized,
            ][..],
            2 => &[P1d4ObservedEffectEvent::GeneratedRequestFinalized][..],
            3 => &[][..],
            _ => unreachable!(),
        };
        for event in suffix {
            p1d4_observe_generated_market_event(*event);
        }
    }
    #[cfg(test)]
    p1d4_observe_generated_market_s_ack(durable.recovery_seal_generation());
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
    ($function:ident, $expected_kind:literal) => {
        pub async fn $function(
            permit: Stage8bP1eContinuationPermitV1,
            commitment_key: &Stage5gLifecycleCommitmentKey,
        ) -> Result<Stage8bP1RedisGeneratedMarketAckCommitted, Stage8bP1RedisSemanticError> {
            resume_stage8b_p1d4_journal_ahead_with_redis(permit, $expected_kind, commitment_key)
                .await
        }
    };
}

define_stage8b_p1d4_journal_ahead_resume!(resume_stage8b_p1d4_dispatch_pending_with_redis, 0);
define_stage8b_p1d4_journal_ahead_resume!(resume_stage8b_p1d4_order_pending_with_redis, 1);
define_stage8b_p1d4_journal_ahead_resume!(resume_stage8b_p1d4_pre_finalization_with_redis, 2);
define_stage8b_p1d4_journal_ahead_resume!(resume_stage8b_p1d4_pre_ack_with_redis, 3);

/// Reattaches only the exact retained source and immutable publication of a
/// combined S_ack. The sole continuation remains truth application.
pub async fn resume_stage8b_p1d4_ack_with_redis(
    permit: Stage8bP1eContinuationPermitV1,
) -> Result<Stage8bP1RedisGeneratedMarketAckCommitted, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d4Ack {
        durable,
        mut transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let (evidence, command, reservation) = durable.redis_resume_material()?;
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
    permit: Stage8bP1eContinuationPermitV1,
) -> Result<Stage8bP1RedisFeedbackResolved, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d4Truth {
        durable,
        mut transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let (evidence, command, reservation) = durable.redis_resume_material()?;
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
    let audit_evidence = durable.feedback_audit_evidence()?;
    let disposition = transport.backend.acknowledge_exact(&pending_m10).await?;
    #[cfg(test)]
    p1d4_observe_generated_market_event(P1d4ObservedEffectEvent::GeneratedXack);
    crate::recovery::stage8b_p1d4_test_crash_frontier("F16");
    let stage7 = durable.into_ready_after_source_resolution();
    Ok(Stage8bP1RedisFeedbackResolved {
        owner: Box::new(Stage8bP1RedisSemanticCompositionOwner { stage7, transport }),
        disposition,
        audit_evidence,
    })
}

/// Reattaches only the exact pending M10 retained by authenticated S_ack.
/// The returned owner has no ACK API and can continue with truth only.
pub async fn resume_stage8b_p1d2_ack_with_redis(
    permit: Stage8bP1eContinuationPermitV1,
) -> Result<Stage8bP1RedisFeedbackAckCommitted, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d2Ack {
        durable,
        transport,
        pending_m10: delivery,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let binding = durable.source_m10_binding()?;
    let parsed = parse_exact_after_p1e_permit(&delivery, durable.operational_identity_sha256())?;
    if parsed.redis_id() != binding.redis_id() {
        return Err(Stage8bP1RedisSemanticError::ExactSourceConflict);
    }
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
    permit: Stage8bP1eContinuationPermitV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisFeedbackAckCommitted, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d2PreAck {
        durable,
        mut transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let successor = transport
        .backend
        .exact_first_successor_m10(
            pending_m10.redis_id(),
            durable.operational_identity_sha256(),
        )
        .await?;
    let durable = durable
        .commit_reconstructed_ack(successor.into_p1d1_execution_evidence()?, commitment_key)?;
    #[cfg(test)]
    p1e_i0_observe_replacement_seal_commit();
    Ok(Stage8bP1RedisFeedbackAckCommitted {
        durable,
        transport,
        pending_m10,
    })
}

/// Reattaches only the exact pending M10 retained by authenticated S_truth.
/// Source XACK is the sole continuation exposed by the returned owner.
pub async fn resume_stage8b_p1d2_truth_with_redis(
    permit: Stage8bP1eContinuationPermitV1,
) -> Result<Stage8bP1RedisFeedbackResolved, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d2Truth {
        durable,
        mut transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let audit_evidence = durable.feedback_audit_evidence()?;
    let disposition = transport.backend.acknowledge_exact(&pending_m10).await?;
    let stage7 = durable.into_ready_after_source_resolution();
    Ok(Stage8bP1RedisFeedbackResolved {
        owner: Box::new(Stage8bP1RedisSemanticCompositionOwner { stage7, transport }),
        disposition,
        audit_evidence,
    })
}

/// Reattaches the exact pending decision M10 and reconstructs replacement
/// S_ack solely from the authenticated V3 outcome already durable in Stage 6.
/// No schedule/provider/candidate lookup is repeated on this path.
pub async fn resume_stage8b_p1d3_pre_ack_with_redis(
    permit: Stage8bP1eContinuationPermitV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisPreAckRecoveryOutcome, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d3PreAck {
        durable,
        transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let source_is_command = durable.source_is_command_m10();
    let operational_identity_sha256 = durable.operational_identity_sha256().to_string();
    let semantic_input = if source_is_command {
        None
    } else {
        let binding = binding_from_delivery(&pending_m10, operational_identity_sha256.clone());
        let accepted_bar =
            parse_exact_after_p1e_permit(&pending_m10, &operational_identity_sha256)?
                .into_stage5c_semantic_bar()?;
        Some((accepted_bar, binding))
    };
    let reconstructed = durable.commit_reconstructed_transition(commitment_key)?;
    #[cfg(test)]
    p1e_i0_observe_replacement_seal_commit();
    match reconstructed {
        Stage8bP1d3RecoveredCommitOutcome::Ready(_) => {
            Err(Stage8bP1RedisSemanticError::ExactSourceConflict)
        }
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
            #[cfg(test)]
            p1e_i0_observe_replacement_seal_commit();
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
    permit: Stage8bP1eContinuationPermitV1,
    schedule: Stage8bP1d3ScheduleStepAuthority,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisLimitAckCommitted, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d3DispatchLimit {
        durable,
        mut transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let successor = transport
        .backend
        .exact_first_successor_m10(
            pending_m10.redis_id(),
            durable.operational_identity_sha256(),
        )
        .await?;
    #[cfg(test)]
    p1d4_observe_p1d3_schedule();
    #[cfg(test)]
    p1d4_observe_p1d3_provider();
    let observation = Stage8bP1d3InitialObservation::Candidate {
        evidence: Box::new(successor.into_p1d3_limit_evidence()?),
        schedule,
    };
    let durable = durable.commit_limit(observation, commitment_key)?;
    #[cfg(test)]
    p1e_i0_observe_replacement_seal_commit();
    Ok(Stage8bP1RedisLimitAckCommitted {
        durable,
        transport,
        pending_m10,
    })
}

/// Reissues only the exact Stage 5E day-boundary authority for a dispatch-only
/// initial LIMIT expiry. The original command source remains pending.
pub async fn resume_stage8b_p1d3_dispatch_expiry_with_redis(
    permit: Stage8bP1eContinuationPermitV1,
    authority: Stage8bP1d3DayExpiryAuthority,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisLimitAckCommitted, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d3DispatchExpiry {
        durable,
        transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    #[cfg(test)]
    p1d4_observe_p1d3_schedule();
    #[cfg(test)]
    p1d4_observe_p1d3_provider();
    let durable = durable.commit_limit(
        Stage8bP1d3InitialObservation::DayExpiry { authority },
        commitment_key,
    )?;
    #[cfg(test)]
    p1e_i0_observe_replacement_seal_commit();
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
    permit: Stage8bP1eContinuationPermitV1,
    schedule: Stage8bP1d3ScheduleStepAuthority,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisCancelCommitOutcome, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d3DispatchCancel {
        durable,
        mut transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let successor = transport
        .backend
        .exact_first_successor_m10(
            pending_m10.redis_id(),
            durable.operational_identity_sha256(),
        )
        .await?;
    #[cfg(test)]
    p1d4_observe_p1d3_schedule();
    #[cfg(test)]
    p1d4_observe_p1d3_provider();
    let committed = durable.commit_cancel(
        successor.into_p1d3_limit_evidence()?,
        schedule,
        commitment_key,
    )?;
    #[cfg(test)]
    p1e_i0_observe_replacement_seal_commit();
    match committed {
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
/// The pre-I0 owner-plus-transport entrypoint is intentionally unavailable.
///
/// ```compile_fail
/// use runtime_durable_service::{
///     resume_stage8b_p1d3_ack_with_redis, Stage8bP1d3AckCommittedOwner,
///     Stage8bP1RedisSemanticCompositionTransport,
/// };
/// async fn legacy(
///     owner: Stage8bP1d3AckCommittedOwner,
///     transport: Stage8bP1RedisSemanticCompositionTransport,
/// ) {
///     let _ = resume_stage8b_p1d3_ack_with_redis(owner, transport).await;
/// }
/// ```
pub async fn resume_stage8b_p1d3_ack_with_redis(
    permit: Stage8bP1eContinuationPermitV1,
) -> Result<Stage8bP1RedisLimitAckCommitted, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d3Ack {
        durable,
        transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    Ok(Stage8bP1RedisLimitAckCommitted {
        durable,
        transport,
        pending_m10,
    })
}

/// Reattaches only the exact pending decision M10 retained by a P1-d3 truth
/// replacement. Source XACK is the sole exposed external mutation.
pub async fn resume_stage8b_p1d3_truth_with_redis(
    permit: Stage8bP1eContinuationPermitV1,
) -> Result<Stage8bP1RedisLimitResolved, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d3Truth {
        durable,
        mut transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let disposition = transport.backend.acknowledge_exact(&pending_m10).await?;
    crate::recovery::stage8b_p1d4_test_crash_frontier("F16");
    let stage7 = durable.into_ready_after_source_resolution();
    Ok(Stage8bP1RedisLimitResolved {
        owner: Box::new(Stage8bP1RedisSemanticCompositionOwner { stage7, transport }),
        disposition,
    })
}

/// Reattaches the exact pending CANCEL source while target truth is already
/// covered by `S_terminal`, then performs only the authenticated no-input
/// recovered-cancel continuation. No successor lookup is repeated.
pub async fn resume_stage8b_p1d3_cancel_continuation_with_redis(
    permit: Stage8bP1eContinuationPermitV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisLimitTruthCommitted, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d3CancelContinuation {
        durable,
        transport,
        pending_m10,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let durable = durable.commit_recovered_cancel(commitment_key)?;
    #[cfg(test)]
    p1e_i0_observe_replacement_seal_commit();
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
    permit: Stage8bP1eContinuationPermitV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1RedisSemanticOutcome, Stage8bP1RedisSemanticError> {
    let Stage8bP1ePostAcquisitionRouteV1::P1d3Semantic {
        durable,
        transport,
        pending_m10: delivery,
    } = consume_permit(permit)
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch);
    };
    let operational_identity_sha256 = durable.operational_identity_sha256().to_string();
    let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
    let accepted_bar = parse_exact_after_p1e_permit(&delivery, &operational_identity_sha256)?
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
    fn next_delivery_generation(&mut self) -> Result<u64, Stage8bP1RedisSemanticError> {
        self.delivery_generation = self
            .delivery_generation
            .checked_add(1)
            .ok_or(Stage8bP1RedisSemanticError::ExactSourceConflict)?;
        Ok(self.delivery_generation)
    }

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
                .map(|delivery| Stage8bP1ReadySourceAcquisition::Delivery(Box::new(delivery))),
            [entry] => {
                let redis_id = entry.id.clone();
                match self.try_reclaim_exact_id(&redis_id).await? {
                    Some(delivery) => Ok(Stage8bP1ReadySourceAcquisition::Delivery(Box::new(
                        delivery,
                    ))),
                    None => Ok(Stage8bP1ReadySourceAcquisition::PendingNotClaimable(
                        redis_id,
                    )),
                }
            }
            _ => Err(Stage8bP1RedisSemanticError::AmbiguousReadyPendingEntries),
        }
    }

    async fn acquire_ready_pending_for_supervisor(
        &mut self,
    ) -> Result<Stage8bP1eReadyPendingAcquisitionV1, Stage8bP1RedisSemanticError> {
        let pending = self.pending_entries("-", "+", 2).await?;
        match pending.ids.as_slice() {
            [] => Ok(Stage8bP1eReadyPendingAcquisitionV1::NoPending),
            [entry] => {
                let redis_id = entry.id.clone();
                match self.try_reclaim_exact_id(&redis_id).await? {
                    Some(delivery) => Ok(Stage8bP1eReadyPendingAcquisitionV1::Delivery {
                        delivery: Box::new(delivery),
                        acquisition_kind: Stage8bP1eAcquisitionKindV1::ReclaimedReady,
                    }),
                    None => Ok(Stage8bP1eReadyPendingAcquisitionV1::PendingNotClaimable(
                        redis_id,
                    )),
                }
            }
            _ => Err(Stage8bP1RedisSemanticError::AmbiguousReadyPendingEntries),
        }
    }

    async fn acquire_ready_fresh_for_supervisor(
        &mut self,
    ) -> Result<Stage8bP1eReadyFreshAcquisitionV1, Stage8bP1RedisSemanticError> {
        match self.read_next_fresh_bounded().await? {
            Some(delivery) => Ok(Stage8bP1eReadyFreshAcquisitionV1::Delivery(Box::new(
                delivery,
            ))),
            None => Ok(Stage8bP1eReadyFreshAcquisitionV1::EmptyFreshPoll),
        }
    }

    async fn read_next_fresh_bounded(
        &mut self,
    ) -> Result<Option<Stage8bP1PendingM10Delivery>, Stage8bP1RedisSemanticError> {
        if !self.groups_verified {
            return Err(Stage8bP1RedisSemanticError::GroupMissing);
        }
        let reply: Option<StreamReadReply> = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(&self.namespace.m10_consumer_group)
            .arg(&self.config.consumer_name)
            .arg("COUNT")
            .arg(self.config.read_count)
            .arg("BLOCK")
            .arg(1_000_u64)
            .arg("STREAMS")
            .arg(&self.namespace.canonical_m10_stream)
            .arg(">")
            .query_async(&mut self.connection)
            .await?;
        let Some(reply) = reply else {
            return Ok(None);
        };
        let mut entries = reply.keys.into_iter().flat_map(|key| key.ids);
        let Some(entry) = entries.next() else {
            return Ok(None);
        };
        if entries.next().is_some() {
            return Err(Stage8bP1RedisSemanticError::InvalidRedisReply);
        }
        delivery_from_entry(entry).map(Some)
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
    ) -> Result<Stage8bP1PendingM10Delivery, Stage8bP1RedisSemanticError> {
        let pending = self.pending_entries("-", "+", 2).await?;
        if pending.ids.len() != 1 || pending.ids[0].id != redis_id {
            return Err(Stage8bP1RedisSemanticError::ExactPendingEntryMissing);
        }
        let delivery = self.reclaim_exact_id(redis_id).await?;
        if delivery.semantic_id_sha256() != semantic_id_sha256
            || delivery.payload_sha256() != payload_sha256
            || delivery.redis_id() != redis_id
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
        let semantic_m10_identity = validated.semantic_m10_identity();
        Ok(Stage8bP1PendingM10Delivery {
            redis_id: redis_id.to_string(),
            semantic_id_sha256: semantic_id_sha256.to_string(),
            payload_sha256: payload_sha256.to_string(),
            canonical_bytes: payload.into_bytes(),
            semantic_m10_identity,
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
        let semantic_m10_identity = validated.semantic_m10_identity();
        Ok(Stage8bP1PendingM10Delivery {
            redis_id: evidence.m10_redis_id.clone(),
            semantic_id_sha256: evidence.m10_semantic_id_sha256.clone(),
            payload_sha256: evidence.m10_payload_sha256.clone(),
            canonical_bytes: payload.into_bytes(),
            semantic_m10_identity,
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
                #[cfg(any(test, feature = "stage8b-p1-test-fixtures"))]
                crate::recovery::stage8b_p1d4_test_observe_xack_reply(
                    acknowledged,
                    &self.namespace.canonical_m10_stream,
                    &self.namespace.m10_consumer_group,
                    delivery.redis_id(),
                );
                if acknowledged == 1 {
                    #[cfg(test)]
                    p1e_i0_observe_xack();
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

    async fn revalidate_exact_command_publication(
        &mut self,
        evidence: &Stage6Stage8bP1SemanticCommitEvidenceV1,
        command: &BrokerCommand,
        delivery: &Stage8bP1PendingM10Delivery,
        publication_seal_generation: u64,
        publication_seal_commitment_sha256: &str,
    ) -> Result<(), Stage8bP1RedisSemanticError> {
        let request_id = evidence
            .strategy_request_id
            .ok_or(Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let command_bytes = serde_json::to_vec(command)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let command_sha256 = sha256_hex(&command_bytes);
        if publication_seal_generation == 0
            || publication_seal_commitment_sha256.len() != 64
            || !publication_seal_commitment_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || evidence.intent_count != 1
            || evidence.canonical_command_sha256.as_deref() != Some(command_sha256.as_str())
            || delivery.redis_id() != evidence.m10_redis_id
            || delivery.semantic_id_sha256() != evidence.m10_semantic_id_sha256
            || delivery.payload_sha256() != evidence.m10_payload_sha256
        {
            return Err(Stage8bP1RedisSemanticError::CommandPublicationConflict);
        }
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
        let envelope_payload = std::str::from_utf8(&envelope_bytes)
            .map_err(|_| Stage8bP1RedisSemanticError::CommandPublicationConflict)?;
        let source_payload = std::str::from_utf8(&delivery.canonical_bytes)
            .map_err(|_| Stage8bP1RedisSemanticError::ExactSourceConflict)?;
        let marker_key = publication_marker_key(&self.namespace, request_id);
        let result: Vec<String> = redis::cmd("EVAL")
            .arg(COMMAND_PUBLICATION_REVALIDATE_LUA)
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
            .arg(sha256_hex(&envelope_bytes))
            .arg(envelope_payload)
            .arg(publication_seal_generation)
            .arg(publication_seal_commitment_sha256)
            .arg(COMMAND_PUBLICATION_MARKER_SCHEMA_VERSION)
            .arg(COMMAND_PUBLICATION_MARKER_DOMAIN)
            .arg(&self.namespace.stage7b_command_consumer_group)
            .query_async(&mut self.connection)
            .await?;
        match result.as_slice() {
            [classification, command_entry_id]
                if classification == "existing" && !command_entry_id.is_empty() =>
            {
                Ok(())
            }
            _ => Err(Stage8bP1RedisSemanticError::InvalidRedisReply),
        }
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
                #[cfg(test)]
                p1e_i0_observe_publication_revalidation();
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
    let semantic_m10_identity = validated.semantic_m10_identity();
    Ok(Stage8bP1PendingM10Delivery {
        redis_id: entry.id,
        semantic_id_sha256: validated.semantic_id_sha256().to_string(),
        payload_sha256: validated.payload_sha256().to_string(),
        canonical_bytes: payload.into_bytes(),
        semantic_m10_identity,
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
pub(crate) mod tests {
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
    use redis::streams::{StreamInfoGroupsReply, StreamPendingReply};
    use rust_decimal::Decimal;
    use std::{
        collections::BTreeMap,
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

    fn p1e_clear_permit(owner: Stage8bP1ePostAcquisitionOwnerV1) -> Stage8bP1eContinuationPermitV1 {
        let latch = Stage8bP1eShutdownLatchV1::new();
        let Stage8bP1ePostAcquisitionDecisionV1::Continue(permit) =
            decide_stage8b_p1e_post_acquisition_latch(owner, &latch)
        else {
            panic!("clear P1-e latch must issue one continuation permit");
        };
        // The accepted I0 driver injects a real signal immediately after the
        // linear permit decision on every inherited continuation path.  The
        // signal cannot revoke or clone the permit; the caller must still
        // drain that one future to its route-exact durable boundary.
        assert!(latch.request(Stage8bP1eShutdownIntentV1::new(
            Stage8bP1eShutdownCauseV1::ExternalSignal,
            20_000,
            1,
        )));
        assert_eq!(
            latch.intent().map(Stage8bP1eShutdownIntentV1::cause),
            Some(Stage8bP1eShutdownCauseV1::ExternalSignal)
        );
        permit
    }

    fn p1e_i0_terminal_effects(
        disposition: Stage8bP1RedisZeroIntentAckDisposition,
        publication_revalidation_total: u64,
    ) -> P1eI0ObservedEffectCountersV1 {
        P1eI0ObservedEffectCountersV1 {
            publication_revalidation_total,
            xack_total: u64::from(matches!(
                disposition,
                Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
            )),
            ..P1eI0ObservedEffectCountersV1::default()
        }
    }

    macro_rules! p1e_test_attach_helper {
        ($helper:ident, $acquire:ident, $resume:ident, $owner:ty, $output:ty, $expected:expr) => {
            async fn $helper(
                owner: $owner,
                transport: Stage8bP1RedisSemanticCompositionTransport,
            ) -> Result<$output, Stage8bP1RedisSemanticError> {
                let acquired = Box::pin($acquire(owner, transport)).await?;
                let permit = p1e_clear_permit(acquired);
                p1e_i0_begin_effect_audit();
                let result = Box::new(Box::pin($resume(permit)).await);
                let observed = p1e_i0_take_effect_audit();
                if let Ok(output) = &*result {
                    let expected: P1eI0ObservedEffectCountersV1 = ($expected)(output);
                    assert_eq!(
                        observed,
                        expected,
                        "{} observed post-permit effect vector drifted",
                        stringify!($helper)
                    );
                }
                *result
            }
        };
    }

    macro_rules! p1e_test_commit_helper {
        ($helper:ident, $acquire:ident, $resume:ident, $owner:ty, $output:ty, $expected:expr) => {
            async fn $helper(
                owner: $owner,
                transport: Stage8bP1RedisSemanticCompositionTransport,
                key: &Stage5gLifecycleCommitmentKey,
            ) -> Result<$output, Stage8bP1RedisSemanticError> {
                let acquired = Box::pin($acquire(owner, transport)).await?;
                let permit = p1e_clear_permit(acquired);
                p1e_i0_begin_effect_audit();
                let result = Box::new(Box::pin($resume(permit, key)).await);
                let observed = p1e_i0_take_effect_audit();
                if let Ok(output) = &*result {
                    let expected: P1eI0ObservedEffectCountersV1 = ($expected)(output);
                    assert_eq!(
                        observed,
                        expected,
                        "{} observed post-permit effect vector drifted",
                        stringify!($helper)
                    );
                }
                *result
            }
        };
    }

    p1e_test_attach_helper!(
        p1e_test_resolve_zero_intent,
        acquire_stage8b_p1_zero_intent_ack_with_redis,
        resolve_stage8b_p1_zero_intent_ack_with_redis,
        P1SemanticZeroIntentAckPending,
        Stage8bP1RedisZeroIntentAckResolved,
        |output: &Stage8bP1RedisZeroIntentAckResolved| {
            p1e_i0_terminal_effects(output.disposition(), 0)
        }
    );
    p1e_test_attach_helper!(
        p1e_test_resume_prepublication,
        acquire_stage8b_p1_prepublication_with_redis,
        resume_stage8b_p1_prepublication_with_redis,
        Stage8bP1SemanticPrepublicationOwner,
        Stage8bP1RedisPrepublicationPending,
        |_: &Stage8bP1RedisPrepublicationPending| P1eI0ObservedEffectCountersV1::default()
    );
    p1e_test_commit_helper!(
        p1e_test_resume_journal_ahead,
        acquire_stage8b_p1_journal_ahead_with_redis,
        resume_stage8b_p1_journal_ahead_with_redis,
        P1SemanticPrepublicationPending,
        Stage8bP1RedisPrepublicationPending,
        |_: &Stage8bP1RedisPrepublicationPending| P1eI0ObservedEffectCountersV1 {
            replacement_seal_commit_total: 1,
            ..P1eI0ObservedEffectCountersV1::default()
        }
    );
    p1e_test_attach_helper!(
        p1e_test_resume_p1d4_prepublication,
        acquire_stage8b_p1d4_prepublication_with_redis,
        resume_stage8b_p1d4_prepublication_with_redis,
        Stage8bP1d4GeneratedMarketPrepublicationOwner,
        Stage8bP1RedisCommandPublished,
        |_: &Stage8bP1RedisCommandPublished| P1eI0ObservedEffectCountersV1 {
            publication_total: 1,
            ..P1eI0ObservedEffectCountersV1::default()
        }
    );
    p1e_test_commit_helper!(
        p1e_test_resume_p1d4_dispatch,
        acquire_stage8b_p1d4_dispatch_pending_with_redis,
        resume_stage8b_p1d4_dispatch_pending_with_redis,
        Stage8bP1d4GeneratedMarketDispatchPendingOwner,
        Stage8bP1RedisGeneratedMarketAckCommitted,
        |_: &Stage8bP1RedisGeneratedMarketAckCommitted| P1eI0ObservedEffectCountersV1 {
            replacement_seal_commit_total: 1,
            publication_revalidation_total: 1,
            ..P1eI0ObservedEffectCountersV1::default()
        }
    );
    p1e_test_commit_helper!(
        p1e_test_resume_p1d4_order,
        acquire_stage8b_p1d4_order_pending_with_redis,
        resume_stage8b_p1d4_order_pending_with_redis,
        Stage8bP1d4GeneratedMarketOrderPendingOwner,
        Stage8bP1RedisGeneratedMarketAckCommitted,
        |_: &Stage8bP1RedisGeneratedMarketAckCommitted| P1eI0ObservedEffectCountersV1 {
            replacement_seal_commit_total: 1,
            publication_revalidation_total: 1,
            ..P1eI0ObservedEffectCountersV1::default()
        }
    );
    p1e_test_commit_helper!(
        p1e_test_resume_p1d4_pre_finalization,
        acquire_stage8b_p1d4_pre_finalization_with_redis,
        resume_stage8b_p1d4_pre_finalization_with_redis,
        Stage8bP1d4GeneratedMarketPreFinalizationPendingOwner,
        Stage8bP1RedisGeneratedMarketAckCommitted,
        |_: &Stage8bP1RedisGeneratedMarketAckCommitted| P1eI0ObservedEffectCountersV1 {
            replacement_seal_commit_total: 1,
            publication_revalidation_total: 1,
            ..P1eI0ObservedEffectCountersV1::default()
        }
    );
    p1e_test_commit_helper!(
        p1e_test_resume_p1d4_pre_ack,
        acquire_stage8b_p1d4_pre_ack_with_redis,
        resume_stage8b_p1d4_pre_ack_with_redis,
        Stage8bP1d4GeneratedMarketPreAckPendingOwner,
        Stage8bP1RedisGeneratedMarketAckCommitted,
        |_: &Stage8bP1RedisGeneratedMarketAckCommitted| P1eI0ObservedEffectCountersV1 {
            replacement_seal_commit_total: 1,
            publication_revalidation_total: 1,
            ..P1eI0ObservedEffectCountersV1::default()
        }
    );
    p1e_test_attach_helper!(
        p1e_test_resume_p1d4_ack,
        acquire_stage8b_p1d4_ack_with_redis,
        resume_stage8b_p1d4_ack_with_redis,
        Stage8bP1d4GeneratedMarketAckCommittedOwner,
        Stage8bP1RedisGeneratedMarketAckCommitted,
        |_: &Stage8bP1RedisGeneratedMarketAckCommitted| P1eI0ObservedEffectCountersV1 {
            publication_revalidation_total: 1,
            ..P1eI0ObservedEffectCountersV1::default()
        }
    );
    p1e_test_attach_helper!(
        p1e_test_resume_p1d4_truth,
        acquire_stage8b_p1d4_truth_with_redis,
        resume_stage8b_p1d4_truth_with_redis,
        Stage8bP1d4GeneratedMarketTruthCommittedOwner,
        Stage8bP1RedisFeedbackResolved,
        |output: &Stage8bP1RedisFeedbackResolved| {
            p1e_i0_terminal_effects(output.disposition(), 1)
        }
    );
    p1e_test_attach_helper!(
        p1e_test_resume_p1d2_ack,
        acquire_stage8b_p1d2_ack_with_redis,
        resume_stage8b_p1d2_ack_with_redis,
        Stage8bP1d2AckCommittedOwner,
        Stage8bP1RedisFeedbackAckCommitted,
        |_: &Stage8bP1RedisFeedbackAckCommitted| P1eI0ObservedEffectCountersV1::default()
    );
    p1e_test_commit_helper!(
        p1e_test_resume_p1d2_pre_ack,
        acquire_stage8b_p1d2_pre_ack_with_redis,
        resume_stage8b_p1d2_pre_ack_with_redis,
        Stage8bP1d2PreAckPendingOwner,
        Stage8bP1RedisFeedbackAckCommitted,
        |_: &Stage8bP1RedisFeedbackAckCommitted| P1eI0ObservedEffectCountersV1 {
            replacement_seal_commit_total: 1,
            ..P1eI0ObservedEffectCountersV1::default()
        }
    );
    p1e_test_attach_helper!(
        p1e_test_resume_p1d2_truth,
        acquire_stage8b_p1d2_truth_with_redis,
        resume_stage8b_p1d2_truth_with_redis,
        Stage8bP1d2TruthCommittedOwner,
        Stage8bP1RedisFeedbackResolved,
        |output: &Stage8bP1RedisFeedbackResolved| {
            p1e_i0_terminal_effects(output.disposition(), 0)
        }
    );
    p1e_test_attach_helper!(
        p1e_test_resume_p1d3_ack,
        acquire_stage8b_p1d3_ack_with_redis,
        resume_stage8b_p1d3_ack_with_redis,
        Stage8bP1d3AckCommittedOwner,
        Stage8bP1RedisLimitAckCommitted,
        |_: &Stage8bP1RedisLimitAckCommitted| P1eI0ObservedEffectCountersV1::default()
    );
    p1e_test_attach_helper!(
        p1e_test_resume_p1d3_truth,
        acquire_stage8b_p1d3_truth_with_redis,
        resume_stage8b_p1d3_truth_with_redis,
        Stage8bP1d3TruthCommittedOwner,
        Stage8bP1RedisLimitResolved,
        |output: &Stage8bP1RedisLimitResolved| { p1e_i0_terminal_effects(output.disposition(), 0) }
    );
    p1e_test_commit_helper!(
        p1e_test_resume_p1d3_cancel_continuation,
        acquire_stage8b_p1d3_cancel_continuation_with_redis,
        resume_stage8b_p1d3_cancel_continuation_with_redis,
        Stage8bP1d3CancelContinuationOwner,
        Stage8bP1RedisLimitTruthCommitted,
        |_: &Stage8bP1RedisLimitTruthCommitted| P1eI0ObservedEffectCountersV1 {
            replacement_seal_commit_total: 1,
            ..P1eI0ObservedEffectCountersV1::default()
        }
    );

    fn p1e_i0_assert_pre_ack_effects(
        output: &Stage8bP1RedisPreAckRecoveryOutcome,
        observed: P1eI0ObservedEffectCountersV1,
    ) {
        match output {
            Stage8bP1RedisPreAckRecoveryOutcome::AckCommitted(_) => assert_eq!(
                observed,
                P1eI0ObservedEffectCountersV1 {
                    replacement_seal_commit_total: 1,
                    ..P1eI0ObservedEffectCountersV1::default()
                }
            ),
            Stage8bP1RedisPreAckRecoveryOutcome::TruthCommitted(_) => assert!(
                observed
                    == (P1eI0ObservedEffectCountersV1 {
                        replacement_seal_commit_total: 1,
                        ..P1eI0ObservedEffectCountersV1::default()
                    })
                    || observed
                        == (P1eI0ObservedEffectCountersV1 {
                            replacement_seal_commit_total: 2,
                            ..P1eI0ObservedEffectCountersV1::default()
                        }),
                "P1-d3 pre-ACK truth recovery emitted an impossible effect vector: {observed:?}"
            ),
            Stage8bP1RedisPreAckRecoveryOutcome::Semantic(outcome) => {
                let expected = match outcome {
                    Stage8bP1RedisSemanticOutcome::Ready {
                        ack_disposition, ..
                    } => P1eI0ObservedEffectCountersV1 {
                        replacement_seal_commit_total: 2,
                        callback_total: 1,
                        xack_total: u64::from(matches!(
                            ack_disposition,
                            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
                        )),
                        ..P1eI0ObservedEffectCountersV1::default()
                    },
                    Stage8bP1RedisSemanticOutcome::Prepublication(_) => {
                        P1eI0ObservedEffectCountersV1 {
                            replacement_seal_commit_total: 2,
                            callback_total: 1,
                            ..P1eI0ObservedEffectCountersV1::default()
                        }
                    }
                    Stage8bP1RedisSemanticOutcome::MultiIntentBlocked { .. } => {
                        P1eI0ObservedEffectCountersV1 {
                            replacement_seal_commit_total: 1,
                            callback_total: 1,
                            ..P1eI0ObservedEffectCountersV1::default()
                        }
                    }
                    Stage8bP1RedisSemanticOutcome::PendingNotClaimable { .. } => {
                        panic!("post-permit semantic continuation cannot become not-claimable")
                    }
                };
                assert_eq!(observed, expected);
            }
        }
    }

    async fn p1e_test_resume_p1d3_pre_ack(
        owner: Stage8bP1d3PreAckPendingOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisPreAckRecoveryOutcome, Stage8bP1RedisSemanticError> {
        let acquired = Box::pin(acquire_stage8b_p1d3_pre_ack_with_redis(owner, transport)).await?;
        let permit = p1e_clear_permit(acquired);
        p1e_i0_begin_effect_audit();
        let result = Box::new(Box::pin(resume_stage8b_p1d3_pre_ack_with_redis(permit, key)).await);
        let observed = p1e_i0_take_effect_audit();
        if let Ok(output) = &*result {
            p1e_i0_assert_pre_ack_effects(output, observed);
        }
        *result
    }

    fn p1e_i0_assert_semantic_effects(
        output: &Stage8bP1RedisSemanticOutcome,
        observed: P1eI0ObservedEffectCountersV1,
    ) {
        let expected = match output {
            Stage8bP1RedisSemanticOutcome::Ready {
                ack_disposition, ..
            } => P1eI0ObservedEffectCountersV1 {
                replacement_seal_commit_total: 1,
                callback_total: 1,
                xack_total: u64::from(matches!(
                    ack_disposition,
                    Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
                )),
                ..P1eI0ObservedEffectCountersV1::default()
            },
            Stage8bP1RedisSemanticOutcome::Prepublication(_) => P1eI0ObservedEffectCountersV1 {
                replacement_seal_commit_total: 1,
                callback_total: 1,
                ..P1eI0ObservedEffectCountersV1::default()
            },
            Stage8bP1RedisSemanticOutcome::MultiIntentBlocked { .. } => {
                P1eI0ObservedEffectCountersV1 {
                    callback_total: 1,
                    ..P1eI0ObservedEffectCountersV1::default()
                }
            }
            Stage8bP1RedisSemanticOutcome::PendingNotClaimable { .. } => {
                panic!("post-permit semantic continuation cannot become not-claimable")
            }
        };
        assert_eq!(observed, expected);
    }

    async fn p1e_test_resume_p1d3_semantic(
        owner: Stage8bP1d3SemanticPendingOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisSemanticOutcome, Stage8bP1RedisSemanticError> {
        let acquired = Box::pin(acquire_stage8b_p1d3_semantic_with_redis(owner, transport)).await?;
        let permit = p1e_clear_permit(acquired);
        p1e_i0_begin_effect_audit();
        let result = Box::new(Box::pin(resume_stage8b_p1d3_semantic_with_redis(permit, key)).await);
        let observed = p1e_i0_take_effect_audit();
        if let Ok(output) = &*result {
            p1e_i0_assert_semantic_effects(output, observed);
        }
        *result
    }

    async fn p1e_test_resume_p1d3_dispatch_limit(
        owner: Stage8bP1d3DispatchPendingOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        schedule: Stage8bP1d3ScheduleStepAuthority,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisLimitAckCommitted, Stage8bP1RedisSemanticError> {
        let acquired = Box::pin(acquire_stage8b_p1d3_dispatch_limit_with_redis(
            owner, transport,
        ))
        .await?;
        let permit = p1e_clear_permit(acquired);
        p1e_i0_begin_effect_audit();
        let result = Box::new(
            Box::pin(resume_stage8b_p1d3_dispatch_limit_with_redis(
                permit, schedule, key,
            ))
            .await,
        );
        let observed = p1e_i0_take_effect_audit();
        if result.is_ok() {
            assert_eq!(
                observed,
                P1eI0ObservedEffectCountersV1 {
                    replacement_seal_commit_total: 1,
                    ..P1eI0ObservedEffectCountersV1::default()
                }
            );
        }
        *result
    }

    async fn p1e_test_resume_p1d3_dispatch_expiry(
        owner: Stage8bP1d3DispatchPendingOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        authority: Stage8bP1d3DayExpiryAuthority,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisLimitAckCommitted, Stage8bP1RedisSemanticError> {
        let acquired = Box::pin(acquire_stage8b_p1d3_dispatch_expiry_with_redis(
            owner, transport,
        ))
        .await?;
        let permit = p1e_clear_permit(acquired);
        p1e_i0_begin_effect_audit();
        let result = Box::new(
            Box::pin(resume_stage8b_p1d3_dispatch_expiry_with_redis(
                permit, authority, key,
            ))
            .await,
        );
        let observed = p1e_i0_take_effect_audit();
        if result.is_ok() {
            assert_eq!(
                observed,
                P1eI0ObservedEffectCountersV1 {
                    replacement_seal_commit_total: 1,
                    ..P1eI0ObservedEffectCountersV1::default()
                }
            );
        }
        *result
    }

    async fn p1e_test_resume_p1d3_dispatch_cancel(
        owner: Stage8bP1d3DispatchPendingOwner,
        transport: Stage8bP1RedisSemanticCompositionTransport,
        schedule: Stage8bP1d3ScheduleStepAuthority,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1RedisCancelCommitOutcome, Stage8bP1RedisSemanticError> {
        let acquired = Box::pin(acquire_stage8b_p1d3_dispatch_cancel_with_redis(
            owner, transport,
        ))
        .await?;
        let permit = p1e_clear_permit(acquired);
        p1e_i0_begin_effect_audit();
        let result = Box::new(
            Box::pin(resume_stage8b_p1d3_dispatch_cancel_with_redis(
                permit, schedule, key,
            ))
            .await,
        );
        let observed = p1e_i0_take_effect_audit();
        if result.is_ok() {
            assert_eq!(
                observed,
                P1eI0ObservedEffectCountersV1 {
                    replacement_seal_commit_total: 1,
                    ..P1eI0ObservedEffectCountersV1::default()
                }
            );
        }
        *result
    }

    struct RedisServer {
        child: Child,
        url: String,
        port: u16,
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
                            return Self { child, url, port };
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

    #[derive(Clone, Copy)]
    enum P1eI0SignalArrival {
        PresetLatchBeforeAcquisition,
        AcquisitionInFlight,
        PostAcquisitionLatchDecision,
        PostPermitContinuation,
    }

    async fn p1e_i0_execute_lr02_signal_arrival(
        cause: Stage8bP1eShutdownCauseV1,
        arrival: P1eI0SignalArrival,
    ) {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i0-lr02-signal-arrival");
        let (pending, key, fresh, _) = one_intent_pending(&redis, &parent).await;
        let exact_source_redis_id = pending.pending_m10_redis_id().to_string();
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
        let Stage7bRestartOutcome::P1SemanticPrepublicationReady(owner) = restart else {
            panic!("LR02 fixture must expose exact prepublication authority");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let intent = Stage8bP1eShutdownIntentV1::new(cause, 20_000, 17);
        let latch = Stage8bP1eShutdownLatchV1::new();
        if matches!(arrival, P1eI0SignalArrival::PresetLatchBeforeAcquisition) {
            assert!(latch.request(intent.clone()));
        }
        let barrier = matches!(arrival, P1eI0SignalArrival::AcquisitionInFlight)
            .then(p1e_i0_arm_acquisition_barrier);
        let acquisition = acquire_stage8b_p1_prepublication_with_redis(*owner, transport);
        tokio::pin!(acquisition);
        let acquisition_entered_total = if let Some((entered, release, entered_total)) = barrier {
            tokio::select! {
                _ = &mut acquisition => {
                    panic!("LR02 acquisition completed before its in-flight barrier")
                }
                entered = entered => {
                    entered.expect("LR02 acquisition must publish its in-flight witness");
                }
            }
            assert_eq!(
                entered_total.load(std::sync::atomic::Ordering::SeqCst),
                1,
                "the one LR02 acquisition future must be entered exactly once"
            );
            assert!(latch.request(intent.clone()));
            release
                .send(())
                .expect("the same LR02 acquisition future must still own the barrier");
            Some(entered_total)
        } else {
            None
        };
        let acquired = acquisition.await.unwrap();
        if let Some(entered_total) = acquisition_entered_total {
            assert_eq!(
                entered_total.load(std::sync::atomic::Ordering::SeqCst),
                1,
                "LR02 must drain the same future without reacquisition"
            );
        }
        p1e_i0_begin_effect_audit();
        if matches!(arrival, P1eI0SignalArrival::PostAcquisitionLatchDecision) {
            assert!(latch.request(intent.clone()));
        }
        match arrival {
            P1eI0SignalArrival::PresetLatchBeforeAcquisition
            | P1eI0SignalArrival::AcquisitionInFlight
            | P1eI0SignalArrival::PostAcquisitionLatchDecision => {
                let Stage8bP1ePostAcquisitionDecisionV1::RetainForRestart(receipt) =
                    decide_stage8b_p1e_post_acquisition_latch(acquired, &latch)
                else {
                    panic!("set latch before permit must retain LR02");
                };
                assert_eq!(receipt.route_id(), "LR02");
                assert_eq!(receipt.shutdown_intent(), &intent);
            }
            P1eI0SignalArrival::PostPermitContinuation => {
                let Stage8bP1ePostAcquisitionDecisionV1::Continue(permit) =
                    decide_stage8b_p1e_post_acquisition_latch(acquired, &latch)
                else {
                    panic!("clear latch must issue the LR02 permit");
                };
                assert_eq!(permit.route.route_id(), "LR02");
                assert!(latch.request(intent.clone()));
                let boundary = resume_stage8b_p1_prepublication_with_redis(permit)
                    .await
                    .unwrap();
                assert_eq!(
                    p1e_i0_take_effect_audit(),
                    P1eI0ObservedEffectCountersV1::default()
                );
                assert_eq!(boundary.evidence().intent_count, 1);
                assert_eq!(latch.intent(), Some(&intent));
                drop(boundary);
            }
        }
        if !matches!(arrival, P1eI0SignalArrival::PostPermitContinuation) {
            assert_eq!(
                p1e_i0_take_effect_audit(),
                P1eI0ObservedEffectCountersV1::default()
            );
        }
        assert!(!latch.request(Stage8bP1eShutdownIntentV1::new(
            Stage8bP1eShutdownCauseV1::OwnerFailure,
            30_000,
            18,
        )));
        let retained = latch.intent().expect("first shutdown intent is retained");
        assert_eq!(retained, &intent);
        assert_eq!(retained.cause(), cause);
        assert_eq!(retained.final_exit_class(), cause.exit_class());
        assert_eq!(retained.first_request_sequence(), 17);
        assert_eq!(retained.bounded_exit_class(19_999), cause.exit_class());
        assert_eq!(retained.bounded_exit_class(20_000), 72);
        assert_eq!(retained.cause(), cause, "grace expiry preserves cause");

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        let pending_entries: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .arg("-")
            .arg("+")
            .arg(2)
            .query_async(&mut connection)
            .await
            .unwrap();
        let command_count: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 1, "LR02 signal retains exact source");
        assert_eq!(pending_entries.ids.len(), 1);
        assert_eq!(pending_entries.ids[0].id, exact_source_redis_id);
        assert_eq!(command_count, 0, "LR02 signal cannot publish");
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1e_i0_shutdown_intent_is_monotonic_and_cause_preserving() {
        for cause in [
            Stage8bP1eShutdownCauseV1::ExternalSignal,
            Stage8bP1eShutdownCauseV1::TelemetryFailure,
            Stage8bP1eShutdownCauseV1::SignalTaskFailure,
        ] {
            for arrival in [
                P1eI0SignalArrival::PresetLatchBeforeAcquisition,
                P1eI0SignalArrival::AcquisitionInFlight,
                P1eI0SignalArrival::PostAcquisitionLatchDecision,
                P1eI0SignalArrival::PostPermitContinuation,
            ] {
                p1e_i0_execute_lr02_signal_arrival(cause, arrival).await;
            }
        }
    }

    #[test]
    fn p1e_i0_inventory_pins_30_route_cells_and_46_effect_profiles() {
        assert_eq!(Stage8bP1eContinuationRouteKindV1::ALL.len(), 23);
        assert_eq!(
            Stage8bP1eContinuationRouteKindV1::ALL
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            23,
            "post-latch continuation inventory must stay exhaustive and unique"
        );
        let route_cells = [
            "LR01-default",
            "LR02-default",
            "LR03-default",
            "LR04-default",
            "LR05-default",
            "LR06-default",
            "LR07-default",
            "LR08-default",
            "LR09-default",
            "LR10-default",
            "LR11-limit",
            "LR11-expiry",
            "LR11-cancel",
            "LR12-command-source",
            "LR12-candidate-source",
            "LR13-default",
            "LR14-default",
            "LR15-s_eval",
            "LR15-s_terminal",
            "LT01-pending",
            "LT01-already-acknowledged",
            "LT02-pending",
            "LT02-already-acknowledged",
            "LT03-pending",
            "LT03-already-acknowledged",
            "LT04-pending",
            "LT04-already-acknowledged",
            "LT05-pending",
            "LT05-already-acknowledged",
            "LT05-due-day-timer",
        ];
        let fixtures = [
            ("FX01", "LR01-default", "COMMIT"),
            ("FX02", "LR02-default", "ATTACH"),
            ("FX03", "LR03-default", "COMMIT"),
            ("FX04", "LR04-default", "ATTACH"),
            ("FX05", "LR05-default", "PUBLICATION"),
            ("FX06", "LR06-default", "P1D4_COMMIT"),
            ("FX07", "LR07-default", "P1D4_COMMIT"),
            ("FX08", "LR08-default", "P1D4_COMMIT"),
            ("FX09", "LR09-default", "P1D4_COMMIT"),
            ("FX10", "LR10-default", "P1D4_ATTACH"),
            ("FX11", "LR11-limit", "COMMIT"),
            ("FX12", "LR11-expiry", "COMMIT"),
            ("FX13", "LR11-cancel", "COMMIT"),
            ("FX14", "LR11-cancel", "COMMIT"),
            ("FX15", "LR11-cancel", "COMMIT"),
            ("FX16", "LR12-command-source", "COMMIT"),
            ("FX17", "LR12-command-source", "COMMIT"),
            ("FX18", "LR12-candidate-source", "COMMIT"),
            ("FX19", "LR12-candidate-source", "COMMIT"),
            (
                "FX20",
                "LR12-candidate-source",
                "RECONSTRUCTED_SEMANTIC_READY",
            ),
            (
                "FX21",
                "LR12-candidate-source",
                "RECONSTRUCTED_SEMANTIC_PREPUBLICATION",
            ),
            (
                "FX22",
                "LR12-candidate-source",
                "RECONSTRUCTED_SEMANTIC_BLOCKED",
            ),
            ("FX23", "LR13-default", "ATTACH"),
            ("FX24", "LR14-default", "COMMIT"),
            ("FX25", "LR15-s_eval", "SEMANTIC_READY"),
            ("FX26", "LR15-s_eval", "SEMANTIC_PREPUBLICATION"),
            ("FX27", "LR15-s_eval", "SEMANTIC_BLOCKED"),
            ("FX28", "LR15-s_terminal", "SEMANTIC_READY"),
            ("FX29", "LR15-s_terminal", "SEMANTIC_PREPUBLICATION"),
            ("FX30", "LR15-s_terminal", "SEMANTIC_BLOCKED"),
            ("FX31", "LT01-pending", "TERMINAL_PENDING"),
            ("FX32", "LT01-already-acknowledged", "TERMINAL_ALREADY"),
            ("FX33", "LT02-pending", "TERMINAL_PENDING"),
            ("FX34", "LT02-already-acknowledged", "TERMINAL_ALREADY"),
            ("FX35", "LT03-pending", "P1D4_TERMINAL_PENDING"),
            ("FX36", "LT03-already-acknowledged", "P1D4_TERMINAL_ALREADY"),
            ("FX37", "LT04-pending", "TERMINAL_PENDING"),
            ("FX38", "LT04-already-acknowledged", "TERMINAL_ALREADY"),
            ("FX39", "LT05-pending", "TERMINAL_PENDING"),
            ("FX40", "LT05-already-acknowledged", "TERMINAL_ALREADY"),
            ("FX41", "LT05-due-day-timer", "DUE_PENDING_RECLASSIFY"),
            ("FX42", "LT05-due-day-timer", "DUE_ALREADY_RECLASSIFY"),
            (
                "FX43",
                "LR12-command-source",
                "RECONSTRUCTED_CANCEL_RECOVERED",
            ),
            (
                "FX44",
                "LR12-candidate-source",
                "RECONSTRUCTED_CANCEL_RECOVERED",
            ),
            ("FX45", "LT05-due-day-timer", "DUE_PENDING_STALE"),
            ("FX46", "LT05-due-day-timer", "DUE_ALREADY_STALE"),
        ];
        let fixture_matrix: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/stage-8/stage8b-p1e-route-outcome-fixture-matrix-v1.json"
        )))
        .unwrap();
        let counter_contract: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/stage-8/stage8b-p1e-route-outcome-counter-contract-v2.json"
        )))
        .unwrap();
        let amendments = counter_contract["fixture_profile_amendments"]
            .as_object()
            .unwrap();
        let contract_fixtures = fixture_matrix["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|fixture| {
                let fixture_id = fixture["fixture_id"].as_str().unwrap();
                let profile = amendments
                    .get(fixture_id)
                    .unwrap_or(&fixture["effect_profile"])
                    .as_str()
                    .unwrap();
                (
                    fixture_id.to_string(),
                    fixture["cell_id"].as_str().unwrap().to_string(),
                    profile.to_string(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        let implemented_fixtures = fixtures
            .iter()
            .map(|(fixture, cell, profile)| {
                (
                    (*fixture).to_string(),
                    (*cell).to_string(),
                    (*profile).to_string(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(implemented_fixtures, contract_fixtures);
        let counters = |profile| -> [u8; 7] {
            match profile {
                "ATTACH" => [0, 0, 0, 0, 0, 0, 0],
                "P1D4_ATTACH" => [0, 0, 0, 1, 0, 0, 0],
                "COMMIT" => [1, 0, 0, 0, 0, 0, 0],
                "RECONSTRUCTED_CANCEL_RECOVERED" => [2, 0, 0, 0, 0, 0, 0],
                "P1D4_COMMIT" => [1, 0, 0, 1, 0, 0, 0],
                "PUBLICATION" => [0, 0, 1, 0, 0, 0, 0],
                "SEMANTIC_READY" => [1, 1, 0, 0, 1, 0, 0],
                "SEMANTIC_PREPUBLICATION" => [1, 1, 0, 0, 0, 0, 0],
                "SEMANTIC_BLOCKED" => [0, 1, 0, 0, 0, 0, 0],
                "RECONSTRUCTED_SEMANTIC_READY" => [2, 1, 0, 0, 1, 0, 0],
                "RECONSTRUCTED_SEMANTIC_PREPUBLICATION" => [2, 1, 0, 0, 0, 0, 0],
                "RECONSTRUCTED_SEMANTIC_BLOCKED" => [1, 1, 0, 0, 0, 0, 0],
                "TERMINAL_PENDING" => [0, 0, 0, 0, 1, 0, 0],
                "TERMINAL_ALREADY" => [0, 0, 0, 0, 0, 0, 0],
                "P1D4_TERMINAL_PENDING" => [0, 0, 0, 1, 1, 0, 0],
                "P1D4_TERMINAL_ALREADY" => [0, 0, 0, 1, 0, 0, 0],
                "DUE_PENDING_RECLASSIFY" | "DUE_PENDING_STALE" => [0, 0, 0, 0, 1, 1, 0],
                "DUE_ALREADY_RECLASSIFY" | "DUE_ALREADY_STALE" => [0, 0, 0, 0, 0, 1, 0],
                other => panic!("unknown P1-e counter profile {other}"),
            }
        };
        let unique_cells = route_cells
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        let unique_fixtures = fixtures
            .iter()
            .map(|(fixture, _, _)| *fixture)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique_cells.len(), 30);
        assert_eq!(unique_fixtures.len(), 46);
        for (fixture, cell, profile) in fixtures {
            assert!(
                unique_cells.contains(cell),
                "{fixture} has unknown route cell"
            );
            let documented_counters = counter_contract["effective_profiles"][profile]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_u64().unwrap() as u8)
                .collect::<Vec<_>>();
            assert_eq!(
                counters(profile).as_slice(),
                documented_counters,
                "{fixture} effect profile drift"
            );
            assert_eq!(
                counters(profile)[6],
                0,
                "{fixture} cannot execute a timer in I0"
            );
        }
        assert_eq!(counters("P1D4_ATTACH"), [0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(
            counters("RECONSTRUCTED_SEMANTIC_READY"),
            [2, 1, 0, 0, 1, 0, 0]
        );
        assert_eq!(
            counters("RECONSTRUCTED_CANCEL_RECOVERED"),
            [2, 0, 0, 0, 0, 0, 0]
        );
    }

    #[tokio::test]
    async fn p1e_i0_preset_latch_retains_exact_source_and_route_mismatch_has_zero_effect() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i0-preset-latch");
        let (pending, key, fresh, _) = one_intent_pending(&redis, &parent).await;
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
        let Stage7bRestartOutcome::P1SemanticPrepublicationReady(owner) = restart else {
            panic!("one-intent restart must expose exact prepublication authority");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let acquired = acquire_stage8b_p1_prepublication_with_redis(*owner, transport)
            .await
            .unwrap();
        let intent =
            Stage8bP1eShutdownIntentV1::new(Stage8bP1eShutdownCauseV1::ExternalSignal, 20_000, 41);
        let latch = Stage8bP1eShutdownLatchV1::new();
        assert!(latch.request(intent.clone()));
        let Stage8bP1ePostAcquisitionDecisionV1::RetainForRestart(receipt) =
            decide_stage8b_p1e_post_acquisition_latch(acquired, &latch)
        else {
            panic!("set latch must retain the source without a permit");
        };
        assert_eq!(receipt.route_id(), "LR02");
        assert_eq!(receipt.shutdown_intent(), &intent);
        assert!(!receipt.was_terminal_observation());

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending_after_retain: StreamPendingReply = redis::cmd("XPENDING")
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
        assert_eq!(pending_after_retain.count(), 1);
        assert_eq!(command_count, 0);
        drop(connection);

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
        let Stage7bRestartOutcome::P1SemanticPrepublicationReady(owner) = restart else {
            panic!("retained source must remain restartable");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let acquired = acquire_stage8b_p1_prepublication_with_redis(*owner, transport)
            .await
            .unwrap();
        let Stage8bP1eRoutedPostAcquisitionDecisionV1::Continue(route) =
            route_stage8b_p1e_post_acquisition_v1(acquired, &Stage8bP1eShutdownLatchV1::new())
        else {
            panic!("LR02 must route only to the prepublication continuation");
        };
        assert_eq!(
            route.kind(),
            Stage8bP1eContinuationRouteKindV1::Prepublication
        );
        let Stage8bP1eRoutedContinuationV1::Prepublication(permit) = route else {
            panic!("LR02 kind and capability variant must agree");
        };
        assert!(matches!(
            resume_stage8b_p1d2_ack_with_redis(permit).await,
            Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch)
        ));
        let mut connection = redis.connection().await;
        let pending_after_mismatch: StreamPendingReply = redis::cmd("XPENDING")
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
        assert_eq!(pending_after_mismatch.count(), 1);
        assert_eq!(command_count, 0);
        fs::remove_dir_all(parent).unwrap();
    }

    fn p1e_i0_set_latch_and_retain(
        acquired: Stage8bP1ePostAcquisitionOwnerV1,
        expected_route_id: &str,
    ) {
        let intent =
            Stage8bP1eShutdownIntentV1::new(Stage8bP1eShutdownCauseV1::ExternalSignal, 20_000, 51);
        let latch = Stage8bP1eShutdownLatchV1::new();
        assert!(latch.request(intent.clone()));
        let Stage8bP1ePostAcquisitionDecisionV1::RetainForRestart(receipt) =
            decide_stage8b_p1e_post_acquisition_latch(acquired, &latch)
        else {
            panic!("set latch must not issue a continuation permit");
        };
        assert_eq!(receipt.route_id(), expected_route_id);
        assert_eq!(receipt.shutdown_intent(), &intent);
    }

    fn p1e_i0_clear_permit_then_signal(
        acquired: Stage8bP1ePostAcquisitionOwnerV1,
        expected_route_id: &str,
    ) -> (Stage8bP1eContinuationPermitV1, Stage8bP1eShutdownLatchV1) {
        let latch = Stage8bP1eShutdownLatchV1::new();
        let Stage8bP1ePostAcquisitionDecisionV1::Continue(permit) =
            decide_stage8b_p1e_post_acquisition_latch(acquired, &latch)
        else {
            panic!("clear latch must issue one continuation permit");
        };
        assert_eq!(permit.route.route_id(), expected_route_id);
        assert!(latch.request(Stage8bP1eShutdownIntentV1::new(
            Stage8bP1eShutdownCauseV1::ExternalSignal,
            20_000,
            52,
        )));
        (permit, latch)
    }

    async fn p1e_i0_assert_lr04_parse_ordering() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i0-lr04-parse-ordering");
        let (mut pending, key, fresh, identity) = one_intent_pending(&redis, &parent).await;
        pending
            .transport
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), 1_785_759_600_000, 2_175),
                &identity,
            )
            .await
            .unwrap();
        let ack = pending
            .publish_exact_command()
            .await
            .unwrap()
            .execute_next_canonical_market(p1d2_test_schedule_authority(), &key)
            .await
            .unwrap();
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
        let Stage7bRestartOutcome::P1d2AckCommitted(owner) = restart else {
            panic!("LR04 fixture must restart at P1-d2 S_ack");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        p1e_i0_begin_post_permit_parse_audit();
        let acquired = acquire_stage8b_p1d2_ack_with_redis(*owner, transport)
            .await
            .unwrap();
        assert_eq!(p1e_i0_take_post_permit_parse_audit(), 0);
        p1e_i0_set_latch_and_retain(acquired, "LR04");

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
        let Stage7bRestartOutcome::P1d2AckCommitted(owner) = restart else {
            panic!("retained LR04 must remain restartable");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        p1e_i0_begin_post_permit_parse_audit();
        let acquired = acquire_stage8b_p1d2_ack_with_redis(*owner, transport)
            .await
            .unwrap();
        assert_eq!(p1e_i0_take_post_permit_parse_audit(), 0);
        let (permit, latch) = p1e_i0_clear_permit_then_signal(acquired, "LR04");
        p1e_i0_begin_post_permit_parse_audit();
        p1e_i0_begin_effect_audit();
        let boundary = resume_stage8b_p1d2_ack_with_redis(permit).await.unwrap();
        assert_eq!(p1e_i0_take_post_permit_parse_audit(), 1);
        assert_eq!(
            p1e_i0_take_effect_audit(),
            P1eI0ObservedEffectCountersV1::default()
        );
        assert!(latch.intent().is_some());
        assert!(!boundary.m10_xack_allowed());
        drop(boundary);
        fs::remove_dir_all(parent).unwrap();
    }

    async fn p1e_i0_restart_p1d4_fixture(
        redis: &RedisServer,
        parent: &Path,
        scenario: &str,
        scenario_id: &str,
        frontier_id: &str,
        fresh: &strategy_runtime_core::HybridIntradayRuntimeStrategy,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> Stage7bRestartOutcome {
        spawn_p1d4_exact_frontier(redis, parent, scenario, scenario_id, frontier_id).await;
        restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.to_path_buf(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            key,
            fresh.clone(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn p1e_i0_parse_exact_is_transitively_after_permit_for_lr04_lr12_and_lr15() {
        p1e_i0_assert_lr04_parse_ordering().await;

        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i0-lr12-parse-ordering");
        let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let restart = p1e_i0_restart_p1d4_fixture(
            &redis,
            &parent,
            "later-filled",
            "S06",
            "F13",
            &fresh,
            &key,
        )
        .await;
        let Stage7bRestartOutcome::P1d3PreAckPending(owner) = restart else {
            panic!("LR12 candidate fixture must restart pre-ACK");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        p1e_i0_begin_post_permit_parse_audit();
        let acquired = acquire_stage8b_p1d3_pre_ack_with_redis(*owner, transport)
            .await
            .unwrap();
        assert_eq!(p1e_i0_take_post_permit_parse_audit(), 0);
        let (permit, latch) = p1e_i0_clear_permit_then_signal(acquired, "LR12");
        p1e_i0_begin_post_permit_parse_audit();
        p1e_i0_begin_effect_audit();
        let boundary = resume_stage8b_p1d3_pre_ack_with_redis(permit, &key)
            .await
            .unwrap();
        assert_eq!(p1e_i0_take_post_permit_parse_audit(), 1);
        assert_eq!(
            p1e_i0_take_effect_audit(),
            P1eI0ObservedEffectCountersV1 {
                replacement_seal_commit_total: 2,
                callback_total: 1,
                xack_total: 1,
                ..P1eI0ObservedEffectCountersV1::default()
            }
        );
        assert!(latch.intent().is_some());
        drop(boundary);
        fs::remove_dir_all(parent).unwrap();

        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i0-lr15-parse-ordering");
        let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let restart = p1e_i0_restart_p1d4_fixture(
            &redis,
            &parent,
            "later-untouched-zero",
            "S04",
            "F14",
            &fresh,
            &key,
        )
        .await;
        let Stage7bRestartOutcome::P1d3SemanticPending(owner) = restart else {
            panic!("LR15 fixture must restart at its semantic continuation");
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        p1e_i0_begin_post_permit_parse_audit();
        let acquired = acquire_stage8b_p1d3_semantic_with_redis(*owner, transport)
            .await
            .unwrap();
        assert_eq!(p1e_i0_take_post_permit_parse_audit(), 0);
        let (permit, latch) = p1e_i0_clear_permit_then_signal(acquired, "LR15");
        p1e_i0_begin_post_permit_parse_audit();
        p1e_i0_begin_effect_audit();
        let boundary = resume_stage8b_p1d3_semantic_with_redis(permit, &key)
            .await
            .unwrap();
        assert_eq!(p1e_i0_take_post_permit_parse_audit(), 1);
        assert_eq!(
            p1e_i0_take_effect_audit(),
            P1eI0ObservedEffectCountersV1 {
                replacement_seal_commit_total: 1,
                callback_total: 1,
                xack_total: 1,
                ..P1eI0ObservedEffectCountersV1::default()
            }
        );
        assert!(latch.intent().is_some());
        assert!(matches!(
            boundary,
            Stage8bP1RedisSemanticOutcome::Ready { .. }
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1e_i0_generated_market_fx05_through_fx10_observe_real_effects() {
        let cases = [
            (
                "FX05",
                "GM00",
                P1eI0ObservedEffectCountersV1 {
                    publication_total: 1,
                    ..P1eI0ObservedEffectCountersV1::default()
                },
            ),
            (
                "FX06",
                "GM03",
                P1eI0ObservedEffectCountersV1 {
                    replacement_seal_commit_total: 1,
                    publication_revalidation_total: 1,
                    ..P1eI0ObservedEffectCountersV1::default()
                },
            ),
            (
                "FX07",
                "GM05",
                P1eI0ObservedEffectCountersV1 {
                    replacement_seal_commit_total: 1,
                    publication_revalidation_total: 1,
                    ..P1eI0ObservedEffectCountersV1::default()
                },
            ),
            (
                "FX08",
                "GM06",
                P1eI0ObservedEffectCountersV1 {
                    replacement_seal_commit_total: 1,
                    publication_revalidation_total: 1,
                    ..P1eI0ObservedEffectCountersV1::default()
                },
            ),
            (
                "FX09",
                "GM07",
                P1eI0ObservedEffectCountersV1 {
                    replacement_seal_commit_total: 1,
                    publication_revalidation_total: 1,
                    ..P1eI0ObservedEffectCountersV1::default()
                },
            ),
            (
                "FX10",
                "GM10",
                P1eI0ObservedEffectCountersV1 {
                    publication_revalidation_total: 1,
                    ..P1eI0ObservedEffectCountersV1::default()
                },
            ),
        ];

        for (fixture_id, frontier_id, expected) in cases {
            let redis = RedisServer::start().await;
            let parent = temp_directory(&format!("p1e-i0-{fixture_id}"));
            let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
            let cell = p1d4_generated_market_registry_cells()
                .into_iter()
                .find(|cell| cell.frontier_id == frontier_id)
                .expect("P1-e fixture frontier must exist in the accepted P1-d4 registry");
            spawn_p1d4_generated_market_frontier(&redis, &parent, &cell).await;
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
            tokio::time::sleep(Duration::from_millis(5)).await;
            let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                .await
                .unwrap();

            let latch = match (frontier_id, restart) {
                (
                    "GM00",
                    Stage7bRestartOutcome::P1d4GeneratedMarketPrepublicationPending(owner),
                ) => {
                    let acquired =
                        acquire_stage8b_p1d4_prepublication_with_redis(*owner, transport)
                            .await
                            .unwrap();
                    let (permit, latch) = p1e_i0_clear_permit_then_signal(acquired, "LR05");
                    p1e_i0_begin_effect_audit();
                    drop(
                        resume_stage8b_p1d4_prepublication_with_redis(permit)
                            .await
                            .unwrap(),
                    );
                    latch
                }
                ("GM03", Stage7bRestartOutcome::P1d4GeneratedMarketDispatchPending(owner)) => {
                    let acquired =
                        acquire_stage8b_p1d4_dispatch_pending_with_redis(*owner, transport)
                            .await
                            .unwrap();
                    let (permit, latch) = p1e_i0_clear_permit_then_signal(acquired, "LR06");
                    p1e_i0_begin_effect_audit();
                    drop(
                        resume_stage8b_p1d4_dispatch_pending_with_redis(permit, &key)
                            .await
                            .unwrap(),
                    );
                    latch
                }
                ("GM05", Stage7bRestartOutcome::P1d4GeneratedMarketOrderPending(owner)) => {
                    let acquired = acquire_stage8b_p1d4_order_pending_with_redis(*owner, transport)
                        .await
                        .unwrap();
                    let (permit, latch) = p1e_i0_clear_permit_then_signal(acquired, "LR07");
                    p1e_i0_begin_effect_audit();
                    drop(
                        resume_stage8b_p1d4_order_pending_with_redis(permit, &key)
                            .await
                            .unwrap(),
                    );
                    latch
                }
                (
                    "GM06",
                    Stage7bRestartOutcome::P1d4GeneratedMarketPreFinalizationPending(owner),
                ) => {
                    let acquired =
                        acquire_stage8b_p1d4_pre_finalization_with_redis(*owner, transport)
                            .await
                            .unwrap();
                    let (permit, latch) = p1e_i0_clear_permit_then_signal(acquired, "LR08");
                    p1e_i0_begin_effect_audit();
                    drop(
                        resume_stage8b_p1d4_pre_finalization_with_redis(permit, &key)
                            .await
                            .unwrap(),
                    );
                    latch
                }
                ("GM07", Stage7bRestartOutcome::P1d4GeneratedMarketPreAckPending(owner)) => {
                    let acquired = acquire_stage8b_p1d4_pre_ack_with_redis(*owner, transport)
                        .await
                        .unwrap();
                    let (permit, latch) = p1e_i0_clear_permit_then_signal(acquired, "LR09");
                    p1e_i0_begin_effect_audit();
                    drop(
                        resume_stage8b_p1d4_pre_ack_with_redis(permit, &key)
                            .await
                            .unwrap(),
                    );
                    latch
                }
                ("GM10", Stage7bRestartOutcome::P1d4GeneratedMarketAckCommitted(owner)) => {
                    let acquired = acquire_stage8b_p1d4_ack_with_redis(*owner, transport)
                        .await
                        .unwrap();
                    let (permit, latch) = p1e_i0_clear_permit_then_signal(acquired, "LR10");
                    p1e_i0_begin_effect_audit();
                    drop(resume_stage8b_p1d4_ack_with_redis(permit).await.unwrap());
                    latch
                }
                (_, other) => panic!(
                    "{fixture_id} restarted at unexpected {}",
                    p1d4_restart_disposition(&other)
                ),
            };
            assert!(latch.intent().is_some(), "{fixture_id}: signal was lost");
            assert_eq!(
                p1e_i0_take_effect_audit(),
                expected,
                "{fixture_id}: observed post-permit effect vector drifted"
            );

            let namespace = stage8b_p1_redis_namespace();
            let mut connection = redis.connection().await;
            let pending: StreamPendingReply = redis::cmd("XPENDING")
                .arg(&namespace.canonical_m10_stream)
                .arg(&namespace.m10_consumer_group)
                .query_async(&mut connection)
                .await
                .unwrap();
            assert_eq!(
                pending.count(),
                1,
                "{fixture_id}: non-terminal boundary must retain source PEL"
            );
            fs::remove_dir_all(parent).unwrap();
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum P1eI0TimerClassification {
        Retained,
        Stale,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum P1eI0TimerFixtureRelation {
        SameAuthenticatedOwner,
        ForeignAuthenticatedOwner,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum P1eI0TimerSignalCheckpoint {
        AfterSourceBeforeReclassification,
        AfterReclassificationBeforeExecution,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct P1eI0AuthenticatedTimerV1 {
        timer_id: String,
        due_at_utc_ms: i64,
        owner_operational_identity_sha256: String,
        source_seal_generation: u64,
        source_seal_commitment_sha256: String,
        evidence_commitment_sha256: String,
    }

    struct P1eI0ClassifiedTimerV1 {
        owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
        timer: P1eI0AuthenticatedTimerV1,
        classification: P1eI0TimerClassification,
    }

    enum P1eI0AfterSourceTimerDecisionV1 {
        RetainForRestart {
            owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
            timer: P1eI0AuthenticatedTimerV1,
        },
        Reclassify {
            owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
            timer: P1eI0AuthenticatedTimerV1,
        },
    }

    enum P1eI0AfterReclassificationDecisionV1 {
        RetainForRestart(P1eI0ClassifiedTimerV1),
        ExecuteOnNextOwnerLoop(P1eI0ClassifiedTimerV1),
    }

    fn p1e_i0_timer_evidence_commitment(
        timer_id: &str,
        due_at_utc_ms: i64,
        owner_operational_identity_sha256: &str,
        source_seal_generation: u64,
        source_seal_commitment_sha256: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"moex.stage8b.p1e.i0.authenticated-timer.v1\0");
        for field in [
            timer_id.as_bytes(),
            owner_operational_identity_sha256.as_bytes(),
            source_seal_commitment_sha256.as_bytes(),
        ] {
            hasher.update((field.len() as u64).to_be_bytes());
            hasher.update(field);
        }
        hasher.update(due_at_utc_ms.to_be_bytes());
        hasher.update(source_seal_generation.to_be_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn p1e_i0_authenticated_timer(
        source_owner: &Stage8bP1d3TruthCommittedOwner,
        frontier_id: &str,
        relation: P1eI0TimerFixtureRelation,
    ) -> P1eI0AuthenticatedTimerV1 {
        let exact_owner = source_owner.operational_identity_sha256();
        let owner_operational_identity_sha256 = match relation {
            P1eI0TimerFixtureRelation::SameAuthenticatedOwner => exact_owner.to_string(),
            P1eI0TimerFixtureRelation::ForeignAuthenticatedOwner => {
                sha256_hex(format!("foreign-owner-for-{frontier_id}-{exact_owner}").as_bytes())
            }
        };
        let timer_id = format!("due-day-{frontier_id}-{relation:?}");
        let due_at_utc_ms = P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000;
        let source_seal_generation = source_owner.recovery_seal_generation();
        let source_seal_commitment_sha256 =
            source_owner.recovery_seal_commitment_sha256().to_string();
        let evidence_commitment_sha256 = p1e_i0_timer_evidence_commitment(
            &timer_id,
            due_at_utc_ms,
            &owner_operational_identity_sha256,
            source_seal_generation,
            &source_seal_commitment_sha256,
        );
        P1eI0AuthenticatedTimerV1 {
            timer_id,
            due_at_utc_ms,
            owner_operational_identity_sha256,
            source_seal_generation,
            source_seal_commitment_sha256,
            evidence_commitment_sha256,
        }
    }

    fn p1e_i0_after_source_timer_latch_decision(
        owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
        timer: P1eI0AuthenticatedTimerV1,
        latch: &Stage8bP1eShutdownLatchV1,
    ) -> P1eI0AfterSourceTimerDecisionV1 {
        if latch.intent().is_some() {
            P1eI0AfterSourceTimerDecisionV1::RetainForRestart { owner, timer }
        } else {
            P1eI0AfterSourceTimerDecisionV1::Reclassify { owner, timer }
        }
    }

    fn p1e_i0_reclassify_timer(
        owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
        timer: P1eI0AuthenticatedTimerV1,
        observed_at_utc_ms: i64,
    ) -> P1eI0ClassifiedTimerV1 {
        assert!(!timer.timer_id.is_empty());
        assert!(timer.due_at_utc_ms > 0);
        assert!(timer.source_seal_generation > 0);
        assert_eq!(timer.source_seal_commitment_sha256.len(), 64);
        assert_eq!(
            timer.evidence_commitment_sha256,
            p1e_i0_timer_evidence_commitment(
                &timer.timer_id,
                timer.due_at_utc_ms,
                &timer.owner_operational_identity_sha256,
                timer.source_seal_generation,
                &timer.source_seal_commitment_sha256,
            ),
            "timer evidence must remain authenticated before reclassification"
        );
        let returned_owner_identity = owner.stage7.stage8b_p1_operational_identity_sha256();
        let classification = if timer.due_at_utc_ms <= observed_at_utc_ms
            && timer.owner_operational_identity_sha256 == returned_owner_identity
        {
            P1eI0TimerClassification::Retained
        } else {
            P1eI0TimerClassification::Stale
        };
        p1e_i0_observe_timer_reclassification();
        P1eI0ClassifiedTimerV1 {
            owner,
            timer,
            classification,
        }
    }

    fn p1e_i0_after_reclassification_latch_decision(
        classified: P1eI0ClassifiedTimerV1,
        latch: &Stage8bP1eShutdownLatchV1,
    ) -> P1eI0AfterReclassificationDecisionV1 {
        if latch.intent().is_some() {
            P1eI0AfterReclassificationDecisionV1::RetainForRestart(classified)
        } else {
            P1eI0AfterReclassificationDecisionV1::ExecuteOnNextOwnerLoop(classified)
        }
    }

    fn p1e_i0_execute_timer_on_next_owner_loop(
        classified: P1eI0ClassifiedTimerV1,
    ) -> (
        Box<Stage8bP1RedisSemanticCompositionOwner>,
        P1eI0AuthenticatedTimerV1,
        P1eI0TimerClassification,
        bool,
    ) {
        let executed = matches!(
            classified.classification,
            P1eI0TimerClassification::Retained
        );
        if executed {
            p1e_i0_observe_timer_execution();
        }
        (
            classified.owner,
            classified.timer,
            classified.classification,
            executed,
        )
    }

    async fn p1e_i0_assert_source_first_timer_checkpoint(
        source_already_acknowledged: bool,
        relation: P1eI0TimerFixtureRelation,
        signal_checkpoint: Option<P1eI0TimerSignalCheckpoint>,
    ) {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i0-source-first-timer-checkpoint");
        let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let frontier_id = if source_already_acknowledged {
            "F16"
        } else {
            "F10"
        };
        let restart = p1e_i0_restart_p1d4_fixture(
            &redis,
            &parent,
            "cancel-target-first",
            "S09",
            frontier_id,
            &fresh,
            &key,
        )
        .await;
        let Stage7bRestartOutcome::P1d3TruthCommitted(owner) = restart else {
            panic!("LT05 timer fixture must restart at cancel-recovered truth");
        };
        let timer = p1e_i0_authenticated_timer(&owner, frontier_id, relation);
        let original_timer = timer.clone();
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        let acquired = acquire_stage8b_p1d3_truth_with_redis(*owner, transport)
            .await
            .unwrap();
        let permit = p1e_clear_permit(acquired);
        let latch = Stage8bP1eShutdownLatchV1::new();
        p1e_i0_begin_effect_audit();

        let resolved = resume_stage8b_p1d3_truth_with_redis(permit).await.unwrap();
        assert_eq!(
            resolved.disposition(),
            if source_already_acknowledged {
                Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged
            } else {
                Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
            }
        );
        let owner = resolved.into_ready_owner();
        let expected_classification = match relation {
            P1eI0TimerFixtureRelation::SameAuthenticatedOwner => P1eI0TimerClassification::Retained,
            P1eI0TimerFixtureRelation::ForeignAuthenticatedOwner => P1eI0TimerClassification::Stale,
        };
        if matches!(
            signal_checkpoint,
            Some(P1eI0TimerSignalCheckpoint::AfterSourceBeforeReclassification)
        ) {
            assert!(latch.request(Stage8bP1eShutdownIntentV1::new(
                Stage8bP1eShutdownCauseV1::ExternalSignal,
                20_000,
                61,
            )));
        }
        let after_source = p1e_i0_after_source_timer_latch_decision(owner, timer, &latch);
        let (owner, timer, observed_classification, executed) = match after_source {
            P1eI0AfterSourceTimerDecisionV1::RetainForRestart { owner, timer } => {
                assert_eq!(
                    signal_checkpoint,
                    Some(P1eI0TimerSignalCheckpoint::AfterSourceBeforeReclassification)
                );
                assert_eq!(timer, original_timer, "first latch retains original timer");
                (owner, timer, None, false)
            }
            P1eI0AfterSourceTimerDecisionV1::Reclassify { owner, timer } => {
                let classified =
                    p1e_i0_reclassify_timer(owner, timer, P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000);
                assert_eq!(classified.timer, original_timer);
                assert_eq!(classified.classification, expected_classification);
                if matches!(
                    signal_checkpoint,
                    Some(P1eI0TimerSignalCheckpoint::AfterReclassificationBeforeExecution)
                ) {
                    assert!(latch.request(Stage8bP1eShutdownIntentV1::new(
                        Stage8bP1eShutdownCauseV1::ExternalSignal,
                        20_000,
                        62,
                    )));
                }
                match p1e_i0_after_reclassification_latch_decision(classified, &latch) {
                    P1eI0AfterReclassificationDecisionV1::RetainForRestart(classified) => {
                        assert_eq!(
                            signal_checkpoint,
                            Some(P1eI0TimerSignalCheckpoint::AfterReclassificationBeforeExecution)
                        );
                        (
                            classified.owner,
                            classified.timer,
                            Some(classified.classification),
                            false,
                        )
                    }
                    P1eI0AfterReclassificationDecisionV1::ExecuteOnNextOwnerLoop(classified) => {
                        assert!(signal_checkpoint.is_none());
                        let (owner, timer, classification, executed) =
                            p1e_i0_execute_timer_on_next_owner_loop(classified);
                        (owner, timer, Some(classification), executed)
                    }
                }
            }
        };
        assert_eq!(timer, original_timer);
        match relation {
            P1eI0TimerFixtureRelation::SameAuthenticatedOwner => assert_eq!(
                owner.stage7.stage8b_p1_operational_identity_sha256(),
                timer.owner_operational_identity_sha256
            ),
            P1eI0TimerFixtureRelation::ForeignAuthenticatedOwner => assert_ne!(
                owner.stage7.stage8b_p1_operational_identity_sha256(),
                timer.owner_operational_identity_sha256,
                "stale timer must be authentically bound to a different owner"
            ),
        }
        if signal_checkpoint.is_some() {
            assert!(latch.intent().is_some());
            assert!(!executed);
        } else {
            assert!(latch.intent().is_none());
            assert_eq!(observed_classification, Some(expected_classification));
            assert_eq!(
                executed,
                matches!(expected_classification, P1eI0TimerClassification::Retained)
            );
        }

        let observed = p1e_i0_take_effect_audit();
        assert_eq!(observed.xack_total, u64::from(!source_already_acknowledged));
        assert_eq!(
            observed.timer_reclassification_total,
            u64::from(matches!(
                signal_checkpoint,
                Some(P1eI0TimerSignalCheckpoint::AfterReclassificationBeforeExecution) | None
            ))
        );
        assert_eq!(observed.timer_execution_total, u64::from(executed));
        assert_eq!(observed.replacement_seal_commit_total, 0);
        assert_eq!(observed.callback_total, 0);
        assert_eq!(observed.publication_total, 0);
        assert_eq!(observed.publication_revalidation_total, 0);

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0, "source must resolve before timer work");
        drop(owner);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1e_i0_source_first_timer_checkpoints_never_execute_timer_in_same_step() {
        for source_already_acknowledged in [false, true] {
            for relation in [
                P1eI0TimerFixtureRelation::SameAuthenticatedOwner,
                P1eI0TimerFixtureRelation::ForeignAuthenticatedOwner,
            ] {
                for signal_checkpoint in [
                    P1eI0TimerSignalCheckpoint::AfterSourceBeforeReclassification,
                    P1eI0TimerSignalCheckpoint::AfterReclassificationBeforeExecution,
                ] {
                    p1e_i0_assert_source_first_timer_checkpoint(
                        source_already_acknowledged,
                        relation,
                        Some(signal_checkpoint),
                    )
                    .await;
                }
            }
        }
    }

    #[tokio::test]
    async fn p1e_i0_timer_execution_probe_has_a_clear_latch_next_step_positive_control() {
        for source_already_acknowledged in [false, true] {
            for relation in [
                P1eI0TimerFixtureRelation::SameAuthenticatedOwner,
                P1eI0TimerFixtureRelation::ForeignAuthenticatedOwner,
            ] {
                p1e_i0_assert_source_first_timer_checkpoint(
                    source_already_acknowledged,
                    relation,
                    None,
                )
                .await;
            }
        }
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

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    pub(crate) async fn p1e_test_plain_market_published(
        redis_url: &str,
        parent: &Path,
    ) -> (
        Stage8bP1RedisCommandPublished,
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        i64,
    ) {
        let (mut pending, key, fresh, identity) = one_intent_pending_at(redis_url, parent).await;
        let candidate_close_ms = 1_785_759_600_000;
        pending
            .transport
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), candidate_close_ms, 2_650),
                &identity,
            )
            .await
            .unwrap();
        let published = pending.publish_exact_command().await.unwrap();
        assert_eq!(
            published.p1e_schedule_route(),
            Stage8bP1ePublishedScheduleRouteV1::PlainMarket
        );
        (published, key, fresh, identity, candidate_close_ms)
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

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    pub(crate) async fn p1e_test_initial_limit_published(
        redis_url: &str,
        parent: &Path,
    ) -> (
        Stage8bP1RedisCommandPublished,
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        i64,
    ) {
        let (key, fresh, identity, published) =
            prepare_p1d3_initial_limit_source(redis_url, parent, 2_220).await;
        (
            published,
            key,
            fresh,
            identity,
            P1D3_CANCEL_DECISION_CLOSE_MS,
        )
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    pub(crate) async fn p1e_test_commit_initial_limit_v4_only(
        mut published: Stage8bP1RedisCommandPublished,
        snapshot: crate::Stage8bP1eVerifiedScheduleSnapshotV1,
        bound_at_utc: DateTime<Utc>,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> crate::Stage8bP1eScheduleBindingCommitReceipt {
        assert_eq!(
            published.p1e_schedule_route(),
            Stage8bP1ePublishedScheduleRouteV1::InitialLimit
        );
        let operational_identity_sha256 = published
            .stage7
            .stage8b_p1_operational_identity_sha256()
            .to_string();
        let predecessor = published
            .pending_m10
            .parse_exact(&operational_identity_sha256)
            .unwrap();
        let candidate = published
            .transport
            .backend
            .exact_first_successor_m10(
                published.pending_m10.redis_id(),
                &operational_identity_sha256,
            )
            .await
            .unwrap();
        let request_id = published.evidence.strategy_request_id.unwrap();
        let command_sha256 = published.evidence.canonical_command_sha256.clone().unwrap();
        let predecessor = stage8b_p1e_m10_identity_from_validated(&predecessor);
        let candidate = stage8b_p1e_m10_identity_from_validated(&candidate);
        let committed = crate::bind_stage8b_p1e_initial_limit_schedule(
            published.stage7,
            snapshot,
            &Stage8bP1eShutdownLatchV1::new(),
            &predecessor,
            &candidate,
            request_id.to_string(),
            command_sha256,
            published.receipt.covering_seal_generation,
            published.receipt.covering_seal_commitment_sha256.clone(),
            bound_at_utc,
            commitment_key,
        )
        .unwrap();
        let crate::Stage8bP1eScheduleBindingCommitV1::Committed(owner) = committed else {
            panic!("clear test latch must commit Initial LIMIT V4")
        };
        let receipt = owner.receipt().clone();
        drop(owner);
        drop(published.transport);
        drop(published.pending_m10);
        receipt
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    pub(crate) async fn p1e_test_commit_generated_market_v4_only(
        mut published: Stage8bP1RedisCommandPublished,
        snapshot: crate::Stage8bP1eVerifiedScheduleSnapshotV1,
        bound_at_utc: DateTime<Utc>,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> crate::Stage8bP1eScheduleBindingCommitReceipt {
        assert_eq!(
            published.p1e_schedule_route(),
            Stage8bP1ePublishedScheduleRouteV1::GeneratedMarket
        );
        let binding = published.p1d4_binding.as_ref().unwrap();
        let publication_seal_generation = binding.prepublication_seal_generation();
        let publication_seal_commitment_sha256 =
            binding.prepublication_seal_commitment_sha256().to_string();
        let operational_identity_sha256 = published
            .stage7
            .stage8b_p1_operational_identity_sha256()
            .to_string();
        let predecessor = published
            .pending_m10
            .parse_exact(&operational_identity_sha256)
            .unwrap();
        let candidate = published
            .transport
            .backend
            .exact_first_successor_m10(
                published.pending_m10.redis_id(),
                &operational_identity_sha256,
            )
            .await
            .unwrap();
        let request_id = published.evidence.strategy_request_id.unwrap();
        let command_sha256 = published.evidence.canonical_command_sha256.clone().unwrap();
        let predecessor = stage8b_p1e_m10_identity_from_validated(&predecessor);
        let candidate = stage8b_p1e_m10_identity_from_validated(&candidate);
        let committed = crate::bind_stage8b_p1e_generated_market_schedule(
            published.stage7,
            snapshot,
            &Stage8bP1eShutdownLatchV1::new(),
            &predecessor,
            &candidate,
            request_id.to_string(),
            command_sha256,
            publication_seal_generation,
            publication_seal_commitment_sha256,
            bound_at_utc,
            commitment_key,
        )
        .unwrap();
        let crate::Stage8bP1eScheduleBindingCommitV1::Committed(owner) = committed else {
            panic!("clear test latch must commit generated-Market V4")
        };
        let receipt = owner.receipt().clone();
        drop(owner);
        drop(published.transport);
        drop(published.pending_m10);
        receipt
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

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    pub(crate) async fn p1e_test_generated_market_published(
        redis_url: &str,
        parent: &Path,
    ) -> (
        Stage8bP1RedisCommandPublished,
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        i64,
    ) {
        let (key, fresh, identity, pending, decision_close_ms) =
            prepare_p1d4_generated_market_prepublication(redis_url, parent).await;
        let published = pending
            .publish_exact_generated_market_command(&key)
            .await
            .unwrap();
        assert_eq!(
            published.p1e_schedule_route(),
            Stage8bP1ePublishedScheduleRouteV1::GeneratedMarket
        );
        (published, key, fresh, identity, decision_close_ms + 600_000)
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn p1e_i1_published_market_signed_schedule_commits_ack_and_retains_source() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i1-published-market-signed-schedule");
        let (mut pending, key, fresh, identity) = one_intent_pending(&redis, &parent).await;
        let candidate_close_ms = 1_785_759_600_000;
        pending
            .transport
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), candidate_close_ms, 2_650),
                &identity,
            )
            .await
            .unwrap();
        let published = pending.publish_exact_command().await.unwrap();
        assert_eq!(
            published.p1e_schedule_route(),
            Stage8bP1ePublishedScheduleRouteV1::PlainMarket
        );
        let bound_at = Utc
            .timestamp_millis_opt(candidate_close_ms)
            .single()
            .unwrap();
        let snapshot = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_snapshot(
            identity,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            format!("{candidate_close_ms}-1"),
            bound_at,
        );
        let outcome = resume_stage8b_p1e_command_published_with_signed_schedule(
            published,
            snapshot,
            &Stage8bP1eShutdownLatchV1::new(),
            bound_at,
            &key,
        )
        .await
        .unwrap();
        assert!(matches!(
            outcome,
            Stage8bP1eSignedMarketScheduleOutcomeV1::FeedbackAckCommitted(_)
        ));

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            pending.count(),
            1,
            "signed Market binding must stop at S_ack before source XACK"
        );
        drop(connection);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn p1e_i1_generated_market_signed_schedule_commits_combined_ack_and_retains_source() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i1-generated-market-signed-schedule");
        let (published, key, fresh, identity, candidate_close_ms) =
            p1e_test_generated_market_published(&redis.url, &parent).await;
        let bound_at = Utc
            .timestamp_millis_opt(candidate_close_ms)
            .single()
            .unwrap();
        let snapshot = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_snapshot(
            identity,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            format!("{candidate_close_ms}-1"),
            bound_at,
        );
        let outcome = resume_stage8b_p1e_generated_market_with_signed_schedule(
            published,
            snapshot,
            &Stage8bP1eShutdownLatchV1::new(),
            bound_at,
            &key,
        )
        .await
        .unwrap();
        assert!(matches!(
            outcome,
            Stage8bP1eSignedGeneratedMarketScheduleOutcomeV1::GeneratedMarketAckCommitted(_)
        ));

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            pending.count(),
            1,
            "generated Market binding must stop at combined S_ack before source XACK"
        );
        drop(connection);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1e_i1_published_schedule_route_separates_initial_limit() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i1-published-initial-limit-route");
        let (_, _, _, published) =
            prepare_p1d3_initial_limit_source(&redis.url, &parent, 2_220).await;
        assert_eq!(
            published.p1e_schedule_route(),
            Stage8bP1ePublishedScheduleRouteV1::InitialLimit
        );
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1e_i1_ready_delivery_routes_working_limit_without_second_acquisition() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i1-ready-working-limit-route");
        let (key, _, identity, mut owner) =
            prepare_p1d4_later_working_owner(&redis.url, &parent, false, false, None).await;
        let candidate_close_ms = P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000;
        owner
            .transport_mut()
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), candidate_close_ms, 2_220),
                &identity,
            )
            .await
            .unwrap();

        let Stage8bP1eReadyFreshAcquisitionOutcomeV1::Acquired(acquired) =
            poll_stage8b_p1e_ready_fresh_with_redis(owner)
                .await
                .unwrap()
        else {
            panic!("one fresh working-LIMIT source must be acquired");
        };
        assert_eq!(
            acquired.ready_source_route(),
            Some(Stage8bP1eReadySourceRouteV1::WorkingLimit)
        );
        let Stage8bP1ePostAcquisitionRouteV1::ReadyWorkingLimit { claimed } =
            acquired.route.as_ref()
        else {
            unreachable!("working-LIMIT route was asserted above")
        };
        assert_eq!(
            claimed.semantic_m10_identity.broker_id,
            STAGE8B_P1_BROKER_ID
        );
        assert_eq!(
            claimed.semantic_m10_identity.internal_symbol,
            STAGE8B_P1_INTERNAL_SYMBOL
        );
        assert_eq!(
            claimed.semantic_m10_identity.venue_symbol,
            STAGE8B_P1_VENUE_SYMBOL
        );
        assert_eq!(claimed.semantic_m10_identity.exchange, STAGE8B_P1_EXCHANGE);
        assert_eq!(claimed.semantic_m10_identity.market, STAGE8B_P1_MARKET);
        assert_eq!(claimed.semantic_m10_identity.timeframe_sec, 600);
        assert_eq!(
            claimed.semantic_m10_identity.open_ts_utc_ms,
            candidate_close_ms - 600_000
        );
        assert_eq!(
            claimed.semantic_m10_identity.close_ts_utc_ms,
            candidate_close_ms
        );
        assert_eq!(
            claimed.semantic_m10_identity.source_kind,
            "finam_derived_m1_to_m10_complete"
        );
        let permit = p1e_clear_permit(acquired);
        let outcome = resume_stage8b_p1e_ready_working_limit_source_with_redis(
            permit,
            p1d3_cancel_schedule(P1D3_CANCEL_CANDIDATE_CLOSE_MS, candidate_close_ms),
            &key,
        )
        .await
        .unwrap();
        assert!(matches!(
            outcome,
            Stage8bP1RedisSemanticOutcome::Ready { .. }
        ));

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0, "claimed continuation must XACK once");
        drop(connection);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1e_i1_empty_fresh_poll_returns_the_exact_ready_owner() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i1-empty-fresh-poll");
        let (stage7, _, _, identity) = first_boot(&parent);
        let transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let owner = Stage8bP1RedisSemanticCompositionOwner::new(stage7, transport);

        let Stage8bP1eReadyFreshAcquisitionOutcomeV1::EmptyFreshPoll(mut owner) =
            poll_stage8b_p1e_ready_fresh_with_redis(owner)
                .await
                .unwrap()
        else {
            panic!("an empty bounded poll must return the exact Ready owner")
        };

        owner
            .transport_mut()
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), 1_785_759_000_000, 2_600),
                &identity,
            )
            .await
            .unwrap();
        assert!(matches!(
            poll_stage8b_p1e_ready_fresh_with_redis(*owner)
                .await
                .unwrap(),
            Stage8bP1eReadyFreshAcquisitionOutcomeV1::Acquired(_)
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn p1e_i1_ready_working_limit_signed_schedule_composes_c_through_f_and_xacks_last() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i1-ready-working-signed-schedule");
        let (key, fresh, identity, mut owner) =
            prepare_p1d4_later_working_owner(&redis.url, &parent, false, false, None).await;
        let candidate_close_ms = P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000;
        owner
            .transport_mut()
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), candidate_close_ms, 2_220),
                &identity,
            )
            .await
            .unwrap();

        let Stage8bP1eReadyFreshAcquisitionOutcomeV1::Acquired(acquired) =
            poll_stage8b_p1e_ready_fresh_with_redis(owner)
                .await
                .unwrap()
        else {
            panic!("one fresh Working-LIMIT source must be acquired")
        };
        let clear_latch = Stage8bP1eShutdownLatchV1::new();
        let Stage8bP1ePostAcquisitionDecisionV1::Continue(permit) =
            decide_stage8b_p1e_post_acquisition_latch(acquired, &clear_latch)
        else {
            panic!("clear latch B must issue one route-exact permit")
        };
        let bound_at = Utc
            .timestamp_millis_opt(candidate_close_ms)
            .single()
            .unwrap();
        let snapshot = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_snapshot(
            identity.clone(),
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            format!("{candidate_close_ms}-1"),
            bound_at,
        );
        let outcome = resume_stage8b_p1e_ready_working_limit_with_signed_schedule(
            permit,
            snapshot,
            &clear_latch,
            bound_at,
            &key,
        )
        .await
        .unwrap();
        assert!(matches!(
            outcome,
            Stage8bP1eSignedWorkingScheduleOutcomeV1::Semantic(
                Stage8bP1RedisSemanticOutcome::Ready { .. }
            )
        ));

        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0, "source XACK must remain lifecycle-last");
        drop(connection);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn p1e_i1_ready_delivery_routes_ordinary_semantic_without_schedule() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1e-i1-ready-ordinary-route");
        let (stage7, key, _, identity) = first_boot(&parent);
        let mut transport = initialize_stage8b_p1_redis_namespace(
            &redis.url,
            Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        transport
            .publish_canonical_m10(
                &canonical_m10(identity.clone(), 1_785_759_000_000, 2_600),
                &identity,
            )
            .await
            .unwrap();
        let owner = Stage8bP1RedisSemanticCompositionOwner::new(stage7, transport);
        let Stage8bP1eReadyFreshAcquisitionOutcomeV1::Acquired(acquired) =
            poll_stage8b_p1e_ready_fresh_with_redis(owner)
                .await
                .unwrap()
        else {
            panic!("one fresh ordinary Ready source must be acquired");
        };
        assert_eq!(
            acquired.ready_source_route(),
            Some(Stage8bP1eReadySourceRouteV1::Semantic)
        );
        let permit = p1e_clear_permit(acquired);
        assert!(matches!(
            resume_stage8b_p1e_ready_source_with_redis(permit, &key)
                .await
                .unwrap(),
            Stage8bP1RedisSemanticOutcome::Ready { .. }
        ));
        fs::remove_dir_all(parent).unwrap();
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
        assert_eq!(
            published.p1e_schedule_route(),
            Stage8bP1ePublishedScheduleRouteV1::GeneratedMarket
        );
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
    async fn p1e_command_publication_revalidation_is_read_only_and_seal_exact() {
        let redis = RedisServer::start().await;
        let mut connection = redis.connection().await;
        let source_stream = "p1e:revalidate:source";
        let source_group = "p1e-revalidate-source-group";
        let source_id = "10-1";
        let source_payload = "canonical-source-payload";
        let command_stream = "p1e:revalidate:commands";
        let command_group = "p1e-revalidate-command-group";
        let command_id = "20-1";
        let command_payload = "canonical-command-envelope";
        let marker_key = "p1e:revalidate:marker";
        let semantic_batch_id = "11".repeat(32);
        let request_id = "request-initial-limit-revalidate";
        let command_sha256 = "22".repeat(32);
        let envelope_sha256 = "33".repeat(32);
        let seal_generation = 17_u64;
        let seal_commitment = "44".repeat(32);

        let _: String = redis::cmd("XADD")
            .arg(source_stream)
            .arg(source_id)
            .arg("payload")
            .arg(source_payload)
            .query_async(&mut connection)
            .await
            .unwrap();
        let _: () = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(source_stream)
            .arg(source_group)
            .arg("0-0")
            .query_async(&mut connection)
            .await
            .unwrap();
        let _: redis::streams::StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(source_group)
            .arg("p1e-revalidate-consumer")
            .arg("COUNT")
            .arg(1)
            .arg("STREAMS")
            .arg(source_stream)
            .arg(">")
            .query_async(&mut connection)
            .await
            .unwrap();
        let _: () = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(command_stream)
            .arg(command_group)
            .arg("0-0")
            .arg("MKSTREAM")
            .query_async(&mut connection)
            .await
            .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(command_stream)
            .arg(command_id)
            .arg("payload")
            .arg(command_payload)
            .query_async(&mut connection)
            .await
            .unwrap();
        let marker = serde_json::json!({
            "schema_version": COMMAND_PUBLICATION_MARKER_SCHEMA_VERSION,
            "domain": COMMAND_PUBLICATION_MARKER_DOMAIN,
            "source_stream": source_stream,
            "source_group": source_group,
            "source_id": source_id,
            "semantic_batch_id_sha256": semantic_batch_id,
            "strategy_request_id": request_id,
            "canonical_command_sha256": command_sha256,
            "canonical_envelope_sha256": envelope_sha256,
            "command_stream": command_stream,
            "command_group": command_group,
            "command_entry_id": command_id,
            "seal_generation": seal_generation,
            "seal_commitment_sha256": seal_commitment,
        })
        .to_string();
        let _: () = redis::cmd("SET")
            .arg(marker_key)
            .arg(&marker)
            .query_async(&mut connection)
            .await
            .unwrap();

        macro_rules! revalidate {
            ($generation:expr, $commitment:expr) => {
                redis::cmd("EVAL")
                    .arg(COMMAND_PUBLICATION_REVALIDATE_LUA)
                    .arg(3)
                    .arg(source_stream)
                    .arg(command_stream)
                    .arg(marker_key)
                    .arg(source_group)
                    .arg(source_id)
                    .arg(source_payload)
                    .arg(&semantic_batch_id)
                    .arg(request_id)
                    .arg(&command_sha256)
                    .arg(&envelope_sha256)
                    .arg(command_payload)
                    .arg($generation)
                    .arg($commitment)
                    .arg(COMMAND_PUBLICATION_MARKER_SCHEMA_VERSION)
                    .arg(COMMAND_PUBLICATION_MARKER_DOMAIN)
                    .arg(command_group)
                    .query_async::<Vec<String>>(&mut connection)
                    .await
            };
        }

        assert_eq!(
            revalidate!(seal_generation, &seal_commitment).unwrap(),
            vec!["existing", command_id]
        );
        assert!(revalidate!(seal_generation + 1, &seal_commitment).is_err());
        assert!(revalidate!(seal_generation, "55".repeat(32)).is_err());
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
        assert_eq!(stored_marker, marker);
        assert_eq!(command_count, 1, "revalidation must never append a command");
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

    trait P1d4RegistryIdentity {
        fn cell_id(&self) -> &str;
        fn scenario_id(&self) -> &str;
        fn frontier_id(&self) -> &str;
        fn kill_hook_name(&self) -> &str;
        fn expected_restart_disposition(&self) -> &str;
    }

    #[derive(Debug, Clone)]
    struct P1d4RegistryCell<'a> {
        cell_id: &'a str,
        scenario_id: &'a str,
        semantic_family: &'a str,
        source_kind: &'a str,
        frontier_id: &'a str,
        precondition: &'a str,
        kill_hook_name: &'a str,
        expected_restart_disposition: &'a str,
        only_legal_continuation: &'a str,
        sequence_expectation: &'a str,
        callback_delta: &'a str,
        provider_delta: &'a str,
        schedule_authority_delta: &'a str,
        pel_before: &'a str,
        pel_after: &'a str,
        xack_expectation: &'a str,
        duplicate_variant_required: bool,
        conflict_variant_required: bool,
        inherited_or_new_test_id: &'a str,
    }

    #[derive(Debug, Clone)]
    struct P1d4BaseEvidenceOracleCell<'a> {
        cell_id: &'a str,
        pre_kill_allocation_kinds: &'a str,
        final_allocation_kinds: &'a str,
        ordered_effect_events: &'a str,
        package_before_p1d3_phase: &'a str,
        package_before_generated_market_phase: &'a str,
        package_before_write_generation: u64,
        package_after_p1d3_phase: &'a str,
        package_after_generated_market_phase: &'a str,
        package_after_write_generation: u64,
        write_generation_advance: u64,
        truth_bearing_outcomes: usize,
        truth_replacement_commits: usize,
    }

    #[derive(Debug, Clone)]
    struct P1d4BaseOperationalEvidenceOracleCell<'a> {
        cell_id: &'a str,
        callback_before: usize,
        callback_after: usize,
        command_publications: usize,
        immediate_xack_attempts: u64,
        pre_kill_xack_reply: &'a str,
        xack_reply: &'a str,
        xack_disposition: &'a str,
        source_disposition_before_continuation: &'a str,
        source_stream: &'a str,
        source_group: &'a str,
        source_m10_redis_id: &'a str,
        before_last_delivered_id: &'a str,
        before_pending: usize,
        post_restart_last_delivered_id: &'a str,
        post_restart_pending: usize,
        final_last_delivered_id: &'a str,
        final_pending: usize,
    }

    impl P1d4RegistryIdentity for P1d4RegistryCell<'_> {
        fn cell_id(&self) -> &str {
            self.cell_id
        }
        fn scenario_id(&self) -> &str {
            self.scenario_id
        }
        fn frontier_id(&self) -> &str {
            self.frontier_id
        }
        fn kill_hook_name(&self) -> &str {
            self.kill_hook_name
        }
        fn expected_restart_disposition(&self) -> &str {
            self.expected_restart_disposition
        }
    }

    impl P1d4RegistryCell<'_> {
        fn registry_fields(&self) -> BTreeMap<String, serde_json::Value> {
            BTreeMap::from([
                ("cell_id".into(), serde_json::json!(self.cell_id)),
                ("scenario_id".into(), serde_json::json!(self.scenario_id)),
                (
                    "semantic_family".into(),
                    serde_json::json!(self.semantic_family),
                ),
                ("source_kind".into(), serde_json::json!(self.source_kind)),
                ("frontier_id".into(), serde_json::json!(self.frontier_id)),
                ("precondition".into(), serde_json::json!(self.precondition)),
                (
                    "kill_hook_name".into(),
                    serde_json::json!(self.kill_hook_name),
                ),
                (
                    "expected_restart_disposition".into(),
                    serde_json::json!(self.expected_restart_disposition),
                ),
                (
                    "only_legal_continuation".into(),
                    serde_json::json!(self.only_legal_continuation),
                ),
                (
                    "sequence_expectation".into(),
                    serde_json::json!(self.sequence_expectation),
                ),
                (
                    "callback_delta".into(),
                    serde_json::json!(self.callback_delta),
                ),
                (
                    "provider_delta".into(),
                    serde_json::json!(self.provider_delta),
                ),
                (
                    "schedule_authority_delta".into(),
                    serde_json::json!(self.schedule_authority_delta),
                ),
                ("pel_before".into(), serde_json::json!(self.pel_before)),
                ("pel_after".into(), serde_json::json!(self.pel_after)),
                (
                    "xack_expectation".into(),
                    serde_json::json!(self.xack_expectation),
                ),
                (
                    "duplicate_variant_required".into(),
                    serde_json::json!(self.duplicate_variant_required),
                ),
                (
                    "conflict_variant_required".into(),
                    serde_json::json!(self.conflict_variant_required),
                ),
                (
                    "inherited_or_new_test_id".into(),
                    serde_json::json!(self.inherited_or_new_test_id),
                ),
            ])
        }
    }

    #[derive(Debug, Clone)]
    struct P1d4GeneratedMarketRegistryCell<'a> {
        cell_id: &'a str,
        parent_scenario_id: &'a str,
        frontier_id: &'a str,
        precondition: &'a str,
        stage6_durable_frontier: &'a str,
        expected_restart_disposition: &'a str,
        classifier_route: &'a str,
        only_legal_continuation: &'a str,
        source_pel_before: &'a str,
        source_pel_after: &'a str,
        publication_reservation_expectation: &'a str,
        publication_binding_expectation: &'a str,
        bar_callback_delta: &'a str,
        command_publication_delta: &'a str,
        provider_delta: &'a str,
        schedule_authority_delta: &'a str,
        dispatch_v1_total: &'a str,
        order_v1_total: &'a str,
        trade_v1_total: &'a str,
        request_finalized_v1_total: &'a str,
        sequence_expectation: &'a str,
        s_ack_delta: &'a str,
        s_truth_delta: &'a str,
        xack_delta: &'a str,
        final_source_disposition: &'a str,
        duplicate_variant_required: bool,
        conflict_variant_required: bool,
        test_id: &'a str,
        kill_hook_name: &'a str,
    }

    impl P1d4RegistryIdentity for P1d4GeneratedMarketRegistryCell<'_> {
        fn cell_id(&self) -> &str {
            self.cell_id
        }
        fn scenario_id(&self) -> &str {
            self.parent_scenario_id
        }
        fn frontier_id(&self) -> &str {
            self.frontier_id
        }
        fn kill_hook_name(&self) -> &str {
            self.kill_hook_name
        }
        fn expected_restart_disposition(&self) -> &str {
            self.expected_restart_disposition
        }
    }

    impl P1d4GeneratedMarketRegistryCell<'_> {
        fn registry_fields(&self) -> BTreeMap<String, serde_json::Value> {
            BTreeMap::from([
                ("cell_id".into(), serde_json::json!(self.cell_id)),
                (
                    "parent_scenario_id".into(),
                    serde_json::json!(self.parent_scenario_id),
                ),
                ("frontier_id".into(), serde_json::json!(self.frontier_id)),
                ("precondition".into(), serde_json::json!(self.precondition)),
                (
                    "stage6_durable_frontier".into(),
                    serde_json::json!(self.stage6_durable_frontier),
                ),
                (
                    "expected_restart_disposition".into(),
                    serde_json::json!(self.expected_restart_disposition),
                ),
                (
                    "classifier_route".into(),
                    serde_json::json!(self.classifier_route),
                ),
                (
                    "only_legal_continuation".into(),
                    serde_json::json!(self.only_legal_continuation),
                ),
                (
                    "source_pel_before".into(),
                    serde_json::json!(self.source_pel_before),
                ),
                (
                    "source_pel_after".into(),
                    serde_json::json!(self.source_pel_after),
                ),
                (
                    "publication_reservation_expectation".into(),
                    serde_json::json!(self.publication_reservation_expectation),
                ),
                (
                    "publication_binding_expectation".into(),
                    serde_json::json!(self.publication_binding_expectation),
                ),
                (
                    "bar_callback_delta".into(),
                    serde_json::json!(self.bar_callback_delta),
                ),
                (
                    "command_publication_delta".into(),
                    serde_json::json!(self.command_publication_delta),
                ),
                (
                    "provider_delta".into(),
                    serde_json::json!(self.provider_delta),
                ),
                (
                    "schedule_authority_delta".into(),
                    serde_json::json!(self.schedule_authority_delta),
                ),
                (
                    "dispatch_v1_total".into(),
                    serde_json::json!(self.dispatch_v1_total),
                ),
                (
                    "order_v1_total".into(),
                    serde_json::json!(self.order_v1_total),
                ),
                (
                    "trade_v1_total".into(),
                    serde_json::json!(self.trade_v1_total),
                ),
                (
                    "request_finalized_v1_total".into(),
                    serde_json::json!(self.request_finalized_v1_total),
                ),
                (
                    "sequence_expectation".into(),
                    serde_json::json!(self.sequence_expectation),
                ),
                ("s_ack_delta".into(), serde_json::json!(self.s_ack_delta)),
                (
                    "s_truth_delta".into(),
                    serde_json::json!(self.s_truth_delta),
                ),
                ("xack_delta".into(), serde_json::json!(self.xack_delta)),
                (
                    "final_source_disposition".into(),
                    serde_json::json!(self.final_source_disposition),
                ),
                (
                    "duplicate_variant_required".into(),
                    serde_json::json!(self.duplicate_variant_required),
                ),
                (
                    "conflict_variant_required".into(),
                    serde_json::json!(self.conflict_variant_required),
                ),
                ("test_id".into(), serde_json::json!(self.test_id)),
            ])
        }
    }

    #[derive(Debug, Clone, Serialize)]
    struct P1d4CrashProcessEvidence {
        child_pid: u32,
        exit_code: Option<i32>,
        exit_signal: i32,
        reaped: bool,
        wall_duration_ms: u64,
        raw_marker_sha256: String,
        normalized_marker_sha256: String,
        pre_kill_filesystem_sha256: String,
        pre_kill_xack_reply: String,
        pre_kill_xack_reply_witness_v1: Option<serde_json::Value>,
        pre_kill_xack_reply_witness_sha256: Option<String>,
        pre_kill_xack_witness_order: String,
        sequence_pair_before_kill: Option<(u64, u64)>,
    }

    fn p1d4_required_bool(value: &str, field: &str) -> bool {
        match value {
            "true" => true,
            "false" => false,
            _ => panic!("P1-d4 {field} must be canonical true/false"),
        }
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
                    semantic_family: fields[2],
                    source_kind: fields[3],
                    frontier_id: fields[4],
                    precondition: fields[5],
                    kill_hook_name: fields[6],
                    expected_restart_disposition: fields[7],
                    only_legal_continuation: fields[8],
                    sequence_expectation: fields[9],
                    callback_delta: fields[10],
                    provider_delta: fields[11],
                    schedule_authority_delta: fields[12],
                    pel_before: fields[13],
                    pel_after: fields[14],
                    xack_expectation: fields[15],
                    duplicate_variant_required: p1d4_required_bool(fields[16], "duplicate variant"),
                    conflict_variant_required: p1d4_required_bool(fields[17], "conflict variant"),
                    inherited_or_new_test_id: fields[18],
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

    fn p1d4_base_evidence_oracle_cells() -> Vec<P1d4BaseEvidenceOracleCell<'static>> {
        let oracle = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/stage-8/stage8b-p1d4-base-evidence-oracle-v1.csv"
        ));
        oracle
            .lines()
            .skip(1)
            .map(|line| {
                let fields = line.split(',').collect::<Vec<_>>();
                assert_eq!(
                    fields.len(),
                    13,
                    "P1-d4 base evidence oracle row must remain exact"
                );
                P1d4BaseEvidenceOracleCell {
                    cell_id: fields[0],
                    pre_kill_allocation_kinds: fields[1],
                    final_allocation_kinds: fields[2],
                    ordered_effect_events: fields[3],
                    package_before_p1d3_phase: fields[4],
                    package_before_generated_market_phase: fields[5],
                    package_before_write_generation: fields[6]
                        .parse()
                        .expect("base oracle before generation"),
                    package_after_p1d3_phase: fields[7],
                    package_after_generated_market_phase: fields[8],
                    package_after_write_generation: fields[9]
                        .parse()
                        .expect("base oracle after generation"),
                    write_generation_advance: fields[10]
                        .parse()
                        .expect("base oracle generation advance"),
                    truth_bearing_outcomes: fields[11]
                        .parse()
                        .expect("base oracle truth-bearing outcomes"),
                    truth_replacement_commits: fields[12]
                        .parse()
                        .expect("base oracle truth replacement commits"),
                }
            })
            .collect()
    }

    fn p1d4_base_evidence_oracle_cell(cell_id: &str) -> P1d4BaseEvidenceOracleCell<'static> {
        let matches = p1d4_base_evidence_oracle_cells()
            .into_iter()
            .filter(|cell| cell.cell_id == cell_id)
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "P1-d4 base evidence oracle lookup");
        matches.into_iter().next().unwrap()
    }

    fn p1d4_base_operational_evidence_oracle_cells(
    ) -> Vec<P1d4BaseOperationalEvidenceOracleCell<'static>> {
        let oracle = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/stage-8/stage8b-p1d4-base-operational-evidence-oracle-v1.csv"
        ));
        oracle
            .lines()
            .skip(1)
            .map(|line| {
                let fields = line.split(',').collect::<Vec<_>>();
                assert_eq!(
                    fields.len(),
                    18,
                    "P1-d4 base operational evidence oracle row must remain exact"
                );
                P1d4BaseOperationalEvidenceOracleCell {
                    cell_id: fields[0],
                    callback_before: fields[1].parse().expect("base callback before"),
                    callback_after: fields[2].parse().expect("base callback after"),
                    command_publications: fields[3].parse().expect("base command publications"),
                    immediate_xack_attempts: fields[4]
                        .parse()
                        .expect("base immediate XACK attempts"),
                    xack_reply: fields[5],
                    xack_disposition: fields[6],
                    source_disposition_before_continuation: fields[7],
                    source_stream: fields[8],
                    source_group: fields[9],
                    source_m10_redis_id: fields[10],
                    before_last_delivered_id: fields[11],
                    before_pending: fields[12].parse().expect("base before PEL"),
                    post_restart_last_delivered_id: fields[13],
                    post_restart_pending: fields[14].parse().expect("base post-restart PEL"),
                    final_last_delivered_id: fields[15],
                    final_pending: fields[16].parse().expect("base final PEL"),
                    pre_kill_xack_reply: fields[17],
                }
            })
            .collect()
    }

    fn p1d4_base_operational_evidence_oracle_cell(
        cell_id: &str,
    ) -> P1d4BaseOperationalEvidenceOracleCell<'static> {
        let matches = p1d4_base_operational_evidence_oracle_cells()
            .into_iter()
            .filter(|cell| cell.cell_id == cell_id)
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "P1-d4 base operational evidence oracle lookup"
        );
        matches.into_iter().next().unwrap()
    }

    fn p1d4_generated_market_registry_cells() -> Vec<P1d4GeneratedMarketRegistryCell<'static>> {
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
                P1d4GeneratedMarketRegistryCell {
                    cell_id: fields[0],
                    parent_scenario_id: fields[1],
                    frontier_id: fields[2],
                    precondition: fields[3],
                    stage6_durable_frontier: fields[4],
                    expected_restart_disposition: fields[5],
                    classifier_route: fields[6],
                    only_legal_continuation: fields[7],
                    source_pel_before: fields[8],
                    source_pel_after: fields[9],
                    publication_reservation_expectation: fields[10],
                    publication_binding_expectation: fields[11],
                    bar_callback_delta: fields[12],
                    command_publication_delta: fields[13],
                    provider_delta: fields[14],
                    schedule_authority_delta: fields[15],
                    dispatch_v1_total: fields[16],
                    order_v1_total: fields[17],
                    trade_v1_total: fields[18],
                    request_finalized_v1_total: fields[19],
                    sequence_expectation: fields[20],
                    s_ack_delta: fields[21],
                    s_truth_delta: fields[22],
                    xack_delta: fields[23],
                    final_source_disposition: fields[24],
                    duplicate_variant_required: p1d4_required_bool(fields[25], "duplicate variant"),
                    conflict_variant_required: p1d4_required_bool(fields[26], "conflict variant"),
                    test_id: fields[27],
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
            Stage7bRestartOutcome::P1eScheduleBindingCommitted(_) => "P1eScheduleBindingCommitted",
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

    #[derive(Debug, Clone, Serialize, PartialEq, Eq)]
    struct P1d4ContinuationEvidence {
        final_disposition: String,
        xack_reply: String,
        xack_disposition: String,
        sequence_after: Option<(u64, u64)>,
        provider_attempts: u64,
        schedule_issue_attempts: u64,
        s_ack_commits: u64,
        s_truth_commits: u64,
        s_ack_generations: Vec<u64>,
        s_truth_generations: Vec<u64>,
        immediate_xack_attempts: u64,
    }

    impl P1d4ContinuationEvidence {
        fn with_immediate_xack(mut self, immediate_xack_attempts: u64) -> Self {
            self.immediate_xack_attempts = immediate_xack_attempts;
            self
        }
    }

    fn p1d4_xack_evidence(
        disposition: Stage8bP1RedisZeroIntentAckDisposition,
        final_disposition: &str,
    ) -> P1d4ContinuationEvidence {
        let (reply, label) = match disposition {
            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending => {
                ("integer:1", "AcknowledgedPending")
            }
            Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged => {
                ("integer:0", "AlreadyAcknowledged")
            }
        };
        P1d4ContinuationEvidence {
            final_disposition: final_disposition.to_string(),
            xack_reply: reply.to_string(),
            xack_disposition: label.to_string(),
            sequence_after: None,
            provider_attempts: 0,
            schedule_issue_attempts: 0,
            s_ack_commits: 0,
            s_truth_commits: 0,
            s_ack_generations: Vec::new(),
            s_truth_generations: Vec::new(),
            immediate_xack_attempts: 0,
        }
    }

    async fn p1d4_finish_limit_truth(
        truth: Stage8bP1RedisLimitTruthCommitted,
    ) -> P1d4ContinuationEvidence {
        assert!(truth.m10_xack_allowed());
        let resolved = Box::pin(truth.acknowledge_source()).await.unwrap();
        p1d4_xack_evidence(resolved.disposition(), "P1d3TruthCommitted")
    }

    async fn p1d4_finish_cancel_outcome(
        outcome: Stage8bP1RedisCancelCommitOutcome,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> P1d4ContinuationEvidence {
        let truth = match outcome {
            Stage8bP1RedisCancelCommitOutcome::AckCommitted(ack) => ack.commit_truth(key).unwrap(),
            Stage8bP1RedisCancelCommitOutcome::TruthCommitted(truth) => truth,
            Stage8bP1RedisCancelCommitOutcome::CancelContinuationPending(pending) => {
                pending.commit_recovered_cancel(key).unwrap()
            }
        };
        Box::pin(p1d4_finish_limit_truth(truth)).await
    }

    async fn p1d4_finish_generated_ack(
        ack: Stage8bP1RedisGeneratedMarketAckCommitted,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> P1d4ContinuationEvidence {
        let truth = ack.commit_truth(key).await.unwrap();
        let audit = truth.audit_evidence().unwrap();
        let sequence_after = Some((audit.core.seq_ack, audit.core.seq_truth));
        let resolved = truth.acknowledge_source().await.unwrap();
        let mut evidence =
            p1d4_xack_evidence(resolved.disposition(), "P1d4GeneratedMarketTruthCommitted");
        evidence.sequence_after = sequence_after;
        evidence
    }

    async fn p1d4_finish_semantic_outcome(
        outcome: Stage8bP1RedisSemanticOutcome,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> P1d4ContinuationEvidence {
        match outcome {
            Stage8bP1RedisSemanticOutcome::Ready {
                ack_disposition, ..
            } => p1d4_xack_evidence(ack_disposition, "Ready"),
            Stage8bP1RedisSemanticOutcome::Prepublication(pending) => {
                let decision_close_ms = P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000;
                let ack = pending
                    .publish_exact_generated_market_command(key)
                    .await
                    .unwrap()
                    .execute_next_canonical_generated_market(
                        strategy_runtime_core::stage8b_p1d1_test_schedule_authority(
                            super::super::p1_instrument(),
                            decision_close_ms,
                            decision_close_ms + 600_000,
                        ),
                        key,
                    )
                    .await
                    .unwrap();
                p1d4_finish_generated_ack(ack, key).await
            }
            Stage8bP1RedisSemanticOutcome::PendingNotClaimable { .. } => {
                panic!("P1-d4 continuation left an exact source temporarily unclaimable")
            }
            Stage8bP1RedisSemanticOutcome::MultiIntentBlocked { .. } => {
                panic!("P1-d4 fixture unexpectedly produced a multi-intent batch")
            }
        }
    }

    fn p1d4_initial_schedule(scenario_id: &str) -> Stage8bP1d3ScheduleStepAuthority {
        let (source_close_ms, candidate_close_ms) = match scenario_id {
            "S01" | "S02" => (P1D3_PLACE_DECISION_CLOSE_MS, P1D3_CANCEL_DECISION_CLOSE_MS),
            "S08" | "S09" | "S10" => (
                P1D3_CANCEL_DECISION_CLOSE_MS,
                P1D3_CANCEL_CANDIDATE_CLOSE_MS,
            ),
            "S11" => (P1D3_PLACE_DECISION_CLOSE_MS, P1D3_CANCEL_CANDIDATE_CLOSE_MS),
            _ => (P1D3_PLACE_DECISION_CLOSE_MS, P1D3_CANCEL_CANDIDATE_CLOSE_MS),
        };
        p1d3_cancel_schedule(source_close_ms, candidate_close_ms)
    }

    fn p1d4_initial_expiry_authority() -> Stage8bP1d3DayExpiryAuthority {
        let trading_day = Utc
            .timestamp_millis_opt(P1D3_PLACE_DECISION_CLOSE_MS)
            .single()
            .unwrap()
            .date_naive()
            .to_string();
        stage8b_p1d3_test_expiry_authority(
            "44".repeat(32),
            trading_day,
            format!("{P1D3_PLACE_DECISION_CLOSE_MS}-0"),
            P1D3_CANCEL_DECISION_CLOSE_MS,
        )
    }

    async fn p1e_i0_retain_authenticated_restart_source(
        restart: Stage7bRestartOutcome,
        redis: &RedisServer,
        scenario_id: &str,
    ) -> Option<&'static str> {
        if scenario_id == "S07" && matches!(&restart, Stage7bRestartOutcome::P1d3PreAckPending(_)) {
            // Day-expiry recovery has authenticated durable authority but no
            // Redis source. It is intentionally outside the 30 source-route
            // cells and must not manufacture an acquisition for this proof.
            return None;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
        let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        p1e_i0_begin_effect_audit();
        macro_rules! acquire_on_heap {
            ($future:expr) => {
                Box::pin($future).await.unwrap()
            };
        }
        let acquired = match restart {
            Stage7bRestartOutcome::Ready(_) => {
                assert_eq!(
                    p1e_i0_take_effect_audit(),
                    P1eI0ObservedEffectCountersV1::default()
                );
                return None;
            }
            Stage7bRestartOutcome::P1SemanticPrepublicationReady(owner) => {
                acquire_on_heap!(acquire_stage8b_p1_prepublication_with_redis(
                    *owner, transport
                ))
            }
            Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(owner) => {
                acquire_on_heap!(acquire_stage8b_p1_zero_intent_ack_with_redis(
                    *owner, transport
                ))
            }
            Stage7bRestartOutcome::P1SemanticPrepublicationPending(owner) => {
                acquire_on_heap!(acquire_stage8b_p1_journal_ahead_with_redis(
                    *owner, transport
                ))
            }
            Stage7bRestartOutcome::P1d2PreAckPending(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d2_pre_ack_with_redis(*owner, transport))
            }
            Stage7bRestartOutcome::P1d2AckCommitted(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d2_ack_with_redis(*owner, transport))
            }
            Stage7bRestartOutcome::P1d2TruthCommitted(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d2_truth_with_redis(*owner, transport))
            }
            Stage7bRestartOutcome::P1d3DispatchPending(owner) => match scenario_id {
                "S03" => acquire_on_heap!(acquire_stage8b_p1d3_dispatch_expiry_with_redis(
                    *owner, transport
                )),
                "S08" | "S09" | "S10" | "S11" => {
                    acquire_on_heap!(acquire_stage8b_p1d3_dispatch_cancel_with_redis(
                        *owner, transport
                    ))
                }
                _ => acquire_on_heap!(acquire_stage8b_p1d3_dispatch_limit_with_redis(
                    *owner, transport
                )),
            },
            Stage7bRestartOutcome::P1d3PreAckPending(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d3_pre_ack_with_redis(*owner, transport))
            }
            Stage7bRestartOutcome::P1d3AckCommitted(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d3_ack_with_redis(*owner, transport))
            }
            Stage7bRestartOutcome::P1d3TruthCommitted(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d3_truth_with_redis(*owner, transport))
            }
            Stage7bRestartOutcome::P1d3CancelContinuationPending(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d3_cancel_continuation_with_redis(
                    *owner, transport
                ))
            }
            Stage7bRestartOutcome::P1d3SemanticPending(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d3_semantic_with_redis(*owner, transport))
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketPrepublicationPending(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d4_prepublication_with_redis(
                    *owner, transport
                ))
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketDispatchPending(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d4_dispatch_pending_with_redis(
                    *owner, transport
                ))
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketOrderPending(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d4_order_pending_with_redis(
                    *owner, transport
                ))
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketPreFinalizationPending(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d4_pre_finalization_with_redis(
                    *owner, transport
                ))
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketPreAckPending(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d4_pre_ack_with_redis(*owner, transport))
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketAckCommitted(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d4_ack_with_redis(*owner, transport))
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketTruthCommitted(owner) => {
                acquire_on_heap!(acquire_stage8b_p1d4_truth_with_redis(*owner, transport))
            }
            Stage7bRestartOutcome::P1eScheduleBindingCommitted(_)
            | Stage7bRestartOutcome::Stage8a4I3Pending(_)
            | Stage7bRestartOutcome::Blocked(_) => {
                panic!("P1-e I0 fixture cannot retain a non-P1 source route")
            }
        };
        let route_id = acquired.route.route_id();
        p1e_i0_set_latch_and_retain(acquired, route_id);
        assert_eq!(
            p1e_i0_take_effect_audit(),
            P1eI0ObservedEffectCountersV1::default(),
            "{route_id}: preset latch must retain with zero effects"
        );
        Some(route_id)
    }

    async fn p1d4_finish_published(
        published: Stage8bP1RedisCommandPublished,
        scenario_id: &str,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> P1d4ContinuationEvidence {
        match scenario_id {
            "S01" | "S02" => {
                let ack = Box::pin(
                    published.execute_next_canonical_limit(p1d4_initial_schedule(scenario_id), key),
                )
                .await
                .unwrap();
                Box::pin(p1d4_finish_limit_truth(ack.commit_truth(key).unwrap())).await
            }
            "S03" => {
                let ack = published
                    .execute_initial_limit_expiry(p1d4_initial_expiry_authority(), key)
                    .unwrap();
                Box::pin(p1d4_finish_limit_truth(ack.commit_truth(key).unwrap())).await
            }
            "S08" | "S09" | "S10" | "S11" => {
                let outcome = Box::pin(
                    published
                        .execute_next_canonical_cancel(p1d4_initial_schedule(scenario_id), key),
                )
                .await
                .unwrap();
                Box::pin(p1d4_finish_cancel_outcome(outcome, key)).await
            }
            _ => panic!("{scenario_id} has no initial command continuation"),
        }
    }

    async fn p1d4_finish_restart(
        restart: Stage7bRestartOutcome,
        redis: &RedisServer,
        scenario_id: &str,
        frontier_id: &str,
        expected_restart_disposition: &str,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> P1d4ContinuationEvidence {
        tokio::time::sleep(Duration::from_millis(5)).await;
        match restart {
            Stage7bRestartOutcome::Ready(owner) => {
                if scenario_id == "S07" {
                    if frontier_id == "F19" {
                        return P1d4ContinuationEvidence {
                            final_disposition: "Ready".into(),
                            xack_reply: "not_applicable".into(),
                            xack_disposition: "NoSource".into(),
                            sequence_after: None,
                            provider_attempts: 0,
                            schedule_issue_attempts: 0,
                            s_ack_commits: 0,
                            s_truth_commits: 0,
                            s_ack_generations: Vec::new(),
                            s_truth_generations: Vec::new(),
                            immediate_xack_attempts: 0,
                        };
                    }
                    assert_eq!(
                        frontier_id, "F17",
                        "only the pre-WAL day-expiry frontier may reissue authority"
                    );
                    let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                        .await
                        .unwrap();
                    let owner = Stage8bP1RedisSemanticCompositionOwner::new(*owner, transport);
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
                    drop(owner.expire_working_limit(authority, key).unwrap());
                    return P1d4ContinuationEvidence {
                        final_disposition: "Ready".into(),
                        xack_reply: "not_applicable".into(),
                        xack_disposition: "NoSource".into(),
                        sequence_after: None,
                        provider_attempts: 0,
                        schedule_issue_attempts: 0,
                        s_ack_commits: 0,
                        s_truth_commits: 0,
                        s_ack_generations: Vec::new(),
                        s_truth_generations: Vec::new(),
                        immediate_xack_attempts: 0,
                    };
                }
                if matches!(scenario_id, "S04" | "S05" | "S06") {
                    let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                        .await
                        .unwrap();
                    let owner = Stage8bP1RedisSemanticCompositionOwner::new(*owner, transport);
                    let (source_close_ms, candidate_close_ms) = if scenario_id == "S06" {
                        (
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS + 1_200_000,
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS + 1_800_000,
                        )
                    } else {
                        (
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS,
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000,
                        )
                    };
                    let outcome = owner
                        .process_next_working_limit(
                            p1d3_cancel_schedule(source_close_ms, candidate_close_ms),
                            key,
                        )
                        .await
                        .unwrap();
                    return p1d4_finish_semantic_outcome(outcome, key).await;
                }
                P1d4ContinuationEvidence {
                    final_disposition: "Ready".into(),
                    xack_reply: "already_completed_before_restart".into(),
                    xack_disposition: "AlreadyAcknowledged".into(),
                    sequence_after: None,
                    provider_attempts: 0,
                    schedule_issue_attempts: 0,
                    s_ack_commits: 0,
                    s_truth_commits: 0,
                    s_ack_generations: Vec::new(),
                    s_truth_generations: Vec::new(),
                    immediate_xack_attempts: 0,
                }
            }
            Stage7bRestartOutcome::P1SemanticPrepublicationReady(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                let pending = p1e_test_resume_prepublication(*owner, transport)
                    .await
                    .unwrap();
                Box::pin(p1d4_finish_published(
                    pending.publish_exact_command().await.unwrap(),
                    scenario_id,
                    key,
                ))
                .await
            }
            Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                let resolved = p1e_test_resolve_zero_intent(*owner, transport)
                    .await
                    .unwrap();
                if expected_restart_disposition == "Ready"
                    && scenario_id == "S07"
                    && frontier_id == "F17"
                {
                    assert_eq!(
                        resolved.disposition(),
                        Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged,
                        "the stale replacement source must already be acknowledged before day-expiry reissue"
                    );
                    let owner = *resolved.into_ready_owner();
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
                    drop(owner.expire_working_limit(authority, key).unwrap());
                    return P1d4ContinuationEvidence {
                        final_disposition: "Ready".into(),
                        xack_reply: "not_applicable".into(),
                        xack_disposition: "NoSource".into(),
                        sequence_after: None,
                        provider_attempts: 0,
                        schedule_issue_attempts: 0,
                        s_ack_commits: 0,
                        s_truth_commits: 0,
                        s_ack_generations: Vec::new(),
                        s_truth_generations: Vec::new(),
                        immediate_xack_attempts: 0,
                    };
                }
                if expected_restart_disposition == "Ready"
                    && matches!(scenario_id, "S04" | "S05" | "S06")
                {
                    assert_eq!(
                        resolved.disposition(),
                        Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged,
                        "a Ready registry frontier may cross only an already-acknowledged stale replacement"
                    );
                    let owner = *resolved.into_ready_owner();
                    let (source_close_ms, candidate_close_ms) = if scenario_id == "S06" {
                        (
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS + 1_200_000,
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS + 1_800_000,
                        )
                    } else {
                        (
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS,
                            P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000,
                        )
                    };
                    let outcome = owner
                        .process_next_working_limit(
                            p1d3_cancel_schedule(source_close_ms, candidate_close_ms),
                            key,
                        )
                        .await
                        .unwrap();
                    return p1d4_finish_semantic_outcome(outcome, key).await;
                }
                p1d4_xack_evidence(resolved.disposition(), "Ready")
            }
            Stage7bRestartOutcome::P1d3DispatchPending(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                match scenario_id {
                    "S01" | "S02" => {
                        let ack = p1e_test_resume_p1d3_dispatch_limit(
                            *owner,
                            transport,
                            p1d4_initial_schedule(scenario_id),
                            key,
                        )
                        .await
                        .unwrap();
                        p1d4_finish_limit_truth(ack.commit_truth(key).unwrap()).await
                    }
                    "S03" => {
                        let ack = p1e_test_resume_p1d3_dispatch_expiry(
                            *owner,
                            transport,
                            p1d4_initial_expiry_authority(),
                            key,
                        )
                        .await
                        .unwrap();
                        p1d4_finish_limit_truth(ack.commit_truth(key).unwrap()).await
                    }
                    "S08" | "S09" | "S10" | "S11" => {
                        let outcome = p1e_test_resume_p1d3_dispatch_cancel(
                            *owner,
                            transport,
                            p1d4_initial_schedule(scenario_id),
                            key,
                        )
                        .await
                        .unwrap();
                        p1d4_finish_cancel_outcome(outcome, key).await
                    }
                    _ => panic!("{scenario_id} has no dispatch-only continuation"),
                }
            }
            Stage7bRestartOutcome::P1d3PreAckPending(owner) => {
                if scenario_id == "S07" {
                    match owner.commit_reconstructed_transition(key).unwrap() {
                        Stage8bP1d3RecoveredCommitOutcome::Ready(ready) => drop(ready),
                        Stage8bP1d3RecoveredCommitOutcome::AckCommitted(ack) => {
                            drop(ack.commit_truth(key).unwrap());
                        }
                        Stage8bP1d3RecoveredCommitOutcome::TruthCommitted(truth) => drop(truth),
                        Stage8bP1d3RecoveredCommitOutcome::CancelContinuationPending(_)
                        | Stage8bP1d3RecoveredCommitOutcome::SemanticCallbackPending(_) => {
                            panic!("S07 expiry WAL reconstructed a non-expiry continuation")
                        }
                    }
                    return P1d4ContinuationEvidence {
                        final_disposition: "Ready".into(),
                        xack_reply: "not_applicable".into(),
                        xack_disposition: "NoSource".into(),
                        sequence_after: None,
                        provider_attempts: 0,
                        schedule_issue_attempts: 0,
                        s_ack_commits: 0,
                        s_truth_commits: 0,
                        s_ack_generations: Vec::new(),
                        s_truth_generations: Vec::new(),
                        immediate_xack_attempts: 0,
                    };
                }
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                match p1e_test_resume_p1d3_pre_ack(*owner, transport, key)
                    .await
                    .unwrap()
                {
                    Stage8bP1RedisPreAckRecoveryOutcome::AckCommitted(ack) => {
                        p1d4_finish_limit_truth(ack.commit_truth(key).unwrap()).await
                    }
                    Stage8bP1RedisPreAckRecoveryOutcome::TruthCommitted(truth) => {
                        p1d4_finish_limit_truth(truth).await
                    }
                    Stage8bP1RedisPreAckRecoveryOutcome::Semantic(outcome) => {
                        p1d4_finish_semantic_outcome(outcome, key).await
                    }
                }
            }
            Stage7bRestartOutcome::P1d3AckCommitted(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                let ack = p1e_test_resume_p1d3_ack(*owner, transport).await.unwrap();
                p1d4_finish_limit_truth(ack.commit_truth(key).unwrap()).await
            }
            Stage7bRestartOutcome::P1d3TruthCommitted(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                let resolved = p1e_test_resume_p1d3_truth(*owner, transport).await.unwrap();
                p1d4_xack_evidence(resolved.disposition(), "P1d3TruthCommitted")
            }
            Stage7bRestartOutcome::P1d3CancelContinuationPending(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                let truth = p1e_test_resume_p1d3_cancel_continuation(*owner, transport, key)
                    .await
                    .unwrap();
                p1d4_finish_limit_truth(truth).await
            }
            Stage7bRestartOutcome::P1d3SemanticPending(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                let outcome = p1e_test_resume_p1d3_semantic(*owner, transport, key)
                    .await
                    .unwrap();
                p1d4_finish_semantic_outcome(outcome, key).await
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketPrepublicationPending(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                let decision_close_ms = P1D3_CANCEL_CANDIDATE_CLOSE_MS + 600_000;
                let published = p1e_test_resume_p1d4_prepublication(*owner, transport)
                    .await
                    .unwrap();
                let ack = published
                    .execute_next_canonical_generated_market(
                        strategy_runtime_core::stage8b_p1d1_test_schedule_authority(
                            super::super::p1_instrument(),
                            decision_close_ms,
                            decision_close_ms + 600_000,
                        ),
                        key,
                    )
                    .await
                    .unwrap();
                assert!(matches!(frontier_id, "GM00" | "GM01" | "GM02" | "F15"));
                p1d4_finish_generated_ack(ack, key).await
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketDispatchPending(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                p1d4_finish_generated_ack(
                    p1e_test_resume_p1d4_dispatch(*owner, transport, key)
                        .await
                        .unwrap(),
                    key,
                )
                .await
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketOrderPending(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                p1d4_finish_generated_ack(
                    p1e_test_resume_p1d4_order(*owner, transport, key)
                        .await
                        .unwrap(),
                    key,
                )
                .await
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketPreFinalizationPending(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                p1d4_finish_generated_ack(
                    p1e_test_resume_p1d4_pre_finalization(*owner, transport, key)
                        .await
                        .unwrap(),
                    key,
                )
                .await
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketPreAckPending(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                p1d4_finish_generated_ack(
                    p1e_test_resume_p1d4_pre_ack(*owner, transport, key)
                        .await
                        .unwrap(),
                    key,
                )
                .await
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketAckCommitted(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                p1d4_finish_generated_ack(
                    p1e_test_resume_p1d4_ack(*owner, transport).await.unwrap(),
                    key,
                )
                .await
            }
            Stage7bRestartOutcome::P1d4GeneratedMarketTruthCommitted(owner) => {
                let transport = attach_stage8b_p1_redis(&redis.url, reclaim_config())
                    .await
                    .unwrap();
                let resolved = p1e_test_resume_p1d4_truth(*owner, transport).await.unwrap();
                let audit = resolved.audit_evidence();
                let sequence_after = Some((audit.core.seq_ack, audit.core.seq_truth));
                let mut evidence =
                    p1d4_xack_evidence(resolved.disposition(), "P1d4GeneratedMarketTruthCommitted");
                evidence.sequence_after = sequence_after;
                evidence.with_immediate_xack(1)
            }
            other => panic!(
                "P1-d4 final continuation cannot start at {}",
                p1d4_restart_disposition(&other)
            ),
        }
    }

    fn assert_p1d4_marker_and_sigkill(
        child: &mut Child,
        marker: &Path,
        parent: &Path,
        cell: &impl P1d4RegistryIdentity,
        started_at: Instant,
    ) -> P1d4CrashProcessEvidence {
        wait_for_p1d3_crash_barrier(child, marker);
        assert!(child.try_wait().unwrap().is_none());
        let marker_bytes = fs::read(marker).unwrap();
        let marker_json: serde_json::Value = serde_json::from_slice(&marker_bytes).unwrap();
        let marker_object = marker_json.as_object().unwrap();
        assert_eq!(marker_object.len(), 8);
        assert_eq!(marker_json["schema_version"], 1);
        assert_eq!(marker_json["domain"], "moex.stage8b.p1d4.crash-marker.v1");
        assert_eq!(marker_json["child_pid"], u64::from(child.id()));
        assert_eq!(marker_json["cell_id"], cell.cell_id());
        assert_eq!(marker_json["scenario_id"], cell.scenario_id());
        assert_eq!(marker_json["frontier_id"], cell.frontier_id());
        assert_eq!(marker_json["kill_hook_name"], cell.kill_hook_name());
        let pre_kill_audit_sha256 = marker_json["pre_kill_audit_sha256"].as_str().unwrap();
        assert_eq!(
            pre_kill_audit_sha256,
            crate::recovery::stage8b_p1d4_pre_kill_audit_sha256(
                parent,
                marker,
                cell.cell_id(),
                cell.scenario_id(),
                cell.frontier_id(),
                cell.kill_hook_name(),
            )
        );
        let expected_canonical = format!(
            "{{\"cell_id\":\"{}\",\"child_pid\":{},\"domain\":\"moex.stage8b.p1d4.crash-marker.v1\",\"frontier_id\":\"{}\",\"kill_hook_name\":\"{}\",\"pre_kill_audit_sha256\":\"{}\",\"scenario_id\":\"{}\",\"schema_version\":1}}",
            cell.cell_id(),
            child.id(),
            cell.frontier_id(),
            cell.kill_hook_name(),
            pre_kill_audit_sha256,
            cell.scenario_id(),
        );
        assert_eq!(marker_bytes, expected_canonical.as_bytes());

        let normalized = format!(
            "{{\"cell_id\":\"{}\",\"child_pid\":0,\"domain\":\"moex.stage8b.p1d4.crash-marker.v1\",\"frontier_id\":\"{}\",\"kill_hook_name\":\"{}\",\"pre_kill_audit_sha256\":\"{}\",\"scenario_id\":\"{}\",\"schema_version\":1}}",
            cell.cell_id(),
            cell.frontier_id(),
            cell.kill_hook_name(),
            pre_kill_audit_sha256,
            cell.scenario_id(),
        );
        let mut normalized_hasher = Sha256::new();
        normalized_hasher.update(b"moex.stage8b.p1d4.crash-marker.normalized.v1\0");
        normalized_hasher.update((normalized.len() as u64).to_be_bytes());
        normalized_hasher.update(normalized.as_bytes());
        let normalized_marker_sha256 = format!("{:x}", normalized_hasher.finalize());
        assert_eq!(normalized_marker_sha256.len(), 64);
        let raw_marker_sha256 = sha256_hex(&marker_bytes);

        let witness_path = marker.with_extension("xack-witness");
        let (pre_kill_xack_reply, witness_v1, witness_sha256, witness_order) = if cell.frontier_id()
            == "F16"
        {
            let witness_bytes = fs::read(&witness_path)
                .expect("P1-d4 F16 must durably retain its XACK reply witness");
            let witness: serde_json::Value = serde_json::from_slice(&witness_bytes).unwrap();
            assert_eq!(witness.as_object().unwrap().len(), 12);
            let oracle = p1d4_base_operational_evidence_oracle_cell(cell.cell_id());
            assert_eq!(witness["schema_version"], 1);
            assert_eq!(
                witness["domain"],
                "moex.stage8b.p1d4.pre-kill-xack-reply-witness.v1"
            );
            assert_eq!(witness["cell_id"], cell.cell_id());
            assert_eq!(witness["child_pid"], u64::from(child.id()));
            assert_eq!(witness["scenario_id"], cell.scenario_id());
            assert_eq!(witness["frontier_id"], cell.frontier_id());
            assert_eq!(witness["kill_hook_name"], cell.kill_hook_name());
            assert_eq!(witness["pre_kill_audit_sha256"], pre_kill_audit_sha256);
            assert_eq!(witness["source_stream"], oracle.source_stream);
            assert_eq!(witness["source_group"], oracle.source_group);
            assert_eq!(witness["source_m10_redis_id"], oracle.source_m10_redis_id);
            assert_eq!(witness["xack_reply"], "integer:1");
            let expected_witness = format!(
                    "{{\"cell_id\":\"{}\",\"child_pid\":{},\"domain\":\"moex.stage8b.p1d4.pre-kill-xack-reply-witness.v1\",\"frontier_id\":\"{}\",\"kill_hook_name\":\"{}\",\"pre_kill_audit_sha256\":\"{}\",\"scenario_id\":\"{}\",\"schema_version\":1,\"source_group\":\"{}\",\"source_m10_redis_id\":\"{}\",\"source_stream\":\"{}\",\"xack_reply\":\"integer:1\"}}",
                    cell.cell_id(),
                    child.id(),
                    cell.frontier_id(),
                    cell.kill_hook_name(),
                    pre_kill_audit_sha256,
                    cell.scenario_id(),
                    oracle.source_group,
                    oracle.source_m10_redis_id,
                    oracle.source_stream,
                );
            assert_eq!(witness_bytes, expected_witness.as_bytes());
            let mut witness_hasher = Sha256::new();
            witness_hasher.update(b"moex.stage8b.p1d4.pre-kill-xack-reply-witness.digest.v1\0");
            witness_hasher.update((witness_bytes.len() as u64).to_be_bytes());
            witness_hasher.update(&witness_bytes);
            (
                "integer:1".to_string(),
                Some(witness),
                Some(format!("{:x}", witness_hasher.finalize())),
                "after_integer_1_before_crash_marker".to_string(),
            )
        } else {
            assert!(
                !witness_path.exists(),
                "P1-d4 non-F16 cell cannot retain an XACK reply witness"
            );
            (
                "not_observed".to_string(),
                None,
                None,
                "not_applicable".to_string(),
            )
        };
        child.kill().unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.code(), None);
        assert_eq!(status.signal(), Some(libc::SIGKILL));
        P1d4CrashProcessEvidence {
            child_pid: child.id(),
            exit_code: status.code(),
            exit_signal: status.signal().unwrap(),
            reaped: child.try_wait().unwrap().is_some(),
            wall_duration_ms: started_at.elapsed().as_millis().try_into().unwrap(),
            raw_marker_sha256,
            normalized_marker_sha256,
            pre_kill_filesystem_sha256: pre_kill_audit_sha256.to_string(),
            pre_kill_xack_reply,
            pre_kill_xack_reply_witness_v1: witness_v1,
            pre_kill_xack_reply_witness_sha256: witness_sha256,
            pre_kill_xack_witness_order: witness_order,
            sequence_pair_before_kill: None,
        }
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
    ) -> P1d4CrashProcessEvidence {
        let cell = p1d4_registry_cell(scenario_id, frontier_id);
        let operational_oracle = p1d4_base_operational_evidence_oracle_cell(cell.cell_id);
        assert!(!cell.expected_restart_disposition.is_empty());
        let marker = parent.join(format!("{}-{}.marker", cell.cell_id, cell.kill_hook_name));
        let started_at = Instant::now();
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
            .env(
                "STAGE8B_P1D4_SOURCE_STREAM",
                operational_oracle.source_stream,
            )
            .env("STAGE8B_P1D4_SOURCE_GROUP", operational_oracle.source_group)
            .env(
                "STAGE8B_P1D4_SOURCE_M10_REDIS_ID",
                operational_oracle.source_m10_redis_id,
            )
            .env("RUST_MIN_STACK", "16777216")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        assert_p1d4_marker_and_sigkill(&mut child, &marker, parent, &cell, started_at)
    }

    async fn spawn_p1d4_generated_market_frontier(
        redis: &RedisServer,
        parent: &Path,
        cell: &P1d4GeneratedMarketRegistryCell<'_>,
    ) -> P1d4CrashProcessEvidence {
        let marker = parent.join(format!("{}-{}.marker", cell.cell_id, cell.kill_hook_name));
        let pair_marker = parent.join(format!("{}-sequence-pair.marker", cell.cell_id));
        let started_at = Instant::now();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("stage8b_p1_semantic::redis::tests::p1d4_dispatch_frontier_child")
            .arg("--nocapture")
            .env("STAGE8B_P1_TEST_PARENT", parent)
            .env("STAGE8B_P1_TEST_CRASH_PHASE", cell.kill_hook_name)
            .env("STAGE8B_P1_TEST_CRASH_MARKER", &marker)
            .env("STAGE8B_P1_TEST_SEQUENCE_PAIR_MARKER", &pair_marker)
            .env("STAGE8B_P1_TEST_REDIS_URL", &redis.url)
            .env("STAGE8B_P1D4_SCENARIO", "generated-market")
            .env("STAGE8B_P1D4_CELL_ID", cell.cell_id)
            .env("STAGE8B_P1D4_SCENARIO_ID", cell.parent_scenario_id)
            .env("STAGE8B_P1D4_FRONTIER_ID", cell.frontier_id)
            .env("RUST_MIN_STACK", "16777216")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut evidence =
            assert_p1d4_marker_and_sigkill(&mut child, &marker, parent, cell, started_at);
        if matches!(cell.frontier_id, "GM08" | "GM09") {
            let pair = fs::read_to_string(&pair_marker)
                .expect("GM08/GM09 must fsync the allocated sequence pair before SIGKILL");
            let mut lines = pair.lines();
            let seq_ack = lines
                .next()
                .and_then(|line| line.strip_prefix("seq_ack="))
                .and_then(|value| value.parse::<u64>().ok())
                .expect("sequence marker must contain canonical seq_ack");
            let seq_truth = lines
                .next()
                .and_then(|line| line.strip_prefix("seq_truth="))
                .and_then(|value| value.parse::<u64>().ok())
                .expect("sequence marker must contain canonical seq_truth");
            assert!(
                lines.next().is_none(),
                "sequence marker must have two lines"
            );
            evidence.sequence_pair_before_kill = Some((seq_ack, seq_truth));
        } else {
            assert!(
                !pair_marker.exists(),
                "{} must not allocate a sequence pair before its frontier",
                cell.frontier_id
            );
        }
        evidence
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct P1d4RedisAudit {
        pel: usize,
        source_stream: String,
        source_group: String,
        last_delivered_id: String,
        command_publications: usize,
    }

    async fn p1d4_redis_audit(redis: &RedisServer) -> P1d4RedisAudit {
        let namespace = stage8b_p1_redis_namespace();
        let mut connection = redis.connection().await;
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        let groups: StreamInfoGroupsReply = redis::cmd("XINFO")
            .arg("GROUPS")
            .arg(&namespace.canonical_m10_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        let group = groups
            .groups
            .into_iter()
            .find(|group| group.name == namespace.m10_consumer_group)
            .expect("P1-d4 canonical M10 group must exist");
        assert_eq!(group.pending, pending.count());
        let command_publications: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        P1d4RedisAudit {
            pel: pending.count(),
            source_stream: namespace.canonical_m10_stream,
            source_group: namespace.m10_consumer_group,
            last_delivered_id: group.last_delivered_id,
            command_publications,
        }
    }

    fn p1d4_redis_group_frontier_v1(
        before: &P1d4RedisAudit,
        post_restart: &P1d4RedisAudit,
        final_audit: &P1d4RedisAudit,
    ) -> serde_json::Value {
        assert_eq!(before.source_stream, post_restart.source_stream);
        assert_eq!(before.source_stream, final_audit.source_stream);
        assert_eq!(before.source_group, post_restart.source_group);
        assert_eq!(before.source_group, final_audit.source_group);
        assert_eq!(before.last_delivered_id, post_restart.last_delivered_id);
        assert_eq!(before.last_delivered_id, final_audit.last_delivered_id);
        p1d4_canonicalize_json(serde_json::json!({
            "schema_version": 1,
            "domain": "moex.stage8b.p1d4.redis-source-frontier.v1",
            "source_stream": before.source_stream.as_str(),
            "source_group": before.source_group.as_str(),
            "source_m10_redis_id": before.last_delivered_id.as_str(),
            "before": {
                "last_delivered_id": before.last_delivered_id.as_str(),
                "pending": before.pel,
            },
            "post_restart": {
                "last_delivered_id": post_restart.last_delivered_id.as_str(),
                "pending": post_restart.pel,
            },
            "final": {
                "last_delivered_id": final_audit.last_delivered_id.as_str(),
                "pending": final_audit.pel,
            },
        }))
    }

    fn p1d4_expected_pel(value: &str) -> usize {
        match value {
            "exact_source_pending_1" => 1,
            "exact_source_absent_after_parsed_xack_1"
            | "exact_source_absent"
            | "no_new_source"
            | "unchanged_no_new_source" => 0,
            other => panic!("unrecognized P1-d4 PEL expectation: {other}"),
        }
    }

    fn p1d4_sequence_label(
        audit: &crate::recovery::Stage8bP1d4RestartAuditV1,
        exact_pair: Option<(u64, u64)>,
        generated_market: bool,
    ) -> String {
        if let Some((seq_ack, seq_truth)) = exact_pair.or(audit.sequence_pair) {
            return format!("seq_ack={seq_ack};seq_truth={seq_truth}");
        }
        if !generated_market {
            if let Some(allocation) = audit.sequence_allocations.last() {
                return match (allocation.seq_ack, allocation.seq_truth) {
                    (Some(seq_ack), Some(seq_truth)) => {
                        format!("seq_ack={seq_ack};seq_truth={seq_truth}")
                    }
                    (Some(seq_ack), None) => format!("seq_ack={seq_ack}"),
                    (None, Some(seq_truth)) => format!("seq_truth={seq_truth}"),
                    (None, None) => panic!("P1-d4 authenticated allocation contains no sequence"),
                };
            }
        }
        format!("lifecycle_sequence={}", audit.lifecycle_sequence)
    }

    fn p1d4_sequence_audit_value(
        audit: &crate::recovery::Stage8bP1d4RestartAuditV1,
        pre_kill_pair: Option<(u64, u64)>,
    ) -> serde_json::Value {
        serde_json::json!({
            "lifecycle_sequence": audit.lifecycle_sequence,
            "journal_lifecycle_sequences": audit.journal_lifecycle_sequences,
            "durable_sequence_pair": audit.sequence_pair,
            "pre_kill_sequence_pair": pre_kill_pair,
            "allocations": audit.sequence_allocations,
        })
    }

    fn p1d4_assert_exact_sequence_allocations(
        cell_id: &str,
        audit: &crate::recovery::Stage8bP1d4RestartAuditV1,
    ) {
        assert_eq!(
            audit.journal_lifecycle_sequences.first(),
            Some(&1),
            "{cell_id}: journal sequence must start at one"
        );
        assert!(
            audit
                .journal_lifecycle_sequences
                .windows(2)
                .all(|pair| pair[1] == 1 || pair[1] == pair[0] + 1),
            "{cell_id}: each authenticated journal segment must be gap-free"
        );
        assert_eq!(
            audit
                .journal_lifecycle_sequences
                .last()
                .copied()
                .unwrap_or(0),
            audit.lifecycle_sequence,
            "{cell_id}: current journal frontier must equal the final authenticated record"
        );
        let mut prior_record_index = None;
        let mut prior_business_frontier = None;
        for (index, allocation) in audit.sequence_allocations.iter().enumerate() {
            assert!(
                prior_record_index.is_none_or(|prior| allocation.journal_record_index > prior),
                "{cell_id}: V3 journal record indexes must be strictly increasing"
            );
            assert_eq!(
                audit
                    .journal_lifecycle_sequences
                    .get(allocation.journal_record_index)
                    .copied(),
                Some(allocation.stage6_lifecycle_sequence),
                "{cell_id}: V3 lifecycle sequence is not bound to its authenticated journal record"
            );
            prior_record_index = Some(allocation.journal_record_index);
            if index == 0 {
                assert_eq!(
                    allocation.sequence_allocation_frontier, 2,
                    "{cell_id}: deterministic fixture business-sequence anchor drifted"
                );
            } else {
                assert_eq!(
                    Some(allocation.sequence_allocation_frontier),
                    prior_business_frontier,
                    "{cell_id}: business sequence allocation is not contiguous"
                );
            }
            let terminal = match allocation.outcome_kind.as_str() {
                "initial_working" | "initial_filled" | "initial_expired" | "cancel_canceled" => {
                    let seq_ack = allocation.seq_ack.expect("request-scoped ACK sequence");
                    let seq_truth = allocation.seq_truth.expect("request-scoped truth sequence");
                    assert_eq!(
                        seq_ack,
                        allocation.sequence_allocation_frontier + 1,
                        "{cell_id}"
                    );
                    assert_eq!(seq_truth, seq_ack + 1, "{cell_id}");
                    seq_truth
                }
                "later_filled" | "later_expired" => {
                    assert!(allocation.seq_ack.is_none(), "{cell_id}");
                    let seq_truth = allocation.seq_truth.expect("autonomous truth sequence");
                    assert_eq!(
                        seq_truth,
                        allocation.sequence_allocation_frontier + 1,
                        "{cell_id}"
                    );
                    seq_truth
                }
                "cancel_execution_observed" | "cancel_already_terminal_non_execution" => {
                    let seq_ack = allocation.seq_ack.expect("recovered cancel ACK sequence");
                    assert!(allocation.seq_truth.is_none(), "{cell_id}");
                    assert_eq!(
                        seq_ack,
                        allocation.sequence_allocation_frontier + 1,
                        "{cell_id}"
                    );
                    seq_ack
                }
                other => panic!("{cell_id}: unknown authenticated outcome kind {other}"),
            };
            prior_business_frontier = Some(terminal);
        }
        assert_eq!(
            audit.durable_outcomes,
            audit.sequence_allocations.len(),
            "{cell_id}"
        );
        assert_eq!(
            audit.truth_bearing_outcomes,
            audit
                .sequence_allocations
                .iter()
                .filter(|allocation| allocation.seq_truth.is_some())
                .count(),
            "{cell_id}: truth count must be independently derived"
        );
    }

    fn p1d4_canonicalize_json(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(p1d4_canonicalize_json).collect())
            }
            serde_json::Value::Object(values) => {
                let ordered = values
                    .into_iter()
                    .map(|(key, value)| (key, p1d4_canonicalize_json(value)))
                    .collect::<BTreeMap<_, _>>();
                serde_json::Value::Object(ordered.into_iter().collect())
            }
            primitive => primitive,
        }
    }

    fn p1d4_expected_outcome_kinds(scenario_id: &str) -> &'static [&'static str] {
        match scenario_id {
            "S01" => &["initial_working"],
            "S02" => &["initial_filled"],
            "S03" => &["initial_expired"],
            "S04" | "S05" => &["initial_working"],
            "S06" => &["initial_working", "later_filled"],
            "S07" => &["initial_working", "later_expired"],
            "S08" => &["initial_working", "cancel_canceled"],
            "S09" => &[
                "initial_working",
                "later_filled",
                "cancel_execution_observed",
            ],
            "S10" => &["initial_filled", "cancel_execution_observed"],
            "S11" => &["initial_expired", "cancel_already_terminal_non_execution"],
            other => panic!("unknown P1-d4 scenario {other}"),
        }
    }

    fn p1d4_assert_exact_scenario_allocations(
        cell_id: &str,
        scenario_id: &str,
        before: &crate::recovery::Stage8bP1d4RestartAuditV1,
        after: &crate::recovery::Stage8bP1d4RestartAuditV1,
    ) {
        assert!(
            after
                .sequence_allocations
                .starts_with(&before.sequence_allocations),
            "{cell_id}: pre-restart allocation vector is not an exact final prefix"
        );
        assert_eq!(
            after
                .sequence_allocations
                .iter()
                .map(|allocation| allocation.outcome_kind.as_str())
                .collect::<Vec<_>>(),
            p1d4_expected_outcome_kinds(scenario_id),
            "{cell_id}: final authenticated outcome vector drifted"
        );
    }

    fn p1d4_oracle_values(value: &str) -> Vec<&str> {
        if value == "none" {
            Vec::new()
        } else {
            value.split('|').collect()
        }
    }

    fn p1d4_phase_label(value: Option<&str>) -> &str {
        value.unwrap_or("none")
    }

    fn p1d4_assert_package_commit_history(
        cell_id: &str,
        before: &crate::recovery::Stage8bP1d4RestartAuditV1,
        after: &crate::recovery::Stage8bP1d4RestartAuditV1,
        history: &[crate::recovery::Stage8bP1d4ObservedPackageCommitV1],
    ) -> usize {
        let before_generation = before
            .package
            .write_generation
            .expect("P1-d4 package before generation");
        let after_generation = after
            .package
            .write_generation
            .expect("P1-d4 package after generation");
        let advance = after_generation
            .checked_sub(before_generation)
            .expect("P1-d4 package generation cannot move backwards");
        assert_eq!(
            history.len() as u64,
            advance,
            "{cell_id}: every replacement generation must be observed"
        );
        let mut prior_p1d3_phase = before.package.p1d3_phase.as_deref();
        let mut prior_generated_phase = before.package.generated_market_phase.as_deref();
        let mut prior_covering_seal_generation = None;
        let mut truth_replacement_commits = 0usize;
        for (index, commit) in history.iter().enumerate() {
            assert_eq!(
                commit.write_generation,
                before_generation + index as u64 + 1,
                "{cell_id}: package commit history generation gap"
            );
            assert!(
                prior_covering_seal_generation
                    .is_none_or(|prior| commit.covering_seal_generation == prior + 1),
                "{cell_id}: covering-seal history generation gap"
            );
            prior_covering_seal_generation = Some(commit.covering_seal_generation);
            let p1d3_truth_transition =
                matches!(commit.p1d3_phase.as_deref(), Some("Working" | "Terminal"))
                    && commit.p1d3_phase.as_deref() != prior_p1d3_phase;
            let generated_truth_transition = commit.generated_market_phase.as_deref()
                == Some("TruthCommitted")
                && prior_generated_phase != Some("TruthCommitted");
            truth_replacement_commits +=
                usize::from(p1d3_truth_transition || generated_truth_transition);
            prior_p1d3_phase = commit.p1d3_phase.as_deref();
            prior_generated_phase = commit.generated_market_phase.as_deref();
        }
        if let Some(last) = history.last() {
            assert_eq!(last.write_generation, after_generation, "{cell_id}");
            assert_eq!(last.p1d3_phase, after.package.p1d3_phase, "{cell_id}");
            assert_eq!(
                last.generated_market_phase, after.package.generated_market_phase,
                "{cell_id}"
            );
        } else {
            assert_eq!(before.package, after.package, "{cell_id}");
        }
        truth_replacement_commits
    }

    fn p1d4_assert_base_evidence_oracle(
        cell_id: &str,
        before: &crate::recovery::Stage8bP1d4RestartAuditV1,
        after: &crate::recovery::Stage8bP1d4RestartAuditV1,
        observed_effect_events: &[&str],
        package_commit_history: &[crate::recovery::Stage8bP1d4ObservedPackageCommitV1],
        truth_replacement_commits: usize,
    ) {
        let oracle = p1d4_base_evidence_oracle_cell(cell_id);
        assert_eq!(
            before
                .sequence_allocations
                .iter()
                .map(|allocation| allocation.outcome_kind.as_str())
                .collect::<Vec<_>>(),
            p1d4_oracle_values(oracle.pre_kill_allocation_kinds),
            "{cell_id}: exact pre-kill allocation prefix"
        );
        assert_eq!(
            after
                .sequence_allocations
                .iter()
                .map(|allocation| allocation.outcome_kind.as_str())
                .collect::<Vec<_>>(),
            p1d4_oracle_values(oracle.final_allocation_kinds),
            "{cell_id}: exact final allocation vector"
        );
        assert_eq!(
            observed_effect_events,
            p1d4_oracle_values(oracle.ordered_effect_events),
            "{cell_id}: exact ordered base effect vector"
        );
        assert_eq!(
            p1d4_phase_label(before.package.p1d3_phase.as_deref()),
            oracle.package_before_p1d3_phase,
            "{cell_id}"
        );
        assert_eq!(
            p1d4_phase_label(before.package.generated_market_phase.as_deref()),
            oracle.package_before_generated_market_phase,
            "{cell_id}"
        );
        assert_eq!(
            before.package.write_generation,
            Some(oracle.package_before_write_generation),
            "{cell_id}"
        );
        assert_eq!(
            p1d4_phase_label(after.package.p1d3_phase.as_deref()),
            oracle.package_after_p1d3_phase,
            "{cell_id}"
        );
        assert_eq!(
            p1d4_phase_label(after.package.generated_market_phase.as_deref()),
            oracle.package_after_generated_market_phase,
            "{cell_id}"
        );
        assert_eq!(
            after.package.write_generation,
            Some(oracle.package_after_write_generation),
            "{cell_id}"
        );
        assert_eq!(
            oracle.package_after_write_generation - oracle.package_before_write_generation,
            oracle.write_generation_advance,
            "{cell_id}: frozen oracle generation arithmetic"
        );
        assert_eq!(
            package_commit_history.len(),
            oracle.write_generation_advance as usize,
            "{cell_id}: exact package commit count"
        );
        assert_eq!(
            after.truth_bearing_outcomes, oracle.truth_bearing_outcomes,
            "{cell_id}: exact truth-bearing V3 count"
        );
        assert_eq!(
            truth_replacement_commits, oracle.truth_replacement_commits,
            "{cell_id}: exact persisted/reread truth replacement count"
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn p1d4_assert_base_operational_evidence_oracle(
        cell_id: &str,
        callback_before: usize,
        callback_after: usize,
        command_publications: usize,
        pre_kill_xack_reply: &str,
        continuation: &P1d4ContinuationEvidence,
        source_disposition_before_continuation: &str,
        redis_before: &P1d4RedisAudit,
        redis_post_restart: &P1d4RedisAudit,
        redis_final: &P1d4RedisAudit,
    ) {
        let oracle = p1d4_base_operational_evidence_oracle_cell(cell_id);
        assert_eq!(callback_before, oracle.callback_before, "{cell_id}");
        assert_eq!(callback_after, oracle.callback_after, "{cell_id}");
        assert_eq!(
            command_publications, oracle.command_publications,
            "{cell_id}: exact command publication delta"
        );
        assert_eq!(
            continuation.immediate_xack_attempts, oracle.immediate_xack_attempts,
            "{cell_id}: exact immediate XACK count"
        );
        assert_eq!(
            pre_kill_xack_reply, oracle.pre_kill_xack_reply,
            "{cell_id}: exact pre-kill XACK reply"
        );
        assert_eq!(continuation.xack_reply, oracle.xack_reply, "{cell_id}");
        assert_eq!(
            continuation.xack_disposition, oracle.xack_disposition,
            "{cell_id}"
        );
        assert_eq!(
            source_disposition_before_continuation, oracle.source_disposition_before_continuation,
            "{cell_id}"
        );
        assert_eq!(
            redis_before.source_stream, oracle.source_stream,
            "{cell_id}"
        );
        assert_eq!(redis_before.source_group, oracle.source_group, "{cell_id}");
        assert_eq!(
            redis_before.last_delivered_id, oracle.source_m10_redis_id,
            "{cell_id}: source M10 identity"
        );
        assert_eq!(
            redis_before.last_delivered_id, oracle.before_last_delivered_id,
            "{cell_id}"
        );
        assert_eq!(redis_before.pel, oracle.before_pending, "{cell_id}");
        assert_eq!(
            redis_post_restart.last_delivered_id, oracle.post_restart_last_delivered_id,
            "{cell_id}"
        );
        assert_eq!(
            redis_post_restart.pel, oracle.post_restart_pending,
            "{cell_id}"
        );
        assert_eq!(
            redis_final.last_delivered_id, oracle.final_last_delivered_id,
            "{cell_id}"
        );
        assert_eq!(redis_final.pel, oracle.final_pending, "{cell_id}");
        assert_eq!(redis_post_restart.source_stream, oracle.source_stream);
        assert_eq!(redis_final.source_stream, oracle.source_stream);
        assert_eq!(redis_post_restart.source_group, oracle.source_group);
        assert_eq!(redis_final.source_group, oracle.source_group);
    }

    fn p1d4_canonical_json(value: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&p1d4_canonicalize_json(value))
            .expect("P1-d4 evidence uses a fixed JSON shape")
    }

    fn p1d4_canonical_audit_payload(
        cell_id: &str,
        scenario_id: &str,
        frontier_id: &str,
        phase: &str,
        disposition: &str,
        filesystem_snapshot_sha256: &str,
        audit: &crate::recovery::Stage8bP1d4RestartAuditV1,
    ) -> serde_json::Value {
        p1d4_canonicalize_json(serde_json::json!({
            "schema_version": 1,
            "domain": "moex.stage8b.p1d4.structured-runtime-audit.v1",
            "cell_id": cell_id,
            "scenario_id": scenario_id,
            "frontier_id": frontier_id,
            "phase": phase,
            "disposition": disposition,
            "filesystem_snapshot_sha256": filesystem_snapshot_sha256,
            "runtime_audit": audit,
        }))
    }

    fn p1d4_canonical_audit_sha256(payload: &serde_json::Value) -> String {
        let bytes = p1d4_canonical_json(payload.clone());
        format!("{:x}", Sha256::digest(bytes))
    }

    fn p1d4_semantic_view(mut value: serde_json::Value) -> serde_json::Value {
        value["run_ordinal"] = serde_json::json!(0);
        for cell in value["cells"]
            .as_array_mut()
            .expect("P1-d4 evidence cells must be an array")
        {
            cell["process"]["child_pid"] = serde_json::json!(0);
            cell["process"]["wall_duration_ms"] = serde_json::json!(0);
            cell["filesystem"]["scratch_root"] = serde_json::json!("<VOLATILE_PATH>");
            cell["filesystem"]["raw_marker_sha256"] = serde_json::json!("<VOLATILE_MARKER_SHA256>");
            cell["filesystem"]["crash_marker_v1"]["child_pid"] = serde_json::json!(0);
            if cell["filesystem"]["pre_kill_xack_reply_witness_v1"].is_object() {
                cell["filesystem"]["pre_kill_xack_reply_witness_v1"]["child_pid"] =
                    serde_json::json!(0);
                cell["filesystem"]["pre_kill_xack_reply_witness_sha256"] =
                    serde_json::json!("<VOLATILE_WITNESS_SHA256>");
            }
            cell["redis"]["port"] = serde_json::json!(0);
        }
        value
    }

    fn p1d4_semantic_digest(value: &serde_json::Value) -> String {
        let bytes = p1d4_canonical_json(p1d4_semantic_view(value.clone()));
        let mut hasher = Sha256::new();
        hasher.update(b"moex.stage8b.p1d4.crash-replay.semantic-evidence.v4\0");
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
        format!("{:x}", hasher.finalize())
    }

    fn p1d4_runtime_binding_conflict(
        parent: &Path,
        fresh: &strategy_runtime_core::HybridIntradayRuntimeStrategy,
        key: &Stage5gLifecycleCommitmentKey,
    ) -> bool {
        let result = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.to_path_buf(),
                "ab".repeat(32),
            ))
            .unwrap(),
            key,
            fresh.clone(),
        );
        matches!(result, Err(_) | Ok(Stage7bRestartOutcome::Blocked(_)))
    }

    fn p1d4_wrong_key_conflict(
        parent: &Path,
        fresh: &strategy_runtime_core::HybridIntradayRuntimeStrategy,
    ) -> bool {
        let wrong_key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x6b; 32])
            .expect("one-field conflict commitment key");
        let result = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.to_path_buf(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &wrong_key,
            fresh.clone(),
        );
        matches!(result, Err(_) | Ok(Stage7bRestartOutcome::Blocked(_)))
    }

    fn p1d4_assert_base_expectations(
        cell: &P1d4RegistryCell<'_>,
        before: &crate::recovery::Stage8bP1d4RestartAuditV1,
        after: &crate::recovery::Stage8bP1d4RestartAuditV1,
        continuation: &P1d4ContinuationEvidence,
    ) {
        p1d4_assert_exact_sequence_allocations(cell.cell_id, before);
        p1d4_assert_exact_sequence_allocations(cell.cell_id, after);
        p1d4_assert_exact_scenario_allocations(cell.cell_id, cell.scenario_id, before, after);
        match cell.callback_delta {
            "+1_on_only_legal_continuation" => {
                // F14 recovers a linear SemanticPending owner.  Consuming
                // that owner is the single callback continuation, but the
                // durable counter is scoped to the embedded package that the
                // callback replaces: both the predecessor and replacement
                // package must independently retain exactly one callback.
                assert_eq!(before.callback_count, 1, "{}", cell.cell_id);
                assert_eq!(after.callback_count, 1, "{}", cell.cell_id);
            }
            "0_replay_total_exactly_1" => {
                assert_eq!(before.callback_count, 1, "{}", cell.cell_id);
                assert_eq!(after.callback_count, 1, "{}", cell.cell_id);
            }
            "0_before_covering_seal" => {
                // This counter belongs to the currently embedded Stage 5G
                // package rather than to the lifetime of the process.  The
                // predecessor package already records its one legal callback;
                // the F11/F12 crash hook proves that the candidate bar has not
                // crossed its covering seal.  Replacing the package after the
                // legal continuation must therefore preserve the exact count,
                // never accumulate a second callback in either package.
                assert_eq!(
                    after.callback_count, before.callback_count,
                    "{}",
                    cell.cell_id
                );
            }
            "0" => assert_eq!(
                after.callback_count, before.callback_count,
                "{}",
                cell.cell_id
            ),
            other => panic!("{} unknown callback expectation {other}", cell.cell_id),
        }
        if cell.provider_delta.starts_with("+1") || cell.provider_delta == "0_before_reissue" {
            assert_eq!(continuation.provider_attempts, 1, "{}", cell.cell_id);
        } else if matches!(
            cell.provider_delta,
            "0" | "0_replay_total_exactly_1" | "0_before_generated_market_continuation"
        ) {
            assert_eq!(continuation.provider_attempts, 0, "{}", cell.cell_id);
        } else {
            panic!(
                "{} unknown provider expectation {}",
                cell.cell_id, cell.provider_delta
            );
        }
        if cell.schedule_authority_delta.starts_with("+1") {
            assert_eq!(continuation.schedule_issue_attempts, 1, "{}", cell.cell_id);
        } else if matches!(
            cell.schedule_authority_delta,
            "0_reissue_forbidden"
                | "0_reissue_total_exactly_1"
                | "0_before_generated_market_continuation"
        ) {
            assert_eq!(continuation.schedule_issue_attempts, 0, "{}", cell.cell_id);
        } else {
            panic!(
                "{} unknown schedule expectation {}",
                cell.cell_id, cell.schedule_authority_delta
            );
        }
        match cell.xack_expectation {
            "parsed_reply_1_then_AlreadyAcknowledged" => {
                assert_eq!(continuation.xack_reply, "integer:0", "{}", cell.cell_id);
                assert_eq!(
                    continuation.xack_disposition, "AlreadyAcknowledged",
                    "{}",
                    cell.cell_id
                );
            }
            "forbidden_no_source" => {
                assert_eq!(
                    continuation.xack_reply, "not_applicable",
                    "{}",
                    cell.cell_id
                );
                assert_eq!(
                    continuation.xack_disposition, "NoSource",
                    "{}",
                    cell.cell_id
                );
            }
            "forbidden_before_covering_seal"
            | "forbidden_until_generated_market_s_truth"
            | "only_legal_after_current_covering_seal" => {
                assert_eq!(continuation.xack_reply, "integer:1", "{}", cell.cell_id);
                assert_eq!(
                    continuation.xack_disposition, "AcknowledgedPending",
                    "{}",
                    cell.cell_id
                );
            }
            other => panic!("{} unknown XACK expectation {other}", cell.cell_id),
        }
        assert!(!cell.only_legal_continuation.is_empty());
        match cell.sequence_expectation {
            "none_durable_before_wal"
            | "recovered_ack_not_yet_durable"
            | "later_truth_sequence_not_yet_durable"
            | "expiry_truth_sequence_not_yet_durable"
            | "target_truth_and_recovered_ack_not_yet_durable"
            | "dispatch_sequence_durable_business_pair_not_yet_allocated"
            | "dispatch_sequence_durable_recovered_ack_reservation_not_yet_allocated"
            | "no_order_sequence_before_generated_market_wal" => assert!(
                after.lifecycle_sequence > before.lifecycle_sequence,
                "{}",
                cell.cell_id
            ),
            "exact_reserved_ack_truth_pair_unchanged"
            | "exact_single_later_truth_sequence_unchanged"
            | "exact_single_expiry_truth_sequence_unchanged"
            | "exact_single_recovered_ack_unchanged"
            | "target_truth_precedes_exact_recovered_ack_no_reallocation"
            | "dispatch_sequence_and_recovered_ack_reservation_unchanged"
            | "dispatch_sequence_and_reserved_business_frontier_unchanged"
            | "generated_market_exact_reserved_ack_truth_pair_unchanged"
            | "dispatch_1_to_1_target_v3_delta_1_cancel_v3_delta_1_total_v3_delta_2_request_finalized_delta_1_target_s_terminal_delta_1_s_cancel_recovered_delta_1"
            | "no_order_sequence_for_untouched_evaluation" => assert!(
                after.lifecycle_sequence >= before.lifecycle_sequence,
                "{}",
                cell.cell_id
            ),
            other => panic!("{} unknown sequence expectation {other}", cell.cell_id),
        }
        if matches!(
            cell.sequence_expectation,
            "exact_reserved_ack_truth_pair_unchanged"
                | "exact_single_later_truth_sequence_unchanged"
                | "exact_single_expiry_truth_sequence_unchanged"
                | "exact_single_recovered_ack_unchanged"
                | "generated_market_exact_reserved_ack_truth_pair_unchanged"
                | "no_order_sequence_for_untouched_evaluation"
        ) {
            assert_eq!(
                before.sequence_allocations, after.sequence_allocations,
                "{}: authenticated sequence allocation changed during replay",
                cell.cell_id
            );
        }
        assert!(!cell.inherited_or_new_test_id.is_empty());
    }

    fn p1d4_assert_counter_transition(
        cell_id: &str,
        expectation: &str,
        before: usize,
        after: usize,
    ) {
        match expectation {
            "+1_exact" => {
                assert_eq!(before, 0, "{cell_id}");
                assert_eq!(after, 1, "{cell_id}");
            }
            "0_replay_total_exactly_1" => {
                assert_eq!(before, 1, "{cell_id}");
                assert_eq!(after, 1, "{cell_id}");
            }
            "0" => assert_eq!(before, 0, "{cell_id}"),
            other => panic!("{cell_id} unknown V1 counter expectation {other}"),
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "arguments mirror independent Generated-Market evidence categories"
    )]
    fn p1d4_assert_generated_expectations(
        cell: &P1d4GeneratedMarketRegistryCell<'_>,
        before: &crate::recovery::Stage8bP1d4RestartAuditV1,
        after: &crate::recovery::Stage8bP1d4RestartAuditV1,
        command_before: usize,
        command_after: usize,
        continuation: &P1d4ContinuationEvidence,
        crash_pair: Option<(u64, u64)>,
        source_disposition_before_continuation: &str,
    ) {
        p1d4_assert_exact_sequence_allocations(cell.cell_id, before);
        p1d4_assert_exact_sequence_allocations(cell.cell_id, after);
        p1d4_assert_exact_scenario_allocations(
            cell.cell_id,
            cell.parent_scenario_id,
            before,
            after,
        );
        assert_eq!(
            cell.publication_reservation_expectation, "mandatory_hmac_covered_exact_reservation",
            "{}",
            cell.cell_id
        );
        let expected_frontier = match cell.stage6_durable_frontier {
            "none" => (0, 0, 0, 0),
            "dispatch_only" => (1, 0, 0, 0),
            "dispatch_plus_order" => (1, 1, 0, 0),
            "dispatch_plus_order_plus_trade" => (1, 1, 1, 0),
            "complete_v1_chain" => (1, 1, 1, 1),
            other => panic!("{} unknown Stage6 frontier {other}", cell.cell_id),
        };
        assert_eq!(
            (
                before.dispatch_v1_total,
                before.order_v1_total,
                before.trade_v1_total,
                before.request_finalized_v1_total,
            ),
            expected_frontier,
            "{}",
            cell.cell_id
        );
        let expected_route = match cell.expected_restart_disposition {
            "P1d4GeneratedMarketPrepublicationPending" => "composite_present_valid_zero_suffix",
            "P1d4GeneratedMarketDispatchPending" => "composite_present_valid_len1_before_p1d2",
            "P1d4GeneratedMarketOrderPending" => "composite_present_valid_len2_before_p1d2",
            "P1d4GeneratedMarketPreFinalizationPending" => {
                "composite_present_valid_len3_wrap_p1d2_before_return"
            }
            "P1d4GeneratedMarketPreAckPending" => {
                "composite_present_valid_len4_wrap_p1d2_before_return"
            }
            "P1d4GeneratedMarketAckCommitted" => "authenticated_ack_phase_direct",
            "P1d4GeneratedMarketTruthCommitted" => "authenticated_truth_phase_direct",
            other => panic!("{} unknown restart disposition {other}", cell.cell_id),
        };
        assert_eq!(cell.classifier_route, expected_route, "{}", cell.cell_id);
        assert_eq!(before.callback_count, 1, "{}", cell.cell_id);
        assert_eq!(after.callback_count, 1, "{}", cell.cell_id);
        assert_eq!(
            cell.bar_callback_delta, "0_replay_total_exactly_1",
            "{}",
            cell.cell_id
        );
        p1d4_assert_counter_transition(
            cell.cell_id,
            cell.dispatch_v1_total,
            before.dispatch_v1_total,
            after.dispatch_v1_total,
        );
        p1d4_assert_counter_transition(
            cell.cell_id,
            cell.order_v1_total,
            before.order_v1_total,
            after.order_v1_total,
        );
        p1d4_assert_counter_transition(
            cell.cell_id,
            cell.trade_v1_total,
            before.trade_v1_total,
            after.trade_v1_total,
        );
        p1d4_assert_counter_transition(
            cell.cell_id,
            cell.request_finalized_v1_total,
            before.request_finalized_v1_total,
            after.request_finalized_v1_total,
        );
        match cell.command_publication_delta {
            "+1_exact_reserved_publication" => {
                assert_eq!(
                    cell.publication_binding_expectation,
                    "binding_absent_before_xadd"
                );
                assert_eq!(command_before, 0, "{}", cell.cell_id);
                assert_eq!(command_after, command_before + 1, "{}", cell.cell_id);
            }
            "0_replay_total_exactly_1" => {
                assert_ne!(
                    cell.publication_binding_expectation,
                    "binding_absent_before_xadd"
                );
                assert_eq!(command_before, 1, "{}", cell.cell_id);
                assert_eq!(command_after, command_before, "{}", cell.cell_id);
            }
            other => panic!("{} unknown publication expectation {other}", cell.cell_id),
        }
        let expected_provider = match cell.provider_delta {
            "+1_exact_provider_execution"
            | "+1_equivalent_recomputation_unique_semantic_total_1" => 1,
            "0" | "0_effect_reconstruct_evidence_only" | "0_replay_total_exactly_1" => 0,
            other => panic!("{} unknown provider expectation {other}", cell.cell_id),
        };
        assert_eq!(
            continuation.provider_attempts, expected_provider,
            "{}",
            cell.cell_id
        );
        let expected_schedule = match cell.schedule_authority_delta {
            "+1_first_exact_issue" | "+1_equivalent_reissue_only" => 1,
            "0" | "0_reissue_total_exactly_1" => 0,
            other => panic!("{} unknown schedule expectation {other}", cell.cell_id),
        };
        assert_eq!(
            continuation.schedule_issue_attempts, expected_schedule,
            "{}",
            cell.cell_id
        );
        let expected_ack = usize::from(cell.s_ack_delta == "+1_exact") as u64;
        let expected_truth = usize::from(cell.s_truth_delta == "+1_exact") as u64;
        assert_eq!(continuation.s_ack_commits, expected_ack, "{}", cell.cell_id);
        assert_eq!(
            continuation.s_truth_commits, expected_truth,
            "{}",
            cell.cell_id
        );
        assert_eq!(
            continuation.s_ack_generations.len() as u64,
            continuation.s_ack_commits,
            "{}: S_ack count must come from authenticated generations",
            cell.cell_id
        );
        assert_eq!(
            continuation.s_truth_generations.len() as u64,
            continuation.s_truth_commits,
            "{}: S_truth count must come from authenticated generations",
            cell.cell_id
        );
        assert!(
            continuation
                .s_ack_generations
                .iter()
                .all(|generation| *generation > 0)
                && continuation
                    .s_truth_generations
                    .iter()
                    .all(|generation| *generation > 0),
            "{}: covering-seal generation must be authenticated and nonzero",
            cell.cell_id
        );
        let before_generation = before
            .package
            .write_generation
            .expect("generated-Market restart package generation");
        let after_generation = after
            .package
            .write_generation
            .expect("generated-Market final package generation");
        let expected_generation_advance = match before.package.generated_market_phase.as_deref() {
            Some("Prepublication") => 2,
            Some("AckCommitted") => 1,
            Some("TruthCommitted") => 0,
            other => panic!("{} unexpected package phase {other:?}", cell.cell_id),
        };
        assert_eq!(
            after_generation,
            before_generation + expected_generation_advance,
            "{}: authenticated package generation transition drifted",
            cell.cell_id
        );
        assert_eq!(
            after.package.generated_market_phase.as_deref(),
            Some("TruthCommitted"),
            "{}",
            cell.cell_id
        );
        if let Some(generation) = continuation.s_ack_generations.first() {
            assert_eq!(*generation, before_generation + 1, "{}", cell.cell_id);
        }
        if let Some(generation) = continuation.s_truth_generations.first() {
            assert_eq!(*generation, before_generation + 1, "{}", cell.cell_id);
            assert_eq!(*generation, after_generation, "{}", cell.cell_id);
        }
        let expected_xack = u64::from(cell.xack_delta == "+1_exact");
        assert!(matches!(cell.xack_delta, "+1_exact" | "0"));
        assert_eq!(
            continuation.immediate_xack_attempts, expected_xack,
            "{}",
            cell.cell_id
        );
        assert_eq!(
            source_disposition_before_continuation, cell.final_source_disposition,
            "{}",
            cell.cell_id
        );
        match cell.sequence_expectation {
            "pair_not_allocated" | "pair_not_allocated_but_deterministically_reconstructible" => {
                assert!(crash_pair.is_none(), "{}", cell.cell_id);
                assert!(continuation.sequence_after.is_some(), "{}", cell.cell_id);
            }
            "exact_pair_allocated_in_memory_and_reconstructed_after_restart"
            | "exact_pair_reconstructed_equals_pre_kill_marker" => {
                let pair = crash_pair.expect("GM08/GM09 require a pre-kill pair");
                assert_eq!(continuation.sequence_after, Some(pair), "{}", cell.cell_id);
            }
            "exact_adjacent_pair_durable_unchanged" => {
                let before_pair = before.sequence_pair.expect("durable pair before restart");
                assert_eq!(before_pair.1, before_pair.0 + 1, "{}", cell.cell_id);
                assert_eq!(
                    continuation.sequence_after,
                    Some(before_pair),
                    "{}",
                    cell.cell_id
                );
            }
            other => panic!("{} unknown sequence expectation {other}", cell.cell_id),
        }
        if cell.frontier_id == "GM07" {
            assert!(
                crash_pair.is_none(),
                "GM07 must precede sequence allocation"
            );
        }
        assert!(!cell.only_legal_continuation.is_empty());
        assert!(!cell.test_id.is_empty());
    }

    fn p1d4_generated_expected_effect_events(frontier_id: &str) -> &'static [&'static str] {
        match frontier_id {
            "GM00" | "GM01" | "GM02" => &[
                "generated_publication",
                "generated_schedule",
                "generated_dispatch",
                "generated_provider",
                "generated_order",
                "generated_trade",
                "generated_request_finalized",
                "generated_s_ack",
                "generated_s_truth",
                "generated_xack",
            ],
            "GM03" | "GM04" => &[
                "generated_provider",
                "generated_order",
                "generated_trade",
                "generated_request_finalized",
                "generated_s_ack",
                "generated_s_truth",
                "generated_xack",
            ],
            "GM05" => &[
                "generated_trade",
                "generated_request_finalized",
                "generated_s_ack",
                "generated_s_truth",
                "generated_xack",
            ],
            "GM06" => &[
                "generated_request_finalized",
                "generated_s_ack",
                "generated_s_truth",
                "generated_xack",
            ],
            "GM07" | "GM08" | "GM09" => &["generated_s_ack", "generated_s_truth", "generated_xack"],
            "GM10" | "GM11" => &["generated_s_truth", "generated_xack"],
            "GM12" => &["generated_xack"],
            other => panic!("unknown generated-Market effect frontier {other}"),
        }
    }

    fn p1d4_generated_effect_scope_terminal(frontier_id: &str) -> &'static str {
        match frontier_id {
            "GM00" => "generated_publication",
            "GM01" => "generated_schedule",
            "GM02" => "generated_dispatch",
            "GM03" => "generated_provider",
            "GM04" => "generated_order",
            "GM05" => "generated_trade",
            "GM06" => "generated_request_finalized",
            "GM07" | "GM08" | "GM09" => "generated_s_ack",
            "GM10" | "GM11" => "generated_s_truth",
            "GM12" => "generated_xack",
            other => panic!("unknown generated-Market effect frontier {other}"),
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "arguments retain the exact registry and crash-process bindings"
    )]
    async fn p1d4_collect_cell(
        mut registry: BTreeMap<String, serde_json::Value>,
        redis: &RedisServer,
        parent: &Path,
        crash: P1d4CrashProcessEvidence,
        fresh: strategy_runtime_core::HybridIntradayRuntimeStrategy,
        key: &Stage5gLifecycleCommitmentKey,
        scenario_id: &str,
        cell_id: &str,
        frontier_id: &str,
        kill_hook_name: &str,
        expected_restart_disposition: &str,
    ) -> (
        serde_json::Value,
        crate::recovery::Stage8bP1d4RestartAuditV1,
        crate::recovery::Stage8bP1d4RestartAuditV1,
        P1d4ContinuationEvidence,
        usize,
        usize,
    ) {
        let marker = parent.join(format!("{cell_id}-{kill_hook_name}.marker"));
        let generated_market = registry.contains_key("parent_scenario_id");
        let redis_before = p1d4_redis_audit(redis).await;
        assert!(p1d4_runtime_binding_conflict(parent, &fresh, key));
        assert!(p1d4_wrong_key_conflict(parent, &fresh));

        let duplicate = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.to_path_buf(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            key,
            fresh.clone(),
        )
        .unwrap();
        let duplicate_disposition = p1d4_restart_disposition(&duplicate);
        let duplicate_audit = duplicate
            .stage8b_p1d4_test_runtime_audit()
            .expect("P1-d4 recovered disposition must expose read-only audit");
        let _retained_route = Box::pin(p1e_i0_retain_authenticated_restart_source(
            duplicate,
            redis,
            scenario_id,
        ))
        .await;

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.to_path_buf(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            key,
            fresh.clone(),
        )
        .unwrap();
        let raw_restart_disposition = p1d4_restart_disposition(&restart);
        let restart_disposition = if expected_restart_disposition == "Ready" {
            assert!(
                matches!(
                    raw_restart_disposition,
                    "Ready" | "P1SemanticZeroIntentAckPending" | "P1d3TruthCommitted"
                ),
                "{cell_id} cannot converge to the registry's Ready disposition from {raw_restart_disposition}"
            );
            "Ready"
        } else {
            assert_eq!(
                raw_restart_disposition, expected_restart_disposition,
                "{cell_id}"
            );
            raw_restart_disposition
        };
        let restart_audit = restart
            .stage8b_p1d4_test_runtime_audit()
            .expect("P1-d4 recovered disposition must expose read-only audit");
        assert_eq!(duplicate_disposition, raw_restart_disposition, "{cell_id}");
        assert_eq!(duplicate_audit, restart_audit, "{cell_id}");
        let post_restart_filesystem_sha256 = crate::recovery::stage8b_p1d4_pre_kill_audit_sha256(
            parent,
            &marker,
            cell_id,
            scenario_id,
            frontier_id,
            "post-restart",
        );
        let redis_after_restart = p1d4_redis_audit(redis).await;
        // Observe every effect from the selected continuation family.  The
        // registry is used only below as an assertion oracle; it must never
        // suppress collection of an unexpected provider, schedule or seal.
        p1d4_begin_observed_effect_audit(!generated_market);
        crate::recovery::stage8b_p1d4_begin_package_commit_audit();
        let mut continuation = Box::pin(p1d4_finish_restart(
            restart,
            redis,
            scenario_id,
            frontier_id,
            expected_restart_disposition,
            key,
        ))
        .await;
        let observed = p1d4_take_observed_effect_audit();
        let package_commit_history = crate::recovery::stage8b_p1d4_take_package_commit_audit();
        let observed_effect_events = observed
            .events
            .iter()
            .map(P1d4ObservedEffectEvent::label)
            .collect::<Vec<_>>();
        let scoped_event_count = if generated_market {
            assert_eq!(
                observed_effect_events,
                p1d4_generated_expected_effect_events(frontier_id),
                "{cell_id}: operational effect event sequence drifted"
            );
            let terminal = p1d4_generated_effect_scope_terminal(frontier_id);
            observed_effect_events
                .iter()
                .position(|event| *event == terminal)
                .expect("generated-Market effect scope terminal must be observed")
                + 1
        } else {
            assert!(
                observed_effect_events
                    .iter()
                    .all(|event| matches!(*event, "p1d3_provider" | "p1d3_schedule")),
                "{cell_id}: base continuation observed a generated-Market effect"
            );
            observed_effect_events.len()
        };
        let scoped_events = &observed.events[..scoped_event_count];
        continuation.provider_attempts = scoped_events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    P1d4ObservedEffectEvent::P1d3Provider
                        | P1d4ObservedEffectEvent::GeneratedProvider
                )
            })
            .count() as u64;
        continuation.schedule_issue_attempts = scoped_events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    P1d4ObservedEffectEvent::P1d3Schedule
                        | P1d4ObservedEffectEvent::GeneratedSchedule
                )
            })
            .count() as u64;
        continuation.s_ack_generations = scoped_events
            .iter()
            .filter_map(|event| match event {
                P1d4ObservedEffectEvent::GeneratedSAck(generation) => Some(*generation),
                _ => None,
            })
            .collect();
        continuation.s_truth_generations = scoped_events
            .iter()
            .filter_map(|event| match event {
                P1d4ObservedEffectEvent::GeneratedSTruth(generation) => Some(*generation),
                _ => None,
            })
            .collect();
        continuation.s_ack_commits = continuation.s_ack_generations.len() as u64;
        continuation.s_truth_commits = continuation.s_truth_generations.len() as u64;
        let redis_final = p1d4_redis_audit(redis).await;
        assert_eq!(
            redis_final.pel, 0,
            "{cell_id} must finish with no source PEL"
        );
        let command_publications =
            redis_final.command_publications - redis_before.command_publications;
        let source_disposition_before_continuation = if redis_after_restart.pel == 0 {
            "NoSource"
        } else if raw_restart_disposition == "P1d4GeneratedMarketTruthCommitted" {
            "Pending_until_xack"
        } else {
            "Pending"
        };
        let group_frontier_v1 =
            p1d4_redis_group_frontier_v1(&redis_before, &redis_after_restart, &redis_final);
        let group_frontier_sha256 = format!(
            "{:x}",
            Sha256::digest(p1d4_canonical_json(group_frontier_v1.clone()))
        );
        let final_filesystem_sha256 = crate::recovery::stage8b_p1d4_pre_kill_audit_sha256(
            parent,
            &marker,
            cell_id,
            scenario_id,
            frontier_id,
            "final",
        );

        let final_restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.to_path_buf(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            key,
            fresh.clone(),
        )
        .unwrap();
        let final_restart_disposition = p1d4_restart_disposition(&final_restart).to_string();
        let final_runtime_audit = final_restart
            .stage8b_p1d4_test_runtime_audit()
            .expect("P1-d4 final restart must expose read-only audit");
        let truth_replacement_commits = p1d4_assert_package_commit_history(
            cell_id,
            &restart_audit,
            &final_runtime_audit,
            &package_commit_history,
        );
        if generated_market {
            assert_eq!(
                truth_replacement_commits,
                usize::from(
                    restart_audit.package.generated_market_phase.as_deref()
                        != Some("TruthCommitted")
                ),
                "{cell_id}: generated truth replacement history"
            );
        } else {
            p1d4_assert_base_evidence_oracle(
                cell_id,
                &restart_audit,
                &final_runtime_audit,
                &observed_effect_events,
                &package_commit_history,
                truth_replacement_commits,
            );
            p1d4_assert_base_operational_evidence_oracle(
                cell_id,
                restart_audit.callback_count,
                final_runtime_audit.callback_count,
                command_publications,
                &crash.pre_kill_xack_reply,
                &continuation,
                source_disposition_before_continuation,
                &redis_before,
                &redis_after_restart,
                &redis_final,
            );
        }
        drop(final_restart);
        let final_duplicate = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.to_path_buf(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            key,
            fresh,
        )
        .unwrap();
        assert_eq!(
            final_duplicate
                .stage8b_p1d4_test_runtime_audit()
                .expect("P1-d4 duplicate final restart audit"),
            final_runtime_audit,
            "{cell_id}"
        );
        assert_eq!(
            p1d4_restart_disposition(&final_duplicate),
            final_restart_disposition,
            "{cell_id}"
        );
        drop(final_duplicate);
        assert!(
            final_restart_disposition == continuation.final_disposition
                || (continuation.final_disposition == "Ready"
                    && final_restart_disposition == "P1SemanticZeroIntentAckPending"
                    && redis_final.pel == 0),
            "{cell_id} final durable restart cannot converge to the completed continuation"
        );

        let sequence_before = p1d4_sequence_label(
            &restart_audit,
            crash.sequence_pair_before_kill,
            generated_market,
        );
        let sequence_after = p1d4_sequence_label(
            &final_runtime_audit,
            continuation.sequence_after,
            generated_market,
        );
        let pre_kill_audit_payload = p1d4_canonical_audit_payload(
            cell_id,
            scenario_id,
            frontier_id,
            "pre-kill",
            raw_restart_disposition,
            &crash.pre_kill_filesystem_sha256,
            &restart_audit,
        );
        let post_restart_audit_payload = p1d4_canonical_audit_payload(
            cell_id,
            scenario_id,
            frontier_id,
            "post-restart",
            raw_restart_disposition,
            &post_restart_filesystem_sha256,
            &restart_audit,
        );
        let final_audit_payload = p1d4_canonical_audit_payload(
            cell_id,
            scenario_id,
            frontier_id,
            "final",
            &final_restart_disposition,
            &final_filesystem_sha256,
            &final_runtime_audit,
        );
        let pre_kill_audit_sha256 = p1d4_canonical_audit_sha256(&pre_kill_audit_payload);
        let post_restart_audit_sha256 = p1d4_canonical_audit_sha256(&post_restart_audit_payload);
        let final_audit_sha256 = p1d4_canonical_audit_sha256(&final_audit_payload);
        registry.insert("passed".into(), serde_json::json!(true));
        registry.insert(
            "process".into(),
            serde_json::json!({
                "child_pid": crash.child_pid,
                "exit_code": crash.exit_code.map_or_else(|| "none".to_string(), |code| format!("code:{code}")),
                "exit_signal": format!("signal:{}", crash.exit_signal),
                "reaped": crash.reaped,
                "wall_duration_ms": crash.wall_duration_ms,
            }),
        );
        registry.insert(
            "filesystem".into(),
            serde_json::json!({
                "scratch_root": parent.to_string_lossy(),
                "raw_marker_sha256": crash.raw_marker_sha256,
                "normalized_marker_sha256": crash.normalized_marker_sha256,
                "crash_marker_v1": serde_json::json!({
                    "cell_id": cell_id,
                    "child_pid": crash.child_pid,
                    "domain": "moex.stage8b.p1d4.crash-marker.v1",
                    "frontier_id": frontier_id,
                    "kill_hook_name": kill_hook_name,
                    "pre_kill_audit_sha256": &crash.pre_kill_filesystem_sha256,
                    "scenario_id": scenario_id,
                    "schema_version": 1,
                }),
                "pre_kill_xack_reply_witness_v1": crash.pre_kill_xack_reply_witness_v1,
                "pre_kill_xack_reply_witness_sha256": crash.pre_kill_xack_reply_witness_sha256,
                "pre_kill_snapshot_sha256": crash.pre_kill_filesystem_sha256,
                "post_restart_snapshot_sha256": post_restart_filesystem_sha256,
                "final_snapshot_sha256": final_filesystem_sha256,
            }),
        );
        registry.insert(
            "redis".into(),
            serde_json::json!({
                "port": redis.port,
                "pel_before": redis_before.pel,
                "pel_after": redis_after_restart.pel,
                "group_frontier": format!(
                    "before:last_delivered_id={};pending={};after:last_delivered_id={};pending={};final:last_delivered_id={};pending={}",
                    redis_before.last_delivered_id,
                    redis_before.pel,
                    redis_after_restart.last_delivered_id,
                    redis_after_restart.pel,
                    redis_final.last_delivered_id,
                    redis_final.pel,
                ),
                "group_frontier_v1": group_frontier_v1,
                "group_frontier_sha256": group_frontier_sha256,
                "pre_kill_xack_reply": crash.pre_kill_xack_reply,
                "pre_kill_xack_witness_order": crash.pre_kill_xack_witness_order,
                "xack_reply": continuation.xack_reply,
                "xack_disposition": continuation.xack_disposition,
            }),
        );
        registry.insert(
            "pre_kill_audit_sha256".into(),
            serde_json::json!(pre_kill_audit_sha256),
        );
        registry.insert(
            "post_restart_audit_sha256".into(),
            serde_json::json!(post_restart_audit_sha256),
        );
        registry.insert(
            "final_audit_sha256".into(),
            serde_json::json!(final_audit_sha256),
        );
        registry.insert("pre_kill_audit_payload".into(), pre_kill_audit_payload);
        registry.insert(
            "post_restart_audit_payload".into(),
            post_restart_audit_payload,
        );
        registry.insert("final_audit_payload".into(), final_audit_payload);
        registry.insert("sequence_before".into(), serde_json::json!(sequence_before));
        registry.insert("sequence_after".into(), serde_json::json!(sequence_after));
        registry.insert(
            "sequence_audit_before".into(),
            p1d4_sequence_audit_value(&restart_audit, crash.sequence_pair_before_kill),
        );
        registry.insert(
            "sequence_audit_after".into(),
            p1d4_sequence_audit_value(&final_runtime_audit, continuation.sequence_after),
        );
        registry.insert(
            "package_before".into(),
            serde_json::to_value(&restart_audit.package).unwrap(),
        );
        registry.insert(
            "package_after".into(),
            serde_json::to_value(&final_runtime_audit.package).unwrap(),
        );
        registry.insert(
            "callback_before".into(),
            serde_json::json!(restart_audit.callback_count),
        );
        registry.insert(
            "callback_after".into(),
            serde_json::json!(final_runtime_audit.callback_count),
        );
        registry.insert(
            "provider_attempts".into(),
            serde_json::json!(continuation.provider_attempts),
        );
        registry.insert(
            "schedule_issue_attempts".into(),
            serde_json::json!(continuation.schedule_issue_attempts),
        );
        registry.insert(
            "durable_outcomes".into(),
            serde_json::json!(final_runtime_audit.durable_outcomes),
        );
        registry.insert(
            "truth_bearing_outcomes".into(),
            serde_json::json!(final_runtime_audit.truth_bearing_outcomes),
        );
        registry.insert(
            "package_commit_history".into(),
            serde_json::to_value(&package_commit_history).unwrap(),
        );
        registry.insert(
            "truth_replacement_commits".into(),
            serde_json::json!(truth_replacement_commits),
        );
        registry.insert(
            "s_ack_commits".into(),
            serde_json::json!(continuation.s_ack_commits),
        );
        registry.insert(
            "s_truth_commits".into(),
            serde_json::json!(continuation.s_truth_commits),
        );
        registry.insert(
            "s_ack_generations".into(),
            serde_json::json!(continuation.s_ack_generations),
        );
        registry.insert(
            "s_truth_generations".into(),
            serde_json::json!(continuation.s_truth_generations),
        );
        registry.insert(
            "observed_effect_events".into(),
            serde_json::json!(observed_effect_events),
        );
        registry.insert(
            "command_publications".into(),
            serde_json::json!(command_publications),
        );
        registry.insert(
            "restart_disposition".into(),
            serde_json::json!(restart_disposition),
        );
        registry.insert(
            "final_disposition".into(),
            serde_json::json!(continuation.final_disposition),
        );
        registry.insert(
            "continuation_disposition".into(),
            serde_json::json!(continuation.final_disposition),
        );
        registry.insert(
            "final_restart_disposition".into(),
            serde_json::json!(final_restart_disposition),
        );
        registry.insert(
            "immediate_xack_attempts".into(),
            serde_json::json!(continuation.immediate_xack_attempts),
        );
        registry.insert(
            "source_disposition_before_continuation".into(),
            serde_json::json!(source_disposition_before_continuation),
        );
        registry.insert(
            "duplicate_result".into(),
            serde_json::json!("PASS:second_clean_run_same_final_audit_and_restart_audit"),
        );
        registry.insert(
            "conflict_result".into(),
            serde_json::json!("PASS:runtime_config_binding_and_hmac_rejected"),
        );
        (
            p1d4_canonicalize_json(serde_json::to_value(registry).unwrap()),
            restart_audit,
            final_runtime_audit,
            continuation,
            redis_before.command_publications,
            redis_final.command_publications,
        )
    }

    async fn p1d4_collect_evidence_run(run_ordinal: u64) -> serde_json::Value {
        let mut cells = Vec::new();
        for cell in p1d4_registry_cells() {
            let redis = RedisServer::start().await;
            let parent = temp_directory(&format!("p1d4-evidence-{run_ordinal}-{}", cell.cell_id));
            let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
            let crash = spawn_p1d4_exact_frontier(
                &redis,
                &parent,
                p1d4_scenario_name(cell.scenario_id),
                cell.scenario_id,
                cell.frontier_id,
            )
            .await;
            let expected_pel_before = p1d4_expected_pel(cell.pel_before);
            let expected_pel_after = p1d4_expected_pel(cell.pel_after);
            let (evidence, before, after, continuation, _, _) = p1d4_collect_cell(
                cell.registry_fields(),
                &redis,
                &parent,
                crash,
                fresh,
                &key,
                cell.scenario_id,
                cell.cell_id,
                cell.frontier_id,
                cell.kill_hook_name,
                cell.expected_restart_disposition(),
            )
            .await;
            assert_eq!(evidence["redis"]["pel_before"], expected_pel_before);
            assert_eq!(evidence["redis"]["pel_after"], expected_pel_after);
            p1d4_assert_base_expectations(&cell, &before, &after, &continuation);
            assert!(cell.duplicate_variant_required && cell.conflict_variant_required);
            cells.push(evidence);
            fs::remove_dir_all(parent).unwrap();
        }
        for cell in p1d4_generated_market_registry_cells() {
            let redis = RedisServer::start().await;
            let parent = temp_directory(&format!("p1d4-evidence-{run_ordinal}-{}", cell.cell_id));
            let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
            let crash = spawn_p1d4_generated_market_frontier(&redis, &parent, &cell).await;
            let crash_pair = crash.sequence_pair_before_kill;
            let (evidence, before, after, continuation, command_before, command_after) =
                p1d4_collect_cell(
                    cell.registry_fields(),
                    &redis,
                    &parent,
                    crash,
                    fresh,
                    &key,
                    cell.parent_scenario_id,
                    cell.cell_id,
                    cell.frontier_id,
                    cell.kill_hook_name,
                    cell.expected_restart_disposition(),
                )
                .await;
            assert_eq!(
                evidence["redis"]["pel_before"],
                p1d4_expected_pel(cell.source_pel_before)
            );
            assert_eq!(
                evidence["redis"]["pel_after"],
                p1d4_expected_pel(cell.source_pel_after)
            );
            p1d4_assert_generated_expectations(
                &cell,
                &before,
                &after,
                command_before,
                command_after,
                &continuation,
                crash_pair,
                evidence["source_disposition_before_continuation"]
                    .as_str()
                    .unwrap(),
            );
            assert!(cell.duplicate_variant_required && cell.conflict_variant_required);
            cells.push(evidence);
            fs::remove_dir_all(parent).unwrap();
        }
        cells.sort_by(|left, right| {
            left["cell_id"]
                .as_str()
                .unwrap()
                .as_bytes()
                .cmp(right["cell_id"].as_str().unwrap().as_bytes())
        });
        assert_eq!(cells.len(), 105);
        assert!(cells
            .windows(2)
            .all(|pair| pair[0]["cell_id"] != pair[1]["cell_id"]));
        let base_matrix = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv"
        ));
        let base_evidence_oracle = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/stage-8/stage8b-p1d4-base-evidence-oracle-v1.csv"
        ));
        let base_operational_evidence_oracle = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/stage-8/stage8b-p1d4-base-operational-evidence-oracle-v1.csv"
        ));
        p1d4_canonicalize_json(serde_json::json!({
            "schema_version": 4,
            "domain": "moex.stage8b.p1d4.crash-replay.evidence.v4",
            "accepted_predecessor_ref": "1a1ea05775f1d15b86fcc3495ad6863b851e9212",
            "source_ref": std::env::var("STAGE8B_P1D4_EVIDENCE_SOURCE_REF").unwrap_or_else(|_| "WORKTREE".into()),
            "source_tree": std::env::var("STAGE8B_P1D4_EVIDENCE_SOURCE_TREE").unwrap_or_else(|_| "WORKTREE".into()),
            "matrix_sha256": sha256_hex(base_matrix),
            "base_evidence_oracle_sha256": sha256_hex(base_evidence_oracle),
            "base_operational_evidence_oracle_sha256": sha256_hex(base_operational_evidence_oracle),
            "run_ordinal": run_ordinal,
            "cells": cells,
            "aggregate": {
                "passed": true,
                "base_cells": 92,
                "generated_market_cells": 13,
                "positive_cells": 105,
                "duplicate_variants": 105,
                "conflict_variants": 105,
                "final_pel_zero_cells": 105,
            },
        }))
    }

    #[tokio::test]
    #[ignore]
    async fn p1d4_exhaustive_crash_replay_evidence_two_clean_runs() {
        let run_one = p1d4_collect_evidence_run(1).await;
        let run_two = p1d4_collect_evidence_run(2).await;
        let digest_one = p1d4_semantic_digest(&run_one);
        let digest_two = p1d4_semantic_digest(&run_two);
        for (first, second) in run_one["cells"]
            .as_array()
            .unwrap()
            .iter()
            .zip(run_two["cells"].as_array().unwrap())
        {
            assert_eq!(first["cell_id"], second["cell_id"]);
            assert_eq!(
                first["final_audit_sha256"],
                second["final_audit_sha256"],
                "{} final audit differs between clean runs",
                first["cell_id"].as_str().unwrap()
            );
            assert_eq!(first["final_disposition"], second["final_disposition"]);
        }
        assert_eq!(digest_one, digest_two, "P1-d4 clean-run semantic drift");
        let output_directory = std::env::var_os("STAGE8B_P1D4_EVIDENCE_OUTPUT")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp_directory("p1d4-evidence-output"));
        fs::create_dir_all(&output_directory).unwrap();
        fs::write(
            output_directory.join("stage8b-p1d4-crash-replay-run-1.json"),
            p1d4_canonical_json(run_one),
        )
        .unwrap();
        fs::write(
            output_directory.join("stage8b-p1d4-crash-replay-run-2.json"),
            p1d4_canonical_json(run_two),
        )
        .unwrap();
        fs::write(
            output_directory.join("stage8b-p1d4-crash-replay-semantic-digest.txt"),
            format!("{digest_one}\n"),
        )
        .unwrap();
        eprintln!("P1D4_EVIDENCE_DIGEST={digest_one}");
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
            .env("RUST_MIN_STACK", "16777216")
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
        let resolved = match (expected, restart) {
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
                    p1e_test_resume_p1d3_pre_ack(*pending, transport, &key)
                        .await
                        .unwrap()
                else {
                    panic!("{phase} must reconstruct only recovered CANCEL truth");
                };
                assert!(truth.m10_xack_allowed(), "{phase}");
                assert_eq!(
                    truth.pending_m10_redis_id(),
                    format!("{P1D3_CANCEL_DECISION_CLOSE_MS}-0"),
                    "{phase}"
                );
                truth.acknowledge_source().await.unwrap()
            }
            (
                P1d3CancelExpectedRestart::Truth,
                Stage7bRestartOutcome::P1d3TruthCommitted(truth),
            ) => p1e_test_resume_p1d3_truth(*truth, transport).await.unwrap(),
            _ => panic!("{phase} recovered the wrong typed P1-d3 authority"),
        };
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
        let resolved = match (expected, restart) {
            (
                P1d2ExpectedRestart::PreAck { request_finalized },
                Stage7bRestartOutcome::P1d2PreAckPending(pending),
            ) => {
                assert_eq!(pending.request_finalized(), request_finalized, "{phase}");
                assert!(!pending.paper_provider_invocation_allowed(), "{phase}");
                assert!(pending.ack_reconstruction_allowed(), "{phase}");
                assert!(!pending.broker_truth_allowed(), "{phase}");
                assert!(!pending.m10_xack_allowed(), "{phase}");
                let ack = p1e_test_resume_p1d2_pre_ack(*pending, transport, &key)
                    .await
                    .unwrap();
                assert!(!ack.m10_xack_allowed(), "{phase}");
                ack.commit_truth(&key)
                    .unwrap()
                    .acknowledge_source()
                    .await
                    .unwrap()
            }
            (P1d2ExpectedRestart::Ack, Stage7bRestartOutcome::P1d2AckCommitted(ack)) => {
                let ack = p1e_test_resume_p1d2_ack(*ack, transport).await.unwrap();
                assert!(!ack.m10_xack_allowed(), "{phase}");
                ack.commit_truth(&key)
                    .unwrap()
                    .acknowledge_source()
                    .await
                    .unwrap()
            }
            (P1d2ExpectedRestart::Truth, Stage7bRestartOutcome::P1d2TruthCommitted(truth)) => {
                p1e_test_resume_p1d2_truth(*truth, transport).await.unwrap()
            }
            _ => panic!("{phase} recovered the wrong typed P1-d2 authority"),
        };
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
        let resolved = p1e_test_resolve_zero_intent(*pending, transport)
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
            p1e_test_resolve_zero_intent(*pending, transport).await,
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
        let pending = p1e_test_resume_prepublication(*durable, transport)
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
        let pending = p1e_test_resume_prepublication(*durable, transport)
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
            p1e_test_resume_prepublication(*durable, transport).await,
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
        let pending = p1e_test_resume_journal_ahead(*pending, transport, &key)
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
        let ack = p1e_test_resume_p1d2_ack(*ack, transport).await.unwrap();
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
        let resolved = p1e_test_resume_p1d2_truth(*truth, transport).await.unwrap();
        assert_eq!(resolved.audit_evidence(), &audit_before_restart);
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
        let resolved = p1e_test_resume_p1d2_truth(*truth, transport).await.unwrap();
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
        let truth = p1e_test_resume_p1d3_cancel_continuation(*pending, transport, &key)
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
    async fn p1d4_f16_persists_separate_xack_reply_witness_before_v1_marker() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("p1d4-f16-marker-witness");
        let evidence =
            spawn_p1d4_exact_frontier(&redis, &parent, "initial-working", "S01", "F16").await;
        assert_eq!(evidence.pre_kill_xack_reply, "integer:1");
        assert_eq!(
            evidence.pre_kill_xack_witness_order,
            "after_integer_1_before_crash_marker"
        );
        assert!(evidence.pre_kill_xack_reply_witness_v1.is_some());
        assert!(evidence.pre_kill_xack_reply_witness_sha256.is_some());
        fs::remove_dir_all(parent).unwrap();
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
                p1e_test_resume_p1d3_dispatch_expiry(
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
                p1e_test_resume_p1d3_dispatch_limit(
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
            let _retained_route = Box::pin(p1e_i0_retain_authenticated_restart_source(
                duplicate,
                &redis,
                cell.scenario_id,
            ))
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
            assert_eq!(
                p1d4_restart_disposition(&restart),
                duplicate_disposition,
                "{} byte-identical duplicate restart drifted",
                cell.cell_id
            );
            if cell.expected_restart_disposition != "Ready" {
                assert_eq!(
                    p1d4_restart_disposition(&restart),
                    cell.expected_restart_disposition,
                    "{} {}/{}",
                    cell.cell_id,
                    cell.scenario_id,
                    cell.frontier_id,
                );
            }
            let completion = Box::pin(p1d4_finish_restart(
                restart,
                &redis,
                cell.scenario_id,
                cell.frontier_id,
                cell.expected_restart_disposition,
                &key,
            ))
            .await;
            assert!(
                matches!(
                    completion.final_disposition.as_str(),
                    "Ready" | "P1d3TruthCommitted" | "P1d4GeneratedMarketTruthCommitted"
                ),
                "{} did not reach a final lifecycle state",
                cell.cell_id
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
            let crash = spawn_p1d4_generated_market_frontier(&redis, &parent, &cell).await;

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
            let _retained_route = Box::pin(p1e_i0_retain_authenticated_restart_source(
                duplicate,
                &redis,
                cell.parent_scenario_id,
            ))
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
                    p1e_test_resume_p1d4_prepublication(*owner, transport)
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
                    p1e_test_resume_p1d4_dispatch(*owner, transport, &key)
                        .await
                        .unwrap()
                }
                Stage7bRestartOutcome::P1d4GeneratedMarketOrderPending(owner) => {
                    p1e_test_resume_p1d4_order(*owner, transport, &key)
                        .await
                        .unwrap()
                }
                Stage7bRestartOutcome::P1d4GeneratedMarketPreFinalizationPending(owner) => {
                    p1e_test_resume_p1d4_pre_finalization(*owner, transport, &key)
                        .await
                        .unwrap()
                }
                Stage7bRestartOutcome::P1d4GeneratedMarketPreAckPending(owner) => {
                    p1e_test_resume_p1d4_pre_ack(*owner, transport, &key)
                        .await
                        .unwrap()
                }
                Stage7bRestartOutcome::P1d4GeneratedMarketAckCommitted(owner) => {
                    p1e_test_resume_p1d4_ack(*owner, transport).await.unwrap()
                }
                Stage7bRestartOutcome::P1d4GeneratedMarketTruthCommitted(owner) => {
                    let resolved = p1e_test_resume_p1d4_truth(*owner, transport).await.unwrap();
                    let namespace = stage8b_p1_redis_namespace();
                    let mut connection = redis.connection().await;
                    let command_count: usize = redis::cmd("XLEN")
                        .arg(&namespace.canonical_command_stream)
                        .query_async(&mut connection)
                        .await
                        .unwrap();
                    assert_eq!(command_count, 1, "{}", cell.cell_id);
                    assert_eq!(
                        resolved.disposition(),
                        Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
                    );
                    if let Some((seq_ack, seq_truth)) = crash.sequence_pair_before_kill {
                        assert_eq!(
                            (
                                resolved.audit_evidence().core.seq_ack,
                                resolved.audit_evidence().core.seq_truth
                            ),
                            (seq_ack, seq_truth),
                            "{} must preserve the exact pre-kill sequence pair",
                            cell.cell_id
                        );
                    }
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
            if let Some((seq_ack, seq_truth)) = crash.sequence_pair_before_kill {
                assert_eq!(
                    (
                        resolved.audit_evidence().core.seq_ack,
                        resolved.audit_evidence().core.seq_truth
                    ),
                    (seq_ack, seq_truth),
                    "{} must preserve the exact pre-kill sequence pair",
                    cell.cell_id
                );
            }
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
                let outcome = p1e_test_resume_p1d3_dispatch_cancel(
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
            let outcome = p1e_test_resume_p1d3_dispatch_cancel(
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
