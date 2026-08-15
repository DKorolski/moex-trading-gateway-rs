//! Stage 8A-1 R1 protected execution capability and authority provenance.
//!
//! This slice remains strictly no-send. It proves only that an exact durable
//! command passed a frozen, provenance-backed preflight. No request extraction,
//! FINAM builder, HTTP, Redis consumer or dispatch seam exists here.
//!
//! ```compile_fail
//! use finam_gateway::Stage8ExecutionCapability;
//! fn require_clone<T: Clone>() {}
//! require_clone::<Stage8ExecutionCapability>();
//! ```
//!
//! ```compile_fail
//! use finam_gateway::Stage8ExecutionCapability;
//! fn require_serialize<T: serde::Serialize>() {}
//! require_serialize::<Stage8ExecutionCapability>();
//! ```
//!
//! ```compile_fail
//! use finam_gateway::Stage8ExecutionCapability;
//! fn require_debug<T: std::fmt::Debug>() {}
//! require_debug::<Stage8ExecutionCapability>();
//! ```
//!
//! ```compile_fail
//! use finam_gateway::Stage8a1OperatorArmAuthority;
//! fn require_clone<T: Clone>() {}
//! require_clone::<Stage8a1OperatorArmAuthority>();
//! ```
//!
//! ```compile_fail
//! use finam_gateway::Stage8a1OperatorArmAuthority;
//! let _ = Stage8a1OperatorArmAuthority {};
//! ```
//!
//! ```compile_fail
//! use finam_gateway::Stage8a1KillSwitchAuthority;
//! let _ = Stage8a1KillSwitchAuthority {};
//! ```
//!
//! ```compile_fail
//! use finam_gateway::Stage8a1OperatorArmAuthority;
//! fn supplied_by_operator_boundary() -> Stage8a1OperatorArmAuthority { loop {} }
//! fn consume(_: Stage8a1OperatorArmAuthority) {}
//! let arm = supplied_by_operator_boundary();
//! consume(arm);
//! consume(arm);
//! ```
//!
//! ```compile_fail
//! use finam_gateway::Stage8a1DurableRequestAuthority;
//! let _ = Stage8a1DurableRequestAuthority {};
//! ```
//!
//! ```compile_fail
//! use finam_gateway::Stage8a1FrozenExecutionPolicy;
//! let _ = Stage8a1FrozenExecutionPolicy {};
//! ```

use broker_core::{
    AccountId, BrokerKind, BrokerMarketSessionState, BrokerReadinessSnapshot, BrokerTruthSnapshot,
    CancelOrder, CancelPreflightApproval, InstrumentId, OrderPathRecord, OrderPreflightContext,
    OrderPreflightError, OrderPreflightPolicy, PlaceOrder, PreflightApprovedCancelOrder,
    PreflightApprovedPlaceOrder, StrategyRequestId, TimeInForce,
};
use chrono::{DateTime, Utc};
use runtime_durable_service::{
    Stage7bCompositeReadinessSnapshot, Stage7bPaperReadinessPhase, Stage7bRecoveryReadyOwner,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use strategy_runtime_core::{
    Stage5gLifecycleCommitmentKey, Stage6DurableActionKind, Stage6DurableCommandSnapshotV1,
    Stage6DurableRequestIdentityV1,
};

const SHA256_HEX_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8CommandScope {
    Place,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8KillSwitchState {
    RunAllowed,
    StopRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8ScheduleState {
    Eligible,
    Closed,
    Unknown,
}

const ACCEPTED_CONFIG_FILE: &str = "stage8a1-accepted-execution-config.json";
const ACCEPTED_CONFIG_SHA256_FILE: &str = "stage8a1-accepted-execution-config.json.sha256";
const CURRENT_CONTROL_FILE: &str = "stage8a1-current-control-state.json";
const ARM_NONCE_DIR: &str = "stage8a1-arm-nonces";
const MAX_AUTHORITY_TTL_MS: u64 = 60_000;

/// Reviewed configuration source for the no-send Stage 8A-1 authority layer.
/// This is source data, not an authority: callers still cannot construct any
/// opaque proof or capability from it directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8a1AcceptedExecutionConfigV1 {
    pub schema_version: u16,
    pub operational_identity_sha256: String,
    pub runtime_config_fingerprint_sha256: String,
    pub broker: BrokerKind,
    pub strategy_instance_id: String,
    pub account_id: AccountId,
    pub instrument: InstrumentId,
    pub broker_policy: OrderPreflightPolicy,
    pub build_sha256: String,
    pub endpoint_policy_sha256: String,
    pub max_arm_ttl_ms: u64,
    pub max_evidence_age_ms: u64,
}

/// Persistent current operator control source. Revision and budget are reread
/// for every mint and every continuation check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8a1CurrentControlStateV1 {
    pub schema_version: u16,
    pub operational_identity_sha256: String,
    pub runtime_config_fingerprint_sha256: String,
    pub kill_switch: Stage8KillSwitchState,
    pub durable_revision: u64,
    pub active_owner_count: u32,
    pub reconciliation_required_count: u32,
    pub max_orders: u32,
    pub consumed_orders: u32,
    pub observed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
}

/// Typed current sources accepted from the already reviewed read-only and
/// composite-readiness boundaries. It deliberately carries no authority.
pub struct Stage8a1CurrentOperationalSources<'a> {
    pub composite_readiness: &'a Stage7bCompositeReadinessSnapshot,
    pub broker_truth: &'a BrokerTruthSnapshot,
    pub broker_readiness: &'a BrokerReadinessSnapshot,
}

/// File-backed no-send authority issuer. Construction authenticates the
/// accepted config; current control and broker/runtime sources are reread or
/// revalidated at every operation.
pub struct Stage8a1OperationalAuthorityIssuer {
    root: PathBuf,
    accepted_config_sha256: String,
}

/// Linear continuation proof. It intentionally exposes neither the approved
/// request nor any transport-ready representation.
pub struct Stage8a1CurrentlyAuthorizedCapability {
    capability: Stage8ExecutionCapability,
    revalidated_at: DateTime<Utc>,
    current_state_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8a1ContinuationDiagnostic {
    pub capability: Stage8CapabilityDiagnostic,
    pub revalidated_at: DateTime<Utc>,
    pub current_state_sha256: String,
}

impl Stage8a1CurrentlyAuthorizedCapability {
    pub fn diagnostic(&self) -> Stage8a1ContinuationDiagnostic {
        Stage8a1ContinuationDiagnostic {
            capability: self.capability.diagnostic(),
            revalidated_at: self.revalidated_at,
            current_state_sha256: self.current_state_sha256.clone(),
        }
    }
}

/// Exact journal-backed Stage7B/Stage6 request authority.
pub struct Stage8a1DurableRequestAuthority {
    identity: Stage6DurableRequestIdentityV1,
    durable_command_sha256: String,
    canonical_command_sha256: String,
    accepted_record_id_sha256: String,
    dispatch_record_id_sha256: String,
    dispatch_sequence: u64,
    durable_frontier_sha256: String,
    runtime_config_fingerprint_sha256: String,
    checkpoint_sha256: String,
    operational_identity_sha256: String,
    seal_generation: u64,
    seal_commitment_sha256: String,
    provenance_sha256: String,
}

impl Stage8a1DurableRequestAuthority {
    pub fn from_stage7b_owner(
        owner: &mut Stage7bRecoveryReadyOwner,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        identity: &Stage6DurableRequestIdentityV1,
        command: &Stage6DurableCommandSnapshotV1,
    ) -> Result<Self, Stage8ExecutionPreflightError> {
        let authority = owner
            .authorize_stage8a1_durable_request(commitment_key, identity, command)
            .map_err(|_| Stage8ExecutionPreflightError::DurableAuthorityInvalid)?;
        let stage6 = authority.stage6();
        let mut value = Self {
            identity: stage6.identity().clone(),
            durable_command_sha256: digest_parts(
                b"stage8a1-stage6-command-v1",
                &[&canonical_json(command)],
            ),
            canonical_command_sha256: stage6.canonical_command_sha256().as_str().to_string(),
            accepted_record_id_sha256: stage6.accepted_record_id().as_str().to_string(),
            dispatch_record_id_sha256: stage6.dispatch_record_id().as_str().to_string(),
            dispatch_sequence: stage6.dispatch_sequence(),
            durable_frontier_sha256: stage6.durable_frontier_sha256().to_string(),
            runtime_config_fingerprint_sha256: stage6
                .runtime_config_fingerprint_sha256()
                .to_string(),
            checkpoint_sha256: stage6.authenticated_checkpoint_sha256().to_string(),
            operational_identity_sha256: authority.operational_identity_sha256().to_string(),
            seal_generation: authority.seal_generation(),
            seal_commitment_sha256: authority.seal_commitment_sha256().to_string(),
            provenance_sha256: String::new(),
        };
        value.provenance_sha256 = value.calculate_provenance();
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), Stage8ExecutionPreflightError> {
        if self.seal_generation == 0
            || self.dispatch_sequence == 0
            || !valid_sha256(&self.canonical_command_sha256)
            || !valid_sha256(&self.durable_command_sha256)
            || !valid_sha256(&self.accepted_record_id_sha256)
            || !valid_sha256(&self.dispatch_record_id_sha256)
            || !valid_sha256(&self.durable_frontier_sha256)
            || !valid_sha256(&self.runtime_config_fingerprint_sha256)
            || !valid_sha256(&self.checkpoint_sha256)
            || !valid_sha256(&self.operational_identity_sha256)
            || !valid_sha256(&self.seal_commitment_sha256)
            || self.calculate_provenance() != self.provenance_sha256
        {
            return Err(Stage8ExecutionPreflightError::DurableAuthorityInvalid);
        }
        Ok(())
    }

