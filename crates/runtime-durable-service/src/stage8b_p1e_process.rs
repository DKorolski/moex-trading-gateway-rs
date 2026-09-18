//! Fixed-path process composition for the Stage 8B-P1-e paper supervisor.
//!
//! This boundary deliberately owns no FINAM dependency and no broker send
//! capability. Bootstrap and recovery are filesystem-only. `run` composes the
//! authenticated durable restart, verify-only Redis attachment and the sole
//! owner loop; it still has no operational activation or broker-send path.

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::Path,
    sync::Arc,
    time::Duration as StdDuration,
};

use chrono::{DateTime, Utc};
use strategy_runtime_core::Stage5gLifecycleCommitmentKey;

use crate::stage8b_p1_semantic::{
    resume_stage8b_p1e_cancel_with_signed_schedule_timeout,
    resume_stage8b_p1e_command_published_with_signed_schedule_timeout,
    resume_stage8b_p1e_day_expiry_with_signed_schedule,
    resume_stage8b_p1e_generated_market_with_signed_schedule_timeout,
    resume_stage8b_p1e_initial_limit_with_signed_schedule_timeout,
};

use crate::{
    acquire_stage8b_p1_journal_ahead_with_redis, acquire_stage8b_p1_prepublication_with_redis,
    acquire_stage8b_p1_zero_intent_ack_with_redis, acquire_stage8b_p1d2_ack_with_redis,
    acquire_stage8b_p1d2_pre_ack_with_redis, acquire_stage8b_p1d2_truth_with_redis,
    acquire_stage8b_p1d3_ack_with_redis, acquire_stage8b_p1d3_cancel_continuation_with_redis,
    acquire_stage8b_p1d3_dispatch_cancel_with_redis, acquire_stage8b_p1d3_pre_ack_with_redis,
    acquire_stage8b_p1d3_semantic_with_redis, acquire_stage8b_p1d3_truth_with_redis,
    acquire_stage8b_p1d4_ack_with_redis, acquire_stage8b_p1d4_dispatch_pending_with_redis,
    acquire_stage8b_p1d4_order_pending_with_redis, acquire_stage8b_p1d4_pre_ack_with_redis,
    acquire_stage8b_p1d4_pre_finalization_with_redis,
    acquire_stage8b_p1d4_prepublication_with_redis, acquire_stage8b_p1d4_truth_with_redis,
    acquire_stage8b_p1e_ready_pending_with_redis, admit_stage8b_p1e_ordinary_run_v1,
    attach_stage8b_p1e_verified_redis, authorize_stage8b_p1_first_boot,
    authorize_stage8b_p1e_pre_seal_recovery_v5, build_stage8b_p1_first_boot_source_v1,
    first_boot_stage8b_p1e_transaction_v5, load_stage8b_p1_commitment_key_from_systemd_credential,
    poll_stage8b_p1e_ready_fresh_with_redis, recover_stage8b_p1e_first_boot_adoption_v5,
    recover_stage8b_p1e_first_boot_pre_seal_from_supervisor_v5,
    resolve_stage8b_p1_zero_intent_ack_with_redis, resume_stage8b_p1_journal_ahead_with_redis,
    resume_stage8b_p1_prepublication_with_redis, resume_stage8b_p1d2_ack_with_redis,
    resume_stage8b_p1d2_pre_ack_with_redis, resume_stage8b_p1d2_truth_with_redis,
    resume_stage8b_p1d3_ack_with_redis, resume_stage8b_p1d3_cancel_continuation_with_redis,
    resume_stage8b_p1d3_pre_ack_with_redis, resume_stage8b_p1d3_semantic_with_redis,
    resume_stage8b_p1d3_truth_with_redis, resume_stage8b_p1d4_ack_with_redis,
    resume_stage8b_p1d4_dispatch_pending_with_redis, resume_stage8b_p1d4_order_pending_with_redis,
    resume_stage8b_p1d4_pre_ack_with_redis, resume_stage8b_p1d4_pre_finalization_with_redis,
    resume_stage8b_p1d4_prepublication_with_redis, resume_stage8b_p1d4_truth_with_redis,
    resume_stage8b_p1e_committed_cancel_with_redis, resume_stage8b_p1e_committed_day_expiry,
    resume_stage8b_p1e_committed_generated_market_with_redis,
    resume_stage8b_p1e_committed_initial_limit_with_redis,
    resume_stage8b_p1e_ready_source_with_redis,
    resume_stage8b_p1e_ready_working_limit_with_signed_schedule,
    route_stage8b_p1e_post_acquisition_v1, validate_stage8b_p1e_supervisor_config_v1,
    Stage7bRestartOutcome, Stage8bP1RedisCancelCommitOutcome,
    Stage8bP1RedisCancelContinuationPending, Stage8bP1RedisCommandPublished,
    Stage8bP1RedisFeedbackAckCommitted, Stage8bP1RedisFeedbackResolved,
    Stage8bP1RedisFeedbackTruthCommitted, Stage8bP1RedisGeneratedMarketAckCommitted,
    Stage8bP1RedisGeneratedMarketTruthCommitted, Stage8bP1RedisLimitAckCommitted,
    Stage8bP1RedisLimitResolved, Stage8bP1RedisLimitTruthCommitted,
    Stage8bP1RedisPreAckRecoveryOutcome, Stage8bP1RedisPrepublicationPending,
    Stage8bP1RedisSemanticCompositionOwner, Stage8bP1RedisSemanticCompositionTransport,
    Stage8bP1RedisSemanticError, Stage8bP1RedisSemanticOutcome,
    Stage8bP1RedisZeroIntentAckDisposition, Stage8bP1RedisZeroIntentAckResolved,
    Stage8bP1eAdoptionRecoveryActionV5, Stage8bP1eCoordinatorV1, Stage8bP1ePostAcquisitionOwnerV1,
    Stage8bP1ePreSealRecoveryActionV5, Stage8bP1ePublishedScheduleRouteV1,
    Stage8bP1eReadyFreshAcquisitionOutcomeV1, Stage8bP1eReadyPendingAcquisitionOutcomeV1,
    Stage8bP1eRecoveredCancelScheduleOutcomeV1, Stage8bP1eRecoveredDayExpiryScheduleOutcomeV1,
    Stage8bP1eRecoveredGeneratedMarketScheduleOutcomeV1,
    Stage8bP1eRecoveredInitialLimitScheduleOutcomeV1, Stage8bP1eRedisControlError,
    Stage8bP1eRedisControlV1, Stage8bP1eRestartKindV1, Stage8bP1eRetainedSourceReceiptV1,
    Stage8bP1eRoutedContinuationV1, Stage8bP1eRoutedPostAcquisitionDecisionV1,
    Stage8bP1eShutdownIntentV1, Stage8bP1eShutdownLatchV1, Stage8bP1eSignedCancelScheduleOutcomeV1,
    Stage8bP1eSignedDayExpiryScheduleOutcomeV1, Stage8bP1eSignedGeneratedMarketScheduleOutcomeV1,
    Stage8bP1eSignedInitialLimitScheduleOutcomeV1, Stage8bP1eSignedMarketScheduleOutcomeV1,
    Stage8bP1eSignedWorkingScheduleOutcomeV1, Stage8bP1eSupervisorConfigV1,
    Stage8bP1eSupervisorEventV1, Stage8bP1eValidatedSupervisorConfigV1,
    Stage8bP1eVerifiedRedisSessionV1, STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION,
    STAGE8B_P1E_SUPERVISOR_CONFIG_PATH,
};

use crate::stage8b_p1e_first_boot_transaction::next_stage8b_p1e_bootstrap_attempt_generation_v5;

