//! Fixed-path process composition for the Stage 8B-P1-e paper supervisor.
//!
//! This boundary deliberately owns no FINAM dependency and no broker send
//! capability. Bootstrap and recovery are filesystem-only. `run` remains a
//! reserved fail-closed command until the durable owner loop is composed; it
//! does not load credentials, restart durable state, or attach Redis.

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::Path,
};

use chrono::{DateTime, Utc};
use strategy_runtime_core::Stage5gLifecycleCommitmentKey;

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
    acquire_stage8b_p1e_ready_pending_with_redis, authorize_stage8b_p1_first_boot,
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
    resume_stage8b_p1e_command_published_with_signed_schedule,
    resume_stage8b_p1e_ready_source_with_redis,
    resume_stage8b_p1e_ready_working_limit_with_signed_schedule,
    route_stage8b_p1e_post_acquisition_v1, validate_stage8b_p1e_supervisor_config_v1,
    Stage7bRestartOutcome, Stage8bP1RedisCommandPublished, Stage8bP1RedisFeedbackAckCommitted,
    Stage8bP1RedisFeedbackResolved, Stage8bP1RedisFeedbackTruthCommitted,
    Stage8bP1RedisGeneratedMarketAckCommitted, Stage8bP1RedisGeneratedMarketTruthCommitted,
    Stage8bP1RedisLimitAckCommitted, Stage8bP1RedisLimitResolved,
    Stage8bP1RedisLimitTruthCommitted, Stage8bP1RedisPreAckRecoveryOutcome,
    Stage8bP1RedisPrepublicationPending, Stage8bP1RedisSemanticCompositionOwner,
    Stage8bP1RedisSemanticCompositionTransport, Stage8bP1RedisSemanticError,
    Stage8bP1RedisSemanticOutcome, Stage8bP1RedisZeroIntentAckResolved,
    Stage8bP1eAdoptionRecoveryActionV5, Stage8bP1ePostAcquisitionOwnerV1,
    Stage8bP1ePreSealRecoveryActionV5, Stage8bP1ePublishedScheduleRouteV1,
    Stage8bP1eReadyFreshAcquisitionOutcomeV1, Stage8bP1eReadyPendingAcquisitionOutcomeV1,
    Stage8bP1eRedisControlError, Stage8bP1eRedisControlV1, Stage8bP1eRestartKindV1,
    Stage8bP1eRetainedSourceReceiptV1, Stage8bP1eRoutedContinuationV1,
    Stage8bP1eRoutedPostAcquisitionDecisionV1, Stage8bP1eShutdownIntentV1,
    Stage8bP1eShutdownLatchV1, Stage8bP1eSignedMarketScheduleOutcomeV1,
    Stage8bP1eSignedWorkingScheduleOutcomeV1, Stage8bP1eSupervisorConfigV1,
    Stage8bP1eValidatedSupervisorConfigV1, Stage8bP1eVerifiedRedisSessionV1,
    STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION, STAGE8B_P1E_SUPERVISOR_CONFIG_PATH,
};

use crate::stage8b_p1e_first_boot_transaction::next_stage8b_p1e_bootstrap_attempt_generation_v5;

const STAGE8B_P1E_BOOT_ID_PATH: &str = "/proc/sys/kernel/random/boot_id";
const STAGE8B_P1E_CONFIG_MAX_BYTES: u64 = 1024 * 1024;
const STAGE8B_P1E_SCHEDULE_FREE_RECOVERY_MAX_ROWS: usize = 8;

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
    _durable: Box<crate::Stage8bP1eScheduleBindingCommittedOwner>,
    _transport: Stage8bP1RedisSemanticCompositionTransport,
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
        let owner = recovered_ready_owner(*_route)?;
        Ok(Stage8bP1eReadyPollingV1 { owner, control })
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
}