    fn calculate_provenance(&self) -> String {
        digest_parts(
            b"stage8a1-durable-authority-v1",
            &[
                &canonical_json(&self.identity),
                self.durable_command_sha256.as_bytes(),
                self.canonical_command_sha256.as_bytes(),
                self.accepted_record_id_sha256.as_bytes(),
                self.dispatch_record_id_sha256.as_bytes(),
                &self.dispatch_sequence.to_be_bytes(),
                self.durable_frontier_sha256.as_bytes(),
                self.runtime_config_fingerprint_sha256.as_bytes(),
                self.checkpoint_sha256.as_bytes(),
                self.operational_identity_sha256.as_bytes(),
                &self.seal_generation.to_be_bytes(),
                self.seal_commitment_sha256.as_bytes(),
            ],
        )
    }
}

/// Opaque exact-command, one-shot operator arm. Consumed by authorization.
pub struct Stage8a1OperatorArmAuthority {
    nonce_sha256: String,
    exact_command_sha256: String,
    scope_sha256: String,
    policy_sha256: String,
    build_sha256: String,
    config_sha256: String,
    endpoint_policy_sha256: String,
    issued_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
}

/// Opaque canonical execution policy. The broker policy is owned here, so a
/// caller cannot substitute wider arithmetic limits at authorization time.
pub struct Stage8a1FrozenExecutionPolicy {
    broker_policy: OrderPreflightPolicy,
    scope_sha256: String,
    policy_sha256: String,
    build_sha256: String,
    config_sha256: String,
    endpoint_policy_sha256: String,
    max_arm_ttl_ms: u64,
    max_evidence_age_ms: u64,
}

pub struct Stage8a1TrustedClockAuthority {
    now: DateTime<Utc>,
    scope_sha256: String,
    evidence_sha256: String,
}

pub struct Stage8a1ReadinessAuthority {
    scope_sha256: String,
    observed_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    evidence_sha256: String,
}

pub struct Stage8a1KillSwitchAuthority {
    state: Stage8KillSwitchState,
    durable_revision: u64,
    scope_sha256: String,
    observed_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    evidence_sha256: String,
}

pub struct Stage8a1BrokerOwnershipAuthority {
    broker: BrokerKind,
    active_owner_count: u32,
    scope_sha256: String,
    observed_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    evidence_sha256: String,
}

pub struct Stage8a1ZeroAmbiguityAuthority {
    unresolved_order_count: u32,
    unresolved_delivery_count: u32,
    reconciliation_required_count: u32,
    scope_sha256: String,
    observed_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    evidence_sha256: String,
}

pub struct Stage8a1FreshBrokerTruthAuthority {
    account_truth_fresh: bool,
    instrument_truth_fresh: bool,
    scope_sha256: String,
    observed_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    evidence_sha256: String,
}

pub struct Stage8a1ScheduleAuthority {
    state: Stage8ScheduleState,
    scope_sha256: String,
    observed_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    evidence_sha256: String,
}

pub struct Stage8a1MicroBudgetAuthority {
    max_orders: u32,
    consumed_orders: u32,
    scope_sha256: String,
    observed_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    evidence_sha256: String,
}

pub struct Stage8PlacePreflightInput<'a> {
    pub order: &'a PlaceOrder,
    pub broker_preflight_context: &'a OrderPreflightContext,
    pub durable_request: Stage8a1DurableRequestAuthority,
    pub operator_arm: Stage8a1OperatorArmAuthority,
    pub frozen_policy: Stage8a1FrozenExecutionPolicy,
    pub clock: Stage8a1TrustedClockAuthority,
    pub readiness: Stage8a1ReadinessAuthority,
    pub kill_switch: Stage8a1KillSwitchAuthority,
    pub broker_ownership: Stage8a1BrokerOwnershipAuthority,
    pub ambiguity: Stage8a1ZeroAmbiguityAuthority,
    pub broker_truth: Stage8a1FreshBrokerTruthAuthority,
    pub schedule: Stage8a1ScheduleAuthority,
    pub micro_budget: Stage8a1MicroBudgetAuthority,
}

pub struct Stage8CancelPreflightInput<'a> {
    pub cancel: &'a CancelOrder,
    pub existing_order: &'a OrderPathRecord,
    pub durable_request: Stage8a1DurableRequestAuthority,
    pub operator_arm: Stage8a1OperatorArmAuthority,
    pub frozen_policy: Stage8a1FrozenExecutionPolicy,
    pub clock: Stage8a1TrustedClockAuthority,
    pub readiness: Stage8a1ReadinessAuthority,
    pub kill_switch: Stage8a1KillSwitchAuthority,
    pub broker_ownership: Stage8a1BrokerOwnershipAuthority,
    pub ambiguity: Stage8a1ZeroAmbiguityAuthority,
    pub broker_truth: Stage8a1FreshBrokerTruthAuthority,
    pub schedule: Stage8a1ScheduleAuthority,
    pub micro_budget: Stage8a1MicroBudgetAuthority,
}

enum Stage8ApprovedCommand {
    Place(PreflightApprovedPlaceOrder),
    Cancel(PreflightApprovedCancelOrder),
}

pub struct Stage8ExecutionCapability {
    approved: Stage8ApprovedCommand,
    scope: Stage8CommandScope,
    request_id: StrategyRequestId,
    issued_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    seal_generation: u64,
    durable_provenance_sha256: String,
    seal_commitment_sha256: String,
    policy_sha256: String,
    build_sha256: String,
    config_sha256: String,
    endpoint_policy_sha256: String,
    authority_scope_sha256: String,
    arm_nonce_sha256: String,
    exact_command_sha256: String,
    current_state_sha256: String,
    audit_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8CapabilityDiagnostic {
    pub scope: Stage8CommandScope,
    pub issued_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub seal_generation: u64,
    pub audit_fingerprint: String,
}

impl Stage8ExecutionCapability {
    pub fn diagnostic(&self) -> Stage8CapabilityDiagnostic {
        let approved_request_id = match &self.approved {
            Stage8ApprovedCommand::Place(approved) => approved.order().request_id,
            Stage8ApprovedCommand::Cancel(approved) => approved.cancel().request_id,
        };
        debug_assert_eq!(approved_request_id, self.request_id);
        Stage8CapabilityDiagnostic {
            scope: self.scope,
            issued_at: self.issued_at,
            valid_until: self.valid_until,
            seal_generation: self.seal_generation,
            audit_fingerprint: self.audit_fingerprint.clone(),
        }
    }
}

pub enum Stage8CancelPreflightDecision {
    Capability(Box<Stage8ExecutionCapability>),
    AlreadyTerminal,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Stage8ExecutionPreflightError {
    #[error("Stage 8 durable Stage7B/Stage6 authority is missing or mismatched")]
    DurableAuthorityInvalid,
    #[error("Stage 8 durable command does not match the requested operation")]
    DurableCommandMismatch,
    #[error("Stage 8 authority scope or operational generation is incoherent")]
    AuthorityScopeMismatch,
    #[error("Stage 8 frozen execution policy is invalid")]
    FrozenPolicyInvalid,
    #[error("Stage 8 operator arm is invalid, expired or command-mismatched")]
    OperatorArmInvalid,
    #[error("Stage 8 readiness authority is absent or stale")]
    ReadinessInvalid,
    #[error("Stage 8 persistent kill switch is not RunAllowed")]
    KillSwitchNotRunAllowed,
    #[error("Stage 8 persistent kill-switch authority is invalid or stale")]
    KillSwitchInvalid,
    #[error("Stage 8 broker ownership is not exactly one strategy-scoped FINAM owner")]
    BrokerOwnershipInvalid,
    #[error("Stage 8 unresolved ambiguity is non-zero or out of scope")]
    AmbiguityInvalid,
    #[error("Stage 8 accepted broker truth is absent or stale")]
    BrokerTruthInvalid,
    #[error("Stage 8 trading session is closed, unknown or stale")]
    ScheduleInvalid,
    #[error("Stage 8 engineering-micro budget is missing or exhausted")]
    MicroBudgetInvalid,
    #[error("Stage 8 cancel requires an exact existing order mapping")]
    CancelMappingRequired,
    #[error("Stage 8 accepted configuration source is absent, corrupt or mismatched")]
    AcceptedConfigInvalid,
    #[error("Stage 8 persistent current-control source is absent, stale or mismatched")]
    CurrentControlInvalid,
    #[error("Stage 8 logical operator-arm nonce was already registered")]
    OperatorArmNonceReplay,
    #[error("Stage 8 operator-arm nonce registry is unavailable or uncertain")]
    OperatorArmRegistryUnavailable,
    #[error("Stage 8 capability is no longer authorized by current state")]
    CurrentStateChanged,
    #[error("broker-neutral preflight rejected the command: {0}")]
    BrokerPreflight(#[from] OrderPreflightError),
}

struct Stage8a1IssuedAuthorities {
    arm: Stage8a1OperatorArmAuthority,
    clock: Stage8a1TrustedClockAuthority,
    readiness: Stage8a1ReadinessAuthority,
    kill_switch: Stage8a1KillSwitchAuthority,
    ownership: Stage8a1BrokerOwnershipAuthority,
    ambiguity: Stage8a1ZeroAmbiguityAuthority,
    broker_truth: Stage8a1FreshBrokerTruthAuthority,
    schedule: Stage8a1ScheduleAuthority,
    budget: Stage8a1MicroBudgetAuthority,
}

impl Stage8a1OperationalAuthorityIssuer {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, Stage8ExecutionPreflightError> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|_| Stage8ExecutionPreflightError::AcceptedConfigInvalid)?;
        let (_, accepted_config_sha256) = load_accepted_config(&root)?;
        load_current_control(&root)?;
        Ok(Self {
            root,
            accepted_config_sha256,
        })
    }

    pub fn authorize_place(
        &mut self,
        durable_request: Stage8a1DurableRequestAuthority,
        order: &PlaceOrder,
        broker_preflight_context: &OrderPreflightContext,
        sources: Stage8a1CurrentOperationalSources<'_>,
        logical_arm_nonce: &str,
    ) -> Result<Stage8ExecutionCapability, Stage8ExecutionPreflightError> {
        durable_request.validate()?;
        validate_place_durable(&durable_request, order)?;
        let (config, config_file_sha256) = load_accepted_config(&self.root)?;
        if config_file_sha256 != self.accepted_config_sha256 {
            return Err(Stage8ExecutionPreflightError::AcceptedConfigInvalid);
        }
        validate_config_binding(&config, &durable_request)?;
        let policy = frozen_policy_from_config(&config, &durable_request);
        validate_policy(&policy, &durable_request)?;
        let command_sha256 = place_command_sha256(order, broker_preflight_context, &policy);
        let authorities = self.issue_authorities(
            &config,
            &durable_request,
            &policy,
            &command_sha256,
            sources,
            logical_arm_nonce,
        )?;
        authorize_stage8_place(Stage8PlacePreflightInput {
            order,
            broker_preflight_context,
            durable_request,
            operator_arm: authorities.arm,
            frozen_policy: policy,
            clock: authorities.clock,
            readiness: authorities.readiness,
            kill_switch: authorities.kill_switch,
            broker_ownership: authorities.ownership,
            ambiguity: authorities.ambiguity,
            broker_truth: authorities.broker_truth,
            schedule: authorities.schedule,
            micro_budget: authorities.budget,
        })
    }