const STAGE8B_P1E_BOOT_ID_PATH: &str = "/proc/sys/kernel/random/boot_id";
const STAGE8B_P1E_CONFIG_MAX_BYTES: u64 = 1024 * 1024;
const STAGE8B_P1E_SCHEDULE_FREE_RECOVERY_MAX_ROWS: usize = 8;
const STAGE8B_P1E_SCHEDULE_ACQUISITION_ATTEMPTS: u8 = 12;
const STAGE8B_P1E_SCHEDULE_ACQUISITION_DEADLINE_MS: u64 = 60_000;
const STAGE8B_P1E_SCHEDULE_ACQUISITION_INITIAL_BACKOFF_MS: u64 = 250;
const STAGE8B_P1E_SCHEDULE_ACQUISITION_MAX_BACKOFF_MS: u64 = 5_000;
const STAGE8B_P1E_SCHEDULE_SHUTDOWN_POLL_MS: u64 = 25;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage8bP1eProcessCommandV1 {
    ValidateConfig,
    Bootstrap,
    BootstrapRecover {
        transaction_id_sha256: String,
        action: Stage8bP1eProcessRecoveryActionV1,
    },
    Run,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eProcessRecoveryActionV1 {
    PreSeal(Stage8bP1ePreSealRecoveryActionV5),
    Adoption(Stage8bP1eAdoptionRecoveryActionV5),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage8bP1eProcessSuccessV1 {
    ConfigValid,
    BootstrapAdopted,
    RecoveryApplied,
    RunStopped,
}

/// Result of the pre-Redis S04 restart classification.  The attachable branch
/// can contain only outcomes for which S05 is legal; the two fail-closed
/// outcomes are retained by a distinct opaque owner and therefore cannot be
/// passed to the Redis acquisition API by construction.
pub enum Stage8bP1ePreRedisRestartV1 {
    Attachable(Stage8bP1eAttachableRestartV1),
    Blocked(Stage8bP1eBlockedRestartV1),
}

/// Opaque, linear owner for one of the 21 restart outcomes that may proceed to
/// verify-only Redis attachment.  It deliberately implements neither Clone
/// nor serde and exposes no raw durable owner.
///
/// ```compile_fail
/// fn require_clone<T: Clone>() {}
/// require_clone::<runtime_durable_service::Stage8bP1eAttachableRestartV1>();
/// ```
pub struct Stage8bP1eAttachableRestartV1 {
    route: Box<Stage8bP1eAttachableRestartRouteV1>,
}

/// Opaque terminal owner for a restart outcome that must stop before Redis.
/// It is not accepted by the S05/S06 acquisition boundary.
///
/// ```compile_fail
/// use runtime_durable_service::{
///     acquire_stage8b_p1e_startup_owner_v1, Stage8bP1eBlockedRestartV1,
///     Stage8bP1eVerifiedRedisSessionV1,
/// };
/// async fn forbidden(
///     blocked: Stage8bP1eBlockedRestartV1,
///     session: Stage8bP1eVerifiedRedisSessionV1,
/// ) {
///     let _ = acquire_stage8b_p1e_startup_owner_v1(blocked, session).await;
/// }
/// ```
pub struct Stage8bP1eBlockedRestartV1 {
    kind: Stage8bP1eRestartKindV1,
    _route: Box<Stage8bP1eBlockedRestartRouteV1>,
}

enum Stage8bP1eBlockedRestartRouteV1 {
    Stage8a4I3Pending(Box<crate::Stage8a4I3RecoveryPendingOwner>),
    Blocked(Box<crate::Stage7bRecoveryBlocked>),
}

enum Stage8bP1eAttachableRestartRouteV1 {
    Ready(Box<crate::Stage7bRecoveryReadyOwner>),
    P1SemanticPrepublicationPending(Box<crate::P1SemanticPrepublicationPending>),
    P1SemanticPrepublicationReady(Box<crate::Stage8bP1SemanticPrepublicationOwner>),
    P1SemanticZeroIntentAckPending(Box<crate::P1SemanticZeroIntentAckPending>),
    P1d2PreAckPending(Box<crate::Stage8bP1d2PreAckPendingOwner>),
    P1d2AckCommitted(Box<crate::Stage8bP1d2AckCommittedOwner>),
    P1d2TruthCommitted(Box<crate::Stage8bP1d2TruthCommittedOwner>),
    P1d4GeneratedMarketPrepublicationPending(
        Box<crate::Stage8bP1d4GeneratedMarketPrepublicationOwner>,
    ),
    P1d4GeneratedMarketDispatchPending(Box<crate::Stage8bP1d4GeneratedMarketDispatchPendingOwner>),
    P1d4GeneratedMarketOrderPending(Box<crate::Stage8bP1d4GeneratedMarketOrderPendingOwner>),
    P1d4GeneratedMarketPreFinalizationPending(
        Box<crate::Stage8bP1d4GeneratedMarketPreFinalizationPendingOwner>,
    ),
    P1d4GeneratedMarketPreAckPending(Box<crate::Stage8bP1d4GeneratedMarketPreAckPendingOwner>),
    P1d4GeneratedMarketAckCommitted(Box<crate::Stage8bP1d4GeneratedMarketAckCommittedOwner>),
    P1d4GeneratedMarketTruthCommitted(Box<crate::Stage8bP1d4GeneratedMarketTruthCommittedOwner>),
    P1d3DispatchPending(Box<crate::Stage8bP1d3DispatchPendingOwner>),
    P1d3PreAckPending(Box<crate::Stage8bP1d3PreAckPendingOwner>),
    P1d3AckCommitted(Box<crate::Stage8bP1d3AckCommittedOwner>),
    P1d3TruthCommitted(Box<crate::Stage8bP1d3TruthCommittedOwner>),
    P1d3CancelContinuationPending(Box<crate::recovery::Stage8bP1d3CancelContinuationOwner>),
    P1d3SemanticPending(Box<crate::Stage8bP1d3SemanticPendingOwner>),
    P1eScheduleBindingCommitted(Box<crate::Stage8bP1eScheduleBindingCommittedOwner>),
}

impl Stage8bP1eAttachableRestartV1 {
    pub fn kind(&self) -> Stage8bP1eRestartKindV1 {
        match self.route.as_ref() {
            Stage8bP1eAttachableRestartRouteV1::Ready(_) => Stage8bP1eRestartKindV1::Ready,
            Stage8bP1eAttachableRestartRouteV1::P1SemanticPrepublicationPending(_) => {
                Stage8bP1eRestartKindV1::P1SemanticPrepublicationPending
            }
            Stage8bP1eAttachableRestartRouteV1::P1SemanticPrepublicationReady(_) => {
                Stage8bP1eRestartKindV1::P1SemanticPrepublicationReady
            }
            Stage8bP1eAttachableRestartRouteV1::P1SemanticZeroIntentAckPending(_) => {
                Stage8bP1eRestartKindV1::P1SemanticZeroIntentAckPending
            }
            Stage8bP1eAttachableRestartRouteV1::P1d2PreAckPending(_) => {
                Stage8bP1eRestartKindV1::P1d2PreAckPending
            }
            Stage8bP1eAttachableRestartRouteV1::P1d2AckCommitted(_) => {
                Stage8bP1eRestartKindV1::P1d2AckCommitted
            }
            Stage8bP1eAttachableRestartRouteV1::P1d2TruthCommitted(_) => {
                Stage8bP1eRestartKindV1::P1d2TruthCommitted
            }
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketPrepublicationPending(_) => {
                Stage8bP1eRestartKindV1::P1d4GeneratedMarketPrepublicationPending
            }
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketDispatchPending(_) => {
                Stage8bP1eRestartKindV1::P1d4GeneratedMarketDispatchPending
            }
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketOrderPending(_) => {
                Stage8bP1eRestartKindV1::P1d4GeneratedMarketOrderPending
            }
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketPreFinalizationPending(_) => {
                Stage8bP1eRestartKindV1::P1d4GeneratedMarketPreFinalizationPending
            }
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketPreAckPending(_) => {
                Stage8bP1eRestartKindV1::P1d4GeneratedMarketPreAckPending
            }
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketAckCommitted(_) => {
                Stage8bP1eRestartKindV1::P1d4GeneratedMarketAckCommitted
            }
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketTruthCommitted(_) => {
                Stage8bP1eRestartKindV1::P1d4GeneratedMarketTruthCommitted
            }
            Stage8bP1eAttachableRestartRouteV1::P1d3DispatchPending(_) => {
                Stage8bP1eRestartKindV1::P1d3DispatchPending
            }
            Stage8bP1eAttachableRestartRouteV1::P1d3PreAckPending(_) => {
                Stage8bP1eRestartKindV1::P1d3PreAckPending
            }
            Stage8bP1eAttachableRestartRouteV1::P1d3AckCommitted(_) => {
                Stage8bP1eRestartKindV1::P1d3AckCommitted
            }
            Stage8bP1eAttachableRestartRouteV1::P1d3TruthCommitted(_) => {
                Stage8bP1eRestartKindV1::P1d3TruthCommitted
            }
            Stage8bP1eAttachableRestartRouteV1::P1d3CancelContinuationPending(_) => {
                Stage8bP1eRestartKindV1::P1d3CancelContinuationPending
            }
            Stage8bP1eAttachableRestartRouteV1::P1d3SemanticPending(_) => {
                Stage8bP1eRestartKindV1::P1d3SemanticPending
            }
            Stage8bP1eAttachableRestartRouteV1::P1eScheduleBindingCommitted(_) => {
                Stage8bP1eRestartKindV1::P1eScheduleBindingCommitted
            }
        }
    }
}

impl Stage8bP1eBlockedRestartV1 {
    pub const fn kind(&self) -> Stage8bP1eRestartKindV1 {
        self.kind
    }

    /// Consumes the terminal owner at the process exit boundary.  No Redis
    /// session can be supplied to this method.
    pub fn finish_before_redis(self) -> Stage8bP1eRestartKindV1 {
        match *self._route {
            Stage8bP1eBlockedRestartRouteV1::Stage8a4I3Pending(owner) => drop(owner),
            Stage8bP1eBlockedRestartRouteV1::Blocked(owner) => drop(owner),
        }
        self.kind
    }
}

/// Exhaustively consumes all 23 durable restart variants before any Redis
/// connection exists.  Adding a new `Stage7bRestartOutcome` variant cannot
/// silently inherit an attach policy because this match has no wildcard.
pub fn stage8b_p1e_route_pre_redis_restart_v1(
    outcome: Stage7bRestartOutcome,
) -> Stage8bP1ePreRedisRestartV1 {
    let route = match outcome {
        Stage7bRestartOutcome::Ready(owner) => Stage8bP1eAttachableRestartRouteV1::Ready(owner),
        Stage7bRestartOutcome::Stage8a4I3Pending(owner) => {
            return Stage8bP1ePreRedisRestartV1::Blocked(Stage8bP1eBlockedRestartV1 {
                kind: Stage8bP1eRestartKindV1::Stage8a4I3Pending,
                _route: Box::new(Stage8bP1eBlockedRestartRouteV1::Stage8a4I3Pending(owner)),
            });
        }
        Stage7bRestartOutcome::P1SemanticPrepublicationPending(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1SemanticPrepublicationPending(owner)
        }
        Stage7bRestartOutcome::P1SemanticPrepublicationReady(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1SemanticPrepublicationReady(owner)
        }
        Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1SemanticZeroIntentAckPending(owner)
        }
        Stage7bRestartOutcome::P1d2PreAckPending(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d2PreAckPending(owner)
        }
        Stage7bRestartOutcome::P1d2AckCommitted(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d2AckCommitted(owner)
        }
        Stage7bRestartOutcome::P1d2TruthCommitted(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d2TruthCommitted(owner)
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketPrepublicationPending(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketPrepublicationPending(owner)
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketDispatchPending(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketDispatchPending(owner)
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketOrderPending(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketOrderPending(owner)
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketPreFinalizationPending(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketPreFinalizationPending(owner)
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketPreAckPending(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketPreAckPending(owner)
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketAckCommitted(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketAckCommitted(owner)
        }
        Stage7bRestartOutcome::P1d4GeneratedMarketTruthCommitted(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketTruthCommitted(owner)
        }
        Stage7bRestartOutcome::P1d3DispatchPending(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d3DispatchPending(owner)
        }
        Stage7bRestartOutcome::P1d3PreAckPending(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d3PreAckPending(owner)
        }
        Stage7bRestartOutcome::P1d3AckCommitted(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d3AckCommitted(owner)
        }
        Stage7bRestartOutcome::P1d3TruthCommitted(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d3TruthCommitted(owner)
        }
        Stage7bRestartOutcome::P1d3CancelContinuationPending(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d3CancelContinuationPending(owner)
        }
        Stage7bRestartOutcome::P1d3SemanticPending(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1d3SemanticPending(owner)
        }
        Stage7bRestartOutcome::P1eScheduleBindingCommitted(owner) => {
            Stage8bP1eAttachableRestartRouteV1::P1eScheduleBindingCommitted(owner)
        }
        Stage7bRestartOutcome::Blocked(owner) => {
            return Stage8bP1ePreRedisRestartV1::Blocked(Stage8bP1eBlockedRestartV1 {
                kind: Stage8bP1eRestartKindV1::Blocked,
                _route: Box::new(Stage8bP1eBlockedRestartRouteV1::Blocked(owner)),
            });
        }
    };
    Stage8bP1ePreRedisRestartV1::Attachable(Stage8bP1eAttachableRestartV1 {
        route: Box::new(route),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eStartupOwnerKindV1 {
    ReadySourceAcquired,
    ReadyNoPending,
    ReadyPendingNotClaimable,
    RecoveredSourceAcquired,
    P1d3LimitDispatchAwaitingSchedule,
    ScheduleBindingCommitted,
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1eStartupErrorV1 {
    #[error("S06 Redis observation failed")]
    RedisControl(#[from] Stage8bP1eRedisControlError),
    #[error("S06/S06R source acquisition failed")]
    Source(#[from] Stage8bP1RedisSemanticError),
    #[error("signed schedule read or binding failed")]
    Schedule(#[from] crate::Stage8bP1eScheduleReadError),
    #[error("quiescent recovery boundary did not contain its exact Ready owner")]
    ReadyBoundaryInvariant,
    #[error("schedule-free recovery exceeded its fixed row budget")]
    RecoveryStepBudgetExceeded,
    #[error("committed Day-expiry requires an empty canonical M10 PEL")]
    CommittedDayExpiryPelNotEmpty,
}

/// Linear S05/S06 result.  The verified diagnostic control connection is held
/// beside exactly one route owner, so no successful attach can return after
/// dropping the durable/source authority.
///
/// ```compile_fail
/// fn require_clone<T: Clone>() {}
/// require_clone::<runtime_durable_service::Stage8bP1eStartupOwnerV1>();
/// ```
pub struct Stage8bP1eStartupOwnerV1 {
    kind: Stage8bP1eStartupOwnerKindV1,
    route: Box<Stage8bP1eStartupOwnerRouteV1>,
    control: Stage8bP1eRedisControlV1,
}

/// Exhaustive S06-to-latch result. Every variant is linear and retains the
/// verified diagnostic Redis control plane beside the sole lifecycle owner or
/// route-bound continuation. No variant can be cloned or serialized.
///
/// ```compile_fail
/// fn require_clone<T: Clone>() {}
/// require_clone::<runtime_durable_service::Stage8bP1eStartupLatchDecisionV1>();
/// ```
pub enum Stage8bP1eStartupLatchDecisionV1 {
    RetainedSource(Stage8bP1eRetainedStartupV1),
    ContinueSource(Stage8bP1eContinuingStartupV1),
    ReadyIdle(Stage8bP1eReadyIdleStartupV1),
    ReadyPendingNotClaimable(Stage8bP1ePendingNotClaimableStartupV1),
    P1d3LimitDispatchAwaitingSchedule(Stage8bP1eLimitScheduleStartupV1),
    ScheduleBindingCommitted(Stage8bP1eCommittedScheduleStartupV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eStartupLatchKindV1 {
    RetainedSource,
    ContinueSource,
    ReadyIdle,
    ReadyPendingNotClaimable,
    P1d3LimitDispatchAwaitingSchedule,
    ScheduleBindingCommitted,
}

impl Stage8bP1eStartupLatchDecisionV1 {
    pub const fn kind(&self) -> Stage8bP1eStartupLatchKindV1 {
        match self {
            Self::RetainedSource(_) => Stage8bP1eStartupLatchKindV1::RetainedSource,
            Self::ContinueSource(_) => Stage8bP1eStartupLatchKindV1::ContinueSource,
            Self::ReadyIdle(_) => Stage8bP1eStartupLatchKindV1::ReadyIdle,
            Self::ReadyPendingNotClaimable(_) => {
                Stage8bP1eStartupLatchKindV1::ReadyPendingNotClaimable
            }
            Self::P1d3LimitDispatchAwaitingSchedule(_) => {
                Stage8bP1eStartupLatchKindV1::P1d3LimitDispatchAwaitingSchedule
            }
            Self::ScheduleBindingCommitted(_) => {
                Stage8bP1eStartupLatchKindV1::ScheduleBindingCommitted
            }
        }
    }
}

pub struct Stage8bP1eRetainedStartupV1 {
    receipt: Stage8bP1eRetainedSourceReceiptV1,
    _control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eRetainedStartupV1 {
    pub fn receipt(&self) -> &Stage8bP1eRetainedSourceReceiptV1 {
        &self.receipt
    }
}

pub struct Stage8bP1eContinuingStartupV1 {
    route: Stage8bP1eRoutedContinuationV1,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eContinuingStartupV1 {
    pub fn route(&self) -> &Stage8bP1eRoutedContinuationV1 {
        &self.route
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }
}

pub struct Stage8bP1eReadyIdleStartupV1 {
    _owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eReadyIdleStartupV1 {
    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }

    /// Enters the steady-state S08 boundary without cloning or reconstructing
    /// the Ready owner retained by startup S06.
    pub fn into_ready_polling(self) -> Stage8bP1eReadyPollingV1 {
        let Self { _owner, control } = self;
        Stage8bP1eReadyPollingV1 {
            owner: _owner,
            control,
            committed_cancel_disposition: None,
        }
    }
}

pub struct Stage8bP1ePendingNotClaimableStartupV1 {
    _owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
    pending_m10_redis_id: String,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1ePendingNotClaimableStartupV1 {
    pub fn pending_m10_redis_id(&self) -> &str {
        &self.pending_m10_redis_id
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }
}

pub struct Stage8bP1eLimitScheduleStartupV1 {
    _durable: Box<crate::Stage8bP1d3DispatchPendingOwner>,
    _transport: Stage8bP1RedisSemanticCompositionTransport,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eLimitScheduleStartupV1 {
    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }
}

pub struct Stage8bP1eCommittedScheduleStartupV1 {
    durable: Box<crate::Stage8bP1eScheduleBindingCommittedOwner>,
    transport: Stage8bP1RedisSemanticCompositionTransport,
    control: Stage8bP1eRedisControlV1,
}

/// One exact S06R continuation result.  This boundary deliberately performs
/// at most one accepted resume transition; later phase settlement remains
/// typed and cannot be skipped.  Schedule-dependent permits remain linear and
/// unconsumed until signed schedule composition supplies their exact authority.
pub enum Stage8bP1eRecoveryStepRouteV1 {
    Semantic(Box<Stage8bP1RedisSemanticOutcome>),
    ZeroIntentResolved(Box<Stage8bP1RedisZeroIntentAckResolved>),
    Prepublication(Box<Stage8bP1RedisPrepublicationPending>),
    CommandPublished(Box<Stage8bP1RedisCommandPublished>),
    GeneratedMarketAckCommitted(Box<Stage8bP1RedisGeneratedMarketAckCommitted>),
    GeneratedMarketTruthCommitted(Box<Stage8bP1RedisGeneratedMarketTruthCommitted>),
    FeedbackAckCommitted(Box<Stage8bP1RedisFeedbackAckCommitted>),
    FeedbackTruthCommitted(Box<Stage8bP1RedisFeedbackTruthCommitted>),
    FeedbackResolved(Box<Stage8bP1RedisFeedbackResolved>),
    LimitPreAckRecovered(Box<Stage8bP1RedisPreAckRecoveryOutcome>),
    CancelContinuationPending(Box<Stage8bP1RedisCancelContinuationPending>),
    LimitAckCommitted(Box<Stage8bP1RedisLimitAckCommitted>),
    LimitTruthCommitted(Box<Stage8bP1RedisLimitTruthCommitted>),
    LimitResolved(Box<Stage8bP1RedisLimitResolved>),
    ScheduleDeferred(Box<Stage8bP1eRoutedContinuationV1>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eRecoveryBoundaryKindV1 {
    Semantic,
    ZeroIntentResolved,
    Prepublication,
    CommandPublished,
    GeneratedMarketAckCommitted,
    GeneratedMarketTruthCommitted,
    FeedbackAckCommitted,
    FeedbackTruthCommitted,
    FeedbackResolved,
    LimitPreAckRecovered,
    CancelContinuationPending,
    LimitAckCommitted,
    LimitTruthCommitted,
    LimitResolved,
    ScheduleDeferred,
}

impl Stage8bP1eRecoveryStepRouteV1 {
    const fn kind(&self) -> Stage8bP1eRecoveryBoundaryKindV1 {
        match self {
            Self::Semantic(_) => Stage8bP1eRecoveryBoundaryKindV1::Semantic,
            Self::ZeroIntentResolved(_) => Stage8bP1eRecoveryBoundaryKindV1::ZeroIntentResolved,
            Self::Prepublication(_) => Stage8bP1eRecoveryBoundaryKindV1::Prepublication,
            Self::CommandPublished(_) => Stage8bP1eRecoveryBoundaryKindV1::CommandPublished,
            Self::GeneratedMarketAckCommitted(_) => {
                Stage8bP1eRecoveryBoundaryKindV1::GeneratedMarketAckCommitted
            }
            Self::GeneratedMarketTruthCommitted(_) => {
                Stage8bP1eRecoveryBoundaryKindV1::GeneratedMarketTruthCommitted
            }
            Self::FeedbackAckCommitted(_) => Stage8bP1eRecoveryBoundaryKindV1::FeedbackAckCommitted,
            Self::FeedbackTruthCommitted(_) => {
                Stage8bP1eRecoveryBoundaryKindV1::FeedbackTruthCommitted
            }
            Self::FeedbackResolved(_) => Stage8bP1eRecoveryBoundaryKindV1::FeedbackResolved,
            Self::LimitPreAckRecovered(_) => Stage8bP1eRecoveryBoundaryKindV1::LimitPreAckRecovered,
            Self::CancelContinuationPending(_) => {
                Stage8bP1eRecoveryBoundaryKindV1::CancelContinuationPending
            }
            Self::LimitAckCommitted(_) => Stage8bP1eRecoveryBoundaryKindV1::LimitAckCommitted,
            Self::LimitTruthCommitted(_) => Stage8bP1eRecoveryBoundaryKindV1::LimitTruthCommitted,
            Self::LimitResolved(_) => Stage8bP1eRecoveryBoundaryKindV1::LimitResolved,
            Self::ScheduleDeferred(_) => Stage8bP1eRecoveryBoundaryKindV1::ScheduleDeferred,
        }
    }
}

pub struct Stage8bP1eRecoveryStepV1 {
    route: Box<Stage8bP1eRecoveryStepRouteV1>,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eRecoveryStepV1 {
    pub fn kind(&self) -> Stage8bP1eRecoveryBoundaryKindV1 {
        self.route.kind()
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }

    pub fn requires_schedule(&self) -> bool {
        matches!(
            self.route.as_ref(),
            Stage8bP1eRecoveryStepRouteV1::ScheduleDeferred(_)
        )
    }
}

/// Diagnostic-only shutdown result at an authenticated S06R row boundary.
/// The lifecycle owner is deliberately dropped so restart must reconstruct it
/// from the durable package; no continuation authority is retained here.
pub struct Stage8bP1eRetainedRecoveryBoundaryV1 {
    kind: Stage8bP1eRecoveryBoundaryKindV1,
    shutdown_intent: Stage8bP1eShutdownIntentV1,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eRetainedRecoveryBoundaryV1 {
    pub const fn kind(&self) -> Stage8bP1eRecoveryBoundaryKindV1 {
        self.kind
    }

    pub fn shutdown_intent(&self) -> &Stage8bP1eShutdownIntentV1 {
        &self.shutdown_intent
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }
}

/// Single-use authority to advance exactly one already-authenticated recovery
/// boundary. It can be created only by a clear post-boundary latch check.
///
/// ```compile_fail
/// fn require_clone<T: Clone>() {}
/// require_clone::<runtime_durable_service::Stage8bP1eRecoveryAdvancePermitV1>();
/// ```
pub struct Stage8bP1eRecoveryAdvancePermitV1 {
    step: Stage8bP1eRecoveryStepV1,
}

pub enum Stage8bP1eRecoveryLatchDecisionV1 {
    RetainForRestart(Stage8bP1eRetainedRecoveryBoundaryV1),
    Continue(Stage8bP1eRecoveryAdvancePermitV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eRecoveredReadyKindV1 {
    Semantic,
    ZeroIntent,
    Feedback,
    Limit,
}

enum Stage8bP1eRecoveredReadyRouteV1 {
    Semantic(Stage8bP1RedisSemanticOutcome),
    ZeroIntent(Stage8bP1RedisZeroIntentAckResolved),
    Feedback(Stage8bP1RedisFeedbackResolved),
    Limit(Stage8bP1RedisLimitResolved),
}

fn recovered_ready_owner(
    route: Stage8bP1eRecoveredReadyRouteV1,
) -> Result<Box<Stage8bP1RedisSemanticCompositionOwner>, Stage8bP1eStartupErrorV1> {
    match route {
        Stage8bP1eRecoveredReadyRouteV1::Semantic(Stage8bP1RedisSemanticOutcome::Ready {
            owner,
            ..
        }) => Ok(owner),
        Stage8bP1eRecoveredReadyRouteV1::Semantic(_) => {
            Err(Stage8bP1eStartupErrorV1::ReadyBoundaryInvariant)
        }
        Stage8bP1eRecoveredReadyRouteV1::ZeroIntent(resolved) => Ok(resolved.into_ready_owner()),
        Stage8bP1eRecoveredReadyRouteV1::Feedback(resolved) => Ok(resolved.into_ready_owner()),
        Stage8bP1eRecoveredReadyRouteV1::Limit(resolved) => Ok(resolved.into_ready_owner()),
    }
}

/// Quiescent owner reached only after the selected lifecycle has completed
/// its exact terminal boundary, including source XACK-last where applicable.
pub struct Stage8bP1eRecoveredReadyV1 {
    kind: Stage8bP1eRecoveredReadyKindV1,
    _route: Box<Stage8bP1eRecoveredReadyRouteV1>,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eRecoveredReadyV1 {
    pub const fn kind(&self) -> Stage8bP1eRecoveredReadyKindV1 {
        self.kind
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }

    /// Enters S08 only from an exact terminal recovery result. The semantic
    /// route is checked explicitly so an internally inconsistent non-Ready
    /// outcome cannot be converted into fresh-poll authority.
    pub fn into_ready_polling(self) -> Result<Stage8bP1eReadyPollingV1, Stage8bP1eStartupErrorV1> {
        let Self {
            kind: _,
            _route,
            control,
        } = self;
        let committed_cancel_disposition = match _route.as_ref() {
            Stage8bP1eRecoveredReadyRouteV1::Limit(resolved) if resolved.cancel_source() => {
                Some(resolved.disposition())
            }
            _ => None,
        };
        let owner = recovered_ready_owner(*_route)?;
        Ok(Stage8bP1eReadyPollingV1 {
            owner,
            control,
            committed_cancel_disposition,
        })
    }
}

/// Sole quiescent owner admitted to one bounded S08 fresh read. Empty reads
/// return this same authority; acquired reads must cross the mandatory
/// post-acquisition latch before any parsing or callback is possible.
///
/// ```compile_fail
/// fn require_clone<T: Clone>() {}
/// require_clone::<runtime_durable_service::Stage8bP1eReadyPollingV1>();
/// ```
pub struct Stage8bP1eReadyPollingV1 {
    owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
    control: Stage8bP1eRedisControlV1,
    committed_cancel_disposition: Option<Stage8bP1RedisZeroIntentAckDisposition>,
}

impl Stage8bP1eReadyPollingV1 {
    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }

    fn is_committed_cancel_resolution(&self) -> bool {
        self.committed_cancel_disposition.is_some()
    }

    fn into_committed_cancel_resolved(
        self,
    ) -> Result<Stage8bP1eCommittedCancelResolvedV1, Stage8bP1eStartupErrorV1> {
        let Self {
            owner,
            control,
            committed_cancel_disposition,
        } = self;
        let disposition =
            committed_cancel_disposition.ok_or(Stage8bP1eStartupErrorV1::ReadyBoundaryInvariant)?;
        Ok(Stage8bP1eCommittedCancelResolvedV1 {
            disposition,
            ready: Stage8bP1eReadyPollingV1 {
                owner,
                control,
                committed_cancel_disposition: None,
            },
        })
    }

    /// Reclassifies an externally due day timer into the signed-schedule
    /// acquisition path after binding the caller's progression context to the
    /// authenticated schedule high-water in the retained Ready owner. This
    /// conversion grants no expiry authority: only a fresh verified Closed
    /// schedule that advances that exact durable history can proceed.
    pub fn into_day_expiry_schedule(
        self,
        context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    ) -> Result<Stage8bP1eScheduleDeferredRecoveryV1, Stage8bP1eStartupErrorV1> {
        self.restore_durable_schedule_high_water(context)?;
        self.into_day_expiry_schedule_with_bound_high_water(context)
    }

    fn into_day_expiry_schedule_with_bound_high_water(
        self,
        context: &strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    ) -> Result<Stage8bP1eScheduleDeferredRecoveryV1, Stage8bP1eStartupErrorV1> {
        let durable_high_water = context
            .high_water
            .clone()
            .ok_or(Stage8bP1RedisSemanticError::P1eScheduleHighWaterConflict)?;
        Ok(Stage8bP1eScheduleDeferredRecoveryV1 {
            kind: Stage8bP1eScheduleDeferredKindV1::ReadyDayExpiry,
            _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::ReadyDayExpiry {
                owner: self.owner,
                durable_high_water,
            }),
            control: self.control,
        })
    }

    /// Restores schedule progression only through the sealed Ready owner. An
    /// already populated in-memory context must match the durable value
    /// exactly; neither side may silently replace the other.
    pub fn restore_durable_schedule_high_water(
        &self,
        context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    ) -> Result<(), Stage8bP1eStartupErrorV1> {
        if self.owner.operational_identity_sha256() != context.expected_operational_identity_sha256
        {
            return Err(Stage8bP1RedisSemanticError::P1eScheduleHighWaterConflict.into());
        }
        let durable = self.owner.recover_stage8b_p1e_latest_schedule_high_water(
            &context.expected_runtime_config_fingerprint_sha256,
            &context.expected_instrument_map_fingerprint_sha256,
        )?;
        apply_recovered_schedule_high_water(context, durable)
    }

    #[cfg(any(test, feature = "stage8a4-i3-test-fixtures"))]
    #[allow(dead_code, reason = "fixture trust is exercised only by restart tests")]
    fn test_restore_durable_schedule_high_water_with_key(
        &self,
        context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
        public_key_hex: &str,
        key_valid_from: DateTime<Utc>,
        key_valid_until: DateTime<Utc>,
    ) -> Result<(), Stage8bP1eStartupErrorV1> {
        if self.owner.operational_identity_sha256() != context.expected_operational_identity_sha256
        {
            return Err(Stage8bP1RedisSemanticError::P1eScheduleHighWaterConflict.into());
        }
        let durable = self
            .owner
            .stage8b_p1e_test_recover_latest_schedule_high_water_with_key(
                &context.expected_runtime_config_fingerprint_sha256,
                &context.expected_instrument_map_fingerprint_sha256,
                public_key_hex,
                key_valid_from,
                key_valid_until,
            )?;
        apply_recovered_schedule_high_water(context, durable)
    }

    #[cfg(any(test, feature = "stage8a4-i3-test-fixtures"))]
    #[allow(
        dead_code,
        reason = "fixture trust is exercised only by feature-gated process tests"
    )]
    fn test_into_day_expiry_schedule_with_key(
        self,
        context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
        public_key_hex: &str,
        key_valid_from: DateTime<Utc>,
        key_valid_until: DateTime<Utc>,
    ) -> Result<Stage8bP1eScheduleDeferredRecoveryV1, Stage8bP1eStartupErrorV1> {
        self.test_restore_durable_schedule_high_water_with_key(
            context,
            public_key_hex,
            key_valid_from,
            key_valid_until,
        )?;
        self.into_day_expiry_schedule_with_bound_high_water(context)
    }
}

fn apply_recovered_schedule_high_water(
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    durable: Option<strategy_runtime_core::Stage8bP1eScheduleHighWaterV1>,
) -> Result<(), Stage8bP1eStartupErrorV1> {
    match (&context.high_water, durable) {
        (None, None) => Ok(()),
        (None, Some(durable)) => {
            context.high_water = Some(durable);
            Ok(())
        }
        (Some(current), Some(durable)) if current == &durable => Ok(()),
        (Some(_), None) | (Some(_), Some(_)) => {
            Err(Stage8bP1RedisSemanticError::P1eScheduleHighWaterConflict.into())
        }
    }
}

/// Result of exactly one bounded fresh read. `Empty` remains quiescent;
/// `RetainedSource` destroys continuation authority after a set latch; and
/// `ContinueSource` carries exactly one route-bound permit.
pub enum Stage8bP1eReadyPollOutcomeV1 {
    Empty(Stage8bP1eReadyPollingV1),
    Stopped(Stage8bP1eStoppedReadyPollingV1),
    RetainedSource(Stage8bP1eRetainedStartupV1),
    ContinueSource(Stage8bP1eContinuingStartupV1),
}

/// Diagnostic-only result when shutdown wins before acquisition or while the
/// bounded fresh read is waiting. The Ready owner has been destroyed, so the
/// process must reconstruct it from durable state after restart.
pub struct Stage8bP1eStoppedReadyPollingV1 {
    shutdown_intent: Stage8bP1eShutdownIntentV1,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eStoppedReadyPollingV1 {
    pub fn shutdown_intent(&self) -> &Stage8bP1eShutdownIntentV1 {
        &self.shutdown_intent
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }
}

/// Performs one S08 fresh poll and immediately applies the post-acquisition
/// latch. No acquired source can escape this function without either losing
/// all effect authority or becoming one exact routed continuation.
pub async fn poll_stage8b_p1e_ready_once_v1(
    ready: Stage8bP1eReadyPollingV1,
    latch: &Stage8bP1eShutdownLatchV1,
) -> Result<Stage8bP1eReadyPollOutcomeV1, Stage8bP1eStartupErrorV1> {
    let Stage8bP1eReadyPollingV1 {
        owner,
        control,
        committed_cancel_disposition,
    } = ready;
    if committed_cancel_disposition.is_some() {
        return Err(Stage8bP1eStartupErrorV1::ReadyBoundaryInvariant);
    }
    if let Some(shutdown_intent) = latch.intent().cloned() {
        drop(owner);
        return Ok(Stage8bP1eReadyPollOutcomeV1::Stopped(
            Stage8bP1eStoppedReadyPollingV1 {
                shutdown_intent,
                control,
            },
        ));
    }
    match poll_stage8b_p1e_ready_fresh_with_redis(*owner).await? {
        Stage8bP1eReadyFreshAcquisitionOutcomeV1::EmptyFreshPoll(owner) => {
            if let Some(shutdown_intent) = latch.intent().cloned() {
                drop(owner);
                Ok(Stage8bP1eReadyPollOutcomeV1::Stopped(
                    Stage8bP1eStoppedReadyPollingV1 {
                        shutdown_intent,
                        control,
                    },
                ))
            } else {
                Ok(Stage8bP1eReadyPollOutcomeV1::Empty(
                    Stage8bP1eReadyPollingV1 {
                        owner,
                        control,
                        committed_cancel_disposition: None,
                    },
                ))
            }
        }
        Stage8bP1eReadyFreshAcquisitionOutcomeV1::Acquired(acquired) => Ok(
            match route_stage8b_p1e_post_acquisition_v1(acquired, latch) {
                Stage8bP1eRoutedPostAcquisitionDecisionV1::RetainForRestart(receipt) => {
                    Stage8bP1eReadyPollOutcomeV1::RetainedSource(Stage8bP1eRetainedStartupV1 {
                        receipt,
                        _control: control,
                    })
                }
                Stage8bP1eRoutedPostAcquisitionDecisionV1::Continue(route) => {
                    Stage8bP1eReadyPollOutcomeV1::ContinueSource(Stage8bP1eContinuingStartupV1 {
                        route,
                        control,
                    })
                }
            },
        ),
    }
}

pub struct Stage8bP1eRecoveredPendingNotClaimableV1 {
    _owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
    pending_m10_redis_id: String,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eRecoveredPendingNotClaimableV1 {
    pub fn pending_m10_redis_id(&self) -> &str {
        &self.pending_m10_redis_id
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }
}

pub struct Stage8bP1eRecoveredBlockedV1 {
    semantic_batch_id_sha256: String,
    intent_count: usize,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eRecoveredBlockedV1 {
    pub fn semantic_batch_id_sha256(&self) -> &str {
        &self.semantic_batch_id_sha256
    }

    pub const fn intent_count(&self) -> usize {
        self.intent_count
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1eScheduleDeferredKindV1 {
    CommandPublishedMarket,
    CommandPublishedGeneratedMarket,
    CommandPublishedInitialLimit,
    CommandPublishedCancel,
    CommandPublishedUnsupported,
    RoutedContinuation,
    ReadyDayExpiry,
}

#[allow(
    dead_code,
    reason = "opaque schedule-dependent owners are retained for I1A composition"
)]
enum Stage8bP1eScheduleDeferredRouteV1 {
    CommandPublished(Box<Stage8bP1RedisCommandPublished>),
    RoutedContinuation(Box<Stage8bP1eRoutedContinuationV1>),
    ReadyDayExpiry {
        owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
        durable_high_water: strategy_runtime_core::Stage8bP1eScheduleHighWaterV1,
    },
}

pub struct Stage8bP1eScheduleDeferredRecoveryV1 {
    kind: Stage8bP1eScheduleDeferredKindV1,
    _route: Box<Stage8bP1eScheduleDeferredRouteV1>,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eScheduleDeferredRecoveryV1 {
    pub const fn kind(&self) -> Stage8bP1eScheduleDeferredKindV1 {
        self.kind
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }
}

pub struct Stage8bP1eScheduleStoppedRecoveryV1 {
    receipt: crate::Stage8bP1eScheduleStopReceiptV1,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eScheduleStoppedRecoveryV1 {
    pub fn receipt(&self) -> &crate::Stage8bP1eScheduleStopReceiptV1 {
        &self.receipt
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }
}

pub enum Stage8bP1eWorkingScheduleAdvanceOutcomeV1 {
    Stopped(Stage8bP1eScheduleStoppedRecoveryV1),
    AwaitingSchedule(Stage8bP1eScheduleDeferredRecoveryV1),
    Lifecycle(Stage8bP1eRecoveryAdvanceOutcomeV1),
}

pub enum Stage8bP1eMarketScheduleAdvanceOutcomeV1 {
    Stopped(Stage8bP1eScheduleStoppedRecoveryV1),
    AwaitingSchedule(Stage8bP1eScheduleDeferredRecoveryV1),
    Lifecycle(Stage8bP1eRecoveryAdvanceOutcomeV1),
}

pub enum Stage8bP1eGeneratedMarketScheduleAdvanceOutcomeV1 {
    Stopped(Stage8bP1eScheduleStoppedRecoveryV1),
    AwaitingSchedule(Stage8bP1eScheduleDeferredRecoveryV1),
    Lifecycle(Stage8bP1eRecoveryAdvanceOutcomeV1),
}

pub enum Stage8bP1eInitialLimitScheduleAdvanceOutcomeV1 {
    Stopped(Stage8bP1eScheduleStoppedRecoveryV1),
    AwaitingSchedule(Stage8bP1eScheduleDeferredRecoveryV1),
    Lifecycle(Stage8bP1eRecoveryAdvanceOutcomeV1),
}

pub enum Stage8bP1eCancelScheduleAdvanceOutcomeV1 {
    Stopped(Stage8bP1eScheduleStoppedRecoveryV1),
    AwaitingSchedule(Stage8bP1eScheduleDeferredRecoveryV1),
    Lifecycle(Stage8bP1eRecoveryAdvanceOutcomeV1),
}

pub enum Stage8bP1eDayExpiryScheduleAdvanceOutcomeV1 {
    Stopped(Stage8bP1eScheduleStoppedRecoveryV1),
    AwaitingSchedule(Stage8bP1eScheduleDeferredRecoveryV1),
    Ready(Stage8bP1eReadyPollingV1),
}

/// One bounded signed-schedule attempt for the route shapes already composed
/// by I1. Unsupported routes retain their exact opaque owner unchanged; they
/// are never coerced into the Market or Working-LIMIT authority paths.
pub enum Stage8bP1eSupportedScheduleCycleOutcomeV1 {
    Stopped(Stage8bP1eScheduleStoppedRecoveryV1),
    AwaitingSchedule(Stage8bP1eScheduleDeferredRecoveryV1),
    Ready(Stage8bP1eReadyPollingV1),
    RetainedRecovery(Stage8bP1eRetainedRecoveryBoundaryV1),
    PendingNotClaimable(Stage8bP1eRecoveredPendingNotClaimableV1),
    Blocked(Stage8bP1eRecoveredBlockedV1),
    ScheduleDeferred(Stage8bP1eScheduleDeferredRecoveryV1),
    Unsupported(Stage8bP1eScheduleDeferredRecoveryV1),
}

/// Terminal result of the compile-time-pinned signed-schedule acquisition
/// policy. `Exhausted` retains the exact linear route owner and its M10 PEL;
/// it grants no authority and cannot fall through to a fresh source read.
pub enum Stage8bP1eBoundedScheduleCycleOutcomeV1 {
    Stopped(Stage8bP1eScheduleStoppedRecoveryV1),
    Exhausted(Stage8bP1eScheduleDeferredRecoveryV1),
    Ready(Stage8bP1eReadyPollingV1),
    RetainedRecovery(Stage8bP1eRetainedRecoveryBoundaryV1),
    PendingNotClaimable(Stage8bP1eRecoveredPendingNotClaimableV1),
    Blocked(Stage8bP1eRecoveredBlockedV1),
    ScheduleDeferred(Stage8bP1eScheduleDeferredRecoveryV1),
    Unsupported(Stage8bP1eScheduleDeferredRecoveryV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stage8bP1eScheduleAcquisitionPolicyV1 {
    attempts: u8,
    total_deadline: StdDuration,
    redis_operation_timeout: StdDuration,
    initial_backoff: StdDuration,
    maximum_backoff: StdDuration,
}

impl Stage8bP1eScheduleAcquisitionPolicyV1 {
    const fn production() -> Self {
        Self {
            attempts: STAGE8B_P1E_SCHEDULE_ACQUISITION_ATTEMPTS,
            total_deadline: StdDuration::from_millis(STAGE8B_P1E_SCHEDULE_ACQUISITION_DEADLINE_MS),
            redis_operation_timeout: StdDuration::from_millis(
                crate::STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS,
            ),
            initial_backoff: StdDuration::from_millis(
                STAGE8B_P1E_SCHEDULE_ACQUISITION_INITIAL_BACKOFF_MS,
            ),
            maximum_backoff: StdDuration::from_millis(
                STAGE8B_P1E_SCHEDULE_ACQUISITION_MAX_BACKOFF_MS,
            ),
        }
    }

    fn is_valid(self) -> bool {
        self.attempts > 0
            && !self.total_deadline.is_zero()
            && !self.redis_operation_timeout.is_zero()
            && !self.initial_backoff.is_zero()
            && self.initial_backoff <= self.maximum_backoff
    }

    fn backoff_after(self, completed_attempts: u8) -> StdDuration {
        let shift = u32::from(completed_attempts.saturating_sub(1)).min(63);
        let multiplier = 1_u64.checked_shl(shift).unwrap_or(u64::MAX);
        let millis = u64::try_from(self.initial_backoff.as_millis())
            .unwrap_or(u64::MAX)
            .saturating_mul(multiplier)
            .min(u64::try_from(self.maximum_backoff.as_millis()).unwrap_or(u64::MAX));
        StdDuration::from_millis(millis)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage8bP1eSupportedScheduleRouteV1 {
    PlainMarket,
    GeneratedMarket,
    InitialLimit,
    Cancel,
    ReadyWorkingLimit,
    ReadyDayExpiry,
}

fn supported_schedule_route(
    deferred: &Stage8bP1eScheduleDeferredRecoveryV1,
) -> Option<Stage8bP1eSupportedScheduleRouteV1> {
    match (deferred.kind, deferred._route.as_ref()) {
        (
            Stage8bP1eScheduleDeferredKindV1::CommandPublishedMarket,
            Stage8bP1eScheduleDeferredRouteV1::CommandPublished(_),
        ) => Some(Stage8bP1eSupportedScheduleRouteV1::PlainMarket),
        (
            Stage8bP1eScheduleDeferredKindV1::CommandPublishedInitialLimit,
            Stage8bP1eScheduleDeferredRouteV1::CommandPublished(_),
        ) => Some(Stage8bP1eSupportedScheduleRouteV1::InitialLimit),
        (
            Stage8bP1eScheduleDeferredKindV1::CommandPublishedGeneratedMarket,
            Stage8bP1eScheduleDeferredRouteV1::CommandPublished(_),
        ) => Some(Stage8bP1eSupportedScheduleRouteV1::GeneratedMarket),
        (
            Stage8bP1eScheduleDeferredKindV1::CommandPublishedCancel,
            Stage8bP1eScheduleDeferredRouteV1::CommandPublished(_),
        ) => Some(Stage8bP1eSupportedScheduleRouteV1::Cancel),
        (
            Stage8bP1eScheduleDeferredKindV1::RoutedContinuation,
            Stage8bP1eScheduleDeferredRouteV1::RoutedContinuation(route),
        ) if matches!(
            route.as_ref(),
            Stage8bP1eRoutedContinuationV1::ReadyWorkingLimit(_)
        ) =>
        {
            Some(Stage8bP1eSupportedScheduleRouteV1::ReadyWorkingLimit)
        }
        (
            Stage8bP1eScheduleDeferredKindV1::ReadyDayExpiry,
            Stage8bP1eScheduleDeferredRouteV1::ReadyDayExpiry { .. },
        ) => Some(Stage8bP1eSupportedScheduleRouteV1::ReadyDayExpiry),
        _ => None,
    }
}

/// Completes schedule checkpoints C-F for one exact published CANCEL and
/// rejoins the inherited P1-d3 ACK/truth/cancel-race lifecycle.
pub async fn advance_stage8b_p1e_cancel_schedule_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eCancelScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    advance_stage8b_p1e_cancel_schedule_with_timeout_v1(
        deferred,
        reader,
        context,
        latch,
        bound_at_utc,
        commitment_key,
        StdDuration::from_millis(crate::STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn advance_stage8b_p1e_cancel_schedule_with_timeout_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    operation_timeout: StdDuration,
) -> Result<Stage8bP1eCancelScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    if context.trusted_now != bound_at_utc {
        return Err(Stage8bP1RedisSemanticError::P1eScheduleClockMismatch.into());
    }
    if deferred.kind != Stage8bP1eScheduleDeferredKindV1::CommandPublishedCancel
        || !matches!(
            deferred._route.as_ref(),
            Stage8bP1eScheduleDeferredRouteV1::CommandPublished(_)
        )
    {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    }
    let read = match reader
        .read_newest_guarded_with_timeout(context, latch, operation_timeout)
        .await
    {
        Ok(read) => read,
        Err(error) if retryable_schedule_read_error(&error) => {
            return Ok(Stage8bP1eCancelScheduleAdvanceOutcomeV1::AwaitingSchedule(
                deferred,
            ));
        }
        Err(error) => return Err(error.into()),
    };
    let Stage8bP1eScheduleDeferredRecoveryV1 {
        kind,
        _route: route,
        control,
    } = deferred;
    let Stage8bP1eScheduleDeferredRouteV1::CommandPublished(published) = *route else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    };
    match read {
        crate::Stage8bP1eGuardedScheduleReadV1::Stopped(receipt) => {
            drop(published);
            Ok(Stage8bP1eCancelScheduleAdvanceOutcomeV1::Stopped(
                Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
            ))
        }
        crate::Stage8bP1eGuardedScheduleReadV1::Read(
            crate::Stage8bP1eNewestScheduleReadV1::Empty,
        ) => Ok(Stage8bP1eCancelScheduleAdvanceOutcomeV1::AwaitingSchedule(
            Stage8bP1eScheduleDeferredRecoveryV1 {
                kind,
                _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                    published,
                )),
                control,
            },
        )),
        crate::Stage8bP1eGuardedScheduleReadV1::Read(
            crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
        ) => {
            let committed_high_water = snapshot.high_water().clone();
            match resume_stage8b_p1e_cancel_with_signed_schedule_timeout(
                *published,
                *snapshot,
                latch,
                bound_at_utc,
                commitment_key,
                operation_timeout,
            )
            .await?
            {
                Stage8bP1eSignedCancelScheduleOutcomeV1::Stopped(receipt) => {
                    Ok(Stage8bP1eCancelScheduleAdvanceOutcomeV1::Stopped(
                        Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
                    ))
                }
                Stage8bP1eSignedCancelScheduleOutcomeV1::AwaitingSuccessor(published) => {
                    Ok(Stage8bP1eCancelScheduleAdvanceOutcomeV1::AwaitingSchedule(
                        Stage8bP1eScheduleDeferredRecoveryV1 {
                            kind,
                            _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                                published,
                            )),
                            control,
                        },
                    ))
                }
                Stage8bP1eSignedCancelScheduleOutcomeV1::CancelCommitted(outcome) => {
                    context.high_water = Some(committed_high_water);
                    let route = match *outcome {
                        Stage8bP1RedisCancelCommitOutcome::AckCommitted(ack) => {
                            Stage8bP1eRecoveryStepRouteV1::LimitAckCommitted(Box::new(ack))
                        }
                        Stage8bP1RedisCancelCommitOutcome::TruthCommitted(truth) => {
                            Stage8bP1eRecoveryStepRouteV1::LimitTruthCommitted(Box::new(truth))
                        }
                        Stage8bP1RedisCancelCommitOutcome::CancelContinuationPending(pending) => {
                            Stage8bP1eRecoveryStepRouteV1::CancelContinuationPending(Box::new(
                                pending,
                            ))
                        }
                    };
                    Ok(Stage8bP1eCancelScheduleAdvanceOutcomeV1::Lifecycle(
                        Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                            Stage8bP1eRecoveryStepV1 {
                                route: Box::new(route),
                                control,
                            },
                        )),
                    ))
                }
            }
        }
    }
}

/// Completes schedule checkpoints C-F for an already published initial LIMIT
/// command, then rejoins the ordinary LIMIT S_ack/S_truth/XACK lifecycle.
pub async fn advance_stage8b_p1e_initial_limit_schedule_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eInitialLimitScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    advance_stage8b_p1e_initial_limit_schedule_with_timeout_v1(
        deferred,
        reader,
        context,
        latch,
        bound_at_utc,
        commitment_key,
        StdDuration::from_millis(crate::STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn advance_stage8b_p1e_initial_limit_schedule_with_timeout_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    operation_timeout: StdDuration,
) -> Result<Stage8bP1eInitialLimitScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    if context.trusted_now != bound_at_utc {
        return Err(Stage8bP1RedisSemanticError::P1eScheduleClockMismatch.into());
    }
    if deferred.kind != Stage8bP1eScheduleDeferredKindV1::CommandPublishedInitialLimit
        || !matches!(
            deferred._route.as_ref(),
            Stage8bP1eScheduleDeferredRouteV1::CommandPublished(_)
        )
    {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    }
    let read = match reader
        .read_newest_guarded_with_timeout(context, latch, operation_timeout)
        .await
    {
        Ok(read) => read,
        Err(error) if retryable_schedule_read_error(&error) => {
            return Ok(Stage8bP1eInitialLimitScheduleAdvanceOutcomeV1::AwaitingSchedule(deferred));
        }
        Err(error) => return Err(error.into()),
    };
    let Stage8bP1eScheduleDeferredRecoveryV1 {
        kind,
        _route: route,
        control,
    } = deferred;
    let Stage8bP1eScheduleDeferredRouteV1::CommandPublished(published) = *route else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    };
    match read {
        crate::Stage8bP1eGuardedScheduleReadV1::Stopped(receipt) => {
            drop(published);
            Ok(Stage8bP1eInitialLimitScheduleAdvanceOutcomeV1::Stopped(
                Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
            ))
        }
        crate::Stage8bP1eGuardedScheduleReadV1::Read(
            crate::Stage8bP1eNewestScheduleReadV1::Empty,
        ) => Ok(
            Stage8bP1eInitialLimitScheduleAdvanceOutcomeV1::AwaitingSchedule(
                Stage8bP1eScheduleDeferredRecoveryV1 {
                    kind,
                    _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                        published,
                    )),
                    control,
                },
            ),
        ),
        crate::Stage8bP1eGuardedScheduleReadV1::Read(
            crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
        ) => {
            let committed_high_water = snapshot.high_water().clone();
            match resume_stage8b_p1e_initial_limit_with_signed_schedule_timeout(
                *published,
                *snapshot,
                latch,
                bound_at_utc,
                commitment_key,
                operation_timeout,
            )
            .await?
            {
                Stage8bP1eSignedInitialLimitScheduleOutcomeV1::Stopped(receipt) => {
                    Ok(Stage8bP1eInitialLimitScheduleAdvanceOutcomeV1::Stopped(
                        Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
                    ))
                }
                Stage8bP1eSignedInitialLimitScheduleOutcomeV1::AwaitingSuccessor(published) => Ok(
                    Stage8bP1eInitialLimitScheduleAdvanceOutcomeV1::AwaitingSchedule(
                        Stage8bP1eScheduleDeferredRecoveryV1 {
                            kind,
                            _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                                published,
                            )),
                            control,
                        },
                    ),
                ),
                Stage8bP1eSignedInitialLimitScheduleOutcomeV1::LimitAckCommitted(ack) => {
                    context.high_water = Some(committed_high_water);
                    Ok(Stage8bP1eInitialLimitScheduleAdvanceOutcomeV1::Lifecycle(
                        Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                            Stage8bP1eRecoveryStepV1 {
                                route: Box::new(Stage8bP1eRecoveryStepRouteV1::LimitAckCommitted(
                                    ack,
                                )),
                                control,
                            },
                        )),
                    ))
                }
            }
        }
    }
}

fn retryable_schedule_read_error(error: &crate::Stage8bP1eScheduleReadError) -> bool {
    matches!(
        error,
        crate::Stage8bP1eScheduleReadError::Redis(_)
            | crate::Stage8bP1eScheduleReadError::OperationTimeout
            | crate::Stage8bP1eScheduleReadError::Source(
                strategy_runtime_core::Stage8bP1eScheduleSourceError::Stale
            )
    )
}

/// Completes schedule checkpoints C-F for an already published Market
/// command. Empty newest-only reads retain the exact published owner. A
/// successful schedule binding reaches exactly one S_ack owner and rejoins
/// the ordinary row-bounded recovery state machine.
pub async fn advance_stage8b_p1e_market_schedule_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eMarketScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    advance_stage8b_p1e_market_schedule_with_timeout_v1(
        deferred,
        reader,
        context,
        latch,
        bound_at_utc,
        commitment_key,
        StdDuration::from_millis(crate::STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn advance_stage8b_p1e_market_schedule_with_timeout_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    operation_timeout: StdDuration,
) -> Result<Stage8bP1eMarketScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    if context.trusted_now != bound_at_utc {
        return Err(Stage8bP1RedisSemanticError::P1eScheduleClockMismatch.into());
    }
    if deferred.kind != Stage8bP1eScheduleDeferredKindV1::CommandPublishedMarket
        || !matches!(
            deferred._route.as_ref(),
            Stage8bP1eScheduleDeferredRouteV1::CommandPublished(_)
        )
    {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    }
    let read = match reader
        .read_newest_guarded_with_timeout(context, latch, operation_timeout)
        .await
    {
        Ok(read) => read,
        Err(error) if retryable_schedule_read_error(&error) => {
            return Ok(Stage8bP1eMarketScheduleAdvanceOutcomeV1::AwaitingSchedule(
                deferred,
            ));
        }
        Err(error) => return Err(error.into()),
    };
    let Stage8bP1eScheduleDeferredRecoveryV1 {
        kind,
        _route: route,
        control,
    } = deferred;
    let Stage8bP1eScheduleDeferredRouteV1::CommandPublished(published) = *route else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    };

    match read {
        crate::Stage8bP1eGuardedScheduleReadV1::Stopped(receipt) => {
            drop(published);
            Ok(Stage8bP1eMarketScheduleAdvanceOutcomeV1::Stopped(
                Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
            ))
        }
        crate::Stage8bP1eGuardedScheduleReadV1::Read(
            crate::Stage8bP1eNewestScheduleReadV1::Empty,
        ) => Ok(Stage8bP1eMarketScheduleAdvanceOutcomeV1::AwaitingSchedule(
            Stage8bP1eScheduleDeferredRecoveryV1 {
                kind,
                _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                    published,
                )),
                control,
            },
        )),
        crate::Stage8bP1eGuardedScheduleReadV1::Read(
            crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
        ) => {
            let committed_high_water = snapshot.high_water().clone();
            match resume_stage8b_p1e_command_published_with_signed_schedule_timeout(
                *published,
                *snapshot,
                latch,
                bound_at_utc,
                commitment_key,
                operation_timeout,
            )
            .await?
            {
                Stage8bP1eSignedMarketScheduleOutcomeV1::Stopped(receipt) => {
                    Ok(Stage8bP1eMarketScheduleAdvanceOutcomeV1::Stopped(
                        Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
                    ))
                }
                Stage8bP1eSignedMarketScheduleOutcomeV1::AwaitingSuccessor(published) => {
                    Ok(Stage8bP1eMarketScheduleAdvanceOutcomeV1::AwaitingSchedule(
                        Stage8bP1eScheduleDeferredRecoveryV1 {
                            kind,
                            _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                                published,
                            )),
                            control,
                        },
                    ))
                }
                Stage8bP1eSignedMarketScheduleOutcomeV1::FeedbackAckCommitted(ack) => {
                    context.high_water = Some(committed_high_water);
                    Ok(Stage8bP1eMarketScheduleAdvanceOutcomeV1::Lifecycle(
                        Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                            Stage8bP1eRecoveryStepV1 {
                                route: Box::new(
                                    Stage8bP1eRecoveryStepRouteV1::FeedbackAckCommitted(ack),
                                ),
                                control,
                            },
                        )),
                    ))
                }
            }
        }
    }
}

/// Completes schedule checkpoints C-F for the exact reservation-bearing
/// generated-Market publication and rejoins the combined P1-d4 lifecycle.
pub async fn advance_stage8b_p1e_generated_market_schedule_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eGeneratedMarketScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    advance_stage8b_p1e_generated_market_schedule_with_timeout_v1(
        deferred,
        reader,
        context,
        latch,
        bound_at_utc,
        commitment_key,
        StdDuration::from_millis(crate::STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn advance_stage8b_p1e_generated_market_schedule_with_timeout_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    operation_timeout: StdDuration,
) -> Result<Stage8bP1eGeneratedMarketScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    if context.trusted_now != bound_at_utc {
        return Err(Stage8bP1RedisSemanticError::P1eScheduleClockMismatch.into());
    }
    if deferred.kind != Stage8bP1eScheduleDeferredKindV1::CommandPublishedGeneratedMarket
        || !matches!(
            deferred._route.as_ref(),
            Stage8bP1eScheduleDeferredRouteV1::CommandPublished(_)
        )
    {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    }
    let read = match reader
        .read_newest_guarded_with_timeout(context, latch, operation_timeout)
        .await
    {
        Ok(read) => read,
        Err(error) if retryable_schedule_read_error(&error) => {
            return Ok(
                Stage8bP1eGeneratedMarketScheduleAdvanceOutcomeV1::AwaitingSchedule(deferred),
            );
        }
        Err(error) => return Err(error.into()),
    };
    let Stage8bP1eScheduleDeferredRecoveryV1 {
        kind,
        _route: route,
        control,
    } = deferred;
    let Stage8bP1eScheduleDeferredRouteV1::CommandPublished(published) = *route else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    };
    match read {
        crate::Stage8bP1eGuardedScheduleReadV1::Stopped(receipt) => {
            drop(published);
            Ok(Stage8bP1eGeneratedMarketScheduleAdvanceOutcomeV1::Stopped(
                Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
            ))
        }
        crate::Stage8bP1eGuardedScheduleReadV1::Read(
            crate::Stage8bP1eNewestScheduleReadV1::Empty,
        ) => Ok(
            Stage8bP1eGeneratedMarketScheduleAdvanceOutcomeV1::AwaitingSchedule(
                Stage8bP1eScheduleDeferredRecoveryV1 {
                    kind,
                    _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                        published,
                    )),
                    control,
                },
            ),
        ),
        crate::Stage8bP1eGuardedScheduleReadV1::Read(
            crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
        ) => {
            let committed_high_water = snapshot.high_water().clone();
            match resume_stage8b_p1e_generated_market_with_signed_schedule_timeout(
                *published,
                *snapshot,
                latch,
                bound_at_utc,
                commitment_key,
                operation_timeout,
            )
            .await?
            {
                Stage8bP1eSignedGeneratedMarketScheduleOutcomeV1::Stopped(receipt) => {
                    Ok(Stage8bP1eGeneratedMarketScheduleAdvanceOutcomeV1::Stopped(
                        Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
                    ))
                }
                Stage8bP1eSignedGeneratedMarketScheduleOutcomeV1::AwaitingSuccessor(published) => {
                    Ok(
                        Stage8bP1eGeneratedMarketScheduleAdvanceOutcomeV1::AwaitingSchedule(
                            Stage8bP1eScheduleDeferredRecoveryV1 {
                                kind,
                                _route: Box::new(
                                    Stage8bP1eScheduleDeferredRouteV1::CommandPublished(published),
                                ),
                                control,
                            },
                        ),
                    )
                }
                Stage8bP1eSignedGeneratedMarketScheduleOutcomeV1::GeneratedMarketAckCommitted(
                    ack,
                ) => {
                    context.high_water = Some(committed_high_water);
                    Ok(
                        Stage8bP1eGeneratedMarketScheduleAdvanceOutcomeV1::Lifecycle(
                            Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                                Stage8bP1eRecoveryStepV1 {
                                    route: Box::new(
                                        Stage8bP1eRecoveryStepRouteV1::GeneratedMarketAckCommitted(
                                            ack,
                                        ),
                                    ),
                                    control,
                                },
                            )),
                        ),
                    )
                }
            }
        }
    }
}

/// Completes schedule checkpoints C-F for the exact Ready/Working-LIMIT
/// continuation. Unsupported schedule routes are rejected before schedule
/// I/O. An empty newest-only read retains the same linear permit; a stop drops
/// effect authority and relies on authenticated restart while keeping the M10
/// pending; a successful effect rejoins the row-bounded recovery state
/// machine.
pub async fn advance_stage8b_p1e_ready_working_schedule_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eWorkingScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    advance_stage8b_p1e_ready_working_schedule_with_timeout_v1(
        deferred,
        reader,
        context,
        latch,
        bound_at_utc,
        commitment_key,
        StdDuration::from_millis(crate::STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn advance_stage8b_p1e_ready_working_schedule_with_timeout_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    operation_timeout: StdDuration,
) -> Result<Stage8bP1eWorkingScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    if context.trusted_now != bound_at_utc {
        return Err(Stage8bP1RedisSemanticError::P1eScheduleClockMismatch.into());
    }
    if deferred.kind != Stage8bP1eScheduleDeferredKindV1::RoutedContinuation
        || !matches!(
            deferred._route.as_ref(),
            Stage8bP1eScheduleDeferredRouteV1::RoutedContinuation(route)
                if matches!(
                    route.as_ref(),
                    Stage8bP1eRoutedContinuationV1::ReadyWorkingLimit(_)
                )
        )
    {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    }
    let read = match reader
        .read_newest_guarded_with_timeout(context, latch, operation_timeout)
        .await
    {
        Ok(read) => read,
        Err(error) if retryable_schedule_read_error(&error) => {
            return Ok(Stage8bP1eWorkingScheduleAdvanceOutcomeV1::AwaitingSchedule(
                deferred,
            ));
        }
        Err(error) => return Err(error.into()),
    };
    let Stage8bP1eScheduleDeferredRecoveryV1 {
        kind,
        _route: route,
        control,
    } = deferred;
    let Stage8bP1eScheduleDeferredRouteV1::RoutedContinuation(route) = *route else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    };
    let Stage8bP1eRoutedContinuationV1::ReadyWorkingLimit(permit) = *route else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    };

    match read {
        crate::Stage8bP1eGuardedScheduleReadV1::Stopped(receipt) => {
            drop(permit);
            Ok(Stage8bP1eWorkingScheduleAdvanceOutcomeV1::Stopped(
                Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
            ))
        }
        crate::Stage8bP1eGuardedScheduleReadV1::Read(
            crate::Stage8bP1eNewestScheduleReadV1::Empty,
        ) => Ok(Stage8bP1eWorkingScheduleAdvanceOutcomeV1::AwaitingSchedule(
            Stage8bP1eScheduleDeferredRecoveryV1 {
                kind,
                _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::RoutedContinuation(
                    Box::new(Stage8bP1eRoutedContinuationV1::ReadyWorkingLimit(permit)),
                )),
                control,
            },
        )),
        crate::Stage8bP1eGuardedScheduleReadV1::Read(
            crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
        ) => {
            let committed_high_water = snapshot.high_water().clone();
            match resume_stage8b_p1e_ready_working_limit_with_signed_schedule(
                permit,
                *snapshot,
                latch,
                bound_at_utc,
                commitment_key,
            )
            .await?
            {
                Stage8bP1eSignedWorkingScheduleOutcomeV1::Stopped(receipt) => {
                    Ok(Stage8bP1eWorkingScheduleAdvanceOutcomeV1::Stopped(
                        Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
                    ))
                }
                Stage8bP1eSignedWorkingScheduleOutcomeV1::Semantic(outcome) => {
                    context.high_water = Some(committed_high_water);
                    Ok(Stage8bP1eWorkingScheduleAdvanceOutcomeV1::Lifecycle(
                        classify_recovered_semantic_outcome(outcome, control),
                    ))
                }
            }
        }
    }
}

/// Completes schedule checkpoints C-F for a source-free Ready Day-expiry
/// route. Empty/retryable reads retain the exact owner; success commits the
/// terminal expiry and returns Ready without any M10 XACK.
pub async fn advance_stage8b_p1e_day_expiry_schedule_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eDayExpiryScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    advance_stage8b_p1e_day_expiry_schedule_with_timeout_v1(
        deferred,
        reader,
        context,
        latch,
        bound_at_utc,
        commitment_key,
        StdDuration::from_millis(crate::STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn advance_stage8b_p1e_day_expiry_schedule_with_timeout_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    operation_timeout: StdDuration,
) -> Result<Stage8bP1eDayExpiryScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    if context.trusted_now != bound_at_utc {
        return Err(Stage8bP1RedisSemanticError::P1eScheduleClockMismatch.into());
    }
    if deferred.kind != Stage8bP1eScheduleDeferredKindV1::ReadyDayExpiry
        || !matches!(
            deferred._route.as_ref(),
            Stage8bP1eScheduleDeferredRouteV1::ReadyDayExpiry { .. }
        )
    {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    }
    let Stage8bP1eScheduleDeferredRouteV1::ReadyDayExpiry {
        durable_high_water, ..
    } = deferred._route.as_ref()
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    };
    apply_recovered_schedule_high_water(context, Some(durable_high_water.clone()))?;
    let read = match reader
        .read_newest_guarded_with_timeout(context, latch, operation_timeout)
        .await
    {
        Ok(read) => read,
        Err(error) if retryable_schedule_read_error(&error) => {
            return Ok(Stage8bP1eDayExpiryScheduleAdvanceOutcomeV1::AwaitingSchedule(deferred));
        }
        Err(error) => return Err(error.into()),
    };
    let Stage8bP1eScheduleDeferredRecoveryV1 {
        kind,
        _route: route,
        control,
    } = deferred;
    let Stage8bP1eScheduleDeferredRouteV1::ReadyDayExpiry {
        owner,
        durable_high_water,
    } = *route
    else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    };
    match read {
        crate::Stage8bP1eGuardedScheduleReadV1::Stopped(receipt) => {
            drop(owner);
            Ok(Stage8bP1eDayExpiryScheduleAdvanceOutcomeV1::Stopped(
                Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
            ))
        }
        crate::Stage8bP1eGuardedScheduleReadV1::Read(
            crate::Stage8bP1eNewestScheduleReadV1::Empty,
        ) => Ok(
            Stage8bP1eDayExpiryScheduleAdvanceOutcomeV1::AwaitingSchedule(
                Stage8bP1eScheduleDeferredRecoveryV1 {
                    kind,
                    _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::ReadyDayExpiry {
                        owner,
                        durable_high_water,
                    }),
                    control,
                },
            ),
        ),
        crate::Stage8bP1eGuardedScheduleReadV1::Read(
            crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
        ) => {
            let committed_high_water = snapshot.high_water().clone();
            match resume_stage8b_p1e_day_expiry_with_signed_schedule(
                *owner,
                *snapshot,
                latch,
                bound_at_utc,
                commitment_key,
            )? {
                Stage8bP1eSignedDayExpiryScheduleOutcomeV1::Stopped(receipt) => {
                    Ok(Stage8bP1eDayExpiryScheduleAdvanceOutcomeV1::Stopped(
                        Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
                    ))
                }
                Stage8bP1eSignedDayExpiryScheduleOutcomeV1::Ready(owner) => {
                    context.high_water = Some(committed_high_water);
                    Ok(Stage8bP1eDayExpiryScheduleAdvanceOutcomeV1::Ready(
                        Stage8bP1eReadyPollingV1 {
                            owner,
                            control,
                            committed_cancel_disposition: None,
                        },
                    ))
                }
            }
        }
    }
}

