//! Opt-in WS-primary source implementation; not operational acceptance.
//!
//! Normalized observations and receipts below do not authorize broker execution.
//! A canonical capability cannot be deserialized or constructed from a bare Bar.
//! The socket adapter binds physical generations and receive clocks to this
//! core. Explicit v2 entry is separate from the legacy REST-driven runner.
#[cfg(feature = "models-bridge")]
pub mod additional_models;
mod cache;
pub mod calendar;
mod continuity;
pub mod entry;
pub mod eod;
mod json;
#[cfg(feature = "models-bridge")]
pub mod models_bridge;
#[cfg(feature = "models-bridge")]
pub mod models_entry;
pub mod verify;
pub(crate) use continuity::ContinuityGate;
mod ingress;
mod policy;
pub(crate) mod rest_observation;
pub mod restart;
pub(crate) mod revisions;
pub(crate) mod wire;

pub use cache::{
    AdmittedCanonicalM10, AdmittedRecovery, CanonicalReceipt, CanonicalStatus, Counters,
    ExceptionReason, PipelineState, PrimaryProcessor, RecoveryCompletion, RecoveryInput,
    RecoveryOutcome, RecoveryPlan, RecoveryReceipt, RecoveryRequestScope, RecoveryWait,
    ResolvedRecovery, WsRecoveryInput, WsRecoveryReceipt, MAX_REST_READS, REST_BACKOFF_MS,
};
pub use ingress::{bounded_ingress, IngressReceiver, IngressSender};
pub use policy::{DeliveryPolicy, PrimaryPolicy, WsPrimaryConfig, WS_PRIMARY_POLICY};
pub use wire::{LiveDataStamp, SnapshotPresence, WireFrame, WireRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PrimaryError {
    Configuration,
    Wire,
    SnapshotFlag,
    Timestamp,
    Numeric,
    Identity,
    Generation,
    EntryWindow,
    ReferenceUnproven,
    ReferenceConflict,
    UnexpectedHistory,
    BootstrapBudget,
    CacheBudget,
    Continuity,
    CanonicalConflict,
    Clock,
    Held,
}

#[cfg(test)]
mod parity;
#[cfg(test)]
pub(crate) mod tests;