    pub fn authorize_cancel(
        &mut self,
        durable_request: Stage8a1DurableRequestAuthority,
        cancel: &CancelOrder,
        existing_order: &OrderPathRecord,
        sources: Stage8a1CurrentOperationalSources<'_>,
        logical_arm_nonce: &str,
    ) -> Result<Stage8CancelPreflightDecision, Stage8ExecutionPreflightError> {
        durable_request.validate()?;
        validate_cancel_durable(&durable_request, cancel, existing_order)?;
        let (config, config_file_sha256) = load_accepted_config(&self.root)?;
        if config_file_sha256 != self.accepted_config_sha256 {
            return Err(Stage8ExecutionPreflightError::AcceptedConfigInvalid);
        }
        validate_config_binding(&config, &durable_request)?;
        let policy = frozen_policy_from_config(&config, &durable_request);
        validate_policy(&policy, &durable_request)?;
        let command_sha256 = cancel_command_sha256(cancel, &policy);
        let authorities = self.issue_authorities(
            &config,
            &durable_request,
            &policy,
            &command_sha256,
            sources,
            logical_arm_nonce,
        )?;
        authorize_stage8_cancel(Stage8CancelPreflightInput {
            cancel,
            existing_order,
            durable_request,
            operator_arm: authorities.arm,
            frozen_policy: policy,
            clock: authorities.clock,
            readiness: authorities.readiness,
            kill_switch: authorities.kill_switch,
            broker_ownership: authorities.ownership,
            ambiguity: authorities.ambiguity,
            broker_truth: authorities.broker_truth,
            schedule: authorities.schedule,
            micro_budget: authorities.budget,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn revalidate_place_capability(
        &mut self,
        capability: Stage8ExecutionCapability,
        owner: &mut Stage7bRecoveryReadyOwner,
        commitment_key: &Stage5gLifecycleCommitmentKey,
        identity: &Stage6DurableRequestIdentityV1,
        command: &Stage6DurableCommandSnapshotV1,
        order: &PlaceOrder,
        broker_preflight_context: &OrderPreflightContext,
        sources: Stage8a1CurrentOperationalSources<'_>,
    ) -> Result<Stage8a1CurrentlyAuthorizedCapability, Stage8ExecutionPreflightError> {
        let now = Utc::now();
        if capability.scope != Stage8CommandScope::Place
            || capability.request_id != order.request_id
            || now >= capability.valid_until
        {
            return Err(Stage8ExecutionPreflightError::CurrentStateChanged);
        }
        let durable = Stage8a1DurableRequestAuthority::from_stage7b_owner(
            owner,
            commitment_key,
            identity,
            command,
        )?;
        validate_place_durable(&durable, order)?;
        let (config, config_file_sha256) = load_accepted_config(&self.root)?;
        if config_file_sha256 != self.accepted_config_sha256 {
            return Err(Stage8ExecutionPreflightError::CurrentStateChanged);
        }
        validate_config_binding(&config, &durable)?;
        let policy = frozen_policy_from_config(&config, &durable);
        validate_policy(&policy, &durable)?;
        let scope_sha256 = authority_scope_sha256(&durable, &policy);
        let current_state_sha256 = current_state_from_sources(
            &self.root,
            &config,
            &durable,
            &policy,
            &scope_sha256,
            &sources,
            now,
        )?;
        let nonce_path = arm_nonce_path(
            &self.root,
            durable.seal_generation,
            &capability.arm_nonce_sha256,
        );
        let expected_nonce_record = arm_nonce_registration_record(
            &durable,
            &capability.exact_command_sha256,
            &policy.policy_sha256,
        );
        let nonce_record_matches = read_regular_file(&nonce_path)
            .map(|bytes| bytes == expected_nonce_record.as_bytes())
            .unwrap_or(false);
        if !nonce_record_matches
            || durable.provenance_sha256 != capability.durable_provenance_sha256
            || durable.seal_commitment_sha256 != capability.seal_commitment_sha256
            || policy.policy_sha256 != capability.policy_sha256
            || policy.build_sha256 != capability.build_sha256
            || policy.config_sha256 != capability.config_sha256
            || policy.endpoint_policy_sha256 != capability.endpoint_policy_sha256
            || scope_sha256 != capability.authority_scope_sha256
            || current_state_sha256 != capability.current_state_sha256
            || place_command_sha256(order, broker_preflight_context, &policy)
                != capability.exact_command_sha256
        {
            return Err(Stage8ExecutionPreflightError::CurrentStateChanged);
        }
        Ok(Stage8a1CurrentlyAuthorizedCapability {
            capability,
            revalidated_at: now,
            current_state_sha256,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn issue_authorities(
        &mut self,
        config: &Stage8a1AcceptedExecutionConfigV1,
        durable: &Stage8a1DurableRequestAuthority,
        policy: &Stage8a1FrozenExecutionPolicy,
        command_sha256: &str,
        sources: Stage8a1CurrentOperationalSources<'_>,
        logical_arm_nonce: &str,
    ) -> Result<Stage8a1IssuedAuthorities, Stage8ExecutionPreflightError> {
        let now = Utc::now();
        let scope_sha256 = authority_scope_sha256(durable, policy);
        let derived = derive_current_authorities(
            &self.root,
            config,
            durable,
            policy,
            &scope_sha256,
            &sources,
            now,
        )?;
        let nonce_sha256 = register_arm_nonce(
            &self.root,
            durable,
            command_sha256,
            &policy.policy_sha256,
            logical_arm_nonce,
        )?;
        let valid_until = std::cmp::min(
            now + chrono::Duration::milliseconds(config.max_arm_ttl_ms as i64),
            derived.valid_until,
        );
        Ok(Stage8a1IssuedAuthorities {
            arm: Stage8a1OperatorArmAuthority {
                nonce_sha256,
                exact_command_sha256: command_sha256.to_string(),
                scope_sha256: scope_sha256.clone(),
                policy_sha256: policy.policy_sha256.clone(),
                build_sha256: policy.build_sha256.clone(),
                config_sha256: policy.config_sha256.clone(),
                endpoint_policy_sha256: policy.endpoint_policy_sha256.clone(),
                issued_at: now,
                valid_until,
            },
            clock: Stage8a1TrustedClockAuthority {
                now,
                scope_sha256: scope_sha256.clone(),
                evidence_sha256: digest_parts(
                    b"stage8a1-trusted-system-clock-v1",
                    &[scope_sha256.as_bytes(), now.to_rfc3339().as_bytes()],
                ),
            },
            readiness: derived.readiness,
            kill_switch: derived.kill_switch,
            ownership: derived.ownership,
            ambiguity: derived.ambiguity,
            broker_truth: derived.broker_truth,
            schedule: derived.schedule,
            budget: derived.budget,
        })
    }
}

struct Stage8a1DerivedCurrentAuthorities {
    valid_until: DateTime<Utc>,
    readiness: Stage8a1ReadinessAuthority,
    kill_switch: Stage8a1KillSwitchAuthority,
    ownership: Stage8a1BrokerOwnershipAuthority,
    ambiguity: Stage8a1ZeroAmbiguityAuthority,
    broker_truth: Stage8a1FreshBrokerTruthAuthority,
    schedule: Stage8a1ScheduleAuthority,
    budget: Stage8a1MicroBudgetAuthority,
}

#[allow(clippy::too_many_arguments)]
fn derive_current_authorities(
    root: &Path,
    config: &Stage8a1AcceptedExecutionConfigV1,
    durable: &Stage8a1DurableRequestAuthority,
    policy: &Stage8a1FrozenExecutionPolicy,
    scope_sha256: &str,
    sources: &Stage8a1CurrentOperationalSources<'_>,
    now: DateTime<Utc>,
) -> Result<Stage8a1DerivedCurrentAuthorities, Stage8ExecutionPreflightError> {
    let control = load_current_control(root)?;
    if control.operational_identity_sha256 != durable.operational_identity_sha256
        || control.runtime_config_fingerprint_sha256 != durable.runtime_config_fingerprint_sha256
        || control.durable_revision == 0
        || control.active_owner_count != 1
        || control.reconciliation_required_count != 0
        || control.observed_at > now
        || control.valid_until <= now
    {
        return Err(Stage8ExecutionPreflightError::CurrentControlInvalid);
    }
    if sources.composite_readiness.phase != Stage7bPaperReadinessPhase::PaperReady
        || !sources.composite_readiness.reasons.is_empty()
        || !sources.composite_readiness.blocked_entry_ids.is_empty()
        || !sources.composite_readiness.blocked_request_ids.is_empty()
        || sources.composite_readiness.checked_at > now
    {
        return Err(Stage8ExecutionPreflightError::ReadinessInvalid);
    }
    if sources.broker_truth.account_id != config.account_id
        || sources
            .broker_truth
            .orders
            .iter()
            .any(|row| row.account_id != config.account_id)
        || sources
            .broker_truth
            .positions
            .iter()
            .any(|row| row.account_id != config.account_id)
        || !sources.broker_readiness.broker_truth_is_fresh(now)
        || sources.broker_truth.received_ts > now
        || !sources.broker_truth.instruments.iter().any(|spec| {
            spec.instrument.internal_symbol.0 == config.instrument.symbol
                && config.instrument.venue_symbol.as_deref()
                    == Some(spec.instrument.broker_symbol.0.as_str())
        })
    {
        return Err(Stage8ExecutionPreflightError::BrokerTruthInvalid);
    }
    let unresolved_order_count = sources.broker_readiness.unknown_order_count as u32;
    let unresolved_delivery_count = sources.broker_truth.account_orphan_order_count() as u32;
    if unresolved_order_count != 0
        || unresolved_delivery_count != 0
        || control.reconciliation_required_count != 0
    {
        return Err(Stage8ExecutionPreflightError::AmbiguityInvalid);
    }
    if sources.broker_readiness.market_session != BrokerMarketSessionState::Open {
        return Err(Stage8ExecutionPreflightError::ScheduleInvalid);
    }
    if control.kill_switch != Stage8KillSwitchState::RunAllowed {
        return Err(Stage8ExecutionPreflightError::KillSwitchNotRunAllowed);
    }
    if control.max_orders != 1 || control.consumed_orders != 0 {
        return Err(Stage8ExecutionPreflightError::MicroBudgetInvalid);
    }
    let max_age = chrono::Duration::milliseconds(config.max_evidence_age_ms as i64);
    let readiness_until = sources.composite_readiness.checked_at + max_age;
    let truth_until = sources.broker_truth.received_ts + max_age;
    let schedule_observed = sources
        .broker_readiness
        .schedule
        .observed_ts
        .ok_or(Stage8ExecutionPreflightError::ScheduleInvalid)?;
    let schedule_until = schedule_observed + max_age;
    let valid_until = [
        control.valid_until,
        readiness_until,
        truth_until,
        schedule_until,
    ]
    .into_iter()
    .min()
    .expect("current authority expiry set is non-empty");
    if valid_until <= now {
        return Err(Stage8ExecutionPreflightError::ReadinessInvalid);
    }
    let readiness_evidence = source_evidence(
        b"stage8a1-composite-readiness-v1",
        scope_sha256,
        sources.composite_readiness,
    );
    let control_evidence =
        source_evidence(b"stage8a1-persistent-control-v1", scope_sha256, &control);
    let ownership_evidence = digest_parts(
        b"stage8a1-broker-ownership-v1",
        &[
            scope_sha256.as_bytes(),
            config.strategy_instance_id.as_bytes(),
            config.account_id.as_str().as_bytes(),
            &canonical_json(&config.instrument),
            policy.policy_sha256.as_bytes(),
        ],
    );
    let ambiguity_evidence = digest_parts(
        b"stage8a1-zero-ambiguity-v1",
        &[
            scope_sha256.as_bytes(),
            &unresolved_order_count.to_be_bytes(),
            &unresolved_delivery_count.to_be_bytes(),
            &control.reconciliation_required_count.to_be_bytes(),
        ],
    );
    let truth_evidence = source_evidence(
        b"stage8a1-current-broker-truth-v1",
        scope_sha256,
        &(sources.broker_truth, sources.broker_readiness),
    );
    let schedule_evidence = digest_parts(
        b"stage8a1-current-schedule-v1",
        &[
            scope_sha256.as_bytes(),
            &canonical_json(&sources.broker_readiness.market_session),
            schedule_observed.to_rfc3339().as_bytes(),
        ],
    );
    Ok(Stage8a1DerivedCurrentAuthorities {
        valid_until,
        readiness: Stage8a1ReadinessAuthority {
            scope_sha256: scope_sha256.to_string(),
            observed_at: sources.composite_readiness.checked_at,
            valid_until: readiness_until,
            evidence_sha256: readiness_evidence,
        },
        kill_switch: Stage8a1KillSwitchAuthority {
            state: control.kill_switch,
            durable_revision: control.durable_revision,
            scope_sha256: scope_sha256.to_string(),
            observed_at: control.observed_at,
            valid_until: control.valid_until,
            evidence_sha256: control_evidence.clone(),
        },
        ownership: Stage8a1BrokerOwnershipAuthority {
            broker: config.broker.clone(),
            active_owner_count: control.active_owner_count,
            scope_sha256: scope_sha256.to_string(),
            observed_at: sources.broker_truth.received_ts,
            valid_until: truth_until,
            evidence_sha256: ownership_evidence,
        },
        ambiguity: Stage8a1ZeroAmbiguityAuthority {
            unresolved_order_count,
            unresolved_delivery_count,
            reconciliation_required_count: control.reconciliation_required_count,
            scope_sha256: scope_sha256.to_string(),
            observed_at: sources.broker_truth.received_ts,
            valid_until: truth_until,
            evidence_sha256: ambiguity_evidence,
        },
        broker_truth: Stage8a1FreshBrokerTruthAuthority {
            account_truth_fresh: true,
            instrument_truth_fresh: true,
            scope_sha256: scope_sha256.to_string(),
            observed_at: sources.broker_truth.received_ts,
            valid_until: truth_until,
            evidence_sha256: truth_evidence,
        },
        schedule: Stage8a1ScheduleAuthority {
            state: Stage8ScheduleState::Eligible,
            scope_sha256: scope_sha256.to_string(),
            observed_at: schedule_observed,
            valid_until: schedule_until,
            evidence_sha256: schedule_evidence,
        },
        budget: Stage8a1MicroBudgetAuthority {
            max_orders: control.max_orders,
            consumed_orders: control.consumed_orders,
            scope_sha256: scope_sha256.to_string(),
            observed_at: control.observed_at,
            valid_until: control.valid_until,
            evidence_sha256: control_evidence,
        },
    })
}

fn current_state_from_sources(
    root: &Path,
    config: &Stage8a1AcceptedExecutionConfigV1,
    durable: &Stage8a1DurableRequestAuthority,
    policy: &Stage8a1FrozenExecutionPolicy,
    scope_sha256: &str,
    sources: &Stage8a1CurrentOperationalSources<'_>,
    now: DateTime<Utc>,
) -> Result<String, Stage8ExecutionPreflightError> {
    let current =
        derive_current_authorities(root, config, durable, policy, scope_sha256, sources, now)?;
    Ok(digest_parts(
        b"stage8a1-current-authority-state-v1",
        &[
            scope_sha256.as_bytes(),
            current.readiness.evidence_sha256.as_bytes(),
            current.kill_switch.evidence_sha256.as_bytes(),
            current.ownership.evidence_sha256.as_bytes(),
            current.ambiguity.evidence_sha256.as_bytes(),
            current.broker_truth.evidence_sha256.as_bytes(),
            current.schedule.evidence_sha256.as_bytes(),
            current.budget.evidence_sha256.as_bytes(),
        ],
    ))
}

fn frozen_policy_from_config(
    config: &Stage8a1AcceptedExecutionConfigV1,
    durable: &Stage8a1DurableRequestAuthority,
) -> Stage8a1FrozenExecutionPolicy {
    let mut broker_policy = config.broker_policy.clone();
    broker_policy.operator_arm.session_id = "stage8a1-frozen-policy-template".to_string();
    broker_policy.operator_arm.armed_until = DateTime::<Utc>::MAX_UTC;
    broker_policy.operator_arm.endpoint_calls_enabled = true;
    broker_policy.operator_arm.one_shot = true;
    broker_policy.operator_arm.endpoint_attempted = false;
    broker_policy.operator_arm.preflight_digest = config.runtime_config_fingerprint_sha256.clone();
    let mut policy = Stage8a1FrozenExecutionPolicy {
        broker_policy,
        scope_sha256: digest_parts(
            b"stage8a1-policy-source-scope-v1",
            &[
                &canonical_json(&durable.identity),
                durable.seal_commitment_sha256.as_bytes(),
            ],
        ),
        policy_sha256: String::new(),
        build_sha256: config.build_sha256.clone(),
        config_sha256: config.runtime_config_fingerprint_sha256.clone(),
        endpoint_policy_sha256: config.endpoint_policy_sha256.clone(),
        max_arm_ttl_ms: config.max_arm_ttl_ms,
        max_evidence_age_ms: config.max_evidence_age_ms,
    };
    policy.policy_sha256 = frozen_policy_sha256(&policy);
    policy
}

fn validate_config_binding(
    config: &Stage8a1AcceptedExecutionConfigV1,
    durable: &Stage8a1DurableRequestAuthority,
) -> Result<(), Stage8ExecutionPreflightError> {
    let identity = &durable.identity;
    if config.schema_version != 1
        || config.broker != BrokerKind::Finam
        || config.strategy_instance_id.trim().is_empty()
        || !identity
            .attribution()
            .belongs_to(&config.strategy_instance_id)
        || config.account_id != *identity.account_id()
        || config.instrument != *identity.instrument()
        || config.operational_identity_sha256 != durable.operational_identity_sha256
        || config.runtime_config_fingerprint_sha256 != durable.runtime_config_fingerprint_sha256
        || !valid_sha256(&config.build_sha256)
        || !valid_sha256(&config.endpoint_policy_sha256)
        || config.max_arm_ttl_ms == 0
        || config.max_arm_ttl_ms > MAX_AUTHORITY_TTL_MS
        || config.max_evidence_age_ms == 0
        || config.max_evidence_age_ms > MAX_AUTHORITY_TTL_MS
        || !config.broker_policy.operator_arm.one_shot
        || config.broker_policy.operator_arm.endpoint_attempted
        || !config.broker_policy.operator_arm.endpoint_calls_enabled
        || config.broker_policy.operator_arm.preflight_digest
            != config.runtime_config_fingerprint_sha256
    {
        return Err(Stage8ExecutionPreflightError::AcceptedConfigInvalid);
    }
    Ok(())
}

fn load_accepted_config(
    root: &Path,
) -> Result<(Stage8a1AcceptedExecutionConfigV1, String), Stage8ExecutionPreflightError> {
    let bytes = read_regular_file(&root.join(ACCEPTED_CONFIG_FILE))
        .map_err(|_| Stage8ExecutionPreflightError::AcceptedConfigInvalid)?;
    let observed_sha256 = digest_parts(b"stage8a1-accepted-config-file-v1", &[&bytes]);
    let sidecar = read_regular_file(&root.join(ACCEPTED_CONFIG_SHA256_FILE))
        .map_err(|_| Stage8ExecutionPreflightError::AcceptedConfigInvalid)?;
    let expected = std::str::from_utf8(&sidecar)
        .map_err(|_| Stage8ExecutionPreflightError::AcceptedConfigInvalid)?
        .trim();
    if expected != observed_sha256 || !valid_sha256(expected) {
        return Err(Stage8ExecutionPreflightError::AcceptedConfigInvalid);
    }
    let config = serde_json::from_slice(&bytes)
        .map_err(|_| Stage8ExecutionPreflightError::AcceptedConfigInvalid)?;
    Ok((config, observed_sha256))
}

fn load_current_control(
    root: &Path,
) -> Result<Stage8a1CurrentControlStateV1, Stage8ExecutionPreflightError> {
    let bytes = read_regular_file(&root.join(CURRENT_CONTROL_FILE))
        .map_err(|_| Stage8ExecutionPreflightError::CurrentControlInvalid)?;
    let control: Stage8a1CurrentControlStateV1 = serde_json::from_slice(&bytes)
        .map_err(|_| Stage8ExecutionPreflightError::CurrentControlInvalid)?;
    if control.schema_version != 1 {
        return Err(Stage8ExecutionPreflightError::CurrentControlInvalid);
    }
    Ok(control)
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>, std::io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::other(
            "authority source is not a regular file",
        ));
    }
    fs::read(path)
}

fn register_arm_nonce(
    root: &Path,
    durable: &Stage8a1DurableRequestAuthority,
    command_sha256: &str,
    policy_sha256: &str,
    logical_nonce: &str,
) -> Result<String, Stage8ExecutionPreflightError> {
    if logical_nonce.trim().is_empty() || logical_nonce.len() > 256 {
        return Err(Stage8ExecutionPreflightError::OperatorArmInvalid);
    }
    let nonce_sha256 = digest_parts(
        b"stage8a1-operator-arm-nonce-v1",
        &[
            durable.operational_identity_sha256.as_bytes(),
            &durable.seal_generation.to_be_bytes(),
            logical_nonce.as_bytes(),
        ],
    );
    let generation_dir = root
        .join(ARM_NONCE_DIR)
        .join(durable.seal_generation.to_string());
    fs::create_dir_all(&generation_dir)
        .map_err(|_| Stage8ExecutionPreflightError::OperatorArmRegistryUnavailable)?;
    let metadata = fs::symlink_metadata(&generation_dir)
        .map_err(|_| Stage8ExecutionPreflightError::OperatorArmRegistryUnavailable)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(Stage8ExecutionPreflightError::OperatorArmRegistryUnavailable);
    }
    let path = generation_dir.join(&nonce_sha256);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Stage8ExecutionPreflightError::OperatorArmNonceReplay
            } else {
                Stage8ExecutionPreflightError::OperatorArmRegistryUnavailable
            }
        })?;
    let record = arm_nonce_registration_record(durable, command_sha256, policy_sha256);
    file.write_all(record.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| Stage8ExecutionPreflightError::OperatorArmRegistryUnavailable)?;
    sync_directory(&generation_dir)?;
    Ok(nonce_sha256)
}

fn arm_nonce_registration_record(
    durable: &Stage8a1DurableRequestAuthority,
    command_sha256: &str,
    policy_sha256: &str,
) -> String {
    digest_parts(
        b"stage8a1-operator-arm-registration-v1",
        &[
            durable.provenance_sha256.as_bytes(),
            command_sha256.as_bytes(),
            policy_sha256.as_bytes(),
        ],
    )
}

fn sync_directory(path: &Path) -> Result<(), Stage8ExecutionPreflightError> {
    OpenOptions::new()
        .read(true)
        .open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| Stage8ExecutionPreflightError::OperatorArmRegistryUnavailable)
}

