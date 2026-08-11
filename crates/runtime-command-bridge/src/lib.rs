//! Stage 7A paper-only Redis command bridge.
//!
//! This crate intentionally has no FINAM client/gateway dependency and no
//! broker network effect. Redis delivery metadata is kept outside the Stage 6
//! durable identity authority.

use broker_core::command::CommandAckStatus;
use broker_core::{
    BrokerAccountId, BrokerCommand, CommandAck, CommandAckReason, CommandAckReasonCode, Envelope,
    HybridRuntimeAttribution, InstrumentId, MessageType, StrategyRequestId, SCHEMA_VERSION,
};
use chrono::{DateTime, Utc};
use redis::aio::ConnectionManager;
use redis::streams::{StreamAutoClaimReply, StreamId, StreamReadReply};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use strategy_runtime_core::{
    admit_stage7a_paper_command, execute_stage6d_paper_outcome,
    resolve_stage7a_cancel_command_context, Stage6dDurableRuntimeRecovered, Stage6dLiveCoreError,
    Stage6dPaperOutcome, Stage7aPaperAdmission, Stage7aPaperAdmissionDecision,
    Stage7aPaperCommandContext, Stage7aPaperHoldReason, Stage7aPaperPolicyRejection,
};

pub const STAGE7A_PAPER_NAMESPACE: &str = "finam_imoexf_paper:";
const DLQ_DOMAIN: &[u8] = b"moex.stage7a.redacted-dlq.v1";
static CONSUMER_INSTANCE_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage7aGroupStart {
    Tail,
    Beginning,
}