/// Performs one schedule read for the two route shapes already implemented by
/// I1, then drains a successful effect through the same bounded/latch-guarded
/// lifecycle adapter used by S09. The caller may retry `AwaitingSchedule`
/// under the separately bounded acquisition policy; unsupported route owners
/// are returned byte-for-byte without schedule I/O.
pub async fn advance_stage8b_p1e_supported_schedule_once_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eSupportedScheduleCycleOutcomeV1, Stage8bP1eStartupErrorV1> {
    advance_stage8b_p1e_supported_schedule_once_with_timeout_v1(
        deferred,
        reader,
        context,
        latch,
        bound_at_utc,
        commitment_key,
        StdDuration::from_millis(crate::STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn advance_stage8b_p1e_supported_schedule_once_with_timeout_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    operation_timeout: StdDuration,
) -> Result<Stage8bP1eSupportedScheduleCycleOutcomeV1, Stage8bP1eStartupErrorV1> {
    let route = match supported_schedule_route(&deferred) {
        Some(route) => route,
        None => {
            return Ok(Stage8bP1eSupportedScheduleCycleOutcomeV1::Unsupported(
                deferred,
            ));
        }
    };
    let lifecycle = match route {
        Stage8bP1eSupportedScheduleRouteV1::PlainMarket => {
            match advance_stage8b_p1e_market_schedule_with_timeout_v1(
                deferred,
                reader,
                context,
                latch,
                bound_at_utc,
                commitment_key,
                operation_timeout,
            )
            .await?
            {
                Stage8bP1eMarketScheduleAdvanceOutcomeV1::Stopped(stopped) => {
                    return Ok(Stage8bP1eSupportedScheduleCycleOutcomeV1::Stopped(stopped));
                }
                Stage8bP1eMarketScheduleAdvanceOutcomeV1::AwaitingSchedule(deferred) => {
                    return Ok(Stage8bP1eSupportedScheduleCycleOutcomeV1::AwaitingSchedule(
                        deferred,
                    ));
                }
                Stage8bP1eMarketScheduleAdvanceOutcomeV1::Lifecycle(lifecycle) => lifecycle,
            }
        }
        Stage8bP1eSupportedScheduleRouteV1::GeneratedMarket => {
            match advance_stage8b_p1e_generated_market_schedule_with_timeout_v1(
                deferred,
                reader,
                context,
                latch,
                bound_at_utc,
                commitment_key,
                operation_timeout,
            )
            .await?
            {
                Stage8bP1eGeneratedMarketScheduleAdvanceOutcomeV1::Stopped(stopped) => {
                    return Ok(Stage8bP1eSupportedScheduleCycleOutcomeV1::Stopped(stopped));
                }
                Stage8bP1eGeneratedMarketScheduleAdvanceOutcomeV1::AwaitingSchedule(deferred) => {
                    return Ok(Stage8bP1eSupportedScheduleCycleOutcomeV1::AwaitingSchedule(
                        deferred,
                    ));
                }
                Stage8bP1eGeneratedMarketScheduleAdvanceOutcomeV1::Lifecycle(lifecycle) => {
                    lifecycle
                }
            }
        }
        Stage8bP1eSupportedScheduleRouteV1::InitialLimit => {
            match advance_stage8b_p1e_initial_limit_schedule_with_timeout_v1(
                deferred,
                reader,
                context,
                latch,
                bound_at_utc,
                commitment_key,
                operation_timeout,
            )
            .await?
            {
                Stage8bP1eInitialLimitScheduleAdvanceOutcomeV1::Stopped(stopped) => {
                    return Ok(Stage8bP1eSupportedScheduleCycleOutcomeV1::Stopped(stopped));
                }
                Stage8bP1eInitialLimitScheduleAdvanceOutcomeV1::AwaitingSchedule(deferred) => {
                    return Ok(Stage8bP1eSupportedScheduleCycleOutcomeV1::AwaitingSchedule(
                        deferred,
                    ));
                }
                Stage8bP1eInitialLimitScheduleAdvanceOutcomeV1::Lifecycle(lifecycle) => lifecycle,
            }
        }
        Stage8bP1eSupportedScheduleRouteV1::Cancel => {
            match advance_stage8b_p1e_cancel_schedule_with_timeout_v1(
                deferred,
                reader,
                context,
                latch,
                bound_at_utc,
                commitment_key,
                operation_timeout,
            )
            .await?
            {
                Stage8bP1eCancelScheduleAdvanceOutcomeV1::Stopped(stopped) => {
                    return Ok(Stage8bP1eSupportedScheduleCycleOutcomeV1::Stopped(stopped));
                }
                Stage8bP1eCancelScheduleAdvanceOutcomeV1::AwaitingSchedule(deferred) => {
                    return Ok(Stage8bP1eSupportedScheduleCycleOutcomeV1::AwaitingSchedule(
                        deferred,
                    ));
                }
                Stage8bP1eCancelScheduleAdvanceOutcomeV1::Lifecycle(lifecycle) => lifecycle,
            }
        }
        Stage8bP1eSupportedScheduleRouteV1::ReadyWorkingLimit => {
            match advance_stage8b_p1e_ready_working_schedule_with_timeout_v1(
                deferred,
                reader,
                context,
                latch,
                bound_at_utc,
                commitment_key,
                operation_timeout,
            )
            .await?
            {
                Stage8bP1eWorkingScheduleAdvanceOutcomeV1::Stopped(stopped) => {
                    return Ok(Stage8bP1eSupportedScheduleCycleOutcomeV1::Stopped(stopped));
                }
                Stage8bP1eWorkingScheduleAdvanceOutcomeV1::AwaitingSchedule(deferred) => {
                    return Ok(Stage8bP1eSupportedScheduleCycleOutcomeV1::AwaitingSchedule(
                        deferred,
                    ));
                }
                Stage8bP1eWorkingScheduleAdvanceOutcomeV1::Lifecycle(lifecycle) => lifecycle,
            }
        }
        Stage8bP1eSupportedScheduleRouteV1::ReadyDayExpiry => {
            return Ok(
                match advance_stage8b_p1e_day_expiry_schedule_with_timeout_v1(
                    deferred,
                    reader,
                    context,
                    latch,
                    bound_at_utc,
                    commitment_key,
                    operation_timeout,
                )
                .await?
                {
                    Stage8bP1eDayExpiryScheduleAdvanceOutcomeV1::Stopped(stopped) => {
                        Stage8bP1eSupportedScheduleCycleOutcomeV1::Stopped(stopped)
                    }
                    Stage8bP1eDayExpiryScheduleAdvanceOutcomeV1::AwaitingSchedule(deferred) => {
                        Stage8bP1eSupportedScheduleCycleOutcomeV1::AwaitingSchedule(deferred)
                    }
                    Stage8bP1eDayExpiryScheduleAdvanceOutcomeV1::Ready(ready) => {
                        Stage8bP1eSupportedScheduleCycleOutcomeV1::Ready(ready)
                    }
                },
            );
        }
    };
    Ok(
        match drain_stage8b_p1e_recovery_lifecycle_v1(lifecycle, latch, commitment_key).await? {
            Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(ready) => {
                Stage8bP1eSupportedScheduleCycleOutcomeV1::Ready(ready)
            }
            Stage8bP1eScheduleFreeDrainOutcomeV1::RetainedForRestart(retained) => {
                Stage8bP1eSupportedScheduleCycleOutcomeV1::RetainedRecovery(retained)
            }
            Stage8bP1eScheduleFreeDrainOutcomeV1::PendingNotClaimable(pending) => {
                Stage8bP1eSupportedScheduleCycleOutcomeV1::PendingNotClaimable(pending)
            }
            Stage8bP1eScheduleFreeDrainOutcomeV1::Blocked(blocked) => {
                Stage8bP1eSupportedScheduleCycleOutcomeV1::Blocked(blocked)
            }
            Stage8bP1eScheduleFreeDrainOutcomeV1::ScheduleDeferred(deferred) => {
                Stage8bP1eSupportedScheduleCycleOutcomeV1::ScheduleDeferred(deferred)
            }
        },
    )
}

/// Runs the exact I1A schedule acquisition policy around the two supported
/// signed-schedule routes. Only empty, stale, Redis-transport and operation-
/// timeout observations are retried. Authentication, identity, progression
/// and binding failures remain immediate fail-closed errors. Once a verified
/// source reaches durable binding, no wall-clock timeout wraps or cancels the
/// linear lifecycle future.
pub async fn advance_stage8b_p1e_supported_schedule_bounded_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eBoundedScheduleCycleOutcomeV1, Stage8bP1eStartupErrorV1> {
    advance_stage8b_p1e_supported_schedule_with_policy_v1(
        deferred,
        reader,
        context,
        latch,
        commitment_key,
        Stage8bP1eScheduleAcquisitionPolicyV1::production(),
    )
    .await
}

async fn advance_stage8b_p1e_supported_schedule_with_policy_v1(
    mut deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    policy: Stage8bP1eScheduleAcquisitionPolicyV1,
) -> Result<Stage8bP1eBoundedScheduleCycleOutcomeV1, Stage8bP1eStartupErrorV1> {
    if !policy.is_valid() {
        return Err(Stage8bP1RedisSemanticError::P1eScheduleRetryPolicyInvalid.into());
    }
    if supported_schedule_route(&deferred).is_none() {
        return Ok(Stage8bP1eBoundedScheduleCycleOutcomeV1::Unsupported(
            deferred,
        ));
    }

    let started = tokio::time::Instant::now();
    let deadline = started + policy.total_deadline;
    let initial_trusted_now = context.trusted_now;
    let mut attempts = 0_u8;
    loop {
        let now = tokio::time::Instant::now();
        if latch.intent().is_none() && (attempts >= policy.attempts || now >= deadline) {
            return Ok(Stage8bP1eBoundedScheduleCycleOutcomeV1::Exhausted(deferred));
        }
        let elapsed = now.saturating_duration_since(started);
        let elapsed = chrono::Duration::from_std(elapsed)
            .map_err(|_| Stage8bP1RedisSemanticError::P1eScheduleClockMismatch)?;
        let bound_at_utc = initial_trusted_now
            .checked_add_signed(elapsed)
            .ok_or(Stage8bP1RedisSemanticError::P1eScheduleClockMismatch)?;
        context.trusted_now = bound_at_utc;
        let remaining = deadline.saturating_duration_since(now);
        let operation_timeout = if latch.intent().is_some() {
            StdDuration::from_nanos(1)
        } else {
            policy.redis_operation_timeout.min(remaining)
        };
        if operation_timeout.is_zero() {
            return Ok(Stage8bP1eBoundedScheduleCycleOutcomeV1::Exhausted(deferred));
        }
        attempts = attempts.saturating_add(1);
        match advance_stage8b_p1e_supported_schedule_once_with_timeout_v1(
            deferred,
            reader,
            context,
            latch,
            bound_at_utc,
            commitment_key,
            operation_timeout,
        )
        .await?
        {
            Stage8bP1eSupportedScheduleCycleOutcomeV1::AwaitingSchedule(next) => {
                deferred = next;
            }
            Stage8bP1eSupportedScheduleCycleOutcomeV1::Stopped(stopped) => {
                return Ok(Stage8bP1eBoundedScheduleCycleOutcomeV1::Stopped(stopped));
            }
            Stage8bP1eSupportedScheduleCycleOutcomeV1::Ready(ready) => {
                return Ok(Stage8bP1eBoundedScheduleCycleOutcomeV1::Ready(ready));
            }
            Stage8bP1eSupportedScheduleCycleOutcomeV1::RetainedRecovery(retained) => {
                return Ok(Stage8bP1eBoundedScheduleCycleOutcomeV1::RetainedRecovery(
                    retained,
                ));
            }
            Stage8bP1eSupportedScheduleCycleOutcomeV1::PendingNotClaimable(pending) => {
                return Ok(Stage8bP1eBoundedScheduleCycleOutcomeV1::PendingNotClaimable(pending));
            }
            Stage8bP1eSupportedScheduleCycleOutcomeV1::Blocked(blocked) => {
                return Ok(Stage8bP1eBoundedScheduleCycleOutcomeV1::Blocked(blocked));
            }
            Stage8bP1eSupportedScheduleCycleOutcomeV1::ScheduleDeferred(next) => {
                return Ok(Stage8bP1eBoundedScheduleCycleOutcomeV1::ScheduleDeferred(
                    next,
                ));
            }
            Stage8bP1eSupportedScheduleCycleOutcomeV1::Unsupported(next) => {
                return Ok(Stage8bP1eBoundedScheduleCycleOutcomeV1::Unsupported(next));
            }
        }

        if attempts >= policy.attempts {
            return Ok(Stage8bP1eBoundedScheduleCycleOutcomeV1::Exhausted(deferred));
        }
        wait_stage8b_p1e_schedule_backoff_v1(policy.backoff_after(attempts), deadline, latch).await;
    }
}

async fn wait_stage8b_p1e_schedule_backoff_v1(
    delay: StdDuration,
    deadline: tokio::time::Instant,
    latch: &Stage8bP1eShutdownLatchV1,
) {
    let wait_deadline = tokio::time::Instant::now()
        .checked_add(delay)
        .map_or(deadline, |candidate| candidate.min(deadline));
    let poll = StdDuration::from_millis(STAGE8B_P1E_SCHEDULE_SHUTDOWN_POLL_MS);
    while latch.intent().is_none() {
        let now = tokio::time::Instant::now();
        if now >= wait_deadline {
            break;
        }
        tokio::time::sleep(poll.min(wait_deadline.saturating_duration_since(now))).await;
    }
}

pub enum Stage8bP1eRecoveryAdvanceOutcomeV1 {
    Continue(Box<Stage8bP1eRecoveryStepV1>),
    Ready(Box<Stage8bP1eRecoveredReadyV1>),
    PendingNotClaimable(Box<Stage8bP1eRecoveredPendingNotClaimableV1>),
    Blocked(Box<Stage8bP1eRecoveredBlockedV1>),
    ScheduleDeferred(Box<Stage8bP1eScheduleDeferredRecoveryV1>),
}

/// Terminal result of one same-invocation schedule-free source drain. A
/// successful `Ready` result owns the only authority that may return to S08;
/// all other variants remain non-pollable by construction.
pub enum Stage8bP1eScheduleFreeDrainOutcomeV1 {
    Ready(Stage8bP1eReadyPollingV1),
    RetainedForRestart(Stage8bP1eRetainedRecoveryBoundaryV1),
    PendingNotClaimable(Stage8bP1eRecoveredPendingNotClaimableV1),
    Blocked(Stage8bP1eRecoveredBlockedV1),
    ScheduleDeferred(Stage8bP1eScheduleDeferredRecoveryV1),
}

/// Terminal boundary of the schedule-free S09 owner task. `Ready` is
/// intentionally absent: an exact Ready owner is always retained inside the
/// task and immediately returns to the next bounded S08 poll. Every returned
/// variant either proves shutdown retention or owns a route that requires
/// external schedule/retry policy before polling may resume.
pub enum Stage8bP1eScheduleFreeOwnerLoopOutcomeV1 {
    StoppedReady(Stage8bP1eStoppedReadyPollingV1),
    CommittedCancelResolved(Stage8bP1eCommittedCancelResolvedV1),
    RetainedSource(Stage8bP1eRetainedStartupV1),
    RetainedRecovery(Stage8bP1eRetainedRecoveryBoundaryV1),
    PendingNotClaimable(Stage8bP1eRecoveredPendingNotClaimableV1),
    Blocked(Stage8bP1eRecoveredBlockedV1),
    ScheduleDeferred(Stage8bP1eScheduleDeferredRecoveryV1),
}

/// Terminal boundary of the combined S08/S09 owner task for the route shapes
/// composed by I1. Exact Ready is deliberately absent: both schedule-free and
/// signed-schedule success remain inside the task and immediately resume
/// bounded fresh polling.
pub enum Stage8bP1eOwnerLoopOutcomeV1 {
    StoppedReady(Stage8bP1eStoppedReadyPollingV1),
    StoppedSchedule(Stage8bP1eScheduleStoppedRecoveryV1),
    CommittedCancelRestartRequired(Stage8bP1eCommittedCancelRestartRequiredV1),
    CommittedCancelResolved(Stage8bP1eCommittedCancelResolvedV1),
    CommittedDayExpiryResolved(Stage8bP1eCommittedDayExpiryResolvedV1),
    RetainedSource(Stage8bP1eRetainedStartupV1),
    RetainedRecovery(Stage8bP1eRetainedRecoveryBoundaryV1),
    PendingNotClaimable(Stage8bP1eRecoveredPendingNotClaimableV1),
    Blocked(Stage8bP1eRecoveredBlockedV1),
    ScheduleExhausted(Stage8bP1eScheduleDeferredRecoveryV1),
    UnsupportedSchedule(Stage8bP1eScheduleDeferredRecoveryV1),
    StartupPendingNotClaimable(Stage8bP1ePendingNotClaimableStartupV1),
    StartupLimitSchedule(Stage8bP1eLimitScheduleStartupV1),
    StartupCommittedSchedule(Stage8bP1eCommittedScheduleStartupV1),
}

/// Typed completion boundary after a CANCEL has persisted replacement truth
/// and resolved its exact source XACK.  The linear Ready owner is retained,
/// but cannot poll until the caller explicitly consumes this boundary through
/// `into_ready_polling`.  That handoff prevents re-admission of the completed
/// V4 while preserving forward progress to the next canonical M10.
pub struct Stage8bP1eCommittedCancelResolvedV1 {
    disposition: Stage8bP1RedisZeroIntentAckDisposition,
    ready: Stage8bP1eReadyPollingV1,
}

impl Stage8bP1eCommittedCancelResolvedV1 {
    pub const fn disposition(&self) -> Stage8bP1RedisZeroIntentAckDisposition {
        self.disposition
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        self.ready.redis_control_mut()
    }

    /// Consumes the completed-CANCEL boundary exactly once and returns the
    /// same authenticated owner to ordinary Ready polling.  The completion
    /// marker is cleared as part of the move, so the old V4 cannot be emitted
    /// again and the next source is not mistaken for the completed CANCEL.
    pub fn into_ready_polling(self) -> Stage8bP1eReadyPollingV1 {
        self.ready
    }
}

/// Terminal composition boundary after source-free committed Day-expiry.
/// Retaining the recovered owner proves that no source claim/XACK or fresh
/// schedule admission was introduced while finishing this invocation.
pub struct Stage8bP1eCommittedDayExpiryResolvedV1 {
    _owner: Box<Stage8bP1RedisSemanticCompositionOwner>,
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eCommittedDayExpiryResolvedV1 {
    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }
}

/// Terminal owner-loop boundary after a committed V4 CANCEL reaches the
/// durable target-first `P1d3CancelContinuationPending` seal. The transient
/// owner is deliberately released: the next process invocation must
/// authenticate and reopen that exact durable frontier before recovered
/// CANCEL settlement. This prevents both same-owner cached-seal reuse and a
/// fall-through to fresh signed-schedule admission.
pub struct Stage8bP1eCommittedCancelRestartRequiredV1 {
    control: Stage8bP1eRedisControlV1,
}

impl Stage8bP1eCommittedCancelRestartRequiredV1 {
    pub const fn kind(&self) -> Stage8bP1eRestartKindV1 {
        Stage8bP1eRestartKindV1::P1d3CancelContinuationPending
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }
}

enum Stage8bP1eOwnerLoopEntryV1 {
    Ready(Stage8bP1eReadyPollingV1),
    ScheduleDeferred(Stage8bP1eScheduleDeferredRecoveryV1),
}

struct Stage8bP1eOwnerLoopClockV1 {
    trusted_utc_anchor: DateTime<Utc>,
    monotonic_anchor: tokio::time::Instant,
}

impl Stage8bP1eOwnerLoopClockV1 {
    fn new(trusted_utc_anchor: DateTime<Utc>) -> Self {
        Self {
            trusted_utc_anchor,
            monotonic_anchor: tokio::time::Instant::now(),
        }
    }

    fn trusted_now(&self) -> Result<DateTime<Utc>, Stage8bP1eStartupErrorV1> {
        let elapsed = tokio::time::Instant::now().saturating_duration_since(self.monotonic_anchor);
        let elapsed = chrono::Duration::from_std(elapsed)
            .map_err(|_| Stage8bP1RedisSemanticError::P1eScheduleClockMismatch)?;
        self.trusted_utc_anchor
            .checked_add_signed(elapsed)
            .ok_or_else(|| Stage8bP1RedisSemanticError::P1eScheduleClockMismatch.into())
    }
}

fn restore_stage8b_p1e_owner_loop_high_water_v1(
    ready: &Stage8bP1eReadyPollingV1,
    _reader: &crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
) -> Result<(), Stage8bP1eStartupErrorV1> {
    #[cfg(all(test, feature = "stage8a4-i3-test-fixtures"))]
    if let Some((public_key_hex, key_valid_from, key_valid_until)) = _reader.test_fixture_trust() {
        return ready.test_restore_durable_schedule_high_water_with_key(
            context,
            &public_key_hex,
            key_valid_from,
            key_valid_until,
        );
    }
    ready.restore_durable_schedule_high_water(context)
}

/// Consumes the sole S05/S06 startup owner and joins every already-composed
/// route to the long-lived S08/S09 task. Startup routes whose signed authority
/// bridge is not yet composed remain typed terminal owners; they are never
/// coerced into a supported schedule path.
pub async fn run_stage8b_p1e_startup_owner_loop_v1(
    startup: Stage8bP1eStartupOwnerV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eOwnerLoopOutcomeV1, Stage8bP1eStartupErrorV1> {
    let entry = match latch_stage8b_p1e_startup_owner_v1(startup, latch) {
        Stage8bP1eStartupLatchDecisionV1::RetainedSource(retained) => {
            return Ok(Stage8bP1eOwnerLoopOutcomeV1::RetainedSource(retained));
        }
        Stage8bP1eStartupLatchDecisionV1::ContinueSource(continuing) => {
            match drain_stage8b_p1e_schedule_free_recovery_v1(continuing, latch, commitment_key)
                .await?
            {
                Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(ready) => {
                    if ready.is_committed_cancel_resolution() {
                        restore_stage8b_p1e_owner_loop_high_water_v1(&ready, reader, context)?;
                        return Ok(Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelResolved(
                            ready.into_committed_cancel_resolved()?,
                        ));
                    }
                    Stage8bP1eOwnerLoopEntryV1::Ready(ready)
                }
                Stage8bP1eScheduleFreeDrainOutcomeV1::RetainedForRestart(retained) => {
                    return Ok(Stage8bP1eOwnerLoopOutcomeV1::RetainedRecovery(retained));
                }
                Stage8bP1eScheduleFreeDrainOutcomeV1::PendingNotClaimable(pending) => {
                    return Ok(Stage8bP1eOwnerLoopOutcomeV1::PendingNotClaimable(pending));
                }
                Stage8bP1eScheduleFreeDrainOutcomeV1::Blocked(blocked) => {
                    return Ok(Stage8bP1eOwnerLoopOutcomeV1::Blocked(blocked));
                }
                Stage8bP1eScheduleFreeDrainOutcomeV1::ScheduleDeferred(deferred) => {
                    Stage8bP1eOwnerLoopEntryV1::ScheduleDeferred(deferred)
                }
            }
        }
        Stage8bP1eStartupLatchDecisionV1::ReadyIdle(ready) => {
            Stage8bP1eOwnerLoopEntryV1::Ready(ready.into_ready_polling())
        }
        Stage8bP1eStartupLatchDecisionV1::ReadyPendingNotClaimable(pending) => {
            return Ok(Stage8bP1eOwnerLoopOutcomeV1::StartupPendingNotClaimable(
                pending,
            ));
        }
        Stage8bP1eStartupLatchDecisionV1::P1d3LimitDispatchAwaitingSchedule(pending) => {
            return Ok(Stage8bP1eOwnerLoopOutcomeV1::StartupLimitSchedule(pending));
        }
        Stage8bP1eStartupLatchDecisionV1::ScheduleBindingCommitted(pending) => {
            let Stage8bP1eCommittedScheduleStartupV1 {
                durable,
                transport,
                mut control,
            } = pending;
            if durable.is_generated_market() {
                match resume_stage8b_p1e_committed_generated_market_with_redis(
                    durable,
                    transport,
                    latch,
                    commitment_key,
                )
                .await
                {
                    Ok(Stage8bP1eRecoveredGeneratedMarketScheduleOutcomeV1::Stopped(
                        receipt,
                    )) => {
                        return Ok(Stage8bP1eOwnerLoopOutcomeV1::StoppedSchedule(
                            Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
                        ));
                    }
                    Ok(
                        Stage8bP1eRecoveredGeneratedMarketScheduleOutcomeV1::GeneratedMarketAckCommitted {
                            owner,
                            high_water,
                        },
                    ) => {
                        apply_recovered_schedule_high_water(context, Some(high_water))?;
                        match drain_stage8b_p1e_recovery_lifecycle_v1(
                            Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                                Stage8bP1eRecoveryStepV1 {
                                    route: Box::new(
                                        Stage8bP1eRecoveryStepRouteV1::GeneratedMarketAckCommitted(
                                            owner,
                                        ),
                                    ),
                                    control,
                                },
                            )),
                            latch,
                            commitment_key,
                        )
                        .await?
                        {
                            Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(ready) => {
                                Stage8bP1eOwnerLoopEntryV1::Ready(ready)
                            }
                            Stage8bP1eScheduleFreeDrainOutcomeV1::RetainedForRestart(retained) => {
                                return Ok(Stage8bP1eOwnerLoopOutcomeV1::RetainedRecovery(retained));
                            }
                            Stage8bP1eScheduleFreeDrainOutcomeV1::PendingNotClaimable(pending) => {
                                return Ok(Stage8bP1eOwnerLoopOutcomeV1::PendingNotClaimable(
                                    pending,
                                ));
                            }
                            Stage8bP1eScheduleFreeDrainOutcomeV1::Blocked(blocked) => {
                                return Ok(Stage8bP1eOwnerLoopOutcomeV1::Blocked(blocked));
                            }
                            Stage8bP1eScheduleFreeDrainOutcomeV1::ScheduleDeferred(deferred) => {
                                Stage8bP1eOwnerLoopEntryV1::ScheduleDeferred(deferred)
                            }
                        }
                    }
                    Err(error) => return Err(error.into()),
                }
            } else if durable.is_cancel() {
                match resume_stage8b_p1e_committed_cancel_with_redis(
                    durable,
                    transport,
                    latch,
                    commitment_key,
                )
                .await
                {
                    Ok(Stage8bP1eRecoveredCancelScheduleOutcomeV1::Stopped(receipt)) => {
                        return Ok(Stage8bP1eOwnerLoopOutcomeV1::StoppedSchedule(
                            Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
                        ));
                    }
                    Ok(Stage8bP1eRecoveredCancelScheduleOutcomeV1::CancelCommitted {
                        outcome,
                        high_water,
                    }) => {
                        apply_recovered_schedule_high_water(context, Some(high_water))?;
                        let route = match *outcome {
                            Stage8bP1RedisCancelCommitOutcome::AckCommitted(owner) => {
                                Stage8bP1eRecoveryStepRouteV1::LimitAckCommitted(Box::new(owner))
                            }
                            Stage8bP1RedisCancelCommitOutcome::TruthCommitted(owner) => {
                                Stage8bP1eRecoveryStepRouteV1::LimitTruthCommitted(Box::new(owner))
                            }
                            Stage8bP1RedisCancelCommitOutcome::CancelContinuationPending(owner) => {
                                debug_assert!(!owner.m10_xack_allowed());
                                drop(owner);
                                return Ok(
                                    Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelRestartRequired(
                                        Stage8bP1eCommittedCancelRestartRequiredV1 { control },
                                    ),
                                );
                            }
                        };
                        match drain_stage8b_p1e_recovery_lifecycle_v1(
                            Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                                Stage8bP1eRecoveryStepV1 {
                                    route: Box::new(route),
                                    control,
                                },
                            )),
                            latch,
                            commitment_key,
                        )
                        .await?
                        {
                            Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(ready) => {
                                if !ready.is_committed_cancel_resolution() {
                                    return Err(Stage8bP1eStartupErrorV1::ReadyBoundaryInvariant);
                                }
                                return Ok(Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelResolved(
                                    ready.into_committed_cancel_resolved()?,
                                ));
                            }
                            Stage8bP1eScheduleFreeDrainOutcomeV1::RetainedForRestart(retained) => {
                                return Ok(Stage8bP1eOwnerLoopOutcomeV1::RetainedRecovery(
                                    retained,
                                ));
                            }
                            Stage8bP1eScheduleFreeDrainOutcomeV1::PendingNotClaimable(pending) => {
                                return Ok(Stage8bP1eOwnerLoopOutcomeV1::PendingNotClaimable(
                                    pending,
                                ));
                            }
                            Stage8bP1eScheduleFreeDrainOutcomeV1::Blocked(blocked) => {
                                return Ok(Stage8bP1eOwnerLoopOutcomeV1::Blocked(blocked));
                            }
                            Stage8bP1eScheduleFreeDrainOutcomeV1::ScheduleDeferred(deferred) => {
                                Stage8bP1eOwnerLoopEntryV1::ScheduleDeferred(deferred)
                            }
                        }
                    }
                    Err(error) => return Err(error.into()),
                }
            } else if durable.is_day_expiry() {
                // A committed Day-expiry transition is source-free. The
                // production composition, rather than the low-level durable
                // continuation, owns the mandatory proof that no M10 delivery
                // can be acknowledged by this route.
                if control.pel_count().await? != 0 {
                    return Err(Stage8bP1eStartupErrorV1::CommittedDayExpiryPelNotEmpty);
                }
                match resume_stage8b_p1e_committed_day_expiry(
                    durable,
                    transport,
                    latch,
                    commitment_key,
                ) {
                    Ok(Stage8bP1eRecoveredDayExpiryScheduleOutcomeV1::Stopped(receipt)) => {
                        return Ok(Stage8bP1eOwnerLoopOutcomeV1::StoppedSchedule(
                            Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
                        ));
                    }
                    Ok(Stage8bP1eRecoveredDayExpiryScheduleOutcomeV1::Ready {
                        owner,
                        high_water,
                    }) => {
                        apply_recovered_schedule_high_water(context, Some(high_water))?;
                        return Ok(Stage8bP1eOwnerLoopOutcomeV1::CommittedDayExpiryResolved(
                            Stage8bP1eCommittedDayExpiryResolvedV1 {
                                _owner: owner,
                                control,
                            },
                        ));
                    }
                    Err(error) => return Err(error.into()),
                }
            } else if !durable.is_initial_limit() {
                return Ok(Stage8bP1eOwnerLoopOutcomeV1::StartupCommittedSchedule(
                    Stage8bP1eCommittedScheduleStartupV1 {
                        durable,
                        transport,
                        control,
                    },
                ));
            } else {
                match resume_stage8b_p1e_committed_initial_limit_with_redis(
                    durable,
                    transport,
                    latch,
                    commitment_key,
                )
                .await
                {
                    Ok(Stage8bP1eRecoveredInitialLimitScheduleOutcomeV1::Stopped(receipt)) => {
                        return Ok(Stage8bP1eOwnerLoopOutcomeV1::StoppedSchedule(
                            Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
                        ));
                    }
                    Ok(Stage8bP1eRecoveredInitialLimitScheduleOutcomeV1::LimitAckCommitted {
                        owner,
                        high_water,
                    }) => {
                        apply_recovered_schedule_high_water(context, Some(high_water))?;
                        match drain_stage8b_p1e_recovery_lifecycle_v1(
                            Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                                Stage8bP1eRecoveryStepV1 {
                                    route: Box::new(
                                        Stage8bP1eRecoveryStepRouteV1::LimitAckCommitted(owner),
                                    ),
                                    control,
                                },
                            )),
                            latch,
                            commitment_key,
                        )
                        .await?
                        {
                            Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(ready) => {
                                Stage8bP1eOwnerLoopEntryV1::Ready(ready)
                            }
                            Stage8bP1eScheduleFreeDrainOutcomeV1::RetainedForRestart(retained) => {
                                return Ok(Stage8bP1eOwnerLoopOutcomeV1::RetainedRecovery(
                                    retained,
                                ));
                            }
                            Stage8bP1eScheduleFreeDrainOutcomeV1::PendingNotClaimable(pending) => {
                                return Ok(Stage8bP1eOwnerLoopOutcomeV1::PendingNotClaimable(
                                    pending,
                                ));
                            }
                            Stage8bP1eScheduleFreeDrainOutcomeV1::Blocked(blocked) => {
                                return Ok(Stage8bP1eOwnerLoopOutcomeV1::Blocked(blocked));
                            }
                            Stage8bP1eScheduleFreeDrainOutcomeV1::ScheduleDeferred(deferred) => {
                                Stage8bP1eOwnerLoopEntryV1::ScheduleDeferred(deferred)
                            }
                        }
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
    };
    run_stage8b_p1e_owner_loop_from_entry_v1(entry, reader, context, latch, commitment_key).await
}

/// Retains the sole polling/lifecycle owner across the completed schedule-free
/// and supported signed-schedule paths. A successful signed transition rejoins
/// exact Ready in-process; no quiescent Ready owner is returned to a caller
/// that could accidentally drop it between polls.
pub async fn run_stage8b_p1e_owner_loop_v1(
    ready: Stage8bP1eReadyPollingV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eOwnerLoopOutcomeV1, Stage8bP1eStartupErrorV1> {
    run_stage8b_p1e_owner_loop_from_entry_v1(
        Stage8bP1eOwnerLoopEntryV1::Ready(ready),
        reader,
        context,
        latch,
        commitment_key,
    )
    .await
}

async fn run_stage8b_p1e_owner_loop_from_entry_v1(
    mut entry: Stage8bP1eOwnerLoopEntryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &mut strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eOwnerLoopOutcomeV1, Stage8bP1eStartupErrorV1> {
    if let Stage8bP1eOwnerLoopEntryV1::Ready(ready) = &entry {
        restore_stage8b_p1e_owner_loop_high_water_v1(ready, reader, context)?;
    }
    let clock = Stage8bP1eOwnerLoopClockV1::new(context.trusted_now);
    loop {
        let mut deferred = match entry {
            Stage8bP1eOwnerLoopEntryV1::Ready(ready) => {
                match run_stage8b_p1e_schedule_free_owner_loop_v1(ready, latch, commitment_key)
                    .await?
                {
                    Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::StoppedReady(stopped) => {
                        return Ok(Stage8bP1eOwnerLoopOutcomeV1::StoppedReady(stopped));
                    }
                    Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::CommittedCancelResolved(resolved) => {
                        return Ok(Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelResolved(
                            resolved,
                        ));
                    }
                    Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::RetainedSource(retained) => {
                        return Ok(Stage8bP1eOwnerLoopOutcomeV1::RetainedSource(retained));
                    }
                    Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::RetainedRecovery(retained) => {
                        return Ok(Stage8bP1eOwnerLoopOutcomeV1::RetainedRecovery(retained));
                    }
                    Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::PendingNotClaimable(pending) => {
                        return Ok(Stage8bP1eOwnerLoopOutcomeV1::PendingNotClaimable(pending));
                    }
                    Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::Blocked(blocked) => {
                        return Ok(Stage8bP1eOwnerLoopOutcomeV1::Blocked(blocked));
                    }
                    Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::ScheduleDeferred(deferred) => {
                        deferred
                    }
                }
            }
            Stage8bP1eOwnerLoopEntryV1::ScheduleDeferred(deferred) => deferred,
        };

        loop {
            // Refresh the trusted schedule clock from one UTC/monotonic anchor
            // before every acquisition cycle. A long Ready polling interval
            // therefore cannot leave freshness checks pinned to process-start
            // time, and a later wall-clock adjustment cannot move time back.
            context.trusted_now = clock.trusted_now()?;
            match advance_stage8b_p1e_supported_schedule_bounded_v1(
                deferred,
                reader,
                context,
                latch,
                commitment_key,
            )
            .await?
            {
                Stage8bP1eBoundedScheduleCycleOutcomeV1::Ready(next) => {
                    entry = Stage8bP1eOwnerLoopEntryV1::Ready(next);
                    break;
                }
                Stage8bP1eBoundedScheduleCycleOutcomeV1::Stopped(stopped) => {
                    return Ok(Stage8bP1eOwnerLoopOutcomeV1::StoppedSchedule(stopped));
                }
                Stage8bP1eBoundedScheduleCycleOutcomeV1::Exhausted(exhausted) => {
                    return Ok(Stage8bP1eOwnerLoopOutcomeV1::ScheduleExhausted(exhausted));
                }
                Stage8bP1eBoundedScheduleCycleOutcomeV1::RetainedRecovery(retained) => {
                    return Ok(Stage8bP1eOwnerLoopOutcomeV1::RetainedRecovery(retained));
                }
                Stage8bP1eBoundedScheduleCycleOutcomeV1::PendingNotClaimable(pending) => {
                    return Ok(Stage8bP1eOwnerLoopOutcomeV1::PendingNotClaimable(pending));
                }
                Stage8bP1eBoundedScheduleCycleOutcomeV1::Blocked(blocked) => {
                    return Ok(Stage8bP1eOwnerLoopOutcomeV1::Blocked(blocked));
                }
                Stage8bP1eBoundedScheduleCycleOutcomeV1::ScheduleDeferred(next) => {
                    deferred = next;
                }
                Stage8bP1eBoundedScheduleCycleOutcomeV1::Unsupported(unsupported) => {
                    return Ok(Stage8bP1eOwnerLoopOutcomeV1::UnsupportedSchedule(
                        unsupported,
                    ));
                }
            }
        }
    }
}

/// Runs the schedule-free portion of the long-lived S09 owner task. Empty
/// bounded reads and terminal Ready recovery results stay inside this loop;
/// no caller can accidentally drop a quiescent owner between polls. The
/// shared first-wins latch can be requested concurrently by the signal or
/// supervision task while Redis is inside its bounded wait.
pub async fn run_stage8b_p1e_schedule_free_owner_loop_v1(
    mut ready: Stage8bP1eReadyPollingV1,
    latch: &Stage8bP1eShutdownLatchV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleFreeOwnerLoopOutcomeV1, Stage8bP1eStartupErrorV1> {
    loop {
        if ready.is_committed_cancel_resolution() {
            return Ok(
                Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::CommittedCancelResolved(
                    ready.into_committed_cancel_resolved()?,
                ),
            );
        }
        match poll_stage8b_p1e_ready_once_v1(ready, latch).await? {
            Stage8bP1eReadyPollOutcomeV1::Empty(next) => ready = next,
            Stage8bP1eReadyPollOutcomeV1::Stopped(stopped) => {
                return Ok(Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::StoppedReady(
                    stopped,
                ));
            }
            Stage8bP1eReadyPollOutcomeV1::RetainedSource(retained) => {
                return Ok(Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::RetainedSource(
                    retained,
                ));
            }
            Stage8bP1eReadyPollOutcomeV1::ContinueSource(continuing) => {
                match drain_stage8b_p1e_schedule_free_recovery_v1(continuing, latch, commitment_key)
                    .await?
                {
                    Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(next) => ready = next,
                    Stage8bP1eScheduleFreeDrainOutcomeV1::RetainedForRestart(retained) => {
                        return Ok(Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::RetainedRecovery(
                            retained,
                        ));
                    }
                    Stage8bP1eScheduleFreeDrainOutcomeV1::PendingNotClaimable(pending) => {
                        return Ok(
                            Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::PendingNotClaimable(pending),
                        );
                    }
                    Stage8bP1eScheduleFreeDrainOutcomeV1::Blocked(blocked) => {
                        return Ok(Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::Blocked(blocked));
                    }
                    Stage8bP1eScheduleFreeDrainOutcomeV1::ScheduleDeferred(deferred) => {
                        return Ok(Stage8bP1eScheduleFreeOwnerLoopOutcomeV1::ScheduleDeferred(
                            deferred,
                        ));
                    }
                }
            }
        }
    }
}

/// Drains one acquired source through every schedule-free authenticated row
/// in the same owner invocation. Each durable row is followed by a mandatory
/// latch recheck. A schedule-dependent route is returned with its exact owner
/// untouched; it is never guessed or bypassed here.
pub async fn drain_stage8b_p1e_schedule_free_recovery_v1(
    startup: Stage8bP1eContinuingStartupV1,
    latch: &Stage8bP1eShutdownLatchV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleFreeDrainOutcomeV1, Stage8bP1eStartupErrorV1> {
    let step = continue_stage8b_p1e_recovery_once_v1(startup, commitment_key).await?;
    // `continue_stage8b_p1e_recovery_once_v1` above consumed row one, so the
    // shared lifecycle drain may consume at most seven additional rows.
    drain_stage8b_p1e_recovery_lifecycle_with_budget_v1(
        Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(step)),
        latch,
        commitment_key,
        STAGE8B_P1E_SCHEDULE_FREE_RECOVERY_MAX_ROWS - 1,
    )
    .await
}

/// Drains a lifecycle result returned by a completed signed-schedule effect
/// back to an exact Ready polling owner or another typed terminal boundary.
/// This is the only schedule-to-S09 re-entry adapter: every `Continue` row
/// must pass the shared shutdown latch before it can advance, and no caller
/// receives an intermediate raw lifecycle owner.
pub async fn drain_stage8b_p1e_recovery_lifecycle_v1(
    outcome: Stage8bP1eRecoveryAdvanceOutcomeV1,
    latch: &Stage8bP1eShutdownLatchV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleFreeDrainOutcomeV1, Stage8bP1eStartupErrorV1> {
    drain_stage8b_p1e_recovery_lifecycle_with_budget_v1(
        outcome,
        latch,
        commitment_key,
        STAGE8B_P1E_SCHEDULE_FREE_RECOVERY_MAX_ROWS,
    )
    .await
}

async fn drain_stage8b_p1e_recovery_lifecycle_with_budget_v1(
    mut outcome: Stage8bP1eRecoveryAdvanceOutcomeV1,
    latch: &Stage8bP1eShutdownLatchV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    max_additional_rows: usize,
) -> Result<Stage8bP1eScheduleFreeDrainOutcomeV1, Stage8bP1eStartupErrorV1> {
    for consumed_rows in 0..=max_additional_rows {
        outcome = match outcome {
            Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(step) => {
                if consumed_rows == max_additional_rows {
                    return Err(Stage8bP1eStartupErrorV1::RecoveryStepBudgetExceeded);
                }
                let permit = match recheck_stage8b_p1e_recovery_step_latch_v1(*step, latch) {
                    Stage8bP1eRecoveryLatchDecisionV1::RetainForRestart(retained) => {
                        return Ok(Stage8bP1eScheduleFreeDrainOutcomeV1::RetainedForRestart(
                            retained,
                        ));
                    }
                    Stage8bP1eRecoveryLatchDecisionV1::Continue(permit) => permit,
                };
                advance_stage8b_p1e_recovery_once_v1(permit, commitment_key).await?
            }
            Stage8bP1eRecoveryAdvanceOutcomeV1::Ready(ready) => {
                return Ok(Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(
                    ready.into_ready_polling()?,
                ));
            }
            Stage8bP1eRecoveryAdvanceOutcomeV1::PendingNotClaimable(pending) => {
                return Ok(Stage8bP1eScheduleFreeDrainOutcomeV1::PendingNotClaimable(
                    *pending,
                ));
            }
            Stage8bP1eRecoveryAdvanceOutcomeV1::Blocked(blocked) => {
                return Ok(Stage8bP1eScheduleFreeDrainOutcomeV1::Blocked(*blocked));
            }
            Stage8bP1eRecoveryAdvanceOutcomeV1::ScheduleDeferred(deferred) => {
                return Ok(Stage8bP1eScheduleFreeDrainOutcomeV1::ScheduleDeferred(
                    *deferred,
                ));
            }
        };
    }
    unreachable!("bounded lifecycle drain always returns at its terminal budget")
}

/// Rechecks the first-wins shutdown latch after exactly one accepted recovery
/// row. A set latch destroys the in-memory continuation. A clear latch issues
/// one opaque permit for exactly one further row.
pub fn recheck_stage8b_p1e_recovery_step_latch_v1(
    step: Stage8bP1eRecoveryStepV1,
    latch: &Stage8bP1eShutdownLatchV1,
) -> Stage8bP1eRecoveryLatchDecisionV1 {
    if let Some(shutdown_intent) = latch.intent().cloned() {
        let Stage8bP1eRecoveryStepV1 { route, control } = step;
        let kind = route.kind();
        drop(route);
        Stage8bP1eRecoveryLatchDecisionV1::RetainForRestart(Stage8bP1eRetainedRecoveryBoundaryV1 {
            kind,
            shutdown_intent,
            control,
        })
    } else {
        Stage8bP1eRecoveryLatchDecisionV1::Continue(Stage8bP1eRecoveryAdvancePermitV1 { step })
    }
}

fn classify_recovered_semantic_outcome(
    outcome: Stage8bP1RedisSemanticOutcome,
    control: Stage8bP1eRedisControlV1,
) -> Stage8bP1eRecoveryAdvanceOutcomeV1 {
    match outcome {
        ready @ Stage8bP1RedisSemanticOutcome::Ready { .. } => {
            Stage8bP1eRecoveryAdvanceOutcomeV1::Ready(Box::new(Stage8bP1eRecoveredReadyV1 {
                kind: Stage8bP1eRecoveredReadyKindV1::Semantic,
                _route: Box::new(Stage8bP1eRecoveredReadyRouteV1::Semantic(ready)),
                control,
            }))
        }
        Stage8bP1RedisSemanticOutcome::Prepublication(pending) => {
            Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(Stage8bP1eRecoveryStepV1 {
                route: Box::new(Stage8bP1eRecoveryStepRouteV1::Prepublication(pending)),
                control,
            }))
        }
        Stage8bP1RedisSemanticOutcome::PendingNotClaimable {
            owner,
            pending_m10_redis_id,
        } => Stage8bP1eRecoveryAdvanceOutcomeV1::PendingNotClaimable(Box::new(
            Stage8bP1eRecoveredPendingNotClaimableV1 {
                _owner: owner,
                pending_m10_redis_id,
                control,
            },
        )),
        Stage8bP1RedisSemanticOutcome::MultiIntentBlocked {
            semantic_batch_id_sha256,
            intent_count,
        } => Stage8bP1eRecoveryAdvanceOutcomeV1::Blocked(Box::new(Stage8bP1eRecoveredBlockedV1 {
            semantic_batch_id_sha256,
            intent_count,
            control,
        })),
    }
}

/// Advances one clear-latch S06R row. ACK boundaries can only create their
/// exact replacement truth; truth boundaries can only resolve the exact
/// source. The returned continuation must pass through another latch recheck
/// before any later row, schedule lookup or fresh source read.
pub async fn advance_stage8b_p1e_recovery_once_v1(
    permit: Stage8bP1eRecoveryAdvancePermitV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eRecoveryAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    let Stage8bP1eRecoveryAdvancePermitV1 { step } = permit;
    let Stage8bP1eRecoveryStepV1 { route, control } = step;
    match *route {
        Stage8bP1eRecoveryStepRouteV1::Semantic(outcome) => {
            Ok(classify_recovered_semantic_outcome(*outcome, control))
        }
        Stage8bP1eRecoveryStepRouteV1::ZeroIntentResolved(resolved) => Ok(
            Stage8bP1eRecoveryAdvanceOutcomeV1::Ready(Box::new(Stage8bP1eRecoveredReadyV1 {
                kind: Stage8bP1eRecoveredReadyKindV1::ZeroIntent,
                _route: Box::new(Stage8bP1eRecoveredReadyRouteV1::ZeroIntent(*resolved)),
                control,
            })),
        ),
        Stage8bP1eRecoveryStepRouteV1::Prepublication(pending) => {
            let published = if pending.requires_generated_market_reservation() {
                pending
                    .publish_exact_generated_market_command(commitment_key)
                    .await?
            } else {
                pending.publish_exact_command().await?
            };
            Ok(Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                Stage8bP1eRecoveryStepV1 {
                    route: Box::new(Stage8bP1eRecoveryStepRouteV1::CommandPublished(Box::new(
                        published,
                    ))),
                    control,
                },
            )))
        }
        Stage8bP1eRecoveryStepRouteV1::CommandPublished(published) => {
            let kind = match published.p1e_schedule_route() {
                Stage8bP1ePublishedScheduleRouteV1::PlainMarket => {
                    Stage8bP1eScheduleDeferredKindV1::CommandPublishedMarket
                }
                Stage8bP1ePublishedScheduleRouteV1::GeneratedMarket => {
                    Stage8bP1eScheduleDeferredKindV1::CommandPublishedGeneratedMarket
                }
                Stage8bP1ePublishedScheduleRouteV1::InitialLimit => {
                    Stage8bP1eScheduleDeferredKindV1::CommandPublishedInitialLimit
                }
                Stage8bP1ePublishedScheduleRouteV1::Cancel => {
                    Stage8bP1eScheduleDeferredKindV1::CommandPublishedCancel
                }
                Stage8bP1ePublishedScheduleRouteV1::Unsupported => {
                    Stage8bP1eScheduleDeferredKindV1::CommandPublishedUnsupported
                }
            };
            Ok(Stage8bP1eRecoveryAdvanceOutcomeV1::ScheduleDeferred(
                Box::new(Stage8bP1eScheduleDeferredRecoveryV1 {
                    kind,
                    _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                        published,
                    )),
                    control,
                }),
            ))
        }
        Stage8bP1eRecoveryStepRouteV1::GeneratedMarketAckCommitted(ack) => {
            let truth = ack.commit_truth(commitment_key).await?;
            Ok(Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                Stage8bP1eRecoveryStepV1 {
                    route: Box::new(
                        Stage8bP1eRecoveryStepRouteV1::GeneratedMarketTruthCommitted(Box::new(
                            truth,
                        )),
                    ),
                    control,
                },
            )))
        }
        Stage8bP1eRecoveryStepRouteV1::GeneratedMarketTruthCommitted(truth) => {
            let resolved = truth.acknowledge_source().await?;
            Ok(Stage8bP1eRecoveryAdvanceOutcomeV1::Ready(Box::new(
                Stage8bP1eRecoveredReadyV1 {
                    kind: Stage8bP1eRecoveredReadyKindV1::Feedback,
                    _route: Box::new(Stage8bP1eRecoveredReadyRouteV1::Feedback(resolved)),
                    control,
                },
            )))
        }
        Stage8bP1eRecoveryStepRouteV1::FeedbackAckCommitted(ack) => {
            let truth = ack.commit_truth(commitment_key)?;
            Ok(Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                Stage8bP1eRecoveryStepV1 {
                    route: Box::new(Stage8bP1eRecoveryStepRouteV1::FeedbackTruthCommitted(
                        Box::new(truth),
                    )),
                    control,
                },
            )))
        }
        Stage8bP1eRecoveryStepRouteV1::FeedbackTruthCommitted(truth) => {
            let resolved = truth.acknowledge_source().await?;
            Ok(Stage8bP1eRecoveryAdvanceOutcomeV1::Ready(Box::new(
                Stage8bP1eRecoveredReadyV1 {
                    kind: Stage8bP1eRecoveredReadyKindV1::Feedback,
                    _route: Box::new(Stage8bP1eRecoveredReadyRouteV1::Feedback(resolved)),
                    control,
                },
            )))
        }
        Stage8bP1eRecoveryStepRouteV1::FeedbackResolved(resolved) => Ok(
            Stage8bP1eRecoveryAdvanceOutcomeV1::Ready(Box::new(Stage8bP1eRecoveredReadyV1 {
                kind: Stage8bP1eRecoveredReadyKindV1::Feedback,
                _route: Box::new(Stage8bP1eRecoveredReadyRouteV1::Feedback(*resolved)),
                control,
            })),
        ),
        Stage8bP1eRecoveryStepRouteV1::LimitPreAckRecovered(recovered) => match *recovered {
            Stage8bP1RedisPreAckRecoveryOutcome::AckCommitted(ack) => Ok(
                Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(Stage8bP1eRecoveryStepV1 {
                    route: Box::new(Stage8bP1eRecoveryStepRouteV1::LimitAckCommitted(Box::new(
                        ack,
                    ))),
                    control,
                })),
            ),
            Stage8bP1RedisPreAckRecoveryOutcome::TruthCommitted(truth) => Ok(
                Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(Stage8bP1eRecoveryStepV1 {
                    route: Box::new(Stage8bP1eRecoveryStepRouteV1::LimitTruthCommitted(
                        Box::new(truth),
                    )),
                    control,
                })),
            ),
            Stage8bP1RedisPreAckRecoveryOutcome::Semantic(outcome) => {
                Ok(classify_recovered_semantic_outcome(outcome, control))
            }
        },
        Stage8bP1eRecoveryStepRouteV1::CancelContinuationPending(pending) => {
            let truth = pending.commit_recovered_cancel(commitment_key)?;
            Ok(Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                Stage8bP1eRecoveryStepV1 {
                    route: Box::new(Stage8bP1eRecoveryStepRouteV1::LimitTruthCommitted(
                        Box::new(truth),
                    )),
                    control,
                },
            )))
        }
        Stage8bP1eRecoveryStepRouteV1::LimitAckCommitted(ack) => {
            let truth = ack.commit_truth(commitment_key)?;
            Ok(Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                Stage8bP1eRecoveryStepV1 {
                    route: Box::new(Stage8bP1eRecoveryStepRouteV1::LimitTruthCommitted(
                        Box::new(truth),
                    )),
                    control,
                },
            )))
        }
        Stage8bP1eRecoveryStepRouteV1::LimitTruthCommitted(truth) => {
            let resolved = truth.acknowledge_source().await?;
            Ok(Stage8bP1eRecoveryAdvanceOutcomeV1::Ready(Box::new(
                Stage8bP1eRecoveredReadyV1 {
                    kind: Stage8bP1eRecoveredReadyKindV1::Limit,
                    _route: Box::new(Stage8bP1eRecoveredReadyRouteV1::Limit(resolved)),
                    control,
                },
            )))
        }
        Stage8bP1eRecoveryStepRouteV1::LimitResolved(resolved) => Ok(
            Stage8bP1eRecoveryAdvanceOutcomeV1::Ready(Box::new(Stage8bP1eRecoveredReadyV1 {
                kind: Stage8bP1eRecoveredReadyKindV1::Limit,
                _route: Box::new(Stage8bP1eRecoveredReadyRouteV1::Limit(*resolved)),
                control,
            })),
        ),
        Stage8bP1eRecoveryStepRouteV1::ScheduleDeferred(route) => {
            Ok(Stage8bP1eRecoveryAdvanceOutcomeV1::ScheduleDeferred(
                Box::new(Stage8bP1eScheduleDeferredRecoveryV1 {
                    kind: Stage8bP1eScheduleDeferredKindV1::RoutedContinuation,
                    _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::RoutedContinuation(route)),
                    control,
                }),
            ))
        }
    }
}