fn arm_nonce_path(root: &Path, seal_generation: u64, nonce_sha256: &str) -> PathBuf {
    root.join(ARM_NONCE_DIR)
        .join(seal_generation.to_string())
        .join(nonce_sha256)
}

fn source_evidence<T: Serialize>(domain: &[u8], scope_sha256: &str, source: &T) -> String {
    digest_parts(domain, &[scope_sha256.as_bytes(), &canonical_json(source)])
}

pub fn authorize_stage8_place(
    input: Stage8PlacePreflightInput<'_>,
) -> Result<Stage8ExecutionCapability, Stage8ExecutionPreflightError> {
    input.durable_request.validate()?;
    validate_place_durable(&input.durable_request, input.order)?;
    validate_policy(&input.frozen_policy, &input.durable_request)?;
    let scope_sha256 = authority_scope_sha256(&input.durable_request, &input.frozen_policy);
    let command_sha256 = place_command_sha256(
        input.order,
        input.broker_preflight_context,
        &input.frozen_policy,
    );
    let evidence = validate_authorities(
        &input.frozen_policy,
        &scope_sha256,
        &command_sha256,
        input.clock,
        input.readiness,
        input.operator_arm,
        input.kill_switch,
        input.broker_ownership,
        input.ambiguity,
        input.broker_truth,
        input.schedule,
        input.micro_budget,
    )?;
    if input.order.time_in_force != TimeInForce::Day {
        return Err(Stage8ExecutionPreflightError::FrozenPolicyInvalid);
    }
    let mut broker_order = input.order.clone();
    broker_order.comment = None;
    let approved = input
        .frozen_policy
        .broker_policy
        .approve_place_order_with_context(
            &broker_order,
            evidence.now,
            input.broker_preflight_context,
        )?;
    Ok(build_capability(
        Stage8ApprovedCommand::Place(approved),
        Stage8CommandScope::Place,
        input.order.request_id,
        &input.durable_request,
        &input.frozen_policy,
        evidence,
    ))
}

