//! Stage 5G-e-d-a fresh mock broker-truth contract.
//!
//! This module deliberately stops before reconciliation. It validates and
//! canonicalizes a fresh, complete-or-explicitly-incomplete mock snapshot, but
//! owns no strategy callback, runtime mutation, Redis client, FINAM transport,
//! HTTP endpoint or broker dispatch authority.

use std::collections::BTreeSet;
use std::fmt::Write;

use broker_core::{
    instrument_identity_matches, BrokerAccountId, BrokerOrderSnapshot, BrokerPositionSnapshot,
    BrokerTradeSnapshot, InstrumentId, OrderStatus, OrderType,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const STAGE5G_FRESH_BROKER_TRUTH_SCHEMA_VERSION: u16 = 1;

macro_rules! stage5g_string_identity {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
        #[serde(transparent)]
        pub(crate) struct $name(String);

        impl $name {
            fn parse(value: impl Into<String>) -> Result<Self, Stage5gFreshBrokerTruthError> {
                let value = value.into();
                if !canonical_identity_token(&value) {
                    return Err(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity);
                }
                Ok(Self(value))
            }

            fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

stage5g_string_identity!(Stage5gBrokerId);
stage5g_string_identity!(Stage5gStrategyDefinitionId);
stage5g_string_identity!(Stage5gStrategyInstanceId);
stage5g_string_identity!(Stage5gDeploymentId);
stage5g_string_identity!(Stage5gGatewayInstanceId);
stage5g_string_identity!(Stage5gPackageId);
stage5g_string_identity!(Stage5gSnapshotEpoch);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub(crate) struct Stage5gDeploymentGeneration(u64);

impl Stage5gDeploymentGeneration {
    fn parse(value: u64) -> Result<Self, Stage5gFreshBrokerTruthError> {
        if value == 0 {
            return Err(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity);
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub(crate) struct Stage5gFeedGeneration(u64);

impl Stage5gFeedGeneration {
    fn parse(value: u64) -> Result<Self, Stage5gFreshBrokerTruthError> {
        if value == 0 {
            return Err(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity);
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub(crate) struct Stage5gSha256(String);

impl Stage5gSha256 {
    fn parse(value: impl Into<String>) -> Result<Self, Stage5gFreshBrokerTruthError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .as_bytes()
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        {
            return Err(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity);
        }
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

/// Identity fields required before any later reconciliation callback may be
/// considered. All members are typed; a free-form `source` string is not an
/// identity proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage5gOperationalIdentityV1 {
    broker_id: Stage5gBrokerId,
    account_id: BrokerAccountId,
    strategy_definition_id: Stage5gStrategyDefinitionId,
    strategy_instance_id: Stage5gStrategyInstanceId,
    deployment_id: Stage5gDeploymentId,
    deployment_generation: Stage5gDeploymentGeneration,
    gateway_instance_id: Stage5gGatewayInstanceId,
    config_fingerprint_sha256: Stage5gSha256,
    instrument_map_fingerprint_sha256: Stage5gSha256,
    market_data_generation: Stage5gFeedGeneration,
    command_consumer_generation: Stage5gFeedGeneration,
    target_instrument: InstrumentId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage5gOperationalIdentityInput {
    pub(crate) broker_id: String,
    pub(crate) account_id: BrokerAccountId,
    pub(crate) strategy_definition_id: String,
    pub(crate) strategy_instance_id: String,
    pub(crate) deployment_id: String,
    pub(crate) deployment_generation: u64,
    pub(crate) gateway_instance_id: String,
    pub(crate) config_fingerprint_sha256: String,
    pub(crate) instrument_map_fingerprint_sha256: String,
    pub(crate) market_data_generation: u64,
    pub(crate) command_consumer_generation: u64,
    pub(crate) target_instrument: InstrumentId,
}

impl Stage5gOperationalIdentityV1 {
    pub(crate) fn validate(
        input: Stage5gOperationalIdentityInput,
    ) -> Result<Self, Stage5gFreshBrokerTruthError> {
        if !canonical_identity_token(input.account_id.as_str())
            || !canonical_identity_token(&input.target_instrument.symbol)
            || input
                .target_instrument
                .venue_symbol
                .as_deref()
                .is_some_and(|value| !canonical_identity_token(value))
        {
            return Err(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity);
        }
        Ok(Self {
            broker_id: Stage5gBrokerId::parse(input.broker_id)?,
            account_id: input.account_id,
            strategy_definition_id: Stage5gStrategyDefinitionId::parse(
                input.strategy_definition_id,
            )?,
            strategy_instance_id: Stage5gStrategyInstanceId::parse(input.strategy_instance_id)?,
            deployment_id: Stage5gDeploymentId::parse(input.deployment_id)?,
            deployment_generation: Stage5gDeploymentGeneration::parse(input.deployment_generation)?,
            gateway_instance_id: Stage5gGatewayInstanceId::parse(input.gateway_instance_id)?,
            config_fingerprint_sha256: Stage5gSha256::parse(input.config_fingerprint_sha256)?,
            instrument_map_fingerprint_sha256: Stage5gSha256::parse(
                input.instrument_map_fingerprint_sha256,
            )?,
            market_data_generation: Stage5gFeedGeneration::parse(input.market_data_generation)?,
            command_consumer_generation: Stage5gFeedGeneration::parse(
                input.command_consumer_generation,
            )?,
            target_instrument: input.target_instrument,
        })
    }
}

/// Untrusted mock package input. Its explicit completeness flags survive
/// validation; an incomplete empty section is not canonical broker absence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage5gFreshBrokerTruthPackageV1 {
    pub(crate) schema_version: u16,
    pub(crate) package_id: String,
    pub(crate) operational_identity: Stage5gOperationalIdentityInput,
    pub(crate) snapshot_epoch: String,
    pub(crate) captured_at: DateTime<Utc>,
    pub(crate) orders_observed_at: DateTime<Utc>,
    pub(crate) trades_observed_at: DateTime<Utc>,
    pub(crate) positions_observed_at: DateTime<Utc>,
    pub(crate) orders_complete: bool,
    pub(crate) trades_complete: bool,
    pub(crate) positions_complete: bool,
    pub(crate) orders: Vec<BrokerOrderSnapshot>,
    pub(crate) trades: Vec<BrokerTradeSnapshot>,
    pub(crate) positions: Vec<BrokerPositionSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stage5gReconciledFreshPackageIdentity {
    package_id: Stage5gPackageId,
    snapshot_epoch: Stage5gSnapshotEpoch,
    canonical_fingerprint_sha256: Stage5gSha256,
}

impl Stage5gReconciledFreshPackageIdentity {
    pub(crate) fn validate(
        package_id: impl Into<String>,
        snapshot_epoch: impl Into<String>,
        canonical_fingerprint_sha256: impl Into<String>,
    ) -> Result<Self, Stage5gFreshBrokerTruthError> {
        Ok(Self {
            package_id: Stage5gPackageId::parse(package_id)
                .map_err(|_| Stage5gFreshBrokerTruthError::EmptyPackageId)?,
            snapshot_epoch: Stage5gSnapshotEpoch::parse(snapshot_epoch)
                .map_err(|_| Stage5gFreshBrokerTruthError::EmptySnapshotEpoch)?,
            canonical_fingerprint_sha256: Stage5gSha256::parse(canonical_fingerprint_sha256)
                .map_err(|_| Stage5gFreshBrokerTruthError::InvalidReplayFingerprint)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gFreshPackageLineage {
    NewFresh,
    ExactLastReconciledReplay,
    ExactAcceptedHistoricalReplay,
}

/// Context proving post-restore collection and separating pre-restart identity
/// from the last-reconciled and bounded historical replay authorities.
pub(crate) struct Stage5gFreshBrokerTruthValidationContext<'a> {
    pub(crate) expected_operational_identity: &'a Stage5gOperationalIdentityV1,
    pub(crate) pre_restart_package_id: &'a str,
    pub(crate) pre_restart_snapshot_epoch: &'a str,
    pub(crate) last_reconciled_fresh_package: Option<&'a Stage5gReconciledFreshPackageIdentity>,
    pub(crate) accepted_replay_ledger: &'a [Stage5gReconciledFreshPackageIdentity],
    pub(crate) known_historical_fresh_packages: &'a [Stage5gReconciledFreshPackageIdentity],
    pub(crate) clean_restore_completed_at: DateTime<Utc>,
    pub(crate) validation_observed_at: DateTime<Utc>,
}

/// Canonical, linear evidence. It is intentionally non-serializable and owns
/// no callback or mutation authority.
pub(crate) struct Stage5gValidatedFreshBrokerTruthPackage {
    package_id: Stage5gPackageId,
    snapshot_epoch: Stage5gSnapshotEpoch,
    operational_identity: Stage5gOperationalIdentityV1,
    captured_at: DateTime<Utc>,
    orders_observed_at: DateTime<Utc>,
    trades_observed_at: DateTime<Utc>,
    positions_observed_at: DateTime<Utc>,
    orders_complete: bool,
    trades_complete: bool,
    positions_complete: bool,
    orders: Vec<BrokerOrderSnapshot>,
    trades: Vec<BrokerTradeSnapshot>,
    positions: Vec<BrokerPositionSnapshot>,
    lineage: Stage5gFreshPackageLineage,
    canonical_fingerprint_sha256: String,
}

impl Stage5gValidatedFreshBrokerTruthPackage {
    pub(crate) fn all_sections_complete(&self) -> bool {
        self.orders_complete && self.trades_complete && self.positions_complete
    }

    pub(crate) fn canonical_fingerprint_sha256(&self) -> &str {
        &self.canonical_fingerprint_sha256
    }

    pub(crate) fn lineage(&self) -> Stage5gFreshPackageLineage {
        self.lineage
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gRestartReconciliationDisposition {
    ExactReplay,
    ContinueFromCommittedCheckpoint,
    ApplyOwnedCandidate,
    AwaitFreshBrokerTruth,
    ReconciliationRequired,
    ManualInterventionRequired,
    TerminalInconsistency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Stage5gRestartScenarioId {
    Grst01RestartBeforeAck,
    Grst02RestartAfterAckBeforeOrder,
    Grst03RestartWithWorkingOrder,
    Grst04RestartAfterPartialFill,
    Grst05RestartFilledBeforePosition,
    Grst06RestartAfterTerminalPositionApplied,
    Grst07RestartAtTimerCheckpoint,
    Grst08RestartWithGeneratedIntentEscrow,
    Grst09ExactReplayIsIdempotent,
    Grst10ConflictingReplayBlocks,
    Grst11FreshBrokerTruthOverridesStaleHint,
    Grst12MissingOrAmbiguousTruthRequiresReconciliation,
}

impl Stage5gRestartScenarioId {
    pub(crate) const ALL: [Self; 12] = [
        Self::Grst01RestartBeforeAck,
        Self::Grst02RestartAfterAckBeforeOrder,
        Self::Grst03RestartWithWorkingOrder,
        Self::Grst04RestartAfterPartialFill,
        Self::Grst05RestartFilledBeforePosition,
        Self::Grst06RestartAfterTerminalPositionApplied,
        Self::Grst07RestartAtTimerCheckpoint,
        Self::Grst08RestartWithGeneratedIntentEscrow,
        Self::Grst09ExactReplayIsIdempotent,
        Self::Grst10ConflictingReplayBlocks,
        Self::Grst11FreshBrokerTruthOverridesStaleHint,
        Self::Grst12MissingOrAmbiguousTruthRequiresReconciliation,
    ];

    pub(crate) const fn frozen_id(self) -> &'static str {
        match self {
            Self::Grst01RestartBeforeAck => "GRST01_RESTART_BEFORE_ACK",
            Self::Grst02RestartAfterAckBeforeOrder => "GRST02_RESTART_AFTER_ACK_BEFORE_ORDER",
            Self::Grst03RestartWithWorkingOrder => "GRST03_RESTART_WITH_WORKING_ORDER",
            Self::Grst04RestartAfterPartialFill => "GRST04_RESTART_AFTER_PARTIAL_FILL",
            Self::Grst05RestartFilledBeforePosition => "GRST05_RESTART_FILLED_BEFORE_POSITION",
            Self::Grst06RestartAfterTerminalPositionApplied => {
                "GRST06_RESTART_AFTER_TERMINAL_POSITION_APPLIED"
            }
            Self::Grst07RestartAtTimerCheckpoint => "GRST07_RESTART_AT_TIMER_CHECKPOINT",
            Self::Grst08RestartWithGeneratedIntentEscrow => {
                "GRST08_RESTART_WITH_GENERATED_INTENT_ESCROW"
            }
            Self::Grst09ExactReplayIsIdempotent => "GRST09_EXACT_REPLAY_IS_IDEMPOTENT",
            Self::Grst10ConflictingReplayBlocks => "GRST10_CONFLICTING_REPLAY_BLOCKS",
            Self::Grst11FreshBrokerTruthOverridesStaleHint => {
                "GRST11_FRESH_BROKER_TRUTH_OVERRIDES_STALE_HINT"
            }
            Self::Grst12MissingOrAmbiguousTruthRequiresReconciliation => {
                "GRST12_MISSING_OR_AMBIGUOUS_TRUTH_REQUIRES_RECONCILIATION"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5gFreshBrokerTruthError {
    UnsupportedSchemaVersion,
    InvalidOperationalIdentity,
    OperationalIdentityMismatch,
    EmptyPackageId,
    EmptySnapshotEpoch,
    InvalidReplayFingerprint,
    ReplayedPackageId,
    ReplayedSnapshotEpoch,
    ReusedFreshSnapshotEpoch,
    FreshPackageIdentityConflict,
    HistoricalReplayNotAccepted,
    InvalidReplayLedger,
    PackageNotCapturedAfterRestore,
    PackageCapturedAfterValidation,
    SectionNotObservedAfterRestore,
    SectionObservedAfterPackageCapture,
    RowAccountMismatch,
    RowReceivedBeforeCleanRestore,
    RowTimestampAfterSectionObservation,
    RowSourceTimestampAfterReceipt,
    MalformedNativeId,
    InvalidOrderQuantity,
    InvalidOrderShape,
    InconsistentActiveZeroRemaining,
    InvalidTradeQuantity,
    OrderLifecycleMismatch,
    DuplicateOrderIdentity,
    DuplicateTradeIdentity,
    DuplicatePositionIdentity,
    CanonicalEncodingFailed,
}

pub(crate) fn validate_stage5g_fresh_broker_truth_package(
    mut package: Stage5gFreshBrokerTruthPackageV1,
    context: Stage5gFreshBrokerTruthValidationContext<'_>,
) -> Result<Stage5gValidatedFreshBrokerTruthPackage, Stage5gFreshBrokerTruthError> {
    if package.schema_version != STAGE5G_FRESH_BROKER_TRUTH_SCHEMA_VERSION {
        return Err(Stage5gFreshBrokerTruthError::UnsupportedSchemaVersion);
    }
    let package_id = Stage5gPackageId::parse(package.package_id.clone())
        .map_err(|_| Stage5gFreshBrokerTruthError::EmptyPackageId)?;
    let snapshot_epoch = Stage5gSnapshotEpoch::parse(package.snapshot_epoch.clone())
        .map_err(|_| Stage5gFreshBrokerTruthError::EmptySnapshotEpoch)?;
    if !canonical_identity_token(context.pre_restart_package_id)
        || !canonical_identity_token(context.pre_restart_snapshot_epoch)
    {
        return Err(Stage5gFreshBrokerTruthError::InvalidReplayLedger);
    }
    if package_id.as_str() == context.pre_restart_package_id {
        return Err(Stage5gFreshBrokerTruthError::ReplayedPackageId);
    }
    if snapshot_epoch.as_str() == context.pre_restart_snapshot_epoch {
        return Err(Stage5gFreshBrokerTruthError::ReplayedSnapshotEpoch);
    }
    if package.captured_at <= context.clean_restore_completed_at {
        return Err(Stage5gFreshBrokerTruthError::PackageNotCapturedAfterRestore);
    }
    if package.captured_at > context.validation_observed_at {
        return Err(Stage5gFreshBrokerTruthError::PackageCapturedAfterValidation);
    }
    validate_section_observation(
        package.orders_observed_at,
        package.captured_at,
        context.clean_restore_completed_at,
    )?;
    validate_section_observation(
        package.trades_observed_at,
        package.captured_at,
        context.clean_restore_completed_at,
    )?;
    validate_section_observation(
        package.positions_observed_at,
        package.captured_at,
        context.clean_restore_completed_at,
    )?;
    let operational_identity =
        Stage5gOperationalIdentityV1::validate(package.operational_identity.clone())?;
    if &operational_identity != context.expected_operational_identity {
        return Err(Stage5gFreshBrokerTruthError::OperationalIdentityMismatch);
    }

    validate_rows(&package, context.clean_restore_completed_at)?;
    canonicalize_rows(&mut package);
    let canonical_fingerprint_sha256 = package_fingerprint(&package)?;
    let lineage = classify_lineage(
        &package_id,
        &snapshot_epoch,
        &canonical_fingerprint_sha256,
        &context,
    )?;

    Ok(Stage5gValidatedFreshBrokerTruthPackage {
        package_id,
        snapshot_epoch,
        operational_identity,
        captured_at: package.captured_at,
        orders_observed_at: package.orders_observed_at,
        trades_observed_at: package.trades_observed_at,
        positions_observed_at: package.positions_observed_at,
        orders_complete: package.orders_complete,
        trades_complete: package.trades_complete,
        positions_complete: package.positions_complete,
        orders: package.orders,
        trades: package.trades,
        positions: package.positions,
        lineage,
        canonical_fingerprint_sha256,
    })
}

fn validate_section_observation(
    observed_at: DateTime<Utc>,
    captured_at: DateTime<Utc>,
    clean_restore_completed_at: DateTime<Utc>,
) -> Result<(), Stage5gFreshBrokerTruthError> {
    if observed_at <= clean_restore_completed_at {
        return Err(Stage5gFreshBrokerTruthError::SectionNotObservedAfterRestore);
    }
    if observed_at > captured_at {
        return Err(Stage5gFreshBrokerTruthError::SectionObservedAfterPackageCapture);
    }
    Ok(())
}

fn classify_lineage(
    package_id: &Stage5gPackageId,
    snapshot_epoch: &Stage5gSnapshotEpoch,
    fingerprint: &str,
    context: &Stage5gFreshBrokerTruthValidationContext<'_>,
) -> Result<Stage5gFreshPackageLineage, Stage5gFreshBrokerTruthError> {
    validate_replay_ledger(context)?;
    if let Some(last) = context.last_reconciled_fresh_package {
        if package_id == &last.package_id {
            return exact_lineage_or_conflict(
                snapshot_epoch,
                fingerprint,
                last,
                Stage5gFreshPackageLineage::ExactLastReconciledReplay,
            );
        }
    }
    if let Some(historical) = context
        .accepted_replay_ledger
        .iter()
        .find(|entry| &entry.package_id == package_id)
    {
        return exact_lineage_or_conflict(
            snapshot_epoch,
            fingerprint,
            historical,
            Stage5gFreshPackageLineage::ExactAcceptedHistoricalReplay,
        );
    }
    if let Some(known) = context
        .known_historical_fresh_packages
        .iter()
        .find(|entry| &entry.package_id == package_id)
    {
        if known.snapshot_epoch != *snapshot_epoch
            || known.canonical_fingerprint_sha256.as_str() != fingerprint
        {
            return Err(Stage5gFreshBrokerTruthError::FreshPackageIdentityConflict);
        }
        return Err(Stage5gFreshBrokerTruthError::HistoricalReplayNotAccepted);
    }
    if context
        .last_reconciled_fresh_package
        .into_iter()
        .chain(context.accepted_replay_ledger.iter())
        .chain(context.known_historical_fresh_packages.iter())
        .any(|entry| entry.snapshot_epoch == *snapshot_epoch)
    {
        return Err(Stage5gFreshBrokerTruthError::ReusedFreshSnapshotEpoch);
    }
    Ok(Stage5gFreshPackageLineage::NewFresh)
}

fn exact_lineage_or_conflict(
    snapshot_epoch: &Stage5gSnapshotEpoch,
    fingerprint: &str,
    expected: &Stage5gReconciledFreshPackageIdentity,
    exact: Stage5gFreshPackageLineage,
) -> Result<Stage5gFreshPackageLineage, Stage5gFreshBrokerTruthError> {
    if snapshot_epoch == &expected.snapshot_epoch
        && fingerprint == expected.canonical_fingerprint_sha256.as_str()
    {
        Ok(exact)
    } else {
        Err(Stage5gFreshBrokerTruthError::FreshPackageIdentityConflict)
    }
}

fn validate_replay_ledger(
    context: &Stage5gFreshBrokerTruthValidationContext<'_>,
) -> Result<(), Stage5gFreshBrokerTruthError> {
    let mut ids = BTreeSet::new();
    let mut epochs = BTreeSet::new();
    for entry in context
        .last_reconciled_fresh_package
        .into_iter()
        .chain(context.accepted_replay_ledger.iter())
        .chain(context.known_historical_fresh_packages.iter())
    {
        if !ids.insert(entry.package_id.as_str()) || !epochs.insert(entry.snapshot_epoch.as_str()) {
            return Err(Stage5gFreshBrokerTruthError::InvalidReplayLedger);
        }
    }
    Ok(())
}

fn validate_rows(
    package: &Stage5gFreshBrokerTruthPackageV1,
    clean_restore_completed_at: DateTime<Utc>,
) -> Result<(), Stage5gFreshBrokerTruthError> {
    let account = &package.operational_identity.account_id;
    let mut broker_order_ids = BTreeSet::new();
    let mut client_order_ids = BTreeSet::new();
    let mut orphan_order_identities = BTreeSet::new();
    let mut trade_ids = BTreeSet::new();

    for order in &package.orders {
        if &order.account_id != account {
            return Err(Stage5gFreshBrokerTruthError::RowAccountMismatch);
        }
        if order.received_ts < clean_restore_completed_at {
            return Err(Stage5gFreshBrokerTruthError::RowReceivedBeforeCleanRestore);
        }
        if order.received_ts > package.orders_observed_at {
            return Err(Stage5gFreshBrokerTruthError::RowTimestampAfterSectionObservation);
        }
        if order
            .source_ts
            .is_some_and(|source_ts| source_ts > order.received_ts)
        {
            return Err(Stage5gFreshBrokerTruthError::RowSourceTimestampAfterReceipt);
        }
        if order.lifecycle != BrokerOrderSnapshot::lifecycle_for(&order.status) {
            return Err(Stage5gFreshBrokerTruthError::OrderLifecycleMismatch);
        }
        if order
            .broker_order_id
            .as_ref()
            .is_some_and(|id| !canonical_native_id(id.as_str()))
            || order
                .client_order_id
                .as_ref()
                .is_some_and(|id| !canonical_native_id(id.as_str()))
        {
            return Err(Stage5gFreshBrokerTruthError::MalformedNativeId);
        }
        if order.qty <= Decimal::ZERO
            || order.filled_qty < Decimal::ZERO
            || order.filled_qty > order.qty
            || match order.remaining_qty {
                Some(remaining) => {
                    remaining < Decimal::ZERO || remaining != order.qty - order.filled_qty
                }
                None => true,
            }
        {
            return Err(Stage5gFreshBrokerTruthError::InvalidOrderQuantity);
        }
        if matches!(order.status, OrderStatus::Filled) && order.filled_qty != order.qty {
            return Err(Stage5gFreshBrokerTruthError::InvalidOrderQuantity);
        }
        if order.is_inconsistent_active_zero_remaining() {
            return Err(Stage5gFreshBrokerTruthError::InconsistentActiveZeroRemaining);
        }
        match order.order_type {
            OrderType::Market if order.limit_price.is_some() => {
                return Err(Stage5gFreshBrokerTruthError::InvalidOrderShape);
            }
            OrderType::Limit
                if match order.limit_price {
                    Some(price) => price <= Decimal::ZERO,
                    None => true,
                } =>
            {
                return Err(Stage5gFreshBrokerTruthError::InvalidOrderShape);
            }
            _ => {}
        }
        if order
            .broker_order_id
            .as_ref()
            .is_some_and(|id| !broker_order_ids.insert(id.as_str().to_owned()))
            || order
                .client_order_id
                .as_ref()
                .is_some_and(|id| !client_order_ids.insert(id.as_str().to_owned()))
        {
            return Err(Stage5gFreshBrokerTruthError::DuplicateOrderIdentity);
        }
        if order.broker_order_id.is_none()
            && order.client_order_id.is_none()
            && !orphan_order_identities.insert(order_identity(order))
        {
            return Err(Stage5gFreshBrokerTruthError::DuplicateOrderIdentity);
        }
    }
    for trade in &package.trades {
        if &trade.account_id != account {
            return Err(Stage5gFreshBrokerTruthError::RowAccountMismatch);
        }
        if trade.received_ts < clean_restore_completed_at {
            return Err(Stage5gFreshBrokerTruthError::RowReceivedBeforeCleanRestore);
        }
        if trade.received_ts > package.trades_observed_at {
            return Err(Stage5gFreshBrokerTruthError::RowTimestampAfterSectionObservation);
        }
        if trade.source_ts > trade.received_ts {
            return Err(Stage5gFreshBrokerTruthError::RowSourceTimestampAfterReceipt);
        }
        if trade.qty <= Decimal::ZERO || trade.price <= Decimal::ZERO {
            return Err(Stage5gFreshBrokerTruthError::InvalidTradeQuantity);
        }
        if !canonical_native_id(trade.broker_trade_id.as_str())
            || trade
                .broker_order_id
                .as_ref()
                .is_some_and(|id| !canonical_native_id(id.as_str()))
            || trade
                .client_order_id
                .as_ref()
                .is_some_and(|id| !canonical_native_id(id.as_str()))
        {
            return Err(Stage5gFreshBrokerTruthError::MalformedNativeId);
        }
        if !trade_ids.insert(trade.broker_trade_id.as_str().to_owned()) {
            return Err(Stage5gFreshBrokerTruthError::DuplicateTradeIdentity);
        }
    }
    for (index, position) in package.positions.iter().enumerate() {
        if &position.account_id != account {
            return Err(Stage5gFreshBrokerTruthError::RowAccountMismatch);
        }
        if position.received_ts < clean_restore_completed_at {
            return Err(Stage5gFreshBrokerTruthError::RowReceivedBeforeCleanRestore);
        }
        if position.received_ts > package.positions_observed_at {
            return Err(Stage5gFreshBrokerTruthError::RowTimestampAfterSectionObservation);
        }
        if position
            .source_ts
            .is_some_and(|source_ts| source_ts > position.received_ts)
        {
            return Err(Stage5gFreshBrokerTruthError::RowSourceTimestampAfterReceipt);
        }
        if package.positions[..index]
            .iter()
            .any(|previous| instrument_identity_matches(&previous.instrument, &position.instrument))
        {
            return Err(Stage5gFreshBrokerTruthError::DuplicatePositionIdentity);
        }
    }
    Ok(())
}

/// Canonical Stage 5G identity-token grammar: non-empty UTF-8 with no Unicode
/// whitespace or control characters. Hyphen, colon and other visible Unicode
/// scalar values serialize deterministically and remain allowed.
fn canonical_identity_token(value: &str) -> bool {
    !value.is_empty()
        && value == value.trim()
        && !value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
}

fn canonical_native_id(value: &str) -> bool {
    canonical_identity_token(value)
}

fn canonicalize_rows(package: &mut Stage5gFreshBrokerTruthPackageV1) {
    package.orders.sort_by_key(order_identity);
    package
        .trades
        .sort_by_key(|trade| trade.broker_trade_id.as_str().to_owned());
    package
        .positions
        .sort_by_key(|position| instrument_key(&position.instrument));
}

fn order_identity(order: &BrokerOrderSnapshot) -> String {
    if let Some(id) = &order.broker_order_id {
        return format!("broker:{}", id.as_str());
    }
    if let Some(id) = &order.client_order_id {
        return format!("client:{}", id.as_str());
    }
    format!(
        "orphan:{}:{}",
        instrument_key(&order.instrument),
        order.received_ts.timestamp_nanos_opt().unwrap_or_default()
    )
}

fn instrument_key(instrument: &InstrumentId) -> String {
    let venue = instrument.venue_symbol.as_deref().unwrap_or_default();
    let exchange = serde_json::to_string(&instrument.exchange).unwrap_or_default();
    let market = serde_json::to_string(&instrument.market).unwrap_or_default();
    format!(
        "{}\u{1f}{venue}\u{1f}{exchange}\u{1f}{market}",
        instrument.symbol
    )
}

fn package_fingerprint(
    package: &Stage5gFreshBrokerTruthPackageV1,
) -> Result<String, Stage5gFreshBrokerTruthError> {
    let encoded = serde_json::to_vec(package)
        .map_err(|_| Stage5gFreshBrokerTruthError::CanonicalEncodingFailed)?;
    let digest = Sha256::digest(encoded);
    let mut fingerprint = String::with_capacity(64);
    for byte in digest {
        write!(&mut fingerprint, "{byte:02x}")
            .map_err(|_| Stage5gFreshBrokerTruthError::CanonicalEncodingFailed)?;
    }
    Ok(fingerprint)
}

#[cfg(test)]
mod tests {
    use broker_core::{
        BrokerAccountId, BrokerOrderId, BrokerOrderLifecycle, BrokerTradeId, ClientOrderId,
        Exchange, Market, OrderSide, OrderStatus, OrderType,
    };
    use chrono::{Duration, TimeZone};
    use rust_decimal::Decimal;

    use super::*;

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_owned(),
            venue_symbol: Some("IMOEXF@RTSX".to_owned()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn identity_input() -> Stage5gOperationalIdentityInput {
        Stage5gOperationalIdentityInput {
            broker_id: "finam-mock".to_owned(),
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            strategy_definition_id: "hybrid-imoexf".to_owned(),
            strategy_instance_id: "hybrid-imoexf-paper-1".to_owned(),
            deployment_id: "stage5g-test".to_owned(),
            deployment_generation: 7,
            gateway_instance_id: "mock-gateway-1".to_owned(),
            config_fingerprint_sha256: "a".repeat(64),
            instrument_map_fingerprint_sha256: "b".repeat(64),
            market_data_generation: 3,
            command_consumer_generation: 4,
            target_instrument: instrument(),
        }
    }

    fn identity() -> Stage5gOperationalIdentityV1 {
        Stage5gOperationalIdentityV1::validate(identity_input()).expect("valid identity")
    }

    fn package() -> Stage5gFreshBrokerTruthPackageV1 {
        let captured_at = Utc.with_ymd_and_hms(2026, 8, 4, 9, 0, 10).unwrap();
        let account_id = BrokerAccountId::new("ACC_TEST_0001");
        let order = BrokerOrderSnapshot {
            account_id: account_id.clone(),
            broker_order_id: Some(BrokerOrderId::new("ORDER-1")),
            client_order_id: Some(ClientOrderId::new("CLIENT-1").expect("client id")),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: None,
            status: OrderStatus::Working,
            lifecycle: BrokerOrderLifecycle::Active,
            qty: Decimal::ONE,
            filled_qty: Decimal::ZERO,
            remaining_qty: Some(Decimal::ONE),
            limit_price: Some(Decimal::new(2210, 0)),
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: Some(captured_at - Duration::seconds(2)),
            received_ts: captured_at - Duration::seconds(1),
        };
        let trade = BrokerTradeSnapshot {
            account_id: account_id.clone(),
            broker_trade_id: BrokerTradeId::new("TRADE-1"),
            broker_order_id: Some(BrokerOrderId::new("ORDER-1")),
            client_order_id: None,
            instrument: instrument(),
            side: OrderSide::Buy,
            qty: Decimal::ONE,
            price: Decimal::new(2210, 0),
            gross_amount: None,
            commission: None,
            broker_asset_id: None,
            board: None,
            expiration_date: None,
            source_ts: captured_at - Duration::seconds(3),
            received_ts: captured_at - Duration::seconds(1),
        };
        let position = BrokerPositionSnapshot {
            account_id,
            instrument: instrument(),
            qty: Decimal::ONE,
            avg_price: Some(Decimal::new(2210, 0)),
            unrealized_pnl: None,
            source_ts: Some(captured_at - Duration::seconds(2)),
            received_ts: captured_at - Duration::seconds(1),
        };
        Stage5gFreshBrokerTruthPackageV1 {
            schema_version: STAGE5G_FRESH_BROKER_TRUTH_SCHEMA_VERSION,
            package_id: "fresh-package-1".to_owned(),
            operational_identity: identity_input(),
            snapshot_epoch: "snapshot-epoch-2".to_owned(),
            captured_at,
            orders_observed_at: captured_at - Duration::seconds(1),
            trades_observed_at: captured_at - Duration::seconds(1),
            positions_observed_at: captured_at - Duration::seconds(1),
            orders_complete: true,
            trades_complete: true,
            positions_complete: true,
            orders: vec![order],
            trades: vec![trade],
            positions: vec![position],
        }
    }

    fn validate_with_lineage(
        package: Stage5gFreshBrokerTruthPackageV1,
        last_reconciled_fresh_package: Option<&Stage5gReconciledFreshPackageIdentity>,
        accepted_replay_ledger: &[Stage5gReconciledFreshPackageIdentity],
        known_historical_fresh_packages: &[Stage5gReconciledFreshPackageIdentity],
    ) -> Result<Stage5gValidatedFreshBrokerTruthPackage, Stage5gFreshBrokerTruthError> {
        let expected = identity();
        let restore_at = package.captured_at - Duration::seconds(5);
        let validation_observed_at = package.captured_at + Duration::seconds(1);
        validate_stage5g_fresh_broker_truth_package(
            package,
            Stage5gFreshBrokerTruthValidationContext {
                expected_operational_identity: &expected,
                pre_restart_package_id: "fresh-package-0",
                pre_restart_snapshot_epoch: "snapshot-epoch-1",
                last_reconciled_fresh_package,
                accepted_replay_ledger,
                known_historical_fresh_packages,
                clean_restore_completed_at: restore_at,
                validation_observed_at,
            },
        )
    }

    fn validate(
        package: Stage5gFreshBrokerTruthPackageV1,
    ) -> Result<Stage5gValidatedFreshBrokerTruthPackage, Stage5gFreshBrokerTruthError> {
        validate_with_lineage(package, None, &[], &[])
    }

    fn reconciled_identity(
        validated: &Stage5gValidatedFreshBrokerTruthPackage,
    ) -> Stage5gReconciledFreshPackageIdentity {
        Stage5gReconciledFreshPackageIdentity::validate(
            validated.package_id.as_str(),
            "snapshot-epoch-2",
            validated.canonical_fingerprint_sha256(),
        )
        .expect("valid reconciled package identity")
    }

    #[test]
    fn frozen_restart_ids_are_exact_and_unique() {
        let ids = Stage5gRestartScenarioId::ALL
            .into_iter()
            .map(Stage5gRestartScenarioId::frozen_id)
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "GRST01_RESTART_BEFORE_ACK",
                "GRST02_RESTART_AFTER_ACK_BEFORE_ORDER",
                "GRST03_RESTART_WITH_WORKING_ORDER",
                "GRST04_RESTART_AFTER_PARTIAL_FILL",
                "GRST05_RESTART_FILLED_BEFORE_POSITION",
                "GRST06_RESTART_AFTER_TERMINAL_POSITION_APPLIED",
                "GRST07_RESTART_AT_TIMER_CHECKPOINT",
                "GRST08_RESTART_WITH_GENERATED_INTENT_ESCROW",
                "GRST09_EXACT_REPLAY_IS_IDEMPOTENT",
                "GRST10_CONFLICTING_REPLAY_BLOCKS",
                "GRST11_FRESH_BROKER_TRUTH_OVERRIDES_STALE_HINT",
                "GRST12_MISSING_OR_AMBIGUOUS_TRUTH_REQUIRES_RECONCILIATION",
            ]
        );
    }

    #[test]
    fn package_schema_and_capture_window_fail_closed() {
        let mut unsupported = package();
        unsupported.schema_version += 1;
        assert_eq!(
            validate(unsupported).err(),
            Some(Stage5gFreshBrokerTruthError::UnsupportedSchemaVersion)
        );

        let captured_at = package().captured_at;
        for restore_at in [captured_at, captured_at + Duration::milliseconds(1)] {
            let expected = identity();
            let result = validate_stage5g_fresh_broker_truth_package(
                package(),
                Stage5gFreshBrokerTruthValidationContext {
                    expected_operational_identity: &expected,
                    pre_restart_package_id: "fresh-package-0",
                    pre_restart_snapshot_epoch: "snapshot-epoch-1",
                    last_reconciled_fresh_package: None,
                    accepted_replay_ledger: &[],
                    known_historical_fresh_packages: &[],
                    clean_restore_completed_at: restore_at,
                    validation_observed_at: captured_at + Duration::seconds(1),
                },
            );
            assert_eq!(
                result.err(),
                Some(Stage5gFreshBrokerTruthError::PackageNotCapturedAfterRestore),
                "capture at/before restore survived: {restore_at}"
            );
        }

        let expected = identity();
        let after_observation = package();
        let captured_at = after_observation.captured_at;
        let result = validate_stage5g_fresh_broker_truth_package(
            after_observation,
            Stage5gFreshBrokerTruthValidationContext {
                expected_operational_identity: &expected,
                pre_restart_package_id: "fresh-package-0",
                pre_restart_snapshot_epoch: "snapshot-epoch-1",
                last_reconciled_fresh_package: None,
                accepted_replay_ledger: &[],
                known_historical_fresh_packages: &[],
                clean_restore_completed_at: captured_at - Duration::seconds(5),
                validation_observed_at: captured_at - Duration::milliseconds(1),
            },
        );
        assert_eq!(
            result.err(),
            Some(Stage5gFreshBrokerTruthError::PackageCapturedAfterValidation)
        );
    }

    #[test]
    fn each_broker_row_is_bound_to_the_package_account() {
        let mut order_mismatch = package();
        order_mismatch.orders[0].account_id = BrokerAccountId::new("ACC_OTHER");
        assert_eq!(
            validate(order_mismatch).err(),
            Some(Stage5gFreshBrokerTruthError::RowAccountMismatch)
        );

        let mut trade_mismatch = package();
        trade_mismatch.trades[0].account_id = BrokerAccountId::new("ACC_OTHER");
        assert_eq!(
            validate(trade_mismatch).err(),
            Some(Stage5gFreshBrokerTruthError::RowAccountMismatch)
        );

        let mut position_mismatch = package();
        position_mismatch.positions[0].account_id = BrokerAccountId::new("ACC_OTHER");
        assert_eq!(
            validate(position_mismatch).err(),
            Some(Stage5gFreshBrokerTruthError::RowAccountMismatch)
        );
    }

    #[test]
    fn trade_identity_quantity_and_price_fail_closed() {
        let mut duplicate = package();
        duplicate.trades.push(duplicate.trades[0].clone());
        assert_eq!(
            validate(duplicate).err(),
            Some(Stage5gFreshBrokerTruthError::DuplicateTradeIdentity)
        );

        for qty in [Decimal::ZERO, -Decimal::ONE] {
            let mut invalid = package();
            invalid.trades[0].qty = qty;
            assert_eq!(
                validate(invalid).err(),
                Some(Stage5gFreshBrokerTruthError::InvalidTradeQuantity),
                "trade quantity survived: {qty}"
            );
        }
        for price in [Decimal::ZERO, -Decimal::ONE] {
            let mut invalid = package();
            invalid.trades[0].price = price;
            assert_eq!(
                validate(invalid).err(),
                Some(Stage5gFreshBrokerTruthError::InvalidTradeQuantity),
                "trade price survived: {price}"
            );
        }
    }

    #[test]
    fn market_and_limit_order_shapes_fail_closed() {
        let mut market_with_price = package();
        market_with_price.orders[0].order_type = OrderType::Market;
        assert_eq!(
            validate(market_with_price).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOrderShape)
        );

        let mut limit_without_price = package();
        limit_without_price.orders[0].limit_price = None;
        assert_eq!(
            validate(limit_without_price).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOrderShape)
        );

        let mut limit_with_zero_price = package();
        limit_with_zero_price.orders[0].limit_price = Some(Decimal::ZERO);
        assert_eq!(
            validate(limit_with_zero_price).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOrderShape)
        );
    }

    #[test]
    fn replay_ledger_rejects_duplicate_package_or_epoch_identity() {
        let first = Stage5gReconciledFreshPackageIdentity::validate(
            "ledger-package-a",
            "ledger-epoch-a",
            "a".repeat(64),
        )
        .expect("first ledger identity");
        let duplicate_package = Stage5gReconciledFreshPackageIdentity::validate(
            "ledger-package-a",
            "ledger-epoch-b",
            "b".repeat(64),
        )
        .expect("duplicate package identity");
        assert_eq!(
            validate_with_lineage(package(), None, &[first, duplicate_package], &[]).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidReplayLedger)
        );

        let first = Stage5gReconciledFreshPackageIdentity::validate(
            "ledger-package-a",
            "ledger-epoch-a",
            "a".repeat(64),
        )
        .expect("first ledger identity");
        let duplicate_epoch = Stage5gReconciledFreshPackageIdentity::validate(
            "ledger-package-b",
            "ledger-epoch-a",
            "b".repeat(64),
        )
        .expect("duplicate epoch identity");
        assert_eq!(
            validate_with_lineage(package(), None, &[first, duplicate_epoch], &[]).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidReplayLedger)
        );
    }

    #[test]
    fn every_order_quantity_component_fails_closed() {
        for qty in [Decimal::ZERO, -Decimal::ONE] {
            let mut invalid = package();
            invalid.orders[0].qty = qty;
            assert_eq!(
                validate(invalid).err(),
                Some(Stage5gFreshBrokerTruthError::InvalidOrderQuantity),
                "order quantity survived: {qty}"
            );
        }

        let mut negative_filled = package();
        negative_filled.orders[0].filled_qty = -Decimal::ONE;
        assert_eq!(
            validate(negative_filled).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOrderQuantity)
        );

        let mut overfilled = package();
        overfilled.orders[0].filled_qty = Decimal::new(2, 0);
        assert_eq!(
            validate(overfilled).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOrderQuantity)
        );

        let mut negative_remaining = package();
        negative_remaining.orders[0].remaining_qty = Some(-Decimal::ONE);
        assert_eq!(
            validate(negative_remaining).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOrderQuantity)
        );

        let mut inconsistent_remaining = package();
        inconsistent_remaining.orders[0].remaining_qty = Some(Decimal::new(2, 0));
        assert_eq!(
            validate(inconsistent_remaining).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOrderQuantity)
        );
    }

    #[test]
    fn identity_generations_target_and_instrument_map_hash_fail_closed() {
        let mut invalid_target = identity_input();
        invalid_target.target_instrument.symbol = "IMO EXF".to_owned();
        assert_eq!(
            Stage5gOperationalIdentityV1::validate(invalid_target).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity)
        );

        let mut zero_market_generation = identity_input();
        zero_market_generation.market_data_generation = 0;
        assert_eq!(
            Stage5gOperationalIdentityV1::validate(zero_market_generation).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity)
        );

        let mut zero_command_generation = identity_input();
        zero_command_generation.command_consumer_generation = 0;
        assert_eq!(
            Stage5gOperationalIdentityV1::validate(zero_command_generation).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity)
        );

        let mut malformed_instrument_map_hash = identity_input();
        malformed_instrument_map_hash.instrument_map_fingerprint_sha256 = "not-a-sha256".to_owned();
        assert_eq!(
            Stage5gOperationalIdentityV1::validate(malformed_instrument_map_hash).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity)
        );
    }

    #[test]
    fn invalid_pre_restart_identity_tokens_fail_closed() {
        for (pre_restart_package_id, pre_restart_snapshot_epoch) in [
            ("pre restart package", "snapshot-epoch-1"),
            ("fresh-package-0", "pre restart epoch"),
        ] {
            let package = package();
            let captured_at = package.captured_at;
            let expected = identity();
            let result = validate_stage5g_fresh_broker_truth_package(
                package,
                Stage5gFreshBrokerTruthValidationContext {
                    expected_operational_identity: &expected,
                    pre_restart_package_id,
                    pre_restart_snapshot_epoch,
                    last_reconciled_fresh_package: None,
                    accepted_replay_ledger: &[],
                    known_historical_fresh_packages: &[],
                    clean_restore_completed_at: captured_at - Duration::seconds(5),
                    validation_observed_at: captured_at + Duration::seconds(1),
                },
            );
            assert_eq!(
                result.err(),
                Some(Stage5gFreshBrokerTruthError::InvalidReplayLedger)
            );
        }
    }

    #[test]
    fn duplicate_idless_order_identity_fails_closed() {
        let mut duplicate = package();
        duplicate.orders[0].broker_order_id = None;
        duplicate.orders[0].client_order_id = None;
        duplicate.orders.push(duplicate.orders[0].clone());
        assert_eq!(
            validate(duplicate).err(),
            Some(Stage5gFreshBrokerTruthError::DuplicateOrderIdentity)
        );
    }

    #[test]
    fn new_package_reusing_known_snapshot_epoch_fails_closed() {
        let known = Stage5gReconciledFreshPackageIdentity::validate(
            "known-package",
            "snapshot-epoch-2",
            "c".repeat(64),
        )
        .expect("known historical identity");
        assert_eq!(
            validate_with_lineage(package(), None, &[], std::slice::from_ref(&known)).err(),
            Some(Stage5gFreshBrokerTruthError::ReusedFreshSnapshotEpoch)
        );
    }

    #[test]
    fn complete_fresh_package_is_canonical_and_deterministic() {
        let first = validate(package()).expect("fresh package validates");
        let second = validate(package()).expect("same package validates");
        assert!(first.all_sections_complete());
        assert_eq!(
            first.canonical_fingerprint_sha256(),
            second.canonical_fingerprint_sha256()
        );
        assert_eq!(first.lineage(), Stage5gFreshPackageLineage::NewFresh);
    }

    #[test]
    fn incomplete_section_is_preserved_not_treated_as_absence() {
        let mut package = package();
        package.positions_complete = false;
        package.positions.clear();
        let validated = validate(package).expect("incomplete package remains valid evidence");
        assert!(!validated.all_sections_complete());
    }

    #[test]
    fn pre_restart_epoch_and_identity_mismatch_fail_closed() {
        let expected = identity();
        let package = package();
        let restore_at = package.captured_at - Duration::seconds(5);
        let validation_observed_at = package.captured_at + Duration::seconds(1);
        let replayed_package = validate_stage5g_fresh_broker_truth_package(
            package.clone(),
            Stage5gFreshBrokerTruthValidationContext {
                expected_operational_identity: &expected,
                pre_restart_package_id: "fresh-package-1",
                pre_restart_snapshot_epoch: "snapshot-epoch-1",
                last_reconciled_fresh_package: None,
                accepted_replay_ledger: &[],
                known_historical_fresh_packages: &[],
                clean_restore_completed_at: restore_at,
                validation_observed_at,
            },
        );
        assert_eq!(
            replayed_package.err(),
            Some(Stage5gFreshBrokerTruthError::ReplayedPackageId)
        );

        let replayed = validate_stage5g_fresh_broker_truth_package(
            package.clone(),
            Stage5gFreshBrokerTruthValidationContext {
                expected_operational_identity: &expected,
                pre_restart_package_id: "fresh-package-0",
                pre_restart_snapshot_epoch: "snapshot-epoch-2",
                last_reconciled_fresh_package: None,
                accepted_replay_ledger: &[],
                known_historical_fresh_packages: &[],
                clean_restore_completed_at: restore_at,
                validation_observed_at,
            },
        );
        assert_eq!(
            replayed.err(),
            Some(Stage5gFreshBrokerTruthError::ReplayedSnapshotEpoch)
        );

        let mut wrong = identity();
        wrong.gateway_instance_id = Stage5gGatewayInstanceId::parse("mock-gateway-2").unwrap();
        let mismatch = validate_stage5g_fresh_broker_truth_package(
            package,
            Stage5gFreshBrokerTruthValidationContext {
                expected_operational_identity: &wrong,
                pre_restart_package_id: "fresh-package-0",
                pre_restart_snapshot_epoch: "snapshot-epoch-1",
                last_reconciled_fresh_package: None,
                accepted_replay_ledger: &[],
                known_historical_fresh_packages: &[],
                clean_restore_completed_at: restore_at,
                validation_observed_at,
            },
        );
        assert_eq!(
            mismatch.err(),
            Some(Stage5gFreshBrokerTruthError::OperationalIdentityMismatch)
        );
    }

    #[test]
    fn conflicting_rows_fail_before_any_reconciliation_authority_exists() {
        let mut package = package();
        package.orders.push(package.orders[0].clone());
        assert_eq!(
            validate(package).err(),
            Some(Stage5gFreshBrokerTruthError::DuplicateOrderIdentity)
        );
    }

    #[test]
    fn shared_client_id_fails_closed() {
        let mut shared_client = package();
        let mut second = shared_client.orders[0].clone();
        second.broker_order_id = Some(BrokerOrderId::new("ORDER-2"));
        shared_client.orders.push(second);
        assert_eq!(
            validate(shared_client).err(),
            Some(Stage5gFreshBrokerTruthError::DuplicateOrderIdentity)
        );
    }

    #[test]
    fn source_chronology_is_enforced_for_order_trade_and_position() {
        let mut invalid_order = package();
        invalid_order.orders[0].source_ts =
            Some(invalid_order.orders[0].received_ts + Duration::seconds(1));
        assert_eq!(
            validate(invalid_order).err(),
            Some(Stage5gFreshBrokerTruthError::RowSourceTimestampAfterReceipt)
        );

        let mut invalid_trade = package();
        invalid_trade.trades[0].source_ts =
            invalid_trade.trades[0].received_ts + Duration::seconds(1);
        assert_eq!(
            validate(invalid_trade).err(),
            Some(Stage5gFreshBrokerTruthError::RowSourceTimestampAfterReceipt)
        );

        let mut invalid_position = package();
        invalid_position.positions[0].source_ts =
            Some(invalid_position.positions[0].received_ts + Duration::seconds(1));
        assert_eq!(
            validate(invalid_position).err(),
            Some(Stage5gFreshBrokerTruthError::RowSourceTimestampAfterReceipt)
        );
    }

    #[test]
    fn stale_pre_restore_order_trade_and_position_rows_fail_closed() {
        let restore_at = package().captured_at - Duration::seconds(5);

        let mut stale_order = package();
        stale_order.orders[0].received_ts = restore_at - Duration::seconds(1);
        stale_order.orders[0].source_ts = Some(restore_at - Duration::seconds(2));
        assert_eq!(
            validate(stale_order).err(),
            Some(Stage5gFreshBrokerTruthError::RowReceivedBeforeCleanRestore)
        );

        let mut stale_trade = package();
        stale_trade.trades[0].received_ts = restore_at - Duration::seconds(1);
        stale_trade.trades[0].source_ts = restore_at - Duration::seconds(2);
        assert_eq!(
            validate(stale_trade).err(),
            Some(Stage5gFreshBrokerTruthError::RowReceivedBeforeCleanRestore)
        );

        let mut stale_position = package();
        stale_position.positions[0].received_ts = restore_at - Duration::seconds(1);
        stale_position.positions[0].source_ts = Some(restore_at - Duration::seconds(2));
        assert_eq!(
            validate(stale_position).err(),
            Some(Stage5gFreshBrokerTruthError::RowReceivedBeforeCleanRestore)
        );
    }

    #[test]
    fn complete_empty_section_requires_post_restore_observation() {
        let mut missing_proof = package();
        missing_proof.orders.clear();
        missing_proof.orders_observed_at = missing_proof.captured_at - Duration::seconds(5);
        assert_eq!(
            validate(missing_proof).err(),
            Some(Stage5gFreshBrokerTruthError::SectionNotObservedAfterRestore)
        );

        let mut valid_empty = package();
        valid_empty.orders.clear();
        let validated = validate(valid_empty).expect("post-restore empty section is evidence");
        assert!(validated.all_sections_complete());
    }

    #[test]
    fn every_section_observation_is_bounded_by_restore_and_capture() {
        let restore_offset = Duration::seconds(5);

        let mut stale_orders = package();
        stale_orders.orders_observed_at = stale_orders.captured_at - restore_offset;
        assert_eq!(
            validate(stale_orders).err(),
            Some(Stage5gFreshBrokerTruthError::SectionNotObservedAfterRestore)
        );
        let mut stale_trades = package();
        stale_trades.trades_observed_at = stale_trades.captured_at - restore_offset;
        assert_eq!(
            validate(stale_trades).err(),
            Some(Stage5gFreshBrokerTruthError::SectionNotObservedAfterRestore)
        );
        let mut stale_positions = package();
        stale_positions.positions_observed_at = stale_positions.captured_at - restore_offset;
        assert_eq!(
            validate(stale_positions).err(),
            Some(Stage5gFreshBrokerTruthError::SectionNotObservedAfterRestore)
        );

        let mut future_orders = package();
        future_orders.orders_observed_at = future_orders.captured_at + Duration::seconds(1);
        assert_eq!(
            validate(future_orders).err(),
            Some(Stage5gFreshBrokerTruthError::SectionObservedAfterPackageCapture)
        );
        let mut future_trades = package();
        future_trades.trades_observed_at = future_trades.captured_at + Duration::seconds(1);
        assert_eq!(
            validate(future_trades).err(),
            Some(Stage5gFreshBrokerTruthError::SectionObservedAfterPackageCapture)
        );
        let mut future_positions = package();
        future_positions.positions_observed_at =
            future_positions.captured_at + Duration::seconds(1);
        assert_eq!(
            validate(future_positions).err(),
            Some(Stage5gFreshBrokerTruthError::SectionObservedAfterPackageCapture)
        );
    }

    #[test]
    fn every_row_receipt_is_bounded_by_its_section_observation() {
        let mut late_order = package();
        late_order.orders[0].received_ts =
            late_order.orders_observed_at + Duration::milliseconds(1);
        late_order.orders[0].source_ts = Some(late_order.orders_observed_at);
        assert_eq!(
            validate(late_order).err(),
            Some(Stage5gFreshBrokerTruthError::RowTimestampAfterSectionObservation)
        );

        let mut late_trade = package();
        late_trade.trades[0].received_ts =
            late_trade.trades_observed_at + Duration::milliseconds(1);
        late_trade.trades[0].source_ts = late_trade.trades_observed_at;
        assert_eq!(
            validate(late_trade).err(),
            Some(Stage5gFreshBrokerTruthError::RowTimestampAfterSectionObservation)
        );

        let mut late_position = package();
        late_position.positions[0].received_ts =
            late_position.positions_observed_at + Duration::milliseconds(1);
        late_position.positions[0].source_ts = Some(late_position.positions_observed_at);
        assert_eq!(
            validate(late_position).err(),
            Some(Stage5gFreshBrokerTruthError::RowTimestampAfterSectionObservation)
        );
    }

    #[test]
    fn semantic_position_duplicates_and_wildcard_bridge_fail_closed() {
        let mut pair = package();
        let mut wildcard = pair.positions[0].clone();
        wildcard.instrument.venue_symbol = None;
        pair.positions.push(wildcard.clone());
        assert_eq!(
            validate(pair).err(),
            Some(Stage5gFreshBrokerTruthError::DuplicatePositionIdentity)
        );

        let mut bridge = package();
        bridge.positions[0].instrument.venue_symbol = Some("IMOEXF-A@RTSX".to_owned());
        let mut second_venue = bridge.positions[0].clone();
        second_venue.instrument.venue_symbol = Some("IMOEXF-B@RTSX".to_owned());
        bridge.positions.push(second_venue);
        bridge.positions.push(wildcard);
        assert_eq!(
            validate(bridge).err(),
            Some(Stage5gFreshBrokerTruthError::DuplicatePositionIdentity)
        );
    }

    #[test]
    fn invalid_json_identity_zero_generation_and_hash_fail_closed() {
        let mut malformed_json = serde_json::to_value(package()).expect("package JSON");
        malformed_json["operational_identity"]["broker_id"] = serde_json::json!(" finam-mock ");
        let malformed_package: Stage5gFreshBrokerTruthPackageV1 =
            serde_json::from_value(malformed_json).expect("raw DTO remains parseable");
        assert_eq!(
            validate(malformed_package).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity)
        );

        let mut zero_generation = identity_input();
        zero_generation.deployment_generation = 0;
        assert_eq!(
            Stage5gOperationalIdentityV1::validate(zero_generation).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity)
        );

        let mut malformed_hash = identity_input();
        malformed_hash.config_fingerprint_sha256 = "not-a-sha256".to_owned();
        assert_eq!(
            Stage5gOperationalIdentityV1::validate(malformed_hash).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity)
        );
    }

    #[test]
    fn canonical_identity_token_grammar_is_constructor_and_json_enforced() {
        for invalid in [
            " token",
            "token ",
            "token with-space",
            "token\twith-tab",
            "token\nwith-newline",
            "token\u{7f}with-control",
            "",
        ] {
            assert_eq!(
                Stage5gBrokerId::parse(invalid).err(),
                Some(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity),
                "invalid token survived: {invalid:?}"
            );
        }
        assert_eq!(
            Stage5gBrokerId::parse("finam-mock:gateway-1")
                .expect("visible hyphen/colon token")
                .as_str(),
            "finam-mock:gateway-1"
        );

        let mut malformed_json = serde_json::to_value(package()).expect("package JSON");
        malformed_json["operational_identity"]["strategy_instance_id"] =
            serde_json::json!("hybrid\ninstance");
        let malformed_package: Stage5gFreshBrokerTruthPackageV1 =
            serde_json::from_value(malformed_json).expect("raw DTO remains parseable");
        assert_eq!(
            validate(malformed_package).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity)
        );

        let mut invalid_account = identity_input();
        invalid_account.account_id = BrokerAccountId::new("ACC TEST");
        assert_eq!(
            Stage5gOperationalIdentityV1::validate(invalid_account).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity)
        );

        let mut invalid_instrument = identity_input();
        invalid_instrument.target_instrument.venue_symbol = Some("IMOEXF\tRTSX".to_owned());
        assert_eq!(
            Stage5gOperationalIdentityV1::validate(invalid_instrument).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOperationalIdentity)
        );

        let mut invalid_package_id = package();
        invalid_package_id.package_id = "package with-space".to_owned();
        assert_eq!(
            validate(invalid_package_id).err(),
            Some(Stage5gFreshBrokerTruthError::EmptyPackageId)
        );

        let mut invalid_epoch = package();
        invalid_epoch.snapshot_epoch = "snapshot\tepoch".to_owned();
        assert_eq!(
            validate(invalid_epoch).err(),
            Some(Stage5gFreshBrokerTruthError::EmptySnapshotEpoch)
        );
    }

    #[test]
    fn filled_incomplete_and_working_zero_remaining_fail_closed() {
        let mut incomplete_fill = package();
        incomplete_fill.orders[0].status = OrderStatus::Filled;
        incomplete_fill.orders[0].lifecycle = BrokerOrderLifecycle::Terminal;
        assert_eq!(
            validate(incomplete_fill).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOrderQuantity)
        );

        let mut working_zero = package();
        working_zero.orders[0].filled_qty = Decimal::ONE;
        working_zero.orders[0].remaining_qty = Some(Decimal::ZERO);
        assert_eq!(
            validate(working_zero).err(),
            Some(Stage5gFreshBrokerTruthError::InconsistentActiveZeroRemaining)
        );
    }

    #[test]
    fn malformed_native_id_and_missing_remaining_fail_closed() {
        let mut malformed_id = package();
        malformed_id.orders[0].broker_order_id = Some(BrokerOrderId::new(" ORDER-1 "));
        assert_eq!(
            validate(malformed_id).err(),
            Some(Stage5gFreshBrokerTruthError::MalformedNativeId)
        );

        let mut missing_remaining = package();
        missing_remaining.orders[0].remaining_qty = None;
        assert_eq!(
            validate(missing_remaining).err(),
            Some(Stage5gFreshBrokerTruthError::InvalidOrderQuantity)
        );
    }

    #[test]
    fn exact_last_replay_is_eligible_and_changed_fingerprint_conflicts() {
        let original = package();
        let first = validate(original.clone()).expect("new package");
        let last = reconciled_identity(&first);
        let exact = validate_with_lineage(original.clone(), Some(&last), &[], &[])
            .expect("exact last replay");
        assert_eq!(
            exact.lineage(),
            Stage5gFreshPackageLineage::ExactLastReconciledReplay
        );

        let mut changed = original;
        changed.orders[0].limit_price = Some(Decimal::new(2209, 0));
        assert_eq!(
            validate_with_lineage(changed, Some(&last), &[], &[]).err(),
            Some(Stage5gFreshBrokerTruthError::FreshPackageIdentityConflict)
        );
    }

    #[test]
    fn old_non_immediate_replay_requires_bounded_acceptance() {
        let original = package();
        let first = validate(original.clone()).expect("new package");
        let known = reconciled_identity(&first);
        assert_eq!(
            validate_with_lineage(original.clone(), None, &[], std::slice::from_ref(&known)).err(),
            Some(Stage5gFreshBrokerTruthError::HistoricalReplayNotAccepted)
        );

        let accepted = validate_with_lineage(original, None, std::slice::from_ref(&known), &[])
            .expect("bounded accepted replay");
        assert_eq!(
            accepted.lineage(),
            Stage5gFreshPackageLineage::ExactAcceptedHistoricalReplay
        );
    }

    #[test]
    fn canonical_fingerprint_is_independent_of_row_order() {
        let mut first = package();
        let mut second_order = first.orders[0].clone();
        second_order.broker_order_id = Some(BrokerOrderId::new("ORDER-2"));
        second_order.client_order_id = Some(ClientOrderId::new("CLIENT-2").unwrap());
        first.orders.push(second_order);

        let mut second_trade = first.trades[0].clone();
        second_trade.broker_trade_id = BrokerTradeId::new("TRADE-2");
        second_trade.broker_order_id = Some(BrokerOrderId::new("ORDER-2"));
        first.trades.push(second_trade);

        let mut reversed = first.clone();
        reversed.orders.reverse();
        reversed.trades.reverse();
        assert_eq!(
            validate(first)
                .expect("canonical first")
                .canonical_fingerprint_sha256(),
            validate(reversed)
                .expect("canonical reversed")
                .canonical_fingerprint_sha256()
        );
    }
}