impl Stage8bP1eCommittedScheduleStartupV1 {
    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }
}

enum Stage8bP1eStartupOwnerRouteV1 {
    ReadyPending(Stage8bP1eReadyPendingAcquisitionOutcomeV1),
    RecoveredSource(Stage8bP1ePostAcquisitionOwnerV1),
    P1d3LimitDispatch {
        durable: Box<crate::Stage8bP1d3DispatchPendingOwner>,
        transport: Stage8bP1RedisSemanticCompositionTransport,
    },
    ScheduleBindingCommitted {
        durable: Box<crate::Stage8bP1eScheduleBindingCommittedOwner>,
        transport: Stage8bP1RedisSemanticCompositionTransport,
    },
}

impl Stage8bP1eStartupOwnerV1 {
    pub const fn kind(&self) -> Stage8bP1eStartupOwnerKindV1 {
        self.kind
    }

    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
    }

    pub fn into_ready_pending(
        self,
    ) -> Result<
        (
            Stage8bP1eReadyPendingAcquisitionOutcomeV1,
            Stage8bP1eRedisControlV1,
        ),
        Box<Self>,
    > {
        let Self {
            kind,
            route,
            control,
        } = self;
        match *route {
            Stage8bP1eStartupOwnerRouteV1::ReadyPending(outcome) => Ok((outcome, control)),
            route => Err(Box::new(Self {
                kind,
                route: Box::new(route),
                control,
            })),
        }
    }

    pub fn into_recovered_source(
        self,
    ) -> Result<(Stage8bP1ePostAcquisitionOwnerV1, Stage8bP1eRedisControlV1), Box<Self>> {
        let Self {
            kind,
            route,
            control,
        } = self;
        match *route {
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(owner) => Ok((owner, control)),
            route => Err(Box::new(Self {
                kind,
                route: Box::new(route),
                control,
            })),
        }
    }

    pub fn into_p1d3_limit_dispatch(
        self,
    ) -> Result<
        (
            Box<crate::Stage8bP1d3DispatchPendingOwner>,
            Stage8bP1RedisSemanticCompositionTransport,
            Stage8bP1eRedisControlV1,
        ),
        Box<Self>,
    > {
        let Self {
            kind,
            route,
            control,
        } = self;
        match *route {
            Stage8bP1eStartupOwnerRouteV1::P1d3LimitDispatch { durable, transport } => {
                Ok((durable, transport, control))
            }
            route => Err(Box::new(Self {
                kind,
                route: Box::new(route),
                control,
            })),
        }
    }

    pub fn into_schedule_binding_committed(
        self,
    ) -> Result<
        (
            Box<crate::Stage8bP1eScheduleBindingCommittedOwner>,
            Stage8bP1RedisSemanticCompositionTransport,
            Stage8bP1eRedisControlV1,
        ),
        Box<Self>,
    > {
        let Self {
            kind,
            route,
            control,
        } = self;
        match *route {
            Stage8bP1eStartupOwnerRouteV1::ScheduleBindingCommitted { durable, transport } => {
                Ok((durable, transport, control))
            }
            route => Err(Box::new(Self {
                kind,
                route: Box::new(route),
                control,
            })),
        }
    }
}