impl Stage7aGroupStart {
    fn redis_id(self) -> &'static str {
        match self {
            Self::Tail => "$",
            Self::Beginning => "0-0",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage7aRedisConfig {
    pub command_stream: String,
    pub ack_stream: String,
    pub dlq_stream: String,
    pub health_stream: String,
    pub readiness_stream: String,
    pub consumer_group: String,
    pub consumer_name: String,
    pub group_start: Stage7aGroupStart,
    pub allow_controlled_beginning: bool,
    pub read_count: usize,
    pub claim_count: usize,
    pub block_ms: u64,
    pub claim_idle_ms: u64,
    pub max_claim_pages: usize,
}

impl Stage7aRedisConfig {
    pub fn paper_default(process_instance: &str) -> Result<Self, Stage7aBridgeError> {
        let suffix = canonical_token(process_instance)?;
        let config = Self {
            command_stream: "finam_imoexf_paper:runtime:commands".to_string(),
            ack_stream: "finam_imoexf_paper:runtime:command-acks".to_string(),
            dlq_stream: "finam_imoexf_paper:runtime:commands:dlq".to_string(),
            health_stream: "finam_imoexf_paper:runtime:command-consumer:health".to_string(),
            readiness_stream: "finam_imoexf_paper:runtime:command-consumer:readiness".to_string(),
            consumer_group: "stage7a-paper-command-consumer-v1".to_string(),
            consumer_name: format!("stage7a-{suffix}"),
            group_start: Stage7aGroupStart::Tail,
            allow_controlled_beginning: false,
            read_count: 32,
            claim_count: 32,
            block_ms: 1_000,
            claim_idle_ms: 30_000,
            max_claim_pages: 128,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn paper_default_auto() -> Result<Self, Stage7aBridgeError> {
        let generation = CONSUMER_INSTANCE_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self::paper_default(&format!("pid{}-gen{generation}", std::process::id()))
    }

    pub fn validate(&self) -> Result<(), Stage7aBridgeError> {
        for stream in [
            &self.command_stream,
            &self.ack_stream,
            &self.dlq_stream,
            &self.health_stream,
            &self.readiness_stream,
        ] {
            if !stream.starts_with(STAGE7A_PAPER_NAMESPACE) || stream.contains(char::is_whitespace)
            {
                return Err(Stage7aBridgeError::NonPaperNamespace);
            }
        }
        if self.command_stream == self.ack_stream
            || self.command_stream == self.dlq_stream
            || self.ack_stream == self.dlq_stream
        {
            return Err(Stage7aBridgeError::StreamAlias);
        }
        canonical_token(&self.consumer_group)?;
        canonical_token(&self.consumer_name)?;
        if self.read_count == 0
            || self.claim_count == 0
            || self.claim_idle_ms == 0
            || self.max_claim_pages == 0
        {
            return Err(Stage7aBridgeError::InvalidBound);
        }
        if self.group_start == Stage7aGroupStart::Beginning && !self.allow_controlled_beginning {
            return Err(Stage7aBridgeError::BeginningNotAuthorized);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage7aDlqReason {
    MissingPayload,
    InvalidJson,
    UnsupportedSchemaVersion,
    MessageTypeMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage7aRedactedDlqRecord {
    pub schema_version: u16,
    pub redis_entry_id: String,
    pub reason: Stage7aDlqReason,
    pub payload_len: usize,
    pub payload_sha256: String,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage7aPendingDecision {
    pub strategy_request_id: StrategyRequestId,
    pub reason: Stage7aPaperHoldReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage7aReadinessPhase {
    PaperReady,
    Degraded,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage7aReadinessReason {
    ConsumerNotAlive,
    RedisUnavailable,
    SettlementUnavailable,
    PollStale,
    CommandLifecycleBlocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage7aConsumerHealthSnapshot {
    pub command_consumer_alive: bool,
    pub source_read_healthy: bool,
    pub claim_scan_healthy: bool,
    pub ack_settlement_healthy: bool,
    pub dlq_settlement_healthy: bool,
    pub stage6_authority_healthy: bool,
    pub redis_healthy: bool,
    pub settlement_healthy: bool,
    pub last_successful_poll_at: Option<DateTime<Utc>>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage7aConsumerReadinessSnapshot {
    pub phase: Stage7aReadinessPhase,
    pub reasons: Vec<Stage7aReadinessReason>,
    pub blocked_request_id: Option<StrategyRequestId>,
    pub blocked_entry_ids: Vec<String>,
    pub blocked_request_ids: Vec<StrategyRequestId>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage7aBlockedKind {
    Authority,
    AckSettlement,
    DlqSettlement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stage7aBlockedEntry {
    request_id: Option<StrategyRequestId>,
    kind: Stage7aBlockedKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stage7aRetryPolicy {
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Stage7aRetryPolicy {
    pub fn validate(self) -> Result<Self, Stage7aBridgeError> {
        if self.initial_delay_ms == 0 || self.max_delay_ms < self.initial_delay_ms {
            Err(Stage7aBridgeError::InvalidBound)
        } else {
            Ok(self)
        }
    }

    pub fn delay_for_failure(self, consecutive_failures: u32) -> std::time::Duration {
        let shift = consecutive_failures.saturating_sub(1).min(20);
        let multiplier = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
        std::time::Duration::from_millis(
            self.initial_delay_ms
                .saturating_mul(multiplier)
                .min(self.max_delay_ms),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage7aBoundedRunSummary {
    pub iterations: usize,
    pub successful_iterations: usize,
    pub failed_iterations: usize,
    pub new_entries_examined: usize,
    pub reclaimed_entries_examined: usize,
}

#[derive(Debug, Default)]
pub struct Stage7aConsumerSupervisor {
    alive: bool,
    source_read_healthy: bool,
    claim_scan_healthy: bool,
    last_successful_poll_at: Option<DateTime<Utc>>,
    blocked_entries: BTreeMap<String, Stage7aBlockedEntry>,
}

impl Stage7aConsumerSupervisor {
    pub fn mark_started(&mut self) {
        self.alive = true;
    }

    pub fn mark_group_attached(&mut self) {
        self.alive = true;
        self.source_read_healthy = true;
        self.claim_scan_healthy = true;
    }

    pub fn mark_source_poll_success(&mut self, observed_at: DateTime<Utc>) {
        self.alive = true;
        self.source_read_healthy = true;
        self.last_successful_poll_at = Some(observed_at);
    }

    pub fn mark_claim_scan_success(&mut self, observed_at: DateTime<Utc>) {
        self.alive = true;
        self.claim_scan_healthy = true;
        self.last_successful_poll_at = Some(observed_at);
    }

    pub fn mark_source_failure(&mut self) {
        self.source_read_healthy = false;
    }

    pub fn mark_claim_failure(&mut self) {
        self.claim_scan_healthy = false;
    }

    fn mark_blocked(
        &mut self,
        entry_id: impl Into<String>,
        request_id: Option<StrategyRequestId>,
        kind: Stage7aBlockedKind,
    ) {
        self.blocked_entries
            .insert(entry_id.into(), Stage7aBlockedEntry { request_id, kind });
    }

    fn clear_blocked_entry(&mut self, entry_id: &str) {
        self.blocked_entries.remove(entry_id);
    }

    pub fn mark_stopped(&mut self) {
        self.alive = false;
    }

    pub fn snapshots(
        &self,
        checked_at: DateTime<Utc>,
        freshness: chrono::Duration,
    ) -> (
        Stage7aConsumerHealthSnapshot,
        Stage7aConsumerReadinessSnapshot,
    ) {
        let ack_settlement_healthy = !self
            .blocked_entries
            .values()
            .any(|entry| entry.kind == Stage7aBlockedKind::AckSettlement);
        let dlq_settlement_healthy = !self
            .blocked_entries
            .values()
            .any(|entry| entry.kind == Stage7aBlockedKind::DlqSettlement);
        let stage6_authority_healthy = !self
            .blocked_entries
            .values()
            .any(|entry| entry.kind == Stage7aBlockedKind::Authority);
        let redis_healthy = self.source_read_healthy && self.claim_scan_healthy;
        let settlement_healthy =
            ack_settlement_healthy && dlq_settlement_healthy && stage6_authority_healthy;
        let mut reasons = Vec::new();
        if !self.alive {
            reasons.push(Stage7aReadinessReason::ConsumerNotAlive);
        }
        if !redis_healthy {
            reasons.push(Stage7aReadinessReason::RedisUnavailable);
        }
        if !settlement_healthy {
            reasons.push(Stage7aReadinessReason::SettlementUnavailable);
        }
        if self
            .last_successful_poll_at
            .map_or(true, |last| checked_at - last > freshness)
        {
            reasons.push(Stage7aReadinessReason::PollStale);
        }
        if !self.blocked_entries.is_empty() {
            reasons.push(Stage7aReadinessReason::CommandLifecycleBlocked);
        }
        let phase = if !self.alive {
            Stage7aReadinessPhase::Stopped
        } else if reasons.is_empty() {
            Stage7aReadinessPhase::PaperReady
        } else {
            Stage7aReadinessPhase::Degraded
        };
        (
            Stage7aConsumerHealthSnapshot {
                command_consumer_alive: self.alive,
                source_read_healthy: self.source_read_healthy,
                claim_scan_healthy: self.claim_scan_healthy,
                ack_settlement_healthy,
                dlq_settlement_healthy,
                stage6_authority_healthy,
                redis_healthy,
                settlement_healthy,
                last_successful_poll_at: self.last_successful_poll_at,
                checked_at,
            },
            Stage7aConsumerReadinessSnapshot {
                phase,
                reasons,
                blocked_request_id: self
                    .blocked_entries
                    .values()
                    .find_map(|entry| entry.request_id),
                blocked_entry_ids: self.blocked_entries.keys().cloned().collect(),
                blocked_request_ids: self
                    .blocked_entries
                    .values()
                    .filter_map(|entry| entry.request_id)
                    .collect(),
                checked_at,
            },
        )
    }
}

pub enum Stage7aHandleOutcome {
    Ack(Envelope<CommandAck>),
    Dlq(Stage7aRedactedDlqRecord),
    Pending(Stage7aPendingDecision),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage7aFaultPoint {
    None,
    BeforePaperOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage7aSettlementFault {
    None,
    AfterAckPublishBeforeXack,
    AfterDlqPublishBeforeXack,
}

pub trait Stage7aPaperOutcomeProvider {
    fn paper_outcome(
        &mut self,
        command: &BrokerCommand,
        observed_at: DateTime<Utc>,
    ) -> Result<Stage6dPaperOutcome, Stage7aPaperProviderError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage7aCommandProfile {
    account_id: BrokerAccountId,
    instrument: InstrumentId,
    strategy_id: String,
}

impl Stage7aCommandProfile {
    pub fn new(
        account_id: BrokerAccountId,
        instrument: InstrumentId,
        strategy_id: impl Into<String>,
    ) -> Result<Self, Stage7aBridgeError> {
        let strategy_id = canonical_token(&strategy_id.into())?.to_string();
        Ok(Self {
            account_id,
            instrument,
            strategy_id,
        })
    }

    fn context_for(
        &self,
        command: &BrokerCommand,
        recovered: &Stage6dDurableRuntimeRecovered,
    ) -> Result<Stage7aPaperCommandContext, Stage7aBridgeError> {
        match command {
            BrokerCommand::PlaceOrder(place) => {
                if place.account_id != self.account_id || place.instrument != self.instrument {
                    return Err(Stage7aBridgeError::CommandProfileMismatch);
                }
                let comment = place
                    .comment
                    .as_deref()
                    .ok_or(Stage7aBridgeError::CommandProfileMismatch)?;
                let attribution = HybridRuntimeAttribution::parse_source_comment(comment)
                    .map_err(|_| Stage7aBridgeError::CommandProfileMismatch)?;
                if !attribution.belongs_to(&self.strategy_id)
                    || attribution.role() == Some(broker_core::HybridRuntimeOrderRole::Cancel)
                {
                    return Err(Stage7aBridgeError::CommandProfileMismatch);
                }
                Ok(Stage7aPaperCommandContext::new(
                    self.instrument.clone(),
                    attribution,
                ))
            }
            BrokerCommand::CancelOrder(cancel) => {
                if cancel.account_id != self.account_id {
                    return Err(Stage7aBridgeError::CommandProfileMismatch);
                }
                resolve_stage7a_cancel_command_context(
                    recovered,
                    cancel,
                    &self.instrument,
                    &self.strategy_id,
                )
                .ok_or(Stage7aBridgeError::CommandProfileMismatch)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage7aPaperProviderError {
    #[error("paper outcome is uncertain")]
    Uncertain,
}

#[derive(Clone)]
struct Stage7aAckPublication {
    command_sha256: String,
    canonical_ack: Envelope<CommandAck>,
    published: bool,
}

pub struct Stage7aCommandAuthority<P> {
    recovered: Stage6dDurableRuntimeRecovered,
    profile: Stage7aCommandProfile,
    provider: P,
    source: String,
    fault_point: Stage7aFaultPoint,
    ack_publications: HashMap<StrategyRequestId, Stage7aAckPublication>,
}

impl<P: Stage7aPaperOutcomeProvider> Stage7aCommandAuthority<P> {
    pub fn new(
        recovered: Stage6dDurableRuntimeRecovered,
        profile: Stage7aCommandProfile,
        provider: P,
        source: impl Into<String>,
    ) -> Result<Self, Stage7aBridgeError> {
        let source = canonical_token(&source.into())?.to_string();
        Ok(Self {
            recovered,
            profile,
            provider,
            source,
            fault_point: Stage7aFaultPoint::None,
            ack_publications: HashMap::new(),
        })
    }

    pub fn set_fault_point(&mut self, fault_point: Stage7aFaultPoint) {
        self.fault_point = fault_point;
    }

    pub fn recovered(&self) -> &Stage6dDurableRuntimeRecovered {
        &self.recovered
    }

    fn mark_ack_published(&mut self, ack: &Envelope<CommandAck>) {
        if let Some(publication) = self.ack_publications.get_mut(&ack.payload.request_id) {
            publication.published = true;
        }
    }

    fn remember_canonical_ack(
        &mut self,
        command_sha256: String,
        ack: Envelope<CommandAck>,
    ) -> Envelope<CommandAck> {
        self.ack_publications.insert(
            ack.payload.request_id,
            Stage7aAckPublication {
                command_sha256,
                canonical_ack: ack.clone(),
                published: false,
            },
        );
        ack
    }

    pub fn handle_payload_now(
        &mut self,
        redis_entry_id: &str,
        raw_payload: &[u8],
    ) -> Result<Stage7aHandleOutcome, Stage7aBridgeError> {
        self.handle_payload(redis_entry_id, raw_payload, Utc::now())
    }

    fn handle_payload(
        &mut self,
        redis_entry_id: &str,
        raw_payload: &[u8],
        observed_at: DateTime<Utc>,
    ) -> Result<Stage7aHandleOutcome, Stage7aBridgeError> {
        let envelope = match serde_json::from_slice::<Envelope<BrokerCommand>>(raw_payload) {
            Ok(envelope) => envelope,
            Err(_) => {
                return Ok(Stage7aHandleOutcome::Dlq(redacted_dlq(
                    redis_entry_id,
                    raw_payload,
                    Stage7aDlqReason::InvalidJson,
                    observed_at,
                )))
            }
        };
        if envelope.schema_version != SCHEMA_VERSION {
            return Ok(Stage7aHandleOutcome::Dlq(redacted_dlq(
                redis_entry_id,
                raw_payload,
                Stage7aDlqReason::UnsupportedSchemaVersion,
                observed_at,
            )));
        }
        if envelope.msg_type != MessageType::Command {
            return Ok(Stage7aHandleOutcome::Dlq(redacted_dlq(
                redis_entry_id,
                raw_payload,
                Stage7aDlqReason::MessageTypeMismatch,
                observed_at,
            )));
        }
        let command = envelope.payload;
        let command_sha256 = command_sha256(&command)?;
        let request_id = command_request_id(&command);
        if let Some(publication) = self.ack_publications.get(&request_id) {
            if publication.command_sha256 != command_sha256 {
                return Ok(Stage7aHandleOutcome::Pending(Stage7aPendingDecision {
                    strategy_request_id: request_id,
                    reason: Stage7aPaperHoldReason::IdentityConflict,
                }));
            }
            let ack = if publication.published {
                duplicate_ack_envelope(&self.source, &publication.canonical_ack, observed_at)
            } else {
                publication.canonical_ack.clone()
            };
            return Ok(Stage7aHandleOutcome::Ack(ack));
        }
        let context = match self.profile.context_for(&command, &self.recovered) {
            Ok(context) => context,
            Err(Stage7aBridgeError::CommandProfileMismatch) => {
                let decision = Stage7aPaperAdmissionDecision {
                    strategy_request_id: request_id,
                    durable_client_order_id: broker_core::ClientOrderId::from_strategy_request(
                        request_id,
                    ),
                    broker_order_id: None,
                };
                let ack = ack_envelope(
                    &self.source,
                    decision,
                    CommandAckStatus::Rejected,
                    Some(CommandAckReasonCode::LocalValidationRejected),
                    observed_at,
                );
                return Ok(Stage7aHandleOutcome::Ack(
                    self.remember_canonical_ack(command_sha256, ack),
                ));
            }
            Err(error) => return Err(error),
        };
        let admission =
            admit_stage7a_paper_command(&mut self.recovered, &command, &context, observed_at)?;
        match admission {
            Stage7aPaperAdmission::DispatchReady(receipt) => {
                if self.fault_point == Stage7aFaultPoint::BeforePaperOutcome {
                    return Ok(Stage7aHandleOutcome::Pending(Stage7aPendingDecision {
                        strategy_request_id: command_request_id(&command),
                        reason: Stage7aPaperHoldReason::ReconciliationRequired,
                    }));
                }
                let outcome = match self.provider.paper_outcome(&command, observed_at) {
                    Ok(outcome) => outcome,
                    Err(Stage7aPaperProviderError::Uncertain) => {
                        return Ok(Stage7aHandleOutcome::Pending(Stage7aPendingDecision {
                            strategy_request_id: command_request_id(&command),
                            reason: Stage7aPaperHoldReason::ReconciliationRequired,
                        }))
                    }
                };
                let report = execute_stage6d_paper_outcome(&mut self.recovered, *receipt, outcome)?;
                if report.dispatch_safety_state
                    != strategy_runtime_core::Stage6DispatchSafetyStateV1::DispatchForbidden
                {
                    return Ok(Stage7aHandleOutcome::Pending(Stage7aPendingDecision {
                        strategy_request_id: report.strategy_request_id,
                        reason: Stage7aPaperHoldReason::ReconciliationRequired,
                    }));
                }
                let ack = ack_envelope(
                    &self.source,
                    Stage7aPaperAdmissionDecision {
                        strategy_request_id: report.strategy_request_id,
                        durable_client_order_id: report.durable_client_order_id,
                        broker_order_id: report.broker_order_id,
                    },
                    CommandAckStatus::Accepted,
                    None,
                    observed_at,
                );
                Ok(Stage7aHandleOutcome::Ack(
                    self.remember_canonical_ack(command_sha256, ack),
                ))
            }
            Stage7aPaperAdmission::Duplicate(decision) => {
                Ok(Stage7aHandleOutcome::Ack(ack_envelope(
                    &self.source,
                    decision,
                    CommandAckStatus::Duplicate,
                    Some(CommandAckReasonCode::DuplicateCommand),
                    observed_at,
                )))
            }
            Stage7aPaperAdmission::PolicyRejected { decision, reason } => {
                let (status, code) = match reason {
                    Stage7aPaperPolicyRejection::Expired => (
                        CommandAckStatus::Expired,
                        CommandAckReasonCode::ExpiredCommand,
                    ),
                    Stage7aPaperPolicyRejection::UnsupportedCommandShape => (
                        CommandAckStatus::Rejected,
                        CommandAckReasonCode::FeatureDisabled,
                    ),
                };
                let ack = ack_envelope(&self.source, decision, status, Some(code), observed_at);
                Ok(Stage7aHandleOutcome::Ack(
                    self.remember_canonical_ack(command_sha256, ack),
                ))
            }
            Stage7aPaperAdmission::Hold { decision, reason } => {
                Ok(Stage7aHandleOutcome::Pending(Stage7aPendingDecision {
                    strategy_request_id: decision.strategy_request_id,
                    reason,
                }))
            }
        }
    }
}

pub struct Stage7aRedisConsumer<P> {
    connection: ConnectionManager,
    config: Stage7aRedisConfig,
    authority: Stage7aCommandAuthority<P>,
    supervisor: Stage7aConsumerSupervisor,
    settlement_fault: Stage7aSettlementFault,
    claim_cursor: String,
}

impl<P: Stage7aPaperOutcomeProvider> Stage7aRedisConsumer<P> {
    pub async fn connect(
        redis_url: &str,
        config: Stage7aRedisConfig,
        authority: Stage7aCommandAuthority<P>,
    ) -> Result<Self, Stage7aBridgeError> {
        config.validate()?;
        let client = redis::Client::open(redis_url)?;
        let connection = ConnectionManager::new(client).await?;
        Ok(Self {
            connection,
            config,
            authority,
            supervisor: Stage7aConsumerSupervisor::default(),
            settlement_fault: Stage7aSettlementFault::None,
            claim_cursor: "0-0".to_string(),
        })
    }

    pub fn set_settlement_fault(&mut self, fault: Stage7aSettlementFault) {
        self.settlement_fault = fault;
    }

    pub fn supervisor(&self) -> &Stage7aConsumerSupervisor {
        &self.supervisor
    }

    pub fn mark_stopped(&mut self) {
        self.supervisor.mark_stopped();
    }

    pub async fn run_bounded(
        &mut self,
        max_iterations: usize,
        retry_policy: Stage7aRetryPolicy,
    ) -> Result<Stage7aBoundedRunSummary, Stage7aBridgeError> {
        if max_iterations == 0 {
            return Err(Stage7aBridgeError::InvalidBound);
        }
        let retry_policy = retry_policy.validate()?;
        self.ensure_group().await?;
        let mut summary = Stage7aBoundedRunSummary {
            iterations: 0,
            successful_iterations: 0,
            failed_iterations: 0,
            new_entries_examined: 0,
            reclaimed_entries_examined: 0,
        };
        let mut consecutive_failures = 0u32;
        for _ in 0..max_iterations {
            summary.iterations += 1;
            let reclaimed = self.reclaim_stale_once().await;
            let fresh = match reclaimed {
                Ok(reclaimed) => {
                    summary.reclaimed_entries_examined =
                        summary.reclaimed_entries_examined.saturating_add(reclaimed);
                    self.poll_new_once().await
                }
                Err(error) => Err(error),
            };
            match fresh {
                Ok(fresh) => {
                    summary.new_entries_examined =
                        summary.new_entries_examined.saturating_add(fresh);
                    summary.successful_iterations += 1;
                    consecutive_failures = 0;
                }
                Err(_) => {
                    summary.failed_iterations += 1;
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    tokio::time::sleep(retry_policy.delay_for_failure(consecutive_failures)).await;
                }
            }
        }
        self.supervisor.mark_stopped();
        Ok(summary)
    }

    pub async fn ensure_group(&mut self) -> Result<(), Stage7aBridgeError> {
        let result: redis::RedisResult<()> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&self.config.command_stream)
            .arg(&self.config.consumer_group)
            .arg(self.config.group_start.redis_id())
            .arg("MKSTREAM")
            .query_async(&mut self.connection)
            .await;
        match result {
            Ok(()) => {
                self.supervisor.mark_group_attached();
                Ok(())
            }
            Err(error) if error.to_string().contains("BUSYGROUP") => {
                self.supervisor.mark_group_attached();
                Ok(())
            }
            Err(error) => {
                self.supervisor.mark_source_failure();
                self.supervisor.mark_claim_failure();
                Err(error.into())
            }
        }
    }

    pub async fn poll_new_once(&mut self) -> Result<usize, Stage7aBridgeError> {
        let result = self.poll_new_once_inner().await;
        if result.is_ok() {
            self.supervisor.mark_source_poll_success(Utc::now());
        }
        result
    }

    async fn poll_new_once_inner(&mut self) -> Result<usize, Stage7aBridgeError> {
        let mut command = redis::cmd("XREADGROUP");
        command
            .arg("GROUP")
            .arg(&self.config.consumer_group)
            .arg(&self.config.consumer_name)
            .arg("COUNT")
            .arg(self.config.read_count);
        if self.config.block_ms > 0 {
            command.arg("BLOCK").arg(self.config.block_ms);
        }
        let reply: StreamReadReply = match command
            .arg("STREAMS")
            .arg(&self.config.command_stream)
            .arg(">")
            .query_async(&mut self.connection)
            .await
        {
            Ok(reply) => reply,
            Err(error) => {
                self.supervisor.mark_source_failure();
                return Err(error.into());
            }
        };
        let entries = reply
            .keys
            .into_iter()
            .flat_map(|key| key.ids)
            .collect::<Vec<_>>();
        let count = entries.len();
        for entry in entries {
            self.settle_entry(entry, Utc::now()).await?;
        }
        Ok(count)
    }

    pub async fn reclaim_stale_once(&mut self) -> Result<usize, Stage7aBridgeError> {
        let result = self.reclaim_stale_once_inner().await;
        if result.is_ok() {
            self.supervisor.mark_claim_scan_success(Utc::now());
        }
        result
    }

    async fn reclaim_stale_once_inner(&mut self) -> Result<usize, Stage7aBridgeError> {
        let mut examined = 0usize;
        for _ in 0..self.config.max_claim_pages {
            let start = self.claim_cursor.clone();
            let reply: StreamAutoClaimReply = match redis::cmd("XAUTOCLAIM")
                .arg(&self.config.command_stream)
                .arg(&self.config.consumer_group)
                .arg(&self.config.consumer_name)
                .arg(self.config.claim_idle_ms)
                .arg(&start)
                .arg("COUNT")
                .arg(self.config.claim_count)
                .query_async(&mut self.connection)
                .await
            {
                Ok(reply) => reply,
                Err(error) => {
                    self.supervisor.mark_claim_failure();
                    return Err(error.into());
                }
            };
            let next = reply.next_stream_id;
            self.claim_cursor = next.clone();
            examined = examined.saturating_add(reply.claimed.len());
            for entry in reply.claimed {
                self.settle_entry(entry, Utc::now()).await?;
            }
            if xautoclaim_cursor_done(&start, &next) {
                self.claim_cursor = "0-0".to_string();
                return Ok(examined);
            }
        }
        Ok(examined)
    }

    pub async fn publish_observability(
        &mut self,
        checked_at: DateTime<Utc>,
        freshness: chrono::Duration,
    ) -> Result<
        (
            Stage7aConsumerHealthSnapshot,
            Stage7aConsumerReadinessSnapshot,
        ),
        Stage7aBridgeError,
    > {
        let (health, readiness) = self.supervisor.snapshots(checked_at, freshness);
        let health_envelope = Envelope {
            schema_version: SCHEMA_VERSION,
            ts_utc: checked_at,
            source: self.authority.source.clone(),
            msg_type: MessageType::Health,
            payload: &health,
        };
        let readiness_envelope = Envelope {
            schema_version: SCHEMA_VERSION,
            ts_utc: checked_at,
            source: self.authority.source.clone(),
            msg_type: MessageType::Readiness,
            payload: &readiness,
        };
        let health_payload = serde_json::to_string(&health_envelope)?;
        let readiness_payload = serde_json::to_string(&readiness_envelope)?;
        if let Err(error) = self
            .publish(&self.config.health_stream.clone(), &health_payload)
            .await
        {
            self.supervisor.mark_source_failure();
            return Err(error);
        }
        if let Err(error) = self
            .publish(&self.config.readiness_stream.clone(), &readiness_payload)
            .await
        {
            self.supervisor.mark_source_failure();
            return Err(error);
        }
        Ok((health, readiness))
    }

    async fn settle_entry(
        &mut self,
        entry: StreamId,
        observed_at: DateTime<Utc>,
    ) -> Result<(), Stage7aBridgeError> {
        let outcome = match entry.get::<String>("payload") {
            Some(payload) => {
                match self
                    .authority
                    .handle_payload(&entry.id, payload.as_bytes(), observed_at)
                {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        let request_id = serde_json::from_str::<Envelope<BrokerCommand>>(&payload)
                            .ok()
                            .map(|envelope| command_request_id(&envelope.payload));
                        self.supervisor.mark_blocked(
                            entry.id.clone(),
                            request_id,
                            Stage7aBlockedKind::Authority,
                        );
                        return Err(error);
                    }
                }
            }
            None => Stage7aHandleOutcome::Dlq(redacted_dlq(
                &entry.id,
                &[],
                Stage7aDlqReason::MissingPayload,
                observed_at,
            )),
        };
        match outcome {
            Stage7aHandleOutcome::Ack(ack) => {
                let payload = serde_json::to_string(&ack)?;
                if let Err(error) = self
                    .publish(&self.config.ack_stream.clone(), &payload)
                    .await
                {
                    self.supervisor.mark_blocked(
                        entry.id.clone(),
                        Some(ack.payload.request_id),
                        Stage7aBlockedKind::AckSettlement,
                    );
                    return Err(error);
                }
                self.authority.mark_ack_published(&ack);
                if self.settlement_fault == Stage7aSettlementFault::AfterAckPublishBeforeXack {
                    self.settlement_fault = Stage7aSettlementFault::None;
                    self.supervisor.mark_blocked(
                        entry.id.clone(),
                        Some(ack.payload.request_id),
                        Stage7aBlockedKind::AckSettlement,
                    );
                    return Err(Stage7aBridgeError::InjectedSettlementFault);
                }
                if let Err(error) = self.xack(&entry.id).await {
                    self.supervisor.mark_blocked(
                        entry.id.clone(),
                        Some(ack.payload.request_id),
                        Stage7aBlockedKind::AckSettlement,
                    );
                    return Err(error);
                }
                self.supervisor.clear_blocked_entry(&entry.id);
            }
            Stage7aHandleOutcome::Dlq(dlq) => {
                let payload = serde_json::to_string(&dlq)?;
                if let Err(error) = self
                    .publish(&self.config.dlq_stream.clone(), &payload)
                    .await
                {
                    self.supervisor.mark_blocked(
                        entry.id.clone(),
                        None,
                        Stage7aBlockedKind::DlqSettlement,
                    );
                    return Err(error);
                }
                if self.settlement_fault == Stage7aSettlementFault::AfterDlqPublishBeforeXack {
                    self.settlement_fault = Stage7aSettlementFault::None;
                    self.supervisor.mark_blocked(
                        entry.id.clone(),
                        None,
                        Stage7aBlockedKind::DlqSettlement,
                    );
                    return Err(Stage7aBridgeError::InjectedSettlementFault);
                }
                if let Err(error) = self.xack(&entry.id).await {
                    self.supervisor.mark_blocked(
                        entry.id.clone(),
                        None,
                        Stage7aBlockedKind::DlqSettlement,
                    );
                    return Err(error);
                }
                self.supervisor.clear_blocked_entry(&entry.id);
            }
            Stage7aHandleOutcome::Pending(pending) => {
                self.supervisor.mark_blocked(
                    entry.id,
                    Some(pending.strategy_request_id),
                    Stage7aBlockedKind::Authority,
                );
            }
        }
        Ok(())
    }

    async fn publish(&mut self, stream: &str, payload: &str) -> Result<(), Stage7aBridgeError> {
        let _: String = redis::cmd("XADD")
            .arg(stream)
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async(&mut self.connection)
            .await?;
        Ok(())
    }

    async fn xack(&mut self, entry_id: &str) -> Result<(), Stage7aBridgeError> {
        let _: i64 = redis::cmd("XACK")
            .arg(&self.config.command_stream)
            .arg(&self.config.consumer_group)
            .arg(entry_id)
            .query_async(&mut self.connection)
            .await?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Stage7aBridgeError {
    #[error("Stage 7A stream is outside the paper namespace")]
    NonPaperNamespace,
    #[error("Stage 7A stream roles alias")]
    StreamAlias,
    #[error("invalid Stage 7A token")]
    InvalidToken,
    #[error("invalid Stage 7A processing bound")]
    InvalidBound,
    #[error("beginning-of-stream replay is not explicitly authorized")]
    BeginningNotAuthorized,
    #[error("invalid Stage 7A command profile")]
    InvalidCommandProfile,
    #[error("command does not match the trusted Stage 7A profile")]
    CommandProfileMismatch,
    #[error("Stage 6 authority rejected command processing")]
    Stage6(#[from] Stage6dLiveCoreError),
    #[error("Redis command failed")]
    Redis(#[from] redis::RedisError),
    #[error("JSON encoding failed")]
    Json(#[from] serde_json::Error),
    #[error("injected Stage 7A settlement fault")]
    InjectedSettlementFault,
}

fn command_request_id(command: &BrokerCommand) -> StrategyRequestId {
    match command {
        BrokerCommand::PlaceOrder(command) => command.request_id,
        BrokerCommand::CancelOrder(command) => command.request_id,
    }
}

fn command_sha256(command: &BrokerCommand) -> Result<String, Stage7aBridgeError> {
    let bytes = serde_json::to_vec(command)?;
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage7a.command-publication.v1");
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn ack_envelope(
    source: &str,
    decision: Stage7aPaperAdmissionDecision,
    status: CommandAckStatus,
    reason: Option<CommandAckReasonCode>,
    received_ts: DateTime<Utc>,
) -> Envelope<CommandAck> {
    Envelope {
        schema_version: SCHEMA_VERSION,
        ts_utc: received_ts,
        source: source.to_string(),
        msg_type: MessageType::CommandAck,
        payload: CommandAck {
            request_id: decision.strategy_request_id,
            client_order_id: Some(decision.durable_client_order_id),
            broker_order_id: decision.broker_order_id,
            status,
            reason: reason.map(CommandAckReason::new),
            received_ts,
        },
    }
}

fn duplicate_ack_envelope(
    source: &str,
    canonical: &Envelope<CommandAck>,
    received_ts: DateTime<Utc>,
) -> Envelope<CommandAck> {
    Envelope {
        schema_version: SCHEMA_VERSION,
        ts_utc: received_ts,
        source: source.to_string(),
        msg_type: MessageType::CommandAck,
        payload: CommandAck {
            request_id: canonical.payload.request_id,
            client_order_id: canonical.payload.client_order_id.clone(),
            broker_order_id: canonical.payload.broker_order_id.clone(),
            status: CommandAckStatus::Duplicate,
            reason: Some(CommandAckReason::new(
                CommandAckReasonCode::DuplicateCommand,
            )),
            received_ts,
        },
    }
}

fn redacted_dlq(
    entry_id: &str,
    payload: &[u8],
    reason: Stage7aDlqReason,
    observed_at: DateTime<Utc>,
) -> Stage7aRedactedDlqRecord {
    let mut hasher = Sha256::new();
    hasher.update(DLQ_DOMAIN);
    hasher.update(payload);
    Stage7aRedactedDlqRecord {
        schema_version: 1,
        redis_entry_id: entry_id.to_string(),
        reason,
        payload_len: payload.len(),
        payload_sha256: format!("{:x}", hasher.finalize()),
        observed_at,
    }
}

fn xautoclaim_cursor_done(start: &str, next: &str) -> bool {
    next == "0-0" || next == start
}

fn canonical_token(value: &str) -> Result<&str, Stage7aBridgeError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        Err(Stage7aBridgeError::InvalidToken)
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{
        BrokerAccountId, BrokerOrderId, BrokerTradeId, CancelOrder, ClientOrderId, Exchange,
        HybridRuntimeAttribution, InstrumentId, Market, OrderSide, OrderType, PlaceOrder,
        TimeInForce,
    };
    use chrono::TimeZone;
    use redis::streams::{StreamPendingCountReply, StreamRangeReply};
    use rust_decimal::Decimal;
    use std::net::TcpListener;
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;
    use strategy_runtime_core::hybrid_intraday::{
        HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
    };
    use strategy_runtime_core::{
        authorize_stage6d_first_boot, first_boot_stage6d_paper, BrokerNeutralMarketOrderStyle,
        HybridIntradayProfile, HybridIntradayRuntimeConfig, HybridIntradayRuntimeStrategy,
        MeanReversionVariant, MrGatePolicy, RiskGateMode, Stage6dFirstBootConfig,
    };
    use uuid::Uuid;

    struct DeterministicPaperProvider;
    struct UncertainPaperProvider;

    impl Stage7aPaperOutcomeProvider for UncertainPaperProvider {
        fn paper_outcome(
            &mut self,
            _command: &BrokerCommand,
            _observed_at: DateTime<Utc>,
        ) -> Result<Stage6dPaperOutcome, Stage7aPaperProviderError> {
            Err(Stage7aPaperProviderError::Uncertain)
        }
    }

    impl Stage7aPaperOutcomeProvider for DeterministicPaperProvider {
        fn paper_outcome(
            &mut self,
            command: &BrokerCommand,
            _observed_at: DateTime<Utc>,
        ) -> Result<Stage6dPaperOutcome, Stage7aPaperProviderError> {
            let request_id = command_request_id(command);
            let order_id = BrokerOrderId::new(format!("PAPER-{request_id}"));
            Ok(match command {
                BrokerCommand::PlaceOrder(place) if place.order_type == OrderType::Market => {
                    Stage6dPaperOutcome::MarketFilled {
                        broker_order_id: order_id,
                        broker_trade_id: BrokerTradeId::new(format!("PAPER-TRADE-{request_id}")),
                    }
                }
                BrokerCommand::PlaceOrder(_) => Stage6dPaperOutcome::LimitPending {
                    broker_order_id: order_id,
                },
                BrokerCommand::CancelOrder(_) => Stage6dPaperOutcome::CancelCanceled,
            })
        }
    }

    fn runtime() -> HybridIntradayRuntimeStrategy {
        HybridIntradayRuntimeStrategy::new(HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_string(),
            profile: HybridIntradayProfile::BaselineRuntimeHybrid,
            mr_variant: MeanReversionVariant::ClassicPrevDayRange,
            mr_gate_policy: MrGatePolicy::Disabled,
            risk_gate_mode: RiskGateMode::Disabled,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: None,
            model_session_end_time: None,
            qty: 1.0,
            live_order_style: BrokerNeutralMarketOrderStyle::Market,
            tick_size: 0.5,
            marketable_limit_offset_ticks: 0,
            timezone_offset_hours: 3,
            session_close_hour: 23,
            session_close_minute: 49,
            weekends_off: true,
            stop_end_buffer_sec: 60,
            repair_deadline_sec: 180,
            sl_escalate_timeout_sec: 30,
            max_repair_retries: 3,
            repair_backoff_base_sec: 5,
            repair_backoff_max_sec: 60,
            pending_timeout_sec: 30,
            partial_entry_fill_timeout_ms: 3_000,
            mr_config: MeanReversionConfig::default(),
            breakout_config: IntradayBreakoutConfig::default(),
            orchestrator_config: HybridOrchestratorConfig::default(),
        })
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn attribution() -> HybridRuntimeAttribution {
        HybridRuntimeAttribution::parse_source_comment(
            "HYB|sid=hybrid_imoexf|c=cycle-stage7a|o=BO|r=ENTRY",
        )
        .unwrap()
    }

    fn recovered() -> Stage6dDurableRuntimeRecovered {
        let runtime = runtime();
        let authorization = authorize_stage6d_first_boot(Stage6dFirstBootConfig {
            deployment_id: "stage7a-paper-test".to_string(),
            expected_runtime_config_fingerprint_sha256: runtime.stage5c_config_fingerprint(),
            allow_create_missing_journal: true,
        })
        .unwrap();
        first_boot_stage6d_paper(authorization, runtime).unwrap()
    }

    fn profile() -> Stage7aCommandProfile {
        Stage7aCommandProfile::new(
            BrokerAccountId::new("ACC_TEST_0001"),
            instrument(),
            "hybrid_imoexf",
        )
        .unwrap()
    }

    fn authority() -> Stage7aCommandAuthority<DeterministicPaperProvider> {
        Stage7aCommandAuthority::new(
            recovered(),
            profile(),
            DeterministicPaperProvider,
            "stage7a-paper-test",
        )
        .unwrap()
    }

    fn command(number: u128) -> BrokerCommand {
        let request_id = StrategyRequestId::from(Uuid::from_u128(number));
        BrokerCommand::PlaceOrder(PlaceOrder {
            request_id,
            created_ts: Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 0).unwrap(),
            ttl_ms: None,
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::from_strategy_request(request_id),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::ONE,
            limit_price: Some(Decimal::new(2210, 1)),
            time_in_force: TimeInForce::Day,
            comment: Some(attribution().internal_comment().to_string()),
        })
    }

    fn encoded_command(number: u128) -> String {
        serde_json::to_string(&Envelope {
            schema_version: SCHEMA_VERSION,
            ts_utc: Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 0).unwrap(),
            source: "stage7a-paper-test".to_string(),
            msg_type: MessageType::Command,
            payload: command(number),
        })
        .unwrap()
    }

    fn encoded_cancel(number: u128, target_request: u128) -> String {
        let request_id = StrategyRequestId::from(Uuid::from_u128(number));
        let target_request_id = StrategyRequestId::from(Uuid::from_u128(target_request));
        let command = BrokerCommand::CancelOrder(CancelOrder {
            request_id,
            created_ts: Utc.with_ymd_and_hms(2026, 8, 11, 9, 1, 0).unwrap(),
            ttl_ms: None,
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            order_id: BrokerOrderId::new(format!("PAPER-{target_request_id}")),
            client_order_id: Some(ClientOrderId::from_strategy_request(target_request_id)),
        });
        serde_json::to_string(&Envelope {
            schema_version: SCHEMA_VERSION,
            ts_utc: Utc.with_ymd_and_hms(2026, 8, 11, 9, 1, 0).unwrap(),
            source: "stage7a-paper-test".to_string(),
            msg_type: MessageType::Command,
            payload: command,
        })
        .unwrap()
    }

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
                .expect("redis-server must be installed for Stage 7A integration");
            let url = format!("redis://127.0.0.1:{port}/");
            for _ in 0..100 {
                if let Ok(client) = redis::Client::open(url.as_str()) {
                    if let Ok(mut manager) = ConnectionManager::new(client).await {
                        let ping: redis::RedisResult<String> =
                            redis::cmd("PING").query_async(&mut manager).await;
                        if ping.as_deref() == Ok("PONG") {
                            return Self { child, url };
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let _ = child.kill();
            let _ = child.wait();
            panic!("temporary Redis did not start")
        }
    }

    impl Drop for RedisServer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    fn non_paper_stream_is_rejected_before_redis() {
        let mut config = Stage7aRedisConfig::paper_default("process-1").unwrap();
        config.command_stream = "cmd.orders.live".to_string();
        assert!(matches!(
            config.validate(),
            Err(Stage7aBridgeError::NonPaperNamespace)
        ));
    }

    #[test]
    fn auto_consumer_names_are_process_unique_and_not_execution_ids() {
        let a = Stage7aRedisConfig::paper_default_auto().unwrap();
        let b = Stage7aRedisConfig::paper_default_auto().unwrap();
        assert_ne!(a.consumer_name, b.consumer_name);
        assert!(!a.consumer_name.contains("request"));
        let mut beginning = Stage7aRedisConfig::paper_default("controlled-replay").unwrap();
        beginning.group_start = Stage7aGroupStart::Beginning;
        assert!(matches!(
            beginning.validate(),
            Err(Stage7aBridgeError::BeginningNotAuthorized)
        ));
        beginning.allow_controlled_beginning = true;
        beginning.validate().unwrap();
    }

    #[test]
    fn dlq_is_redacted_and_cursor_is_bounded() {
        let raw = br#"token=SECRET&comment=raw-sensitive-body"#;
        let record = redacted_dlq("1-0", raw, Stage7aDlqReason::InvalidJson, Utc::now());
        let encoded = serde_json::to_string(&record).unwrap();
        assert!(!encoded.contains("SECRET"));
        assert!(!encoded.contains("raw-sensitive-body"));
        assert!(xautoclaim_cursor_done("7-0", "0-0"));
        assert!(xautoclaim_cursor_done("7-0", "7-0"));
        assert!(!xautoclaim_cursor_done("7-0", "8-0"));
    }

    #[test]
    fn envelope_policy_and_ttl_fail_before_paper_effect() {
        let mut authority = authority();
        let mut wrong_schema: Envelope<BrokerCommand> =
            serde_json::from_str(&encoded_command(8001)).unwrap();
        wrong_schema.schema_version = SCHEMA_VERSION + 1;
        assert!(matches!(
            authority
                .handle_payload(
                    "20-0",
                    serde_json::to_string(&wrong_schema).unwrap().as_bytes(),
                    Utc::now(),
                )
                .unwrap(),
            Stage7aHandleOutcome::Dlq(Stage7aRedactedDlqRecord {
                reason: Stage7aDlqReason::UnsupportedSchemaVersion,
                ..
            })
        ));
        let mut wrong_type: Envelope<BrokerCommand> =
            serde_json::from_str(&encoded_command(8002)).unwrap();
        wrong_type.msg_type = MessageType::MarketData;
        assert!(matches!(
            authority
                .handle_payload(
                    "21-0",
                    serde_json::to_string(&wrong_type).unwrap().as_bytes(),
                    Utc::now(),
                )
                .unwrap(),
            Stage7aHandleOutcome::Dlq(Stage7aRedactedDlqRecord {
                reason: Stage7aDlqReason::MessageTypeMismatch,
                ..
            })
        ));

        let mut expired = match command(8003) {
            BrokerCommand::PlaceOrder(command) => command,
            BrokerCommand::CancelOrder(_) => unreachable!(),
        };
        expired.ttl_ms = Some(1_000);
        let observed_at = expired.created_ts + chrono::Duration::seconds(2);
        let envelope = Envelope {
            schema_version: SCHEMA_VERSION,
            ts_utc: expired.created_ts,
            source: "stage7a-paper-test".to_string(),
            msg_type: MessageType::Command,
            payload: BrokerCommand::PlaceOrder(expired),
        };
        assert!(matches!(
            authority
                .handle_payload(
                    "22-0",
                    serde_json::to_string(&envelope).unwrap().as_bytes(),
                    observed_at,
                )
                .unwrap(),
            Stage7aHandleOutcome::Ack(Envelope {
                payload: CommandAck {
                    status: CommandAckStatus::Expired,
                    ..
                },
                ..
            })
        ));
        assert_eq!(authority.recovered().journal_frontier().frame_count(), 0);
    }

    #[test]
    fn stop_shape_and_profile_drift_cannot_reach_provider() {
        let mut authority = authority();
        let mut stop = match command(8004) {
            BrokerCommand::PlaceOrder(command) => command,
            BrokerCommand::CancelOrder(_) => unreachable!(),
        };
        stop.order_type = OrderType::Stop;
        stop.limit_price = None;
        let stop_envelope = Envelope {
            schema_version: SCHEMA_VERSION,
            ts_utc: stop.created_ts,
            source: "stage7a-paper-test".to_string(),
            msg_type: MessageType::Command,
            payload: BrokerCommand::PlaceOrder(stop),
        };
        assert!(matches!(
            authority
                .handle_payload(
                    "23-0",
                    serde_json::to_string(&stop_envelope).unwrap().as_bytes(),
                    Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 1).unwrap(),
                )
                .unwrap(),
            Stage7aHandleOutcome::Ack(Envelope {
                payload: CommandAck {
                    status: CommandAckStatus::Rejected,
                    ..
                },
                ..
            })
        ));

        let mut drift = match command(8005) {
            BrokerCommand::PlaceOrder(command) => command,
            BrokerCommand::CancelOrder(_) => unreachable!(),
        };
        drift.instrument.symbol = "RTS-9.26".to_string();
        let drift_envelope = Envelope {
            schema_version: SCHEMA_VERSION,
            ts_utc: drift.created_ts,
            source: "stage7a-paper-test".to_string(),
            msg_type: MessageType::Command,
            payload: BrokerCommand::PlaceOrder(drift),
        };
        assert!(matches!(
            authority
                .handle_payload(
                    "24-0",
                    serde_json::to_string(&drift_envelope).unwrap().as_bytes(),
                    Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 1).unwrap(),
                )
                .unwrap(),
            Stage7aHandleOutcome::Ack(Envelope {
                payload: CommandAck {
                    status: CommandAckStatus::Rejected,
                    reason: Some(CommandAckReason {
                        code: CommandAckReasonCode::LocalValidationRejected,
                    }),
                    ..
                },
                ..
            })
        ));
        assert_eq!(authority.recovered().journal_frontier().frame_count(), 0);
    }

    #[test]
    fn accepted_ack_then_runtime_duplicate_is_stage5g_noop() {
        let mut authority = authority();
        let payload = encoded_command(801);
        let observed_at = Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 1).unwrap();
        let first = match authority
            .handle_payload("1-0", payload.as_bytes(), observed_at)
            .unwrap()
        {
            Stage7aHandleOutcome::Ack(ack) => ack,
            _ => panic!("first delivery must settle"),
        };
        assert_eq!(first.payload.status, CommandAckStatus::Accepted);
        let frontier = authority.recovered().journal_frontier().frame_count();
        let unpublished_redelivery = match authority
            .handle_payload("2-0", payload.as_bytes(), observed_at)
            .unwrap()
        {
            Stage7aHandleOutcome::Ack(ack) => ack,
            _ => panic!("exact redelivery must replay ACK"),
        };
        assert_eq!(first, unpublished_redelivery);
        authority.mark_ack_published(&first);
        let duplicate = match authority
            .handle_payload(
                "2-0",
                payload.as_bytes(),
                observed_at + chrono::Duration::seconds(1),
            )
            .unwrap()
        {
            Stage7aHandleOutcome::Ack(ack) => ack,
            _ => panic!("published ACK redelivery must be a runtime duplicate"),
        };
        assert_eq!(duplicate.payload.status, CommandAckStatus::Duplicate);
        assert_eq!(
            duplicate.payload.reason.as_ref().map(|reason| reason.code),
            Some(CommandAckReasonCode::DuplicateCommand)
        );
        assert_eq!(duplicate.payload.request_id, first.payload.request_id);
        assert_eq!(
            duplicate.payload.client_order_id,
            first.payload.client_order_id
        );
        assert_eq!(
            duplicate.payload.broker_order_id,
            first.payload.broker_order_id
        );
        assert_eq!(
            authority.recovered().journal_frontier().frame_count(),
            frontier
        );
    }

    #[test]
    fn place_then_cancel_use_one_profile_without_redis_identity_authority() {
        let mut authority = authority();
        let place = encoded_command(806);
        let place_ack = match authority
            .handle_payload(
                "10-0",
                place.as_bytes(),
                Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 1).unwrap(),
            )
            .unwrap()
        {
            Stage7aHandleOutcome::Ack(ack) => ack,
            _ => panic!("place must settle"),
        };
        assert!(place_ack.payload.broker_order_id.is_some());
        let cancel = encoded_cancel(807, 806);
        let cancel_ack = match authority
            .handle_payload(
                "11-0",
                cancel.as_bytes(),
                Utc.with_ymd_and_hms(2026, 8, 11, 9, 1, 1).unwrap(),
            )
            .unwrap()
        {
            Stage7aHandleOutcome::Ack(ack) => ack,
            _ => panic!("cancel must settle"),
        };
        assert_eq!(cancel_ack.payload.status, CommandAckStatus::Accepted);
        assert_eq!(
            cancel_ack.payload.client_order_id,
            Some(ClientOrderId::from_strategy_request(
                StrategyRequestId::from(Uuid::from_u128(807))
            ))
        );
    }

    #[test]
    fn cancel_overlap_policy_is_explicit_and_fail_closed() {
        let observed_place = Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 1).unwrap();
        let observed_cancel = Utc.with_ymd_and_hms(2026, 8, 11, 9, 1, 1).unwrap();
        let mut command_authority = authority();
        assert!(matches!(
            command_authority
                .handle_payload(
                    "cancel-1",
                    encoded_command(8_401).as_bytes(),
                    observed_place
                )
                .unwrap(),
            Stage7aHandleOutcome::Ack(Envelope {
                payload: CommandAck {
                    status: CommandAckStatus::Accepted,
                    ..
                },
                ..
            })
        ));
        assert!(matches!(
            command_authority
                .handle_payload(
                    "cancel-2",
                    encoded_cancel(8_402, 8_401).as_bytes(),
                    observed_cancel,
                )
                .unwrap(),
            Stage7aHandleOutcome::Ack(Envelope {
                payload: CommandAck {
                    status: CommandAckStatus::Accepted,
                    ..
                },
                ..
            })
        ));
        assert!(matches!(
            command_authority
                .handle_payload(
                    "cancel-3",
                    encoded_cancel(8_403, 8_401).as_bytes(),
                    observed_cancel,
                )
                .unwrap(),
            Stage7aHandleOutcome::Pending(Stage7aPendingDecision {
                reason: Stage7aPaperHoldReason::AnotherLifecycleUnresolved,
                ..
            })
        ));

        let mut unknown_target = authority();
        assert!(matches!(
            unknown_target
                .handle_payload(
                    "cancel-unknown",
                    encoded_cancel(8_404, 9_999).as_bytes(),
                    observed_cancel,
                )
                .unwrap(),
            Stage7aHandleOutcome::Ack(Envelope {
                payload: CommandAck {
                    status: CommandAckStatus::Rejected,
                    reason: Some(CommandAckReason {
                        code: CommandAckReasonCode::LocalValidationRejected,
                    }),
                    ..
                },
                ..
            })
        ));
        assert_eq!(
            unknown_target.recovered().journal_frontier().frame_count(),
            0
        );
    }

    #[test]
    fn uncertain_provider_and_post_dispatch_crash_remain_pending() {
        let mut uncertain = Stage7aCommandAuthority::new(
            recovered(),
            profile(),
            UncertainPaperProvider,
            "stage7a-uncertain-test",
        )
        .unwrap();
        let payload = encoded_command(808);
        let observed_at = Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 1).unwrap();
        assert!(matches!(
            uncertain
                .handle_payload("12-0", payload.as_bytes(), observed_at)
                .unwrap(),
            Stage7aHandleOutcome::Pending(_)
        ));
        let frontier = uncertain.recovered().journal_frontier().frame_count();
        assert!(matches!(
            uncertain
                .handle_payload("12-0", payload.as_bytes(), observed_at)
                .unwrap(),
            Stage7aHandleOutcome::Pending(_)
        ));
        assert_eq!(
            uncertain.recovered().journal_frontier().frame_count(),
            frontier
        );

        let mut crashed = authority();
        crashed.set_fault_point(Stage7aFaultPoint::BeforePaperOutcome);
        assert!(matches!(
            crashed
                .handle_payload("13-0", encoded_command(809).as_bytes(), observed_at)
                .unwrap(),
            Stage7aHandleOutcome::Pending(_)
        ));
        crashed.set_fault_point(Stage7aFaultPoint::None);
        assert!(matches!(
            crashed
                .handle_payload("13-0", encoded_command(809).as_bytes(), observed_at)
                .unwrap(),
            Stage7aHandleOutcome::Pending(_)
        ));
    }

    #[test]
    fn supervisor_never_leaves_stale_ready_after_failure_or_stop() {
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 0).unwrap();
        let mut supervisor = Stage7aConsumerSupervisor::default();
        supervisor.mark_group_attached();
        supervisor.mark_source_poll_success(now);
        let (_, ready) = supervisor.snapshots(now, chrono::Duration::seconds(5));
        assert_eq!(ready.phase, Stage7aReadinessPhase::PaperReady);
        supervisor.mark_source_failure();
        let (_, degraded) = supervisor.snapshots(now, chrono::Duration::seconds(5));
        assert_eq!(degraded.phase, Stage7aReadinessPhase::Degraded);
        supervisor.mark_stopped();
        let (_, stopped) = supervisor.snapshots(now, chrono::Duration::seconds(5));
        assert_eq!(stopped.phase, Stage7aReadinessPhase::Stopped);
        assert!(stopped
            .reasons
            .contains(&Stage7aReadinessReason::ConsumerNotAlive));
        let retry = Stage7aRetryPolicy {
            initial_delay_ms: 10,
            max_delay_ms: 80,
        }
        .validate()
        .unwrap();
        assert_eq!(retry.delay_for_failure(1), Duration::from_millis(10));
        assert_eq!(retry.delay_for_failure(4), Duration::from_millis(80));
        assert_eq!(retry.delay_for_failure(20), Duration::from_millis(80));
    }

    #[test]
    fn unrelated_success_does_not_clear_blocked_request() {
        let now = Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 0).unwrap();
        let request_a = StrategyRequestId::from(Uuid::from_u128(8_201));
        let request_b = StrategyRequestId::from(Uuid::from_u128(8_202));
        let mut supervisor = Stage7aConsumerSupervisor::default();
        supervisor.mark_group_attached();
        supervisor.mark_source_poll_success(now);
        supervisor.mark_blocked("8201-0", Some(request_a), Stage7aBlockedKind::Authority);
        supervisor.mark_blocked("8202-0", Some(request_b), Stage7aBlockedKind::AckSettlement);
        supervisor.clear_blocked_entry("8202-0");
        let (health, readiness) = supervisor.snapshots(now, chrono::Duration::seconds(5));
        assert!(!health.stage6_authority_healthy);
        assert!(health.ack_settlement_healthy);
        assert_eq!(readiness.phase, Stage7aReadinessPhase::Degraded);
        assert_eq!(readiness.blocked_entry_ids, vec!["8201-0"]);
        assert_eq!(readiness.blocked_request_ids, vec![request_a]);
    }

    #[tokio::test]
    async fn real_redis_xreadgroup_ack_and_xautoclaim_share_canonical_handler() {
        let redis = RedisServer::start().await;
        let mut config = Stage7aRedisConfig::paper_default("redis-integration").unwrap();
        config.group_start = Stage7aGroupStart::Beginning;
        config.allow_controlled_beginning = true;
        config.block_ms = 0;
        config.claim_idle_ms = 1;
        config.claim_count = 1;
        let mut consumer = Stage7aRedisConsumer::connect(&redis.url, config.clone(), authority())
            .await
            .unwrap();
        consumer.ensure_group().await.unwrap();
        consumer.ensure_group().await.unwrap();

        let client = redis::Client::open(redis.url.as_str()).unwrap();
        let mut manager = ConnectionManager::new(client).await.unwrap();
        let payload = encoded_command(802);
        let _: String = redis::cmd("XADD")
            .arg(&config.command_stream)
            .arg("*")
            .arg("payload")
            .arg(&payload)
            .query_async(&mut manager)
            .await
            .unwrap();
        assert_eq!(consumer.poll_new_once().await.unwrap(), 1);
        let (_, readiness) = consumer
            .publish_observability(Utc::now(), chrono::Duration::seconds(5))
            .await
            .unwrap();
        assert_eq!(readiness.phase, Stage7aReadinessPhase::PaperReady);
        let ack_len: i64 = redis::cmd("XLEN")
            .arg(&config.ack_stream)
            .query_async(&mut manager)
            .await
            .unwrap();
        assert_eq!(ack_len, 1);
        let health_len: i64 = redis::cmd("XLEN")
            .arg(&config.health_stream)
            .query_async(&mut manager)
            .await
            .unwrap();
        let readiness_len: i64 = redis::cmd("XLEN")
            .arg(&config.readiness_stream)
            .query_async(&mut manager)
            .await
            .unwrap();
        assert_eq!((health_len, readiness_len), (1, 1));
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&config.command_stream)
            .arg(&config.consumer_group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut manager)
            .await
            .unwrap();
        assert!(pending.ids.is_empty());

        let _: String = redis::cmd("XADD")
            .arg(&config.command_stream)
            .arg("*")
            .arg("payload")
            .arg(&payload)
            .query_async(&mut manager)
            .await
            .unwrap();
        let pending_read: StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(&config.consumer_group)
            .arg("crashed-consumer")
            .arg("COUNT")
            .arg(1)
            .arg("STREAMS")
            .arg(&config.command_stream)
            .arg(">")
            .query_async(&mut manager)
            .await
            .unwrap();
        assert_eq!(pending_read.keys[0].ids.len(), 1);
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(consumer.reclaim_stale_once().await.unwrap(), 1);
        let acks: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&config.ack_stream)
            .arg("-")
            .arg("+")
            .query_async(&mut manager)
            .await
            .unwrap();
        assert_eq!(acks.ids.len(), 2);
    }

    #[tokio::test]
    async fn ack_xadd_success_before_xack_redelivery_emits_runtime_duplicate() {
        let redis = RedisServer::start().await;
        let mut config = Stage7aRedisConfig::paper_default("ack-xack-fault").unwrap();
        config.group_start = Stage7aGroupStart::Beginning;
        config.allow_controlled_beginning = true;
        config.block_ms = 0;
        config.claim_idle_ms = 1;
        let mut consumer = Stage7aRedisConsumer::connect(&redis.url, config.clone(), authority())
            .await
            .unwrap();
        consumer.ensure_group().await.unwrap();
        let client = redis::Client::open(redis.url.as_str()).unwrap();
        let mut manager = ConnectionManager::new(client).await.unwrap();
        let _: String = redis::cmd("XADD")
            .arg(&config.command_stream)
            .arg("*")
            .arg("payload")
            .arg(encoded_command(810))
            .query_async(&mut manager)
            .await
            .unwrap();
        consumer.set_settlement_fault(Stage7aSettlementFault::AfterAckPublishBeforeXack);
        assert!(matches!(
            consumer.poll_new_once().await,
            Err(Stage7aBridgeError::InjectedSettlementFault)
        ));
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&config.command_stream)
            .arg(&config.consumer_group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut manager)
            .await
            .unwrap();
        assert_eq!(pending.ids.len(), 1);
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(consumer.reclaim_stale_once().await.unwrap(), 1);
        let acks: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&config.ack_stream)
            .arg("-")
            .arg("+")
            .query_async(&mut manager)
            .await
            .unwrap();
        assert_eq!(acks.ids.len(), 2);
        let accepted: Envelope<CommandAck> =
            serde_json::from_str(&acks.ids[0].get::<String>("payload").unwrap()).unwrap();
        let duplicate: Envelope<CommandAck> =
            serde_json::from_str(&acks.ids[1].get::<String>("payload").unwrap()).unwrap();
        assert_eq!(accepted.payload.status, CommandAckStatus::Accepted);
        assert_eq!(duplicate.payload.status, CommandAckStatus::Duplicate);
        assert_eq!(accepted.payload.request_id, duplicate.payload.request_id);
        assert_eq!(
            accepted.payload.client_order_id,
            duplicate.payload.client_order_id
        );
        assert_eq!(
            accepted.payload.broker_order_id,
            duplicate.payload.broker_order_id
        );
        assert_eq!(
            duplicate.payload.reason.map(|reason| reason.code),
            Some(CommandAckReasonCode::DuplicateCommand)
        );
        let (_, readiness) = consumer
            .supervisor()
            .snapshots(Utc::now(), chrono::Duration::seconds(5));
        assert_eq!(readiness.phase, Stage7aReadinessPhase::PaperReady);
    }

    #[tokio::test]
    async fn ack_xadd_failure_redelivery_republishes_canonical_accepted() {
        let redis = RedisServer::start().await;
        let mut config = Stage7aRedisConfig::paper_default("ack-xadd-failure").unwrap();
        config.group_start = Stage7aGroupStart::Beginning;
        config.allow_controlled_beginning = true;
        config.block_ms = 0;
        config.claim_idle_ms = 1;
        let mut consumer = Stage7aRedisConsumer::connect(&redis.url, config.clone(), authority())
            .await
            .unwrap();
        consumer.ensure_group().await.unwrap();
        let client = redis::Client::open(redis.url.as_str()).unwrap();
        let mut manager = ConnectionManager::new(client).await.unwrap();
        let _: () = redis::cmd("SET")
            .arg(&config.ack_stream)
            .arg("wrong-type")
            .query_async(&mut manager)
            .await
            .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(&config.command_stream)
            .arg("*")
            .arg("payload")
            .arg(encoded_command(8_301))
            .query_async(&mut manager)
            .await
            .unwrap();
        assert!(matches!(
            consumer.poll_new_once().await,
            Err(Stage7aBridgeError::Redis(_))
        ));
        assert_eq!(consumer.poll_new_once().await.unwrap(), 0);
        let (health, readiness) = consumer
            .supervisor()
            .snapshots(Utc::now(), chrono::Duration::seconds(5));
        assert!(!health.ack_settlement_healthy);
        assert_eq!(readiness.phase, Stage7aReadinessPhase::Degraded);

        let _: i64 = redis::cmd("DEL")
            .arg(&config.ack_stream)
            .query_async(&mut manager)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(consumer.reclaim_stale_once().await.unwrap(), 1);
        let acks: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&config.ack_stream)
            .arg("-")
            .arg("+")
            .query_async(&mut manager)
            .await
            .unwrap();
        assert_eq!(acks.ids.len(), 1);
        let accepted: Envelope<CommandAck> =
            serde_json::from_str(&acks.ids[0].get::<String>("payload").unwrap()).unwrap();
        assert_eq!(accepted.payload.status, CommandAckStatus::Accepted);
        let (health, readiness) = consumer
            .supervisor()
            .snapshots(Utc::now(), chrono::Duration::seconds(5));
        assert!(health.ack_settlement_healthy);
        assert_eq!(readiness.phase, Stage7aReadinessPhase::PaperReady);
    }

    #[tokio::test]
    async fn xautoclaim_tail_eventually_reached_with_claim_count_1_max_pages_1() {
        let redis = RedisServer::start().await;
        let mut config = Stage7aRedisConfig::paper_default("claim-cursor-r1").unwrap();
        config.group_start = Stage7aGroupStart::Beginning;
        config.allow_controlled_beginning = true;
        config.block_ms = 0;
        config.claim_idle_ms = 1;
        config.claim_count = 1;
        config.max_claim_pages = 1;
        let mut consumer = Stage7aRedisConsumer::connect(&redis.url, config.clone(), authority())
            .await
            .unwrap();
        consumer.ensure_group().await.unwrap();
        let client = redis::Client::open(redis.url.as_str()).unwrap();
        let mut manager = ConnectionManager::new(client).await.unwrap();
        for suffix in 1..=3 {
            let _: String = redis::cmd("XADD")
                .arg(&config.command_stream)
                .arg("*")
                .arg("payload")
                .arg(format!("poison-{suffix}"))
                .query_async(&mut manager)
                .await
                .unwrap();
        }
        let pending_read: StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(&config.consumer_group)
            .arg("stalled-consumer")
            .arg("COUNT")
            .arg(3)
            .arg("STREAMS")
            .arg(&config.command_stream)
            .arg(">")
            .query_async(&mut manager)
            .await
            .unwrap();
        assert_eq!(pending_read.keys[0].ids.len(), 3);
        tokio::time::sleep(Duration::from_millis(5)).await;

        let mut examined = 0usize;
        let mut observed_nonzero_cursor = false;
        for _ in 0..6 {
            examined += consumer.reclaim_stale_once().await.unwrap();
            observed_nonzero_cursor |= consumer.claim_cursor != "0-0";
            let pending: StreamPendingCountReply = redis::cmd("XPENDING")
                .arg(&config.command_stream)
                .arg(&config.consumer_group)
                .arg("-")
                .arg("+")
                .arg(10)
                .query_async(&mut manager)
                .await
                .unwrap();
            if pending.ids.is_empty() {
                break;
            }
        }
        assert!(observed_nonzero_cursor);
        assert_eq!(examined, 3);
        let pending: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&config.command_stream)
            .arg(&config.consumer_group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut manager)
            .await
            .unwrap();
        assert!(pending.ids.is_empty());
        let dlq: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&config.dlq_stream)
            .arg("-")
            .arg("+")
            .query_async(&mut manager)
            .await
            .unwrap();
        assert_eq!(dlq.ids.len(), 3);
    }

    #[tokio::test]
    async fn dlq_outage_empty_polls_do_not_restore_readiness() {
        let redis = RedisServer::start().await;
        let mut config = Stage7aRedisConfig::paper_default("publish-failure").unwrap();
        config.group_start = Stage7aGroupStart::Beginning;
        config.allow_controlled_beginning = true;
        config.block_ms = 0;
        config.claim_idle_ms = 1;
        let mut consumer = Stage7aRedisConsumer::connect(&redis.url, config.clone(), authority())
            .await
            .unwrap();
        consumer.ensure_group().await.unwrap();
        let client = redis::Client::open(redis.url.as_str()).unwrap();
        let mut manager = ConnectionManager::new(client).await.unwrap();

        let _: () = redis::cmd("SET")
            .arg(&config.ack_stream)
            .arg("wrong-type")
            .query_async(&mut manager)
            .await
            .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(&config.command_stream)
            .arg("*")
            .arg("payload")
            .arg(encoded_command(811))
            .query_async(&mut manager)
            .await
            .unwrap();
        assert!(matches!(
            consumer.poll_new_once().await,
            Err(Stage7aBridgeError::Redis(_))
        ));
        let pending_ack: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&config.command_stream)
            .arg(&config.consumer_group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut manager)
            .await
            .unwrap();
        assert_eq!(pending_ack.ids.len(), 1);
        assert_eq!(consumer.poll_new_once().await.unwrap(), 0);
        let (health, readiness) = consumer
            .supervisor()
            .snapshots(Utc::now(), chrono::Duration::seconds(5));
        assert!(!health.ack_settlement_healthy);
        assert_eq!(readiness.phase, Stage7aReadinessPhase::Degraded);
        assert_eq!(readiness.blocked_entry_ids.len(), 1);
        let _: i64 = redis::cmd("DEL")
            .arg(&config.ack_stream)
            .query_async(&mut manager)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(consumer.reclaim_stale_once().await.unwrap(), 1);

        let _: () = redis::cmd("SET")
            .arg(&config.dlq_stream)
            .arg("wrong-type")
            .query_async(&mut manager)
            .await
            .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(&config.command_stream)
            .arg("*")
            .arg("payload")
            .arg("token=SECRET&not-json")
            .query_async(&mut manager)
            .await
            .unwrap();
        assert!(matches!(
            consumer.poll_new_once().await,
            Err(Stage7aBridgeError::Redis(_))
        ));
        let pending_dlq: StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&config.command_stream)
            .arg(&config.consumer_group)
            .arg("-")
            .arg("+")
            .arg(10)
            .query_async(&mut manager)
            .await
            .unwrap();
        assert_eq!(pending_dlq.ids.len(), 1);
        assert_eq!(consumer.poll_new_once().await.unwrap(), 0);
        let (health, readiness) = consumer
            .supervisor()
            .snapshots(Utc::now(), chrono::Duration::seconds(5));
        assert!(!health.dlq_settlement_healthy);
        assert_eq!(readiness.phase, Stage7aReadinessPhase::Degraded);
        assert_eq!(readiness.blocked_entry_ids.len(), 1);
        let _: i64 = redis::cmd("DEL")
            .arg(&config.dlq_stream)
            .query_async(&mut manager)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(consumer.reclaim_stale_once().await.unwrap(), 1);
        let dlq: StreamRangeReply = redis::cmd("XRANGE")
            .arg(&config.dlq_stream)
            .arg("-")
            .arg("+")
            .query_async(&mut manager)
            .await
            .unwrap();
        let encoded = dlq.ids[0].get::<String>("payload").unwrap();
        assert!(!encoded.contains("SECRET"));
    }
}
