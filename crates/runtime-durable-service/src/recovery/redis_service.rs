//! Stage 7B-d-c supervised paper Redis service.
//!
//! The service composes the accepted Stage 7B recovery owner with the atomic
//! d-b Redis settlement primitive. Redis delivery metadata remains transport
//! only: Stage 6 journal/replay is the sole command execution authority.

use super::redis_settlement::{Stage7bRedisSettlementBackend, Stage7bRedisSettlementContext};
use super::Stage7bRecoveryReadyOwner;
use broker_core::{BrokerCommand, Envelope, StrategyRequestId};
use chrono::{DateTime, Utc};
use redis::aio::ConnectionManager;
use redis::streams::{StreamAutoClaimReply, StreamId, StreamPendingReply, StreamReadReply};
use runtime_command_bridge::{
    classify_stage7a_permanent_pre_admission_poison, decode_stage7a_pre_admission,
    Stage7aCommandProfile, Stage7aPaperOutcomeProvider, Stage7aPaperProviderError,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use strategy_runtime_core::{Stage5gLifecycleCommitmentKey, Stage7aPaperAdmission};
use uuid::Uuid;

const PAPER_PREFIX: &str = "finam_imoexf_paper:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage7bRedisServiceConfig {
    pub hash_tag: String,
    pub command_stream: String,
    pub ack_stream: String,
    pub dlq_stream: String,
    pub consumer_group: String,
    pub consumer_name: String,
    pub read_count: usize,
    pub claim_count: usize,
    pub block_ms: u64,
    pub claim_idle_ms: u64,
    pub max_claim_pages: usize,
    pub freshness_ms: i64,
}