/// Performs S06 pending-only acquisition after an already verified S05
/// session.  Non-Ready source routes reuse their accepted exact lookup/reclaim
/// wrappers; Ready alone uses the pending-only scan. LIMIT dispatch and a
/// committed schedule binding remain linearly deferred until the schedule
/// classifier selects their exact authority subtype.
pub async fn acquire_stage8b_p1e_startup_owner_v1(
    restart: Stage8bP1eAttachableRestartV1,
    session: Stage8bP1eVerifiedRedisSessionV1,
) -> Result<Stage8bP1eStartupOwnerV1, Stage8bP1eStartupErrorV1> {
    let (transport, mut control) = session.into_parts();
    // S06 observation is deliberately non-authorizing for non-Ready routes.
    // It proves that the group can be inspected before the accepted exact
    // resume wrapper becomes the sole reclaim/lookup owner below.
    let _observed_pel_count = control.pel_count().await?;
    let (kind, route) = match *restart.route {
        Stage8bP1eAttachableRestartRouteV1::Ready(owner) => {
            let owner = Stage8bP1RedisSemanticCompositionOwner::new(*owner, transport);
            let outcome = acquire_stage8b_p1e_ready_pending_with_redis(owner).await?;
            let kind = match &outcome {
                Stage8bP1eReadyPendingAcquisitionOutcomeV1::Acquired(_) => {
                    Stage8bP1eStartupOwnerKindV1::ReadySourceAcquired
                }
                Stage8bP1eReadyPendingAcquisitionOutcomeV1::NoPending(_) => {
                    Stage8bP1eStartupOwnerKindV1::ReadyNoPending
                }
                Stage8bP1eReadyPendingAcquisitionOutcomeV1::PendingNotClaimable { .. } => {
                    Stage8bP1eStartupOwnerKindV1::ReadyPendingNotClaimable
                }
            };
            (kind, Stage8bP1eStartupOwnerRouteV1::ReadyPending(outcome))
        }
        Stage8bP1eAttachableRestartRouteV1::P1SemanticPrepublicationPending(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1_journal_ahead_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1SemanticPrepublicationReady(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1_prepublication_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1SemanticZeroIntentAckPending(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1_zero_intent_ack_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d2PreAckPending(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d2_pre_ack_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d2AckCommitted(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d2_ack_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d2TruthCommitted(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d2_truth_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketPrepublicationPending(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d4_prepublication_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketDispatchPending(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d4_dispatch_pending_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketOrderPending(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d4_order_pending_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketPreFinalizationPending(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d4_pre_finalization_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketPreAckPending(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d4_pre_ack_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketAckCommitted(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d4_ack_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d4GeneratedMarketTruthCommitted(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d4_truth_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d3DispatchPending(owner) if owner.is_cancel() => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d3_dispatch_cancel_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d3DispatchPending(owner)
            if owner.is_limit_place() =>
        {
            (
                Stage8bP1eStartupOwnerKindV1::P1d3LimitDispatchAwaitingSchedule,
                Stage8bP1eStartupOwnerRouteV1::P1d3LimitDispatch {
                    durable: owner,
                    transport,
                },
            )
        }
        Stage8bP1eAttachableRestartRouteV1::P1d3DispatchPending(_) => {
            return Err(Stage8bP1RedisSemanticError::ExactSourceConflict.into());
        }
        Stage8bP1eAttachableRestartRouteV1::P1d3PreAckPending(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d3_pre_ack_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d3AckCommitted(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d3_ack_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d3TruthCommitted(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d3_truth_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d3CancelContinuationPending(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d3_cancel_continuation_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1d3SemanticPending(owner) => (
            Stage8bP1eStartupOwnerKindV1::RecoveredSourceAcquired,
            Stage8bP1eStartupOwnerRouteV1::RecoveredSource(
                acquire_stage8b_p1d3_semantic_with_redis(*owner, transport).await?,
            ),
        ),
        Stage8bP1eAttachableRestartRouteV1::P1eScheduleBindingCommitted(owner) => (
            Stage8bP1eStartupOwnerKindV1::ScheduleBindingCommitted,
            Stage8bP1eStartupOwnerRouteV1::ScheduleBindingCommitted {
                durable: owner,
                transport,
            },
        ),
    };
    Ok(Stage8bP1eStartupOwnerV1 {
        kind,
        route: Box::new(route),
        control,
    })
}

/// Applies the mandatory post-acquisition latch without losing the verified
/// control plane or exposing a second acquisition path. Ready/no-pending and
/// unclaimable states retain their exact semantic owner. Schedule-dependent
/// routes remain opaque until the signed schedule classifier supplies the
/// route-specific authority.
pub fn latch_stage8b_p1e_startup_owner_v1(
    startup: Stage8bP1eStartupOwnerV1,
    latch: &Stage8bP1eShutdownLatchV1,
) -> Stage8bP1eStartupLatchDecisionV1 {
    let Stage8bP1eStartupOwnerV1 {
        kind: _,
        route,
        control,
    } = startup;
    let acquired = match *route {
        Stage8bP1eStartupOwnerRouteV1::ReadyPending(
            Stage8bP1eReadyPendingAcquisitionOutcomeV1::Acquired(owner),
        )
        | Stage8bP1eStartupOwnerRouteV1::RecoveredSource(owner) => owner,
        Stage8bP1eStartupOwnerRouteV1::ReadyPending(
            Stage8bP1eReadyPendingAcquisitionOutcomeV1::NoPending(owner),
        ) => {
            return Stage8bP1eStartupLatchDecisionV1::ReadyIdle(Stage8bP1eReadyIdleStartupV1 {
                _owner: owner,
                control,
            });
        }
        Stage8bP1eStartupOwnerRouteV1::ReadyPending(
            Stage8bP1eReadyPendingAcquisitionOutcomeV1::PendingNotClaimable {
                owner,
                pending_m10_redis_id,
            },
        ) => {
            return Stage8bP1eStartupLatchDecisionV1::ReadyPendingNotClaimable(
                Stage8bP1ePendingNotClaimableStartupV1 {
                    _owner: owner,
                    pending_m10_redis_id,
                    control,
                },
            );
        }
        Stage8bP1eStartupOwnerRouteV1::P1d3LimitDispatch { durable, transport } => {
            return Stage8bP1eStartupLatchDecisionV1::P1d3LimitDispatchAwaitingSchedule(
                Stage8bP1eLimitScheduleStartupV1 {
                    _durable: durable,
                    _transport: transport,
                    control,
                },
            );
        }
        Stage8bP1eStartupOwnerRouteV1::ScheduleBindingCommitted { durable, transport } => {
            return Stage8bP1eStartupLatchDecisionV1::ScheduleBindingCommitted(
                Stage8bP1eCommittedScheduleStartupV1 {
                    durable,
                    transport,
                    control,
                },
            );
        }
    };
    match route_stage8b_p1e_post_acquisition_v1(acquired, latch) {
        Stage8bP1eRoutedPostAcquisitionDecisionV1::RetainForRestart(receipt) => {
            Stage8bP1eStartupLatchDecisionV1::RetainedSource(Stage8bP1eRetainedStartupV1 {
                receipt,
                _control: control,
            })
        }
        Stage8bP1eRoutedPostAcquisitionDecisionV1::Continue(route) => {
            Stage8bP1eStartupLatchDecisionV1::ContinueSource(Stage8bP1eContinuingStartupV1 {
                route,
                control,
            })
        }
    }
}

/// Executes exactly one accepted post-latch recovery continuation.  The match
/// is exhaustive over the 23 route-bound permits and contains no generic
/// fallback. Four schedule-dependent routes are retained unchanged rather
/// than receiving reconstructed or guessed schedule authority.
pub async fn continue_stage8b_p1e_recovery_once_v1(
    startup: Stage8bP1eContinuingStartupV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eRecoveryStepV1, Stage8bP1eStartupErrorV1> {
    let Stage8bP1eContinuingStartupV1 { route, control } = startup;
    let route = match route {
        Stage8bP1eRoutedContinuationV1::ReadySemantic(permit) => {
            Stage8bP1eRecoveryStepRouteV1::Semantic(Box::new(
                resume_stage8b_p1e_ready_source_with_redis(permit, commitment_key).await?,
            ))
        }
        route @ Stage8bP1eRoutedContinuationV1::ReadyWorkingLimit(_)
        | route @ Stage8bP1eRoutedContinuationV1::P1d3DispatchLimit(_)
        | route @ Stage8bP1eRoutedContinuationV1::P1d3DispatchExpiry(_)
        | route @ Stage8bP1eRoutedContinuationV1::P1d3DispatchCancel(_) => {
            Stage8bP1eRecoveryStepRouteV1::ScheduleDeferred(Box::new(route))
        }
        Stage8bP1eRoutedContinuationV1::ZeroIntentAck(permit) => {
            Stage8bP1eRecoveryStepRouteV1::ZeroIntentResolved(Box::new(
                resolve_stage8b_p1_zero_intent_ack_with_redis(permit).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::JournalAhead(permit) => {
            Stage8bP1eRecoveryStepRouteV1::Prepublication(Box::new(
                resume_stage8b_p1_journal_ahead_with_redis(permit, commitment_key).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::Prepublication(permit) => {
            Stage8bP1eRecoveryStepRouteV1::Prepublication(Box::new(
                resume_stage8b_p1_prepublication_with_redis(permit).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d4Prepublication(permit) => {
            Stage8bP1eRecoveryStepRouteV1::CommandPublished(Box::new(
                resume_stage8b_p1d4_prepublication_with_redis(permit).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d4DispatchPending(permit) => {
            Stage8bP1eRecoveryStepRouteV1::GeneratedMarketAckCommitted(Box::new(
                resume_stage8b_p1d4_dispatch_pending_with_redis(permit, commitment_key).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d4OrderPending(permit) => {
            Stage8bP1eRecoveryStepRouteV1::GeneratedMarketAckCommitted(Box::new(
                resume_stage8b_p1d4_order_pending_with_redis(permit, commitment_key).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d4PreFinalizationPending(permit) => {
            Stage8bP1eRecoveryStepRouteV1::GeneratedMarketAckCommitted(Box::new(
                resume_stage8b_p1d4_pre_finalization_with_redis(permit, commitment_key).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d4PreAckPending(permit) => {
            Stage8bP1eRecoveryStepRouteV1::GeneratedMarketAckCommitted(Box::new(
                resume_stage8b_p1d4_pre_ack_with_redis(permit, commitment_key).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d4Ack(permit) => {
            Stage8bP1eRecoveryStepRouteV1::GeneratedMarketAckCommitted(Box::new(
                resume_stage8b_p1d4_ack_with_redis(permit).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d4Truth(permit) => {
            Stage8bP1eRecoveryStepRouteV1::FeedbackResolved(Box::new(
                resume_stage8b_p1d4_truth_with_redis(permit).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d2Ack(permit) => {
            Stage8bP1eRecoveryStepRouteV1::FeedbackAckCommitted(Box::new(
                resume_stage8b_p1d2_ack_with_redis(permit).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d2PreAck(permit) => {
            Stage8bP1eRecoveryStepRouteV1::FeedbackAckCommitted(Box::new(
                resume_stage8b_p1d2_pre_ack_with_redis(permit, commitment_key).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d2Truth(permit) => {
            Stage8bP1eRecoveryStepRouteV1::FeedbackResolved(Box::new(
                resume_stage8b_p1d2_truth_with_redis(permit).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d3PreAck(permit) => {
            Stage8bP1eRecoveryStepRouteV1::LimitPreAckRecovered(Box::new(
                resume_stage8b_p1d3_pre_ack_with_redis(permit, commitment_key).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d3Ack(permit) => {
            Stage8bP1eRecoveryStepRouteV1::LimitAckCommitted(Box::new(
                resume_stage8b_p1d3_ack_with_redis(permit).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d3Truth(permit) => {
            Stage8bP1eRecoveryStepRouteV1::LimitResolved(Box::new(
                resume_stage8b_p1d3_truth_with_redis(permit).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d3CancelContinuation(permit) => {
            Stage8bP1eRecoveryStepRouteV1::LimitTruthCommitted(Box::new(
                resume_stage8b_p1d3_cancel_continuation_with_redis(permit, commitment_key).await?,
            ))
        }
        Stage8bP1eRoutedContinuationV1::P1d3Semantic(permit) => {
            Stage8bP1eRecoveryStepRouteV1::Semantic(Box::new(
                resume_stage8b_p1d3_semantic_with_redis(permit, commitment_key).await?,
            ))
        }
    };
    Ok(Stage8bP1eRecoveryStepV1 {
        route: Box::new(route),
        control,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1eProcessErrorV1 {
    #[error("invalid Stage 8B-P1-e command line")]
    Usage,
    #[error("supervisor config boundary is invalid")]
    ConfigBoundary,
    #[error("supervisor config is invalid")]
    Config,
    #[error("host boot identity is invalid")]
    BootIdentity,
    #[error("lifecycle credential is invalid")]
    Credential,
    #[error("first-boot source is invalid")]
    FirstBootSource,
    #[error("first-boot transaction failed")]
    FirstBootTransaction,
    #[error("first-boot recovery failed")]
    FirstBootRecovery,
    #[error("durable restart failed")]
    DurableRestart,
    #[error("durable restart is blocked before Redis")]
    RestartBlocked,
    #[error("Redis deployment identity, key type, or group inventory is invalid")]
    RedisDeployment,
    #[error("verify-only Redis attachment failed")]
    RedisAttach,
    #[error("signed schedule reader attachment failed")]
    ScheduleReader,
    #[error("owner loop failed or returned at a restart-only boundary")]
    OwnerLoop,
    #[error("owner task panicked or returned without an authenticated owner boundary")]
    OwnerTaskFailed,
    #[error("shutdown grace expired before an authenticated boundary")]
    ShutdownGraceExpired,
    #[error("process signal task failed")]
    SignalTaskFailed,
}

impl Stage8bP1eProcessErrorV1 {
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Usage
            | Self::ConfigBoundary
            | Self::Config
            | Self::BootIdentity
            | Self::Credential => 64,
            Self::FirstBootSource
            | Self::FirstBootTransaction
            | Self::FirstBootRecovery
            | Self::DurableRestart
            | Self::RestartBlocked
            | Self::RedisDeployment => 66,
            Self::RedisAttach | Self::ScheduleReader | Self::OwnerLoop => 67,
            Self::OwnerTaskFailed => 70,
            Self::ShutdownGraceExpired => 72,
            Self::SignalTaskFailed => 73,
        }
    }
}

/// Parses only the deployment-identity V2 argv grammar.  The config path is
/// fixed; arbitrary files and environment-selected paths are rejected.
pub fn parse_stage8b_p1e_process_command_v1<I, S>(
    args: I,
) -> Result<Stage8bP1eProcessCommandV1, Stage8bP1eProcessErrorV1>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let strings: Option<Vec<&str>> = args.iter().map(|value| value.to_str()).collect();
    let strings = strings.ok_or(Stage8bP1eProcessErrorV1::Usage)?;
    match strings.as_slice() {
        [mode, config]
            if *config == STAGE8B_P1E_SUPERVISOR_CONFIG_PATH && *mode == "validate-config" =>
        {
            Ok(Stage8bP1eProcessCommandV1::ValidateConfig)
        }
        [mode, config, confirmation]
            if *config == STAGE8B_P1E_SUPERVISOR_CONFIG_PATH
                && *mode == "bootstrap"
                && *confirmation == crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION =>
        {
            Ok(Stage8bP1eProcessCommandV1::Bootstrap)
        }
        [mode, config] if *config == STAGE8B_P1E_SUPERVISOR_CONFIG_PATH && *mode == "run" => {
            Ok(Stage8bP1eProcessCommandV1::Run)
        }
        [mode, config, selector, confirmation]
            if *config == STAGE8B_P1E_SUPERVISOR_CONFIG_PATH
                && *mode == "bootstrap-recover"
                && *confirmation == STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION =>
        {
            let (transaction_id_sha256, action) = parse_recovery_selector(selector)?;
            Ok(Stage8bP1eProcessCommandV1::BootstrapRecover {
                transaction_id_sha256,
                action,
            })
        }
        _ => Err(Stage8bP1eProcessErrorV1::Usage),
    }
}

pub async fn execute_stage8b_p1e_process_command_v1(
    command: Stage8bP1eProcessCommandV1,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    execute_with_boundaries(
        command,
        Path::new(STAGE8B_P1E_SUPERVISOR_CONFIG_PATH),
        Path::new(STAGE8B_P1E_BOOT_ID_PATH),
        Utc::now(),
        0,
    )
    .await
}

async fn execute_with_boundaries(
    command: Stage8bP1eProcessCommandV1,
    config_path: &Path,
    boot_id_path: &Path,
    trusted_now: DateTime<Utc>,
    expected_config_uid: u32,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    let supervisor = load_validated_supervisor(config_path, boot_id_path, expected_config_uid)?;
    match command {
        Stage8bP1eProcessCommandV1::ValidateConfig => Ok(Stage8bP1eProcessSuccessV1::ConfigValid),
        Stage8bP1eProcessCommandV1::Bootstrap => execute_bootstrap(supervisor, trusted_now),
        Stage8bP1eProcessCommandV1::BootstrapRecover {
            transaction_id_sha256,
            action,
        } => execute_bootstrap_recovery(supervisor, trusted_now, &transaction_id_sha256, action),
        Stage8bP1eProcessCommandV1::Run => execute_run(supervisor, trusted_now).await,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage8bP1eOwnerTaskBoundaryV1 {
    AuthenticatedStop,
    RestartRequired,
}

/// Runs the accepted linear owner until it reaches a process-observable
/// authenticated boundary.  CANCEL completion is not a process exit: the
/// exact retained owner is consumed back into ordinary polling in the same
/// task.  Every other non-shutdown terminal is restart-only and can never be
/// mistaken for a successful daemon exit.
async fn run_stage8b_p1e_process_owner_v1(
    startup: Stage8bP1eStartupOwnerV1,
    mut reader: crate::Stage8bP1eRedisScheduleReader,
    mut context: strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: Arc<Stage8bP1eShutdownLatchV1>,
    commitment_key: Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eOwnerTaskBoundaryV1, Stage8bP1eStartupErrorV1> {
    let mut outcome = run_stage8b_p1e_startup_owner_loop_v1(
        startup,
        &mut reader,
        &mut context,
        latch.as_ref(),
        &commitment_key,
    )
    .await?;
    loop {
        outcome = match outcome {
            Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelResolved(resolved) => {
                run_stage8b_p1e_owner_loop_v1(
                    resolved.into_ready_polling(),
                    &mut reader,
                    &mut context,
                    latch.as_ref(),
                    &commitment_key,
                )
                .await?
            }
            Stage8bP1eOwnerLoopOutcomeV1::StoppedReady(_)
            | Stage8bP1eOwnerLoopOutcomeV1::StoppedSchedule(_)
            | Stage8bP1eOwnerLoopOutcomeV1::RetainedSource(_)
            | Stage8bP1eOwnerLoopOutcomeV1::RetainedRecovery(_) => {
                return Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop);
            }
            Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelRestartRequired(_)
            | Stage8bP1eOwnerLoopOutcomeV1::CommittedDayExpiryResolved(_)
            | Stage8bP1eOwnerLoopOutcomeV1::PendingNotClaimable(_)
            | Stage8bP1eOwnerLoopOutcomeV1::Blocked(_)
            | Stage8bP1eOwnerLoopOutcomeV1::ScheduleExhausted(_)
            | Stage8bP1eOwnerLoopOutcomeV1::UnsupportedSchedule(_)
            | Stage8bP1eOwnerLoopOutcomeV1::StartupPendingNotClaimable(_)
            | Stage8bP1eOwnerLoopOutcomeV1::StartupLimitSchedule(_)
            | Stage8bP1eOwnerLoopOutcomeV1::StartupCommittedSchedule(_) => {
                return Ok(Stage8bP1eOwnerTaskBoundaryV1::RestartRequired);
            }
        };
    }
}

async fn execute_run(
    supervisor: Stage8bP1eValidatedSupervisorConfigV1,
    trusted_now: DateTime<Utc>,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    // Register and actively supervise both Unix signals before spawning the
    // production startup path. The coordinator/latch and its single grace
    // deadline therefore cover credential load, V5 admission, Redis attach,
    // S06 acquisition, schedule-reader attach and the steady-state owner.
    let terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| Stage8bP1eProcessErrorV1::SignalTaskFailed)?;
    let interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|_| Stage8bP1eProcessErrorV1::SignalTaskFailed)?;
    let shutdown_grace_ms = supervisor.shutdown_grace_ms();
    let coordinator = Stage8bP1eCoordinatorV1::new();
    let latch = coordinator.shutdown_latch();
    let owner = tokio::spawn(run_stage8b_p1e_production_owner_v1(
        supervisor,
        trusted_now,
        latch,
    ));
    supervise_stage8b_p1e_owner_task_v1(owner, coordinator, shutdown_grace_ms, terminate, interrupt)
        .await
}

async fn run_stage8b_p1e_production_owner_v1(
    supervisor: Stage8bP1eValidatedSupervisorConfigV1,
    trusted_now: DateTime<Utc>,
    latch: Arc<Stage8bP1eShutdownLatchV1>,
) -> Result<Stage8bP1eOwnerTaskBoundaryV1, Stage8bP1eProcessErrorV1> {
    stage8b_p1e_process_startup_test_barrier_v1("before-admission", &latch).await;
    if latch.intent().is_some() {
        // No durable owner has been authenticated yet.  A stop here cannot
        // be reported as the exit-0 authenticated shutdown boundary.
        return Err(Stage8bP1eProcessErrorV1::DurableRestart);
    }
    let context = strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1 {
        expected_instrument_map_fingerprint_sha256:
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
        expected_operational_identity_sha256: supervisor
            .bootstrap()
            .operational_identity_sha256()
            .to_string(),
        expected_registry_identity_sha256: supervisor
            .schedule_registry_identity_sha256()
            .to_string(),
        expected_registry_version: supervisor.schedule_registry_version().to_string(),
        expected_runtime_config_fingerprint_sha256: supervisor
            .runtime_config_fingerprint_sha256()
            .to_string(),
        high_water: None,
        trusted_now,
    };
    let commitment_key = load_stage8b_p1_commitment_key_from_systemd_credential()
        .map_err(|_| Stage8bP1eProcessErrorV1::Credential)?;
    let (bootstrap, runtime, attach_plan, _settings) = supervisor.into_run_parts();
    let restart = admit_stage8b_p1e_ordinary_run_v1(bootstrap, &commitment_key, runtime)
        .map_err(|_| Stage8bP1eProcessErrorV1::DurableRestart)?;
    stage8b_p1e_process_startup_test_barrier_v1("after-admission", &latch).await;
    if latch.intent().is_some() {
        drop(restart);
        return Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop);
    }
    let attachable = match stage8b_p1e_route_pre_redis_restart_v1(restart) {
        Stage8bP1ePreRedisRestartV1::Attachable(attachable) => attachable,
        Stage8bP1ePreRedisRestartV1::Blocked(_) => {
            return Err(Stage8bP1eProcessErrorV1::RestartBlocked);
        }
    };
    if latch.intent().is_some() {
        drop(attachable);
        return Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop);
    }

    let redis_url = attach_plan.redis_url().to_string();
    let consumer_name = attach_plan.consumer_name().to_string();
    stage8b_p1e_process_startup_test_barrier_v1("during-redis-attach", &latch).await;
    if latch.intent().is_some() {
        drop(attachable);
        return Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop);
    }
    let mut session = attach_stage8b_p1e_verified_redis(&attach_plan)
        .await
        .map_err(map_redis_attach_error)?;
    if latch.intent().is_some() {
        drop(session);
        drop(attachable);
        return Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop);
    }
    session
        .redis_control_mut()
        .clean_stale_zero_pending_consumers(&consumer_name)
        .await
        .map_err(map_redis_attach_error)?;
    if latch.intent().is_some() {
        drop(session);
        drop(attachable);
        return Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop);
    }
    stage8b_p1e_process_startup_test_barrier_v1("during-s06-acquisition", &latch).await;
    if latch.intent().is_some() {
        drop(session);
        drop(attachable);
        return Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop);
    }
    let startup = acquire_stage8b_p1e_startup_owner_v1(attachable, session)
        .await
        .map_err(map_startup_error)?;
    if latch.intent().is_some() {
        drop(startup);
        return Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop);
    }
    let reader = crate::Stage8bP1eRedisScheduleReader::connect(&redis_url)
        .await
        .map_err(|_| Stage8bP1eProcessErrorV1::ScheduleReader)?;
    if latch.intent().is_some() {
        drop(reader);
        drop(startup);
        return Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop);
    }
    run_stage8b_p1e_process_owner_v1(startup, reader, context, latch, commitment_key)
        .await
        .map_err(map_startup_error)
}

#[cfg(test)]
async fn stage8b_p1e_process_startup_test_barrier_v1(
    phase: &str,
    latch: &Stage8bP1eShutdownLatchV1,
) {
    const PHASE_ENV: &str = "STAGE8B_P1E_PROCESS_PRODUCTION_PHASE";
    const READY_ENV: &str = "STAGE8B_P1E_PROCESS_FIXTURE_READY";
    if std::env::var(PHASE_ENV).as_deref() != Ok(phase) {
        return;
    }
    let ready = std::path::PathBuf::from(
        std::env::var_os(READY_ENV).expect("production process fixture ready path"),
    );
    fs::write(&ready, phase.as_bytes()).expect("production process fixture ready marker");
    while latch.intent().is_none() {
        tokio::time::sleep(StdDuration::from_millis(5)).await;
    }
}

#[cfg(not(test))]
async fn stage8b_p1e_process_startup_test_barrier_v1(
    _phase: &str,
    _latch: &Stage8bP1eShutdownLatchV1,
) {
}

async fn supervise_stage8b_p1e_owner_task_v1(
    mut owner: tokio::task::JoinHandle<
        Result<Stage8bP1eOwnerTaskBoundaryV1, Stage8bP1eProcessErrorV1>,
    >,
    mut coordinator: Stage8bP1eCoordinatorV1,
    shutdown_grace_ms: u64,
    mut terminate: tokio::signal::unix::Signal,
    mut interrupt: tokio::signal::unix::Signal,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    let signal_received = tokio::select! {
        biased;
        signal = terminate.recv() => signal.is_some(),
        signal = interrupt.recv() => signal.is_some(),
        result = &mut owner => return finish_owner_task(&mut coordinator, result),
    };
    if !signal_received {
        let now = Utc::now().timestamp_millis();
        let _ = coordinator.coordinate(
            Stage8bP1eSupervisorEventV1::SignalTaskFailed,
            true,
            now,
            now.saturating_add(shutdown_grace_ms as i64),
            1,
        );
        return match tokio::time::timeout(StdDuration::from_millis(shutdown_grace_ms), &mut owner)
            .await
        {
            Ok(result) => finish_owner_task(&mut coordinator, result),
            Err(_) => {
                let decision = coordinator.coordinate(
                    Stage8bP1eSupervisorEventV1::GraceExpired,
                    true,
                    Utc::now().timestamp_millis(),
                    now.saturating_add(shutdown_grace_ms as i64),
                    2,
                );
                owner.abort();
                terminal_process_result(decision.exit_code)
            }
        };
    }

    let now = Utc::now().timestamp_millis();
    let grace_deadline = now.saturating_add(shutdown_grace_ms as i64);
    let _ = coordinator.coordinate(
        Stage8bP1eSupervisorEventV1::ExternalSignal,
        true,
        now,
        grace_deadline,
        1,
    );
    match tokio::time::timeout(StdDuration::from_millis(shutdown_grace_ms), &mut owner).await {
        Ok(result) => finish_owner_task(&mut coordinator, result),
        Err(_) => {
            let decision = coordinator.coordinate(
                Stage8bP1eSupervisorEventV1::GraceExpired,
                true,
                Utc::now().timestamp_millis(),
                grace_deadline,
                1,
            );
            owner.abort();
            terminal_process_result(decision.exit_code)
        }
    }
}

fn map_redis_attach_error(error: Stage8bP1eRedisControlError) -> Stage8bP1eProcessErrorV1 {
    match error {
        Stage8bP1eRedisControlError::ManifestMismatch
        | Stage8bP1eRedisControlError::KeyTypeMismatch
        | Stage8bP1eRedisControlError::GroupMismatch => Stage8bP1eProcessErrorV1::RedisDeployment,
        Stage8bP1eRedisControlError::Redis
        | Stage8bP1eRedisControlError::OperationTimeout
        | Stage8bP1eRedisControlError::ConsumerInventoryInvalid
        | Stage8bP1eRedisControlError::TelemetryWriteFailed => {
            Stage8bP1eProcessErrorV1::RedisAttach
        }
    }
}

fn map_startup_error(error: Stage8bP1eStartupErrorV1) -> Stage8bP1eProcessErrorV1 {
    match error {
        Stage8bP1eStartupErrorV1::RedisControl(error) => map_redis_attach_error(error),
        Stage8bP1eStartupErrorV1::Source(_)
        | Stage8bP1eStartupErrorV1::Schedule(_)
        | Stage8bP1eStartupErrorV1::ReadyBoundaryInvariant
        | Stage8bP1eStartupErrorV1::RecoveryStepBudgetExceeded
        | Stage8bP1eStartupErrorV1::CommittedDayExpiryPelNotEmpty => {
            Stage8bP1eProcessErrorV1::OwnerLoop
        }
    }
}

fn finish_owner_task(
    coordinator: &mut Stage8bP1eCoordinatorV1,
    result: Result<
        Result<Stage8bP1eOwnerTaskBoundaryV1, Stage8bP1eProcessErrorV1>,
        tokio::task::JoinError,
    >,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    finish_owner_task_at(coordinator, result, Utc::now().timestamp_millis())
}

fn finish_owner_task_at(
    coordinator: &mut Stage8bP1eCoordinatorV1,
    result: Result<
        Result<Stage8bP1eOwnerTaskBoundaryV1, Stage8bP1eProcessErrorV1>,
        tokio::task::JoinError,
    >,
    now_utc_ms: i64,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    match result {
        Err(_) => {
            let decision = coordinator.coordinate(
                Stage8bP1eSupervisorEventV1::OwnerPanicked,
                false,
                now_utc_ms,
                now_utc_ms,
                2,
            );
            terminal_process_result(decision.exit_code)
        }
        Ok(Err(error)) => {
            let _ = coordinator.coordinate(
                Stage8bP1eSupervisorEventV1::RedisLifecycleFailed,
                false,
                now_utc_ms,
                now_utc_ms,
                2,
            );
            Err(error)
        }
        Ok(Ok(Stage8bP1eOwnerTaskBoundaryV1::RestartRequired)) => {
            Err(Stage8bP1eProcessErrorV1::OwnerLoop)
        }
        Ok(Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop)) => {
            let decision = coordinator.coordinate(
                Stage8bP1eSupervisorEventV1::AuthenticatedBoundaryReached,
                false,
                now_utc_ms,
                0,
                2,
            );
            if decision.exit_code.is_some() {
                terminal_process_result(decision.exit_code)
            } else {
                let decision = coordinator.coordinate(
                    Stage8bP1eSupervisorEventV1::OwnerReturnedUnexpectedly,
                    false,
                    now_utc_ms,
                    now_utc_ms,
                    3,
                );
                terminal_process_result(decision.exit_code)
            }
        }
    }
}

fn terminal_process_result(
    exit_code: Option<u8>,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    match exit_code {
        Some(0) => Ok(Stage8bP1eProcessSuccessV1::RunStopped),
        Some(64) => Err(Stage8bP1eProcessErrorV1::Config),
        Some(66) => Err(Stage8bP1eProcessErrorV1::DurableRestart),
        Some(67) => Err(Stage8bP1eProcessErrorV1::OwnerLoop),
        Some(72) => Err(Stage8bP1eProcessErrorV1::ShutdownGraceExpired),
        Some(73) => Err(Stage8bP1eProcessErrorV1::SignalTaskFailed),
        Some(70 | 71) | Some(_) | None => Err(Stage8bP1eProcessErrorV1::OwnerTaskFailed),
    }
}

fn load_validated_supervisor(
    config_path: &Path,
    boot_id_path: &Path,
    expected_config_uid: u32,
) -> Result<Stage8bP1eValidatedSupervisorConfigV1, Stage8bP1eProcessErrorV1> {
    let bytes = read_protected_config(config_path, expected_config_uid)?;
    let config: Stage8bP1eSupervisorConfigV1 =
        crate::parse_stage8b_p1e_supervisor_config_v1(&bytes)
            .map_err(|_| Stage8bP1eProcessErrorV1::Config)?;
    let boot_id = read_boot_id(boot_id_path)?;
    validate_stage8b_p1e_supervisor_config_v1(config, boot_id)
        .map_err(|_| Stage8bP1eProcessErrorV1::Config)
}

fn execute_bootstrap(
    supervisor: Stage8bP1eValidatedSupervisorConfigV1,
    trusted_now: DateTime<Utc>,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    let commitment_key = load_stage8b_p1_commitment_key_from_systemd_credential()
        .map_err(|_| Stage8bP1eProcessErrorV1::Credential)?;
    let admin = authorize_stage8b_p1_first_boot(
        supervisor.bootstrap(),
        crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
    )
    .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootTransaction)?;
    let generation =
        next_stage8b_p1e_bootstrap_attempt_generation_v5(supervisor.bootstrap(), &commitment_key)
            .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootTransaction)?;
    let prepared = build_stage8b_p1_first_boot_source_v1(supervisor, trusted_now)
        .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootSource)?;
    drop(
        first_boot_stage8b_p1e_transaction_v5(prepared, admin, generation, &commitment_key)
            .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootTransaction)?,
    );
    Ok(Stage8bP1eProcessSuccessV1::BootstrapAdopted)
}

fn execute_bootstrap_recovery(
    supervisor: Stage8bP1eValidatedSupervisorConfigV1,
    trusted_now: DateTime<Utc>,
    transaction_id_sha256: &str,
    action: Stage8bP1eProcessRecoveryActionV1,
) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
    let commitment_key = load_stage8b_p1_commitment_key_from_systemd_credential()
        .map_err(|_| Stage8bP1eProcessErrorV1::Credential)?;
    match action {
        Stage8bP1eProcessRecoveryActionV1::PreSeal(action) => {
            let selector = authorize_stage8b_p1e_pre_seal_recovery_v5(
                supervisor.bootstrap(),
                transaction_id_sha256,
                action,
                STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION,
            )
            .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootRecovery)?;
            drop(
                recover_stage8b_p1e_first_boot_pre_seal_from_supervisor_v5(
                    supervisor,
                    trusted_now,
                    selector,
                    &commitment_key,
                )
                .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootRecovery)?,
            );
        }
        Stage8bP1eProcessRecoveryActionV1::Adoption(action) => {
            let (config, runtime) = supervisor.into_first_boot_parts();
            drop(
                recover_stage8b_p1e_first_boot_adoption_v5(
                    config,
                    &commitment_key,
                    runtime,
                    transaction_id_sha256,
                    action,
                )
                .map_err(|_| Stage8bP1eProcessErrorV1::FirstBootRecovery)?,
            );
        }
    }
    Ok(Stage8bP1eProcessSuccessV1::RecoveryApplied)
}

fn parse_recovery_selector(
    selector: &str,
) -> Result<(String, Stage8bP1eProcessRecoveryActionV1), Stage8bP1eProcessErrorV1> {
    let Some((transaction_id, action)) = selector.split_once('.') else {
        return Err(Stage8bP1eProcessErrorV1::Usage);
    };
    if transaction_id.len() != 64
        || !transaction_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || action.contains('.')
    {
        return Err(Stage8bP1eProcessErrorV1::Usage);
    }
    let action = match action {
        "remove-marker-temp" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::RemoveMarkerTemp,
        ),
        "resume-prepared" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::ResumePrepared,
        ),
        "quarantine-root" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::QuarantineRoot,
        ),
        "finalize-quarantine" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::FinalizeQuarantine,
        ),
        "complete-prepared-to-root-published" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::CompletePreparedToRootPublished,
        ),
        "complete-root-published-to-journal-durable" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::CompleteRootPublishedToJournalDurable,
        ),
        "complete-journal-durable-to-seal-committed" => Stage8bP1eProcessRecoveryActionV1::PreSeal(
            Stage8bP1ePreSealRecoveryActionV5::CompleteJournalDurableToSealCommitted,
        ),
        "remove-receipt-temp-and-adopt" => Stage8bP1eProcessRecoveryActionV1::Adoption(
            Stage8bP1eAdoptionRecoveryActionV5::RemoveReceiptTempAndAdopt,
        ),
        "adopt-committed-root" => Stage8bP1eProcessRecoveryActionV1::Adoption(
            Stage8bP1eAdoptionRecoveryActionV5::AdoptCommittedRoot,
        ),
        "start-seal-committed-to-adopted" => Stage8bP1eProcessRecoveryActionV1::Adoption(
            Stage8bP1eAdoptionRecoveryActionV5::StartSealCommittedToAdopted,
        ),
        "complete-seal-committed-to-adopted" => Stage8bP1eProcessRecoveryActionV1::Adoption(
            Stage8bP1eAdoptionRecoveryActionV5::CompleteSealCommittedToAdopted,
        ),
        _ => return Err(Stage8bP1eProcessErrorV1::Usage),
    };
    Ok((transaction_id.to_string(), action))
}

fn read_protected_config(
    path: &Path,
    expected_uid: u32,
) -> Result<Vec<u8>, Stage8bP1eProcessErrorV1> {
    read_protected_config_with_before_open(path, expected_uid, || {})
}

fn read_protected_config_with_before_open<F>(
    path: &Path,
    expected_uid: u32,
    before_open: F,
) -> Result<Vec<u8>, Stage8bP1eProcessErrorV1>
where
    F: FnOnce(),
{
    if !path.is_absolute() || path.as_os_str().as_bytes().contains(&0) {
        return Err(Stage8bP1eProcessErrorV1::ConfigBoundary);
    }
    let before =
        fs::symlink_metadata(path).map_err(|_| Stage8bP1eProcessErrorV1::ConfigBoundary)?;
    validate_config_metadata(&before, expected_uid)?;
    before_open();
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| Stage8bP1eProcessErrorV1::ConfigBoundary)?;
    let opened = file
        .metadata()
        .map_err(|_| Stage8bP1eProcessErrorV1::ConfigBoundary)?;
    validate_config_metadata(&opened, expected_uid)?;
    if !same_file_metadata(&before, &opened) {
        return Err(Stage8bP1eProcessErrorV1::ConfigBoundary);
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(opened.len()).map_err(|_| Stage8bP1eProcessErrorV1::ConfigBoundary)?,
    );
    file.by_ref()
        .take(STAGE8B_P1E_CONFIG_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Stage8bP1eProcessErrorV1::ConfigBoundary)?;
    let after = file
        .metadata()
        .map_err(|_| Stage8bP1eProcessErrorV1::ConfigBoundary)?;
    if bytes.len() as u64 != opened.len()
        || bytes.len() as u64 > STAGE8B_P1E_CONFIG_MAX_BYTES
        || !same_file_metadata(&opened, &after)
    {
        return Err(Stage8bP1eProcessErrorV1::ConfigBoundary);
    }
    Ok(bytes)
}

fn validate_config_metadata(
    metadata: &fs::Metadata,
    expected_uid: u32,
) -> Result<(), Stage8bP1eProcessErrorV1> {
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.nlink() != 1
        || metadata.uid() != expected_uid
        || metadata.permissions().mode() & 0o022 != 0
        || metadata.len() > STAGE8B_P1E_CONFIG_MAX_BYTES
    {
        return Err(Stage8bP1eProcessErrorV1::ConfigBoundary);
    }
    Ok(())
}

fn same_file_metadata(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.len() == right.len()
        && left.uid() == right.uid()
        && left.gid() == right.gid()
        && left.nlink() == right.nlink()
        && left.permissions().mode() & 0o777 == right.permissions().mode() & 0o777
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

fn read_boot_id(path: &Path) -> Result<[u8; 16], Stage8bP1eProcessErrorV1> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(64).read_to_end(&mut bytes))
        .map_err(|_| Stage8bP1eProcessErrorV1::BootIdentity)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Stage8bP1eProcessErrorV1::BootIdentity)?
        .trim_end_matches('\n');
    if text.len() != 36 || text.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(Stage8bP1eProcessErrorV1::BootIdentity);
    }
    let uuid = uuid::Uuid::parse_str(text).map_err(|_| Stage8bP1eProcessErrorV1::BootIdentity)?;
    if uuid.hyphenated().to_string() != text {
        return Err(Stage8bP1eProcessErrorV1::BootIdentity);
    }
    Ok(*uuid.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        ffi::CString,
        io::Write,
        net::TcpListener,
        os::unix::fs::DirBuilderExt,
        os::unix::process::ExitStatusExt,
        path::PathBuf,
        process::{Child, Command, Stdio},
        time::{Duration as StdDuration, Instant},
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
                .expect("redis-server is required for the P1-e process proof");
            let url = format!("redis://127.0.0.1:{port}/");
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
                tokio::time::sleep(StdDuration::from_millis(10)).await;
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

    const PROCESS_FIXTURE_PARENT: &str = "STAGE8B_P1E_PROCESS_FIXTURE_PARENT";
    const PROCESS_FIXTURE_REDIS_URL: &str = "STAGE8B_P1E_PROCESS_FIXTURE_REDIS_URL";
    const PROCESS_FIXTURE_READY: &str = "STAGE8B_P1E_PROCESS_FIXTURE_READY";
    const PROCESS_FIXTURE_TRUSTED_NOW_MS: &str = "STAGE8B_P1E_PROCESS_FIXTURE_TRUSTED_NOW_MS";
    const PROCESS_PRODUCTION_PHASE: &str = "STAGE8B_P1E_PROCESS_PRODUCTION_PHASE";
    const PROCESS_FIXTURE_MANIFEST_SHA256: &str = "STAGE8B_P1E_PROCESS_FIXTURE_MANIFEST_SHA256";
    const PROCESS_FIXTURE_CREDENTIALS: &str = "STAGE8B_P1E_PROCESS_FIXTURE_CREDENTIALS";
    const PROCESS_FIXTURE_BOOT_ID: [u8; 16] = [0x42; 16];

    async fn seed_idle_process_fixture(redis_url: &str, parent: &Path) -> String {
        let (source, export_input, key, fresh) =
            strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let validated = crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent.to_path_buf(),
            fresh.stage5c_config_fingerprint(),
        ))
        .unwrap();
        let admin = crate::authorize_stage8b_p1_first_boot(
            &validated,
            crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
        )
        .unwrap();
        let first_boot =
            crate::first_boot_stage8b_p1(validated, admin, source, export_input, &key, fresh)
                .unwrap();
        let operational_identity_sha256 = first_boot.receipt().operational_identity_sha256.clone();
        drop(first_boot);
        drop(
            crate::initialize_stage8b_p1_redis_namespace(
                redis_url,
                crate::Stage8bP1RedisConfig::paper_default_auto(),
            )
            .await
            .unwrap(),
        );
        operational_identity_sha256
    }

    fn seed_adopted_production_process_fixture(parent: &Path) {
        let (_, runtime_fingerprint) = crate::Stage8bP1RuntimeProfileV1::build_hybrid_runtime()
            .expect("production process fixture runtime");
        let (prepared, admin, key, _) =
            crate::stage8b_p1e_first_boot_source::tests::prepared_transaction_with_bootstrap(
                parent,
                bootstrap_config(parent.to_path_buf(), runtime_fingerprint),
            );
        let adopted = crate::first_boot_stage8b_p1e_transaction_v5(prepared, admin, 1, &key)
            .expect("production process fixture adoption");
        assert!(adopted.owner().recovery_ready());
        drop(adopted);
    }

    fn production_process_credentials(label: &str) -> PathBuf {
        let directory = temp_directory(label);
        let path = directory.join(crate::STAGE8B_P1_COMMITMENT_CREDENTIAL_FILE);
        fs::write(&path, [0x8b; 32]).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        directory
    }

    async fn redis_database_snapshot(redis_url: &str) -> BTreeMap<String, Vec<u8>> {
        let client = redis::Client::open(redis_url).unwrap();
        let mut connection = redis::aio::ConnectionManager::new(client).await.unwrap();
        let mut keys: Vec<String> = redis::cmd("KEYS")
            .arg("*")
            .query_async(&mut connection)
            .await
            .unwrap();
        keys.sort();
        let mut snapshot = BTreeMap::new();
        for key in keys {
            let mut bytes: Vec<u8> = redis::cmd("DUMP")
                .arg(&key)
                .query_async(&mut connection)
                .await
                .unwrap();
            let ttl_ms: i64 = redis::cmd("PTTL")
                .arg(&key)
                .query_async(&mut connection)
                .await
                .unwrap();
            bytes.extend_from_slice(&ttl_ms.to_be_bytes());
            snapshot.insert(key, bytes);
        }
        snapshot
    }

    fn spawn_process_fixture_child(
        test_name: &str,
        redis_url: &str,
        parent: &Path,
        ready: &Path,
    ) -> Child {
        Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg(test_name)
            .arg("--nocapture")
            .env(PROCESS_FIXTURE_PARENT, parent)
            .env(PROCESS_FIXTURE_REDIS_URL, redis_url)
            .env(PROCESS_FIXTURE_READY, ready)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    fn spawn_production_startup_fixture_child(
        redis_url: &str,
        parent: &Path,
        ready: &Path,
        credentials: &Path,
        phase: &str,
        manifest_sha256: &str,
    ) -> Child {
        Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("stage8b_p1e_process::tests::stage8b_p1e_production_startup_signal_fixture_child")
            .arg("--nocapture")
            .env(PROCESS_FIXTURE_PARENT, parent)
            .env(PROCESS_FIXTURE_REDIS_URL, redis_url)
            .env(PROCESS_FIXTURE_READY, ready)
            .env(PROCESS_PRODUCTION_PHASE, phase)
            .env(PROCESS_FIXTURE_MANIFEST_SHA256, manifest_sha256)
            .env(PROCESS_FIXTURE_CREDENTIALS, credentials)
            .env("CREDENTIALS_DIRECTORY", credentials)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    fn wait_for_process_fixture(child: &mut Child, ready: &Path) {
        let deadline = Instant::now() + StdDuration::from_secs(15);
        while !ready.exists() && Instant::now() < deadline {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("process fixture exited before ready: {status}");
            }
            std::thread::sleep(StdDuration::from_millis(20));
        }
        assert!(ready.exists(), "process fixture did not become ready");
    }

    fn wait_for_process_exit(child: &mut Child) -> std::process::ExitStatus {
        let deadline = Instant::now() + StdDuration::from_secs(15);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("process fixture did not exit before deadline");
            }
            std::thread::sleep(StdDuration::from_millis(20));
        }
    }

    async fn run_idle_process_fixture_child(
    ) -> Result<Stage8bP1eProcessSuccessV1, Stage8bP1eProcessErrorV1> {
        let parent = fs::canonicalize(std::env::var_os(PROCESS_FIXTURE_PARENT).unwrap()).unwrap();
        let redis_url = std::env::var(PROCESS_FIXTURE_REDIS_URL).unwrap();
        let ready = PathBuf::from(std::env::var_os(PROCESS_FIXTURE_READY).unwrap());
        let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let fingerprint = fresh.stage5c_config_fingerprint().to_string();
        let identity = operational_identity_config(parent.clone(), fingerprint.clone());
        let identity_sha256 =
            strategy_runtime_core::stage6d_operational_identity_sha256(&identity).unwrap();
        let restart_config = crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent,
            fingerprint.clone(),
        ))
        .unwrap();
        let restart = crate::restart_stage8b_p1(restart_config, &key, fresh).unwrap();
        let Stage8bP1ePreRedisRestartV1::Attachable(restart) =
            stage8b_p1e_route_pre_redis_restart_v1(restart)
        else {
            panic!("idle process fixture restart must be attachable")
        };
        let session =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_verified_redis_session_v1(&redis_url)
                .await;
        let startup = acquire_stage8b_p1e_startup_owner_v1(restart, session)
            .await
            .unwrap();
        let reader = crate::Stage8bP1eRedisScheduleReader::connect(&redis_url)
            .await
            .unwrap();
        let context = schedule_context(
            identity_sha256.as_str().to_string(),
            fingerprint,
            DateTime::<Utc>::from_timestamp_millis(1_785_759_000_000).unwrap(),
        );
        let terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        let interrupt =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();
        let coordinator = Stage8bP1eCoordinatorV1::new();
        let latch = coordinator.shutdown_latch();
        let owner = tokio::spawn(async move {
            run_stage8b_p1e_process_owner_v1(startup, reader, context, latch, key)
                .await
                .map_err(map_startup_error)
        });
        fs::write(ready, b"owner-live-signal-handlers-installed").unwrap();
        supervise_stage8b_p1e_owner_task_v1(owner, coordinator, 5_000, terminate, interrupt).await
    }

    #[tokio::test]
    #[ignore]
    async fn stage8b_p1e_production_startup_signal_fixture_child() {
        let parent = fs::canonicalize(std::env::var_os(PROCESS_FIXTURE_PARENT).unwrap()).unwrap();
        let redis_url = std::env::var(PROCESS_FIXTURE_REDIS_URL).unwrap();
        let manifest_sha256 = std::env::var(PROCESS_FIXTURE_MANIFEST_SHA256).unwrap();
        let credentials =
            fs::canonicalize(std::env::var_os(PROCESS_FIXTURE_CREDENTIALS).unwrap()).unwrap();
        assert_eq!(
            std::env::var_os("CREDENTIALS_DIRECTORY").as_deref(),
            Some(credentials.as_os_str())
        );
        let supervisor =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_validated_production_supervisor_v1(
                production_supervisor_config(parent),
                PROCESS_FIXTURE_BOOT_ID,
                &redis_url,
                &manifest_sha256,
            );
        match execute_run(supervisor, Utc::now()).await {
            Ok(Stage8bP1eProcessSuccessV1::RunStopped) => {}
            Ok(other) => panic!("unexpected production startup success: {other:?}"),
            Err(error) => std::process::exit(error.exit_code().into()),
        }
    }

    #[tokio::test]
    #[ignore]
    async fn stage8b_p1e_idle_process_fixture_child() {
        assert_eq!(
            run_idle_process_fixture_child().await.unwrap(),
            Stage8bP1eProcessSuccessV1::RunStopped
        );
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    #[ignore]
    async fn stage8b_p1e_committed_cancel_process_fixture_child() {
        let parent = fs::canonicalize(std::env::var_os(PROCESS_FIXTURE_PARENT).unwrap()).unwrap();
        let redis_url = std::env::var(PROCESS_FIXTURE_REDIS_URL).unwrap();
        let ready = PathBuf::from(std::env::var_os(PROCESS_FIXTURE_READY).unwrap());
        let trusted_now_ms: i64 = std::env::var(PROCESS_FIXTURE_TRUSTED_NOW_MS)
            .unwrap()
            .parse()
            .unwrap();
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(trusted_now_ms).unwrap();
        let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let fingerprint = fresh.stage5c_config_fingerprint().to_string();
        let identity = operational_identity_config(parent.clone(), fingerprint.clone());
        let identity_sha256 = strategy_runtime_core::stage6d_operational_identity_sha256(&identity)
            .unwrap()
            .as_str()
            .to_string();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity_sha256,
            fingerprint,
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let root_name =
            crate::Stage7bDurableRootAuthority::expected_directory_name(&identity).unwrap();
        let root = crate::Stage7bDurableRootAuthority::validate(parent.join(root_name), &identity)
            .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            identity,
            &key,
            fresh,
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let Stage8bP1ePreRedisRestartV1::Attachable(restart) =
            stage8b_p1e_route_pre_redis_restart_v1(restart)
        else {
            panic!("committed Cancel process fixture must be attachable")
        };
        let session =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_verified_redis_session_v1(&redis_url)
                .await;
        let startup = acquire_stage8b_p1e_startup_owner_v1(restart, session)
            .await
            .unwrap();
        let reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis_url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        let interrupt =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();
        let coordinator = Stage8bP1eCoordinatorV1::new();
        let latch = coordinator.shutdown_latch();
        let owner = tokio::spawn(async move {
            run_stage8b_p1e_process_owner_v1(startup, reader, fixture.context, latch, key)
                .await
                .map_err(map_startup_error)
        });
        fs::write(ready, b"committed-cancel-owner-live").unwrap();
        match supervise_stage8b_p1e_owner_task_v1(owner, coordinator, 5_000, terminate, interrupt)
            .await
        {
            Ok(Stage8bP1eProcessSuccessV1::RunStopped) => {}
            Ok(other) => panic!("unexpected committed-Cancel process success: {other:?}"),
            Err(error) => std::process::exit(error.exit_code().into()),
        }
    }

    async fn panicking_process_owner_fixture(
    ) -> Result<Stage8bP1eOwnerTaskBoundaryV1, Stage8bP1eProcessErrorV1> {
        panic!("deterministic owner panic fixture")
    }

    #[tokio::test]
    #[ignore]
    async fn stage8b_p1e_panicking_process_fixture_child() {
        let ready = PathBuf::from(std::env::var_os(PROCESS_FIXTURE_READY).unwrap());
        let terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        let interrupt =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();
        let coordinator = Stage8bP1eCoordinatorV1::new();
        let owner = tokio::spawn(panicking_process_owner_fixture());
        fs::write(ready, b"panic-owner-spawned").unwrap();
        let result =
            supervise_stage8b_p1e_owner_task_v1(owner, coordinator, 5_000, terminate, interrupt)
                .await;
        let error = result.expect_err("owner panic must fail the process");
        std::process::exit(error.exit_code().into());
    }

    fn assert_idle_process_restart_is_ready(parent: &Path) {
        let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let restart_config = crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent.to_path_buf(),
            fresh.stage5c_config_fingerprint(),
        ))
        .unwrap();
        let restart = crate::restart_stage8b_p1(restart_config, &key, fresh).unwrap();
        assert!(matches!(restart, Stage7bRestartOutcome::Ready(_)));
    }

    #[tokio::test]
    async fn os_process_idle_sigterm_exits_zero_at_authenticated_boundary() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("os-process-idle-sigterm");
        let control = temp_directory("os-process-idle-sigterm-control");
        seed_idle_process_fixture(&redis.url, &parent).await;
        let durable_before = durable_file_snapshot(&parent);
        let ready = control.join("child-ready");
        let mut child = spawn_process_fixture_child(
            "stage8b_p1e_process::tests::stage8b_p1e_idle_process_fixture_child",
            &redis.url,
            &parent,
            &ready,
        );
        wait_for_process_fixture(&mut child, &ready);
        assert_eq!(
            unsafe { libc::kill(child.id().try_into().unwrap(), libc::SIGTERM) },
            0
        );
        let status = wait_for_process_exit(&mut child);
        assert_eq!(status.code(), Some(0));
        assert_eq!(status.signal(), None);
        assert_eq!(durable_file_snapshot(&parent), durable_before);
        assert_idle_process_restart_is_ready(&parent);

        let namespace = crate::stage8b_p1_redis_namespace();
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let pending: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0);
        fs::remove_dir_all(parent).unwrap();
        fs::remove_dir_all(control).unwrap();
    }

    #[tokio::test]
    async fn production_run_signals_cover_admission_attach_and_s06_without_effects() {
        for (label, phase, signal, expected_exit) in [
            (
                "pre-admission-sigterm",
                "before-admission",
                libc::SIGTERM,
                66,
            ),
            ("admitted-sigterm", "after-admission", libc::SIGTERM, 0),
            ("attach-sigint", "during-redis-attach", libc::SIGINT, 0),
            ("s06-sigterm", "during-s06-acquisition", libc::SIGTERM, 0),
        ] {
            let redis = RedisServer::start().await;
            let parent = temp_directory(&format!("production-{label}"));
            let control = temp_directory(&format!("production-{label}-control"));
            let credentials = production_process_credentials(&format!("production-{label}-creds"));
            seed_adopted_production_process_fixture(&parent);
            let manifest_sha256 =
                crate::stage8b_p1_supervisor::stage8b_p1e_test_provision_production_redis_v1(
                    production_supervisor_config(parent.clone()),
                    PROCESS_FIXTURE_BOOT_ID,
                    &redis.url,
                )
                .await;
            let durable_before = durable_file_snapshot(&parent);
            let redis_before = redis_database_snapshot(&redis.url).await;
            let ready = control.join("production-startup-ready");
            let mut child = spawn_production_startup_fixture_child(
                &redis.url,
                &parent,
                &ready,
                &credentials,
                phase,
                &manifest_sha256,
            );
            wait_for_process_fixture(&mut child, &ready);
            let signal_started = Instant::now();
            assert_eq!(
                unsafe { libc::kill(child.id().try_into().unwrap(), signal) },
                0
            );
            let status = wait_for_process_exit(&mut child);
            assert_eq!(status.code(), Some(expected_exit), "phase {phase}");
            assert_eq!(status.signal(), None, "phase {phase}");
            assert!(
                signal_started.elapsed() < StdDuration::from_secs(5),
                "phase {phase} exceeded its single shutdown grace"
            );
            assert_eq!(
                durable_file_snapshot(&parent),
                durable_before,
                "phase {phase} changed durable authority"
            );
            assert_eq!(
                redis_database_snapshot(&redis.url).await,
                redis_before,
                "phase {phase} performed callback/publication/XACK or Redis repair"
            );

            let (fresh, _) = crate::Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
            let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x8b; 32]).unwrap();
            let admitted = crate::admit_stage8b_p1e_ordinary_run_v1(
                crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh,
            )
            .expect("signal boundary must retain one admissible owner");
            assert!(matches!(admitted, Stage7bRestartOutcome::Ready(_)));
            drop(admitted);
            fs::remove_dir_all(parent).unwrap();
            fs::remove_dir_all(control).unwrap();
            fs::remove_dir_all(credentials).unwrap();
        }
    }

    #[tokio::test]
    async fn os_process_sigkill_then_restart_preserves_single_ready_owner() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("os-process-sigkill-restart");
        let control = temp_directory("os-process-sigkill-restart-control");
        seed_idle_process_fixture(&redis.url, &parent).await;
        let durable_before = durable_file_snapshot(&parent);
        let first_ready = control.join("first-child-ready");
        let mut first = spawn_process_fixture_child(
            "stage8b_p1e_process::tests::stage8b_p1e_idle_process_fixture_child",
            &redis.url,
            &parent,
            &first_ready,
        );
        wait_for_process_fixture(&mut first, &first_ready);
        assert_eq!(
            unsafe { libc::kill(first.id().try_into().unwrap(), libc::SIGKILL) },
            0
        );
        let status = wait_for_process_exit(&mut first);
        assert_eq!(status.signal(), Some(libc::SIGKILL));
        assert_eq!(durable_file_snapshot(&parent), durable_before);
        assert_idle_process_restart_is_ready(&parent);

        let second_ready = control.join("second-child-ready");
        let mut second = spawn_process_fixture_child(
            "stage8b_p1e_process::tests::stage8b_p1e_idle_process_fixture_child",
            &redis.url,
            &parent,
            &second_ready,
        );
        wait_for_process_fixture(&mut second, &second_ready);
        assert_eq!(
            unsafe { libc::kill(second.id().try_into().unwrap(), libc::SIGTERM) },
            0
        );
        assert_eq!(wait_for_process_exit(&mut second).code(), Some(0));
        assert_eq!(durable_file_snapshot(&parent), durable_before);
        fs::remove_dir_all(parent).unwrap();
        fs::remove_dir_all(control).unwrap();
    }

    #[tokio::test]
    async fn os_process_owner_panic_exits_exact_class_70() {
        let parent = temp_directory("os-process-owner-panic");
        let control = temp_directory("os-process-owner-panic-control");
        let ready = control.join("panic-child-ready");
        let mut child = spawn_process_fixture_child(
            "stage8b_p1e_process::tests::stage8b_p1e_panicking_process_fixture_child",
            "redis://127.0.0.1:1/",
            &parent,
            &ready,
        );
        wait_for_process_fixture(&mut child, &ready);
        let status = wait_for_process_exit(&mut child);
        assert_eq!(status.code(), Some(70));
        assert_eq!(status.signal(), None);
        fs::remove_dir_all(parent).unwrap();
        fs::remove_dir_all(control).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn os_process_consumes_committed_cancel_handoff_and_keeps_polling() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("os-process-committed-cancel");
        let control = temp_directory("os-process-committed-cancel-control");
        let (published, key, fresh, identity_sha256, trusted_now_ms) =
            crate::stage8b_p1_semantic::p1e_test_cancel_published(&redis.url, &parent).await;
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(trusted_now_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity_sha256,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut binding_reader =
            crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
                &redis.url,
                fixture.public_key_hex.clone(),
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .await
            .unwrap();
        let snapshot = match binding_reader
            .read_newest_guarded(&fixture.context, &Stage8bP1eShutdownLatchV1::new())
            .await
            .unwrap()
        {
            crate::Stage8bP1eGuardedScheduleReadV1::Read(
                crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
            ) => snapshot,
            _ => panic!("fixture schedule must produce one verified Cancel snapshot"),
        };
        crate::stage8b_p1_semantic::p1e_test_commit_cancel_v4_only(
            published,
            *snapshot,
            trusted_now,
            &key,
        )
        .await;
        let namespace = crate::stage8b_p1_redis_namespace();
        let commands_before: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        let pending_before: redis::streams::StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .arg("-")
            .arg("+")
            .arg(2)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_before.ids.len(), 1);
        let cancel_source_redis_id = pending_before.ids[0].id.clone();
        let ready = control.join("cancel-child-ready");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--ignored")
            .arg("--exact")
            .arg("stage8b_p1e_process::tests::stage8b_p1e_committed_cancel_process_fixture_child")
            .arg("--nocapture")
            .env(PROCESS_FIXTURE_PARENT, &parent)
            .env(PROCESS_FIXTURE_REDIS_URL, &redis.url)
            .env(PROCESS_FIXTURE_READY, &ready)
            .env(PROCESS_FIXTURE_TRUSTED_NOW_MS, trusted_now_ms.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn().unwrap();
        wait_for_process_fixture(&mut child, &ready);

        let deadline = Instant::now() + StdDuration::from_secs(15);
        let successor_redis_id = loop {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("process owner returned before SIGTERM: {status}");
            }
            let pending: redis::streams::StreamPendingCountReply = redis::cmd("XPENDING")
                .arg(&namespace.canonical_m10_stream)
                .arg(&namespace.m10_consumer_group)
                .arg("-")
                .arg("+")
                .arg(2)
                .query_async(&mut connection)
                .await
                .unwrap();
            if pending.ids.len() == 1 && pending.ids[0].id != cancel_source_redis_id {
                break pending.ids[0].id.clone();
            }
            assert!(
                Instant::now() < deadline,
                "committed Cancel source was not XACKed before successor acquisition"
            );
            tokio::time::sleep(StdDuration::from_millis(20)).await;
        };
        assert_ne!(successor_redis_id, cancel_source_redis_id);
        assert!(
            child.try_wait().unwrap().is_none(),
            "the process must consume Cancel completion and continue polling"
        );
        assert_eq!(
            unsafe { libc::kill(child.id().try_into().unwrap(), libc::SIGTERM) },
            0
        );
        assert_eq!(wait_for_process_exit(&mut child).code(), Some(0));
        let commands_after: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            commands_after,
            commands_before + 1,
            "the successor callback may publish its own command while the completed Cancel is not republished"
        );

        let identity =
            operational_identity_config(parent.clone(), fresh.stage5c_config_fingerprint());
        let root_name =
            crate::Stage7bDurableRootAuthority::expected_directory_name(&identity).unwrap();
        let root = crate::Stage7bDurableRootAuthority::validate(parent.join(root_name), &identity)
            .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            identity,
            &key,
            fresh,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        assert!(
            !matches!(restart, Stage7bRestartOutcome::P1d3TruthCommitted(_)),
            "the completed Cancel V4 cannot be replayed after successor acquisition"
        );
        let audit = restart.stage8b_p1d4_test_runtime_audit().unwrap();
        assert_eq!(
            audit
                .sequence_allocations
                .iter()
                .filter(|allocation| allocation.outcome_kind == "cancel_canceled")
                .count(),
            1,
            "process restart must retain exactly one terminal Cancel allocation"
        );
        fs::remove_dir_all(parent).unwrap();
        fs::remove_dir_all(control).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn os_process_sigkill_after_cancel_truth_recovers_xack_last_and_keeps_polling() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("os-process-cancel-truth-sigkill");
        let control = temp_directory("os-process-cancel-truth-sigkill-control");
        let (published, key, fresh, identity_sha256, trusted_now_ms) =
            crate::stage8b_p1_semantic::p1e_test_target_first_cancel_published(&redis.url, &parent)
                .await;
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(trusted_now_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity_sha256,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut binding_reader =
            crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
                &redis.url,
                fixture.public_key_hex.clone(),
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .await
            .unwrap();
        let snapshot = match binding_reader
            .read_newest_guarded(&fixture.context, &Stage8bP1eShutdownLatchV1::new())
            .await
            .unwrap()
        {
            crate::Stage8bP1eGuardedScheduleReadV1::Read(
                crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
            ) => snapshot,
            _ => panic!("fixture schedule must produce one verified Cancel snapshot"),
        };
        crate::stage8b_p1_semantic::p1e_test_commit_cancel_v4_only(
            published,
            *snapshot,
            trusted_now,
            &key,
        )
        .await;

        let namespace = crate::stage8b_p1_redis_namespace();
        let pending_before: redis::streams::StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .arg("-")
            .arg("+")
            .arg(2)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_before.ids.len(), 1);
        let cancel_source_redis_id = pending_before.ids[0].id.clone();

        let target_first_ready = control.join("cancel-target-first-child-ready");
        let mut target_first = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("stage8b_p1e_process::tests::stage8b_p1e_committed_cancel_process_fixture_child")
            .arg("--nocapture")
            .env(PROCESS_FIXTURE_PARENT, &parent)
            .env(PROCESS_FIXTURE_REDIS_URL, &redis.url)
            .env(PROCESS_FIXTURE_READY, &target_first_ready)
            .env(PROCESS_FIXTURE_TRUSTED_NOW_MS, trusted_now_ms.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_process_fixture(&mut target_first, &target_first_ready);
        assert_eq!(
            wait_for_process_exit(&mut target_first).code(),
            Some(Stage8bP1eProcessErrorV1::OwnerLoop.exit_code().into()),
            "target-first Cancel must exit at the typed restart-required boundary"
        );

        let target_restart_root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(
                crate::Stage7bDurableRootAuthority::expected_directory_name(
                    &operational_identity_config(
                        parent.clone(),
                        fresh.stage5c_config_fingerprint(),
                    ),
                )
                .unwrap(),
            ),
            &operational_identity_config(parent.clone(), fresh.stage5c_config_fingerprint()),
        )
        .unwrap();
        let target_restart =
            crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
                target_restart_root,
                operational_identity_config(parent.clone(), fresh.stage5c_config_fingerprint()),
                &key,
                fresh.clone(),
                fixture.public_key_hex.clone(),
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .unwrap();
        assert!(matches!(
            target_restart,
            Stage7bRestartOutcome::P1d3CancelContinuationPending(_)
        ));
        drop(target_restart);

        let first_ready = control.join("cancel-crash-child-ready");
        let crash_marker = control.join("cancel-truth-before-xack.marker");
        let crash_phase = "p1d3-after-s-cancel-recovered-before-source-xack";
        let mut first = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("stage8b_p1e_process::tests::stage8b_p1e_committed_cancel_process_fixture_child")
            .arg("--nocapture")
            .env(PROCESS_FIXTURE_PARENT, &parent)
            .env(PROCESS_FIXTURE_REDIS_URL, &redis.url)
            .env(PROCESS_FIXTURE_READY, &first_ready)
            .env(PROCESS_FIXTURE_TRUSTED_NOW_MS, trusted_now_ms.to_string())
            .env("STAGE8B_P1_TEST_CRASH_PHASE", crash_phase)
            .env("STAGE8B_P1_TEST_CRASH_MARKER", &crash_marker)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_process_fixture(&mut first, &first_ready);
        wait_for_process_fixture(&mut first, &crash_marker);
        let marker: serde_json::Value =
            serde_json::from_slice(&fs::read(&crash_marker).unwrap()).unwrap();
        assert_eq!(marker["phase"], crash_phase);
        assert_eq!(marker["pid"], u64::from(first.id()));

        let pending_at_crash: redis::streams::StreamPendingCountReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .arg("-")
            .arg("+")
            .arg(2)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_at_crash.ids.len(), 1);
        assert_eq!(pending_at_crash.ids[0].id, cancel_source_redis_id);
        assert_eq!(
            unsafe { libc::kill(first.id().try_into().unwrap(), libc::SIGKILL) },
            0
        );
        assert_eq!(
            wait_for_process_exit(&mut first).signal(),
            Some(libc::SIGKILL)
        );

        let identity =
            operational_identity_config(parent.clone(), fresh.stage5c_config_fingerprint());
        let root_name =
            crate::Stage7bDurableRootAuthority::expected_directory_name(&identity).unwrap();
        let root = crate::Stage7bDurableRootAuthority::validate(parent.join(&root_name), &identity)
            .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            identity.clone(),
            &key,
            fresh.clone(),
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        assert!(
            matches!(restart, Stage7bRestartOutcome::P1d3TruthCommitted(_)),
            "SIGKILL after durable truth and before XACK must recover exact truth authority"
        );
        drop(restart);

        let second_ready = control.join("cancel-restart-child-ready");
        let mut second = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("stage8b_p1e_process::tests::stage8b_p1e_committed_cancel_process_fixture_child")
            .arg("--nocapture")
            .env(PROCESS_FIXTURE_PARENT, &parent)
            .env(PROCESS_FIXTURE_REDIS_URL, &redis.url)
            .env(PROCESS_FIXTURE_READY, &second_ready)
            .env(PROCESS_FIXTURE_TRUSTED_NOW_MS, trusted_now_ms.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_process_fixture(&mut second, &second_ready);

        let deadline = Instant::now() + StdDuration::from_secs(15);
        loop {
            if let Some(status) = second.try_wait().unwrap() {
                panic!("restart owner returned before SIGTERM: {status}");
            }
            let pending: redis::streams::StreamPendingCountReply = redis::cmd("XPENDING")
                .arg(&namespace.canonical_m10_stream)
                .arg(&namespace.m10_consumer_group)
                .arg("-")
                .arg("+")
                .arg(2)
                .query_async(&mut connection)
                .await
                .unwrap();
            if pending.ids.len() == 1 && pending.ids[0].id != cancel_source_redis_id {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "restart did not XACK the truth-covered Cancel source before successor acquisition"
            );
            tokio::time::sleep(StdDuration::from_millis(20)).await;
        }
        assert_eq!(
            unsafe { libc::kill(second.id().try_into().unwrap(), libc::SIGTERM) },
            0
        );
        assert_eq!(wait_for_process_exit(&mut second).code(), Some(0));

        let root = crate::Stage7bDurableRootAuthority::validate(parent.join(root_name), &identity)
            .unwrap();
        let final_restart =
            crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
                root,
                identity,
                &key,
                fresh,
                fixture.public_key_hex,
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .unwrap();
        assert!(
            !matches!(final_restart, Stage7bRestartOutcome::P1d3TruthCommitted(_)),
            "restart must not replay Cancel truth after XACK-last and successor acquisition"
        );
        let audit = final_restart.stage8b_p1d4_test_runtime_audit().unwrap();
        let target_truth = audit
            .sequence_allocations
            .iter()
            .find(|allocation| allocation.outcome_kind == "later_filled")
            .expect("target truth allocation must survive process SIGKILL/restart");
        assert!(target_truth.seq_ack.is_none());
        let target_truth_sequence = target_truth.seq_truth.unwrap();
        assert_eq!(
            target_truth.sequence_allocation_frontier.checked_add(1),
            Some(target_truth_sequence)
        );
        let recovered_cancel = audit
            .sequence_allocations
            .iter()
            .find(|allocation| allocation.outcome_kind == "cancel_execution_observed")
            .expect("recovered Cancel allocation must survive process SIGKILL/restart");
        assert!(recovered_cancel.seq_truth.is_none());
        assert_eq!(
            recovered_cancel.seq_ack,
            target_truth_sequence.checked_add(1)
        );
        assert_eq!(
            audit
                .sequence_allocations
                .iter()
                .filter(|allocation| allocation.outcome_kind == "cancel_execution_observed")
                .count(),
            1
        );
        fs::remove_dir_all(parent).unwrap();
        fs::remove_dir_all(control).unwrap();
    }

    fn fixed_config() -> &'static str {
        STAGE8B_P1E_SUPERVISOR_CONFIG_PATH
    }

    fn temp_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stage8b-p1e-process-{label}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder.create(&path).unwrap();
        fs::canonicalize(path).unwrap()
    }

    fn durable_file_snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn visit(root: &Path, current: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(current).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let file_type = entry.file_type().unwrap();
                if file_type.is_dir() {
                    visit(root, &path, files);
                } else if file_type.is_file() {
                    files.insert(
                        path.strip_prefix(root).unwrap().to_path_buf(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }

        let mut files = BTreeMap::new();
        visit(root, root, &mut files);
        files
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    async fn assert_day_expiry_progression_rejected(
        label: &str,
        prior_publication_sequence: u64,
        prior_semantic_revision: u64,
        publication_sequence: u64,
        semantic_revision: u64,
        expected: strategy_runtime_core::Stage8bP1eScheduleSourceError,
    ) {
        let redis = RedisServer::start().await;
        let parent = temp_directory(label);
        let (owner, key, fresh, identity, boundary_ms) =
            crate::stage8b_p1_semantic::p1e_test_day_expiry_ready_with_prior(
                &redis.url,
                &parent,
                prior_publication_sequence,
                prior_semantic_revision,
            )
            .await;
        let boundary = DateTime::<Utc>::from_timestamp_millis(boundary_ms).unwrap();
        let mut fixture =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_closed_schedule_envelope(
                identity,
                fresh.stage5c_config_fingerprint(),
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                boundary,
            );
        fixture.bytes =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_revised_schedule_envelope(
                &fixture.bytes,
                publication_sequence,
                semantic_revision,
                boundary,
            );
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let mut context = fixture.context.clone();
        let deferred = Stage8bP1eReadyPollingV1 {
            owner: Box::new(owner),
            control,
            committed_cancel_disposition: None,
        }
        .test_into_day_expiry_schedule_with_key(
            &mut context,
            &fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let prior_high_water = context
            .high_water
            .clone()
            .expect("Day-expiry admission must restore the durable Open high-water");
        let durable_before = durable_file_snapshot(&parent);

        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let result = advance_stage8b_p1e_supported_schedule_with_policy_v1(
            deferred,
            &mut reader,
            &mut context,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
            Stage8bP1eScheduleAcquisitionPolicyV1 {
                attempts: 1,
                total_deadline: StdDuration::from_millis(100),
                redis_operation_timeout: StdDuration::from_millis(20),
                initial_backoff: StdDuration::from_millis(1),
                maximum_backoff: StdDuration::from_millis(2),
            },
        )
        .await;
        let actual = match result {
            Err(Stage8bP1eStartupErrorV1::Schedule(
                crate::Stage8bP1eScheduleReadError::Source(error),
            )) => error,
            _ => panic!("invalid Day-expiry progression must fail in the schedule reader"),
        };
        assert_eq!(actual, expected, "unexpected rejection class for {label}");
        assert_eq!(context.high_water.as_ref(), Some(&prior_high_water));
        assert_eq!(
            durable_file_snapshot(&parent),
            durable_before,
            "rejected schedule must not change the Working V4 frontier or seal"
        );
        fs::remove_dir_all(parent).unwrap();
    }

    fn bootstrap_config(
        parent: PathBuf,
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
            deployment_id: "finam-imoexf-paper-p1".to_string(),
            deployment_generation: 1,
            gateway_instance_id: "finam-imoexf-paper-gateway-1".to_string(),
            market_data_generation: 1,
            command_consumer_generation: 1,
            stage8a4_writer_issuer_public_key_hex: "22".repeat(32),
            durable_parent: parent,
        }
    }

    fn supervisor_config_bytes(parent: &Path) -> Vec<u8> {
        let (_, runtime_config_fingerprint_sha256) =
            crate::Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
        serde_json::to_vec(&serde_json::json!({
            "schema_version": crate::STAGE8B_P1E_SUPERVISOR_CONFIG_SCHEMA_VERSION,
            "runtime_profile_id": crate::STAGE8B_P1E_RUNTIME_PROFILE_ID,
            "runtime_profile_sha256": crate::STAGE8B_P1E_RUNTIME_PROFILE_SHA256,
            "first_boot_source_bundle_sha256": "11".repeat(32),
            "schedule_registry_version": "test-registry-v1",
            "schedule_registry_identity_sha256": "11".repeat(32),
            "redis_url": crate::STAGE8B_P1E_REDIS_URL_IPV4,
            "redis_deployment_manifest_sha256": "22".repeat(32),
            "redis_runtime_policy_id": crate::STAGE8B_P1E_REDIS_RUNTIME_POLICY_ID,
            "redis_runtime_policy_sha256": crate::STAGE8B_P1E_REDIS_RUNTIME_POLICY_SHA256,
            "telemetry_contract_sha256": crate::STAGE8B_P1E_TELEMETRY_CONTRACT_SHA256,
            "health_interval_ms": 5_000,
            "shutdown_grace_ms": 30_000,
            "bootstrap": {
                "schema_version": crate::STAGE8B_P1_BOOTSTRAP_CONFIG_SCHEMA_VERSION,
                "broker_id": crate::STAGE8B_P1_BROKER_ID,
                "strategy_id": crate::STAGE8B_P1_STRATEGY_ID,
                "account_id": "ACC_TEST_0001",
                "internal_symbol": crate::STAGE8B_P1_INTERNAL_SYMBOL,
                "venue_symbol": crate::STAGE8B_P1_VENUE_SYMBOL,
                "exchange": crate::STAGE8B_P1_EXCHANGE,
                "market": crate::STAGE8B_P1_MARKET,
                "tick_size": crate::STAGE8B_P1_TICK_SIZE,
                "runtime_config_fingerprint_sha256": runtime_config_fingerprint_sha256,
                "instrument_map_fingerprint_sha256":
                    crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                "deployment_id": "finam-imoexf-paper-p1",
                "deployment_generation": 1,
                "gateway_instance_id": "finam-imoexf-paper-gateway-1",
                "market_data_generation": 1,
                "command_consumer_generation": 1,
                "stage8a4_writer_issuer_public_key_hex": "33".repeat(32),
                "durable_parent": parent,
            }
        }))
        .unwrap()
    }

    fn production_supervisor_config(parent: PathBuf) -> Stage8bP1eSupervisorConfigV1 {
        let (_, runtime_config_fingerprint_sha256) =
            crate::Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
        Stage8bP1eSupervisorConfigV1 {
            schema_version: crate::STAGE8B_P1E_SUPERVISOR_CONFIG_SCHEMA_VERSION,
            runtime_profile_id: crate::STAGE8B_P1E_RUNTIME_PROFILE_ID.to_string(),
            runtime_profile_sha256: crate::STAGE8B_P1E_RUNTIME_PROFILE_SHA256.to_string(),
            first_boot_source_bundle_sha256: "11".repeat(32),
            schedule_registry_version: "test-registry-v1".to_string(),
            schedule_registry_identity_sha256: "11".repeat(32),
            redis_url: crate::STAGE8B_P1E_REDIS_URL_IPV4.to_string(),
            redis_deployment_manifest_sha256: "22".repeat(32),
            redis_runtime_policy_id: crate::STAGE8B_P1E_REDIS_RUNTIME_POLICY_ID.to_string(),
            redis_runtime_policy_sha256: crate::STAGE8B_P1E_REDIS_RUNTIME_POLICY_SHA256.to_string(),
            telemetry_contract_sha256: crate::STAGE8B_P1E_TELEMETRY_CONTRACT_SHA256.to_string(),
            health_interval_ms: 5_000,
            shutdown_grace_ms: 5_000,
            bootstrap: bootstrap_config(parent, runtime_config_fingerprint_sha256),
        }
    }

    fn source_m1(open_ts_utc_ms: i64) -> Vec<crate::Stage8bP1CanonicalM10SourceM1> {
        (0..10)
            .map(|index| {
                let open = open_ts_utc_ms + index * 60_000;
                let close = open + 60_000;
                crate::Stage8bP1CanonicalM10SourceM1 {
                    redis_id: format!("{close}-0"),
                    semantic_id_sha256: format!("{:064x}", index + 1),
                    payload_sha256: format!("{:064x}", index + 101),
                    open_ts_utc_ms: open,
                    close_ts_utc_ms: close,
                }
            })
            .collect()
    }

    fn canonical_m10(operational_identity_sha256: String) -> Vec<u8> {
        let close_ts_utc_ms = 1_785_759_000_000;
        canonical_m10_at(operational_identity_sha256, close_ts_utc_ms, "2600")
    }

    fn canonical_m10_at(
        operational_identity_sha256: String,
        close_ts_utc_ms: i64,
        close: &str,
    ) -> Vec<u8> {
        let open_ts_utc_ms = close_ts_utc_ms - 600_000;
        crate::build_stage8b_p1_canonical_m10(crate::Stage8bP1CanonicalM10BuildInput {
            operational_identity_sha256,
            open_ts_utc_ms,
            close_ts_utc_ms,
            open: close.to_string(),
            high: close.to_string(),
            low: close.to_string(),
            close: close.to_string(),
            volume: "10000".to_string(),
            source_m1: source_m1(open_ts_utc_ms),
        })
        .unwrap()
    }

    fn schedule_context(
        operational_identity_sha256: String,
        runtime_config_fingerprint_sha256: String,
        trusted_now: DateTime<Utc>,
    ) -> strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1 {
        strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1 {
            expected_instrument_map_fingerprint_sha256:
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            expected_operational_identity_sha256: operational_identity_sha256,
            expected_registry_identity_sha256: "11".repeat(32),
            expected_registry_version: "test-registry-v1".to_string(),
            expected_runtime_config_fingerprint_sha256: runtime_config_fingerprint_sha256,
            high_water: None,
            trusted_now,
        }
    }

    fn operational_identity_config(
        parent: PathBuf,
        runtime_config_fingerprint_sha256: String,
    ) -> strategy_runtime_core::Stage6dOperationalIdentityConfig {
        let config = bootstrap_config(parent, runtime_config_fingerprint_sha256);
        strategy_runtime_core::Stage6dOperationalIdentityConfig {
            broker_id: config.broker_id,
            strategy_instance_id: config.strategy_id,
            deployment_id: config.deployment_id,
            deployment_generation: config.deployment_generation,
            gateway_instance_id: config.gateway_instance_id,
            instrument_map_fingerprint_sha256: config.instrument_map_fingerprint_sha256,
            market_data_generation: config.market_data_generation,
            command_consumer_generation: config.command_consumer_generation,
            stage8a4_writer_issuer_public_key_hex: config.stage8a4_writer_issuer_public_key_hex,
        }
    }

    #[test]
    fn cli_accepts_only_fixed_deployment_grammar() {
        assert_eq!(
            parse_stage8b_p1e_process_command_v1(["validate-config", fixed_config()]).unwrap(),
            Stage8bP1eProcessCommandV1::ValidateConfig
        );
        assert_eq!(
            parse_stage8b_p1e_process_command_v1([
                "bootstrap",
                fixed_config(),
                crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
            ])
            .unwrap(),
            Stage8bP1eProcessCommandV1::Bootstrap
        );
        assert!(parse_stage8b_p1e_process_command_v1(["run", "/tmp/supervisor.json"]).is_err());
        assert!(
            parse_stage8b_p1e_process_command_v1(["bootstrap", fixed_config(), "YES",]).is_err()
        );
    }

    #[test]
    fn process_errors_keep_the_accepted_stable_exit_classes() {
        assert_eq!(Stage8bP1eProcessErrorV1::Config.exit_code(), 64);
        assert_eq!(Stage8bP1eProcessErrorV1::Credential.exit_code(), 64);
        assert_eq!(Stage8bP1eProcessErrorV1::RestartBlocked.exit_code(), 66);
        assert_eq!(Stage8bP1eProcessErrorV1::RedisDeployment.exit_code(), 66);
        assert_eq!(Stage8bP1eProcessErrorV1::RedisAttach.exit_code(), 67);
        assert_eq!(Stage8bP1eProcessErrorV1::OwnerTaskFailed.exit_code(), 70);
        assert_eq!(
            Stage8bP1eProcessErrorV1::ShutdownGraceExpired.exit_code(),
            72
        );
        assert_eq!(Stage8bP1eProcessErrorV1::SignalTaskFailed.exit_code(), 73);
    }

    #[test]
    fn process_wrapper_preserves_coordinator_boundary_exit_classes() {
        let mut before_deadline = Stage8bP1eCoordinatorV1::new();
        let _ = before_deadline.coordinate(
            Stage8bP1eSupervisorEventV1::ExternalSignal,
            true,
            100,
            200,
            1,
        );
        assert_eq!(
            finish_owner_task_at(
                &mut before_deadline,
                Ok(Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop)),
                199,
            )
            .unwrap(),
            Stage8bP1eProcessSuccessV1::RunStopped
        );

        let mut at_deadline = Stage8bP1eCoordinatorV1::new();
        let _ = at_deadline.coordinate(
            Stage8bP1eSupervisorEventV1::ExternalSignal,
            true,
            100,
            200,
            1,
        );
        assert!(matches!(
            finish_owner_task_at(
                &mut at_deadline,
                Ok(Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop)),
                200,
            ),
            Err(Stage8bP1eProcessErrorV1::ShutdownGraceExpired)
        ));

        let mut signal_failure = Stage8bP1eCoordinatorV1::new();
        let _ = signal_failure.coordinate(
            Stage8bP1eSupervisorEventV1::SignalTaskFailed,
            true,
            100,
            200,
            1,
        );
        assert!(matches!(
            finish_owner_task_at(
                &mut signal_failure,
                Ok(Ok(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop)),
                150,
            ),
            Err(Stage8bP1eProcessErrorV1::SignalTaskFailed)
        ));
        let mut restart_required = Stage8bP1eCoordinatorV1::new();
        assert!(matches!(
            finish_owner_task_at(
                &mut restart_required,
                Ok(Ok(Stage8bP1eOwnerTaskBoundaryV1::RestartRequired)),
                150,
            ),
            Err(Stage8bP1eProcessErrorV1::OwnerLoop)
        ));
    }

    #[tokio::test]
    async fn process_wrapper_keeps_owner_panic_fatal_precedence() {
        for event in [
            Stage8bP1eSupervisorEventV1::SignalTaskFailed,
            Stage8bP1eSupervisorEventV1::ExternalSignal,
        ] {
            let mut coordinator = Stage8bP1eCoordinatorV1::new();
            let _ = coordinator.coordinate(event, true, 100, 200, 1);
            let joined = tokio::spawn(async {
                panic!("deterministic process-wrapper panic");
                #[allow(unreachable_code)]
                Ok::<_, Stage8bP1eProcessErrorV1>(Stage8bP1eOwnerTaskBoundaryV1::AuthenticatedStop)
            })
            .await;
            assert!(matches!(
                finish_owner_task_at(&mut coordinator, joined, 150),
                Err(Stage8bP1eProcessErrorV1::OwnerTaskFailed)
            ));
        }
    }

    #[test]
    fn recovery_selector_is_exact_and_complete() {
        let transaction = "a".repeat(64);
        let actions = [
            "remove-marker-temp",
            "resume-prepared",
            "quarantine-root",
            "finalize-quarantine",
            "complete-prepared-to-root-published",
            "complete-root-published-to-journal-durable",
            "complete-journal-durable-to-seal-committed",
            "remove-receipt-temp-and-adopt",
            "adopt-committed-root",
            "start-seal-committed-to-adopted",
            "complete-seal-committed-to-adopted",
        ];
        for action in actions {
            let selector = format!("{transaction}.{action}");
            assert!(parse_stage8b_p1e_process_command_v1([
                "bootstrap-recover",
                fixed_config(),
                selector.as_str(),
                STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION,
            ])
            .is_ok());
        }
        for selector in [
            format!("{}.resume-prepared", "A".repeat(64)),
            format!("{transaction}.unknown"),
            format!("{transaction}.resume-prepared.extra"),
        ] {
            assert!(parse_stage8b_p1e_process_command_v1([
                "bootstrap-recover",
                fixed_config(),
                selector.as_str(),
                STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION,
            ])
            .is_err());
        }
    }

    #[test]
    fn boot_id_parser_rejects_noncanonical_text() {
        let directory =
            std::env::temp_dir().join(format!("stage8b-p1e-boot-id-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("boot_id");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"01234567-89ab-cdef-0123-456789abcdef\n")
            .unwrap();
        assert_eq!(
            read_boot_id(&path).unwrap(),
            *uuid::Uuid::parse_str("01234567-89ab-cdef-0123-456789abcdef")
                .unwrap()
                .as_bytes()
        );
        fs::write(&path, b"01234567-89AB-cdef-0123-456789abcdef\n").unwrap();
        assert!(read_boot_id(&path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn protected_config_rejects_symlink_and_writable_file() {
        let directory = std::env::temp_dir().join(format!(
            "stage8b-p1e-config-boundary-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("supervisor.json");
        fs::write(&path, b"{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let uid = fs::metadata(&path).unwrap().uid();
        assert_eq!(read_protected_config(&path, uid).unwrap(), b"{}");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o620)).unwrap();
        assert!(read_protected_config(&path, uid).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let link = directory.join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_protected_config(&link, uid).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn protected_config_regular_to_fifo_replacement_is_bounded() {
        let directory = std::env::temp_dir().join(format!(
            "stage8b-p1e-config-fifo-race-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("supervisor.json");
        fs::write(&path, b"{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let uid = fs::metadata(&path).unwrap().uid();

        let started = Instant::now();
        let result = read_protected_config_with_before_open(&path, uid, || {
            fs::remove_file(&path).unwrap();
            let raw_path = CString::new(path.as_os_str().as_bytes()).unwrap();
            // SAFETY: `raw_path` is a live NUL-terminated path and mode is valid.
            assert_eq!(unsafe { libc::mkfifo(raw_path.as_ptr(), 0o600) }, 0);
        });

        assert!(matches!(
            result,
            Err(Stage8bP1eProcessErrorV1::ConfigBoundary)
        ));
        assert!(started.elapsed() < StdDuration::from_secs(1));
        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn run_rejects_missing_credential_before_redis_or_durable_effect() {
        let directory = temp_directory("run-guard-routing");
        let durable_parent = directory.join("durable");
        fs::create_dir(&durable_parent).unwrap();
        fs::set_permissions(&durable_parent, fs::Permissions::from_mode(0o700)).unwrap();
        let durable_parent = fs::canonicalize(durable_parent).unwrap();
        let config_path = directory.join("supervisor.json");
        fs::write(&config_path, supervisor_config_bytes(&durable_parent)).unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        let expected_uid = fs::metadata(&config_path).unwrap().uid();
        let boot_id_path = directory.join("boot_id");
        fs::write(&boot_id_path, b"01234567-89ab-cdef-0123-456789abcdef\n").unwrap();

        let result = execute_with_boundaries(
            Stage8bP1eProcessCommandV1::Run,
            &config_path,
            &boot_id_path,
            DateTime::<Utc>::from_timestamp_millis(1_785_759_000_000).unwrap(),
            expected_uid,
        )
        .await;

        assert!(matches!(result, Err(Stage8bP1eProcessErrorV1::Credential)));
        assert_eq!(fs::read_dir(&durable_parent).unwrap().count(), 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn authenticated_ready_restart_becomes_attachable_without_losing_owner() {
        let parent = temp_directory("ready-route");
        let (source, export_input, key, fresh) =
            strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let fingerprint = fresh.stage5c_config_fingerprint();
        let validated = crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent.clone(),
            fingerprint.clone(),
        ))
        .unwrap();
        let admin = crate::authorize_stage8b_p1_first_boot(
            &validated,
            crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
        )
        .unwrap();
        drop(
            crate::first_boot_stage8b_p1(
                validated,
                admin,
                source,
                export_input,
                &key,
                fresh.clone(),
            )
            .unwrap(),
        );

        let restart_config = crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent.clone(),
            fingerprint,
        ))
        .unwrap();
        let restart = crate::restart_stage8b_p1(restart_config, &key, fresh).unwrap();
        let Stage8bP1ePreRedisRestartV1::Attachable(owner) =
            stage8b_p1e_route_pre_redis_restart_v1(restart)
        else {
            panic!("authenticated Ready restart must remain attachable")
        };
        assert_eq!(owner.kind(), Stage8bP1eRestartKindV1::Ready);
        drop(owner);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn recovered_ready_conversion_rejects_a_non_ready_semantic_route() {
        let result = recovered_ready_owner(Stage8bP1eRecoveredReadyRouteV1::Semantic(
            Stage8bP1RedisSemanticOutcome::MultiIntentBlocked {
                semantic_batch_id_sha256: "11".repeat(32),
                intent_count: 2,
            },
        ));
        assert!(matches!(
            result,
            Err(Stage8bP1eStartupErrorV1::ReadyBoundaryInvariant)
        ));
    }

    #[test]
    fn signed_schedule_retry_policy_has_the_exact_pinned_backoff() {
        let policy = Stage8bP1eScheduleAcquisitionPolicyV1::production();
        assert!(policy.is_valid());
        assert_eq!(policy.attempts, 12);
        assert_eq!(policy.total_deadline, StdDuration::from_secs(60));
        assert_eq!(policy.redis_operation_timeout, StdDuration::from_secs(2));
        assert_eq!(
            (1..=11)
                .map(|attempt| policy.backoff_after(attempt).as_millis())
                .collect::<Vec<_>>(),
            vec![250, 500, 1_000, 2_000, 4_000, 5_000, 5_000, 5_000, 5_000, 5_000, 5_000]
        );
    }

    #[tokio::test]
    async fn s08_s09_schedule_free_source_drains_once_and_preserves_ready_owner() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("s08-s09-schedule-free");
        let (source, export_input, key, fresh) =
            strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let validated = crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent.clone(),
            fresh.stage5c_config_fingerprint(),
        ))
        .unwrap();
        let admin = crate::authorize_stage8b_p1_first_boot(
            &validated,
            crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
        )
        .unwrap();
        let first_boot =
            crate::first_boot_stage8b_p1(validated, admin, source, export_input, &key, fresh)
                .unwrap();
        let operational_identity_sha256 = first_boot.receipt().operational_identity_sha256.clone();
        let transport = crate::initialize_stage8b_p1_redis_namespace(
            &redis.url,
            crate::Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let mut owner =
            Stage8bP1RedisSemanticCompositionOwner::new(first_boot.into_owner(), transport);
        owner
            .transport_mut()
            .publish_canonical_m10(
                &canonical_m10(operational_identity_sha256.clone()),
                &operational_identity_sha256,
            )
            .await
            .unwrap();
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let ready = Stage8bP1eReadyPollingV1 {
            owner: Box::new(owner),
            control,
            committed_cancel_disposition: None,
        };
        let latch = Stage8bP1eShutdownLatchV1::new();

        let Stage8bP1eReadyPollOutcomeV1::ContinueSource(continuing) =
            poll_stage8b_p1e_ready_once_v1(ready, &latch).await.unwrap()
        else {
            panic!("one fresh ordinary source must cross the clear latch")
        };
        let Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(ready) =
            drain_stage8b_p1e_schedule_free_recovery_v1(continuing, &latch, &key)
                .await
                .unwrap()
        else {
            panic!("the ordinary zero-intent source must drain to Ready")
        };
        let Stage8bP1eReadyPollOutcomeV1::Empty(ready) =
            poll_stage8b_p1e_ready_once_v1(ready, &latch).await.unwrap()
        else {
            panic!("the next bounded fresh poll must preserve the Ready owner")
        };

        let intent = Stage8bP1eShutdownIntentV1::new(
            crate::Stage8bP1eShutdownCauseV1::ExternalSignal,
            10_000,
            1,
        );
        assert!(latch.request(intent.clone()));
        let Stage8bP1eReadyPollOutcomeV1::Stopped(stopped) =
            poll_stage8b_p1e_ready_once_v1(ready, &latch).await.unwrap()
        else {
            panic!("preset shutdown must destroy Ready authority before Redis read")
        };
        assert_eq!(stopped.shutdown_intent(), &intent);
        drop(stopped);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn s05_s09_startup_owner_loop_observes_concurrent_shutdown_during_bounded_poll() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("s09-concurrent-shutdown");
        let (source, export_input, key, fresh) =
            strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let runtime_config_fingerprint_sha256 = fresh.stage5c_config_fingerprint().to_string();
        let validated = crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent.clone(),
            fresh.stage5c_config_fingerprint(),
        ))
        .unwrap();
        let admin = crate::authorize_stage8b_p1_first_boot(
            &validated,
            crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
        )
        .unwrap();
        let first_boot =
            crate::first_boot_stage8b_p1(validated, admin, source, export_input, &key, fresh)
                .unwrap();
        let operational_identity_sha256 = first_boot.receipt().operational_identity_sha256.clone();
        let transport = crate::initialize_stage8b_p1_redis_namespace(
            &redis.url,
            crate::Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let startup = Stage8bP1eStartupOwnerV1 {
            kind: Stage8bP1eStartupOwnerKindV1::ReadyNoPending,
            route: Box::new(Stage8bP1eStartupOwnerRouteV1::ReadyPending(
                Stage8bP1eReadyPendingAcquisitionOutcomeV1::NoPending(Box::new(
                    Stage8bP1RedisSemanticCompositionOwner::new(first_boot.into_owner(), transport),
                )),
            )),
            control: crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url)
                .await,
        };
        let latch = Stage8bP1eShutdownLatchV1::new();
        let mut reader = crate::Stage8bP1eRedisScheduleReader::connect(&redis.url)
            .await
            .unwrap();
        let mut context = schedule_context(
            operational_identity_sha256,
            runtime_config_fingerprint_sha256,
            DateTime::<Utc>::from_timestamp_millis(1_785_759_000_000).unwrap(),
        );
        let intent = Stage8bP1eShutdownIntentV1::new(
            crate::Stage8bP1eShutdownCauseV1::ExternalSignal,
            20_000,
            1,
        );
        let runner =
            run_stage8b_p1e_startup_owner_loop_v1(startup, &mut reader, &mut context, &latch, &key);
        let signal = async {
            tokio::time::sleep(StdDuration::from_millis(50)).await;
            assert!(latch.request(intent.clone()));
        };
        let (outcome, ()) = tokio::join!(runner, signal);
        let Stage8bP1eOwnerLoopOutcomeV1::StoppedReady(stopped) = outcome.unwrap() else {
            panic!("concurrent shutdown must stop the retained Ready owner")
        };
        assert_eq!(stopped.shutdown_intent(), &intent);
        assert!(!latch.request(Stage8bP1eShutdownIntentV1::new(
            crate::Stage8bP1eShutdownCauseV1::TelemetryFailure,
            30_000,
            2,
        )));
        drop(stopped);
        fs::remove_dir_all(parent).unwrap();
    }

    #[tokio::test]
    async fn owner_loop_trusted_clock_advances_monotonically_across_idle_time() {
        let anchor = DateTime::<Utc>::from_timestamp_millis(1_785_759_000_000).unwrap();
        let clock = Stage8bP1eOwnerLoopClockV1::new(anchor);

        let first = clock.trusted_now().unwrap();
        tokio::time::sleep(StdDuration::from_millis(10)).await;
        let second = clock.trusted_now().unwrap();

        assert!(first >= anchor);
        assert!(second > first);
    }

    #[tokio::test]
    async fn s05_pending_not_claimable_stays_startup_typed_without_schedule_read() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("s05-pending-not-claimable");
        let (source, export_input, key, fresh) =
            strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let validated = crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent.clone(),
            fresh.stage5c_config_fingerprint(),
        ))
        .unwrap();
        let admin = crate::authorize_stage8b_p1_first_boot(
            &validated,
            crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
        )
        .unwrap();
        let first_boot =
            crate::first_boot_stage8b_p1(validated, admin, source, export_input, &key, fresh)
                .unwrap();
        let transport = crate::initialize_stage8b_p1_redis_namespace(
            &redis.url,
            crate::Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let startup = Stage8bP1eStartupOwnerV1 {
            kind: Stage8bP1eStartupOwnerKindV1::ReadyPendingNotClaimable,
            route: Box::new(Stage8bP1eStartupOwnerRouteV1::ReadyPending(
                Stage8bP1eReadyPendingAcquisitionOutcomeV1::PendingNotClaimable {
                    owner: Box::new(Stage8bP1RedisSemanticCompositionOwner::new(
                        first_boot.into_owner(),
                        transport,
                    )),
                    pending_m10_redis_id: "1785759000000-0".to_string(),
                },
            )),
            control: crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url)
                .await,
        };
        let mut reader = crate::Stage8bP1eRedisScheduleReader::connect(&redis.url)
            .await
            .unwrap();
        let mut context = schedule_context(
            "00".repeat(32),
            "11".repeat(32),
            DateTime::<Utc>::from_timestamp_millis(1_785_759_000_000).unwrap(),
        );

        let outcome = run_stage8b_p1e_startup_owner_loop_v1(
            startup,
            &mut reader,
            &mut context,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await
        .unwrap();
        let Stage8bP1eOwnerLoopOutcomeV1::StartupPendingNotClaimable(pending) = outcome else {
            panic!("startup PEL threshold must remain a startup-typed boundary")
        };
        assert_eq!(pending.pending_m10_redis_id(), "1785759000000-0");
        assert_eq!(reader.test_read_attempts(), 0);
        drop(pending);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    async fn generated_market_deferred_without_successor(
        redis_url: &str,
        parent: &Path,
    ) -> (
        Stage8bP1eScheduleDeferredRecoveryV1,
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        String,
        i64,
    ) {
        let (pending, key, fresh, identity, successor_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_generated_market_prepublication_without_successor(
                redis_url, parent,
            )
            .await;
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(redis_url).await;
        let lifecycle = classify_recovered_semantic_outcome(
            Stage8bP1RedisSemanticOutcome::Prepublication(Box::new(pending)),
            control,
        );
        let Stage8bP1eScheduleFreeDrainOutcomeV1::ScheduleDeferred(deferred) =
            drain_stage8b_p1e_recovery_lifecycle_v1(
                lifecycle,
                &Stage8bP1eShutdownLatchV1::new(),
                &key,
            )
            .await
            .unwrap()
        else {
            panic!("Working generated Market must use reservation-bearing publication")
        };
        (deferred, key, fresh, identity, successor_close_ms)
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn generated_working_prepublication_uses_reserved_route_and_waits_for_successor() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("generated-working-process-waiting-successor");
        let (deferred, key, fresh, identity, successor_close_ms) =
            generated_market_deferred_without_successor(&redis.url, &parent).await;
        let latch = Stage8bP1eShutdownLatchV1::new();
        assert_eq!(
            deferred.kind(),
            Stage8bP1eScheduleDeferredKindV1::CommandPublishedGeneratedMarket
        );

        let namespace = crate::stage8b_p1_redis_namespace();
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let commands_after_publication: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            commands_after_publication, 1,
            "generated process route must publish one reserved command"
        );

        let trusted_now = DateTime::<Utc>::from_timestamp_millis(successor_close_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity.clone(),
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let mut context = fixture.context.clone();
        let one_attempt = Stage8bP1eScheduleAcquisitionPolicyV1 {
            attempts: 1,
            total_deadline: StdDuration::from_millis(100),
            redis_operation_timeout: StdDuration::from_millis(20),
            initial_backoff: StdDuration::from_millis(1),
            maximum_backoff: StdDuration::from_millis(2),
        };
        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Exhausted(deferred) =
            advance_stage8b_p1e_supported_schedule_with_policy_v1(
                deferred,
                &mut reader,
                &mut context,
                &latch,
                &key,
                one_attempt,
            )
            .await
            .unwrap()
        else {
            panic!("a verified schedule without its successor must retain the typed owner")
        };
        assert_eq!(
            deferred.kind(),
            Stage8bP1eScheduleDeferredKindV1::CommandPublishedGeneratedMarket
        );
        assert!(context.high_water.is_none());
        assert_eq!(reader.test_read_attempts(), 1);

        context.trusted_now += chrono::Duration::milliseconds(1);
        let revised = crate::stage8b_p1e_schedule_source::tests::p1e_test_revised_schedule_envelope(
            &fixture.bytes,
            2,
            2,
            context.trusted_now,
        );
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(revised)
            .query_async(&mut connection)
            .await
            .unwrap();
        let successor_payload = canonical_m10_at(identity, successor_close_ms, "2175");
        let successor_id: String = redis::cmd("XADD")
            .arg(&namespace.canonical_m10_stream)
            .arg(format!("{successor_close_ms}-0"))
            .arg("payload")
            .arg(successor_payload)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(successor_id, format!("{successor_close_ms}-0"));

        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Ready(ready) =
            advance_stage8b_p1e_supported_schedule_with_policy_v1(
                deferred,
                &mut reader,
                &mut context,
                &latch,
                &key,
                Stage8bP1eScheduleAcquisitionPolicyV1 {
                    attempts: 2,
                    total_deadline: StdDuration::from_millis(200),
                    redis_operation_timeout: StdDuration::from_millis(20),
                    initial_backoff: StdDuration::from_millis(1),
                    maximum_backoff: StdDuration::from_millis(2),
                },
            )
            .await
            .unwrap()
        else {
            panic!("the same generated owner must continue after its exact successor arrives")
        };
        assert_eq!(
            reader.test_read_attempts(),
            2,
            "schedule must be reread after waiting"
        );
        let high_water = context.high_water.as_ref().expect("V4 high-water");
        assert_eq!(high_water.publication_sequence(), 2);
        assert_eq!(high_water.semantic_revision(), 2);
        let commands_after_completion: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            commands_after_completion, commands_after_publication,
            "successor waiting must not republish the generated command"
        );
        let pending_after: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_after.count(), 0, "S_truth must precede XACK-last");
        drop(ready);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn generated_successor_gap_payload_mismatch_and_missing_predecessor_fail_closed() {
        for case in ["gap", "payload-mismatch", "missing-predecessor"] {
            let redis = RedisServer::start().await;
            let parent = temp_directory(&format!("generated-successor-{case}"));
            let (deferred, key, fresh, identity, successor_close_ms) =
                generated_market_deferred_without_successor(&redis.url, &parent).await;
            let namespace = crate::stage8b_p1_redis_namespace();
            let mut connection = redis::aio::ConnectionManager::new(
                redis::Client::open(redis.url.as_str()).unwrap(),
            )
            .await
            .unwrap();
            let trusted_now = DateTime::<Utc>::from_timestamp_millis(successor_close_ms).unwrap();
            let fixture =
                crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
                    identity.clone(),
                    fresh.stage5c_config_fingerprint(),
                    crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                    trusted_now,
                );
            let _: String = redis::cmd("XADD")
                .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
                .arg("*")
                .arg("payload")
                .arg(&fixture.bytes)
                .query_async(&mut connection)
                .await
                .unwrap();

            match case {
                "gap" => {
                    let gap_close_ms = successor_close_ms + 600_000;
                    let payload = canonical_m10_at(identity.clone(), gap_close_ms, "2175");
                    let _: String = redis::cmd("XADD")
                        .arg(&namespace.canonical_m10_stream)
                        .arg(format!("{gap_close_ms}-0"))
                        .arg("payload")
                        .arg(payload)
                        .query_async(&mut connection)
                        .await
                        .unwrap();
                }
                "payload-mismatch" => {
                    let payload = canonical_m10_at("f".repeat(64), successor_close_ms, "2175");
                    let _: String = redis::cmd("XADD")
                        .arg(&namespace.canonical_m10_stream)
                        .arg(format!("{successor_close_ms}-0"))
                        .arg("payload")
                        .arg(payload)
                        .query_async(&mut connection)
                        .await
                        .unwrap();
                }
                "missing-predecessor" => {
                    let predecessor_close_ms = successor_close_ms - 600_000;
                    let removed: usize = redis::cmd("XDEL")
                        .arg(&namespace.canonical_m10_stream)
                        .arg(format!("{predecessor_close_ms}-0"))
                        .query_async(&mut connection)
                        .await
                        .unwrap();
                    assert_eq!(removed, 1);
                }
                _ => unreachable!(),
            }

            let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
                &redis.url,
                fixture.public_key_hex,
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .await
            .unwrap();
            let mut context = fixture.context;
            let error = match advance_stage8b_p1e_supported_schedule_with_policy_v1(
                deferred,
                &mut reader,
                &mut context,
                &Stage8bP1eShutdownLatchV1::new(),
                &key,
                Stage8bP1eScheduleAcquisitionPolicyV1 {
                    attempts: 3,
                    total_deadline: StdDuration::from_millis(200),
                    redis_operation_timeout: StdDuration::from_millis(20),
                    initial_backoff: StdDuration::from_millis(1),
                    maximum_backoff: StdDuration::from_millis(2),
                },
            )
            .await
            {
                Ok(_) => {
                    panic!("an observed invalid source state must not become successor waiting")
                }
                Err(error) => error,
            };
            match case {
                "gap" => assert!(matches!(
                    error,
                    Stage8bP1eStartupErrorV1::Source(
                        Stage8bP1RedisSemanticError::ExactSourceConflict
                    )
                )),
                "payload-mismatch" => assert!(matches!(
                    error,
                    Stage8bP1eStartupErrorV1::Source(Stage8bP1RedisSemanticError::CanonicalM10(
                        crate::Stage8bP1CanonicalM10Error::IdentityMismatch
                    ))
                )),
                "missing-predecessor" => assert!(matches!(
                    error,
                    Stage8bP1eStartupErrorV1::Source(
                        Stage8bP1RedisSemanticError::ExactPendingEntryMissing
                    )
                )),
                _ => unreachable!(),
            }
            assert_eq!(
                reader.test_read_attempts(),
                1,
                "{case} must fail on its first observed invalid source state"
            );
            let commands: usize = redis::cmd("XLEN")
                .arg(&namespace.canonical_command_stream)
                .query_async(&mut connection)
                .await
                .unwrap();
            assert_eq!(commands, 1, "{case} must not republish the command");
            drop(error);
            fs::remove_dir_all(parent).unwrap();
        }
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn generated_successor_wait_observes_shutdown_and_preserves_cause() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("generated-successor-wait-stop");
        let (deferred, key, fresh, identity, successor_close_ms) =
            generated_market_deferred_without_successor(&redis.url, &parent).await;
        let namespace = crate::stage8b_p1_redis_namespace();
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(successor_close_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let mut context = fixture.context;
        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Exhausted(deferred) =
            advance_stage8b_p1e_supported_schedule_with_policy_v1(
                deferred,
                &mut reader,
                &mut context,
                &Stage8bP1eShutdownLatchV1::new(),
                &key,
                Stage8bP1eScheduleAcquisitionPolicyV1 {
                    attempts: 1,
                    total_deadline: StdDuration::from_millis(100),
                    redis_operation_timeout: StdDuration::from_millis(20),
                    initial_backoff: StdDuration::from_millis(1),
                    maximum_backoff: StdDuration::from_millis(2),
                },
            )
            .await
            .unwrap()
        else {
            panic!("missing successor must retain the generated owner")
        };

        let latch = Stage8bP1eShutdownLatchV1::new();
        let intent = Stage8bP1eShutdownIntentV1::new(
            crate::Stage8bP1eShutdownCauseV1::ExternalSignal,
            30_000,
            9,
        );
        assert!(latch.request(intent.clone()));
        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Stopped(stopped) =
            advance_stage8b_p1e_supported_schedule_with_policy_v1(
                deferred,
                &mut reader,
                &mut context,
                &latch,
                &key,
                Stage8bP1eScheduleAcquisitionPolicyV1 {
                    attempts: 1,
                    total_deadline: StdDuration::from_millis(100),
                    redis_operation_timeout: StdDuration::from_millis(20),
                    initial_backoff: StdDuration::from_millis(1),
                    maximum_backoff: StdDuration::from_millis(2),
                },
            )
            .await
            .unwrap()
        else {
            panic!("shutdown must stop the retained successor-wait owner")
        };
        assert_eq!(stopped.receipt().shutdown_intent(), &intent);
        assert_eq!(
            stopped.receipt().checkpoint(),
            crate::Stage8bP1eScheduleLatchCheckpointV1::BeforeScheduleRead
        );
        assert_eq!(reader.test_read_attempts(), 1);
        let pending: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 1);
        let commands: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(commands, 1);
        drop(stopped);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn generated_prepublication_shutdown_retains_before_reserved_publication() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("generated-prepublication-stop");
        let (pending, key, _, _, _) =
            crate::stage8b_p1_semantic::p1e_test_generated_market_prepublication_without_successor(
                &redis.url, &parent,
            )
            .await;
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let lifecycle = classify_recovered_semantic_outcome(
            Stage8bP1RedisSemanticOutcome::Prepublication(Box::new(pending)),
            control,
        );
        let latch = Stage8bP1eShutdownLatchV1::new();
        let intent = Stage8bP1eShutdownIntentV1::new(
            crate::Stage8bP1eShutdownCauseV1::ExternalSignal,
            20_000,
            1,
        );
        assert!(latch.request(intent.clone()));
        let Stage8bP1eScheduleFreeDrainOutcomeV1::RetainedForRestart(retained) =
            drain_stage8b_p1e_recovery_lifecycle_v1(lifecycle, &latch, &key)
                .await
                .unwrap()
        else {
            panic!("shutdown must win before generated publication")
        };
        assert_eq!(
            retained.kind(),
            Stage8bP1eRecoveryBoundaryKindV1::Prepublication
        );
        assert_eq!(retained.shutdown_intent(), &intent);

        let namespace = crate::stage8b_p1_redis_namespace();
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let commands: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            commands, 0,
            "stop-before-publication must not XADD a command"
        );
        let pending_source: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_source.count(), 1);
        drop(retained);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn bounded_schedule_acquisition_exhausts_without_losing_owner_or_xacking_m10() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("signed-schedule-exhausted");
        let (published, key, fresh, identity, candidate_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_plain_market_published(&redis.url, &parent).await;
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let deferred = Stage8bP1eScheduleDeferredRecoveryV1 {
            kind: Stage8bP1eScheduleDeferredKindV1::CommandPublishedMarket,
            _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                Box::new(published),
            )),
            control,
        };
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(candidate_close_ms).unwrap();
        let mut context =
            schedule_context(identity, fresh.stage5c_config_fingerprint(), trusted_now);
        let mut reader = crate::Stage8bP1eRedisScheduleReader::connect(&redis.url)
            .await
            .unwrap();
        let policy = Stage8bP1eScheduleAcquisitionPolicyV1 {
            attempts: 3,
            total_deadline: StdDuration::from_millis(100),
            redis_operation_timeout: StdDuration::from_millis(20),
            initial_backoff: StdDuration::from_millis(1),
            maximum_backoff: StdDuration::from_millis(2),
        };

        let outcome = advance_stage8b_p1e_supported_schedule_with_policy_v1(
            deferred,
            &mut reader,
            &mut context,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
            policy,
        )
        .await
        .unwrap();
        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Exhausted(deferred) = outcome else {
            panic!("an empty schedule stream must exhaust with the same owner")
        };
        assert_eq!(
            deferred.kind(),
            Stage8bP1eScheduleDeferredKindV1::CommandPublishedMarket
        );
        assert_eq!(reader.test_read_attempts(), 3);

        let namespace = crate::stage8b_p1_redis_namespace();
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let pending: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            pending.count(),
            1,
            "schedule exhaustion must retain M10 PEL"
        );
        drop(deferred);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn bounded_signed_market_cycle_rejoins_ready_and_xacks_source_last() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("signed-schedule-bounded-success");
        let (published, key, fresh, identity, candidate_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_plain_market_published(&redis.url, &parent).await;
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(candidate_close_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let deferred = Stage8bP1eScheduleDeferredRecoveryV1 {
            kind: Stage8bP1eScheduleDeferredKindV1::CommandPublishedMarket,
            _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                Box::new(published),
            )),
            control,
        };
        let mut context = fixture.context.clone();
        let fixture_public_key_hex = fixture.public_key_hex.clone();
        let fixture_key_valid_from = fixture.key_valid_from;
        let fixture_key_valid_until = fixture.key_valid_until;
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();

        let outcome = advance_stage8b_p1e_supported_schedule_bounded_v1(
            deferred,
            &mut reader,
            &mut context,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await
        .unwrap();
        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Ready(ready) = outcome else {
            panic!("fresh signed Market schedule must rejoin exact Ready")
        };
        assert_eq!(reader.test_read_attempts(), 1);
        assert!(context.high_water.is_some());
        let accepted_high_water = context.high_water.clone();
        let mut restarted_context = fixture.context;
        ready
            .test_restore_durable_schedule_high_water_with_key(
                &mut restarted_context,
                &fixture_public_key_hex,
                fixture_key_valid_from,
                fixture_key_valid_until,
            )
            .unwrap();
        assert_eq!(restarted_context.high_water, accepted_high_water);

        let namespace = crate::stage8b_p1_redis_namespace();
        let pending: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0, "S_truth must precede source XACK-last");
        drop(ready);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn bounded_signed_initial_limit_cycle_rejoins_ready_and_xacks_source_last() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("signed-schedule-initial-limit-bounded-success");
        let (published, key, fresh, identity, candidate_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_initial_limit_published(&redis.url, &parent).await;
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(candidate_close_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let deferred = Stage8bP1eScheduleDeferredRecoveryV1 {
            kind: Stage8bP1eScheduleDeferredKindV1::CommandPublishedInitialLimit,
            _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                Box::new(published),
            )),
            control,
        };
        let mut context = fixture.context.clone();
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();

        let outcome = advance_stage8b_p1e_supported_schedule_bounded_v1(
            deferred,
            &mut reader,
            &mut context,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await
        .unwrap();
        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Ready(ready) = outcome else {
            panic!("fresh signed initial LIMIT schedule must rejoin exact Ready")
        };
        assert_eq!(reader.test_read_attempts(), 1);
        assert!(context.high_water.is_some());

        let namespace = crate::stage8b_p1_redis_namespace();
        let pending: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0, "S_truth must precede source XACK-last");
        drop(ready);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn bounded_signed_cancel_cycle_rejoins_ready_and_xacks_source_last() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("signed-schedule-cancel-bounded-success");
        let (published, key, fresh, identity, candidate_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_cancel_published(&redis.url, &parent).await;
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(candidate_close_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let deferred = Stage8bP1eScheduleDeferredRecoveryV1 {
            kind: Stage8bP1eScheduleDeferredKindV1::CommandPublishedCancel,
            _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                Box::new(published),
            )),
            control,
        };
        let mut context = fixture.context;
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();

        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Ready(ready) =
            advance_stage8b_p1e_supported_schedule_bounded_v1(
                deferred,
                &mut reader,
                &mut context,
                &Stage8bP1eShutdownLatchV1::new(),
                &key,
            )
            .await
            .unwrap()
        else {
            panic!("fresh signed CANCEL must drain through replacement truth to Ready")
        };
        assert_eq!(reader.test_read_attempts(), 1);
        assert!(context.high_water.is_some());
        let namespace = crate::stage8b_p1_redis_namespace();
        let pending: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0, "cancel truth must precede XACK-last");
        let commands: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(commands, 1, "signed composition must not republish CANCEL");
        assert!(ready.is_committed_cancel_resolution());
        let resolved = ready.into_committed_cancel_resolved().unwrap();
        assert_eq!(
            resolved.disposition(),
            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
        );
        let next = poll_stage8b_p1e_ready_once_v1(
            resolved.into_ready_polling(),
            &Stage8bP1eShutdownLatchV1::new(),
        )
        .await
        .unwrap();
        let Stage8bP1eReadyPollOutcomeV1::ContinueSource(next) = next else {
            panic!("fresh CANCEL completion must hand its owner to the next canonical M10")
        };
        let next = drain_stage8b_p1e_schedule_free_recovery_v1(
            next,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await
        .unwrap();
        assert!(matches!(
            next,
            Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(_)
                | Stage8bP1eScheduleFreeDrainOutcomeV1::ScheduleDeferred(_)
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn day_expiry_rejects_conflict_same_revision_and_rollbacks_before_effect() {
        use strategy_runtime_core::Stage8bP1eScheduleSourceError::{ProgressionConflict, Rollback};

        assert_day_expiry_progression_rejected(
            "signed-schedule-day-expiry-conflicting-one-one",
            1,
            1,
            1,
            1,
            ProgressionConflict,
        )
        .await;
        assert_day_expiry_progression_rejected(
            "signed-schedule-day-expiry-same-revision-new-hash",
            1,
            1,
            2,
            1,
            ProgressionConflict,
        )
        .await;
        assert_day_expiry_progression_rejected(
            "signed-schedule-day-expiry-sequence-rollback",
            2,
            2,
            1,
            2,
            Rollback,
        )
        .await;
        assert_day_expiry_progression_rejected(
            "signed-schedule-day-expiry-revision-rollback",
            2,
            2,
            3,
            1,
            Rollback,
        )
        .await;
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn day_expiry_admission_rejects_conflicting_external_high_water() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("signed-schedule-day-expiry-context-conflict");
        let (owner, _key, fresh, identity, boundary_ms) =
            crate::stage8b_p1_semantic::p1e_test_day_expiry_ready(&redis.url, &parent).await;
        let boundary = DateTime::<Utc>::from_timestamp_millis(boundary_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_closed_schedule_envelope(
            identity.clone(),
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            boundary,
        );
        let conflicting =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_snapshot(
                identity,
                fresh.stage5c_config_fingerprint(),
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                format!("{boundary_ms}-conflict"),
                boundary,
            )
            .high_water()
            .clone();
        let mut context = fixture.context;
        context.high_water = Some(conflicting.clone());
        let durable_before = durable_file_snapshot(&parent);
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let result = Stage8bP1eReadyPollingV1 {
            owner: Box::new(owner),
            control,
            committed_cancel_disposition: None,
        }
        .test_into_day_expiry_schedule_with_key(
            &mut context,
            &fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        );
        assert!(matches!(
            result,
            Err(Stage8bP1eStartupErrorV1::Source(
                Stage8bP1RedisSemanticError::P1eScheduleHighWaterConflict
            ))
        ));
        assert_eq!(context.high_water.as_ref(), Some(&conflicting));
        assert_eq!(durable_file_snapshot(&parent), durable_before);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn signed_day_expiry_commits_terminal_book_without_source_or_xack() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("signed-schedule-day-expiry-success");
        let (owner, key, fresh, identity, boundary_ms) =
            crate::stage8b_p1_semantic::p1e_test_day_expiry_ready(&redis.url, &parent).await;
        let boundary = DateTime::<Utc>::from_timestamp_millis(boundary_ms).unwrap();
        let mut fixture =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_closed_schedule_envelope(
                identity,
                fresh.stage5c_config_fingerprint(),
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                boundary,
            );
        fixture.bytes =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_revised_schedule_envelope(
                &fixture.bytes,
                2,
                2,
                boundary,
            );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let namespace = crate::stage8b_p1_redis_namespace();
        let pending_before: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            pending_before.count(),
            0,
            "Day expiry must begin source-free"
        );
        let schedule_rows_before: usize = redis::cmd("XLEN")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            schedule_rows_before, 0,
            "retention may remove prior schedule rows without erasing durable progression"
        );
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let mut context = fixture.context.clone();
        let deferred = Stage8bP1eReadyPollingV1 {
            owner: Box::new(owner),
            control,
            committed_cancel_disposition: None,
        }
        .test_into_day_expiry_schedule_with_key(
            &mut context,
            &fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let prior_high_water = context
            .high_water
            .clone()
            .expect("Day-expiry admission must restore durable Open high-water");
        assert_eq!(prior_high_water.publication_sequence(), 1);
        assert_eq!(prior_high_water.semantic_revision(), 1);
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            deferred.kind(),
            Stage8bP1eScheduleDeferredKindV1::ReadyDayExpiry
        );
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();

        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Ready(ready) =
            advance_stage8b_p1e_supported_schedule_bounded_v1(
                deferred,
                &mut reader,
                &mut context,
                &Stage8bP1eShutdownLatchV1::new(),
                &key,
            )
            .await
            .unwrap()
        else {
            panic!("fresh Closed schedule must commit exact terminal Day expiry")
        };
        assert_eq!(reader.test_read_attempts(), 1);
        let committed_high_water = context
            .high_water
            .as_ref()
            .expect("valid Closed progression must advance high-water");
        assert_eq!(committed_high_water.publication_sequence(), 2);
        assert_eq!(committed_high_water.semantic_revision(), 2);
        assert!(
            !ready.owner.requires_later_limit_evaluation(),
            "Day expiry must leave a terminal working-book projection"
        );
        let pending_after: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_after.count(), 0, "source-free expiry cannot XACK");
        drop(ready);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn missing_day_expiry_schedule_retains_exact_ready_owner_until_retry() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("signed-schedule-day-expiry-missing-retry");
        let (owner, key, fresh, identity, boundary_ms) =
            crate::stage8b_p1_semantic::p1e_test_day_expiry_ready(&redis.url, &parent).await;
        let boundary = DateTime::<Utc>::from_timestamp_millis(boundary_ms).unwrap();
        let mut fixture =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_closed_schedule_envelope(
                identity,
                fresh.stage5c_config_fingerprint(),
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                boundary,
            );
        fixture.bytes =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_revised_schedule_envelope(
                &fixture.bytes,
                2,
                2,
                boundary,
            );
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let mut context = fixture.context.clone();
        let deferred = Stage8bP1eReadyPollingV1 {
            owner: Box::new(owner),
            control,
            committed_cancel_disposition: None,
        }
        .test_into_day_expiry_schedule_with_key(
            &mut context,
            &fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let prior_high_water = context
            .high_water
            .clone()
            .expect("missing schedule must still start from durable high-water");
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let one_attempt = Stage8bP1eScheduleAcquisitionPolicyV1 {
            attempts: 1,
            total_deadline: StdDuration::from_millis(100),
            redis_operation_timeout: StdDuration::from_millis(20),
            initial_backoff: StdDuration::from_millis(1),
            maximum_backoff: StdDuration::from_millis(2),
        };
        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Exhausted(deferred) =
            advance_stage8b_p1e_supported_schedule_with_policy_v1(
                deferred,
                &mut reader,
                &mut context,
                &Stage8bP1eShutdownLatchV1::new(),
                &key,
                one_attempt,
            )
            .await
            .unwrap()
        else {
            panic!("missing Closed schedule must retain exact Day-expiry owner")
        };
        assert_eq!(
            deferred.kind(),
            Stage8bP1eScheduleDeferredKindV1::ReadyDayExpiry
        );
        assert_eq!(context.high_water.as_ref(), Some(&prior_high_water));
        assert_eq!(reader.test_read_attempts(), 1);

        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Ready(ready) =
            advance_stage8b_p1e_supported_schedule_with_policy_v1(
                deferred,
                &mut reader,
                &mut context,
                &Stage8bP1eShutdownLatchV1::new(),
                &key,
                one_attempt,
            )
            .await
            .unwrap()
        else {
            panic!("retained owner must continue after exact Closed schedule arrives")
        };
        assert_eq!(
            context.high_water.as_ref().unwrap().publication_sequence(),
            2
        );
        assert_eq!(context.high_water.as_ref().unwrap().semantic_revision(), 2);
        assert!(
            !ready.owner.requires_later_limit_evaluation(),
            "retried Day expiry must become terminal exactly once"
        );
        let namespace = crate::stage8b_p1_redis_namespace();
        let pending: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0);
        drop(ready);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn open_schedule_cannot_authorize_day_expiry() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("signed-schedule-day-expiry-open-denied");
        let (owner, key, fresh, identity, boundary_ms) =
            crate::stage8b_p1_semantic::p1e_test_day_expiry_ready(&redis.url, &parent).await;
        let boundary = DateTime::<Utc>::from_timestamp_millis(boundary_ms).unwrap();
        let mut fixture =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
                identity,
                fresh.stage5c_config_fingerprint(),
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                boundary,
            );
        fixture.bytes =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_revised_schedule_envelope(
                &fixture.bytes,
                2,
                2,
                boundary,
            );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let mut context = fixture.context.clone();
        let deferred = Stage8bP1eReadyPollingV1 {
            owner: Box::new(owner),
            control,
            committed_cancel_disposition: None,
        }
        .test_into_day_expiry_schedule_with_key(
            &mut context,
            &fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let prior_high_water = context.high_water.clone().unwrap();
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let result = advance_stage8b_p1e_supported_schedule_with_policy_v1(
            deferred,
            &mut reader,
            &mut context,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
            Stage8bP1eScheduleAcquisitionPolicyV1 {
                attempts: 1,
                total_deadline: StdDuration::from_millis(100),
                redis_operation_timeout: StdDuration::from_millis(20),
                initial_backoff: StdDuration::from_millis(1),
                maximum_backoff: StdDuration::from_millis(2),
            },
        )
        .await;
        assert!(matches!(
            result,
            Err(Stage8bP1eStartupErrorV1::Source(
                Stage8bP1RedisSemanticError::P1eSchedule(
                    crate::Stage8bP1eScheduleReadError::Source(
                        strategy_runtime_core::Stage8bP1eScheduleSourceError::RouteDenied
                    )
                )
            ))
        ));
        assert_eq!(context.high_water.as_ref(), Some(&prior_high_water));
        let namespace = crate::stage8b_p1_redis_namespace();
        let pending: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0, "denied expiry must remain source-free");
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn signed_cancel_waits_for_successor_without_republish_or_high_water_advance() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("signed-schedule-cancel-successor-wait");
        let (published, key, fresh, identity, candidate_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_cancel_published_without_successor(
                &redis.url, &parent,
            )
            .await;
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(candidate_close_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity.clone(),
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let deferred = Stage8bP1eScheduleDeferredRecoveryV1 {
            kind: Stage8bP1eScheduleDeferredKindV1::CommandPublishedCancel,
            _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                Box::new(published),
            )),
            control,
        };
        let mut context = fixture.context;
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let policy = Stage8bP1eScheduleAcquisitionPolicyV1 {
            attempts: 1,
            total_deadline: StdDuration::from_millis(100),
            redis_operation_timeout: StdDuration::from_millis(20),
            initial_backoff: StdDuration::from_millis(1),
            maximum_backoff: StdDuration::from_millis(2),
        };
        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Exhausted(deferred) =
            advance_stage8b_p1e_supported_schedule_with_policy_v1(
                deferred,
                &mut reader,
                &mut context,
                &Stage8bP1eShutdownLatchV1::new(),
                &key,
                policy,
            )
            .await
            .unwrap()
        else {
            panic!("missing cancel successor must retain the exact published owner")
        };
        assert_eq!(
            deferred.kind(),
            Stage8bP1eScheduleDeferredKindV1::CommandPublishedCancel
        );
        assert!(context.high_water.is_none());
        let namespace = crate::stage8b_p1_redis_namespace();
        let commands_before: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(commands_before, 1);
        let mut publisher = crate::attach_stage8b_p1_redis(
            &redis.url,
            crate::Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        publisher
            .publish_canonical_m10(
                &canonical_m10_at(identity.clone(), candidate_close_ms, "2225"),
                &identity,
            )
            .await
            .unwrap();

        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Ready(ready) =
            advance_stage8b_p1e_supported_schedule_with_policy_v1(
                deferred,
                &mut reader,
                &mut context,
                &Stage8bP1eShutdownLatchV1::new(),
                &key,
                policy,
            )
            .await
            .unwrap()
        else {
            panic!("the retained CANCEL owner must complete after successor arrival")
        };
        assert!(context.high_water.is_some());
        let commands_after: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            commands_after, commands_before,
            "CANCEL must not be republished"
        );
        let pending: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0);
        drop(ready);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn committed_initial_limit_restart_revalidates_without_schedule_reread_or_republish() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("committed-initial-limit-restart");
        let (published, key, fresh, identity_sha256, candidate_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_initial_limit_published(&redis.url, &parent).await;
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(candidate_close_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity_sha256,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut binding_reader =
            crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
                &redis.url,
                fixture.public_key_hex.clone(),
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .await
            .unwrap();
        let snapshot = match binding_reader
            .read_newest_guarded(&fixture.context, &Stage8bP1eShutdownLatchV1::new())
            .await
            .unwrap()
        {
            crate::Stage8bP1eGuardedScheduleReadV1::Read(
                crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
            ) => snapshot,
            _ => panic!("fixture schedule must produce one verified Initial LIMIT snapshot"),
        };
        let expected_high_water = snapshot.high_water().clone();
        let _binding_receipt = crate::stage8b_p1_semantic::p1e_test_commit_initial_limit_v4_only(
            published,
            *snapshot,
            trusted_now,
            &key,
        )
        .await;
        assert_eq!(binding_reader.test_read_attempts(), 1);

        let namespace = crate::stage8b_p1_redis_namespace();
        let commands_before: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(commands_before, 1);
        let pending_before: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_before.count(), 1);

        let operational_identity =
            operational_identity_config(parent.clone(), fresh.stage5c_config_fingerprint());
        let root_name =
            crate::Stage7bDurableRootAuthority::expected_directory_name(&operational_identity)
                .unwrap();
        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity,
            &key,
            fresh,
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let Stage8bP1ePreRedisRestartV1::Attachable(restart) =
            stage8b_p1e_route_pre_redis_restart_v1(restart)
        else {
            panic!("authenticated Initial LIMIT V4 restart must remain attachable")
        };
        assert_eq!(
            restart.kind(),
            Stage8bP1eRestartKindV1::P1eScheduleBindingCommitted
        );
        let session =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_verified_redis_session_v1(&redis.url)
                .await;
        let startup = acquire_stage8b_p1e_startup_owner_v1(restart, session)
            .await
            .unwrap();
        assert_eq!(
            startup.kind(),
            Stage8bP1eStartupOwnerKindV1::ScheduleBindingCommitted
        );
        let restart_reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let mut restart_context = fixture.context;
        let latch = Stage8bP1eShutdownLatchV1::new();
        let Stage8bP1eStartupLatchDecisionV1::ScheduleBindingCommitted(pending) =
            latch_stage8b_p1e_startup_owner_v1(startup, &latch)
        else {
            panic!("restart must retain the committed schedule startup route")
        };
        let Stage8bP1eCommittedScheduleStartupV1 {
            durable,
            transport,
            control,
        } = pending;
        let Stage8bP1eRecoveredInitialLimitScheduleOutcomeV1::LimitAckCommitted {
            owner,
            high_water,
        } = resume_stage8b_p1e_committed_initial_limit_with_redis(durable, transport, &latch, &key)
            .await
            .unwrap()
        else {
            panic!("clear restart latch must commit the inherited Initial LIMIT S_ack")
        };
        apply_recovered_schedule_high_water(&mut restart_context, Some(high_water)).unwrap();
        let lifecycle =
            Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(Stage8bP1eRecoveryStepV1 {
                route: Box::new(Stage8bP1eRecoveryStepRouteV1::LimitAckCommitted(owner)),
                control,
            }));
        let Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(ready) =
            drain_stage8b_p1e_recovery_lifecycle_v1(lifecycle, &latch, &key)
                .await
                .unwrap()
        else {
            panic!("restarted Initial LIMIT S_ack must drain to exact Ready")
        };
        assert_eq!(
            restart_reader.test_read_attempts(),
            0,
            "committed V4 restart must not reread signed schedule source"
        );
        assert_eq!(
            restart_context.high_water,
            Some(expected_high_water.clone())
        );
        let pending_after: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            pending_after.count(),
            0,
            "S_truth must precede source XACK-last"
        );
        let commands_after: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            commands_after, commands_before,
            "committed V4 restart must revalidate, never republish, the command"
        );
        drop(ready);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn committed_generated_market_restart_revalidates_without_schedule_reread_or_republish() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("committed-generated-market-restart");
        let (published, key, fresh, identity_sha256, candidate_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_generated_market_published(&redis.url, &parent)
                .await;
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(candidate_close_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity_sha256,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut binding_reader =
            crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
                &redis.url,
                fixture.public_key_hex.clone(),
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .await
            .unwrap();
        let snapshot = match binding_reader
            .read_newest_guarded(&fixture.context, &Stage8bP1eShutdownLatchV1::new())
            .await
            .unwrap()
        {
            crate::Stage8bP1eGuardedScheduleReadV1::Read(
                crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
            ) => snapshot,
            _ => panic!("fixture schedule must produce one verified generated-Market snapshot"),
        };
        let expected_high_water = snapshot.high_water().clone();
        let _binding_receipt =
            crate::stage8b_p1_semantic::p1e_test_commit_generated_market_v4_only(
                published,
                *snapshot,
                trusted_now,
                &key,
            )
            .await;
        assert_eq!(binding_reader.test_read_attempts(), 1);

        let namespace = crate::stage8b_p1_redis_namespace();
        let commands_before: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(commands_before, 1);
        let pending_before: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_before.count(), 1);

        let operational_identity =
            operational_identity_config(parent.clone(), fresh.stage5c_config_fingerprint());
        let root_name =
            crate::Stage7bDurableRootAuthority::expected_directory_name(&operational_identity)
                .unwrap();
        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity,
            &key,
            fresh,
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let Stage8bP1ePreRedisRestartV1::Attachable(restart) =
            stage8b_p1e_route_pre_redis_restart_v1(restart)
        else {
            panic!("authenticated generated-Market V4 restart must remain attachable")
        };
        assert_eq!(
            restart.kind(),
            Stage8bP1eRestartKindV1::P1eScheduleBindingCommitted
        );
        let session =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_verified_redis_session_v1(&redis.url)
                .await;
        let startup = acquire_stage8b_p1e_startup_owner_v1(restart, session)
            .await
            .unwrap();
        assert_eq!(
            startup.kind(),
            Stage8bP1eStartupOwnerKindV1::ScheduleBindingCommitted
        );
        let restart_reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let mut restart_context = fixture.context;
        let latch = Stage8bP1eShutdownLatchV1::new();
        let Stage8bP1eStartupLatchDecisionV1::ScheduleBindingCommitted(pending) =
            latch_stage8b_p1e_startup_owner_v1(startup, &latch)
        else {
            panic!("restart must retain the committed generated-Market schedule route")
        };
        let Stage8bP1eCommittedScheduleStartupV1 {
            durable,
            transport,
            control,
        } = pending;
        let Stage8bP1eRecoveredGeneratedMarketScheduleOutcomeV1::GeneratedMarketAckCommitted {
            owner,
            high_water,
        } = resume_stage8b_p1e_committed_generated_market_with_redis(
            durable, transport, &latch, &key,
        )
        .await
        .unwrap()
        else {
            panic!("clear restart latch must commit inherited generated-Market S_ack")
        };
        apply_recovered_schedule_high_water(&mut restart_context, Some(high_water)).unwrap();
        let lifecycle =
            Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(Stage8bP1eRecoveryStepV1 {
                route: Box::new(Stage8bP1eRecoveryStepRouteV1::GeneratedMarketAckCommitted(
                    owner,
                )),
                control,
            }));
        let Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(ready) =
            drain_stage8b_p1e_recovery_lifecycle_v1(lifecycle, &latch, &key)
                .await
                .unwrap()
        else {
            panic!("restarted generated-Market S_ack must drain to exact Ready")
        };
        assert_eq!(
            restart_reader.test_read_attempts(),
            0,
            "committed generated-Market V4 restart must not reread signed schedule source"
        );
        assert_eq!(restart_context.high_water, Some(expected_high_water));
        let pending_after: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            pending_after.count(),
            0,
            "generated S_truth must precede source XACK-last"
        );
        let commands_after: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            commands_after, commands_before,
            "committed generated-Market V4 restart must revalidate, never republish, the command"
        );
        drop(ready);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn committed_target_first_cancel_restart_preserves_pel_and_xacks_truth_last() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("committed-target-first-cancel-restart");
        let (published, key, fresh, identity_sha256, candidate_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_target_first_cancel_published(&redis.url, &parent)
                .await;
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(candidate_close_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity_sha256,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut binding_reader =
            crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
                &redis.url,
                fixture.public_key_hex.clone(),
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .await
            .unwrap();
        let snapshot = match binding_reader
            .read_newest_guarded(&fixture.context, &Stage8bP1eShutdownLatchV1::new())
            .await
            .unwrap()
        {
            crate::Stage8bP1eGuardedScheduleReadV1::Read(
                crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
            ) => snapshot,
            _ => panic!("fixture schedule must produce one verified CANCEL snapshot"),
        };
        let expected_high_water = snapshot.high_water().clone();
        crate::stage8b_p1_semantic::p1e_test_commit_cancel_v4_only(
            published,
            *snapshot,
            trusted_now,
            &key,
        )
        .await;
        assert_eq!(binding_reader.test_read_attempts(), 1);

        let namespace = crate::stage8b_p1_redis_namespace();
        let commands_before: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        let pending_before: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(commands_before, 1);
        assert_eq!(pending_before.count(), 1);

        let operational_identity =
            operational_identity_config(parent.clone(), fresh.stage5c_config_fingerprint());
        let root_name =
            crate::Stage7bDurableRootAuthority::expected_directory_name(&operational_identity)
                .unwrap();
        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(&root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity.clone(),
            &key,
            fresh.clone(),
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1eScheduleBindingCommitted(committed) = restart else {
            panic!("authenticated CANCEL V4 must restart as its exact committed binding")
        };
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let mut reclaim = crate::Stage8bP1RedisConfig::paper_default_auto();
        reclaim.claim_idle_ms = 1;
        let transport = crate::attach_stage8b_p1_redis(&redis.url, reclaim)
            .await
            .unwrap();
        let crate::Stage8bP1eRecoveredCancelScheduleOutcomeV1::CancelCommitted {
            outcome,
            high_water,
        } = crate::resume_stage8b_p1e_committed_cancel_with_redis(
            committed,
            transport,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await
        .unwrap()
        else {
            panic!("clear restart latch must continue the committed CANCEL")
        };
        assert_eq!(high_water, expected_high_water);
        assert_eq!(
            binding_reader.test_read_attempts(),
            1,
            "committed Cancel restart cannot reread signed schedule"
        );
        let Stage8bP1RedisCancelCommitOutcome::CancelContinuationPending(pending) = *outcome else {
            panic!("target truth must win before recovered CANCEL settlement")
        };
        assert!(!pending.m10_xack_allowed());
        let pending_snapshot = pending.stage8b_p1d3_test_restart_snapshot();
        drop(pending);

        let pending_mid: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        let commands_mid: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            pending_mid.count(),
            1,
            "target truth cannot acknowledge M10"
        );
        assert_eq!(
            commands_mid, commands_before,
            "CANCEL cannot be republished"
        );

        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(&root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity.clone(),
            &key,
            fresh.clone(),
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let audit_before_settlement = restart.stage8b_p1d4_test_runtime_audit().unwrap();
        let Stage7bRestartOutcome::P1d3CancelContinuationPending(pending) = restart else {
            panic!("restart must expose only recovered-CANCEL continuation")
        };
        assert_eq!(
            pending.stage8b_p1d3_test_restart_snapshot(),
            pending_snapshot,
            "restart must not repeat target/provider/callback effects"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let mut reclaim = crate::Stage8bP1RedisConfig::paper_default_auto();
        reclaim.claim_idle_ms = 1;
        let transport = crate::attach_stage8b_p1_redis(&redis.url, reclaim)
            .await
            .unwrap();
        let truth = crate::stage8b_p1_semantic::p1e_test_resume_p1d3_cancel_continuation(
            *pending, transport, &key,
        )
        .await
        .unwrap();
        assert!(truth.m10_xack_allowed());
        let truth_snapshot = truth.stage8b_p1d3_test_restart_snapshot();
        assert_eq!(truth_snapshot.0, pending_snapshot.0 + 1);
        assert_eq!(truth_snapshot.2, pending_snapshot.2);
        let pending_truth: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_truth.count(), 1, "S_truth must precede XACK-last");
        let resolved = truth.acknowledge_source().await.unwrap();
        assert_eq!(
            resolved.disposition(),
            crate::Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
        );
        drop(resolved);
        let pending_after: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_after.count(), 0);

        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity,
            &key,
            fresh,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let audit_after_settlement = restart.stage8b_p1d4_test_runtime_audit().unwrap();
        assert_eq!(
            audit_after_settlement.dispatch_v1_total,
            audit_before_settlement.dispatch_v1_total
        );
        assert_eq!(
            audit_after_settlement.callback_count,
            audit_before_settlement.callback_count
        );
        assert_eq!(commands_mid, commands_before);
        assert!(matches!(
            restart,
            Stage7bRestartOutcome::P1d3TruthCommitted(_)
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn committed_day_expiry_restart_is_source_free_and_does_not_reread_schedule() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("committed-day-expiry-restart");
        let (owner, key, fresh, identity, boundary_ms) =
            crate::stage8b_p1_semantic::p1e_test_day_expiry_ready(&redis.url, &parent).await;
        let boundary = DateTime::<Utc>::from_timestamp_millis(boundary_ms).unwrap();
        let mut fixture =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_closed_schedule_envelope(
                identity,
                fresh.stage5c_config_fingerprint(),
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                boundary,
            );
        fixture.bytes =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_revised_schedule_envelope(
                &fixture.bytes,
                2,
                2,
                boundary,
            );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let snapshot = match reader
            .read_newest_guarded(&fixture.context, &Stage8bP1eShutdownLatchV1::new())
            .await
            .unwrap()
        {
            crate::Stage8bP1eGuardedScheduleReadV1::Read(
                crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
            ) => snapshot,
            _ => panic!("fixture schedule must produce one verified Closed snapshot"),
        };
        let expected_high_water = snapshot.high_water().clone();
        crate::stage8b_p1_semantic::p1e_test_commit_day_expiry_v4_only(
            owner, *snapshot, boundary, &key,
        );
        assert_eq!(reader.test_read_attempts(), 1);

        let namespace = crate::stage8b_p1_redis_namespace();
        let pending_before: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_before.count(), 0);
        let operational_identity =
            operational_identity_config(parent.clone(), fresh.stage5c_config_fingerprint());
        let root_name =
            crate::Stage7bDurableRootAuthority::expected_directory_name(&operational_identity)
                .unwrap();
        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity,
            &key,
            fresh,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1eScheduleBindingCommitted(committed) = restart else {
            panic!("authenticated Day-expiry V4 must restart as its exact committed binding")
        };
        let transport = crate::attach_stage8b_p1_redis(
            &redis.url,
            crate::Stage8bP1RedisConfig::paper_default_auto(),
        )
        .await
        .unwrap();
        let crate::Stage8bP1eRecoveredDayExpiryScheduleOutcomeV1::Ready { owner, high_water } =
            crate::resume_stage8b_p1e_committed_day_expiry(
                committed,
                transport,
                &Stage8bP1eShutdownLatchV1::new(),
                &key,
            )
            .unwrap()
        else {
            panic!("clear restart latch must complete source-free Day expiry")
        };
        assert_eq!(high_water, expected_high_water);
        assert_eq!(
            reader.test_read_attempts(),
            1,
            "committed Day-expiry restart cannot reread signed schedule"
        );
        assert!(!owner.requires_later_limit_evaluation());
        let pending_after: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_after.count(), 0, "source-free restart cannot XACK");
        drop(owner);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn owner_loop_routes_committed_cancel_ack_truth_to_xack_last() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("owner-loop-committed-cancel-ack-truth");
        let (published, key, fresh, identity_sha256, candidate_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_cancel_published(&redis.url, &parent).await;
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(candidate_close_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity_sha256,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut binding_reader =
            crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
                &redis.url,
                fixture.public_key_hex.clone(),
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .await
            .unwrap();
        let snapshot = match binding_reader
            .read_newest_guarded(&fixture.context, &Stage8bP1eShutdownLatchV1::new())
            .await
            .unwrap()
        {
            crate::Stage8bP1eGuardedScheduleReadV1::Read(
                crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
            ) => snapshot,
            _ => panic!("fixture schedule must produce one verified CANCEL snapshot"),
        };
        let expected_high_water = snapshot.high_water().clone();
        crate::stage8b_p1_semantic::p1e_test_commit_cancel_v4_only(
            published,
            *snapshot,
            trusted_now,
            &key,
        )
        .await;

        let namespace = crate::stage8b_p1_redis_namespace();
        let operational_identity =
            operational_identity_config(parent.clone(), fresh.stage5c_config_fingerprint());
        let root_name =
            crate::Stage7bDurableRootAuthority::expected_directory_name(&operational_identity)
                .unwrap();
        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(&root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity.clone(),
            &key,
            fresh.clone(),
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let Stage8bP1ePreRedisRestartV1::Attachable(restart) =
            stage8b_p1e_route_pre_redis_restart_v1(restart)
        else {
            panic!("authenticated committed CANCEL must be attachable")
        };
        tokio::time::sleep(StdDuration::from_millis(5)).await;
        crate::stage8b_p1_semantic::p1e_i1_begin_direct_effect_audit();
        let session =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_verified_redis_session_v1(&redis.url)
                .await;
        let startup = acquire_stage8b_p1e_startup_owner_v1(restart, session)
            .await
            .unwrap();
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let mut context = fixture.context.clone();
        let outcome = run_stage8b_p1e_startup_owner_loop_v1(
            startup,
            &mut reader,
            &mut context,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await
        .unwrap();
        let resolved = match outcome {
            Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelResolved(resolved) => resolved,
            _ => panic!("committed CANCEL ACK/truth must stop at typed XACK-last completion"),
        };
        assert_eq!(
            resolved.disposition(),
            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
        );
        let ready = resolved.into_ready_polling();
        let effects = crate::stage8b_p1_semantic::p1e_i1_take_direct_effect_audit();
        assert_eq!(effects.provider_total, 1);
        assert_eq!(effects.callback_total, 0);
        assert_eq!(effects.publication_attempt_total, 0);
        assert_eq!(effects.publication_total, 0);
        assert!(effects.claim_total > 0);
        assert_eq!(effects.xack_attempt_total, 1);
        assert_eq!(effects.xack_total, 1);
        assert_eq!(effects.schedule_read_total, 0);
        assert_eq!(reader.test_read_attempts(), 0);
        assert_eq!(context.high_water, Some(expected_high_water));
        let pending: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0);

        crate::stage8b_p1_semantic::p1e_i1_begin_direct_effect_audit();
        let next = poll_stage8b_p1e_ready_once_v1(ready, &Stage8bP1eShutdownLatchV1::new())
            .await
            .unwrap();
        let Stage8bP1eReadyPollOutcomeV1::ContinueSource(next) = next else {
            panic!("recovered CANCEL completion must hand its owner to the next canonical M10")
        };
        let next = drain_stage8b_p1e_schedule_free_recovery_v1(
            next,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await
        .unwrap();
        assert!(matches!(
            next,
            Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(_)
                | Stage8bP1eScheduleFreeDrainOutcomeV1::ScheduleDeferred(_)
        ));
        let next_effects = crate::stage8b_p1_semantic::p1e_i1_take_direct_effect_audit();
        assert_eq!(next_effects.provider_total, 0);
        assert_eq!(next_effects.xack_attempt_total, next_effects.xack_total);
        drop(next);

        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity,
            &key,
            fresh,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let audit = restart.stage8b_p1d4_test_runtime_audit().unwrap();
        let cancel = audit
            .sequence_allocations
            .iter()
            .find(|allocation| allocation.outcome_kind == "cancel_canceled")
            .expect("committed CANCEL must retain its exact terminal allocation");
        let seq_ack = cancel.seq_ack.expect("CANCEL ACK sequence");
        let seq_truth = cancel.seq_truth.expect("CANCEL truth sequence");
        assert_eq!(seq_ack.checked_add(1), Some(seq_truth));
        assert!(
            !matches!(restart, Stage7bRestartOutcome::P1d3TruthCommitted(_)),
            "the completed CANCEL source must not be replayed after its successor M10"
        );
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn owner_loop_routes_committed_target_first_cancel_to_xack_last() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("owner-loop-committed-target-first-cancel");
        let (published, key, fresh, identity_sha256, candidate_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_target_first_cancel_published(&redis.url, &parent)
                .await;
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(candidate_close_ms).unwrap();
        let fixture = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_envelope(
            identity_sha256,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            trusted_now,
        );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut binding_reader =
            crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
                &redis.url,
                fixture.public_key_hex.clone(),
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .await
            .unwrap();
        let snapshot = match binding_reader
            .read_newest_guarded(&fixture.context, &Stage8bP1eShutdownLatchV1::new())
            .await
            .unwrap()
        {
            crate::Stage8bP1eGuardedScheduleReadV1::Read(
                crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
            ) => snapshot,
            _ => panic!("fixture schedule must produce one verified CANCEL snapshot"),
        };
        let expected_high_water = snapshot.high_water().clone();
        crate::stage8b_p1_semantic::p1e_test_commit_cancel_v4_only(
            published,
            *snapshot,
            trusted_now,
            &key,
        )
        .await;

        let namespace = crate::stage8b_p1_redis_namespace();
        let commands_before: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        let operational_identity =
            operational_identity_config(parent.clone(), fresh.stage5c_config_fingerprint());
        let root_name =
            crate::Stage7bDurableRootAuthority::expected_directory_name(&operational_identity)
                .unwrap();
        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(&root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity.clone(),
            &key,
            fresh.clone(),
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let audit_before = restart.stage8b_p1d4_test_runtime_audit().unwrap();
        let Stage8bP1ePreRedisRestartV1::Attachable(restart) =
            stage8b_p1e_route_pre_redis_restart_v1(restart)
        else {
            panic!("authenticated committed CANCEL must be attachable")
        };
        tokio::time::sleep(StdDuration::from_millis(5)).await;
        let session =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_verified_redis_session_v1(&redis.url)
                .await;
        let startup = acquire_stage8b_p1e_startup_owner_v1(restart, session)
            .await
            .unwrap();
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let mut context = fixture.context.clone();
        let outcome = run_stage8b_p1e_startup_owner_loop_v1(
            startup,
            &mut reader,
            &mut context,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await
        .unwrap();
        let restart_required = match outcome {
            Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelRestartRequired(boundary) => boundary,
            _ => panic!("target-first CANCEL must stop at its authenticated restart boundary"),
        };
        assert_eq!(
            restart_required.kind(),
            Stage8bP1eRestartKindV1::P1d3CancelContinuationPending
        );
        drop(restart_required);

        assert_eq!(reader.test_read_attempts(), 0);
        assert_eq!(context.high_water, Some(expected_high_water.clone()));
        let pending_at_restart: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            pending_at_restart.count(),
            1,
            "target-first restart boundary cannot acknowledge the source"
        );
        let commands_at_restart: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(commands_at_restart, commands_before);

        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(&root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity.clone(),
            &key,
            fresh.clone(),
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let audit_at_restart = restart.stage8b_p1d4_test_runtime_audit().unwrap();
        assert_eq!(audit_at_restart.callback_count, audit_before.callback_count);
        assert_eq!(
            audit_at_restart.dispatch_v1_total,
            audit_before.dispatch_v1_total + 1,
            "the committed CANCEL executes exactly once before target-first restart"
        );
        assert!(matches!(
            &restart,
            Stage7bRestartOutcome::P1d3CancelContinuationPending(_)
        ));
        let Stage8bP1ePreRedisRestartV1::Attachable(restart) =
            stage8b_p1e_route_pre_redis_restart_v1(restart)
        else {
            panic!("authenticated recovered-CANCEL continuation must be attachable")
        };
        tokio::time::sleep(StdDuration::from_millis(5)).await;
        crate::stage8b_p1_semantic::p1e_i1_begin_direct_effect_audit();
        crate::stage8b_p1_semantic::p1e_i1_inject_xack_response_loss_once();
        let session =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_verified_redis_session_v1(&redis.url)
                .await;
        let startup = acquire_stage8b_p1e_startup_owner_v1(restart, session)
            .await
            .unwrap();
        let mut restart_reader =
            crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
                &redis.url,
                fixture.public_key_hex.clone(),
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .await
            .unwrap();
        let mut restart_context = fixture.context.clone();
        let outcome = run_stage8b_p1e_startup_owner_loop_v1(
            startup,
            &mut restart_reader,
            &mut restart_context,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await;
        assert!(matches!(
            outcome,
            Err(Stage8bP1eStartupErrorV1::Source(
                Stage8bP1RedisSemanticError::Redis(_)
            ))
        ));
        let effects = crate::stage8b_p1_semantic::p1e_i1_take_direct_effect_audit();
        assert_eq!(effects.provider_total, 0);
        assert_eq!(effects.callback_total, 0);
        assert_eq!(effects.publication_attempt_total, 0);
        assert_eq!(effects.publication_total, 0);
        assert!(effects.claim_total > 0);
        assert_eq!(effects.xack_attempt_total, 1);
        assert_eq!(effects.xack_total, 0);
        assert_eq!(effects.schedule_read_total, 0);

        assert_eq!(restart_reader.test_read_attempts(), 0);
        assert_eq!(
            restart_context.high_water, None,
            "an injected XACK response loss must not report successful loop settlement"
        );
        let commands_after: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            commands_after, commands_before,
            "CANCEL cannot be republished"
        );
        let pending_after: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_after.count(), 0);

        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(&root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity.clone(),
            &key,
            fresh.clone(),
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let audit_after = restart.stage8b_p1d4_test_runtime_audit().unwrap();
        assert_eq!(
            audit_after
                .sequence_allocations
                .iter()
                .map(|allocation| allocation.outcome_kind.as_str())
                .collect::<Vec<_>>(),
            vec![
                "initial_working",
                "later_filled",
                "cancel_execution_observed"
            ]
        );
        let target_truth = audit_after
            .sequence_allocations
            .iter()
            .find(|allocation| allocation.outcome_kind == "later_filled")
            .expect("target-first CANCEL must retain target truth allocation");
        assert!(target_truth.seq_ack.is_none());
        let target_truth_sequence = target_truth.seq_truth.expect("target truth sequence");
        assert_eq!(
            target_truth.sequence_allocation_frontier.checked_add(1),
            Some(target_truth_sequence)
        );
        let recovered_cancel_ack = audit_after
            .sequence_allocations
            .iter()
            .find(|allocation| allocation.outcome_kind == "cancel_execution_observed")
            .expect("target-first CANCEL must retain exact recovered ACK allocation");
        assert!(recovered_cancel_ack.seq_truth.is_none());
        assert_eq!(
            recovered_cancel_ack.sequence_allocation_frontier,
            target_truth_sequence
        );
        assert_eq!(
            recovered_cancel_ack.seq_ack,
            target_truth_sequence.checked_add(1)
        );
        assert_eq!(audit_after.callback_count, audit_before.callback_count);
        assert_eq!(
            audit_after.dispatch_v1_total, audit_at_restart.dispatch_v1_total,
            "recovered CANCEL continuation cannot repeat provider dispatch"
        );
        assert!(matches!(
            &restart,
            Stage7bRestartOutcome::P1d3TruthCommitted(_)
        ));

        let exact_sequence_allocations = audit_after.sequence_allocations.clone();
        let Stage8bP1ePreRedisRestartV1::Attachable(restart) =
            stage8b_p1e_route_pre_redis_restart_v1(restart)
        else {
            panic!("truth-committed CANCEL response-loss replay must be attachable")
        };
        crate::stage8b_p1_semantic::p1e_i1_begin_direct_effect_audit();
        let session =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_verified_redis_session_v1(&redis.url)
                .await;
        let startup = acquire_stage8b_p1e_startup_owner_v1(restart, session)
            .await
            .unwrap();
        let mut replay_reader =
            crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
                &redis.url,
                fixture.public_key_hex.clone(),
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .await
            .unwrap();
        let mut replay_context = fixture.context.clone();
        let replay = run_stage8b_p1e_startup_owner_loop_v1(
            startup,
            &mut replay_reader,
            &mut replay_context,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await
        .unwrap();
        let replay_resolved = match replay {
            Stage8bP1eOwnerLoopOutcomeV1::CommittedCancelResolved(resolved) => resolved,
            _ => panic!("final-XACK response loss must settle at the typed CANCEL boundary"),
        };
        assert_eq!(
            replay_resolved.disposition(),
            Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged
        );
        assert_eq!(replay_reader.test_read_attempts(), 0);
        assert_eq!(replay_context.high_water, Some(expected_high_water));
        let replay_ready = replay_resolved.into_ready_polling();
        let replay_effects = crate::stage8b_p1_semantic::p1e_i1_take_direct_effect_audit();
        assert_eq!(replay_effects.provider_total, 0);
        assert_eq!(replay_effects.callback_total, 0);
        assert_eq!(replay_effects.publication_attempt_total, 0);
        assert_eq!(replay_effects.publication_total, 0);
        assert_eq!(replay_effects.claim_total, 0);
        assert_eq!(replay_effects.xack_attempt_total, 0);
        assert_eq!(replay_effects.xack_total, 0);
        assert_eq!(replay_effects.schedule_read_total, 0);

        crate::stage8b_p1_semantic::p1e_i1_begin_direct_effect_audit();
        let next = poll_stage8b_p1e_ready_once_v1(replay_ready, &Stage8bP1eShutdownLatchV1::new())
            .await
            .unwrap();
        let Stage8bP1eReadyPollOutcomeV1::ContinueSource(next) = next else {
            panic!("response-loss replay must hand its owner to the next canonical M10")
        };
        let next = drain_stage8b_p1e_schedule_free_recovery_v1(
            next,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await
        .unwrap();
        assert!(matches!(
            next,
            Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(_)
                | Stage8bP1eScheduleFreeDrainOutcomeV1::ScheduleDeferred(_)
        ));
        let next_effects = crate::stage8b_p1_semantic::p1e_i1_take_direct_effect_audit();
        assert_eq!(next_effects.provider_total, 0);
        assert_eq!(next_effects.callback_total, 1);
        assert_eq!(next_effects.publication_attempt_total, 1);
        assert_eq!(next_effects.publication_total, 1);
        drop(next);

        let commands_after_successor: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            commands_after_successor,
            commands_before + usize::try_from(next_effects.publication_total).unwrap(),
            "the successor M10 may publish only its own newly generated command"
        );

        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(root_name),
            &operational_identity,
        )
        .unwrap();
        let replay_restart =
            crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
                root,
                operational_identity,
                &key,
                fresh,
                fixture.public_key_hex,
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .unwrap();
        let replay_audit = replay_restart.stage8b_p1d4_test_runtime_audit().unwrap();
        assert_eq!(
            replay_audit.sequence_allocations, exact_sequence_allocations,
            "final-XACK response loss cannot allocate another ACK/truth sequence"
        );
        assert_eq!(
            replay_audit.callback_count, audit_after.callback_count,
            "restart projection must retain the single-callback authority shape"
        );
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn owner_loop_routes_committed_day_expiry_source_free_to_terminal() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("owner-loop-committed-day-expiry");
        let (owner, key, fresh, identity, boundary_ms) =
            crate::stage8b_p1_semantic::p1e_test_day_expiry_ready(&redis.url, &parent).await;
        let boundary = DateTime::<Utc>::from_timestamp_millis(boundary_ms).unwrap();
        let mut fixture =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_closed_schedule_envelope(
                identity,
                fresh.stage5c_config_fingerprint(),
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                boundary,
            );
        fixture.bytes =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_revised_schedule_envelope(
                &fixture.bytes,
                2,
                2,
                boundary,
            );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut binding_reader =
            crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
                &redis.url,
                fixture.public_key_hex.clone(),
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .await
            .unwrap();
        let snapshot = match binding_reader
            .read_newest_guarded(&fixture.context, &Stage8bP1eShutdownLatchV1::new())
            .await
            .unwrap()
        {
            crate::Stage8bP1eGuardedScheduleReadV1::Read(
                crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
            ) => snapshot,
            _ => panic!("fixture schedule must produce one verified Closed snapshot"),
        };
        let expected_high_water = snapshot.high_water().clone();
        crate::stage8b_p1_semantic::p1e_test_commit_day_expiry_v4_only(
            owner, *snapshot, boundary, &key,
        );
        let namespace = crate::stage8b_p1_redis_namespace();
        let commands_before: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        let operational_identity =
            operational_identity_config(parent.clone(), fresh.stage5c_config_fingerprint());
        let root_name =
            crate::Stage7bDurableRootAuthority::expected_directory_name(&operational_identity)
                .unwrap();
        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(&root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity.clone(),
            &key,
            fresh.clone(),
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let audit_before = restart.stage8b_p1d4_test_runtime_audit().unwrap();
        let Stage8bP1ePreRedisRestartV1::Attachable(restart) =
            stage8b_p1e_route_pre_redis_restart_v1(restart)
        else {
            panic!("authenticated committed Day-expiry must be attachable")
        };
        crate::stage8b_p1_semantic::p1e_i1_begin_direct_effect_audit();
        let session =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_verified_redis_session_v1(&redis.url)
                .await;
        let startup = acquire_stage8b_p1e_startup_owner_v1(restart, session)
            .await
            .unwrap();
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let mut context = fixture.context;
        let outcome = run_stage8b_p1e_startup_owner_loop_v1(
            startup,
            &mut reader,
            &mut context,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await
        .unwrap();
        let Stage8bP1eOwnerLoopOutcomeV1::CommittedDayExpiryResolved(resolved) = outcome else {
            panic!("source-free Day-expiry must stop at its typed terminal boundary")
        };
        drop(resolved);
        let effects = crate::stage8b_p1_semantic::p1e_i1_take_direct_effect_audit();
        assert_eq!(effects.provider_total, 0);
        assert_eq!(effects.callback_total, 0);
        assert_eq!(effects.publication_total, 0);
        assert_eq!(effects.claim_total, 0);
        assert_eq!(effects.xack_total, 0);
        assert_eq!(effects.schedule_read_total, 0);

        assert_eq!(reader.test_read_attempts(), 0);
        assert_eq!(context.high_water, Some(expected_high_water));
        let pending_after: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_after.count(), 0);
        let commands_after: usize = redis::cmd("XLEN")
            .arg(&namespace.canonical_command_stream)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(commands_after, commands_before);

        let root = crate::Stage7bDurableRootAuthority::validate(
            parent.join(root_name),
            &operational_identity,
        )
        .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity,
            &key,
            fresh,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let audit_after = restart.stage8b_p1d4_test_runtime_audit().unwrap();
        assert!(
            audit_after
                .sequence_allocations
                .starts_with(&audit_before.sequence_allocations),
            "Day-expiry must preserve the exact pre-restart sequence prefix"
        );
        let expiry = audit_after
            .sequence_allocations
            .strip_prefix(audit_before.sequence_allocations.as_slice())
            .and_then(|suffix| match suffix {
                [expiry] if expiry.outcome_kind == "later_expired" => Some(expiry),
                _ => None,
            })
            .expect("source-free Day-expiry must allocate one terminal truth sequence");
        assert!(expiry.seq_ack.is_none());
        assert_eq!(
            expiry.seq_truth,
            expiry.sequence_allocation_frontier.checked_add(1)
        );
        assert_eq!(audit_after.callback_count, audit_before.callback_count);
        assert_eq!(
            audit_after.dispatch_v1_total,
            audit_before.dispatch_v1_total
        );
        assert!(matches!(restart, Stage7bRestartOutcome::Ready(_)));
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn owner_loop_rejects_committed_day_expiry_when_pel_is_not_empty() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("owner-loop-day-expiry-nonempty-pel");
        let (owner, key, fresh, identity, boundary_ms) =
            crate::stage8b_p1_semantic::p1e_test_day_expiry_ready(&redis.url, &parent).await;
        let boundary = DateTime::<Utc>::from_timestamp_millis(boundary_ms).unwrap();
        let mut fixture =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_closed_schedule_envelope(
                identity,
                fresh.stage5c_config_fingerprint(),
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
                boundary,
            );
        fixture.bytes =
            crate::stage8b_p1e_schedule_source::tests::p1e_test_revised_schedule_envelope(
                &fixture.bytes,
                2,
                2,
                boundary,
            );
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let _: String = redis::cmd("XADD")
            .arg(strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("*")
            .arg("payload")
            .arg(&fixture.bytes)
            .query_async(&mut connection)
            .await
            .unwrap();
        let mut binding_reader =
            crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
                &redis.url,
                fixture.public_key_hex.clone(),
                fixture.key_valid_from,
                fixture.key_valid_until,
            )
            .await
            .unwrap();
        let snapshot = match binding_reader
            .read_newest_guarded(&fixture.context, &Stage8bP1eShutdownLatchV1::new())
            .await
            .unwrap()
        {
            crate::Stage8bP1eGuardedScheduleReadV1::Read(
                crate::Stage8bP1eNewestScheduleReadV1::Verified(snapshot),
            ) => snapshot,
            _ => panic!("fixture schedule must produce one verified Closed snapshot"),
        };
        crate::stage8b_p1_semantic::p1e_test_commit_day_expiry_v4_only(
            owner, *snapshot, boundary, &key,
        );
        let namespace = crate::stage8b_p1_redis_namespace();
        let injected_id: String = redis::cmd("XADD")
            .arg(&namespace.canonical_m10_stream)
            .arg("*")
            .arg("payload")
            .arg("{}")
            .query_async(&mut connection)
            .await
            .unwrap();
        let claimed: redis::streams::StreamReadReply = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(&namespace.m10_consumer_group)
            .arg("day-expiry-negative")
            .arg("COUNT")
            .arg(1)
            .arg("STREAMS")
            .arg(&namespace.canonical_m10_stream)
            .arg(">")
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(claimed.keys[0].ids[0].id, injected_id);

        let operational_identity =
            operational_identity_config(parent.clone(), fresh.stage5c_config_fingerprint());
        let root_name =
            crate::Stage7bDurableRootAuthority::expected_directory_name(&operational_identity)
                .unwrap();
        let root_path = parent.join(root_name);
        let durable_before = durable_file_snapshot(&root_path);
        let root =
            crate::Stage7bDurableRootAuthority::validate(root_path.clone(), &operational_identity)
                .unwrap();
        let restart = crate::Stage7bRecoveryReadyOwner::stage8b_p1e_test_restart_with_schedule_key(
            root,
            operational_identity,
            &key,
            fresh,
            fixture.public_key_hex.clone(),
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .unwrap();
        let Stage8bP1ePreRedisRestartV1::Attachable(restart) =
            stage8b_p1e_route_pre_redis_restart_v1(restart)
        else {
            panic!("authenticated committed Day-expiry must be attachable")
        };
        crate::stage8b_p1_semantic::p1e_i1_begin_direct_effect_audit();
        let session =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_verified_redis_session_v1(&redis.url)
                .await;
        let startup = acquire_stage8b_p1e_startup_owner_v1(restart, session)
            .await
            .unwrap();
        let mut reader = crate::Stage8bP1eRedisScheduleReader::test_connect_with_fixture_trust(
            &redis.url,
            fixture.public_key_hex,
            fixture.key_valid_from,
            fixture.key_valid_until,
        )
        .await
        .unwrap();
        let mut context = fixture.context;
        let error = match run_stage8b_p1e_startup_owner_loop_v1(
            startup,
            &mut reader,
            &mut context,
            &Stage8bP1eShutdownLatchV1::new(),
            &key,
        )
        .await
        {
            Ok(_) => panic!("nonempty PEL must fail before Day-expiry continuation"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            Stage8bP1eStartupErrorV1::CommittedDayExpiryPelNotEmpty
        ));
        let effects = crate::stage8b_p1_semantic::p1e_i1_take_direct_effect_audit();
        assert_eq!(effects.provider_total, 0);
        assert_eq!(effects.callback_total, 0);
        assert_eq!(effects.publication_total, 0);
        assert_eq!(effects.claim_total, 0);
        assert_eq!(effects.xack_total, 0);
        assert_eq!(effects.schedule_read_total, 0);
        assert_eq!(reader.test_read_attempts(), 0);
        assert_eq!(context.high_water, None);
        let pending_after: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending_after.count(), 1);
        assert_eq!(durable_file_snapshot(&root_path), durable_before);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn bounded_schedule_backoff_observes_shutdown_before_another_redis_read() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("signed-schedule-shutdown-backoff");
        let (published, key, fresh, identity, candidate_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_plain_market_published(&redis.url, &parent).await;
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let deferred = Stage8bP1eScheduleDeferredRecoveryV1 {
            kind: Stage8bP1eScheduleDeferredKindV1::CommandPublishedMarket,
            _route: Box::new(Stage8bP1eScheduleDeferredRouteV1::CommandPublished(
                Box::new(published),
            )),
            control,
        };
        let trusted_now = DateTime::<Utc>::from_timestamp_millis(candidate_close_ms).unwrap();
        let mut context =
            schedule_context(identity, fresh.stage5c_config_fingerprint(), trusted_now);
        let mut reader = crate::Stage8bP1eRedisScheduleReader::connect(&redis.url)
            .await
            .unwrap();
        let latch = Stage8bP1eShutdownLatchV1::new();
        let intent = Stage8bP1eShutdownIntentV1::new(
            crate::Stage8bP1eShutdownCauseV1::ExternalSignal,
            20_000,
            1,
        );
        let policy = Stage8bP1eScheduleAcquisitionPolicyV1 {
            attempts: 3,
            total_deadline: StdDuration::from_secs(1),
            redis_operation_timeout: StdDuration::from_millis(20),
            initial_backoff: StdDuration::from_millis(200),
            maximum_backoff: StdDuration::from_millis(200),
        };
        let started = Instant::now();
        let runner = advance_stage8b_p1e_supported_schedule_with_policy_v1(
            deferred,
            &mut reader,
            &mut context,
            &latch,
            &key,
            policy,
        );
        let signal = async {
            tokio::time::sleep(StdDuration::from_millis(10)).await;
            assert!(latch.request(intent.clone()));
        };
        let (outcome, ()) = tokio::join!(runner, signal);
        let Stage8bP1eBoundedScheduleCycleOutcomeV1::Stopped(stopped) = outcome.unwrap() else {
            panic!("shutdown must stop the retained schedule owner")
        };
        assert_eq!(stopped.receipt().shutdown_intent(), &intent);
        assert_eq!(
            stopped.receipt().checkpoint(),
            crate::Stage8bP1eScheduleLatchCheckpointV1::BeforeScheduleRead
        );
        assert_eq!(reader.test_read_attempts(), 1);
        assert!(started.elapsed() < StdDuration::from_millis(200));
        drop(stopped);
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(feature = "stage8a4-i3-test-fixtures")]
    #[tokio::test]
    async fn signed_market_lifecycle_drains_back_to_exact_s09_ready_owner() {
        let redis = RedisServer::start().await;
        let parent = temp_directory("signed-market-s09-reentry");
        let (published, key, fresh, identity, candidate_close_ms) =
            crate::stage8b_p1_semantic::p1e_test_plain_market_published(&redis.url, &parent).await;
        let bound_at = DateTime::<Utc>::from_timestamp_millis(candidate_close_ms).unwrap();
        let snapshot = crate::stage8b_p1e_schedule_source::tests::p1e_test_open_schedule_snapshot(
            identity,
            fresh.stage5c_config_fingerprint(),
            crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            format!("{candidate_close_ms}-1"),
            bound_at,
        );
        let Stage8bP1eSignedMarketScheduleOutcomeV1::FeedbackAckCommitted(ack) =
            crate::resume_stage8b_p1e_command_published_with_signed_schedule(
                published,
                snapshot,
                &Stage8bP1eShutdownLatchV1::new(),
                bound_at,
                &key,
            )
            .await
            .unwrap()
        else {
            panic!("clear signed Market path must commit the exact S_ack")
        };
        let control =
            crate::stage8b_p1_supervisor::stage8b_p1e_test_redis_control_v1(&redis.url).await;
        let lifecycle =
            Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(Stage8bP1eRecoveryStepV1 {
                route: Box::new(Stage8bP1eRecoveryStepRouteV1::FeedbackAckCommitted(ack)),
                control,
            }));
        let latch = Stage8bP1eShutdownLatchV1::new();
        let Stage8bP1eScheduleFreeDrainOutcomeV1::Ready(ready) =
            drain_stage8b_p1e_recovery_lifecycle_v1(lifecycle, &latch, &key)
                .await
                .unwrap()
        else {
            panic!("signed Market S_ack must drain through S_truth to exact Ready")
        };

        let namespace = crate::stage8b_p1_redis_namespace();
        let mut connection =
            redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
                .await
                .unwrap();
        let pending: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0, "S_truth must precede source XACK-last");
        drop(connection);
        drop(ready);
        fs::remove_dir_all(parent).unwrap();
    }
}