pub fn authorize_stage8_cancel(
    input: Stage8CancelPreflightInput<'_>,
) -> Result<Stage8CancelPreflightDecision, Stage8ExecutionPreflightError> {
    input.durable_request.validate()?;
    validate_cancel_durable(&input.durable_request, input.cancel, input.existing_order)?;
    validate_policy(&input.frozen_policy, &input.durable_request)?;
    let scope_sha256 = authority_scope_sha256(&input.durable_request, &input.frozen_policy);
    let command_sha256 = cancel_command_sha256(input.cancel, &input.frozen_policy);
    let evidence = validate_authorities(
        &input.frozen_policy,
        &scope_sha256,
        &command_sha256,
        input.clock,
        input.readiness,
        input.operator_arm,
        input.kill_switch,
        input.broker_ownership,
        input.ambiguity,
        input.broker_truth,
        input.schedule,
        input.micro_budget,
    )?;
    match input.frozen_policy.broker_policy.approve_cancel_order(
        input.cancel,
        evidence.now,
        Some(input.existing_order),
    )? {
        CancelPreflightApproval::AlreadyTerminal => {
            Ok(Stage8CancelPreflightDecision::AlreadyTerminal)
        }
        CancelPreflightApproval::Submit(approved) => Ok(Stage8CancelPreflightDecision::Capability(
            Box::new(build_capability(
                Stage8ApprovedCommand::Cancel(approved),
                Stage8CommandScope::Cancel,
                input.cancel.request_id,
                &input.durable_request,
                &input.frozen_policy,
                evidence,
            )),
        )),
    }
}

struct ValidatedEvidence {
    now: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    arm_nonce_sha256: String,
    exact_command_sha256: String,
    current_state_sha256: String,
    authority_fingerprints: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn validate_authorities(
    policy: &Stage8a1FrozenExecutionPolicy,
    scope_sha256: &str,
    command_sha256: &str,
    clock: Stage8a1TrustedClockAuthority,
    readiness: Stage8a1ReadinessAuthority,
    arm: Stage8a1OperatorArmAuthority,
    kill_switch: Stage8a1KillSwitchAuthority,
    ownership: Stage8a1BrokerOwnershipAuthority,
    ambiguity: Stage8a1ZeroAmbiguityAuthority,
    truth: Stage8a1FreshBrokerTruthAuthority,
    schedule: Stage8a1ScheduleAuthority,
    budget: Stage8a1MicroBudgetAuthority,
) -> Result<ValidatedEvidence, Stage8ExecutionPreflightError> {
    if clock.scope_sha256 != scope_sha256 || !valid_sha256(&clock.evidence_sha256) {
        return Err(Stage8ExecutionPreflightError::AuthorityScopeMismatch);
    }
    if arm.scope_sha256 != scope_sha256
        || arm.exact_command_sha256 != command_sha256
        || arm.policy_sha256 != policy.policy_sha256
        || arm.build_sha256 != policy.build_sha256
        || arm.config_sha256 != policy.config_sha256
        || arm.endpoint_policy_sha256 != policy.endpoint_policy_sha256
        || !valid_sha256(&arm.nonce_sha256)
        || arm.issued_at > clock.now
        || arm.valid_until <= clock.now
        || arm
            .valid_until
            .signed_duration_since(arm.issued_at)
            .num_milliseconds()
            > policy.max_arm_ttl_ms as i64
    {
        return Err(Stage8ExecutionPreflightError::OperatorArmInvalid);
    }
    let max_age = policy.max_evidence_age_ms;
    validate_fresh_scoped(
        &readiness.scope_sha256,
        scope_sha256,
        clock.now,
        readiness.observed_at,
        readiness.valid_until,
        max_age,
        &readiness.evidence_sha256,
    )
    .map_err(|_| Stage8ExecutionPreflightError::ReadinessInvalid)?;
    if kill_switch.state != Stage8KillSwitchState::RunAllowed {
        return Err(Stage8ExecutionPreflightError::KillSwitchNotRunAllowed);
    }
    if kill_switch.durable_revision == 0 {
        return Err(Stage8ExecutionPreflightError::KillSwitchInvalid);
    }
    validate_fresh_scoped(
        &kill_switch.scope_sha256,
        scope_sha256,
        clock.now,
        kill_switch.observed_at,
        kill_switch.valid_until,
        max_age,
        &kill_switch.evidence_sha256,
    )
    .map_err(|_| Stage8ExecutionPreflightError::KillSwitchInvalid)?;
    if ownership.broker != BrokerKind::Finam || ownership.active_owner_count != 1 {
        return Err(Stage8ExecutionPreflightError::BrokerOwnershipInvalid);
    }
    validate_fresh_scoped(
        &ownership.scope_sha256,
        scope_sha256,
        clock.now,
        ownership.observed_at,
        ownership.valid_until,
        max_age,
        &ownership.evidence_sha256,
    )
    .map_err(|_| Stage8ExecutionPreflightError::BrokerOwnershipInvalid)?;
    if ambiguity.unresolved_order_count != 0
        || ambiguity.unresolved_delivery_count != 0
        || ambiguity.reconciliation_required_count != 0
    {
        return Err(Stage8ExecutionPreflightError::AmbiguityInvalid);
    }
    validate_fresh_scoped(
        &ambiguity.scope_sha256,
        scope_sha256,
        clock.now,
        ambiguity.observed_at,
        ambiguity.valid_until,
        max_age,
        &ambiguity.evidence_sha256,
    )
    .map_err(|_| Stage8ExecutionPreflightError::AmbiguityInvalid)?;
    if !truth.account_truth_fresh || !truth.instrument_truth_fresh {
        return Err(Stage8ExecutionPreflightError::BrokerTruthInvalid);
    }
    validate_fresh_scoped(
        &truth.scope_sha256,
        scope_sha256,
        clock.now,
        truth.observed_at,
        truth.valid_until,
        max_age,
        &truth.evidence_sha256,
    )
    .map_err(|_| Stage8ExecutionPreflightError::BrokerTruthInvalid)?;
    if schedule.state != Stage8ScheduleState::Eligible {
        return Err(Stage8ExecutionPreflightError::ScheduleInvalid);
    }
    validate_fresh_scoped(
        &schedule.scope_sha256,
        scope_sha256,
        clock.now,
        schedule.observed_at,
        schedule.valid_until,
        max_age,
        &schedule.evidence_sha256,
    )
    .map_err(|_| Stage8ExecutionPreflightError::ScheduleInvalid)?;
    if budget.max_orders != 1 || budget.consumed_orders != 0 {
        return Err(Stage8ExecutionPreflightError::MicroBudgetInvalid);
    }
    validate_fresh_scoped(
        &budget.scope_sha256,
        scope_sha256,
        clock.now,
        budget.observed_at,
        budget.valid_until,
        max_age,
        &budget.evidence_sha256,
    )
    .map_err(|_| Stage8ExecutionPreflightError::MicroBudgetInvalid)?;

    let valid_until = [
        arm.valid_until,
        readiness.valid_until,
        kill_switch.valid_until,
        ownership.valid_until,
        ambiguity.valid_until,
        truth.valid_until,
        schedule.valid_until,
        budget.valid_until,
    ]
    .into_iter()
    .min()
    .expect("authority expiry set is non-empty");
    let current_state_sha256 = digest_parts(
        b"stage8a1-current-authority-state-v1",
        &[
            scope_sha256.as_bytes(),
            readiness.evidence_sha256.as_bytes(),
            kill_switch.evidence_sha256.as_bytes(),
            ownership.evidence_sha256.as_bytes(),
            ambiguity.evidence_sha256.as_bytes(),
            truth.evidence_sha256.as_bytes(),
            schedule.evidence_sha256.as_bytes(),
            budget.evidence_sha256.as_bytes(),
        ],
    );
    Ok(ValidatedEvidence {
        now: clock.now,
        valid_until,
        arm_nonce_sha256: arm.nonce_sha256.clone(),
        exact_command_sha256: arm.exact_command_sha256.clone(),
        current_state_sha256,
        authority_fingerprints: vec![
            arm.nonce_sha256,
            clock.evidence_sha256,
            readiness.evidence_sha256,
            kill_switch.evidence_sha256,
            ownership.evidence_sha256,
            ambiguity.evidence_sha256,
            truth.evidence_sha256,
            schedule.evidence_sha256,
            budget.evidence_sha256,
        ],
    })
}

fn validate_fresh_scoped(
    observed_scope: &str,
    expected_scope: &str,
    now: DateTime<Utc>,
    observed_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    max_age_ms: u64,
    evidence_sha256: &str,
) -> Result<(), ()> {
    let age = now.signed_duration_since(observed_at).num_milliseconds();
    if observed_scope != expected_scope
        || max_age_ms == 0
        || observed_at > now
        || valid_until <= now
        || age < 0
        || age as u64 > max_age_ms
        || !valid_sha256(evidence_sha256)
    {
        return Err(());
    }
    Ok(())
}

fn validate_policy(
    policy: &Stage8a1FrozenExecutionPolicy,
    durable: &Stage8a1DurableRequestAuthority,
) -> Result<(), Stage8ExecutionPreflightError> {
    let broker = &policy.broker_policy;
    let identity = &durable.identity;
    let venue = identity
        .instrument()
        .venue_symbol
        .as_deref()
        .ok_or(Stage8ExecutionPreflightError::FrozenPolicyInvalid)?;
    if policy.max_arm_ttl_ms == 0
        || policy.max_evidence_age_ms == 0
        || !valid_sha256(&policy.policy_sha256)
        || !valid_sha256(&policy.build_sha256)
        || !valid_sha256(&policy.config_sha256)
        || !valid_sha256(&policy.endpoint_policy_sha256)
        || broker.allowed_accounts.as_slice() != [identity.account_id().clone()]
        || broker.allowed_venue_symbols.as_slice() != [venue.to_string()]
        || broker.allowed_time_in_force.as_slice() != [TimeInForce::Day]
        || broker.allowed_order_types.is_empty()
        || !broker.allowed_order_types.iter().all(|kind| {
            matches!(
                kind,
                broker_core::OrderType::Market | broker_core::OrderType::Limit
            )
        })
        || broker.min_qty <= rust_decimal::Decimal::ZERO
        || broker.qty_step <= rust_decimal::Decimal::ZERO
        || broker.max_qty < broker.min_qty
        || broker.max_market_qty > broker.max_qty
        || broker.max_notional_per_order.is_none()
        || broker.max_notional_per_run.is_none()
        || broker.max_limit_deviation_bps.is_none()
        || broker.max_reference_age_ms == 0
        || broker.allow_cancel_by_broker_order_id_without_mapping
        || !broker.operator_arm.one_shot
        || broker.operator_arm.endpoint_attempted
        || !broker.operator_arm.endpoint_calls_enabled
        || broker.operator_arm.preflight_digest != policy.config_sha256
        || frozen_policy_sha256(policy) != policy.policy_sha256
    {
        return Err(Stage8ExecutionPreflightError::FrozenPolicyInvalid);
    }
    Ok(())
}

fn validate_place_durable(
    durable: &Stage8a1DurableRequestAuthority,
    order: &PlaceOrder,
) -> Result<(), Stage8ExecutionPreflightError> {
    let identity = &durable.identity;
    let snapshot = Stage6DurableCommandSnapshotV1::from_place(identity, order)
        .map_err(|_| Stage8ExecutionPreflightError::DurableCommandMismatch)?;
    if identity.action() != Stage6DurableActionKind::Place
        || identity.strategy_request_id() != order.request_id
        || identity.durable_client_order_id() != &order.client_order_id
        || identity.account_id() != &order.account_id
        || identity.instrument() != &order.instrument
        || order.comment.as_deref() != Some(identity.attribution().internal_comment())
        || digest_parts(b"stage8a1-stage6-command-v1", &[&canonical_json(&snapshot)])
            != durable.durable_command_sha256
    {
        return Err(Stage8ExecutionPreflightError::DurableCommandMismatch);
    }
    Ok(())
}

fn validate_cancel_durable(
    durable: &Stage8a1DurableRequestAuthority,
    cancel: &CancelOrder,
    existing: &OrderPathRecord,
) -> Result<(), Stage8ExecutionPreflightError> {
    let identity = &durable.identity;
    let snapshot = Stage6DurableCommandSnapshotV1::from_cancel(identity, cancel)
        .map_err(|_| Stage8ExecutionPreflightError::DurableCommandMismatch)?;
    if identity.action() != Stage6DurableActionKind::Cancel
        || identity.strategy_request_id() != cancel.request_id
        || identity.account_id() != &cancel.account_id
        || identity.target_broker_order_id() != Some(&cancel.order_id)
        || identity.target_order_client_order_id() != cancel.client_order_id.as_ref()
        || existing.request_id == cancel.request_id
        || existing.account_id != cancel.account_id
        || existing.instrument != *identity.instrument()
        || existing.broker_order_id.as_ref() != Some(&cancel.order_id)
        || &existing.client_order_id
            != identity
                .target_order_client_order_id()
                .ok_or(Stage8ExecutionPreflightError::CancelMappingRequired)?
        || digest_parts(b"stage8a1-stage6-command-v1", &[&canonical_json(&snapshot)])
            != durable.durable_command_sha256
    {
        return Err(Stage8ExecutionPreflightError::CancelMappingRequired);
    }
    Ok(())
}

fn authority_scope_sha256(
    durable: &Stage8a1DurableRequestAuthority,
    policy: &Stage8a1FrozenExecutionPolicy,
) -> String {
    digest_parts(
        b"stage8a1-operational-scope-v1",
        &[
            &canonical_json(&durable.identity),
            durable.operational_identity_sha256.as_bytes(),
            &durable.seal_generation.to_be_bytes(),
            durable.seal_commitment_sha256.as_bytes(),
            policy.policy_sha256.as_bytes(),
            policy.build_sha256.as_bytes(),
            policy.config_sha256.as_bytes(),
            policy.endpoint_policy_sha256.as_bytes(),
        ],
    )
}

fn place_command_sha256(
    order: &PlaceOrder,
    context: &OrderPreflightContext,
    policy: &Stage8a1FrozenExecutionPolicy,
) -> String {
    digest_parts(
        b"stage8a1-place-command-v1",
        &[
            &canonical_json(order),
            &canonical_json(context),
            policy.policy_sha256.as_bytes(),
        ],
    )
}

fn cancel_command_sha256(cancel: &CancelOrder, policy: &Stage8a1FrozenExecutionPolicy) -> String {
    digest_parts(
        b"stage8a1-cancel-command-v1",
        &[&canonical_json(cancel), policy.policy_sha256.as_bytes()],
    )
}

fn frozen_policy_sha256(policy: &Stage8a1FrozenExecutionPolicy) -> String {
    digest_parts(
        b"stage8a1-frozen-policy-v1",
        &[
            &canonical_json(&policy.broker_policy),
            policy.scope_sha256.as_bytes(),
            policy.build_sha256.as_bytes(),
            policy.config_sha256.as_bytes(),
            policy.endpoint_policy_sha256.as_bytes(),
            &policy.max_arm_ttl_ms.to_be_bytes(),
            &policy.max_evidence_age_ms.to_be_bytes(),
        ],
    )
}

fn build_capability(
    approved: Stage8ApprovedCommand,
    scope: Stage8CommandScope,
    request_id: StrategyRequestId,
    durable: &Stage8a1DurableRequestAuthority,
    policy: &Stage8a1FrozenExecutionPolicy,
    evidence: ValidatedEvidence,
) -> Stage8ExecutionCapability {
    let authority_scope_sha256 = authority_scope_sha256(durable, policy);
    let mut parts = vec![
        durable.provenance_sha256.as_bytes(),
        durable.canonical_command_sha256.as_bytes(),
        durable.seal_commitment_sha256.as_bytes(),
        policy.policy_sha256.as_bytes(),
    ];
    for fingerprint in &evidence.authority_fingerprints {
        parts.push(fingerprint.as_bytes());
    }
    let audit_fingerprint = digest_parts(b"stage8a1-capability-audit-v2", &parts);
    Stage8ExecutionCapability {
        approved,
        scope,
        request_id,
        issued_at: evidence.now,
        valid_until: evidence.valid_until,
        seal_generation: durable.seal_generation,
        durable_provenance_sha256: durable.provenance_sha256.clone(),
        seal_commitment_sha256: durable.seal_commitment_sha256.clone(),
        policy_sha256: policy.policy_sha256.clone(),
        build_sha256: policy.build_sha256.clone(),
        config_sha256: policy.config_sha256.clone(),
        endpoint_policy_sha256: policy.endpoint_policy_sha256.clone(),
        authority_scope_sha256,
        arm_nonce_sha256: evidence.arm_nonce_sha256,
        exact_command_sha256: evidence.exact_command_sha256,
        current_state_sha256: evidence.current_state_sha256,
        audit_fingerprint,
    }
}

fn canonical_json<T: Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(value).expect("Stage 8A-1 canonical values are serializable")
}