impl Stage7bRedisServiceConfig {
    pub fn paper_default_auto(hash_tag: &str) -> Result<Self, Stage7bRedisServiceError> {
        let boot = Uuid::new_v4();
        let prefix = format!("{PAPER_PREFIX}{{{hash_tag}}}:stage7b");
        let config = Self {
            hash_tag: hash_tag.to_string(),
            command_stream: format!("{prefix}:commands"),
            ack_stream: format!("{prefix}:acks"),
            dlq_stream: format!("{prefix}:dlq"),
            consumer_group: "stage7b-paper-command-consumer-v1".to_string(),
            consumer_name: format!("stage7b-boot-{}", boot.simple()),
            read_count: 32,
            claim_count: 32,
            block_ms: 1_000,
            claim_idle_ms: 30_000,
            max_claim_pages: 128,
            freshness_ms: 60_000,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), Stage7bRedisServiceError> {
        if !token(&self.hash_tag)
            || !token(&self.consumer_group)
            || !token(&self.consumer_name)
            || self.read_count == 0
            || self.claim_count == 0
            || self.claim_idle_ms == 0
            || self.max_claim_pages == 0
            || self.freshness_ms <= 0
        {
            return Err(Stage7bRedisServiceError::InvalidConfig);
        }
        let expected_tag = format!("{{{}}}", self.hash_tag);
        let streams = [&self.command_stream, &self.ack_stream, &self.dlq_stream];
        if streams.iter().any(|stream| {
            !stream.starts_with(PAPER_PREFIX)
                || !stream.contains(&expected_tag)
                || stream.matches('{').count() != 1
                || stream.matches('}').count() != 1
                || stream.contains(char::is_whitespace)
        }) || self.command_stream == self.ack_stream
            || self.command_stream == self.dlq_stream
            || self.ack_stream == self.dlq_stream
        {
            return Err(Stage7bRedisServiceError::InvalidConfig);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage7bPaperReadinessPhase {
    PaperReady,
    Degraded,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage7bPaperReadinessReason {
    ConsumerNotAlive,
    StorageUnavailable,
    SourcePollStale,
    ClaimScanStale,
    SettlementUnavailable,
    DurablePendingEntries,
    CommandLifecycleBlocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage7bCompositeHealthSnapshot {
    pub command_consumer_alive: bool,
    pub durable_storage_ready: bool,
    pub source_poll_fresh: bool,
    pub claim_scan_fresh: bool,
    pub settlement_healthy: bool,
    pub durable_pending_count: usize,
    pub blocked_entry_count: usize,
    pub last_successful_source_poll_at: Option<DateTime<Utc>>,
    pub last_successful_claim_scan_at: Option<DateTime<Utc>>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage7bCompositeReadinessSnapshot {
    pub phase: Stage7bPaperReadinessPhase,
    pub reasons: Vec<Stage7bPaperReadinessReason>,
    pub blocked_entry_ids: Vec<String>,
    pub blocked_request_ids: Vec<StrategyRequestId>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct BlockedEntry {
    request_id: Option<StrategyRequestId>,
}

#[derive(Debug, Default)]
struct SupervisorState {
    durable_storage_ready: bool,
    settlement_healthy: bool,
    durable_pending_count: usize,
    last_successful_source_poll_at: Option<DateTime<Utc>>,
    last_successful_claim_scan_at: Option<DateTime<Utc>>,
    blocked_entries: BTreeMap<String, BlockedEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct Stage7bTaskReadinessHandle {
    alive: Arc<AtomicBool>,
}

impl Stage7bTaskReadinessHandle {
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    fn mark_started(&self) {
        self.alive.store(true, Ordering::Release);
    }

    fn mark_stopped(&self) {
        self.alive.store(false, Ordering::Release);
    }
}

struct Stage7bTaskStopGuard(Stage7bTaskReadinessHandle);

impl Drop for Stage7bTaskStopGuard {
    fn drop(&mut self) {
        self.0.mark_stopped();
    }
}

pub fn spawn_stage7b_supervised_task<F, T>(
    readiness: Stage7bTaskReadinessHandle,
    future: F,
) -> tokio::task::JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    readiness.mark_started();
    let stop_guard = Stage7bTaskStopGuard(readiness);
    tokio::spawn(async move {
        let _stop_guard = stop_guard;
        future.await
    })
}

#[derive(Debug, Clone, Default)]
pub struct Stage7bServiceSupervisor {
    task: Stage7bTaskReadinessHandle,
    state: Arc<Mutex<SupervisorState>>,
}

impl Stage7bServiceSupervisor {
    pub fn task_readiness_handle(&self) -> Stage7bTaskReadinessHandle {
        self.task.clone()
    }

    pub fn snapshots(
        &self,
        checked_at: DateTime<Utc>,
        freshness: chrono::Duration,
    ) -> (
        Stage7bCompositeHealthSnapshot,
        Stage7bCompositeReadinessSnapshot,
    ) {
        let state = lock(&self.state);
        let alive = self.task.is_alive();
        let source_poll_fresh = state
            .last_successful_source_poll_at
            .is_some_and(|last| checked_at - last <= freshness);
        let claim_scan_fresh = state
            .last_successful_claim_scan_at
            .is_some_and(|last| checked_at - last <= freshness);
        let mut reasons = Vec::new();
        if !alive {
            reasons.push(Stage7bPaperReadinessReason::ConsumerNotAlive);
        }
        if !state.durable_storage_ready {
            reasons.push(Stage7bPaperReadinessReason::StorageUnavailable);
        }
        if !source_poll_fresh {
            reasons.push(Stage7bPaperReadinessReason::SourcePollStale);
        }
        if !claim_scan_fresh {
            reasons.push(Stage7bPaperReadinessReason::ClaimScanStale);
        }
        if !state.settlement_healthy {
            reasons.push(Stage7bPaperReadinessReason::SettlementUnavailable);
        }
        if state.durable_pending_count != 0 {
            reasons.push(Stage7bPaperReadinessReason::DurablePendingEntries);
        }
        if !state.blocked_entries.is_empty() {
            reasons.push(Stage7bPaperReadinessReason::CommandLifecycleBlocked);
        }
        let phase = if !alive {
            Stage7bPaperReadinessPhase::Stopped
        } else if reasons.is_empty() {
            Stage7bPaperReadinessPhase::PaperReady
        } else {
            Stage7bPaperReadinessPhase::Degraded
        };
        let health = Stage7bCompositeHealthSnapshot {
            command_consumer_alive: alive,
            durable_storage_ready: state.durable_storage_ready,
            source_poll_fresh,
            claim_scan_fresh,
            settlement_healthy: state.settlement_healthy,
            durable_pending_count: state.durable_pending_count,
            blocked_entry_count: state.blocked_entries.len(),
            last_successful_source_poll_at: state.last_successful_source_poll_at,
            last_successful_claim_scan_at: state.last_successful_claim_scan_at,
            checked_at,
        };
        let readiness = Stage7bCompositeReadinessSnapshot {
            phase,
            reasons,
            blocked_entry_ids: state.blocked_entries.keys().cloned().collect(),
            blocked_request_ids: state
                .blocked_entries
                .values()
                .filter_map(|entry| entry.request_id)
                .collect(),
            checked_at,
        };
        (health, readiness)
    }

    fn mark_source_success(&self, now: DateTime<Utc>) {
        lock(&self.state).last_successful_source_poll_at = Some(now);
    }

    fn mark_claim_success(&self, now: DateTime<Utc>) {
        lock(&self.state).last_successful_claim_scan_at = Some(now);
    }

    fn mark_storage(&self, ready: bool) {
        lock(&self.state).durable_storage_ready = ready;
    }

    fn mark_settlement(&self, healthy: bool) {
        lock(&self.state).settlement_healthy = healthy;
    }

    fn mark_pending_count(&self, count: usize) {
        lock(&self.state).durable_pending_count = count;
    }

    fn block(&self, entry_id: String, request_id: Option<StrategyRequestId>) {
        lock(&self.state)
            .blocked_entries
            .insert(entry_id, BlockedEntry { request_id });
    }

    fn clear(&self, entry_id: &str) {
        lock(&self.state).blocked_entries.remove(entry_id);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage7bServiceRunSummary {
    pub iterations: usize,
    pub fresh_entries_examined: usize,
    pub reclaimed_entries_examined: usize,
}

pub struct Stage7bRedisService<P> {
    connection: ConnectionManager,
    settlement: Stage7bRedisSettlementBackend,
    owner: Stage7bRecoveryReadyOwner,
    commitment_key: Stage5gLifecycleCommitmentKey,
    profile: Stage7aCommandProfile,
    provider: P,
    config: Stage7bRedisServiceConfig,
    supervisor: Stage7bServiceSupervisor,
    claim_cursor: String,
}

pub type Stage7bServiceTaskOutput<P> = (
    Stage7bRedisService<P>,
    Result<Stage7bServiceRunSummary, Stage7bRedisServiceError>,
);
pub type Stage7bServiceTaskHandle<P> = tokio::task::JoinHandle<Stage7bServiceTaskOutput<P>>;

impl<P: Stage7aPaperOutcomeProvider> Stage7bRedisService<P> {
    pub async fn connect(
        redis_url: &str,
        config: Stage7bRedisServiceConfig,
        owner: Stage7bRecoveryReadyOwner,
        commitment_key: Stage5gLifecycleCommitmentKey,
        profile: Stage7aCommandProfile,
        provider: P,
    ) -> Result<Self, Stage7bRedisServiceError> {
        config.validate()?;
        let client = redis::Client::open(redis_url)?;
        let connection = ConnectionManager::new(client).await?;
        let settlement = Stage7bRedisSettlementBackend::connect(redis_url)
            .await
            .map_err(|error| Stage7bRedisServiceError::Settlement(error.to_string()))?;
        Ok(Self {
            connection,
            settlement,
            owner,
            commitment_key,
            profile,
            provider,
            config,
            supervisor: Stage7bServiceSupervisor::default(),
            claim_cursor: "0-0".to_string(),
        })
    }

    pub fn supervisor(&self) -> Stage7bServiceSupervisor {
        self.supervisor.clone()
    }

    pub fn consumer_name(&self) -> &str {
        &self.config.consumer_name
    }

    pub fn claim_cursor(&self) -> &str {
        &self.claim_cursor
    }

    pub fn redis_consumer_attached(&self) -> bool {
        true
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

    pub async fn run_bounded(
        &mut self,
        max_iterations: usize,
    ) -> Result<Stage7bServiceRunSummary, Stage7bRedisServiceError> {
        if max_iterations == 0 {
            return Err(Stage7bRedisServiceError::InvalidConfig);
        }
        self.ensure_group().await?;
        self.reconstruct_durable_pending().await?;
        let mut summary = Stage7bServiceRunSummary {
            iterations: 0,
            fresh_entries_examined: 0,
            reclaimed_entries_examined: 0,
        };
        for _ in 0..max_iterations {
            summary.iterations += 1;
            let storage_ready = self
                .owner
                .validate_composite_readiness(&self.commitment_key);
            self.supervisor.mark_storage(storage_ready);
            if !storage_ready {
                return Err(Stage7bRedisServiceError::StorageUnavailable);
            }
            summary.reclaimed_entries_examined += self.reclaim_stale_once().await?;
            if self.blocked() {
                self.reconstruct_durable_pending().await?;
                continue;
            }
            summary.fresh_entries_examined += self.poll_new_once().await?;
            self.reconstruct_durable_pending().await?;
        }
        Ok(summary)
    }

    pub fn spawn_supervised_bounded(
        self,
        max_iterations: usize,
    ) -> (Stage7bServiceSupervisor, Stage7bServiceTaskHandle<P>)
    where
        P: Send + 'static,
    {
        let supervisor = self.supervisor();
        let readiness = supervisor.task_readiness_handle();
        let join = spawn_stage7b_supervised_task(readiness, async move {
            let mut service = self;
            let result = service.run_bounded(max_iterations).await;
            (service, result)
        });
        (supervisor, join)
    }

    pub async fn ensure_group(&mut self) -> Result<(), Stage7bRedisServiceError> {
        let result: redis::RedisResult<()> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&self.config.command_stream)
            .arg(&self.config.consumer_group)
            .arg("$")
            .arg("MKSTREAM")
            .query_async(&mut self.connection)
            .await;
        match result {
            Ok(()) => Ok(()),
            Err(error) if error.to_string().contains("BUSYGROUP") => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn poll_new_once(&mut self) -> Result<usize, Stage7bRedisServiceError> {
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
        let reply: StreamReadReply = command
            .arg("STREAMS")
            .arg(&self.config.command_stream)
            .arg(">")
            .query_async(&mut self.connection)
            .await?;
        self.supervisor.mark_source_success(Utc::now());
        self.process_entries(reply.keys.into_iter().flat_map(|key| key.ids))
            .await
    }

    pub async fn reclaim_stale_once(&mut self) -> Result<usize, Stage7bRedisServiceError> {
        let mut examined = 0usize;
        for _ in 0..self.config.max_claim_pages {
            let start = self.claim_cursor.clone();
            let reply: StreamAutoClaimReply = redis::cmd("XAUTOCLAIM")
                .arg(&self.config.command_stream)
                .arg(&self.config.consumer_group)
                .arg(&self.config.consumer_name)
                .arg(self.config.claim_idle_ms)
                .arg(&start)
                .arg("COUNT")
                .arg(self.config.claim_count)
                .query_async(&mut self.connection)
                .await?;
            self.claim_cursor = reply.next_stream_id.clone();
            examined = examined.saturating_add(reply.claimed.len());
            let processed = self.process_entries(reply.claimed).await?;
            if processed != 0 && self.blocked() {
                break;
            }
            if claim_scan_complete(&start, &self.claim_cursor) {
                self.claim_cursor = "0-0".to_string();
                break;
            }
        }
        self.supervisor.mark_claim_success(Utc::now());
        Ok(examined)
    }

    async fn process_entries<I>(&mut self, entries: I) -> Result<usize, Stage7bRedisServiceError>
    where
        I: IntoIterator<Item = StreamId>,
    {
        let mut count = 0usize;
        for entry in entries {
            count = count.saturating_add(1);
            if !self.process_entry(entry).await? {
                break;
            }
        }
        Ok(count)
    }

    /// Returns true only when processing may continue to another entry.
    async fn process_entry(&mut self, entry: StreamId) -> Result<bool, Stage7bRedisServiceError> {
        let payload = entry.get::<String>("payload");
        let envelope = match payload.as_deref() {
            Some(payload) => match decode_stage7a_pre_admission(payload.as_bytes()) {
                Ok(envelope) => envelope,
                Err(_) => {
                    return self.settle_poison(entry.id, Some(payload.as_bytes())).await;
                }
            },
            None => return self.settle_poison(entry.id, None).await,
        };
        self.process_valid_command(entry.id, envelope).await
    }

    async fn settle_poison(
        &mut self,
        entry_id: String,
        payload: Option<&[u8]>,
    ) -> Result<bool, Stage7bRedisServiceError> {
        let context = self.context(&entry_id)?;
        // Evidence is minted from this exact consumed entry/payload and is
        // immediately consumed by the owner; it is never queued or paired.
        let evidence = classify_stage7a_permanent_pre_admission_poison(&entry_id, payload)?;
        let observation = self
            .owner
            .observe_pre_stage6_poison(&self.commitment_key, context, evidence)
            .map_err(|error| Stage7bRedisServiceError::Authority(error.to_string()))?;
        match self
            .owner
            .settle_pre_stage6_poison(&self.commitment_key, observation, &mut self.settlement)
            .await
        {
            Ok(_) => {
                self.supervisor.clear(&entry_id);
                self.supervisor.mark_settlement(self.settlement.healthy());
                Ok(true)
            }
            Err(error) => {
                self.supervisor.block(entry_id, None);
                self.supervisor.mark_settlement(false);
                Err(Stage7bRedisServiceError::Authority(error.to_string()))
            }
        }
    }

    async fn process_valid_command(
        &mut self,
        entry_id: String,
        envelope: Envelope<BrokerCommand>,
    ) -> Result<bool, Stage7bRedisServiceError> {
        let request_id = command_request_id(&envelope.payload);
        let context = match self
            .profile
            .context_for_recovered(&envelope.payload, self.owner.recovered()?)
        {
            Ok(context) => context,
            Err(_) => {
                self.supervisor.block(entry_id, Some(request_id));
                return Ok(false);
            }
        };
        let admission = self
            .owner
            .admit_paper_command(&envelope.payload, &context, Utc::now())?;
        let finalized = match admission {
            Stage7aPaperAdmission::DispatchReady(receipt) => {
                let outcome = match self.provider.paper_outcome(&envelope.payload, Utc::now()) {
                    Ok(outcome) => outcome,
                    Err(Stage7aPaperProviderError::Uncertain) => {
                        self.supervisor.block(entry_id, Some(request_id));
                        return Ok(false);
                    }
                };
                let report = self.owner.record_paper_outcome(*receipt, outcome)?;
                let (_, finalized) = self.owner.finalize_paper_request(report, Utc::now())?;
                finalized
            }
            Stage7aPaperAdmission::Duplicate(_) => self
                .owner
                .finalize_replayed_paper_request(request_id, Utc::now())?,
            Stage7aPaperAdmission::Hold { .. } | Stage7aPaperAdmission::PolicyRejected { .. } => {
                self.supervisor.block(entry_id, Some(request_id));
                return Ok(false);
            }
        };
        let settlement_context = self.context(&entry_id)?;
        match self
            .owner
            .settle_finalized_ack(
                finalized,
                &self.commitment_key,
                settlement_context,
                &mut self.settlement,
            )
            .await
        {
            Ok(_) => {
                self.supervisor.clear(&entry_id);
                self.supervisor.mark_settlement(self.settlement.healthy());
                Ok(true)
            }
            Err(error) => {
                self.supervisor.block(entry_id, Some(request_id));
                self.supervisor.mark_settlement(false);
                Err(Stage7bRedisServiceError::Authority(error.to_string()))
            }
        }
    }

    async fn reconstruct_durable_pending(&mut self) -> Result<(), Stage7bRedisServiceError> {
        let pending: StreamPendingReply = redis::cmd("XPENDING")
            .arg(&self.config.command_stream)
            .arg(&self.config.consumer_group)
            .query_async(&mut self.connection)
            .await?;
        self.supervisor.mark_pending_count(pending.count());
        self.supervisor.mark_settlement(self.settlement.healthy());
        Ok(())
    }

    fn context(
        &self,
        entry_id: &str,
    ) -> Result<Stage7bRedisSettlementContext, Stage7bRedisServiceError> {
        Stage7bRedisSettlementContext::new(
            self.config.hash_tag.clone(),
            self.config.command_stream.clone(),
            self.config.ack_stream.clone(),
            self.config.dlq_stream.clone(),
            self.config.consumer_group.clone(),
            entry_id.to_string(),
        )
        .map_err(|error| Stage7bRedisServiceError::Settlement(error.to_string()))
    }

    fn blocked(&self) -> bool {
        !lock(&self.supervisor.state).blocked_entries.is_empty()
    }
}

fn command_request_id(command: &BrokerCommand) -> StrategyRequestId {
    match command {
        BrokerCommand::PlaceOrder(command) => command.request_id,
        BrokerCommand::CancelOrder(command) => command.request_id,
    }
}

fn claim_scan_complete(start: &str, next: &str) -> bool {
    next == "0-0" || next == start
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[derive(Debug, thiserror::Error)]
pub enum Stage7bRedisServiceError {
    #[error("invalid Stage 7B Redis service configuration")]
    InvalidConfig,
    #[error("Stage 7B durable storage or seal is unavailable")]
    StorageUnavailable,
    #[error("Stage 7B authority rejected command processing: {0}")]
    Authority(String),
    #[error("Stage 7B recovery rejected command processing")]
    Recovery(#[from] super::Stage7bRecoveryError),
    #[error("Stage 7B Redis settlement rejected operation: {0}")]
    Settlement(String),
    #[error("Stage 7A command boundary rejected operation")]
    Stage7a(#[from] runtime_command_bridge::Stage7aBridgeError),
    #[error("Redis command failed")]
    Redis(#[from] redis::RedisError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn stage7b_d_c_b065_b066_composite_readiness_requires_independent_inputs() {
        let supervisor = Stage7bServiceSupervisor::default();
        let now = Utc::now();
        supervisor.task.mark_started();
        supervisor.mark_storage(true);
        supervisor.mark_settlement(true);
        supervisor.mark_pending_count(0);
        supervisor.mark_source_success(now);
        supervisor.mark_claim_success(now);
        let (_, ready) = supervisor.snapshots(now, chrono::Duration::seconds(5));
        assert_eq!(ready.phase, Stage7bPaperReadinessPhase::PaperReady);

        supervisor.mark_claim_success(now - chrono::Duration::seconds(10));
        let (_, stale_claim) = supervisor.snapshots(now, chrono::Duration::seconds(5));
        assert_eq!(stale_claim.phase, Stage7bPaperReadinessPhase::Degraded);
        assert!(stale_claim
            .reasons
            .contains(&Stage7bPaperReadinessReason::ClaimScanStale));
        assert!(!stale_claim
            .reasons
            .contains(&Stage7bPaperReadinessReason::SourcePollStale));

        supervisor.mark_claim_success(now);
        supervisor.mark_source_success(now - chrono::Duration::seconds(10));
        let (_, stale_source) = supervisor.snapshots(now, chrono::Duration::seconds(5));
        assert!(stale_source
            .reasons
            .contains(&Stage7bPaperReadinessReason::SourcePollStale));
        assert!(!stale_source
            .reasons
            .contains(&Stage7bPaperReadinessReason::ClaimScanStale));

        supervisor.mark_source_success(now);
        supervisor.mark_storage(false);
        let (_, storage_bad) = supervisor.snapshots(now, chrono::Duration::seconds(5));
        assert!(storage_bad
            .reasons
            .contains(&Stage7bPaperReadinessReason::StorageUnavailable));
        assert_ne!(storage_bad.phase, Stage7bPaperReadinessPhase::PaperReady);
    }

    #[tokio::test]
    async fn stage7b_d_c_b067_supervision_clears_normal_error_panic_and_abort() {
        let normal = Stage7bTaskReadinessHandle::default();
        let join = spawn_stage7b_supervised_task(normal.clone(), async {});
        join.await.unwrap();
        assert!(!normal.is_alive());

        let returned_error = Stage7bTaskReadinessHandle::default();
        let join = spawn_stage7b_supervised_task(returned_error.clone(), async {
            Result::<(), &'static str>::Err("expected")
        });
        assert!(join.await.unwrap().is_err());
        assert!(!returned_error.is_alive());

        let panicked = Stage7bTaskReadinessHandle::default();
        let join = spawn_stage7b_supervised_task(panicked.clone(), async {
            panic!("expected Stage 7B supervisor panic test")
        });
        assert!(join.await.is_err());
        assert!(!panicked.is_alive());

        let aborted = Stage7bTaskReadinessHandle::default();
        let join = spawn_stage7b_supervised_task(aborted.clone(), async {
            std::future::pending::<()>().await
        });
        assert!(aborted.is_alive());
        join.abort();
        assert!(join.await.is_err());
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert!(!aborted.is_alive());
    }

    #[test]
    fn stage7b_d_c_b068_each_boot_gets_new_consumer_identity() {
        let first = Stage7bRedisServiceConfig::paper_default_auto("boot-identity").unwrap();
        let second = Stage7bRedisServiceConfig::paper_default_auto("boot-identity").unwrap();
        assert_ne!(first.consumer_name, second.consumer_name);
        assert_eq!(first.command_stream, second.command_stream);
        assert_eq!(first.consumer_group, second.consumer_group);
    }

    #[test]
    fn stage7b_d_c_b070_has_no_legacy_execution_authority_dependency() {
        let manifest = include_str!("../../Cargo.toml").to_ascii_lowercase();
        assert!(!manifest.contains("rusqlite"));
        assert!(!manifest.contains("sqlx"));
        assert!(!manifest.contains("m3"));
    }
}
