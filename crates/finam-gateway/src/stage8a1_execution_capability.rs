//! Stage 8A-1 protected execution capability and strict no-send preflight.
//!
//! The capability is intentionally opaque, non-cloneable and non-serializable.
//! Stage 8A-1 provides no method that can extract a broker request or perform
//! transport I/O. Later stages must add their own independently accepted,
//! consuming transition.
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
//! use finam_gateway::Stage8ExecutionCapability;
//! let _ = Stage8ExecutionCapability {};
//! ```

use broker_core::{
    AccountId, BrokerKind, CancelOrder, CancelPreflightApproval, InstrumentId, OrderPathRecord,
    OrderPreflightContext, OrderPreflightError, OrderPreflightPolicy, PlaceOrder,
    PreflightApprovedCancelOrder, PreflightApprovedPlaceOrder, StrategyRequestId, TimeInForce,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const SHA256_HEX_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8CommandScope {
    Place,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8KillSwitchState {
    RunAllowed,
    StopRequested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8ExecutionAllowlist {
    pub accounts: Vec<AccountId>,
    pub instruments: Vec<InstrumentId>,
    pub strategy_ids: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Stage8OperatorArmInput {
    pub arm_id: String,
    pub session_id: String,
    pub command_scope: Stage8CommandScope,
    pub request_id: StrategyRequestId,
    pub account_id: AccountId,
    pub instrument: InstrumentId,
    pub strategy_id: String,
    pub armed_at: DateTime<Utc>,
    pub armed_until: DateTime<Utc>,
    pub one_shot: bool,
    pub restart_generation: u64,
    pub config_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8PersistentKillSwitchEvidenceInput {
    pub state: Stage8KillSwitchState,
    pub durable_revision: u64,
    pub persisted_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub restart_generation: u64,
    pub config_fingerprint: String,
    pub evidence_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8SingleBrokerOwnershipEvidenceInput {
    pub broker: BrokerKind,
    pub active_broker_owner_count: u32,
    pub account_id: AccountId,
    pub instrument: InstrumentId,
    pub observed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub restart_generation: u64,
    pub config_fingerprint: String,
    pub evidence_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8ZeroAmbiguityEvidenceInput {
    pub unresolved_order_count: u32,
    pub unresolved_delivery_count: u32,
    pub reconciliation_required_count: u32,
    pub observed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub restart_generation: u64,
    pub config_fingerprint: String,
    pub evidence_sha256: String,
}

#[derive(Debug, PartialEq)]
pub struct Stage8PlacePreflightInput<'a> {
    pub order: &'a PlaceOrder,
    pub strategy_id: &'a str,
    pub now: DateTime<Utc>,
    pub restart_generation: u64,
    pub config_fingerprint: &'a str,
    pub max_arm_ttl_ms: u64,
    pub max_evidence_age_ms: u64,
    pub allowlist: &'a Stage8ExecutionAllowlist,
    pub broker_preflight_policy: &'a OrderPreflightPolicy,
    pub broker_preflight_context: &'a OrderPreflightContext,
    pub operator_arm: Stage8OperatorArmInput,
    pub kill_switch: Stage8PersistentKillSwitchEvidenceInput,
    pub broker_ownership: Stage8SingleBrokerOwnershipEvidenceInput,
    pub ambiguity: Stage8ZeroAmbiguityEvidenceInput,
}

#[derive(Debug, PartialEq)]
pub struct Stage8CancelPreflightInput<'a> {
    pub cancel: &'a CancelOrder,
    pub existing_order: &'a OrderPathRecord,
    pub strategy_id: &'a str,
    pub now: DateTime<Utc>,
    pub restart_generation: u64,
    pub config_fingerprint: &'a str,
    pub max_arm_ttl_ms: u64,
    pub max_evidence_age_ms: u64,
    pub allowlist: &'a Stage8ExecutionAllowlist,
    pub broker_preflight_policy: &'a OrderPreflightPolicy,
    pub operator_arm: Stage8OperatorArmInput,
    pub kill_switch: Stage8PersistentKillSwitchEvidenceInput,
    pub broker_ownership: Stage8SingleBrokerOwnershipEvidenceInput,
    pub ambiguity: Stage8ZeroAmbiguityEvidenceInput,
}

enum Stage8ApprovedCommand {
    Place(PreflightApprovedPlaceOrder),
    Cancel(PreflightApprovedCancelOrder),
}

/// Linear authority issued only by the Stage 8A-1 strict preflight.
///
/// No public or crate-private extraction/dispatch method exists in Stage 8A-1.
pub struct Stage8ExecutionCapability {
    approved: Stage8ApprovedCommand,
    scope: Stage8CommandScope,
    request_id: StrategyRequestId,
    issued_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    restart_generation: u64,
    config_fingerprint: String,
    audit_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8CapabilityDiagnostic {
    pub scope: Stage8CommandScope,
    pub issued_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub restart_generation: u64,
    pub audit_fingerprint: String,
}

impl Stage8ExecutionCapability {
    pub fn diagnostic(&self) -> Stage8CapabilityDiagnostic {
        let approved_request_id = match &self.approved {
            Stage8ApprovedCommand::Place(approved) => approved.order().request_id,
            Stage8ApprovedCommand::Cancel(approved) => approved.cancel().request_id,
        };
        debug_assert_eq!(approved_request_id, self.request_id);
        debug_assert!(valid_sha256(&self.config_fingerprint));
        Stage8CapabilityDiagnostic {
            scope: self.scope,
            issued_at: self.issued_at,
            valid_until: self.valid_until,
            restart_generation: self.restart_generation,
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
    #[error("Stage 8 allowlist is empty or contains duplicates")]
    InvalidAllowlist,
    #[error("Stage 8 strategy id is invalid")]
    InvalidStrategyId,
    #[error("Stage 8 account is not allowlisted")]
    AccountNotAllowed,
    #[error("Stage 8 instrument is not allowlisted")]
    InstrumentNotAllowed,
    #[error("Stage 8 strategy is not allowlisted")]
    StrategyNotAllowed,
    #[error("Stage 8 only permits DAY time in force")]
    TimeInForceNotDay,
    #[error("Stage 8 broker preflight policy is wider than the frozen policy")]
    BrokerPolicyTooWide,
    #[error("Stage 8 operator arm is invalid")]
    InvalidOperatorArm,
    #[error("Stage 8 operator arm binding does not match the command")]
    OperatorArmBindingMismatch,
    #[error("Stage 8 operator arm expired or is not current")]
    OperatorArmExpired,
    #[error("Stage 8 restart generation is invalid or stale")]
    RestartGenerationMismatch,
    #[error("Stage 8 configuration fingerprint is invalid or stale")]
    ConfigFingerprintMismatch,
    #[error("Stage 8 persistent kill switch is not RunAllowed")]
    KillSwitchNotRunAllowed,
    #[error("Stage 8 persistent kill-switch evidence is invalid or stale")]
    KillSwitchEvidenceInvalid,
    #[error("Stage 8 broker ownership is not exactly one FINAM owner")]
    BrokerOwnershipInvalid,
    #[error("Stage 8 unresolved ambiguity is non-zero")]
    UnresolvedAmbiguity,
    #[error("Stage 8 ambiguity evidence is invalid or stale")]
    AmbiguityEvidenceInvalid,
    #[error("Stage 8 cancel requires an exact existing order mapping")]
    CancelMappingRequired,
    #[error("broker-neutral preflight rejected the command: {0}")]
    BrokerPreflight(#[from] OrderPreflightError),
}

pub fn authorize_stage8_place(
    input: Stage8PlacePreflightInput<'_>,
) -> Result<Stage8ExecutionCapability, Stage8ExecutionPreflightError> {
    validate_common(
        Stage8CommandScope::Place,
        input.order.request_id,
        &input.order.account_id,
        &input.order.instrument,
        input.strategy_id,
        input.now,
        input.restart_generation,
        input.config_fingerprint,
        input.max_arm_ttl_ms,
        input.max_evidence_age_ms,
        input.allowlist,
        &input.operator_arm,
        &input.kill_switch,
        &input.broker_ownership,
        &input.ambiguity,
    )?;
    validate_frozen_broker_policy(
        input.broker_preflight_policy,
        input.allowlist,
        &input.operator_arm,
        input.config_fingerprint,
    )?;
    if input.order.time_in_force != TimeInForce::Day {
        return Err(Stage8ExecutionPreflightError::TimeInForceNotDay);
    }
    let approved = input
        .broker_preflight_policy
        .approve_place_order_with_context(input.order, input.now, input.broker_preflight_context)?;
    Ok(build_capability(
        Stage8ApprovedCommand::Place(approved),
        Stage8CommandScope::Place,
        input.order.request_id,
        input.now,
        common_valid_until(
            input.operator_arm.armed_until,
            input.kill_switch.valid_until,
            input.broker_ownership.valid_until,
            input.ambiguity.valid_until,
        ),
        input.restart_generation,
        input.config_fingerprint,
        &input.operator_arm.arm_id,
        &input.kill_switch.evidence_sha256,
        &input.broker_ownership.evidence_sha256,
        &input.ambiguity.evidence_sha256,
    ))
}

pub fn authorize_stage8_cancel(
    input: Stage8CancelPreflightInput<'_>,
) -> Result<Stage8CancelPreflightDecision, Stage8ExecutionPreflightError> {
    if input.existing_order.request_id == input.cancel.request_id
        || input.existing_order.account_id != input.cancel.account_id
        || input.existing_order.instrument != input.operator_arm.instrument
        || input
            .cancel
            .client_order_id
            .as_ref()
            .is_some_and(|value| value != &input.existing_order.client_order_id)
    {
        return Err(Stage8ExecutionPreflightError::CancelMappingRequired);
    }
    validate_common(
        Stage8CommandScope::Cancel,
        input.cancel.request_id,
        &input.cancel.account_id,
        &input.existing_order.instrument,
        input.strategy_id,
        input.now,
        input.restart_generation,
        input.config_fingerprint,
        input.max_arm_ttl_ms,
        input.max_evidence_age_ms,
        input.allowlist,
        &input.operator_arm,
        &input.kill_switch,
        &input.broker_ownership,
        &input.ambiguity,
    )?;
    validate_frozen_broker_policy(
        input.broker_preflight_policy,
        input.allowlist,
        &input.operator_arm,
        input.config_fingerprint,
    )?;
    if input
        .broker_preflight_policy
        .allow_cancel_by_broker_order_id_without_mapping
    {
        return Err(Stage8ExecutionPreflightError::BrokerPolicyTooWide);
    }
    match input.broker_preflight_policy.approve_cancel_order(
        input.cancel,
        input.now,
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
                input.now,
                common_valid_until(
                    input.operator_arm.armed_until,
                    input.kill_switch.valid_until,
                    input.broker_ownership.valid_until,
                    input.ambiguity.valid_until,
                ),
                input.restart_generation,
                input.config_fingerprint,
                &input.operator_arm.arm_id,
                &input.kill_switch.evidence_sha256,
                &input.broker_ownership.evidence_sha256,
                &input.ambiguity.evidence_sha256,
            )),
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_common(
    scope: Stage8CommandScope,
    request_id: StrategyRequestId,
    account_id: &AccountId,
    instrument: &InstrumentId,
    strategy_id: &str,
    now: DateTime<Utc>,
    restart_generation: u64,
    config_fingerprint: &str,
    max_arm_ttl_ms: u64,
    max_evidence_age_ms: u64,
    allowlist: &Stage8ExecutionAllowlist,
    arm: &Stage8OperatorArmInput,
    kill_switch: &Stage8PersistentKillSwitchEvidenceInput,
    ownership: &Stage8SingleBrokerOwnershipEvidenceInput,
    ambiguity: &Stage8ZeroAmbiguityEvidenceInput,
) -> Result<(), Stage8ExecutionPreflightError> {
    validate_allowlist(allowlist)?;
    validate_strategy_id(strategy_id)?;
    if !allowlist.accounts.contains(account_id) {
        return Err(Stage8ExecutionPreflightError::AccountNotAllowed);
    }
    if !allowlist.instruments.contains(instrument) {
        return Err(Stage8ExecutionPreflightError::InstrumentNotAllowed);
    }
    if !allowlist
        .strategy_ids
        .iter()
        .any(|value| value == strategy_id)
    {
        return Err(Stage8ExecutionPreflightError::StrategyNotAllowed);
    }
    if restart_generation == 0
        || arm.restart_generation != restart_generation
        || kill_switch.restart_generation != restart_generation
        || ownership.restart_generation != restart_generation
        || ambiguity.restart_generation != restart_generation
    {
        return Err(Stage8ExecutionPreflightError::RestartGenerationMismatch);
    }
    if !valid_sha256(config_fingerprint)
        || arm.config_fingerprint != config_fingerprint
        || kill_switch.config_fingerprint != config_fingerprint
        || ownership.config_fingerprint != config_fingerprint
        || ambiguity.config_fingerprint != config_fingerprint
    {
        return Err(Stage8ExecutionPreflightError::ConfigFingerprintMismatch);
    }
    validate_arm(
        scope,
        request_id,
        account_id,
        instrument,
        strategy_id,
        now,
        max_arm_ttl_ms,
        arm,
    )?;
    validate_kill_switch(now, max_evidence_age_ms, kill_switch)?;
    validate_ownership(account_id, instrument, now, max_evidence_age_ms, ownership)?;
    validate_ambiguity(now, max_evidence_age_ms, ambiguity)?;
    Ok(())
}

fn validate_allowlist(
    allowlist: &Stage8ExecutionAllowlist,
) -> Result<(), Stage8ExecutionPreflightError> {
    let account_count = allowlist.accounts.iter().collect::<HashSet<_>>().len();
    let instrument_count = allowlist.instruments.iter().collect::<HashSet<_>>().len();
    let strategy_count = allowlist.strategy_ids.iter().collect::<HashSet<_>>().len();
    if allowlist.accounts.is_empty()
        || allowlist.instruments.is_empty()
        || allowlist.strategy_ids.is_empty()
        || account_count != allowlist.accounts.len()
        || instrument_count != allowlist.instruments.len()
        || strategy_count != allowlist.strategy_ids.len()
        || allowlist
            .strategy_ids
            .iter()
            .any(|value| validate_strategy_id(value).is_err())
    {
        return Err(Stage8ExecutionPreflightError::InvalidAllowlist);
    }
    Ok(())
}

fn validate_strategy_id(value: &str) -> Result<(), Stage8ExecutionPreflightError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(Stage8ExecutionPreflightError::InvalidStrategyId);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_arm(
    scope: Stage8CommandScope,
    request_id: StrategyRequestId,
    account_id: &AccountId,
    instrument: &InstrumentId,
    strategy_id: &str,
    now: DateTime<Utc>,
    max_arm_ttl_ms: u64,
    arm: &Stage8OperatorArmInput,
) -> Result<(), Stage8ExecutionPreflightError> {
    if !valid_token(&arm.arm_id, 96)
        || !valid_token(&arm.session_id, 96)
        || !arm.one_shot
        || max_arm_ttl_ms == 0
        || arm.armed_at > now
        || arm.armed_until <= now
    {
        return Err(Stage8ExecutionPreflightError::InvalidOperatorArm);
    }
    let ttl_ms = arm
        .armed_until
        .signed_duration_since(arm.armed_at)
        .num_milliseconds();
    if ttl_ms <= 0 || ttl_ms as u64 > max_arm_ttl_ms {
        return Err(Stage8ExecutionPreflightError::OperatorArmExpired);
    }
    if arm.command_scope != scope
        || arm.request_id != request_id
        || &arm.account_id != account_id
        || &arm.instrument != instrument
        || arm.strategy_id != strategy_id
    {
        return Err(Stage8ExecutionPreflightError::OperatorArmBindingMismatch);
    }
    Ok(())
}

fn validate_kill_switch(
    now: DateTime<Utc>,
    max_age_ms: u64,
    evidence: &Stage8PersistentKillSwitchEvidenceInput,
) -> Result<(), Stage8ExecutionPreflightError> {
    if evidence.state != Stage8KillSwitchState::RunAllowed {
        return Err(Stage8ExecutionPreflightError::KillSwitchNotRunAllowed);
    }
    if evidence.durable_revision == 0
        || !fresh_evidence(now, evidence.persisted_at, evidence.valid_until, max_age_ms)
        || !valid_sha256(&evidence.evidence_sha256)
    {
        return Err(Stage8ExecutionPreflightError::KillSwitchEvidenceInvalid);
    }
    Ok(())
}

fn validate_ownership(
    account_id: &AccountId,
    instrument: &InstrumentId,
    now: DateTime<Utc>,
    max_age_ms: u64,
    evidence: &Stage8SingleBrokerOwnershipEvidenceInput,
) -> Result<(), Stage8ExecutionPreflightError> {
    if evidence.broker != BrokerKind::Finam
        || evidence.active_broker_owner_count != 1
        || &evidence.account_id != account_id
        || &evidence.instrument != instrument
        || !fresh_evidence(now, evidence.observed_at, evidence.valid_until, max_age_ms)
        || !valid_sha256(&evidence.evidence_sha256)
    {
        return Err(Stage8ExecutionPreflightError::BrokerOwnershipInvalid);
    }
    Ok(())
}

fn validate_ambiguity(
    now: DateTime<Utc>,
    max_age_ms: u64,
    evidence: &Stage8ZeroAmbiguityEvidenceInput,
) -> Result<(), Stage8ExecutionPreflightError> {
    if evidence.unresolved_order_count != 0
        || evidence.unresolved_delivery_count != 0
        || evidence.reconciliation_required_count != 0
    {
        return Err(Stage8ExecutionPreflightError::UnresolvedAmbiguity);
    }
    if !fresh_evidence(now, evidence.observed_at, evidence.valid_until, max_age_ms)
        || !valid_sha256(&evidence.evidence_sha256)
    {
        return Err(Stage8ExecutionPreflightError::AmbiguityEvidenceInvalid);
    }
    Ok(())
}

fn validate_frozen_broker_policy(
    policy: &OrderPreflightPolicy,
    allowlist: &Stage8ExecutionAllowlist,
    arm: &Stage8OperatorArmInput,
    config_fingerprint: &str,
) -> Result<(), Stage8ExecutionPreflightError> {
    let policy_accounts = policy.allowed_accounts.iter().collect::<HashSet<_>>();
    let stage8_accounts = allowlist.accounts.iter().collect::<HashSet<_>>();
    let policy_symbols = policy
        .allowed_venue_symbols
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let stage8_symbols = allowlist
        .instruments
        .iter()
        .filter_map(|instrument| instrument.venue_symbol.as_deref())
        .collect::<HashSet<_>>();
    let order_types = policy.allowed_order_types.iter().collect::<HashSet<_>>();
    if policy.allowed_time_in_force.as_slice() != [TimeInForce::Day]
        || policy_accounts != stage8_accounts
        || policy_symbols != stage8_symbols
        || order_types.is_empty()
        || order_types.len() != policy.allowed_order_types.len()
        || !order_types.iter().all(|value| {
            matches!(
                value,
                broker_core::OrderType::Market | broker_core::OrderType::Limit
            )
        })
        || !policy.operator_arm.one_shot
        || policy.operator_arm.endpoint_attempted
        || !policy.operator_arm.endpoint_calls_enabled
        || policy.operator_arm.session_id != arm.session_id
        || policy.operator_arm.armed_until != arm.armed_until
        || policy.operator_arm.preflight_digest != config_fingerprint
    {
        return Err(Stage8ExecutionPreflightError::BrokerPolicyTooWide);
    }
    Ok(())
}

fn fresh_evidence(
    now: DateTime<Utc>,
    observed_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    max_age_ms: u64,
) -> bool {
    if max_age_ms == 0 || observed_at > now || valid_until <= now {
        return false;
    }
    let age_ms = now.signed_duration_since(observed_at).num_milliseconds();
    age_ms >= 0 && age_ms as u64 <= max_age_ms
}

fn valid_sha256(value: &str) -> bool {
    value.len() == SHA256_HEX_LEN
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_token(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn common_valid_until(
    arm: DateTime<Utc>,
    kill_switch: DateTime<Utc>,
    ownership: DateTime<Utc>,
    ambiguity: DateTime<Utc>,
) -> DateTime<Utc> {
    [arm, kill_switch, ownership, ambiguity]
        .into_iter()
        .min()
        .expect("four timestamps are always present")
}

#[allow(clippy::too_many_arguments)]
fn build_capability(
    approved: Stage8ApprovedCommand,
    scope: Stage8CommandScope,
    request_id: StrategyRequestId,
    issued_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    restart_generation: u64,
    config_fingerprint: &str,
    arm_id: &str,
    kill_switch_sha256: &str,
    ownership_sha256: &str,
    ambiguity_sha256: &str,
) -> Stage8ExecutionCapability {
    let mut hasher = Sha256::new();
    hasher.update(b"stage8a1-execution-capability-v1\0");
    hasher.update(match scope {
        Stage8CommandScope::Place => b"place".as_slice(),
        Stage8CommandScope::Cancel => b"cancel".as_slice(),
    });
    hasher.update(b"\0");
    hasher.update(request_id.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(issued_at.to_rfc3339().as_bytes());
    hasher.update(b"\0");
    hasher.update(valid_until.to_rfc3339().as_bytes());
    hasher.update(b"\0");
    hasher.update(restart_generation.to_be_bytes());
    hasher.update(b"\0");
    hasher.update(config_fingerprint.as_bytes());
    hasher.update(b"\0");
    hasher.update(arm_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(kill_switch_sha256.as_bytes());
    hasher.update(b"\0");
    hasher.update(ownership_sha256.as_bytes());
    hasher.update(b"\0");
    hasher.update(ambiguity_sha256.as_bytes());
    let audit_fingerprint = format!("{:x}", hasher.finalize());
    Stage8ExecutionCapability {
        approved,
        scope,
        request_id,
        issued_at,
        valid_until,
        restart_generation,
        config_fingerprint: config_fingerprint.to_string(),
        audit_fingerprint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{
        ClientOrderId, Exchange, Market, OperatorArm, OrderPathCommandKind, OrderPathState,
        OrderSide, OrderType,
    };
    use chrono::{Duration, TimeZone};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    const FP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const EVIDENCE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 15, 9, 0, 0).single().unwrap()
    }

    fn account() -> AccountId {
        AccountId::new("ACC_TEST_0001")
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn request_id(value: u128) -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128(value))
    }

    fn place() -> PlaceOrder {
        PlaceOrder {
            request_id: request_id(1),
            created_ts: now() - Duration::seconds(1),
            ttl_ms: Some(10_000),
            account_id: account(),
            client_order_id: ClientOrderId::new("S8A1TEST000000000001").unwrap(),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::ONE,
            limit_price: Some(Decimal::new(2200, 0)),
            time_in_force: TimeInForce::Day,
            comment: None,
        }
    }

    fn policy() -> OrderPreflightPolicy {
        OrderPreflightPolicy {
            allowed_accounts: vec![account()],
            allowed_venue_symbols: vec!["IMOEXF@RTSX".to_string()],
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
                session_id: "SESSION_TEST".to_string(),
                armed_until: now() + Duration::seconds(30),
                endpoint_calls_enabled: true,
                one_shot: true,
                endpoint_attempted: false,
                preflight_digest: FP.to_string(),
            },
        }
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

    fn allowlist() -> Stage8ExecutionAllowlist {
        Stage8ExecutionAllowlist {
            accounts: vec![account()],
            instruments: vec![instrument()],
            strategy_ids: vec!["hybrid_imoexf".to_string()],
        }
    }

    fn arm(scope: Stage8CommandScope, request_id: StrategyRequestId) -> Stage8OperatorArmInput {
        Stage8OperatorArmInput {
            arm_id: "ARM_TEST_1".to_string(),
            session_id: "SESSION_TEST".to_string(),
            command_scope: scope,
            request_id,
            account_id: account(),
            instrument: instrument(),
            strategy_id: "hybrid_imoexf".to_string(),
            armed_at: now() - Duration::seconds(1),
            armed_until: now() + Duration::seconds(30),
            one_shot: true,
            restart_generation: 7,
            config_fingerprint: FP.to_string(),
        }
    }

    fn kill_switch() -> Stage8PersistentKillSwitchEvidenceInput {
        Stage8PersistentKillSwitchEvidenceInput {
            state: Stage8KillSwitchState::RunAllowed,
            durable_revision: 9,
            persisted_at: now() - Duration::seconds(1),
            valid_until: now() + Duration::seconds(20),
            restart_generation: 7,
            config_fingerprint: FP.to_string(),
            evidence_sha256: EVIDENCE.to_string(),
        }
    }

    fn ownership() -> Stage8SingleBrokerOwnershipEvidenceInput {
        Stage8SingleBrokerOwnershipEvidenceInput {
            broker: BrokerKind::Finam,
            active_broker_owner_count: 1,
            account_id: account(),
            instrument: instrument(),
            observed_at: now() - Duration::seconds(1),
            valid_until: now() + Duration::seconds(25),
            restart_generation: 7,
            config_fingerprint: FP.to_string(),
            evidence_sha256: EVIDENCE.to_string(),
        }
    }

    fn ambiguity() -> Stage8ZeroAmbiguityEvidenceInput {
        Stage8ZeroAmbiguityEvidenceInput {
            unresolved_order_count: 0,
            unresolved_delivery_count: 0,
            reconciliation_required_count: 0,
            observed_at: now() - Duration::seconds(1),
            valid_until: now() + Duration::seconds(22),
            restart_generation: 7,
            config_fingerprint: FP.to_string(),
            evidence_sha256: EVIDENCE.to_string(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn authorize_place(
        order: &PlaceOrder,
        policy: &OrderPreflightPolicy,
        context: &OrderPreflightContext,
        allowlist: &Stage8ExecutionAllowlist,
        arm: Stage8OperatorArmInput,
        kill_switch: Stage8PersistentKillSwitchEvidenceInput,
        ownership: Stage8SingleBrokerOwnershipEvidenceInput,
        ambiguity: Stage8ZeroAmbiguityEvidenceInput,
    ) -> Result<Stage8ExecutionCapability, Stage8ExecutionPreflightError> {
        authorize_stage8_place(Stage8PlacePreflightInput {
            order,
            strategy_id: "hybrid_imoexf",
            now: now(),
            restart_generation: 7,
            config_fingerprint: FP,
            max_arm_ttl_ms: 60_000,
            max_evidence_age_ms: 5_000,
            allowlist,
            broker_preflight_policy: policy,
            broker_preflight_context: context,
            operator_arm: arm,
            kill_switch,
            broker_ownership: ownership,
            ambiguity,
        })
    }

    fn place_error(
        result: Result<Stage8ExecutionCapability, Stage8ExecutionPreflightError>,
    ) -> Stage8ExecutionPreflightError {
        match result {
            Err(error) => error,
            Ok(_) => panic!("expected Stage 8A-1 place preflight rejection"),
        }
    }

    fn cancel_error(
        result: Result<Stage8CancelPreflightDecision, Stage8ExecutionPreflightError>,
    ) -> Stage8ExecutionPreflightError {
        match result {
            Err(error) => error,
            Ok(_) => panic!("expected Stage 8A-1 cancel preflight rejection"),
        }
    }

    #[test]
    fn exact_place_preflight_issues_opaque_capability() {
        let order = place();
        let capability = authorize_place(
            &order,
            &policy(),
            &context(),
            &allowlist(),
            arm(Stage8CommandScope::Place, order.request_id),
            kill_switch(),
            ownership(),
            ambiguity(),
        )
        .unwrap();
        let diagnostic = capability.diagnostic();
        assert_eq!(diagnostic.scope, Stage8CommandScope::Place);
        assert_eq!(diagnostic.valid_until, now() + Duration::seconds(20));
        assert_eq!(diagnostic.audit_fingerprint.len(), 64);
    }

    #[test]
    fn stop_requested_blocks_capability() {
        let order = place();
        let mut kill = kill_switch();
        kill.state = Stage8KillSwitchState::StopRequested;
        assert_eq!(
            place_error(authorize_place(
                &order,
                &policy(),
                &context(),
                &allowlist(),
                arm(Stage8CommandScope::Place, order.request_id),
                kill,
                ownership(),
                ambiguity(),
            )),
            Stage8ExecutionPreflightError::KillSwitchNotRunAllowed
        );
    }

    #[test]
    fn restart_and_config_drift_block_capability() {
        let order = place();
        let mut stale_arm = arm(Stage8CommandScope::Place, order.request_id);
        stale_arm.restart_generation = 6;
        assert_eq!(
            place_error(authorize_place(
                &order,
                &policy(),
                &context(),
                &allowlist(),
                stale_arm,
                kill_switch(),
                ownership(),
                ambiguity(),
            )),
            Stage8ExecutionPreflightError::RestartGenerationMismatch
        );
        let mut drifted = ownership();
        drifted.config_fingerprint = "c".repeat(64);
        assert_eq!(
            place_error(authorize_place(
                &order,
                &policy(),
                &context(),
                &allowlist(),
                arm(Stage8CommandScope::Place, order.request_id),
                kill_switch(),
                drifted,
                ambiguity(),
            )),
            Stage8ExecutionPreflightError::ConfigFingerprintMismatch
        );
    }

    #[test]
    fn ambiguity_and_multiple_broker_owners_block_capability() {
        let order = place();
        let mut ambiguous = ambiguity();
        ambiguous.unresolved_delivery_count = 1;
        assert_eq!(
            place_error(authorize_place(
                &order,
                &policy(),
                &context(),
                &allowlist(),
                arm(Stage8CommandScope::Place, order.request_id),
                kill_switch(),
                ownership(),
                ambiguous,
            )),
            Stage8ExecutionPreflightError::UnresolvedAmbiguity
        );
        let mut multiple = ownership();
        multiple.active_broker_owner_count = 2;
        assert_eq!(
            place_error(authorize_place(
                &order,
                &policy(),
                &context(),
                &allowlist(),
                arm(Stage8CommandScope::Place, order.request_id),
                kill_switch(),
                multiple,
                ambiguity(),
            )),
            Stage8ExecutionPreflightError::BrokerOwnershipInvalid
        );
    }

    #[test]
    fn non_day_and_non_one_shot_arm_are_blocked() {
        let mut order = place();
        order.time_in_force = TimeInForce::GoodTillCancel;
        assert_eq!(
            place_error(authorize_place(
                &order,
                &policy(),
                &context(),
                &allowlist(),
                arm(Stage8CommandScope::Place, order.request_id),
                kill_switch(),
                ownership(),
                ambiguity(),
            )),
            Stage8ExecutionPreflightError::TimeInForceNotDay
        );
        order.time_in_force = TimeInForce::Day;
        let mut reusable = arm(Stage8CommandScope::Place, order.request_id);
        reusable.one_shot = false;
        assert_eq!(
            place_error(authorize_place(
                &order,
                &policy(),
                &context(),
                &allowlist(),
                reusable,
                kill_switch(),
                ownership(),
                ambiguity(),
            )),
            Stage8ExecutionPreflightError::InvalidOperatorArm
        );
    }

    #[test]
    fn stale_evidence_and_broker_policy_drift_are_blocked() {
        let order = place();
        let mut stale = ambiguity();
        stale.observed_at = now() - Duration::seconds(10);
        assert_eq!(
            place_error(authorize_place(
                &order,
                &policy(),
                &context(),
                &allowlist(),
                arm(Stage8CommandScope::Place, order.request_id),
                kill_switch(),
                ownership(),
                stale,
            )),
            Stage8ExecutionPreflightError::AmbiguityEvidenceInvalid
        );

        let mut wider = policy();
        wider
            .allowed_time_in_force
            .push(TimeInForce::GoodTillCancel);
        assert_eq!(
            place_error(authorize_place(
                &order,
                &wider,
                &context(),
                &allowlist(),
                arm(Stage8CommandScope::Place, order.request_id),
                kill_switch(),
                ownership(),
                ambiguity(),
            )),
            Stage8ExecutionPreflightError::BrokerPolicyTooWide
        );
    }

    #[test]
    fn account_instrument_and_strategy_allowlists_are_exact() {
        let order = place();
        let mut missing_account = allowlist();
        missing_account.accounts = vec![AccountId::new("ACC_TEST_OTHER")];
        assert_eq!(
            place_error(authorize_place(
                &order,
                &policy(),
                &context(),
                &missing_account,
                arm(Stage8CommandScope::Place, order.request_id),
                kill_switch(),
                ownership(),
                ambiguity(),
            )),
            Stage8ExecutionPreflightError::AccountNotAllowed
        );

        let mut duplicate_strategy = allowlist();
        duplicate_strategy
            .strategy_ids
            .push("hybrid_imoexf".to_string());
        assert_eq!(
            place_error(authorize_place(
                &order,
                &policy(),
                &context(),
                &duplicate_strategy,
                arm(Stage8CommandScope::Place, order.request_id),
                kill_switch(),
                ownership(),
                ambiguity(),
            )),
            Stage8ExecutionPreflightError::InvalidAllowlist
        );
    }

    fn existing_record(place: &PlaceOrder) -> OrderPathRecord {
        let mut record = OrderPathRecord::from_place_order(place, now(), None);
        record.broker_order_id = Some(broker_core::BrokerOrderId::new("BROKER_TEST_1"));
        record.state = OrderPathState::Submitted;
        record.command_kind = OrderPathCommandKind::Place;
        record
    }

    #[test]
    fn exact_cancel_mapping_issues_capability_and_terminal_does_not() {
        let placed = place();
        let mut existing = existing_record(&placed);
        let cancel = CancelOrder {
            request_id: request_id(2),
            created_ts: now() - Duration::seconds(1),
            ttl_ms: Some(10_000),
            account_id: account(),
            order_id: existing.broker_order_id.clone().unwrap(),
            client_order_id: Some(placed.client_order_id.clone()),
        };
        let decision = authorize_stage8_cancel(Stage8CancelPreflightInput {
            cancel: &cancel,
            existing_order: &existing,
            strategy_id: "hybrid_imoexf",
            now: now(),
            restart_generation: 7,
            config_fingerprint: FP,
            max_arm_ttl_ms: 60_000,
            max_evidence_age_ms: 5_000,
            allowlist: &allowlist(),
            broker_preflight_policy: &policy(),
            operator_arm: arm(Stage8CommandScope::Cancel, cancel.request_id),
            kill_switch: kill_switch(),
            broker_ownership: ownership(),
            ambiguity: ambiguity(),
        })
        .unwrap();
        assert!(matches!(
            decision,
            Stage8CancelPreflightDecision::Capability(_)
        ));

        existing.state = OrderPathState::Terminal;
        let decision = authorize_stage8_cancel(Stage8CancelPreflightInput {
            cancel: &cancel,
            existing_order: &existing,
            strategy_id: "hybrid_imoexf",
            now: now(),
            restart_generation: 7,
            config_fingerprint: FP,
            max_arm_ttl_ms: 60_000,
            max_evidence_age_ms: 5_000,
            allowlist: &allowlist(),
            broker_preflight_policy: &policy(),
            operator_arm: arm(Stage8CommandScope::Cancel, cancel.request_id),
            kill_switch: kill_switch(),
            broker_ownership: ownership(),
            ambiguity: ambiguity(),
        })
        .unwrap();
        assert!(matches!(
            decision,
            Stage8CancelPreflightDecision::AlreadyTerminal
        ));
    }

    #[test]
    fn cancel_client_identity_mismatch_is_blocked() {
        let placed = place();
        let existing = existing_record(&placed);
        let cancel = CancelOrder {
            request_id: request_id(3),
            created_ts: now() - Duration::seconds(1),
            ttl_ms: Some(10_000),
            account_id: account(),
            order_id: existing.broker_order_id.clone().unwrap(),
            client_order_id: Some(ClientOrderId::new("S8A1TEST000000000002").unwrap()),
        };
        assert_eq!(
            cancel_error(authorize_stage8_cancel(Stage8CancelPreflightInput {
                cancel: &cancel,
                existing_order: &existing,
                strategy_id: "hybrid_imoexf",
                now: now(),
                restart_generation: 7,
                config_fingerprint: FP,
                max_arm_ttl_ms: 60_000,
                max_evidence_age_ms: 5_000,
                allowlist: &allowlist(),
                broker_preflight_policy: &policy(),
                operator_arm: arm(Stage8CommandScope::Cancel, cancel.request_id),
                kill_switch: kill_switch(),
                broker_ownership: ownership(),
                ambiguity: ambiguity(),
            })),
            Stage8ExecutionPreflightError::CancelMappingRequired
        );
    }
}
