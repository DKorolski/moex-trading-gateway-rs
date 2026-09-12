//! Stage 8B-P1-e newest-only signed schedule source.
//!
//! Schedule evidence is deliberately not a consumer-group workload.  The
//! production reader performs one bounded `XREVRANGE + - COUNT 64`, validates
//! the newest candidate without fallback and authenticates the retained
//! window only for conflict/high-water detection. Reading or verifying a
//! snapshot grants no strategy authority; authority is created only after the
//! exact transition is appended as Stage 6 V4 and covered by a reread seal.

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
    Stage8bP1eShutdownIntentV1, Stage8bP1eShutdownLatchV1,
};

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1eScheduleReadError {
    #[error("Redis schedule read failed")]
    Redis(#[from] redis::RedisError),
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

fn check_latch_e(
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
}

impl Stage8bP1eRedisScheduleReader {
    pub async fn connect(redis_url: &str) -> Result<Self, Stage8bP1eScheduleReadError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            connection: ConnectionManager::new(client).await?,
        })
    }

    async fn read_newest(
        &mut self,
        context: &Stage8bP1eScheduleVerificationContextV1,
    ) -> Result<Stage8bP1eNewestScheduleReadV1, Stage8bP1eScheduleReadError> {
        let reply: StreamRangeReply = redis::cmd("XREVRANGE")
            .arg(STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("+")
            .arg("-")
            .arg("COUNT")
            .arg(64)
            .query_async(&mut self.connection)
            .await?;
        verify_newest_reply(reply, context)
    }

    /// Latch C is evaluated before any Redis command. The result of a
    /// successful read is still non-authorizing and must pass latch D at the
    /// binding boundary.
    pub async fn read_newest_guarded(
        &mut self,
        context: &Stage8bP1eScheduleVerificationContextV1,
        latch: &Stage8bP1eShutdownLatchV1,
    ) -> Result<Stage8bP1eGuardedScheduleReadV1, Stage8bP1eScheduleReadError> {
        if let Some(receipt) = stop_receipt(
            latch,
            Stage8bP1eScheduleLatchCheckpointV1::BeforeScheduleRead,
        ) {
            return Ok(Stage8bP1eGuardedScheduleReadV1::Stopped(receipt));
        }
        Ok(Stage8bP1eGuardedScheduleReadV1::Read(
            self.read_newest(context).await?,
        ))
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
            || (older.semantic_revision < newer.semantic_revision
                && older.schedule_semantic_sha256 == newer.schedule_semantic_sha256)
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
    let owner = match check_latch_d(owner, latch) {
        Ok(owner) => owner,
        Err(decision) => return Ok(decision),
    };
    let binding = snapshot.accepted.prepare_market_binding(
        predecessor,
        candidate,
        strategy_request_id,
        canonical_command_sha256,
        snapshot.redis_stream_id,
    )?;
    commit_binding_and_check_latch_e(owner, binding, bound_at_utc, commitment_key, latch)
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
    let owner = match check_latch_d(owner, latch) {
        Ok(owner) => owner,
        Err(decision) => return Ok(decision),
    };
    let binding = snapshot.accepted.prepare_working_limit_binding(
        predecessor,
        candidate,
        active_broker_order_id,
        working_book_transition_sha256,
        snapshot.redis_stream_id,
    )?;
    commit_binding_and_check_latch_e(owner, binding, bound_at_utc, commitment_key, latch)
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
    let owner = match check_latch_d(owner, latch) {
        Ok(owner) => owner,
        Err(decision) => return Ok(decision),
    };
    let binding = snapshot.accepted.prepare_cancel_binding(
        predecessor,
        candidate,
        active_broker_order_id,
        working_book_transition_sha256,
        snapshot.redis_stream_id,
    )?;
    commit_binding_and_check_latch_e(owner, binding, bound_at_utc, commitment_key, latch)
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
    let owner = match check_latch_d(owner, latch) {
        Ok(owner) => owner,
        Err(decision) => return Ok(decision),
    };
    let binding = snapshot.accepted.prepare_day_expiry_binding(
        predecessor,
        last_evaluated_m10,
        trusted_now,
        active_broker_order_id,
        working_book_transition_sha256,
        snapshot.redis_stream_id,
    )?;
    commit_binding_and_check_latch_e(owner, binding, bound_at_utc, commitment_key, latch)
}

fn commit_binding_and_check_latch_e(
    owner: Stage7bRecoveryReadyOwner,
    binding: strategy_runtime_core::Stage8bP1eScheduleBindingCandidateV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    latch: &Stage8bP1eShutdownLatchV1,
) -> Result<Stage8bP1eScheduleBindingDecisionV1, Stage8bP1eScheduleReadError> {
    let committed =
        owner.commit_stage8b_p1e_schedule_binding(binding, bound_at_utc, commitment_key)?;
    Ok(check_latch_e(committed, latch))
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
mod tests {
    use super::*;
    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    use broker_core::{
        BrokerAccountId, BrokerInstrumentSpec, BrokerKind, BrokerMarketSessionState, BrokerSymbol,
        BrokerTruthSnapshot, Exchange, InstrumentMapEntry, InternalSymbol, Market, Money,
        Stage4AdoptionDisposition, Stage4BootstrapEvidenceSourceStatusSection,
        Stage4BrokerTruthBootstrapInput, Stage4BrokerTruthFreshnessInput,
        Stage4BrokerTruthSafetyBoundary, Stage4BrokerTruthSourceStatus,
    };
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
    fn bounded_window_rejects_reorder_and_same_sequence_conflict() {
        let newest = point(20, 2, 'b', '2');
        let reordered = point(21, 2, 'b', '3');
        assert!(validate_bounded_progression(&[newest.clone(), reordered], None).is_err());

        let mut conflicting = newest.clone();
        conflicting.envelope_sha256 = "9".repeat(64);
        assert!(validate_bounded_progression(&[newest, conflicting], None).is_err());
    }

    #[test]
    fn bounded_window_rejects_nonadvancing_time_and_false_revision_change() {
        let newer = point(20, 2, 'b', '2');
        let mut same_time = point(19, 1, 'a', '1');
        same_time.published_at_utc = newer.published_at_utc;
        assert!(validate_bounded_progression(&[newer.clone(), same_time], None).is_err());

        let false_revision = point(19, 1, 'b', '1');
        assert!(validate_bounded_progression(&[newer, false_revision], None).is_err());
    }

    #[test]
    fn bounded_window_rejects_durable_high_water_conflict() {
        let retained = point(20, 2, 'b', '2');
        let mut high_water = retained.clone();
        high_water.envelope_sha256 = "9".repeat(64);
        assert!(validate_bounded_progression(&[retained], Some(high_water)).is_err());
    }

    fn requested_latch() -> Stage8bP1eShutdownLatchV1 {
        let mut latch = Stage8bP1eShutdownLatchV1::new();
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
    fn signed_market_candidate(
        operational_identity_sha256: String,
        runtime_config_fingerprint_sha256: String,
        instrument_map_fingerprint_sha256: String,
        strategy_request_id: String,
        canonical_command_sha256: String,
        predecessor: strategy_runtime_core::Stage8bP1eM10IdentityV1,
    ) -> (
        strategy_runtime_core::Stage8bP1eScheduleBindingCandidateV1,
        String,
        DateTime<Utc>,
        DateTime<Utc>,
    ) {
        use strategy_runtime_core::{
            stage8b_p1e_canonical_json, stage8b_p1e_schedule_payload_sha256,
            stage8b_p1e_schedule_semantic_sha256, stage8b_p1e_schedule_unsigned_signature_sha256,
            stage8b_p1e_test_verify_schedule_envelope_with_key, Stage8bP1eNormalizedScheduleV2,
            Stage8bP1eScheduleEnvelopeV3, Stage8bP1eScheduleEvidenceKindV1,
            Stage8bP1eScheduleInstrumentV1, Stage8bP1eSchedulePayloadV2,
            Stage8bP1eScheduleRegistryV1, Stage8bP1eScheduleSemanticIdentityV1,
            Stage8bP1eScheduleSessionTypeV1, Stage8bP1eScheduleSessionV1,
            Stage8bP1eScheduleStateV1, Stage8bP1eScheduleVerificationContextV1,
            Stage8bP1eStage4EvidenceV2, Stage8bP1eStage4SemanticStateV1,
        };

        let now = DateTime::parse_from_rfc3339("2026-08-03T12:30:00.000000Z")
            .unwrap()
            .with_timezone(&Utc);
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
                    now, 60_000,
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
            end_utc: "2026-08-03T18:00:00.000000Z".to_string(),
            session_type: Stage8bP1eScheduleSessionTypeV1::TradableOpen,
            start_utc: "2026-08-03T06:00:00.000000Z".to_string(),
        }];
        let registry = Stage8bP1eScheduleRegistryV1 {
            registry_identity_sha256: "2".repeat(64),
            registry_version: "imoexf-v1".to_string(),
        };
        let payload = Stage8bP1eSchedulePayloadV2 {
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
                source_expires_at_utc: timestamp_text(now + chrono::Duration::seconds(60)),
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
        let semantic_identity = Stage8bP1eScheduleSemanticIdentityV1 {
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
        let candidate_open_ts_utc_ms = predecessor.close_ts_utc_ms;
        let candidate_close_ts_utc_ms = candidate_open_ts_utc_ms + 600_000;
        let candidate_m10 = strategy_runtime_core::Stage8bP1eM10IdentityV1 {
            close_ts_utc_ms: candidate_close_ts_utc_ms,
            open_ts_utc_ms: candidate_open_ts_utc_ms,
            payload_sha256: "9".repeat(64),
            redis_id: format!("{candidate_close_ts_utc_ms}-0"),
            semantic_id_sha256: "a".repeat(64),
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
        (candidate, public_key_hex, trust_from, trust_until)
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
        let (candidate, schedule_public_key_hex, schedule_key_valid_from, schedule_key_valid_until) =
            signed_market_candidate(
                operational_identity,
                fresh_runtime.stage5c_config_fingerprint(),
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                request_id,
                command_sha256,
                predecessor,
            );
        (
            ScheduleReadyTestSetup {
                parent,
                commitment_key,
                fresh_runtime,
                schedule_public_key_hex,
                schedule_key_valid_from,
                schedule_key_valid_until,
            },
            owner,
            candidate,
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
        let decision = check_latch_e(committed, &requested_latch());
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
        let permit = match check_latch_e(committed, &Stage8bP1eShutdownLatchV1::new()) {
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
    fn clear_latches_issue_one_route_bound_authority_after_one_binding_seal() {
        let (setup, committed, prior_generation) = committed_schedule_fixture();
        let permit = match check_latch_e(committed, &Stage8bP1eShutdownLatchV1::new()) {
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
        let permit = match check_latch_e(committed, &Stage8bP1eShutdownLatchV1::new()) {
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
    fn v4_journal_ahead_restart_commits_exactly_one_covering_seal() {
        let (setup, mut owner, candidate) = schedule_ready_fixture();
        let prior_generation = owner.committed_seal().unwrap().seal_generation();
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
        let permit = match check_latch_e(*committed, &Stage8bP1eShutdownLatchV1::new()) {
            Stage8bP1eScheduleBindingDecisionV1::Continue(permit) => permit,
            _ => panic!("covered restart must retain the route-bound permit"),
        };
        let Stage8bP1eScheduleAuthorityDecisionV1::Continue { owner, authority } =
            continue_stage8b_p1e_market_schedule(permit, &Stage8bP1eShutdownLatchV1::new())
                .unwrap()
        else {
            panic!("clear latch must issue only the recovered exact authority")
        };
        drop(authority);
        drop(owner);
        std::fs::remove_dir_all(&setup.parent).unwrap();
    }
}
