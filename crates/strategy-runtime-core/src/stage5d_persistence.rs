//! Stage 5D additive persistence freeze surface.
//!
//! Stage 5D-b1 intentionally exposes only opaque capability names and
//! enforcement evidence. It does not implement persistence DTO mutation,
//! runtime-private snapshot application, Redis, FINAM, transport, dispatch, or
//! runtime-live behavior.

/// Stage 5D additive freeze manifest schema version.
pub const STAGE5D_ADDITIVE_FREEZE_SCHEMA_VERSION: u16 = 1;

/// Opaque proof that a validated Stage 5D runtime-private extension has been
/// applied in the persistence-enabled restore path.
pub struct Stage5dPrivateStateAppliedPaperStrategy {
    _private: (),
}

/// Opaque proof that the Stage 5D restore path has passed controlled bootstrap.
pub struct Stage5dBootstrappedPaperStrategy {
    _private: (),
}

/// Opaque proof that authoritative riskgate state has been injected before the
/// runtime-state-restored callback.
pub struct Stage5dRiskGateInjectedPaperStrategy {
    _private: (),
}

/// Opaque validated runtime-private extension marker.
pub struct Stage5dValidatedRuntimePrivateExtension {
    _private: (),
}

/// Redacted blocked-restore marker for future Stage 5D transitions.
pub struct Stage5dRestoreBlocked {
    reason: Stage5dRestoreBlockReason,
}

impl Stage5dRestoreBlocked {
    /// Return the redacted block reason without exposing strategy internals.
    pub fn reason(&self) -> Stage5dRestoreBlockReason {
        self.reason
    }
}

/// Public redacted restore blocker categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5dRestoreBlockReason {
    PrivateExtension,
    RiskGate,
    BrokerTruth,
    Integrity,
}

/// Redacted evidence that the Stage 5D additive freeze enforcement layer is
/// present. This is not a trading capability.
pub struct Stage5dAdditiveFreezeEvidence {
    schema_version: u16,
}

impl Stage5dAdditiveFreezeEvidence {
    /// Construct redacted local evidence for checker/tests.
    pub fn local() -> Self {
        Self {
            schema_version: STAGE5D_ADDITIVE_FREEZE_SCHEMA_VERSION,
        }
    }

    /// Schema version of the Stage 5D additive freeze surface.
    pub fn schema_version(&self) -> u16 {
        self.schema_version
    }
}