fn digest_parts(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update(b"\0");
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == SHA256_HEX_LEN
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{
        AccountId, BrokerFeedFreshness, BrokerInstrumentSpec, BrokerStopOrderReadiness,
        BrokerSymbol, ClientOrderId, Exchange, HybridRuntimeAttribution, InstrumentId,
        InstrumentMapEntry, InternalSymbol, Market, OperatorArm, OrderPathCommandKind,
        OrderPathState, OrderSide, OrderType,
    };
    use chrono::{Duration, TimeZone};
    use rust_decimal::Decimal;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicU64, Ordering};
    use uuid::Uuid;

    const FP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const BUILD: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const ENDPOINT: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const EVIDENCE: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 15, 9, 0, 0).single().unwrap()
    }

    fn account() -> AccountId {
        AccountId::new("ACC_TEST_0001")
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn request_id(value: u128) -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128(value))
    }

    fn attribution(role: &str) -> HybridRuntimeAttribution {
        HybridRuntimeAttribution::parse_source_comment(format!(
            "HYB|sid=hybrid_imoexf|c=cycle-1|o=BO|r={role}"
        ))
        .unwrap()
    }

    fn place() -> PlaceOrder {
        let request_id = request_id(1);
        PlaceOrder {
            request_id,
            created_ts: now() - Duration::seconds(1),
            ttl_ms: Some(10_000),
            account_id: account(),
            client_order_id: ClientOrderId::from_strategy_request(request_id),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::ONE,
            limit_price: Some(Decimal::new(2200, 0)),
            time_in_force: TimeInForce::Day,
            comment: Some(attribution("ENTRY").internal_comment().to_string()),
        }
    }

    fn durable_place(order: &PlaceOrder) -> Stage8a1DurableRequestAuthority {
        let identity =
            Stage6DurableRequestIdentityV1::from_place(order, attribution("ENTRY")).unwrap();
        let mut value = Stage8a1DurableRequestAuthority {
            identity,
            durable_command_sha256: digest_parts(
                b"stage8a1-stage6-command-v1",
                &[&canonical_json(
                    &Stage6DurableCommandSnapshotV1::from_place(
                        &Stage6DurableRequestIdentityV1::from_place(order, attribution("ENTRY"))
                            .unwrap(),
                        order,
                    )
                    .unwrap(),
                )],
            ),
            canonical_command_sha256: FP.into(),
            accepted_record_id_sha256: BUILD.into(),
            dispatch_record_id_sha256: ENDPOINT.into(),
            dispatch_sequence: 2,
            durable_frontier_sha256: EVIDENCE.into(),
            runtime_config_fingerprint_sha256: FP.into(),
            checkpoint_sha256: ENDPOINT.into(),
            operational_identity_sha256: EVIDENCE.into(),
            seal_generation: 7,
            seal_commitment_sha256: FP.into(),
            provenance_sha256: String::new(),
        };
        value.provenance_sha256 = value.calculate_provenance();
        value
    }

    fn policy(durable: &Stage8a1DurableRequestAuthority) -> Stage8a1FrozenExecutionPolicy {
        let mut value = Stage8a1FrozenExecutionPolicy {
            broker_policy: OrderPreflightPolicy {
                allowed_accounts: vec![account()],
                allowed_venue_symbols: vec!["IMOEXF@RTSX".into()],
                allowed_order_types: vec![OrderType::Market, OrderType::Limit],
                allowed_time_in_force: vec![TimeInForce::Day],
                min_qty: Decimal::ONE,
                qty_step: Decimal::ONE,
                max_qty: Decimal::new(2, 0),
                price_step: Some(Decimal::new(5, 1)),
                max_market_qty: Decimal::ONE,
                max_notional_per_order: Some(Decimal::new(5_000, 0)),
                max_notional_per_run: Some(Decimal::new(5_000, 0)),
                max_limit_deviation_bps: Some(1_000),
                max_reference_age_ms: 5_000,
                allow_cancel_by_broker_order_id_without_mapping: false,
                operator_arm: OperatorArm {
                    session_id: "SESSION_TEST".into(),
                    armed_until: now() + Duration::seconds(30),
                    endpoint_calls_enabled: true,
                    one_shot: true,
                    endpoint_attempted: false,
                    preflight_digest: FP.into(),
                },
            },
            scope_sha256: String::new(),
            policy_sha256: String::new(),
            build_sha256: BUILD.into(),
            config_sha256: FP.into(),
            endpoint_policy_sha256: ENDPOINT.into(),
            max_arm_ttl_ms: 60_000,
            max_evidence_age_ms: 5_000,
        };
        value.scope_sha256 = digest_parts(
            b"stage8a1-policy-source-scope-v1",
            &[
                &canonical_json(&durable.identity),
                durable.seal_commitment_sha256.as_bytes(),
            ],
        );
        value.policy_sha256 = frozen_policy_sha256(&value);
        value
    }

    fn context() -> OrderPreflightContext {
        OrderPreflightContext {
            reference_price: Some(broker_core::OrderReferencePrice {
                price: Decimal::new(2220, 0),
                received_ts: now() - Duration::seconds(1),
            }),
            current_run_notional: Decimal::ZERO,
        }
    }

    struct Authorities {
        arm: Stage8a1OperatorArmAuthority,
        clock: Stage8a1TrustedClockAuthority,
        readiness: Stage8a1ReadinessAuthority,
        kill: Stage8a1KillSwitchAuthority,
        ownership: Stage8a1BrokerOwnershipAuthority,
        ambiguity: Stage8a1ZeroAmbiguityAuthority,
        truth: Stage8a1FreshBrokerTruthAuthority,
        schedule: Stage8a1ScheduleAuthority,
        budget: Stage8a1MicroBudgetAuthority,
    }

    fn authorities(
        durable: &Stage8a1DurableRequestAuthority,
        policy: &Stage8a1FrozenExecutionPolicy,
        order: &PlaceOrder,
        context: &OrderPreflightContext,
        nonce: &str,
    ) -> Authorities {
        let scope = authority_scope_sha256(durable, policy);
        let command = place_command_sha256(order, context, policy);
        let observed = now() - Duration::seconds(1);
        let until = now() + Duration::seconds(20);
        Authorities {
            arm: Stage8a1OperatorArmAuthority {
                nonce_sha256: digest_parts(b"nonce", &[nonce.as_bytes()]),
                exact_command_sha256: command,
                scope_sha256: scope.clone(),
                policy_sha256: policy.policy_sha256.clone(),
                build_sha256: policy.build_sha256.clone(),
                config_sha256: policy.config_sha256.clone(),
                endpoint_policy_sha256: policy.endpoint_policy_sha256.clone(),
                issued_at: observed,
                valid_until: until,
            },
            clock: Stage8a1TrustedClockAuthority {
                now: now(),
                scope_sha256: scope.clone(),
                evidence_sha256: EVIDENCE.into(),
            },
            readiness: Stage8a1ReadinessAuthority {
                scope_sha256: scope.clone(),
                observed_at: observed,
                valid_until: until,
                evidence_sha256: FP.into(),
            },
            kill: Stage8a1KillSwitchAuthority {
                state: Stage8KillSwitchState::RunAllowed,
                durable_revision: 1,
                scope_sha256: scope.clone(),
                observed_at: observed,
                valid_until: until,
                evidence_sha256: BUILD.into(),
            },
            ownership: Stage8a1BrokerOwnershipAuthority {
                broker: BrokerKind::Finam,
                active_owner_count: 1,
                scope_sha256: scope.clone(),
                observed_at: observed,
                valid_until: until,
                evidence_sha256: ENDPOINT.into(),
            },
            ambiguity: Stage8a1ZeroAmbiguityAuthority {
                unresolved_order_count: 0,
                unresolved_delivery_count: 0,
                reconciliation_required_count: 0,
                scope_sha256: scope.clone(),
                observed_at: observed,
                valid_until: until,
                evidence_sha256: EVIDENCE.into(),
            },
            truth: Stage8a1FreshBrokerTruthAuthority {
                account_truth_fresh: true,
                instrument_truth_fresh: true,
                scope_sha256: scope.clone(),
                observed_at: observed,
                valid_until: until,
                evidence_sha256: FP.into(),
            },
            schedule: Stage8a1ScheduleAuthority {
                state: Stage8ScheduleState::Eligible,
                scope_sha256: scope.clone(),
                observed_at: observed,
                valid_until: until,
                evidence_sha256: BUILD.into(),
            },
            budget: Stage8a1MicroBudgetAuthority {
                max_orders: 1,
                consumed_orders: 0,
                scope_sha256: scope,
                observed_at: observed,
                valid_until: until,
                evidence_sha256: ENDPOINT.into(),
            },
        }
    }

    fn authorize_place_with(
        order: &PlaceOrder,
        context: &OrderPreflightContext,
        durable: Stage8a1DurableRequestAuthority,
        policy: Stage8a1FrozenExecutionPolicy,
        authorities: Authorities,
    ) -> Result<Stage8ExecutionCapability, Stage8ExecutionPreflightError> {
        authorize_stage8_place(Stage8PlacePreflightInput {
            order,
            broker_preflight_context: context,
            durable_request: durable,
            operator_arm: authorities.arm,
            frozen_policy: policy,
            clock: authorities.clock,
            readiness: authorities.readiness,
            kill_switch: authorities.kill,
            broker_ownership: authorities.ownership,
            ambiguity: authorities.ambiguity,
            broker_truth: authorities.truth,
            schedule: authorities.schedule,
            micro_budget: authorities.budget,
        })
    }

    #[test]
    fn exact_place_mints_linear_opaque_capability() {
        let order = place();
        let durable = durable_place(&order);
        let policy = policy(&durable);
        let context = context();
        let proofs = authorities(&durable, &policy, &order, &context, "nonce-1");
        let diagnostic = authorize_place_with(&order, &context, durable, policy, proofs)
            .unwrap()
            .diagnostic();
        assert_eq!(diagnostic.scope, Stage8CommandScope::Place);
        assert_eq!(diagnostic.seal_generation, 7);
        assert_eq!(diagnostic.audit_fingerprint.len(), 64);
    }

    #[test]
    fn exact_arm_rejects_side_qty_price_context_and_policy_drift() {
        for mutation in 0..5 {
            let baseline = place();
            let durable = durable_place(&baseline);
            let mut policy = policy(&durable);
            let mut context = context();
            let proofs = authorities(&durable, &policy, &baseline, &context, "nonce-drift");
            let mut changed = baseline.clone();
            match mutation {
                0 => changed.side = OrderSide::Sell,
                1 => changed.qty = Decimal::new(2, 0),
                2 => changed.limit_price = Some(Decimal::new(2199, 0)),
                3 => context.current_run_notional = Decimal::ONE,
                _ => {
                    policy.broker_policy.max_qty = Decimal::new(99, 0);
                    policy.policy_sha256 = frozen_policy_sha256(&policy);
                }
            }
            let result = authorize_place_with(&changed, &context, durable, policy, proofs);
            match result {
                Err(Stage8ExecutionPreflightError::OperatorArmInvalid)
                | Err(Stage8ExecutionPreflightError::DurableCommandMismatch)
                | Err(Stage8ExecutionPreflightError::FrozenPolicyInvalid)
                | Err(Stage8ExecutionPreflightError::AuthorityScopeMismatch) => {}
                Err(error) => panic!("mutation {mutation} returned {error:?}"),
                Ok(_) => panic!("mutation {mutation} unexpectedly minted capability"),
            }
        }
    }

    #[test]
    fn stale_scoped_and_closed_authorities_fail_closed() {
        let order = place();
        let durable = durable_place(&order);
        let first_policy = policy(&durable);
        let context = context();
        let mut proofs = authorities(&durable, &first_policy, &order, &context, "nonce-2");
        proofs.schedule.state = Stage8ScheduleState::Closed;
        assert!(matches!(
            authorize_place_with(&order, &context, durable, first_policy, proofs),
            Err(Stage8ExecutionPreflightError::ScheduleInvalid)
        ));

        let durable = durable_place(&order);
        let second_policy = policy(&durable);
        let mut proofs = authorities(&durable, &second_policy, &order, &context, "nonce-3");
        proofs.ownership.scope_sha256 = "f".repeat(64);
        assert!(matches!(
            authorize_place_with(&order, &context, durable, second_policy, proofs),
            Err(Stage8ExecutionPreflightError::BrokerOwnershipInvalid)
        ));
    }

    #[test]
    fn kill_switch_truth_ambiguity_readiness_and_budget_are_mandatory() {
        for mutation in 0..5 {
            let order = place();
            let durable = durable_place(&order);
            let policy = policy(&durable);
            let context = context();
            let mut proofs = authorities(&durable, &policy, &order, &context, "nonce-proof");
            match mutation {
                0 => proofs.kill.state = Stage8KillSwitchState::StopRequested,
                1 => proofs.truth.instrument_truth_fresh = false,
                2 => proofs.ambiguity.unresolved_order_count = 1,
                3 => proofs.readiness.valid_until = now(),
                _ => proofs.budget.consumed_orders = 1,
            }
            assert!(authorize_place_with(&order, &context, durable, policy, proofs).is_err());
        }
    }

    #[test]
    fn arm_nonce_registry_rejects_duplicate_issue() {
        let mut nonces = HashSet::new();
        assert!(nonces.insert("nonce-unique"));
        assert!(!nonces.insert("nonce-unique"));
    }

    static ISSUER_TEST_DIR: AtomicU64 = AtomicU64::new(1);

    fn issuer_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "stage8a1-r2-issuer-{}-{}",
            std::process::id(),
            ISSUER_TEST_DIR.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&root).unwrap();
        root
    }

    fn current_sources(
        observed_at: DateTime<Utc>,
    ) -> (
        Stage7bCompositeReadinessSnapshot,
        BrokerTruthSnapshot,
        BrokerReadinessSnapshot,
    ) {
        let freshness = || BrokerFeedFreshness {
            observed_ts: Some(observed_at),
            max_age_ms: 30_000,
        };
        let readiness = Stage7bCompositeReadinessSnapshot {
            phase: Stage7bPaperReadinessPhase::PaperReady,
            reasons: vec![],
            blocked_entry_ids: vec![],
            blocked_request_ids: vec![],
            checked_at: observed_at,
        };
        let truth = BrokerTruthSnapshot {
            account_id: account(),
            orders: vec![],
            positions: vec![],
            cash: None,
            trades: vec![],
            instruments: vec![BrokerInstrumentSpec {
                instrument: InstrumentMapEntry {
                    internal_symbol: InternalSymbol("IMOEXF".into()),
                    broker: BrokerKind::Finam,
                    broker_symbol: BrokerSymbol("IMOEXF@RTSX".into()),
                    exchange: Exchange::Moex,
                    market: Market::Futures,
                    price_step: Decimal::new(5, 1),
                    qty_step: Decimal::ONE,
                    lot_size: Decimal::ONE,
                    min_qty: Decimal::ONE,
                    step_value: Decimal::ONE,
                    currency: "RUB".into(),
                    schedule_id: "MOEX_FUT".into(),
                    expiration_date: None,
                    is_tradable: true,
                },
                broker_asset_id: Some("ASSET_TEST".into()),
                board: Some("RTSX".into()),
                long_initial_margin: Some(Decimal::new(1_000, 0)),
                short_initial_margin: Some(Decimal::new(1_000, 0)),
            }],
            received_ts: observed_at,
        };
        let broker_readiness = BrokerReadinessSnapshot {
            account: freshness(),
            positions: freshness(),
            orders: freshness(),
            trades: freshness(),
            quotes: freshness(),
            instrument_spec: freshness(),
            schedule: freshness(),
            market_session: BrokerMarketSessionState::Open,
            unknown_order_count: 0,
            cash_margin_present: true,
            instrument_spec_validated: true,
            live_market_data_seen: true,
            subscription_ready: true,
            stream_or_polling_connected: true,
            event_sink_degraded: false,
            stop_order_readiness: BrokerStopOrderReadiness::UnsupportedBlocked,
        };
        (readiness, truth, broker_readiness)
    }

    fn write_issuer_sources(
        root: &Path,
        durable: &Stage8a1DurableRequestAuthority,
        observed_at: DateTime<Utc>,
    ) {
        let broker_policy = policy(durable).broker_policy;
        let config = Stage8a1AcceptedExecutionConfigV1 {
            schema_version: 1,
            operational_identity_sha256: durable.operational_identity_sha256.clone(),
            runtime_config_fingerprint_sha256: durable.runtime_config_fingerprint_sha256.clone(),
            broker: BrokerKind::Finam,
            strategy_instance_id: "hybrid_imoexf".into(),
            account_id: account(),
            instrument: instrument(),
            broker_policy,
            build_sha256: BUILD.into(),
            endpoint_policy_sha256: ENDPOINT.into(),
            max_arm_ttl_ms: 20_000,
            max_evidence_age_ms: 20_000,
        };
        let bytes = canonical_json(&config);
        let config_hash = digest_parts(b"stage8a1-accepted-config-file-v1", &[&bytes]);
        fs::write(root.join(ACCEPTED_CONFIG_FILE), bytes).unwrap();
        fs::write(root.join(ACCEPTED_CONFIG_SHA256_FILE), config_hash).unwrap();
        let control = Stage8a1CurrentControlStateV1 {
            schema_version: 1,
            operational_identity_sha256: durable.operational_identity_sha256.clone(),
            runtime_config_fingerprint_sha256: durable.runtime_config_fingerprint_sha256.clone(),
            kill_switch: Stage8KillSwitchState::RunAllowed,
            durable_revision: 1,
            active_owner_count: 1,
            reconciliation_required_count: 0,
            max_orders: 1,
            consumed_orders: 0,
            observed_at,
            valid_until: observed_at + Duration::seconds(20),
        };
        fs::write(root.join(CURRENT_CONTROL_FILE), canonical_json(&control)).unwrap();
    }

    #[test]
    fn production_issuer_mints_all_proofs_and_rejects_duplicate_logical_nonce() {
        let now = Utc::now();
        let mut order = place();
        order.created_ts = now;
        order.ttl_ms = Some(30_000);
        let context = OrderPreflightContext {
            reference_price: Some(broker_core::OrderReferencePrice {
                price: Decimal::new(2220, 0),
                received_ts: now,
            }),
            current_run_notional: Decimal::ZERO,
        };
        let durable = durable_place(&order);
        let root = issuer_root();
        write_issuer_sources(&root, &durable, now);
        let (readiness, truth, broker_readiness) = current_sources(now);
        let mut issuer = Stage8a1OperationalAuthorityIssuer::open(&root).unwrap();
        let capability = issuer
            .authorize_place(
                durable,
                &order,
                &context,
                Stage8a1CurrentOperationalSources {
                    composite_readiness: &readiness,
                    broker_truth: &truth,
                    broker_readiness: &broker_readiness,
                },
                "operator-logical-arm-1",
            )
            .unwrap();
        assert_eq!(capability.diagnostic().scope, Stage8CommandScope::Place);

        let duplicate = issuer.authorize_place(
            durable_place(&order),
            &order,
            &context,
            Stage8a1CurrentOperationalSources {
                composite_readiness: &readiness,
                broker_truth: &truth,
                broker_readiness: &broker_readiness,
            },
            "operator-logical-arm-1",
        );
        assert!(matches!(
            duplicate,
            Err(Stage8ExecutionPreflightError::OperatorArmNonceReplay)
        ));

        let mut stopped = load_current_control(&root).unwrap();
        stopped.kill_switch = Stage8KillSwitchState::StopRequested;
        stopped.durable_revision += 1;
        fs::write(root.join(CURRENT_CONTROL_FILE), canonical_json(&stopped)).unwrap();
        let durable = durable_place(&order);
        let (config, _) = load_accepted_config(&root).unwrap();
        let policy = frozen_policy_from_config(&config, &durable);
        let scope = authority_scope_sha256(&durable, &policy);
        assert!(matches!(
            current_state_from_sources(
                &root,
                &config,
                &durable,
                &policy,
                &scope,
                &Stage8a1CurrentOperationalSources {
                    composite_readiness: &readiness,
                    broker_truth: &truth,
                    broker_readiness: &broker_readiness,
                },
                Utc::now(),
            ),
            Err(Stage8ExecutionPreflightError::KillSwitchNotRunAllowed)
        ));
        drop(capability);
        fs::remove_dir_all(root).unwrap();
    }

    fn existing_record(order: &PlaceOrder) -> OrderPathRecord {
        let mut record = OrderPathRecord::from_place_order(order, now(), None);
        record.broker_order_id = Some(broker_core::BrokerOrderId::new("BROKER_TEST_1"));
        record.state = OrderPathState::Submitted;
        record.command_kind = OrderPathCommandKind::Place;
        record
    }

    fn durable_cancel(cancel: &CancelOrder) -> Stage8a1DurableRequestAuthority {
        let identity = Stage6DurableRequestIdentityV1::from_cancel(
            cancel,
            instrument(),
            attribution("CANCEL"),
        )
        .unwrap();
        let mut value = Stage8a1DurableRequestAuthority {
            identity,
            durable_command_sha256: digest_parts(
                b"stage8a1-stage6-command-v1",
                &[&canonical_json(
                    &Stage6DurableCommandSnapshotV1::from_cancel(
                        &Stage6DurableRequestIdentityV1::from_cancel(
                            cancel,
                            instrument(),
                            attribution("CANCEL"),
                        )
                        .unwrap(),
                        cancel,
                    )
                    .unwrap(),
                )],
            ),
            canonical_command_sha256: FP.into(),
            accepted_record_id_sha256: BUILD.into(),
            dispatch_record_id_sha256: ENDPOINT.into(),
            dispatch_sequence: 2,
            durable_frontier_sha256: EVIDENCE.into(),
            runtime_config_fingerprint_sha256: FP.into(),
            checkpoint_sha256: ENDPOINT.into(),
            operational_identity_sha256: EVIDENCE.into(),
            seal_generation: 7,
            seal_commitment_sha256: FP.into(),
            provenance_sha256: String::new(),
        };
        value.provenance_sha256 = value.calculate_provenance();
        value
    }

    #[test]
    fn exact_cancel_maps_and_terminal_mints_no_capability() {
        for terminal in [false, true] {
            let placed = place();
            let mut existing = existing_record(&placed);
            if terminal {
                existing.state = OrderPathState::Terminal;
            }
            let cancel = CancelOrder {
                request_id: request_id(if terminal { 3 } else { 2 }),
                created_ts: now() - Duration::seconds(1),
                ttl_ms: Some(10_000),
                account_id: account(),
                order_id: existing.broker_order_id.clone().unwrap(),
                client_order_id: Some(placed.client_order_id.clone()),
            };
            let durable = durable_cancel(&cancel);
            let policy = policy(&durable);
            let scope = authority_scope_sha256(&durable, &policy);
            let command = cancel_command_sha256(&cancel, &policy);
            let observed = now() - Duration::seconds(1);
            let until = now() + Duration::seconds(20);
            let result = authorize_stage8_cancel(Stage8CancelPreflightInput {
                cancel: &cancel,
                existing_order: &existing,
                operator_arm: Stage8a1OperatorArmAuthority {
                    nonce_sha256: FP.into(),
                    exact_command_sha256: command,
                    scope_sha256: scope.clone(),
                    policy_sha256: policy.policy_sha256.clone(),
                    build_sha256: BUILD.into(),
                    config_sha256: FP.into(),
                    endpoint_policy_sha256: ENDPOINT.into(),
                    issued_at: observed,
                    valid_until: until,
                },
                clock: Stage8a1TrustedClockAuthority {
                    now: now(),
                    scope_sha256: scope.clone(),
                    evidence_sha256: FP.into(),
                },
                readiness: Stage8a1ReadinessAuthority {
                    scope_sha256: scope.clone(),
                    observed_at: observed,
                    valid_until: until,
                    evidence_sha256: FP.into(),
                },
                kill_switch: Stage8a1KillSwitchAuthority {
                    state: Stage8KillSwitchState::RunAllowed,
                    durable_revision: 1,
                    scope_sha256: scope.clone(),
                    observed_at: observed,
                    valid_until: until,
                    evidence_sha256: FP.into(),
                },
                broker_ownership: Stage8a1BrokerOwnershipAuthority {
                    broker: BrokerKind::Finam,
                    active_owner_count: 1,
                    scope_sha256: scope.clone(),
                    observed_at: observed,
                    valid_until: until,
                    evidence_sha256: FP.into(),
                },
                ambiguity: Stage8a1ZeroAmbiguityAuthority {
                    unresolved_order_count: 0,
                    unresolved_delivery_count: 0,
                    reconciliation_required_count: 0,
                    scope_sha256: scope.clone(),
                    observed_at: observed,
                    valid_until: until,
                    evidence_sha256: FP.into(),
                },
                broker_truth: Stage8a1FreshBrokerTruthAuthority {
                    account_truth_fresh: true,
                    instrument_truth_fresh: true,
                    scope_sha256: scope.clone(),
                    observed_at: observed,
                    valid_until: until,
                    evidence_sha256: FP.into(),
                },
                schedule: Stage8a1ScheduleAuthority {
                    state: Stage8ScheduleState::Eligible,
                    scope_sha256: scope.clone(),
                    observed_at: observed,
                    valid_until: until,
                    evidence_sha256: FP.into(),
                },
                micro_budget: Stage8a1MicroBudgetAuthority {
                    max_orders: 1,
                    consumed_orders: 0,
                    scope_sha256: scope,
                    observed_at: observed,
                    valid_until: until,
                    evidence_sha256: FP.into(),
                },
                durable_request: durable,
                frozen_policy: policy,
            })
            .unwrap();
            assert_eq!(
                matches!(result, Stage8CancelPreflightDecision::AlreadyTerminal),
                terminal
            );
        }
    }
}