impl Stage8bP1eReadyPollingV1 {
    pub fn redis_control_mut(&mut self) -> &mut Stage8bP1eRedisControlV1 {
        &mut self.control
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
    let Stage8bP1eReadyPollingV1 { owner, control } = ready;
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
                    Stage8bP1eReadyPollingV1 { owner, control },
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
    CommandPublishedUnsupported,
    RoutedContinuation,
}

#[allow(
    dead_code,
    reason = "opaque schedule-dependent owners are retained for I1A composition"
)]
enum Stage8bP1eScheduleDeferredRouteV1 {
    CommandPublished(Box<Stage8bP1RedisCommandPublished>),
    RoutedContinuation(Box<Stage8bP1eRoutedContinuationV1>),
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

/// Completes schedule checkpoints C-F for an already published Market
/// command. Empty newest-only reads retain the exact published owner. A
/// successful schedule binding reaches exactly one S_ack owner and rejoins
/// the ordinary row-bounded recovery state machine.
pub async fn advance_stage8b_p1e_market_schedule_v1(
    deferred: Stage8bP1eScheduleDeferredRecoveryV1,
    reader: &mut crate::Stage8bP1eRedisScheduleReader,
    context: &strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eMarketScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
    let Stage8bP1eScheduleDeferredRecoveryV1 {
        kind,
        _route: route,
        control,
    } = deferred;
    if kind != Stage8bP1eScheduleDeferredKindV1::CommandPublishedMarket {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    }
    let Stage8bP1eScheduleDeferredRouteV1::CommandPublished(published) = *route else {
        return Err(Stage8bP1RedisSemanticError::P1eContinuationPermitRouteMismatch.into());
    };

    match reader.read_newest_guarded(context, latch).await? {
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
        ) => match resume_stage8b_p1e_command_published_with_signed_schedule(
            *published,
            *snapshot,
            latch,
            bound_at_utc,
            commitment_key,
        )
        .await?
        {
            Stage8bP1eSignedMarketScheduleOutcomeV1::Stopped(receipt) => {
                Ok(Stage8bP1eMarketScheduleAdvanceOutcomeV1::Stopped(
                    Stage8bP1eScheduleStoppedRecoveryV1 { receipt, control },
                ))
            }
            Stage8bP1eSignedMarketScheduleOutcomeV1::FeedbackAckCommitted(ack) => {
                Ok(Stage8bP1eMarketScheduleAdvanceOutcomeV1::Lifecycle(
                    Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(Box::new(
                        Stage8bP1eRecoveryStepV1 {
                            route: Box::new(Stage8bP1eRecoveryStepRouteV1::FeedbackAckCommitted(
                                ack,
                            )),
                            control,
                        },
                    )),
                ))
            }
        },
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
    context: &strategy_runtime_core::Stage8bP1eScheduleVerificationContextV1,
    latch: &Stage8bP1eShutdownLatchV1,
    bound_at_utc: DateTime<Utc>,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eWorkingScheduleAdvanceOutcomeV1, Stage8bP1eStartupErrorV1> {
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

    match reader.read_newest_guarded(context, latch).await? {
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
        ) => match resume_stage8b_p1e_ready_working_limit_with_signed_schedule(
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
                Ok(Stage8bP1eWorkingScheduleAdvanceOutcomeV1::Lifecycle(
                    classify_recovered_semantic_outcome(outcome, control),
                ))
            }
        },
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

/// Drains one acquired source through every schedule-free authenticated row
/// in the same owner invocation. Each durable row is followed by a mandatory
/// latch recheck. A schedule-dependent route is returned with its exact owner
/// untouched; it is never guessed or bypassed here.
pub async fn drain_stage8b_p1e_schedule_free_recovery_v1(
    startup: Stage8bP1eContinuingStartupV1,
    latch: &Stage8bP1eShutdownLatchV1,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1eScheduleFreeDrainOutcomeV1, Stage8bP1eStartupErrorV1> {
    let mut step = continue_stage8b_p1e_recovery_once_v1(startup, commitment_key).await?;
    // `continue_stage8b_p1e_recovery_once_v1` above consumed row one. Each
    // loop iteration may consume exactly one additional authenticated row.
    for _ in 1..STAGE8B_P1E_SCHEDULE_FREE_RECOVERY_MAX_ROWS {
        let permit = match recheck_stage8b_p1e_recovery_step_latch_v1(step, latch) {
            Stage8bP1eRecoveryLatchDecisionV1::RetainForRestart(retained) => {
                return Ok(Stage8bP1eScheduleFreeDrainOutcomeV1::RetainedForRestart(
                    retained,
                ));
            }
            Stage8bP1eRecoveryLatchDecisionV1::Continue(permit) => permit,
        };
        match advance_stage8b_p1e_recovery_once_v1(permit, commitment_key).await? {
            Stage8bP1eRecoveryAdvanceOutcomeV1::Continue(next) => step = *next,
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
        }
    }
    Err(Stage8bP1eStartupErrorV1::RecoveryStepBudgetExceeded)
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
            let published = pending.publish_exact_command().await?;
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
                    _durable: durable,
                    _transport: transport,
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
    #[error("ordinary run requires one authenticated adopted root")]
    RunNotAdopted,
    #[error("owner-loop composition is not present in this administrative checkpoint")]
    OwnerLoopUnavailable,
    #[error("durable restart failed")]
    DurableRestart,
    #[error("durable restart is blocked before Redis")]
    RestartBlocked,
    #[error("verify-only Redis attachment failed")]
    RedisAttach,
}

impl Stage8bP1eProcessErrorV1 {
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Usage | Self::ConfigBoundary | Self::Config | Self::BootIdentity => 64,
            Self::Credential
            | Self::FirstBootSource
            | Self::FirstBootTransaction
            | Self::FirstBootRecovery
            | Self::RunNotAdopted
            | Self::OwnerLoopUnavailable
            | Self::DurableRestart
            | Self::RestartBlocked => 66,
            Self::RedisAttach => 67,
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
        // `run` is part of the frozen argv grammar, but this intermediate
        // administrative checkpoint must fail closed until the exhaustive
        // restart-route owner loop is composed. In particular it must not
        // attach Redis and then exit successfully after dropping the owner.
        Stage8bP1eProcessCommandV1::Run => {
            drop(supervisor);
            Err(Stage8bP1eProcessErrorV1::OwnerLoopUnavailable)
        }
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
        ffi::CString,
        io::Write,
        net::TcpListener,
        os::unix::fs::DirBuilderExt,
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
        let open_ts_utc_ms = close_ts_utc_ms - 600_000;
        crate::build_stage8b_p1_canonical_m10(crate::Stage8bP1CanonicalM10BuildInput {
            operational_identity_sha256,
            open_ts_utc_ms,
            close_ts_utc_ms,
            open: "2600".to_string(),
            high: "2601".to_string(),
            low: "2599".to_string(),
            close: "2600".to_string(),
            volume: "10000".to_string(),
            source_m1: source_m1(open_ts_utc_ms),
        })
        .unwrap()
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
        };
        let mut latch = Stage8bP1eShutdownLatchV1::new();

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
}
