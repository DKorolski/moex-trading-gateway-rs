//! Stage 8B-P1-e newest-only signed schedule source.
//!
//! Schedule evidence is deliberately not a consumer-group workload.  The
//! production reader performs one bounded `XREVRANGE + - COUNT 64`, validates
//! the newest candidate without fallback and authenticates the retained
//! window only for conflict/high-water detection. Reading or verifying a
//! snapshot grants no strategy authority; authority is created only after the
//! exact transition is appended as Stage 6 V4 and covered by a reread seal.

use std::time::Duration as StdDuration;

use chrono::{DateTime, Utc};
use redis::{aio::ConnectionManager, streams::StreamRangeReply};
use strategy_runtime_core::{
    authenticate_stage8b_p1e_schedule_observation_v3, verify_stage8b_p1e_schedule_envelope_v3,
    Stage5gLifecycleCommitmentKey, Stage8bP1eAcceptedScheduleSourceV1, Stage8bP1eM10IdentityV1,
    Stage8bP1eScheduleHighWaterV1, Stage8bP1eScheduleSourceError,
    Stage8bP1eScheduleVerificationContextV1, STAGE8B_P1E_SCHEDULE_STREAM,
};

use crate::{
    Stage7bRecoveryError, Stage7bRecoveryReadyOwner, Stage8bP1eScheduleBindingCommittedOwner,
    Stage8bP1eShutdownIntentV1, Stage8bP1eShutdownLatchV1, STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS,
};

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1eScheduleReadError {
    #[error("Redis schedule read failed")]
    Redis(#[from] redis::RedisError),
    #[error("Redis schedule read exceeded its bounded operation timeout")]
    OperationTimeout,
    #[error("schedule stream reply is not an exact newest-only payload")]
    InvalidRedisReply,
    #[error("signed schedule source was rejected: {0}")]
    Source(#[from] Stage8bP1eScheduleSourceError),
    #[error("durable schedule binding failed: {0}")]
    Durable(#[from] Stage7bRecoveryError),
}

/// Opaque verified snapshot. It is intentionally non-clone and carries the
/// exact Redis entry identity beside the exact signed envelope retained by the
/// core verifier.
///
/// ```compile_fail
/// fn assert_clone<T: Clone>() {}
/// assert_clone::<runtime_durable_service::Stage8bP1eVerifiedScheduleSnapshotV1>();
/// ```
///
/// ```compile_fail
/// fn assert_serialize<T: serde::Serialize>() {}
/// assert_serialize::<runtime_durable_service::Stage8bP1eVerifiedScheduleSnapshotV1>();
/// ```
pub struct Stage8bP1eVerifiedScheduleSnapshotV1 {
    redis_stream_id: String,
    accepted: Stage8bP1eAcceptedScheduleSourceV1,
}

impl Stage8bP1eVerifiedScheduleSnapshotV1 {
    pub fn redis_stream_id(&self) -> &str {
        &self.redis_stream_id
    }

    pub fn high_water(&self) -> &Stage8bP1eScheduleHighWaterV1 {
        self.accepted.high_water()
    }

    pub fn envelope_sha256(&self) -> &str {
        self.accepted.envelope_sha256()
    }

    pub fn trading_day(&self) -> &str {
        self.accepted.trading_day()
    }
}

pub enum Stage8bP1eNewestScheduleReadV1 {
    Empty,
    Verified(Box<Stage8bP1eVerifiedScheduleSnapshotV1>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eScheduleLatchCheckpointV1 {
    BeforeScheduleRead,
    AfterScheduleReadBeforeBinding,
    AfterBinding,
    BeforeEffect,
}

/// Diagnostic-only proof that schedule work stopped at one exact latch
/// checkpoint. It contains neither verified source bytes nor authority.
pub struct Stage8bP1eScheduleStopReceiptV1 {
    checkpoint: Stage8bP1eScheduleLatchCheckpointV1,
    shutdown_intent: Stage8bP1eShutdownIntentV1,
}

impl Stage8bP1eScheduleStopReceiptV1 {
    pub const fn checkpoint(&self) -> Stage8bP1eScheduleLatchCheckpointV1 {
        self.checkpoint
    }

    pub fn shutdown_intent(&self) -> &Stage8bP1eShutdownIntentV1 {
        &self.shutdown_intent
    }
}

pub enum Stage8bP1eGuardedScheduleReadV1 {
    Stopped(Stage8bP1eScheduleStopReceiptV1),
    Read(Stage8bP1eNewestScheduleReadV1),
}

/// Linear boundary returned only after the V4 append, covering seal and
/// authenticated reread have all completed while latch E remained clear.
///
/// ```compile_fail
/// fn assert_clone<T: Clone>() {}
/// assert_clone::<runtime_durable_service::Stage8bP1ePostBindingPermitV1>();
/// ```
///
/// ```compile_fail
/// fn assert_deserialize<T: serde::de::DeserializeOwned>() {}
/// assert_deserialize::<runtime_durable_service::Stage8bP1ePostBindingPermitV1>();
/// ```
pub struct Stage8bP1ePostBindingPermitV1 {
    committed: Box<Stage8bP1eScheduleBindingCommittedOwner>,
}

impl Stage8bP1ePostBindingPermitV1 {
    pub fn receipt(&self) -> &crate::Stage8bP1eScheduleBindingCommitReceipt {
        self.committed.receipt()
    }
}

pub enum Stage8bP1eScheduleBindingDecisionV1 {
    StoppedBeforeBinding {
        owner: Box<Stage7bRecoveryReadyOwner>,
        receipt: Stage8bP1eScheduleStopReceiptV1,
    },
    StoppedAfterBinding {
        owner: Box<Stage8bP1eScheduleBindingCommittedOwner>,
        receipt: Stage8bP1eScheduleStopReceiptV1,
    },
    Continue(Stage8bP1ePostBindingPermitV1),
}

/// First half of the binding boundary. `Committed` is deliberately
/// non-authorizing: the caller must observe retained shutdown state through
/// `resume_stage8b_p1e_committed_schedule_binding` before any route permit can
/// exist.
pub enum Stage8bP1eScheduleBindingCommitV1 {
    StoppedBeforeBinding {
        owner: Box<Stage7bRecoveryReadyOwner>,
        receipt: Stage8bP1eScheduleStopReceiptV1,
    },
    Committed(Box<Stage8bP1eScheduleBindingCommittedOwner>),
}

pub enum Stage8bP1eScheduleAuthorityDecisionV1<T> {
    RetainForRestart {
        owner: Box<Stage8bP1eScheduleBindingCommittedOwner>,
        receipt: Stage8bP1eScheduleStopReceiptV1,
    },
    Continue {
        owner: Box<Stage7bRecoveryReadyOwner>,
        authority: T,
    },
}

fn stop_receipt(
    latch: &Stage8bP1eShutdownLatchV1,
    checkpoint: Stage8bP1eScheduleLatchCheckpointV1,
) -> Option<Stage8bP1eScheduleStopReceiptV1> {
    latch
        .intent()
        .cloned()
        .map(|shutdown_intent| Stage8bP1eScheduleStopReceiptV1 {
            checkpoint,
            shutdown_intent,
        })
}

fn check_latch_d(
    owner: Stage7bRecoveryReadyOwner,
    latch: &Stage8bP1eShutdownLatchV1,
) -> Result<Stage7bRecoveryReadyOwner, Stage8bP1eScheduleBindingDecisionV1> {
    match stop_receipt(
        latch,
        Stage8bP1eScheduleLatchCheckpointV1::AfterScheduleReadBeforeBinding,
    ) {
        Some(receipt) => Err(Stage8bP1eScheduleBindingDecisionV1::StoppedBeforeBinding {
            owner: Box::new(owner),
            receipt,
        }),
        None => Ok(owner),
    }
}

pub fn resume_stage8b_p1e_committed_schedule_binding(
    committed: Stage8bP1eScheduleBindingCommittedOwner,
    latch: &Stage8bP1eShutdownLatchV1,
) -> Stage8bP1eScheduleBindingDecisionV1 {
    match stop_receipt(latch, Stage8bP1eScheduleLatchCheckpointV1::AfterBinding) {
        Some(receipt) => Stage8bP1eScheduleBindingDecisionV1::StoppedAfterBinding {
            owner: Box::new(committed),
            receipt,
        },
        None => Stage8bP1eScheduleBindingDecisionV1::Continue(Stage8bP1ePostBindingPermitV1 {
            committed: Box::new(committed),
        }),
    }
}

/// Dedicated read-only Redis transport. It contains no group, PEL, XACK or
/// command-publication API.
pub struct Stage8bP1eRedisScheduleReader {
    connection: ConnectionManager,
    #[cfg(test)]
    read_attempts: usize,
    #[cfg(all(test, feature = "stage8a4-i3-test-fixtures"))]
    fixture_trust: Option<Stage8bP1eFixtureScheduleTrust>,
}

#[cfg(all(test, feature = "stage8a4-i3-test-fixtures"))]
struct Stage8bP1eFixtureScheduleTrust {
    public_key_hex: String,
    key_valid_from: DateTime<Utc>,
    key_valid_until: DateTime<Utc>,
}

impl Stage8bP1eRedisScheduleReader {
    pub async fn connect(redis_url: &str) -> Result<Self, Stage8bP1eScheduleReadError> {
        let client = redis::Client::open(redis_url)?;
        let connection = tokio::time::timeout(
            StdDuration::from_millis(STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
            ConnectionManager::new(client),
        )
        .await
        .map_err(|_| Stage8bP1eScheduleReadError::OperationTimeout)??;
        Ok(Self {
            connection,
            #[cfg(test)]
            read_attempts: 0,
            #[cfg(all(test, feature = "stage8a4-i3-test-fixtures"))]
            fixture_trust: None,
        })
    }

    #[cfg(all(test, feature = "stage8a4-i3-test-fixtures"))]
    pub(crate) async fn test_connect_with_fixture_trust(
        redis_url: &str,
        public_key_hex: String,
        key_valid_from: DateTime<Utc>,
        key_valid_until: DateTime<Utc>,
    ) -> Result<Self, Stage8bP1eScheduleReadError> {
        let mut reader = Self::connect(redis_url).await?;
        reader.fixture_trust = Some(Stage8bP1eFixtureScheduleTrust {
            public_key_hex,
            key_valid_from,
            key_valid_until,
        });
        Ok(reader)
    }

    async fn read_newest_with_timeout(
        &mut self,
        context: &Stage8bP1eScheduleVerificationContextV1,
        operation_timeout: StdDuration,
    ) -> Result<Stage8bP1eNewestScheduleReadV1, Stage8bP1eScheduleReadError> {
        #[cfg(test)]
        {
            self.read_attempts += 1;
        }
        let reply: StreamRangeReply = tokio::time::timeout(
            operation_timeout,
            redis::cmd("XREVRANGE")
                .arg(STAGE8B_P1E_SCHEDULE_STREAM)
                .arg("+")
                .arg("-")
                .arg("COUNT")
                .arg(64)
                .query_async(&mut self.connection),
        )
        .await
        .map_err(|_| Stage8bP1eScheduleReadError::OperationTimeout)??;
        #[cfg(all(test, feature = "stage8a4-i3-test-fixtures"))]
        if let Some(trust) = &self.fixture_trust {
            return verify_newest_reply_with_fixture_key(
                reply,
                context,
                &trust.public_key_hex,
                trust.key_valid_from,
                trust.key_valid_until,
            );
        }
        verify_newest_reply(reply, context)
    }

    pub(crate) async fn read_newest_guarded_with_timeout(
        &mut self,
        context: &Stage8bP1eScheduleVerificationContextV1,
        latch: &Stage8bP1eShutdownLatchV1,
        operation_timeout: StdDuration,
    ) -> Result<Stage8bP1eGuardedScheduleReadV1, Stage8bP1eScheduleReadError> {
        if let Some(receipt) = stop_receipt(
            latch,
            Stage8bP1eScheduleLatchCheckpointV1::BeforeScheduleRead,
        ) {
            return Ok(Stage8bP1eGuardedScheduleReadV1::Stopped(receipt));
        }
        Ok(Stage8bP1eGuardedScheduleReadV1::Read(
            self.read_newest_with_timeout(context, operation_timeout)
                .await?,
        ))
    }

    /// Latch C is evaluated before any Redis command. The result of a
    /// successful read is still non-authorizing and must pass latch D at the
    /// binding boundary.
    pub async fn read_newest_guarded(
        &mut self,
        context: &Stage8bP1eScheduleVerificationContextV1,
        latch: &Stage8bP1eShutdownLatchV1,
    ) -> Result<Stage8bP1eGuardedScheduleReadV1, Stage8bP1eScheduleReadError> {
        self.read_newest_guarded_with_timeout(
            context,
            latch,
            StdDuration::from_millis(STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
        )
        .await
    }

    #[cfg(test)]
    pub(crate) const fn test_read_attempts(&self) -> usize {
        self.read_attempts
    }
}

fn verify_newest_reply(
    reply: StreamRangeReply,
    context: &Stage8bP1eScheduleVerificationContextV1,
) -> Result<Stage8bP1eNewestScheduleReadV1, Stage8bP1eScheduleReadError> {
    let rows = parse_newest_reply(reply)?;
    if rows.is_empty() {
        return Ok(Stage8bP1eNewestScheduleReadV1::Empty);
    }
    let (newest_redis_id, exact_envelope_bytes) = &rows[0];
    let accepted = verify_stage8b_p1e_schedule_envelope_v3(exact_envelope_bytes, context)?;
    let observations = rows
        .iter()
        .skip(1)
        .map(|(_, bytes)| authenticate_stage8b_p1e_schedule_observation_v3(bytes, context))
        .collect::<Result<Vec<_>, _>>()?;
    finish_verified_newest_reply(newest_redis_id, accepted, observations, context)
}

#[cfg(all(test, feature = "stage8a4-i3-test-fixtures"))]
#[allow(clippy::too_many_arguments)]
fn verify_newest_reply_with_fixture_key(
    reply: StreamRangeReply,
    context: &Stage8bP1eScheduleVerificationContextV1,
    public_key_hex: &str,
    key_valid_from: DateTime<Utc>,
    key_valid_until: DateTime<Utc>,
) -> Result<Stage8bP1eNewestScheduleReadV1, Stage8bP1eScheduleReadError> {
    let rows = parse_newest_reply(reply)?;
    if rows.is_empty() {
        return Ok(Stage8bP1eNewestScheduleReadV1::Empty);
    }
    let (newest_redis_id, exact_envelope_bytes) = &rows[0];
    let accepted = strategy_runtime_core::stage8b_p1e_test_verify_schedule_envelope_with_key(
        exact_envelope_bytes,
        context,
        public_key_hex,
        key_valid_from,
        key_valid_until,
    )?;
    let observations = rows
        .iter()
        .skip(1)
        .map(|(_, bytes)| {
            strategy_runtime_core::stage8b_p1e_test_authenticate_schedule_observation_with_key(
                bytes,
                context,
                public_key_hex,
                key_valid_from,
                key_valid_until,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    finish_verified_newest_reply(newest_redis_id, accepted, observations, context)
}

fn parse_newest_reply(
    reply: StreamRangeReply,
) -> Result<Vec<(String, Vec<u8>)>, Stage8bP1eScheduleReadError> {
    if reply.ids.len() > 64 {
        return Err(Stage8bP1eScheduleReadError::InvalidRedisReply);
    }
    let mut rows = Vec::with_capacity(reply.ids.len());
    let mut prior_redis_id = None;
    for entry in &reply.ids {
        let redis_id = parse_redis_stream_id(&entry.id)
            .ok_or(Stage8bP1eScheduleReadError::InvalidRedisReply)?;
        if prior_redis_id.is_some_and(|prior| redis_id >= prior) || entry.map.len() != 1 {
            return Err(Stage8bP1eScheduleReadError::InvalidRedisReply);
        }
        let payload = entry
            .get::<Vec<u8>>("payload")
            .filter(|value| !value.is_empty())
            .ok_or(Stage8bP1eScheduleReadError::InvalidRedisReply)?;
        rows.push((entry.id.clone(), payload));
        prior_redis_id = Some(redis_id);
    }
    Ok(rows)
}

fn finish_verified_newest_reply(
    newest_redis_id: &str,
    accepted: Stage8bP1eAcceptedScheduleSourceV1,
    observations: Vec<strategy_runtime_core::Stage8bP1eAuthenticatedScheduleObservationV1>,
    context: &Stage8bP1eScheduleVerificationContextV1,
) -> Result<Stage8bP1eNewestScheduleReadV1, Stage8bP1eScheduleReadError> {
    let mut newest_to_oldest = vec![ScheduleWindowPoint::from_high_water(accepted.high_water())];
    for observation in observations {
        newest_to_oldest.push(ScheduleWindowPoint::from_high_water(
            observation.high_water(),
        ));
    }
    validate_bounded_progression(
        &newest_to_oldest,
        context
            .high_water
            .as_ref()
            .map(ScheduleWindowPoint::from_high_water),
    )?;
    Ok(Stage8bP1eNewestScheduleReadV1::Verified(Box::new(
        Stage8bP1eVerifiedScheduleSnapshotV1 {
            redis_stream_id: newest_redis_id.to_string(),
            accepted,
        },
    )))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScheduleWindowPoint {
    envelope_sha256: String,
    publication_sequence: u64,
    published_at_utc: DateTime<Utc>,
    schedule_semantic_sha256: String,
    semantic_revision: u64,
    source_generation: u64,
}

impl ScheduleWindowPoint {
    fn from_high_water(value: &Stage8bP1eScheduleHighWaterV1) -> Self {
        Self {
            envelope_sha256: value.envelope_sha256().to_string(),
            publication_sequence: value.publication_sequence(),
            published_at_utc: value.published_at_utc(),
            schedule_semantic_sha256: value.schedule_semantic_sha256().to_string(),
            semantic_revision: value.semantic_revision(),
            source_generation: value.source_generation(),
        }
    }
}

fn validate_bounded_progression(
    newest_to_oldest: &[ScheduleWindowPoint],
    durable_high_water: Option<ScheduleWindowPoint>,
) -> Result<(), Stage8bP1eScheduleReadError> {
    let conflict =
        || Stage8bP1eScheduleReadError::Source(Stage8bP1eScheduleSourceError::ProgressionConflict);
    for pair in newest_to_oldest.windows(2) {
        let newer = &pair[0];
        let older = &pair[1];
        if older.source_generation != newer.source_generation
            || older.publication_sequence > newer.publication_sequence
            || older.published_at_utc > newer.published_at_utc
            || older.semantic_revision > newer.semantic_revision
            || (older.publication_sequence == newer.publication_sequence && older != newer)
            || (older.publication_sequence < newer.publication_sequence
                && older.published_at_utc >= newer.published_at_utc)
            || (older.semantic_revision == newer.semantic_revision
                && older.schedule_semantic_sha256 != newer.schedule_semantic_sha256)
        {
            return Err(conflict());
        }
    }
    if let Some(high_water) = durable_high_water {
        for retained in newest_to_oldest.iter().filter(|retained| {
            retained.source_generation == high_water.source_generation
                && retained.publication_sequence == high_water.publication_sequence
        }) {
            if retained != &high_water {
                return Err(conflict());
            }
        }
    }
    Ok(())
}

/// The only public Market binding composition. The source snapshot is consumed
/// and cannot be reused after the non-cancellable V4 append/seal boundary.
#[allow(clippy::too_many_arguments)]
pub fn commit_stage8b_p1e_market_schedule(
    owner: Stage7bRecoveryReadyOwner,
    snapshot: Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    predecessor: &Stage8bP1eM10IdentityV1,
    candidate: &Stage8bP1eM10IdentityV1,
    strategy_request_id: impl Into<String>,
    canonical_command_sha256: impl Into<String>,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleBindingDecisionV1, Stage8bP1eScheduleReadError> {
    finish_binding_commit(
        bind_stage8b_p1e_market_schedule(
            owner,
            snapshot,
            latch,
            predecessor,
            candidate,
            strategy_request_id,
            canonical_command_sha256,
            bound_at_utc,
            commitment_key,
        )?,
        latch,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn bind_stage8b_p1e_market_schedule(
    owner: Stage7bRecoveryReadyOwner,
    snapshot: Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    predecessor: &Stage8bP1eM10IdentityV1,
    candidate: &Stage8bP1eM10IdentityV1,
    strategy_request_id: impl Into<String>,
    canonical_command_sha256: impl Into<String>,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleBindingCommitV1, Stage8bP1eScheduleReadError> {
    let owner = match binding_owner_after_latch_d(owner, latch) {
        Ok(owner) => owner,
        Err(commit) => return Ok(commit),
    };
    let binding = snapshot.accepted.prepare_market_binding(
        predecessor,
        candidate,
        strategy_request_id,
        canonical_command_sha256,
        snapshot.redis_stream_id,
    )?;
    commit_binding_only(owner, binding, bound_at_utc, commitment_key)
}

/// The only public initial-LIMIT schedule-step binding composition. Its
/// request/command identity is bound like Market while the resulting one-use
/// authority is restricted to the initial LIMIT transition.
#[allow(clippy::too_many_arguments)]
pub fn commit_stage8b_p1e_initial_limit_schedule(
    owner: Stage7bRecoveryReadyOwner,
    snapshot: Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    predecessor: &Stage8bP1eM10IdentityV1,
    candidate: &Stage8bP1eM10IdentityV1,
    strategy_request_id: impl Into<String>,
    canonical_command_sha256: impl Into<String>,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleBindingDecisionV1, Stage8bP1eScheduleReadError> {
    finish_binding_commit(
        bind_stage8b_p1e_initial_limit_schedule(
            owner,
            snapshot,
            latch,
            predecessor,
            candidate,
            strategy_request_id,
            canonical_command_sha256,
            bound_at_utc,
            commitment_key,
        )?,
        latch,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn bind_stage8b_p1e_initial_limit_schedule(
    owner: Stage7bRecoveryReadyOwner,
    snapshot: Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    predecessor: &Stage8bP1eM10IdentityV1,
    candidate: &Stage8bP1eM10IdentityV1,
    strategy_request_id: impl Into<String>,
    canonical_command_sha256: impl Into<String>,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleBindingCommitV1, Stage8bP1eScheduleReadError> {
    let owner = match binding_owner_after_latch_d(owner, latch) {
        Ok(owner) => owner,
        Err(commit) => return Ok(commit),
    };
    let binding = snapshot.accepted.prepare_initial_limit_binding(
        predecessor,
        candidate,
        strategy_request_id,
        canonical_command_sha256,
        snapshot.redis_stream_id,
    )?;
    commit_binding_only(owner, binding, bound_at_utc, commitment_key)
}

/// The only public working-LIMIT schedule-step binding composition.
#[allow(clippy::too_many_arguments)]
pub fn commit_stage8b_p1e_working_limit_schedule(
    owner: Stage7bRecoveryReadyOwner,
    snapshot: Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    predecessor: &Stage8bP1eM10IdentityV1,
    candidate: &Stage8bP1eM10IdentityV1,
    active_broker_order_id: impl Into<String>,
    working_book_transition_sha256: impl Into<String>,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleBindingDecisionV1, Stage8bP1eScheduleReadError> {
    finish_binding_commit(
        bind_stage8b_p1e_working_limit_schedule(
            owner,
            snapshot,
            latch,
            predecessor,
            candidate,
            active_broker_order_id,
            working_book_transition_sha256,
            bound_at_utc,
            commitment_key,
        )?,
        latch,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn bind_stage8b_p1e_working_limit_schedule(
    owner: Stage7bRecoveryReadyOwner,
    snapshot: Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    predecessor: &Stage8bP1eM10IdentityV1,
    candidate: &Stage8bP1eM10IdentityV1,
    active_broker_order_id: impl Into<String>,
    working_book_transition_sha256: impl Into<String>,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleBindingCommitV1, Stage8bP1eScheduleReadError> {
    let owner = match binding_owner_after_latch_d(owner, latch) {
        Ok(owner) => owner,
        Err(commit) => return Ok(commit),
    };
    let binding = snapshot.accepted.prepare_working_limit_binding(
        predecessor,
        candidate,
        active_broker_order_id,
        working_book_transition_sha256,
        snapshot.redis_stream_id,
    )?;
    commit_binding_only(owner, binding, bound_at_utc, commitment_key)
}

/// The only public cancel schedule-step binding composition.
#[allow(clippy::too_many_arguments)]
pub fn commit_stage8b_p1e_cancel_schedule(
    owner: Stage7bRecoveryReadyOwner,
    snapshot: Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    predecessor: &Stage8bP1eM10IdentityV1,
    candidate: &Stage8bP1eM10IdentityV1,
    active_broker_order_id: impl Into<String>,
    working_book_transition_sha256: impl Into<String>,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleBindingDecisionV1, Stage8bP1eScheduleReadError> {
    finish_binding_commit(
        bind_stage8b_p1e_cancel_schedule(
            owner,
            snapshot,
            latch,
            predecessor,
            candidate,
            active_broker_order_id,
            working_book_transition_sha256,
            bound_at_utc,
            commitment_key,
        )?,
        latch,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn bind_stage8b_p1e_cancel_schedule(
    owner: Stage7bRecoveryReadyOwner,
    snapshot: Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    predecessor: &Stage8bP1eM10IdentityV1,
    candidate: &Stage8bP1eM10IdentityV1,
    active_broker_order_id: impl Into<String>,
    working_book_transition_sha256: impl Into<String>,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleBindingCommitV1, Stage8bP1eScheduleReadError> {
    let owner = match binding_owner_after_latch_d(owner, latch) {
        Ok(owner) => owner,
        Err(commit) => return Ok(commit),
    };
    let binding = snapshot.accepted.prepare_cancel_binding(
        predecessor,
        candidate,
        active_broker_order_id,
        working_book_transition_sha256,
        snapshot.redis_stream_id,
    )?;
    commit_binding_only(owner, binding, bound_at_utc, commitment_key)
}

/// The only public day-expiry binding composition.
#[allow(clippy::too_many_arguments)]
pub fn commit_stage8b_p1e_day_expiry_schedule(
    owner: Stage7bRecoveryReadyOwner,
    snapshot: Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    predecessor: &Stage8bP1eM10IdentityV1,
    last_evaluated_m10: &Stage8bP1eM10IdentityV1,
    trusted_now: DateTime<Utc>,
    active_broker_order_id: impl Into<String>,
    working_book_transition_sha256: impl Into<String>,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleBindingDecisionV1, Stage8bP1eScheduleReadError> {
    finish_binding_commit(
        bind_stage8b_p1e_day_expiry_schedule(
            owner,
            snapshot,
            latch,
            predecessor,
            last_evaluated_m10,
            trusted_now,
            active_broker_order_id,
            working_book_transition_sha256,
            bound_at_utc,
            commitment_key,
        )?,
        latch,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn bind_stage8b_p1e_day_expiry_schedule(
    owner: Stage7bRecoveryReadyOwner,
    snapshot: Stage8bP1eVerifiedScheduleSnapshotV1,
    latch: &Stage8bP1eShutdownLatchV1,
    predecessor: &Stage8bP1eM10IdentityV1,
    last_evaluated_m10: &Stage8bP1eM10IdentityV1,
    trusted_now: DateTime<Utc>,
    active_broker_order_id: impl Into<String>,
    working_book_transition_sha256: impl Into<String>,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleBindingCommitV1, Stage8bP1eScheduleReadError> {
    let owner = match binding_owner_after_latch_d(owner, latch) {
        Ok(owner) => owner,
        Err(commit) => return Ok(commit),
    };
    let binding = snapshot.accepted.prepare_day_expiry_binding(
        predecessor,
        last_evaluated_m10,
        trusted_now,
        active_broker_order_id,
        working_book_transition_sha256,
        snapshot.redis_stream_id,
    )?;
    commit_binding_only(owner, binding, bound_at_utc, commitment_key)
}

fn binding_owner_after_latch_d(
    owner: Stage7bRecoveryReadyOwner,
    latch: &Stage8bP1eShutdownLatchV1,
) -> Result<Stage7bRecoveryReadyOwner, Stage8bP1eScheduleBindingCommitV1> {
    match check_latch_d(owner, latch) {
        Ok(owner) => Ok(owner),
        Err(Stage8bP1eScheduleBindingDecisionV1::StoppedBeforeBinding { owner, receipt }) => {
            Err(Stage8bP1eScheduleBindingCommitV1::StoppedBeforeBinding { owner, receipt })
        }
        Err(_) => unreachable!("latch D can only stop before binding"),
    }
}

fn commit_binding_only(
    owner: Stage7bRecoveryReadyOwner,
    binding: strategy_runtime_core::Stage8bP1eScheduleBindingCandidateV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleBindingCommitV1, Stage8bP1eScheduleReadError> {
    let committed =
        owner.commit_stage8b_p1e_schedule_binding(binding, bound_at_utc, commitment_key)?;
    Ok(Stage8bP1eScheduleBindingCommitV1::Committed(Box::new(
        committed,
    )))
}

fn finish_binding_commit(
    commit: Stage8bP1eScheduleBindingCommitV1,
    latch: &Stage8bP1eShutdownLatchV1,
) -> Result<Stage8bP1eScheduleBindingDecisionV1, Stage8bP1eScheduleReadError> {
    Ok(match commit {
        Stage8bP1eScheduleBindingCommitV1::StoppedBeforeBinding { owner, receipt } => {
            Stage8bP1eScheduleBindingDecisionV1::StoppedBeforeBinding { owner, receipt }
        }
        Stage8bP1eScheduleBindingCommitV1::Committed(owner) => {
            resume_stage8b_p1e_committed_schedule_binding(*owner, latch)
        }
    })
}

macro_rules! define_authority_continuation {
    ($function:ident, $authority:ty, $method:ident) => {
        pub fn $function(
            permit: Stage8bP1ePostBindingPermitV1,
            latch: &Stage8bP1eShutdownLatchV1,
        ) -> Result<Stage8bP1eScheduleAuthorityDecisionV1<$authority>, Stage8bP1eScheduleReadError>
        {
            if let Some(receipt) =
                stop_receipt(latch, Stage8bP1eScheduleLatchCheckpointV1::BeforeEffect)
            {
                return Ok(Stage8bP1eScheduleAuthorityDecisionV1::RetainForRestart {
                    owner: permit.committed,
                    receipt,
                });
            }
            let (owner, authority) = permit.committed.$method()?;
            Ok(Stage8bP1eScheduleAuthorityDecisionV1::Continue {
                owner: Box::new(owner),
                authority,
            })
        }
    };
}

define_authority_continuation!(
    continue_stage8b_p1e_market_schedule,
    strategy_runtime_core::Stage8bP1d1ExecutionScheduleAuthority,
    into_market_authority
);
define_authority_continuation!(
    continue_stage8b_p1e_schedule_step,
    strategy_runtime_core::Stage8bP1d3ScheduleStepAuthority,
    into_schedule_step_authority
);
define_authority_continuation!(
    continue_stage8b_p1e_day_expiry_schedule,
    strategy_runtime_core::Stage8bP1d3DayExpiryAuthority,
    into_day_expiry_authority
);

fn parse_redis_stream_id(value: &str) -> Option<(u64, u64)> {
    let (milliseconds, sequence) = value.split_once('-')?;
    if milliseconds.is_empty()
        || milliseconds.len() > 20
        || milliseconds.as_bytes()[0] == b'0'
        || !milliseconds.bytes().all(|byte| byte.is_ascii_digit())
        || sequence.is_empty()
        || sequence.len() > 20
        || sequence.contains('-')
        || !sequence.bytes().all(|byte| byte.is_ascii_digit())
        || (sequence.len() > 1 && sequence.as_bytes()[0] == b'0')
    {
        return None;
    }
    Some((milliseconds.parse().ok()?, sequence.parse().ok()?))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    use broker_core::{
        BrokerAccountId, BrokerCommand, BrokerInstrumentSpec, BrokerKind, BrokerMarketSessionState,
        BrokerSymbol, BrokerTruthSnapshot, ClientOrderId, Exchange, InstrumentId,
        InstrumentMapEntry, InternalSymbol, Market, Money, OrderSide, OrderType, PlaceOrder,
        Stage4AdoptionDisposition, Stage4BootstrapEvidenceSourceStatusSection,
        Stage4BrokerTruthBootstrapInput, Stage4BrokerTruthFreshnessInput,
        Stage4BrokerTruthSafetyBoundary, Stage4BrokerTruthSourceStatus, StrategyRequestId,
        TimeInForce,
    };
    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    use chrono::TimeZone;
    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    use ed25519_dalek::{Signer, SigningKey};
    use redis::{streams::StreamId, Value};
    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    use rust_decimal::Decimal;
    use std::collections::HashMap;
    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    use std::os::unix::fs::DirBuilderExt;
    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    static SCHEDULE_TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    const P1E_TEST_PLACE_DECISION_CLOSE_MS: i64 = 1_785_760_200_000;
    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    const P1E_TEST_INITIAL_CANDIDATE_CLOSE_MS: i64 = 1_785_760_800_000;
    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    const P1E_TEST_LATER_CANDIDATE_CLOSE_MS: i64 = 1_785_761_400_000;

    fn entry(id: &str, fields: &[(&str, &[u8])]) -> StreamRangeReply {
        let map = fields
            .iter()
            .map(|(key, value)| ((*key).to_string(), Value::BulkString((*value).to_vec())))
            .collect::<HashMap<_, _>>();
        StreamRangeReply {
            ids: vec![StreamId {
                id: id.to_string(),
                map,
            }],
        }
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn schedule_reply(rows: &[(&str, &[u8])]) -> StreamRangeReply {
        StreamRangeReply {
            ids: rows
                .iter()
                .map(|(id, payload)| StreamId {
                    id: (*id).to_string(),
                    map: HashMap::from([(
                        "payload".to_string(),
                        Value::BulkString((*payload).to_vec()),
                    )]),
                })
                .collect(),
        }
    }

    fn unused_context() -> Stage8bP1eScheduleVerificationContextV1 {
        Stage8bP1eScheduleVerificationContextV1 {
            expected_instrument_map_fingerprint_sha256: "1".repeat(64),
            expected_operational_identity_sha256: "2".repeat(64),
            expected_registry_identity_sha256: "3".repeat(64),
            expected_registry_version: "v1".to_string(),
            expected_runtime_config_fingerprint_sha256: "4".repeat(64),
            high_water: None,
            trusted_now: Utc::now(),
        }
    }

    fn point(sequence: u64, revision: u64, semantic: char, envelope: char) -> ScheduleWindowPoint {
        ScheduleWindowPoint {
            envelope_sha256: envelope.to_string().repeat(64),
            publication_sequence: sequence,
            published_at_utc: DateTime::parse_from_rfc3339(&format!(
                "2026-09-14T12:30:{:02}.000000Z",
                sequence % 60
            ))
            .unwrap()
            .with_timezone(&Utc),
            schedule_semantic_sha256: semantic.to_string().repeat(64),
            semantic_revision: revision,
            source_generation: 1,
        }
    }

    #[test]
    fn newest_only_reply_rejects_wrong_shape_before_trust_parsing() {
        let context = unused_context();
        assert!(matches!(
            verify_newest_reply(entry("1700000000000-0", &[("wrong", b"{}")]), &context),
            Err(Stage8bP1eScheduleReadError::InvalidRedisReply)
        ));
        assert!(matches!(
            verify_newest_reply(
                entry(
                    "1700000000000-0",
                    &[("payload", b"{}"), ("extra", b"forbidden")]
                ),
                &context
            ),
            Err(Stage8bP1eScheduleReadError::InvalidRedisReply)
        ));
        assert!(matches!(
            verify_newest_reply(entry("0-0", &[("payload", b"{}")]), &context),
            Err(Stage8bP1eScheduleReadError::InvalidRedisReply)
        ));
    }

    #[test]
    fn empty_newest_only_reply_is_non_authorizing() {
        assert!(matches!(
            verify_newest_reply(StreamRangeReply { ids: Vec::new() }, &unused_context()),
            Ok(Stage8bP1eNewestScheduleReadV1::Empty)
        ));
    }

    #[test]
    fn bounded_window_accepts_jumps_and_exact_idempotent_rows() {
        let newest = point(20, 2, 'b', '2');
        let duplicate = newest.clone();
        let older = point(10, 1, 'a', '1');
        assert!(
            validate_bounded_progression(&[newest, duplicate, older.clone()], Some(older),).is_ok()
        );
    }

    #[test]
    fn bounded_window_accepts_retained_a_to_b_to_a_and_omitted_middle_snapshot() {
        let newest_a = point(30, 9, 'a', '3');
        let middle_b = point(20, 8, 'b', '2');
        let oldest_a = point(10, 7, 'a', '1');
        assert!(validate_bounded_progression(
            &[newest_a.clone(), middle_b, oldest_a.clone()],
            Some(oldest_a.clone()),
        )
        .is_ok());
        assert!(validate_bounded_progression(&[newest_a], Some(oldest_a)).is_ok());
    }

    #[test]
    fn bounded_window_rejects_reorder_and_same_sequence_conflict() {
        let newest = point(20, 2, 'b', '2');
        let reordered = point(21, 2, 'b', '3');
        assert!(validate_bounded_progression(&[newest.clone(), reordered], None).is_err());

        let mut conflicting = newest.clone();
        conflicting.envelope_sha256 = "9".repeat(64);
        assert!(validate_bounded_progression(&[newest, conflicting], None).is_err());
    }

    #[test]
    fn bounded_window_rejects_nonadvancing_time_and_same_revision_hash_change() {
        let newer = point(20, 2, 'b', '2');
        let mut same_time = point(19, 1, 'a', '1');
        same_time.published_at_utc = newer.published_at_utc;
        assert!(validate_bounded_progression(&[newer.clone(), same_time], None).is_err());

        let same_revision_different_hash = point(19, 2, 'a', '1');
        assert!(
            validate_bounded_progression(&[newer, same_revision_different_hash], None).is_err()
        );
    }

    #[test]
    fn bounded_window_rejects_durable_high_water_conflict() {
        let retained = point(20, 2, 'b', '2');
        let mut high_water = retained.clone();
        high_water.envelope_sha256 = "9".repeat(64);
        assert!(validate_bounded_progression(&[retained], Some(high_water)).is_err());
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_reader_accepts_late_join_retention_and_a_b_a_snapshot_progression() {
        let (a7, b8, a9, context, public_key, key_from, key_until) = snapshot_progression_fixture();
        let accepted_a7 =
            strategy_runtime_core::stage8b_p1e_test_verify_schedule_envelope_with_key(
                &a7,
                &context,
                &public_key,
                key_from,
                key_until,
            )
            .unwrap();

        let late_join = verify_newest_reply_with_fixture_key(
            schedule_reply(&[("30-0", &a9)]),
            &context,
            &public_key,
            key_from,
            key_until,
        )
        .expect("a late consumer must accept newest revision after older retention");
        let Stage8bP1eNewestScheduleReadV1::Verified(late_join) = late_join else {
            panic!("late join must return the newest verified snapshot")
        };
        assert_eq!(late_join.high_water().semantic_revision(), 9);

        let mut progressed_context = context.clone();
        progressed_context.high_water = Some(accepted_a7.high_water().clone());
        let full_window = verify_newest_reply_with_fixture_key(
            schedule_reply(&[("30-0", &a9), ("20-0", &b8), ("10-0", &a7)]),
            &progressed_context,
            &public_key,
            key_from,
            key_until,
        )
        .expect("retained signed A-to-B-to-A progression must be accepted");
        let Stage8bP1eNewestScheduleReadV1::Verified(full_window) = full_window else {
            panic!("full retained window must return the newest snapshot")
        };
        assert_eq!(full_window.high_water().semantic_revision(), 9);

        let omitted_middle = verify_newest_reply_with_fixture_key(
            schedule_reply(&[("30-0", &a9), ("10-0", &a7)]),
            &progressed_context,
            &public_key,
            key_from,
            key_until,
        )
        .expect("consumer snapshot may omit an intermediate signed B revision");
        assert!(matches!(
            omitted_middle,
            Stage8bP1eNewestScheduleReadV1::Verified(_)
        ));
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_reader_rejects_revision_rollback_and_sequence_or_hash_conflicts() {
        let (a7, _b8, _a9, context, public_key, key_from, key_until) =
            snapshot_progression_fixture();
        let accepted_a7 =
            strategy_runtime_core::stage8b_p1e_test_verify_schedule_envelope_with_key(
                &a7,
                &context,
                &public_key,
                key_from,
                key_until,
            )
            .unwrap();
        let mut progressed_context = context.clone();
        progressed_context.high_water = Some(accepted_a7.high_water().clone());
        let now = context.trusted_now;

        let same_revision_different_hash = revised_schedule_envelope(&a7, 8, 7, now, 1);
        assert!(matches!(
            verify_newest_reply_with_fixture_key(
                schedule_reply(&[("20-0", &same_revision_different_hash)]),
                &progressed_context,
                &public_key,
                key_from,
                key_until,
            ),
            Err(Stage8bP1eScheduleReadError::Source(
                Stage8bP1eScheduleSourceError::ProgressionConflict
            ))
        ));

        let rollback = revised_schedule_envelope(&a7, 8, 6, now, 0);
        assert!(matches!(
            verify_newest_reply_with_fixture_key(
                schedule_reply(&[("20-0", &rollback)]),
                &progressed_context,
                &public_key,
                key_from,
                key_until,
            ),
            Err(Stage8bP1eScheduleReadError::Source(
                Stage8bP1eScheduleSourceError::Rollback
            ))
        ));

        let same_sequence_different_bytes = revised_schedule_envelope(&a7, 7, 7, now, 1);
        assert!(matches!(
            verify_newest_reply_with_fixture_key(
                schedule_reply(&[("20-0", &same_sequence_different_bytes)]),
                &progressed_context,
                &public_key,
                key_from,
                key_until,
            ),
            Err(Stage8bP1eScheduleReadError::Source(
                Stage8bP1eScheduleSourceError::ProgressionConflict
            ))
        ));
    }

    fn requested_latch() -> Stage8bP1eShutdownLatchV1 {
        let latch = Stage8bP1eShutdownLatchV1::new();
        assert!(latch.request(Stage8bP1eShutdownIntentV1::new(
            crate::Stage8bP1eShutdownCauseV1::ExternalSignal,
            2_000,
            17,
        )));
        latch
    }

    #[test]
    fn latch_c_stops_before_schedule_transport_and_carries_no_authority() {
        let latch = requested_latch();
        let receipt = stop_receipt(
            &latch,
            Stage8bP1eScheduleLatchCheckpointV1::BeforeScheduleRead,
        )
        .unwrap();
        assert_eq!(
            receipt.checkpoint(),
            Stage8bP1eScheduleLatchCheckpointV1::BeforeScheduleRead
        );
        assert_eq!(receipt.shutdown_intent().first_request_sequence(), 17);
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    struct ScheduleReadyTestSetup {
        parent: std::path::PathBuf,
        commitment_key: Stage5gLifecycleCommitmentKey,
        fresh_runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
        market_predecessor: strategy_runtime_core::Stage8bP1eM10IdentityV1,
        accepted_schedule: Option<strategy_runtime_core::Stage8bP1eAcceptedScheduleSourceV1>,
        schedule_public_key_hex: String,
        schedule_key_valid_from: DateTime<Utc>,
        schedule_key_valid_until: DateTime<Utc>,
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn schedule_bootstrap_config(
        parent: std::path::PathBuf,
        runtime_config_fingerprint_sha256: String,
    ) -> crate::Stage8bP1BootstrapConfig {
        crate::Stage8bP1BootstrapConfig {
            schema_version: crate::STAGE8B_P1_BOOTSTRAP_CONFIG_SCHEMA_VERSION,
            broker_id: crate::STAGE8B_P1_BROKER_ID.to_string(),
            strategy_id: crate::STAGE8B_P1_STRATEGY_ID.to_string(),
            account_id: "ACC_TEST_0001".to_string(),
            internal_symbol: crate::STAGE8B_P1_INTERNAL_SYMBOL.to_string(),
            venue_symbol: crate::STAGE8B_P1_VENUE_SYMBOL.to_string(),
            exchange: crate::STAGE8B_P1_EXCHANGE.to_string(),
            market: crate::STAGE8B_P1_MARKET.to_string(),
            tick_size: crate::STAGE8B_P1_TICK_SIZE.to_string(),
            runtime_config_fingerprint_sha256,
            instrument_map_fingerprint_sha256:
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            deployment_id: "finam-imoexf-paper-p1e-i1a-test".to_string(),
            deployment_generation: 1,
            gateway_instance_id: "finam-imoexf-paper-p1e-i1a-gateway".to_string(),
            market_data_generation: 1,
            command_consumer_generation: 1,
            stage8a4_writer_issuer_public_key_hex: "22".repeat(32),
            durable_parent: parent,
        }
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn schedule_source_m1(open_ts_utc_ms: i64) -> Vec<crate::Stage8bP1CanonicalM10SourceM1> {
        (0_i64..10)
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
            .collect()
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn p1e_test_instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn p1e_test_canonical_m10(
        operational_identity_sha256: String,
        close_ts_utc_ms: i64,
        close_price: i64,
    ) -> Vec<u8> {
        let open_ts_utc_ms = close_ts_utc_ms - 600_000;
        crate::build_stage8b_p1_canonical_m10(crate::Stage8bP1CanonicalM10BuildInput {
            operational_identity_sha256,
            open_ts_utc_ms,
            close_ts_utc_ms,
            open: close_price.to_string(),
            high: (close_price + 1).to_string(),
            low: (close_price - 1).to_string(),
            close: close_price.to_string(),
            volume: "10000".to_string(),
            source_m1: schedule_source_m1(open_ts_utc_ms),
        })
        .unwrap()
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn p1e_test_m10_identity(
        operational_identity_sha256: &str,
        close_ts_utc_ms: i64,
        close_price: i64,
    ) -> strategy_runtime_core::Stage8bP1eM10IdentityV1 {
        let bytes = p1e_test_canonical_m10(
            operational_identity_sha256.to_string(),
            close_ts_utc_ms,
            close_price,
        );
        let parsed =
            crate::parse_stage8b_p1_canonical_m10(&bytes, operational_identity_sha256).unwrap();
        strategy_runtime_core::Stage8bP1eM10IdentityV1 {
            close_ts_utc_ms: parsed.close_ts_utc_ms(),
            open_ts_utc_ms: parsed.open_ts_utc_ms(),
            payload_sha256: parsed.payload_sha256().to_string(),
            redis_id: parsed.redis_id().to_string(),
            semantic_id_sha256: parsed.semantic_id_sha256().to_string(),
        }
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn p1e_test_p1d3_evidence(
        operational_identity_sha256: &str,
        close_ts_utc_ms: i64,
        close_price: i64,
    ) -> strategy_runtime_core::Stage8bP1d3CanonicalM10Evidence {
        let bytes = p1e_test_canonical_m10(
            operational_identity_sha256.to_string(),
            close_ts_utc_ms,
            close_price,
        );
        crate::parse_stage8b_p1_canonical_m10(&bytes, operational_identity_sha256)
            .unwrap()
            .into_p1d3_limit_evidence()
            .unwrap()
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn p1e_test_p1d3_schedule_authority(
        predecessor_close_ts_utc_ms: i64,
        candidate_close_ts_utc_ms: i64,
    ) -> strategy_runtime_core::Stage8bP1d3ScheduleStepAuthority {
        strategy_runtime_core::stage8b_p1d3_test_step_authority(
            "44".repeat(32),
            Utc.timestamp_millis_opt(candidate_close_ts_utc_ms)
                .single()
                .unwrap()
                .date_naive()
                .to_string(),
            format!("{candidate_close_ts_utc_ms}-0"),
            format!("{predecessor_close_ts_utc_ms}-0"),
            format!("{candidate_close_ts_utc_ms}-0"),
        )
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn encode_lower_hex(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn decode_sha256(value: &str) -> [u8; 32] {
        assert_eq!(value.len(), 64);
        let mut output = [0_u8; 32];
        for (index, byte) in output.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).unwrap();
        }
        output
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn timestamp_text(value: DateTime<Utc>) -> String {
        value.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn signed_schedule_source(
        operational_identity_sha256: String,
        runtime_config_fingerprint_sha256: String,
        instrument_map_fingerprint_sha256: String,
        now: DateTime<Utc>,
        session_end_utc_ms: i64,
        closed: bool,
    ) -> (
        strategy_runtime_core::Stage8bP1eAcceptedScheduleSourceV1,
        String,
        DateTime<Utc>,
        DateTime<Utc>,
    ) {
        use strategy_runtime_core::{
            stage8b_p1e_canonical_json, stage8b_p1e_schedule_payload_sha256,
            stage8b_p1e_schedule_semantic_sha256, stage8b_p1e_schedule_unsigned_signature_sha256,
            stage8b_p1e_test_verify_schedule_envelope_with_key, Stage8bP1eDayBoundaryProofV1,
            Stage8bP1eNormalizedScheduleV2, Stage8bP1eScheduleEnvelopeV3,
            Stage8bP1eScheduleEvidenceKindV1, Stage8bP1eScheduleInstrumentV1,
            Stage8bP1eSchedulePayloadV2, Stage8bP1eScheduleRegistryV1,
            Stage8bP1eScheduleSemanticIdentityV1, Stage8bP1eScheduleSessionTypeV1,
            Stage8bP1eScheduleSessionV1, Stage8bP1eScheduleStateV1,
            Stage8bP1eScheduleVerificationContextV1, Stage8bP1eStage4EvidenceV2,
            Stage8bP1eStage4SemanticStateV1,
        };

        let trust_from = DateTime::parse_from_rfc3339("2026-01-01T00:00:00.000000Z")
            .unwrap()
            .with_timezone(&Utc);
        let trust_until = DateTime::parse_from_rfc3339("2027-01-01T00:00:00.000000Z")
            .unwrap()
            .with_timezone(&Utc);
        let instrument_id = broker_core::InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        };
        let truth = BrokerTruthSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            orders: Vec::new(),
            positions: Vec::new(),
            cash: None,
            trades: Vec::new(),
            instruments: vec![BrokerInstrumentSpec {
                instrument: InstrumentMapEntry {
                    internal_symbol: InternalSymbol("IMOEXF".to_string()),
                    broker: BrokerKind::Finam,
                    broker_symbol: BrokerSymbol("IMOEXF@RTSX".to_string()),
                    exchange: Exchange::Moex,
                    market: Market::Futures,
                    price_step: Decimal::new(5, 1),
                    qty_step: Decimal::ONE,
                    lot_size: Decimal::ONE,
                    min_qty: Decimal::ONE,
                    step_value: Decimal::new(5, 0),
                    currency: "RUB".to_string(),
                    schedule_id: "RTSX".to_string(),
                    expiration_date: None,
                    is_tradable: true,
                },
                broker_asset_id: Some("ASSET_TEST_1".to_string()),
                board: Some("RTSX".to_string()),
                long_initial_margin: Some(Money::new(5_000, 0)),
                short_initial_margin: Some(Money::new(5_000, 0)),
            }],
            received_ts: now,
        };
        let validated = broker_core::stage4_bootstrap::validate_stage4_broker_truth_bootstrap(
            Stage4BrokerTruthBootstrapInput {
                broker_truth: &truth,
                broker_truth_source_status: Stage4BrokerTruthSourceStatus::Present,
                target_instrument: instrument_id,
                restored_runtime_state: None,
                freshness: Stage4BrokerTruthFreshnessInput::synthetic_all_sections_fresh_for_tests(
                    now, 3_600_000,
                ),
                schedule_state: BrokerMarketSessionState::Open,
                adoption: Stage4AdoptionDisposition::default(),
                external_issues: Vec::new(),
                safety_boundary: Stage4BrokerTruthSafetyBoundary::closed(),
                checked_ts: now,
            },
        );
        let source_sections = validated
            .freshness
            .sections
            .iter()
            .map(|section| Stage4BootstrapEvidenceSourceStatusSection {
                section: section.section,
                source_status: Stage4BrokerTruthSourceStatus::Present,
                required_for_bootstrap: section.required_for_bootstrap,
            })
            .collect::<Vec<_>>();
        let stage4 = broker_core::stage4_bootstrap::build_stage4_accepted_paper_host_evidence(
            &validated,
            &source_sections,
        )
        .unwrap();
        let report_bytes = stage8b_p1e_canonical_json(stage4.report()).unwrap();
        let instrument = Stage8bP1eScheduleInstrumentV1 {
            board: "FUT".to_string(),
            broker_symbol: "IMOEXF@RTSX".to_string(),
            exchange: "moex".to_string(),
            market: "futures".to_string(),
            symbol: "IMOEXF".to_string(),
            tick_size: "0.5".to_string(),
            venue_mic: "RTSX".to_string(),
        };
        let sessions = vec![Stage8bP1eScheduleSessionV1 {
            end_utc: timestamp_text(
                Utc.timestamp_millis_opt(session_end_utc_ms)
                    .single()
                    .unwrap(),
            ),
            session_type: Stage8bP1eScheduleSessionTypeV1::TradableOpen,
            start_utc: "2026-08-03T06:00:00.000000Z".to_string(),
        }];
        let registry = Stage8bP1eScheduleRegistryV1 {
            registry_identity_sha256: "2".repeat(64),
            registry_version: "imoexf-v1".to_string(),
        };
        let mut payload = Stage8bP1eSchedulePayloadV2 {
            domain: "moex.stage8b.p1e.schedule-payload.v2".to_string(),
            instrument: instrument.clone(),
            normalized_schedule: Stage8bP1eNormalizedScheduleV2 {
                normalized_payload_sha256: strategy_runtime_core::stage8b_p1e_canonical_json(
                    &sessions,
                )
                .map(|bytes| {
                    use sha2::{Digest, Sha256};
                    encode_lower_hex(Sha256::digest(bytes))
                })
                .unwrap(),
                raw_response_sha256: "1".repeat(64),
                sessions: sessions.clone(),
                source_expires_at_utc: timestamp_text(now + chrono::Duration::hours(1)),
                source_observed_at_utc: timestamp_text(now),
            },
            registry: registry.clone(),
            schema_version: 2,
            stage4_evidence: Stage8bP1eStage4EvidenceV2 {
                boundary_proof: None,
                evidence_kind: Stage8bP1eScheduleEvidenceKindV1::Tradability,
                report_canonical_json_hex: encode_lower_hex(&report_bytes),
                report_sha256: {
                    use sha2::{Digest, Sha256};
                    encode_lower_hex(Sha256::digest(&report_bytes))
                },
                schedule_state: Stage8bP1eScheduleStateV1::Open,
                source_expires_at_utc: timestamp_text(stage4.required_source_expires_at()),
                source_observed_at_utc: timestamp_text(now),
            },
            timezone: "Europe/Moscow".to_string(),
            trading_day: "2026-08-03".to_string(),
        };
        let mut semantic_identity = Stage8bP1eScheduleSemanticIdentityV1 {
            domain: "moex.stage8b.p1e.schedule-semantic-identity.v1".to_string(),
            instrument,
            registry: registry.clone(),
            schema_version: 1,
            sessions,
            stage4_semantic_state: Stage8bP1eStage4SemanticStateV1 {
                boundary_proof: None,
                evidence_kind: Stage8bP1eScheduleEvidenceKindV1::Tradability,
                schedule_state: Stage8bP1eScheduleStateV1::Open,
            },
            timeframe_sec: 600,
            timezone: "Europe/Moscow".to_string(),
            trading_day: "2026-08-03".to_string(),
        };
        if closed {
            let boundary = Utc
                .timestamp_millis_opt(session_end_utc_ms)
                .single()
                .unwrap();
            let proof = Stage8bP1eDayBoundaryProofV1 {
                boundary_ts_utc: timestamp_text(boundary),
                last_eligible_m10_close_ts_utc: timestamp_text(boundary),
                last_eligible_m10_open_ts_utc: timestamp_text(
                    boundary - chrono::Duration::seconds(600),
                ),
                trading_day: "2026-08-03".to_string(),
            };
            payload.stage4_evidence.evidence_kind = Stage8bP1eScheduleEvidenceKindV1::DayBoundary;
            payload.stage4_evidence.schedule_state = Stage8bP1eScheduleStateV1::Closed;
            payload.stage4_evidence.boundary_proof = Some(proof.clone());
            semantic_identity.stage4_semantic_state = Stage8bP1eStage4SemanticStateV1 {
                boundary_proof: Some(proof),
                evidence_kind: Stage8bP1eScheduleEvidenceKindV1::DayBoundary,
                schedule_state: Stage8bP1eScheduleStateV1::Closed,
            };
        }
        let mut envelope = Stage8bP1eScheduleEnvelopeV3 {
            domain: "moex.stage8b.p1e.schedule-envelope.v3".to_string(),
            instrument_map_fingerprint_sha256: instrument_map_fingerprint_sha256.clone(),
            key_generation: 2,
            key_id: "schedule-ed25519-v1".to_string(),
            operational_identity_sha256: operational_identity_sha256.clone(),
            payload_sha256: stage8b_p1e_schedule_payload_sha256(&payload).unwrap(),
            payload,
            producer_contract_version: "finam-rest-schedule-to-broker-neutral-v3".to_string(),
            producer_id: "finam-readonly-schedule-normalizer-v3".to_string(),
            publication_sequence: "1".to_string(),
            published_at_utc: timestamp_text(now),
            runtime_config_fingerprint_sha256: runtime_config_fingerprint_sha256.clone(),
            schedule_semantic_sha256: stage8b_p1e_schedule_semantic_sha256(&semantic_identity)
                .unwrap(),
            schema_version: 3,
            semantic_identity,
            semantic_identity_contract_version: 1,
            semantic_revision: "1".to_string(),
            signature_ed25519_hex: String::new(),
            source_generation: "1".to_string(),
        };
        let signing_key = SigningKey::from_bytes(&[0x71; 32]);
        let signature_digest = stage8b_p1e_schedule_unsigned_signature_sha256(&envelope).unwrap();
        envelope.signature_ed25519_hex = encode_lower_hex(
            signing_key
                .sign(&decode_sha256(&signature_digest))
                .to_bytes(),
        );
        let exact = stage8b_p1e_canonical_json(&envelope).unwrap();
        let public_key_hex = encode_lower_hex(signing_key.verifying_key().to_bytes());
        let accepted = stage8b_p1e_test_verify_schedule_envelope_with_key(
            &exact,
            &Stage8bP1eScheduleVerificationContextV1 {
                expected_instrument_map_fingerprint_sha256: instrument_map_fingerprint_sha256,
                expected_operational_identity_sha256: operational_identity_sha256,
                expected_registry_identity_sha256: registry.registry_identity_sha256,
                expected_registry_version: registry.registry_version,
                expected_runtime_config_fingerprint_sha256: runtime_config_fingerprint_sha256,
                high_water: None,
                trusted_now: now,
            },
            &public_key_hex,
            trust_from,
            trust_until,
        )
        .unwrap();
        (accepted, public_key_hex, trust_from, trust_until)
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    pub(crate) struct P1eTestOpenScheduleEnvelope {
        pub(crate) bytes: Vec<u8>,
        pub(crate) context: Stage8bP1eScheduleVerificationContextV1,
        pub(crate) public_key_hex: String,
        pub(crate) key_valid_from: DateTime<Utc>,
        pub(crate) key_valid_until: DateTime<Utc>,
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    pub(crate) fn p1e_test_open_schedule_envelope(
        operational_identity_sha256: String,
        runtime_config_fingerprint_sha256: String,
        instrument_map_fingerprint_sha256: String,
        now: DateTime<Utc>,
    ) -> P1eTestOpenScheduleEnvelope {
        let (accepted, public_key_hex, key_valid_from, key_valid_until) = signed_schedule_source(
            operational_identity_sha256.clone(),
            runtime_config_fingerprint_sha256.clone(),
            instrument_map_fingerprint_sha256.clone(),
            now,
            Utc.with_ymd_and_hms(2026, 8, 3, 18, 0, 0)
                .single()
                .unwrap()
                .timestamp_millis(),
            false,
        );
        P1eTestOpenScheduleEnvelope {
            bytes: accepted.exact_envelope_bytes().to_vec(),
            context: Stage8bP1eScheduleVerificationContextV1 {
                expected_instrument_map_fingerprint_sha256: instrument_map_fingerprint_sha256,
                expected_operational_identity_sha256: operational_identity_sha256,
                expected_registry_identity_sha256: "2".repeat(64),
                expected_registry_version: "imoexf-v1".to_string(),
                expected_runtime_config_fingerprint_sha256: runtime_config_fingerprint_sha256,
                high_water: None,
                trusted_now: now,
            },
            public_key_hex,
            key_valid_from,
            key_valid_until,
        }
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    pub(crate) fn p1e_test_open_schedule_snapshot(
        operational_identity_sha256: String,
        runtime_config_fingerprint_sha256: String,
        instrument_map_fingerprint_sha256: String,
        redis_stream_id: String,
        now: DateTime<Utc>,
    ) -> Stage8bP1eVerifiedScheduleSnapshotV1 {
        let (accepted, _, _, _) = signed_schedule_source(
            operational_identity_sha256,
            runtime_config_fingerprint_sha256,
            instrument_map_fingerprint_sha256,
            now,
            Utc.with_ymd_and_hms(2026, 8, 3, 18, 0, 0)
                .single()
                .unwrap()
                .timestamp_millis(),
            false,
        );
        Stage8bP1eVerifiedScheduleSnapshotV1 {
            redis_stream_id,
            accepted,
        }
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn revised_schedule_envelope(
        base: &[u8],
        publication_sequence: u64,
        semantic_revision: u64,
        published_at: DateTime<Utc>,
        semantic_variant: u8,
    ) -> Vec<u8> {
        use strategy_runtime_core::{
            stage8b_p1e_canonical_json, stage8b_p1e_schedule_payload_sha256,
            stage8b_p1e_schedule_semantic_sha256, stage8b_p1e_schedule_unsigned_signature_sha256,
            Stage8bP1eScheduleEnvelopeV3,
        };

        let mut envelope: Stage8bP1eScheduleEnvelopeV3 = serde_json::from_slice(base).unwrap();
        if semantic_variant == 1 {
            let end = "2026-08-03T17:50:00.000000Z".to_string();
            envelope.payload.normalized_schedule.sessions[0].end_utc = end.clone();
            envelope.semantic_identity.sessions[0].end_utc = end;
            let session_bytes =
                stage8b_p1e_canonical_json(&envelope.payload.normalized_schedule.sessions).unwrap();
            use sha2::{Digest, Sha256};
            envelope
                .payload
                .normalized_schedule
                .normalized_payload_sha256 = encode_lower_hex(Sha256::digest(session_bytes));
        }
        envelope.publication_sequence = publication_sequence.to_string();
        envelope.semantic_revision = semantic_revision.to_string();
        envelope.published_at_utc = timestamp_text(published_at);
        envelope.payload_sha256 = stage8b_p1e_schedule_payload_sha256(&envelope.payload).unwrap();
        envelope.schedule_semantic_sha256 =
            stage8b_p1e_schedule_semantic_sha256(&envelope.semantic_identity).unwrap();
        envelope.signature_ed25519_hex.clear();
        let digest = stage8b_p1e_schedule_unsigned_signature_sha256(&envelope).unwrap();
        let key = SigningKey::from_bytes(&[0x71; 32]);
        envelope.signature_ed25519_hex =
            encode_lower_hex(key.sign(&decode_sha256(&digest)).to_bytes());
        stage8b_p1e_canonical_json(&envelope).unwrap()
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    type SnapshotProgressionFixture = (
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Stage8bP1eScheduleVerificationContextV1,
        String,
        DateTime<Utc>,
        DateTime<Utc>,
    );

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn snapshot_progression_fixture() -> SnapshotProgressionFixture {
        let now = DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
            .unwrap()
            .with_timezone(&Utc);
        let (base, public_key_hex, key_valid_from, key_valid_until) = signed_schedule_source(
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64),
            now,
            Utc.with_ymd_and_hms(2026, 8, 3, 18, 0, 0)
                .single()
                .unwrap()
                .timestamp_millis(),
            false,
        );
        let a7 = revised_schedule_envelope(
            base.exact_envelope_bytes(),
            7,
            7,
            now - chrono::Duration::seconds(2),
            0,
        );
        let b8 = revised_schedule_envelope(
            base.exact_envelope_bytes(),
            8,
            8,
            now - chrono::Duration::seconds(1),
            1,
        );
        let a9 = revised_schedule_envelope(base.exact_envelope_bytes(), 9, 9, now, 0);
        let context = Stage8bP1eScheduleVerificationContextV1 {
            expected_instrument_map_fingerprint_sha256: "c".repeat(64),
            expected_operational_identity_sha256: "a".repeat(64),
            expected_registry_identity_sha256: "2".repeat(64),
            expected_registry_version: "imoexf-v1".to_string(),
            expected_runtime_config_fingerprint_sha256: "b".repeat(64),
            high_water: None,
            trusted_now: now,
        };
        (
            a7,
            b8,
            a9,
            context,
            public_key_hex,
            key_valid_from,
            key_valid_until,
        )
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn signed_market_candidate(
        operational_identity_sha256: String,
        runtime_config_fingerprint_sha256: String,
        instrument_map_fingerprint_sha256: String,
        strategy_request_id: String,
        canonical_command_sha256: String,
        predecessor: strategy_runtime_core::Stage8bP1eM10IdentityV1,
    ) -> (
        strategy_runtime_core::Stage8bP1eScheduleBindingCandidateV1,
        strategy_runtime_core::Stage8bP1eAcceptedScheduleSourceV1,
        String,
        DateTime<Utc>,
        DateTime<Utc>,
    ) {
        let now = DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
            .unwrap()
            .with_timezone(&Utc);
        let (accepted, public_key_hex, trust_from, trust_until) = signed_schedule_source(
            operational_identity_sha256.clone(),
            runtime_config_fingerprint_sha256,
            instrument_map_fingerprint_sha256,
            now,
            Utc.with_ymd_and_hms(2026, 8, 3, 18, 0, 0)
                .single()
                .unwrap()
                .timestamp_millis(),
            false,
        );
        let candidate_open_ts_utc_ms = predecessor.close_ts_utc_ms;
        let candidate_close_ts_utc_ms = candidate_open_ts_utc_ms + 600_000;
        let candidate_bytes = p1e_test_canonical_m10(
            operational_identity_sha256.clone(),
            candidate_close_ts_utc_ms,
            2_175,
        );
        let parsed =
            crate::parse_stage8b_p1_canonical_m10(&candidate_bytes, &operational_identity_sha256)
                .unwrap();
        let candidate_m10 = strategy_runtime_core::Stage8bP1eM10IdentityV1 {
            close_ts_utc_ms: parsed.close_ts_utc_ms(),
            open_ts_utc_ms: parsed.open_ts_utc_ms(),
            payload_sha256: parsed.payload_sha256().to_string(),
            redis_id: parsed.redis_id().to_string(),
            semantic_id_sha256: parsed.semantic_id_sha256().to_string(),
        };
        let candidate = accepted
            .prepare_market_binding(
                &predecessor,
                &candidate_m10,
                strategy_request_id,
                canonical_command_sha256,
                format!("{candidate_close_ts_utc_ms}-1"),
            )
            .unwrap();
        (candidate, accepted, public_key_hex, trust_from, trust_until)
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn schedule_ready_fixture() -> (
        ScheduleReadyTestSetup,
        Stage7bRecoveryReadyOwner,
        strategy_runtime_core::Stage8bP1eScheduleBindingCandidateV1,
    ) {
        use strategy_runtime_core::Stage5gP1SemanticBindingInput;

        let parent = std::env::temp_dir().join(format!(
            "stage8b-p1e-i1a-schedule-{}-{}",
            std::process::id(),
            SCHEDULE_TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::SeqCst)
        ));
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700).create(&parent).unwrap();
        let parent = std::fs::canonicalize(parent).unwrap();
        let (source, export_input, commitment_key, fresh_runtime) =
            strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let config = crate::validate_stage8b_p1_bootstrap_config(schedule_bootstrap_config(
            parent.clone(),
            fresh_runtime.stage5c_config_fingerprint(),
        ))
        .unwrap();
        let admin = crate::authorize_stage8b_p1_first_boot(
            &config,
            crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
        )
        .unwrap();
        let owner = crate::first_boot_stage8b_p1(
            config,
            admin,
            source,
            export_input,
            &commitment_key,
            fresh_runtime.clone(),
        )
        .unwrap()
        .into_owner();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();
        let close_ts_utc_ms = 1_785_759_000_000_i64;
        let bytes = crate::build_stage8b_p1_canonical_m10(crate::Stage8bP1CanonicalM10BuildInput {
            operational_identity_sha256: operational_identity.clone(),
            open_ts_utc_ms: close_ts_utc_ms - 600_000,
            close_ts_utc_ms,
            open: "2650".to_string(),
            high: "2651".to_string(),
            low: "2649".to_string(),
            close: "2650".to_string(),
            volume: "10000".to_string(),
            source_m1: schedule_source_m1(close_ts_utc_ms - 600_000),
        })
        .unwrap();
        let parsed = crate::parse_stage8b_p1_canonical_m10(&bytes, &operational_identity).unwrap();
        let binding = Stage5gP1SemanticBindingInput {
            operational_identity_sha256: operational_identity.clone(),
            m10_redis_id: parsed.redis_id().to_string(),
            m10_semantic_id_sha256: parsed.semantic_id_sha256().to_string(),
            m10_payload_sha256: parsed.payload_sha256().to_string(),
        };
        let predecessor = strategy_runtime_core::Stage8bP1eM10IdentityV1 {
            close_ts_utc_ms: parsed.close_ts_utc_ms(),
            open_ts_utc_ms: parsed.open_ts_utc_ms(),
            payload_sha256: parsed.payload_sha256().to_string(),
            redis_id: parsed.redis_id().to_string(),
            semantic_id_sha256: parsed.semantic_id_sha256().to_string(),
        };
        let accepted_bar = parsed.into_stage5c_semantic_bar().unwrap();
        let crate::Stage8bP1SemanticCommitOutcome::OneIntentPrepublication(durable) = owner
            .commit_stage8b_p1_semantic(accepted_bar, binding, &commitment_key)
            .unwrap()
        else {
            panic!("breakout M10 must produce one exact semantic command")
        };
        let (owner, _, _) = (*durable).into_p1c_parts();
        let decision = owner.stage8b_p1d1_command_decision_binding().unwrap();
        let (request_id, command_sha256) = decision.stage8b_p1e_test_candidate_parts();
        let (
            candidate,
            accepted_schedule,
            schedule_public_key_hex,
            schedule_key_valid_from,
            schedule_key_valid_until,
        ) = signed_market_candidate(
            operational_identity,
            fresh_runtime.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            request_id,
            command_sha256,
            predecessor.clone(),
        );
        (
            ScheduleReadyTestSetup {
                parent,
                commitment_key,
                fresh_runtime,
                market_predecessor: predecessor,
                accepted_schedule: Some(accepted_schedule),
                schedule_public_key_hex,
                schedule_key_valid_from,
                schedule_key_valid_until,
            },
            owner,
            candidate,
        )
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn p1e_test_working_limit_fixture() -> (
        ScheduleReadyTestSetup,
        Stage7bRecoveryReadyOwner,
        strategy_runtime_core::Stage8bP1eM10IdentityV1,
    ) {
        use strategy_runtime_core::{Stage5gP1SemanticBindingInput, Stage8bP1d3InitialObservation};

        let (setup, mut owner, _market_candidate) = schedule_ready_fixture();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();

        let market_successor_close_ms = 1_785_759_600_000_i64;
        let market_successor_bytes = p1e_test_canonical_m10(
            operational_identity.clone(),
            market_successor_close_ms,
            2_175,
        );
        let market_successor =
            crate::parse_stage8b_p1_canonical_m10(&market_successor_bytes, &operational_identity)
                .unwrap();
        let eligibility = owner
            .stage8b_p1d1_execution_eligibility(
                strategy_runtime_core::stage8b_p1d1_test_schedule_authority(
                    p1e_test_instrument(),
                    1_785_759_000_000,
                    market_successor_close_ms,
                ),
                market_successor.into_p1d1_execution_evidence().unwrap(),
            )
            .unwrap();
        let provider = owner
            .admit_p1d1_eligible_market_dispatch(eligibility)
            .unwrap();
        let market_outcome = provider.execute();
        let owner = owner
            .commit_stage8b_p1d2_ack(market_outcome, &setup.commitment_key)
            .unwrap()
            .commit_truth(&setup.commitment_key)
            .unwrap()
            .into_ready_after_source_resolution()
            .migrate_stage8b_p1d3_from_resolved_p1d2(&setup.commitment_key)
            .unwrap();

        let place_binding_bytes = p1e_test_canonical_m10(
            operational_identity.clone(),
            P1E_TEST_PLACE_DECISION_CLOSE_MS,
            2_210,
        );
        let place_binding_m10 =
            crate::parse_stage8b_p1_canonical_m10(&place_binding_bytes, &operational_identity)
                .unwrap();
        let place_binding = Stage5gP1SemanticBindingInput {
            operational_identity_sha256: operational_identity.clone(),
            m10_redis_id: place_binding_m10.redis_id().to_string(),
            m10_semantic_id_sha256: place_binding_m10.semantic_id_sha256().to_string(),
            m10_payload_sha256: place_binding_m10.payload_sha256().to_string(),
        };
        let inherited_attribution = owner.stage8b_p1d3_test_working_book_attribution().unwrap();
        let (attribution_prefix, _) = inherited_attribution
            .internal_comment()
            .rsplit_once("|r=")
            .unwrap();
        let attribution = broker_core::HybridRuntimeAttribution::parse_source_comment(format!(
            "{attribution_prefix}|r=EXIT"
        ))
        .unwrap();
        let request_id = StrategyRequestId::from(uuid::Uuid::from_u128(
            0xe11a_0000_0000_4000_8000_0000_0000_0001,
        ));
        let place = BrokerCommand::PlaceOrder(PlaceOrder {
            request_id,
            created_ts: Utc
                .timestamp_millis_opt(P1E_TEST_PLACE_DECISION_CLOSE_MS)
                .single()
                .unwrap(),
            ttl_ms: None,
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::from_strategy_request(request_id),
            instrument: p1e_test_instrument(),
            side: OrderSide::Sell,
            order_type: OrderType::Limit,
            qty: Decimal::ONE,
            limit_price: Some(Decimal::new(2_230, 0)),
            time_in_force: TimeInForce::Day,
            comment: Some(attribution.internal_comment().to_string()),
        });
        let prepublication = owner
            .stage8b_p1d3_test_inject_one_intent(
                place_binding,
                place,
                attribution,
                &setup.commitment_key,
            )
            .unwrap();
        let (owner, _, _) = prepublication.into_p1c_parts();
        let initial_evidence = p1e_test_p1d3_evidence(
            &operational_identity,
            P1E_TEST_INITIAL_CANDIDATE_CLOSE_MS,
            2_220,
        );
        let owner = owner
            .commit_stage8b_p1d3_initial_limit_ack(
                Stage8bP1d3InitialObservation::Candidate {
                    evidence: Box::new(initial_evidence),
                    schedule: p1e_test_p1d3_schedule_authority(
                        P1E_TEST_PLACE_DECISION_CLOSE_MS,
                        P1E_TEST_INITIAL_CANDIDATE_CLOSE_MS,
                    ),
                },
                &setup.commitment_key,
            )
            .unwrap()
            .commit_truth(&setup.commitment_key)
            .unwrap()
            .into_ready_after_source_resolution();
        let (_, _, predecessor) = owner
            .stage8b_p1e_working_binding_parts()
            .expect("initial LIMIT must remain Working");
        (setup, owner, predecessor)
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn p1e_test_bound_working_schedule(
        close_price: i64,
    ) -> (
        ScheduleReadyTestSetup,
        Stage7bRecoveryReadyOwner,
        strategy_runtime_core::Stage8bP1d3ScheduleStepAuthority,
        strategy_runtime_core::Stage8bP1d3CanonicalM10Evidence,
    ) {
        let (mut setup, owner, predecessor) = p1e_test_working_limit_fixture();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();
        let (active_order, transition_sha256, _) = owner
            .stage8b_p1e_working_binding_parts()
            .expect("working binding material must be authenticated");
        let candidate = p1e_test_m10_identity(
            &operational_identity,
            P1E_TEST_LATER_CANDIDATE_CLOSE_MS,
            close_price,
        );
        let committed = match bind_stage8b_p1e_working_limit_schedule(
            owner,
            Stage8bP1eVerifiedScheduleSnapshotV1 {
                redis_stream_id: format!("{}-1", P1E_TEST_LATER_CANDIDATE_CLOSE_MS),
                accepted: setup.accepted_schedule.take().unwrap(),
            },
            &Stage8bP1eShutdownLatchV1::new(),
            &predecessor,
            &candidate,
            active_order.as_str(),
            transition_sha256,
            DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
                .unwrap()
                .with_timezone(&Utc),
            &setup.commitment_key,
        )
        .unwrap()
        {
            Stage8bP1eScheduleBindingCommitV1::Committed(owner) => *owner,
            _ => panic!("clear latch D must commit the exact Working V4"),
        };
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must continue"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } =
            continue_stage8b_p1e_schedule_step(permit, &Stage8bP1eShutdownLatchV1::new()).unwrap()
        else {
            panic!("clear latch F must issue Working authority")
        };
        (
            setup,
            *owner,
            authority,
            p1e_test_p1d3_evidence(
                &operational_identity,
                P1E_TEST_LATER_CANDIDATE_CLOSE_MS,
                close_price,
            ),
        )
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn p1e_test_install_closed_schedule(
        setup: &mut ScheduleReadyTestSetup,
        owner: &Stage7bRecoveryReadyOwner,
        boundary_ts_utc_ms: i64,
    ) {
        let now = Utc
            .timestamp_millis_opt(boundary_ts_utc_ms)
            .single()
            .unwrap();
        let (accepted, public_key_hex, trust_from, trust_until) = signed_schedule_source(
            owner.stage8b_p1_operational_identity_sha256().to_string(),
            setup.fresh_runtime.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            now,
            boundary_ts_utc_ms,
            true,
        );
        setup.accepted_schedule = Some(accepted);
        setup.schedule_public_key_hex = public_key_hex;
        setup.schedule_key_valid_from = trust_from;
        setup.schedule_key_valid_until = trust_until;
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn p1e_test_bound_day_expiry_schedule() -> (
        ScheduleReadyTestSetup,
        Stage7bRecoveryReadyOwner,
        strategy_runtime_core::Stage8bP1d3DayExpiryAuthority,
    ) {
        let (mut setup, owner, last_evaluated) = p1e_test_working_limit_fixture();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();
        let (active_order, transition_sha256, exact_last_evaluated) = owner
            .stage8b_p1e_working_binding_parts()
            .expect("working binding material must be authenticated");
        assert_eq!(last_evaluated, exact_last_evaluated);
        p1e_test_install_closed_schedule(&mut setup, &owner, P1E_TEST_INITIAL_CANDIDATE_CLOSE_MS);
        let predecessor = p1e_test_m10_identity(
            &operational_identity,
            P1E_TEST_PLACE_DECISION_CLOSE_MS,
            2_210,
        );
        let trusted_now = Utc
            .timestamp_millis_opt(P1E_TEST_INITIAL_CANDIDATE_CLOSE_MS)
            .single()
            .unwrap();
        let committed = match bind_stage8b_p1e_day_expiry_schedule(
            owner,
            Stage8bP1eVerifiedScheduleSnapshotV1 {
                redis_stream_id: format!("{}-1", P1E_TEST_INITIAL_CANDIDATE_CLOSE_MS),
                accepted: setup.accepted_schedule.take().unwrap(),
            },
            &Stage8bP1eShutdownLatchV1::new(),
            &predecessor,
            &last_evaluated,
            trusted_now,
            active_order.as_str(),
            transition_sha256,
            trusted_now,
            &setup.commitment_key,
        )
        .unwrap()
        {
            Stage8bP1eScheduleBindingCommitV1::Committed(owner) => *owner,
            _ => panic!("clear latch D must commit the exact expiry V4"),
        };
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must continue"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } =
            continue_stage8b_p1e_day_expiry_schedule(permit, &Stage8bP1eShutdownLatchV1::new())
                .unwrap()
        else {
            panic!("clear latch F must issue expiry authority")
        };
        (setup, *owner, authority)
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn p1e_test_cancel_binding_fixture() -> (
        ScheduleReadyTestSetup,
        Stage7bRecoveryReadyOwner,
        strategy_runtime_core::Stage8bP1d3ScheduleStepAuthority,
        strategy_runtime_core::Stage8bP1d3CanonicalM10Evidence,
    ) {
        use strategy_runtime_core::Stage5gP1SemanticBindingInput;

        let (mut setup, owner, _) = p1e_test_working_limit_fixture();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();
        let (target_order, _, _) = owner
            .stage8b_p1e_working_binding_parts()
            .expect("cancel fixture requires one authenticated Working LIMIT");
        let source_attribution = owner.stage8b_p1d3_test_working_book_attribution().unwrap();
        let (attribution_prefix, _) = source_attribution
            .internal_comment()
            .rsplit_once("|r=")
            .unwrap();
        let cancel_attribution = broker_core::HybridRuntimeAttribution::parse_source_comment(
            format!("{attribution_prefix}|r=CANCEL"),
        )
        .unwrap();
        let cancel_decision_close_ms = P1E_TEST_LATER_CANDIDATE_CLOSE_MS;
        let cancel_source_bytes = p1e_test_canonical_m10(
            operational_identity.clone(),
            cancel_decision_close_ms,
            2_220,
        );
        let cancel_source =
            crate::parse_stage8b_p1_canonical_m10(&cancel_source_bytes, &operational_identity)
                .unwrap();
        let binding = Stage5gP1SemanticBindingInput {
            operational_identity_sha256: operational_identity.clone(),
            m10_redis_id: cancel_source.redis_id().to_string(),
            m10_semantic_id_sha256: cancel_source.semantic_id_sha256().to_string(),
            m10_payload_sha256: cancel_source.payload_sha256().to_string(),
        };
        let cancel_request_id = StrategyRequestId::from(uuid::Uuid::from_u128(
            0xca11_ce10_0000_4000_8000_0000_0000_0001,
        ));
        let command = BrokerCommand::CancelOrder(broker_core::CancelOrder {
            request_id: cancel_request_id,
            created_ts: Utc
                .timestamp_millis_opt(cancel_decision_close_ms)
                .single()
                .unwrap(),
            ttl_ms: None,
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            order_id: target_order,
            client_order_id: None,
        });
        let prepublication = owner
            .stage8b_p1d3_test_inject_one_intent(
                binding,
                command,
                cancel_attribution,
                &setup.commitment_key,
            )
            .unwrap();
        let (owner, _, _) = prepublication.into_p1c_parts();
        let (active_order, transition_sha256, predecessor) = owner
            .stage8b_p1e_working_binding_parts()
            .expect("cancel semantic commit must preserve the exact Working LIMIT");
        let candidate_close_ms = cancel_decision_close_ms + 600_000;
        let candidate = p1e_test_m10_identity(&operational_identity, candidate_close_ms, 2_220);
        let boundary = Utc.with_ymd_and_hms(2026, 8, 3, 18, 0, 0).single().unwrap();
        let (accepted, public_key_hex, trust_from, trust_until) = signed_schedule_source(
            operational_identity.clone(),
            setup.fresh_runtime.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            boundary,
            boundary.timestamp_millis(),
            true,
        );
        setup.schedule_public_key_hex = public_key_hex;
        setup.schedule_key_valid_from = trust_from;
        setup.schedule_key_valid_until = trust_until;
        let committed = match bind_stage8b_p1e_cancel_schedule(
            owner,
            Stage8bP1eVerifiedScheduleSnapshotV1 {
                redis_stream_id: format!("{}-1", boundary.timestamp_millis()),
                accepted,
            },
            &Stage8bP1eShutdownLatchV1::new(),
            &predecessor,
            &candidate,
            active_order.as_str(),
            transition_sha256,
            boundary,
            &setup.commitment_key,
        )
        .unwrap()
        {
            Stage8bP1eScheduleBindingCommitV1::Committed(owner) => *owner,
            Stage8bP1eScheduleBindingCommitV1::StoppedBeforeBinding { .. } => {
                panic!("clear latch D must commit the exact cancel V4")
            }
        };
        let binding_record_id = committed.receipt().journal_record_id().to_string();
        drop(committed);
        let crate::Stage7bRestartOutcome::P1eScheduleBindingCommitted(committed) =
            restart_schedule_fixture(&setup)
        else {
            panic!("covered cancel V4 must restart as the exact committed binding")
        };
        assert_eq!(committed.receipt().journal_record_id(), binding_record_id);
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            *committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must preserve the cancel-only V4"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } =
            continue_stage8b_p1e_schedule_step(permit, &Stage8bP1eShutdownLatchV1::new()).unwrap()
        else {
            panic!("clear latch F must issue the cancel-only authority")
        };
        (
            setup,
            *owner,
            authority,
            p1e_test_p1d3_evidence(&operational_identity, candidate_close_ms, 2_220),
        )
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn committed_schedule_fixture() -> (
        ScheduleReadyTestSetup,
        Stage8bP1eScheduleBindingCommittedOwner,
        u64,
    ) {
        let (setup, owner, candidate) = schedule_ready_fixture();
        let prior_generation = owner.committed_seal().unwrap().seal_generation();
        let committed = owner
            .commit_stage8b_p1e_schedule_binding(
                candidate,
                DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
                    .unwrap()
                    .with_timezone(&Utc),
                &setup.commitment_key,
            )
            .unwrap();
        (setup, committed, prior_generation)
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn restart_schedule_fixture(setup: &ScheduleReadyTestSetup) -> crate::Stage7bRestartOutcome {
        let config = schedule_bootstrap_config(
            setup.parent.clone(),
            setup.fresh_runtime.stage5c_config_fingerprint(),
        );
        let identity = strategy_runtime_core::Stage6dOperationalIdentityConfig {
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
        let root_name =
            crate::Stage7bDurableRootAuthority::expected_directory_name(&identity).unwrap();
        let root =
            crate::Stage7bDurableRootAuthority::validate(setup.parent.join(root_name), &identity)
                .unwrap();
        crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            identity,
            &setup.commitment_key,
            setup.fresh_runtime.clone(),
            setup.schedule_public_key_hex.clone(),
            setup.schedule_key_valid_from,
            setup.schedule_key_valid_until,
        )
        .unwrap()
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn latch_d_returns_the_exact_owner_without_a_binding_seal() {
        let (setup, owner, _candidate) = schedule_ready_fixture();
        let prior_generation = owner.committed_seal().unwrap().seal_generation();
        let decision = match check_latch_d(owner, &requested_latch()) {
            Err(decision) => decision,
            Ok(_) => panic!("set latch D must stop before binding"),
        };
        let Stage8bP1eScheduleBindingDecisionV1::StoppedBeforeBinding { owner, receipt } = decision
        else {
            panic!("latch D must retain the pre-binding owner");
        };
        assert_eq!(
            receipt.checkpoint(),
            Stage8bP1eScheduleLatchCheckpointV1::AfterScheduleReadBeforeBinding
        );
        assert_eq!(
            owner.committed_seal().unwrap().seal_generation(),
            prior_generation
        );
        drop(owner);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn latches_e_and_f_retain_the_exact_committed_binding_without_a_second_seal() {
        let (setup, committed, prior_generation) = committed_schedule_fixture();
        let expected_record_id = committed.receipt().journal_record_id().to_string();
        let decision = resume_stage8b_p1e_committed_schedule_binding(committed, &requested_latch());
        let Stage8bP1eScheduleBindingDecisionV1::StoppedAfterBinding { owner, receipt } = decision
        else {
            panic!("latch E must retain the committed binding");
        };
        assert_eq!(
            receipt.checkpoint(),
            Stage8bP1eScheduleLatchCheckpointV1::AfterBinding
        );
        assert_eq!(owner.receipt().journal_record_id(), expected_record_id);
        assert_eq!(
            owner.receipt().covering_seal_generation(),
            prior_generation + 1
        );
        drop(owner);
        std::fs::remove_dir_all(setup.parent).unwrap();

        let (setup, committed, prior_generation) = committed_schedule_fixture();
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must return the post-binding permit"),
        };
        let expected_record_id = permit.receipt().journal_record_id().to_string();
        let decision = continue_stage8b_p1e_market_schedule(permit, &requested_latch()).unwrap();
        let Stage8bP1eScheduleAuthorityDecisionV1::RetainForRestart { owner, receipt } = decision
        else {
            panic!("latch F must retain the committed binding");
        };
        assert_eq!(
            receipt.checkpoint(),
            Stage8bP1eScheduleLatchCheckpointV1::BeforeEffect
        );
        assert_eq!(owner.receipt().journal_record_id(), expected_record_id);
        assert_eq!(
            owner.receipt().covering_seal_generation(),
            prior_generation + 1
        );
        drop(owner);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signal_after_latch_d_and_binding_is_observed_by_mandatory_latch_e() {
        let (setup, owner, candidate) = schedule_ready_fixture();
        let prior_generation = owner.committed_seal().unwrap().seal_generation();
        let clear = Stage8bP1eShutdownLatchV1::new();
        let owner = match binding_owner_after_latch_d(owner, &clear) {
            Ok(owner) => owner,
            Err(_) => panic!("clear latch D must retain the only pre-binding owner"),
        };
        let committed = commit_binding_only(
            owner,
            candidate,
            DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
                .unwrap()
                .with_timezone(&Utc),
            &setup.commitment_key,
        )
        .unwrap();

        let pending_signal = requested_latch();
        let decision = finish_binding_commit(committed, &pending_signal).unwrap();
        let Stage8bP1eScheduleBindingDecisionV1::StoppedAfterBinding { owner, receipt } = decision
        else {
            panic!("signal observed after binding must stop before route authority issuance");
        };
        assert_eq!(
            receipt.checkpoint(),
            Stage8bP1eScheduleLatchCheckpointV1::AfterBinding
        );
        assert_eq!(
            owner.receipt().covering_seal_generation(),
            prior_generation + 1,
            "the non-cancellable binding completes exactly one covering seal"
        );
        drop(owner);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn clear_latches_issue_one_route_bound_authority_after_one_binding_seal() {
        let (setup, committed, prior_generation) = committed_schedule_fixture();
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must continue"),
        };
        let decision =
            continue_stage8b_p1e_market_schedule(permit, &Stage8bP1eShutdownLatchV1::new())
                .unwrap();
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } = decision else {
            panic!("clear latch F must issue authority");
        };
        assert_eq!(
            owner.committed_seal().unwrap().seal_generation(),
            prior_generation + 1
        );
        drop(authority);
        drop(owner);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    fn p1e_test_commit_exact_later_semantic(
        pending: crate::Stage8bP1d3SemanticPendingOwner,
        operational_identity_sha256: &str,
        close_price: i64,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> (Stage7bRecoveryReadyOwner, usize) {
        use strategy_runtime_core::Stage5gP1SemanticBindingInput;

        let source = pending.source_binding().unwrap();
        let bytes = p1e_test_canonical_m10(
            operational_identity_sha256.to_string(),
            P1E_TEST_LATER_CANDIDATE_CLOSE_MS,
            close_price,
        );
        let parsed =
            crate::parse_stage8b_p1_canonical_m10(&bytes, operational_identity_sha256).unwrap();
        assert_eq!(source.redis_id(), parsed.redis_id());
        assert_eq!(source.semantic_id_sha256(), parsed.semantic_id_sha256());
        assert_eq!(source.payload_sha256(), parsed.payload_sha256());
        let binding = Stage5gP1SemanticBindingInput {
            operational_identity_sha256: operational_identity_sha256.to_string(),
            m10_redis_id: parsed.redis_id().to_string(),
            m10_semantic_id_sha256: parsed.semantic_id_sha256().to_string(),
            m10_payload_sha256: parsed.payload_sha256().to_string(),
        };
        let accepted_bar = parsed.into_stage5c_semantic_bar().unwrap();
        match pending
            .commit_exact_semantic(accepted_bar, binding, commitment_key)
            .unwrap()
        {
            crate::Stage8bP1SemanticCommitOutcome::ZeroIntent { owner, .. } => (*owner, 0),
            crate::Stage8bP1SemanticCommitOutcome::OneIntentPrepublication(pending) => {
                let (owner, _, _) = (*pending).into_p1c_parts();
                (owner, 1)
            }
            crate::Stage8bP1SemanticCommitOutcome::MultiIntentBlocked(_) => {
                panic!("canonical later bar produced multiple intents")
            }
        }
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_v4_market_reaches_existing_effect_and_restart_once() {
        let (mut setup, owner, _candidate) = schedule_ready_fixture();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();
        let before = owner.stage8b_p1e_test_checkpoint_snapshot();
        let predecessor = setup.market_predecessor.clone();
        let candidate_close_ts_utc_ms = predecessor.close_ts_utc_ms + 600_000;
        let candidate_bytes = p1e_test_canonical_m10(
            operational_identity.clone(),
            candidate_close_ts_utc_ms,
            2_175,
        );
        let candidate =
            crate::parse_stage8b_p1_canonical_m10(&candidate_bytes, &operational_identity).unwrap();
        let candidate_identity = Stage8bP1eM10IdentityV1 {
            close_ts_utc_ms: candidate.close_ts_utc_ms(),
            open_ts_utc_ms: candidate.open_ts_utc_ms(),
            payload_sha256: candidate.payload_sha256().to_string(),
            redis_id: candidate.redis_id().to_string(),
            semantic_id_sha256: candidate.semantic_id_sha256().to_string(),
        };
        let decision = owner.stage8b_p1d1_command_decision_binding().unwrap();
        let (request_id, command_sha256) = decision.stage8b_p1e_test_candidate_parts();
        let committed = match bind_stage8b_p1e_market_schedule(
            owner,
            Stage8bP1eVerifiedScheduleSnapshotV1 {
                redis_stream_id: format!("{candidate_close_ts_utc_ms}-1"),
                accepted: setup.accepted_schedule.take().unwrap(),
            },
            &Stage8bP1eShutdownLatchV1::new(),
            &predecessor,
            &candidate_identity,
            request_id,
            command_sha256,
            DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
                .unwrap()
                .with_timezone(&Utc),
            &setup.commitment_key,
        )
        .unwrap()
        {
            Stage8bP1eScheduleBindingCommitV1::Committed(owner) => owner,
            Stage8bP1eScheduleBindingCommitV1::StoppedBeforeBinding { .. } => {
                panic!("clear latch D must commit the exact market V4")
            }
        };
        let binding_record_id = committed.receipt().journal_record_id().to_string();
        assert_eq!(committed.receipt().covering_seal_generation(), before.0 + 1);
        drop(committed);
        let crate::Stage7bRestartOutcome::P1eScheduleBindingCommitted(committed) =
            restart_schedule_fixture(&setup)
        else {
            panic!("covered market V4 must restart as the exact committed binding")
        };
        assert_eq!(committed.receipt().journal_record_id(), binding_record_id);
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            *committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must preserve recovered market continuation"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue {
            mut owner,
            authority,
        } = continue_stage8b_p1e_market_schedule(permit, &Stage8bP1eShutdownLatchV1::new())
            .unwrap()
        else {
            panic!("clear latch F must issue the recovered market-only authority")
        };
        let eligibility = owner
            .stage8b_p1d1_execution_eligibility(
                authority,
                candidate.into_p1d1_execution_evidence().unwrap(),
            )
            .unwrap();
        let provider = owner
            .admit_p1d1_eligible_market_dispatch(eligibility)
            .unwrap();
        let outcome = provider.execute();
        let ready = owner
            .commit_stage8b_p1d2_ack(outcome, &setup.commitment_key)
            .unwrap()
            .commit_truth(&setup.commitment_key)
            .unwrap()
            .into_ready_after_source_resolution();
        let after_effect = ready.stage8b_p1e_test_checkpoint_snapshot();
        assert_eq!(after_effect.0, before.0 + 3);
        assert_eq!(after_effect.1, before.1 + 5);
        assert_eq!(after_effect.2, None);
        drop(ready);
        let restart = restart_schedule_fixture(&setup);
        let audit = restart
            .stage8b_p1d4_test_runtime_audit()
            .expect("post-market restart must remain fully auditable");
        assert_eq!(audit.dispatch_v1_total, 1);
        assert_eq!(audit.order_v1_total, 1);
        assert_eq!(audit.trade_v1_total, 1);
        assert_eq!(audit.request_finalized_v1_total, 1);
        assert_eq!(audit.durable_outcomes, 0);
        assert_eq!(audit.truth_bearing_outcomes, 0);
        assert_eq!(
            audit.journal_lifecycle_sequences.len() as u64,
            after_effect.1
        );
        assert_eq!(audit.lifecycle_sequence, after_effect.1);
        drop(restart);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_v4_market_rejects_same_redis_id_with_different_payload_before_effect() {
        let (setup, owner, candidate) = schedule_ready_fixture();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();
        let committed = owner
            .commit_stage8b_p1e_schedule_binding(
                candidate,
                DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
                    .unwrap()
                    .with_timezone(&Utc),
                &setup.commitment_key,
            )
            .unwrap();
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must continue"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } =
            continue_stage8b_p1e_market_schedule(permit, &Stage8bP1eShutdownLatchV1::new())
                .unwrap()
        else {
            panic!("clear latch F must issue the market authority")
        };
        let before_effect = owner.stage8b_p1e_test_checkpoint_snapshot();
        let close_ts_utc_ms = setup.market_predecessor.close_ts_utc_ms + 600_000;
        let conflicting_bytes =
            p1e_test_canonical_m10(operational_identity.clone(), close_ts_utc_ms, 2_180);
        let conflicting =
            crate::parse_stage8b_p1_canonical_m10(&conflicting_bytes, &operational_identity)
                .unwrap();
        assert_eq!(
            conflicting.redis_id(),
            format!("{close_ts_utc_ms}-0"),
            "counterexample must preserve the V4 Redis ID"
        );
        assert!(owner
            .stage8b_p1d1_execution_eligibility(
                authority,
                conflicting.into_p1d1_execution_evidence().unwrap(),
            )
            .is_err());
        assert_eq!(owner.stage8b_p1e_test_checkpoint_snapshot(), before_effect);
        drop(owner);
        let restarted = restart_schedule_fixture(&setup);
        assert_eq!(
            restarted
                .stage8b_p1d4_test_runtime_audit()
                .unwrap()
                .dispatch_v1_total,
            0,
            "identity conflict must not append a dispatch or invoke the provider"
        );
        drop(restarted);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_v4_market_recovers_dispatch_outcome_ahead_of_ack_replacement_once() {
        let (setup, owner, candidate) = schedule_ready_fixture();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();
        let close_ts_utc_ms = setup.market_predecessor.close_ts_utc_ms + 600_000;
        let exact_bytes =
            p1e_test_canonical_m10(operational_identity.clone(), close_ts_utc_ms, 2_175);
        let exact =
            crate::parse_stage8b_p1_canonical_m10(&exact_bytes, &operational_identity).unwrap();
        let committed = owner
            .commit_stage8b_p1e_schedule_binding(
                candidate,
                DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
                    .unwrap()
                    .with_timezone(&Utc),
                &setup.commitment_key,
            )
            .unwrap();
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must continue"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue {
            mut owner,
            authority,
        } = continue_stage8b_p1e_market_schedule(permit, &Stage8bP1eShutdownLatchV1::new())
            .unwrap()
        else {
            panic!("clear latch F must issue market authority")
        };
        let covered_v4 = owner.stage8b_p1e_test_checkpoint_snapshot();
        let eligibility = owner
            .stage8b_p1d1_execution_eligibility(
                authority,
                exact.into_p1d1_execution_evidence().unwrap(),
            )
            .unwrap();
        let outcome = owner
            .admit_p1d1_eligible_market_dispatch(eligibility)
            .unwrap()
            .execute();
        crate::stage8a4_i3_test_fail_before_covering_seal(&mut owner);
        assert!(matches!(
            owner.commit_stage8b_p1d2_ack(outcome, &setup.commitment_key),
            Err(crate::Stage7bRecoveryError::Runtime(
                strategy_runtime_core::Stage6dLiveCoreError::JournalMutationMayHaveOccurred
            ))
        ));

        let restarted = restart_schedule_fixture(&setup);
        let before_recovery = restarted
            .stage8b_p1d4_test_runtime_audit()
            .expect("journal-ahead market outcome must remain auditable");
        let pending = match restarted {
            crate::Stage7bRestartOutcome::P1d2PreAckPending(pending) => pending,
            crate::Stage7bRestartOutcome::Blocked(blocked) => panic!(
                "V4-prefixed market outcome blocked during exact ACK recovery: {:?}",
                blocked.reason()
            ),
            crate::Stage7bRestartOutcome::P1d4GeneratedMarketPreAckPending(_) => {
                panic!("V4-prefixed market outcome was classified as generated-Market pre-ACK")
            }
            _ => panic!("V4-prefixed market outcome must enter exact ACK recovery"),
        };
        let conflicting_bytes =
            p1e_test_canonical_m10(operational_identity.clone(), close_ts_utc_ms, 2_180);
        let conflicting =
            crate::parse_stage8b_p1_canonical_m10(&conflicting_bytes, &operational_identity)
                .unwrap();
        assert!(pending
            .commit_reconstructed_ack(
                conflicting.into_p1d1_execution_evidence().unwrap(),
                &setup.commitment_key,
            )
            .is_err());

        let restarted = restart_schedule_fixture(&setup);
        assert_eq!(
            restarted
                .stage8b_p1d4_test_runtime_audit()
                .unwrap()
                .journal_lifecycle_sequences,
            before_recovery.journal_lifecycle_sequences,
            "conflicting recovery M10 must append neither finalization nor replacement"
        );
        let crate::Stage7bRestartOutcome::P1d2PreAckPending(pending) = restarted else {
            panic!("rejected recovery identity must leave the exact ACK frontier recoverable")
        };
        let exact =
            crate::parse_stage8b_p1_canonical_m10(&exact_bytes, &operational_identity).unwrap();
        let truth = pending
            .commit_reconstructed_ack(
                exact.into_p1d1_execution_evidence().unwrap(),
                &setup.commitment_key,
            )
            .unwrap()
            .commit_truth(&setup.commitment_key)
            .unwrap();
        assert_eq!(truth.recovery_seal_generation(), covered_v4.0 + 2);
        drop(truth);

        let restarted = restart_schedule_fixture(&setup);
        let after_recovery = restarted
            .stage8b_p1d4_test_runtime_audit()
            .expect("second restart must preserve the recovered market outcome");
        assert_eq!(
            after_recovery.dispatch_v1_total,
            before_recovery.dispatch_v1_total
        );
        assert_eq!(
            after_recovery.order_v1_total,
            before_recovery.order_v1_total
        );
        assert_eq!(
            after_recovery.trade_v1_total,
            before_recovery.trade_v1_total
        );
        assert_eq!(
            after_recovery.journal_lifecycle_sequences,
            before_recovery.journal_lifecycle_sequences
        );
        drop(restarted);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_v4_working_untouched_reaches_existing_effect_once() {
        use strategy_runtime_core::Stage8bP1d3LaterObservation;

        let (mut setup, owner, predecessor) = p1e_test_working_limit_fixture();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();
        let before = owner.stage8b_p1e_test_checkpoint_snapshot();
        let (active_order, transition_sha256, exact_predecessor) = owner
            .stage8b_p1e_working_binding_parts()
            .expect("working binding material must be authenticated");
        assert_eq!(predecessor, exact_predecessor);
        let candidate = p1e_test_m10_identity(
            &operational_identity,
            P1E_TEST_LATER_CANDIDATE_CLOSE_MS,
            2_220,
        );
        let snapshot = Stage8bP1eVerifiedScheduleSnapshotV1 {
            redis_stream_id: format!("{}-1", P1E_TEST_LATER_CANDIDATE_CLOSE_MS),
            accepted: setup.accepted_schedule.take().unwrap(),
        };
        let committed = match bind_stage8b_p1e_working_limit_schedule(
            owner,
            snapshot,
            &Stage8bP1eShutdownLatchV1::new(),
            &predecessor,
            &candidate,
            active_order.as_str(),
            transition_sha256,
            DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
                .unwrap()
                .with_timezone(&Utc),
            &setup.commitment_key,
        )
        .unwrap()
        {
            Stage8bP1eScheduleBindingCommitV1::Committed(owner) => *owner,
            Stage8bP1eScheduleBindingCommitV1::StoppedBeforeBinding { .. } => {
                panic!("clear latch D must commit the exact V4 binding")
            }
        };
        assert_eq!(committed.receipt().covering_seal_generation(), before.0 + 1);
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must preserve exact V4 continuation"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } =
            continue_stage8b_p1e_schedule_step(permit, &Stage8bP1eShutdownLatchV1::new()).unwrap()
        else {
            panic!("clear latch F must issue the Working-only authority")
        };
        let evidence = p1e_test_p1d3_evidence(
            &operational_identity,
            P1E_TEST_LATER_CANDIDATE_CLOSE_MS,
            2_220,
        );
        let crate::Stage8bP1d3LaterCommitOutcome::SemanticPending(pending) = owner
            .commit_stage8b_p1d3_later_limit(
                Stage8bP1d3LaterObservation::Candidate {
                    evidence: Box::new(evidence),
                    schedule: authority,
                },
                &setup.commitment_key,
            )
            .unwrap()
        else {
            panic!("one untouched Working observation must reach the existing effect boundary")
        };
        let after_effect = pending.stage8b_p1e_test_checkpoint_snapshot();
        assert_eq!(after_effect.0, before.0 + 2);
        assert_eq!(after_effect.1, before.1 + 1);
        assert_eq!(after_effect.2, before.2);
        let (ready, callback_intent_count) = p1e_test_commit_exact_later_semantic(
            *pending,
            &operational_identity,
            2_220,
            &setup.commitment_key,
        );
        assert_eq!(callback_intent_count, 0);
        let after_callback = ready.stage8b_p1e_test_checkpoint_snapshot();
        assert_eq!(after_callback.0, before.0 + 3);
        assert_eq!(after_callback.1, before.1 + 1);
        assert_eq!(after_callback.2, before.2);
        drop(ready);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_v4_working_rejects_same_redis_id_with_different_payload_before_effect() {
        use strategy_runtime_core::Stage8bP1d3LaterObservation;

        let (mut setup, owner, predecessor) = p1e_test_working_limit_fixture();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();
        let (active_order, transition_sha256, _) = owner
            .stage8b_p1e_working_binding_parts()
            .expect("working binding material must be authenticated");
        let exact = p1e_test_m10_identity(
            &operational_identity,
            P1E_TEST_LATER_CANDIDATE_CLOSE_MS,
            2_230,
        );
        let committed = match bind_stage8b_p1e_working_limit_schedule(
            owner,
            Stage8bP1eVerifiedScheduleSnapshotV1 {
                redis_stream_id: format!("{}-1", P1E_TEST_LATER_CANDIDATE_CLOSE_MS),
                accepted: setup.accepted_schedule.take().unwrap(),
            },
            &Stage8bP1eShutdownLatchV1::new(),
            &predecessor,
            &exact,
            active_order.as_str(),
            transition_sha256,
            DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
                .unwrap()
                .with_timezone(&Utc),
            &setup.commitment_key,
        )
        .unwrap()
        {
            Stage8bP1eScheduleBindingCommitV1::Committed(owner) => *owner,
            _ => panic!("clear latch D must commit V4"),
        };
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must continue"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } =
            continue_stage8b_p1e_schedule_step(permit, &Stage8bP1eShutdownLatchV1::new()).unwrap()
        else {
            panic!("clear latch F must issue Working authority")
        };
        let before_effect = owner.stage8b_p1e_test_checkpoint_snapshot();
        let conflicting = p1e_test_p1d3_evidence(
            &operational_identity,
            P1E_TEST_LATER_CANDIDATE_CLOSE_MS,
            2_220,
        );
        assert_eq!(conflicting.redis_id, exact.redis_id);
        assert_ne!(conflicting.payload_sha256, exact.payload_sha256);
        assert!(owner
            .commit_stage8b_p1d3_later_limit(
                Stage8bP1d3LaterObservation::Candidate {
                    evidence: Box::new(conflicting),
                    schedule: authority,
                },
                &setup.commitment_key,
            )
            .is_err());
        let crate::Stage7bRestartOutcome::P1eScheduleBindingCommitted(committed) =
            restart_schedule_fixture(&setup)
        else {
            panic!("identity conflict must preserve only the covered V4")
        };
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            *committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("covered V4 must remain recoverable"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } =
            continue_stage8b_p1e_schedule_step(permit, &Stage8bP1eShutdownLatchV1::new()).unwrap()
        else {
            panic!("covered V4 must retain its exact authority")
        };
        assert_eq!(owner.stage8b_p1e_test_checkpoint_snapshot(), before_effect);
        drop(authority);
        drop(owner);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_v4_working_fill_survives_binding_and_effect_restarts_without_duplication() {
        use strategy_runtime_core::Stage8bP1d3LaterObservation;

        let (mut setup, owner, predecessor) = p1e_test_working_limit_fixture();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();
        let before = owner.stage8b_p1e_test_checkpoint_snapshot();
        let (active_order, transition_sha256, _) = owner
            .stage8b_p1e_working_binding_parts()
            .expect("working binding material must be authenticated");
        let candidate = p1e_test_m10_identity(
            &operational_identity,
            P1E_TEST_LATER_CANDIDATE_CLOSE_MS,
            2_230,
        );
        let snapshot = Stage8bP1eVerifiedScheduleSnapshotV1 {
            redis_stream_id: format!("{}-1", P1E_TEST_LATER_CANDIDATE_CLOSE_MS),
            accepted: setup.accepted_schedule.take().unwrap(),
        };
        let committed = match bind_stage8b_p1e_working_limit_schedule(
            owner,
            snapshot,
            &Stage8bP1eShutdownLatchV1::new(),
            &predecessor,
            &candidate,
            active_order.as_str(),
            transition_sha256,
            DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
                .unwrap()
                .with_timezone(&Utc),
            &setup.commitment_key,
        )
        .unwrap()
        {
            Stage8bP1eScheduleBindingCommitV1::Committed(owner) => owner,
            Stage8bP1eScheduleBindingCommitV1::StoppedBeforeBinding { .. } => {
                panic!("clear latch D must commit the exact V4 binding")
            }
        };
        let binding_record_id = committed.receipt().journal_record_id().to_string();
        drop(committed);
        let crate::Stage7bRestartOutcome::P1eScheduleBindingCommitted(committed) =
            restart_schedule_fixture(&setup)
        else {
            panic!("covered V4 must restart as the exact committed binding")
        };
        assert_eq!(committed.receipt().journal_record_id(), binding_record_id);
        assert_eq!(committed.receipt().covering_seal_generation(), before.0 + 1);
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            *committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must preserve recovered exact V4 continuation"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } =
            continue_stage8b_p1e_schedule_step(permit, &Stage8bP1eShutdownLatchV1::new()).unwrap()
        else {
            panic!("clear latch F must issue the recovered Working-only authority")
        };
        let evidence = p1e_test_p1d3_evidence(
            &operational_identity,
            P1E_TEST_LATER_CANDIDATE_CLOSE_MS,
            2_230,
        );
        let crate::Stage8bP1d3LaterCommitOutcome::SemanticPending(pending) = owner
            .commit_stage8b_p1d3_later_limit(
                Stage8bP1d3LaterObservation::Candidate {
                    evidence: Box::new(evidence),
                    schedule: authority,
                },
                &setup.commitment_key,
            )
            .unwrap()
        else {
            panic!("one touched Working observation must commit fill truth")
        };
        let after_effect = pending.stage8b_p1e_test_checkpoint_snapshot();
        let expected_semantic_source = pending.source_binding().unwrap();
        drop(pending);
        let restarted = match restart_schedule_fixture(&setup) {
            crate::Stage7bRestartOutcome::P1d3SemanticPending(restarted) => *restarted,
            crate::Stage7bRestartOutcome::P1SemanticPrepublicationReady(prepublication) => {
                let (ready, _, _) = prepublication.into_p1c_parts();
                ready
                    .into_stage8b_p1d3_pending_semantic_for_exact_source(&expected_semantic_source)
                    .unwrap()
            }
            _ => panic!("post-effect restart must expose only the exact pending callback"),
        };
        assert_eq!(
            restarted.stage8b_p1e_test_checkpoint_snapshot(),
            after_effect,
            "restart must not duplicate V4 or fill effect"
        );
        let (ready, callback_intent_count) = p1e_test_commit_exact_later_semantic(
            restarted,
            &operational_identity,
            2_230,
            &setup.commitment_key,
        );
        assert_eq!(callback_intent_count, 1);
        let after_callback = ready.stage8b_p1e_test_checkpoint_snapshot();
        assert_eq!(after_effect.0, before.0 + 2);
        assert_eq!(after_effect.1, before.1 + 2);
        assert_eq!(after_effect.2, before.2);
        assert_eq!(after_callback.0, before.0 + 3);
        assert_eq!(after_callback.1, before.1 + 3);
        assert_eq!(after_callback.2, before.2);
        drop(ready);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_v4_working_fill_recovers_outcome_ahead_of_replacement_once() {
        use strategy_runtime_core::Stage8bP1d3LaterObservation;

        let (setup, mut owner, authority, evidence) = p1e_test_bound_working_schedule(2_230);
        let covered_v4 = owner.stage8b_p1e_test_checkpoint_snapshot();
        crate::stage8a4_i3_test_fail_before_covering_seal(&mut owner);
        assert!(matches!(
            owner.commit_stage8b_p1d3_later_limit(
                Stage8bP1d3LaterObservation::Candidate {
                    evidence: Box::new(evidence),
                    schedule: authority,
                },
                &setup.commitment_key,
            ),
            Err(crate::Stage7bRecoveryError::Runtime(
                strategy_runtime_core::Stage6dLiveCoreError::JournalMutationMayHaveOccurred
            ))
        ));

        let restarted = restart_schedule_fixture(&setup);
        let before_recovery = restarted
            .stage8b_p1d4_test_runtime_audit()
            .expect("journal-ahead Working outcome must remain auditable");
        let crate::Stage7bRestartOutcome::P1d3PreAckPending(pending) = restarted else {
            panic!("V4 plus uncovered Working outcome must enter exact pre-seal recovery")
        };
        let crate::recovery::Stage8bP1d3RecoveredCommitOutcome::SemanticCallbackPending(pending) =
            pending
                .commit_reconstructed_transition(&setup.commitment_key)
                .unwrap()
        else {
            panic!("recovery must commit only the exact terminal replacement")
        };
        let recovered_snapshot = pending.stage8b_p1e_test_checkpoint_snapshot();
        assert_eq!(recovered_snapshot.0, covered_v4.0 + 1);
        assert_eq!(recovered_snapshot.1, covered_v4.1 + 1);
        drop(pending);

        let restarted = restart_schedule_fixture(&setup);
        let after_recovery = restarted
            .stage8b_p1d4_test_runtime_audit()
            .expect("second restart must preserve the recovered Working outcome");
        assert_eq!(
            after_recovery.durable_outcomes,
            before_recovery.durable_outcomes
        );
        assert_eq!(
            after_recovery.journal_lifecycle_sequences, before_recovery.journal_lifecycle_sequences,
            "second restart must append neither another outcome nor another effect"
        );
        drop(restarted);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_v4_day_expiry_reaches_existing_effect_and_restart_once() {
        use strategy_runtime_core::Stage8bP1d3LaterObservation;

        let (mut setup, owner, last_evaluated) = p1e_test_working_limit_fixture();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();
        let before = owner.stage8b_p1e_test_checkpoint_snapshot();
        let (active_order, transition_sha256, exact_last_evaluated) = owner
            .stage8b_p1e_working_binding_parts()
            .expect("working binding material must be authenticated");
        assert_eq!(last_evaluated, exact_last_evaluated);
        p1e_test_install_closed_schedule(&mut setup, &owner, P1E_TEST_INITIAL_CANDIDATE_CLOSE_MS);
        let predecessor = p1e_test_m10_identity(
            &operational_identity,
            P1E_TEST_PLACE_DECISION_CLOSE_MS,
            2_210,
        );
        let snapshot = Stage8bP1eVerifiedScheduleSnapshotV1 {
            redis_stream_id: format!("{}-1", P1E_TEST_INITIAL_CANDIDATE_CLOSE_MS),
            accepted: setup.accepted_schedule.take().unwrap(),
        };
        let trusted_now = Utc
            .timestamp_millis_opt(P1E_TEST_INITIAL_CANDIDATE_CLOSE_MS)
            .single()
            .unwrap();
        let committed = match bind_stage8b_p1e_day_expiry_schedule(
            owner,
            snapshot,
            &Stage8bP1eShutdownLatchV1::new(),
            &predecessor,
            &last_evaluated,
            trusted_now,
            active_order.as_str(),
            transition_sha256,
            trusted_now,
            &setup.commitment_key,
        )
        .unwrap()
        {
            Stage8bP1eScheduleBindingCommitV1::Committed(owner) => owner,
            Stage8bP1eScheduleBindingCommitV1::StoppedBeforeBinding { .. } => {
                panic!("clear latch D must commit the exact expiry V4")
            }
        };
        let binding_record_id = committed.receipt().journal_record_id().to_string();
        drop(committed);
        let crate::Stage7bRestartOutcome::P1eScheduleBindingCommitted(committed) =
            restart_schedule_fixture(&setup)
        else {
            panic!("covered expiry V4 must restart as the exact committed binding")
        };
        assert_eq!(committed.receipt().journal_record_id(), binding_record_id);
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            *committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must preserve recovered expiry continuation"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } =
            continue_stage8b_p1e_day_expiry_schedule(permit, &Stage8bP1eShutdownLatchV1::new())
                .unwrap()
        else {
            panic!("clear latch F must issue the recovered expiry-only authority")
        };
        let crate::Stage8bP1d3LaterCommitOutcome::Ready(ready) = owner
            .commit_stage8b_p1d3_later_limit(
                Stage8bP1d3LaterObservation::DayExpiry { authority },
                &setup.commitment_key,
            )
            .unwrap()
        else {
            panic!("exact signed day boundary must reach the existing expiry effect")
        };
        let after_effect = ready.stage8b_p1e_test_checkpoint_snapshot();
        assert_eq!(after_effect.0, before.0 + 2);
        assert_eq!(after_effect.1, before.1 + 2);
        assert_eq!(after_effect.2, before.2);
        drop(ready);
        let restarted = match restart_schedule_fixture(&setup) {
            crate::Stage7bRestartOutcome::Ready(restarted) => *restarted,
            crate::Stage7bRestartOutcome::P1SemanticPrepublicationReady(prepublication) => {
                let (restarted, _, _) = (*prepublication).into_p1c_parts();
                restarted
            }
            _ => panic!("post-expiry restart must expose the exact terminal replacement"),
        };
        assert_eq!(
            restarted.stage8b_p1e_test_checkpoint_snapshot(),
            after_effect,
            "restart must not duplicate V4 or expiry effect"
        );
        drop(restarted);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_v4_day_expiry_recovers_outcome_ahead_of_replacement_once() {
        use strategy_runtime_core::Stage8bP1d3LaterObservation;

        let (setup, mut owner, authority) = p1e_test_bound_day_expiry_schedule();
        let covered_v4 = owner.stage8b_p1e_test_checkpoint_snapshot();
        crate::stage8a4_i3_test_fail_before_covering_seal(&mut owner);
        assert!(matches!(
            owner.commit_stage8b_p1d3_later_limit(
                Stage8bP1d3LaterObservation::DayExpiry { authority },
                &setup.commitment_key,
            ),
            Err(crate::Stage7bRecoveryError::Runtime(
                strategy_runtime_core::Stage6dLiveCoreError::JournalMutationMayHaveOccurred
            ))
        ));

        let restarted = restart_schedule_fixture(&setup);
        let before_recovery = restarted
            .stage8b_p1d4_test_runtime_audit()
            .expect("journal-ahead expiry must remain auditable");
        let crate::Stage7bRestartOutcome::P1d3PreAckPending(pending) = restarted else {
            panic!("V4 plus uncovered expiry outcome must enter exact pre-seal recovery")
        };
        let crate::recovery::Stage8bP1d3RecoveredCommitOutcome::Ready(ready) = pending
            .commit_reconstructed_transition(&setup.commitment_key)
            .unwrap()
        else {
            panic!("expiry recovery must commit one terminal replacement")
        };
        let after_commit = ready.stage8b_p1e_test_checkpoint_snapshot();
        assert_eq!(after_commit.0, covered_v4.0 + 1);
        assert_eq!(after_commit.1, covered_v4.1 + 1);
        drop(ready);

        let restarted = restart_schedule_fixture(&setup);
        let after_recovery = restarted
            .stage8b_p1d4_test_runtime_audit()
            .expect("second restart must preserve the recovered expiry");
        assert_eq!(
            after_recovery.durable_outcomes,
            before_recovery.durable_outcomes
        );
        assert_eq!(
            after_recovery.journal_lifecycle_sequences,
            before_recovery.journal_lifecycle_sequences
        );
        drop(restarted);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_closed_cancel_authority_cannot_enter_working_evaluation() {
        use strategy_runtime_core::Stage8bP1d3LaterObservation;

        let (setup, owner, authority, evidence) = p1e_test_cancel_binding_fixture();
        let before = owner.stage8b_p1e_test_checkpoint_snapshot();
        assert!(owner
            .commit_stage8b_p1d3_later_limit(
                Stage8bP1d3LaterObservation::Candidate {
                    evidence: Box::new(evidence),
                    schedule: authority,
                },
                &setup.commitment_key,
            )
            .is_err());
        let restarted = match restart_schedule_fixture(&setup) {
            crate::Stage7bRestartOutcome::P1eScheduleBindingCommitted(committed) => {
                let permit = match resume_stage8b_p1e_committed_schedule_binding(
                    *committed,
                    &Stage8bP1eShutdownLatchV1::new(),
                ) {
                    Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
                    _ => panic!("failed route attempt must retain the exact cancel binding"),
                };
                let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, .. } =
                    continue_stage8b_p1e_schedule_step(permit, &Stage8bP1eShutdownLatchV1::new())
                        .unwrap()
                else {
                    panic!("failed route attempt must leave the cancel authority recoverable")
                };
                *owner
            }
            _ => panic!("route rejection must not append or seal an effect"),
        };
        assert_eq!(restarted.stage8b_p1e_test_checkpoint_snapshot(), before);
        drop(restarted);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_closed_cancel_authority_reaches_cancel_effect_once() {
        let (setup, owner, authority, evidence) = p1e_test_cancel_binding_fixture();
        let before = owner.stage8b_p1e_test_checkpoint_snapshot();
        let crate::recovery::Stage8bP1d3CancelCommitOutcome::AckCommitted(ack) = owner
            .commit_stage8b_p1d3_cancel(evidence, authority, &setup.commitment_key)
            .unwrap()
        else {
            panic!("untouched target must reach the existing cancel ACK effect")
        };
        let truth = ack.commit_truth(&setup.commitment_key).unwrap();
        let after_effect = truth.stage8b_p1d3_test_restart_snapshot();
        assert_eq!(after_effect.0, before.0 + 2);
        assert_eq!(after_effect.1, before.1 + 3);
        drop(truth);
        let restarted = restart_schedule_fixture(&setup);
        assert_eq!(
            restarted
                .stage8b_p1d4_test_runtime_audit()
                .unwrap()
                .durable_outcomes,
            2,
            "restart must preserve one Working LIMIT and one cancel outcome"
        );
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_v4_cancel_recovers_dispatch_and_outcome_ahead_of_replacement_once() {
        let (setup, mut owner, authority, evidence) = p1e_test_cancel_binding_fixture();
        let covered_v4 = owner.stage8b_p1e_test_checkpoint_snapshot();
        crate::stage8a4_i3_test_fail_before_covering_seal(&mut owner);
        assert!(matches!(
            owner.commit_stage8b_p1d3_cancel(evidence, authority, &setup.commitment_key),
            Err(crate::Stage7bRecoveryError::Runtime(
                strategy_runtime_core::Stage6dLiveCoreError::JournalMutationMayHaveOccurred
            ))
        ));

        let restarted = restart_schedule_fixture(&setup);
        let before_recovery = restarted
            .stage8b_p1d4_test_runtime_audit()
            .expect("journal-ahead cancel must remain auditable");
        let crate::Stage7bRestartOutcome::P1d3PreAckPending(pending) = restarted else {
            panic!("V4 plus uncovered cancel outcome must enter exact pre-seal recovery")
        };
        let crate::recovery::Stage8bP1d3RecoveredCommitOutcome::AckCommitted(ack) = pending
            .commit_reconstructed_transition(&setup.commitment_key)
            .unwrap()
        else {
            panic!("cancel recovery must reconstruct only S_ack")
        };
        let truth = ack.commit_truth(&setup.commitment_key).unwrap();
        let after_commit = truth.stage8b_p1d3_test_restart_snapshot();
        assert_eq!(after_commit.0, covered_v4.0 + 2);
        assert_eq!(after_commit.1, covered_v4.1 + 3);
        drop(truth);

        let restarted = restart_schedule_fixture(&setup);
        let after_recovery = restarted
            .stage8b_p1d4_test_runtime_audit()
            .expect("second restart must preserve the recovered cancel");
        assert_eq!(
            after_recovery.durable_outcomes,
            before_recovery.durable_outcomes
        );
        assert_eq!(
            after_recovery.journal_lifecycle_sequences,
            before_recovery.journal_lifecycle_sequences
        );
        drop(restarted);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn signed_v4_cancel_rejects_exact_id_with_conflicting_semantic_identity_before_dispatch() {
        let (setup, owner, authority, mut evidence) = p1e_test_cancel_binding_fixture();
        let before_effect = owner.stage8b_p1e_test_checkpoint_snapshot();
        evidence.semantic_id_sha256 = "ef".repeat(32);
        assert!(owner
            .commit_stage8b_p1d3_cancel(evidence, authority, &setup.commitment_key)
            .is_err());
        let crate::Stage7bRestartOutcome::P1eScheduleBindingCommitted(committed) =
            restart_schedule_fixture(&setup)
        else {
            panic!("identity conflict must preserve only the exact cancel V4")
        };
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            *committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("cancel V4 must remain recoverable"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } =
            continue_stage8b_p1e_schedule_step(permit, &Stage8bP1eShutdownLatchV1::new()).unwrap()
        else {
            panic!("cancel V4 must retain exact Cancel authority")
        };
        assert_eq!(owner.stage8b_p1e_test_checkpoint_snapshot(), before_effect);
        drop(authority);
        drop(owner);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn v4_binding_uses_existing_append_cover_reread_chain() {
        let (setup, owner, candidate) = schedule_ready_fixture();
        let prior_generation = owner.committed_seal().unwrap().seal_generation();
        let committed = owner
            .commit_stage8b_p1e_schedule_binding(
                candidate,
                DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
                    .unwrap()
                    .with_timezone(&Utc),
                &setup.commitment_key,
            )
            .unwrap();
        assert_eq!(
            committed.receipt().covering_seal_generation(),
            prior_generation + 1
        );
        assert_eq!(committed.receipt().lifecycle_sequence(), 2);
        assert_eq!(committed.receipt().journal_record_id().len(), 64);
        assert_eq!(
            committed.receipt().post_append_checkpoint_sha256().len(),
            64
        );
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("clear latch E must continue"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue {
            owner: ready,
            authority: _authority,
        } = continue_stage8b_p1e_market_schedule(permit, &Stage8bP1eShutdownLatchV1::new())
            .unwrap()
        else {
            panic!("clear latch F must continue");
        };
        assert!(ready.recovery_ready());
        drop(ready);
        std::fs::remove_dir_all(setup.parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[test]
    fn v4_journal_ahead_restart_commits_one_seal_and_reaches_market_effect_once() {
        let (setup, mut owner, candidate) = schedule_ready_fixture();
        let prior_generation = owner.committed_seal().unwrap().seal_generation();
        let before = owner.stage8b_p1e_test_checkpoint_snapshot();
        let operational_identity = owner.stage8b_p1_operational_identity_sha256().to_string();
        let candidate_close_ts_utc_ms = setup.market_predecessor.close_ts_utc_ms + 600_000;
        let candidate_bytes = p1e_test_canonical_m10(
            operational_identity.clone(),
            candidate_close_ts_utc_ms,
            2_175,
        );
        let candidate_bar =
            crate::parse_stage8b_p1_canonical_m10(&candidate_bytes, &operational_identity).unwrap();
        crate::stage8a4_i3_test_fail_before_covering_seal(&mut owner);
        let error = owner
            .commit_stage8b_p1e_schedule_binding(
                candidate,
                DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
                    .unwrap()
                    .with_timezone(&Utc),
                &setup.commitment_key,
            )
            .err()
            .expect("fixture must stop after the durable V4 append");
        assert!(matches!(
            error,
            crate::Stage7bRecoveryError::Runtime(
                strategy_runtime_core::Stage6dLiveCoreError::JournalMutationMayHaveOccurred
            )
        ));

        let crate::Stage7bRestartOutcome::P1eScheduleBindingCommitted(committed) =
            restart_schedule_fixture(&setup)
        else {
            panic!("one exact journal-ahead V4 must complete its covering seal")
        };
        assert_eq!(
            committed.receipt().covering_seal_generation(),
            prior_generation + 1
        );
        assert_eq!(committed.receipt().lifecycle_sequence(), 2);
        let record_id = committed.receipt().journal_record_id().to_string();
        drop(committed);

        let crate::Stage7bRestartOutcome::P1eScheduleBindingCommitted(committed) =
            restart_schedule_fixture(&setup)
        else {
            panic!("covered V4 must recover as the same committed binding")
        };
        assert_eq!(committed.receipt().journal_record_id(), record_id);
        assert_eq!(
            committed.receipt().covering_seal_generation(),
            prior_generation + 1,
            "covered replay must not write a second seal"
        );
        let permit = match resume_stage8b_p1e_committed_schedule_binding(
            *committed,
            &Stage8bP1eShutdownLatchV1::new(),
        ) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("covered restart must retain the route-bound permit"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue {
            mut owner,
            authority,
        } = continue_stage8b_p1e_market_schedule(permit, &Stage8bP1eShutdownLatchV1::new())
            .unwrap()
        else {
            panic!("clear latch must issue only the recovered exact authority")
        };
        let eligibility = owner
            .stage8b_p1d1_execution_eligibility(
                authority,
                candidate_bar.into_p1d1_execution_evidence().unwrap(),
            )
            .unwrap();
        let provider = owner
            .admit_p1d1_eligible_market_dispatch(eligibility)
            .unwrap();
        let outcome = provider.execute();
        let ready = owner
            .commit_stage8b_p1d2_ack(outcome, &setup.commitment_key)
            .unwrap()
            .commit_truth(&setup.commitment_key)
            .unwrap()
            .into_ready_after_source_resolution();
        let after_effect = ready.stage8b_p1e_test_checkpoint_snapshot();
        assert_eq!(after_effect.0, before.0 + 3);
        assert_eq!(after_effect.1, before.1 + 5);
        assert_eq!(after_effect.2, None);
        drop(ready);
        let restart = restart_schedule_fixture(&setup);
        let audit = restart
            .stage8b_p1d4_test_runtime_audit()
            .expect("journal-ahead continuation must remain fully auditable");
        assert_eq!(audit.dispatch_v1_total, 1);
        assert_eq!(audit.order_v1_total, 1);
        assert_eq!(audit.trade_v1_total, 1);
        assert_eq!(audit.request_finalized_v1_total, 1);
        assert_eq!(
            audit.journal_lifecycle_sequences.len() as u64,
            after_effect.1
        );
        assert_eq!(audit.lifecycle_sequence, after_effect.1);
        drop(restart);
        std::fs::remove_dir_all(&setup.parent).unwrap();
    }
}
